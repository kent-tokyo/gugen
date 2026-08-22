#!/usr/bin/env python3
"""Phase 21B condition 1 (owner-redirected to OQMD, avoiding a Materials
Project API-key block): queries OQMD's unauthenticated REST API for
every distinct target/precursor formula needed by
benchmarks/data/thermodynamic_selectivity_clean_population.json, and
records whether each resolves to a real OQMD formation-energy entry.

This script does NOT run gugen's own thermodynamic functions and does
NOT compute a calibration -- it only measures coverage (condition 1's
own scope). See docs/thermodynamic_selectivity_calibration.md §6 for
the pre-registered coverage gate and polymorph policy this script
implements, fixed *before* any data was fetched.

Source: OQMD (Open Quantum Materials Database), https://oqmd.org/
API: GET /oqmdapi/formationenergy, no authentication required (verified
against the qmpy_rester reference client and OQMD's own restful.html
docs -- see docs/thermodynamic_selectivity_calibration.md §6.1).
Data license: CC BY 4.0 (verified via a Wayback Machine capture while
oqmd.org's live site was down -- see §6.1 for the full citation
requirement: Saal et al. 2013, Kirklin et al. 2015).

Fails loudly and writes NOTHING on any error: a non-200 response, a
response missing an expected field, or a network failure all abort the
whole run with a non-zero exit and no partial manifest/snapshot file --
a half-populated coverage snapshot would silently become a fake
denominator for the coverage report, which this project's own
discipline (docs/thermodynamic_selectivity_dataset_feasibility.md §7,
"not measured in this phase") exists to prevent. The one exception is
the gitignored per-formula resume cache (--cache-path, see _load_cache
below) and its sidecar fingerprint file (_cache_meta_path): together
they persist raw fetched rows and the query-shape/population identity
they were fetched under, across an aborted run, so a later run can
resume without re-querying and without silently reusing stale data --
but neither is ever a substitute for the two deliverable files above,
and both are deleted once those files are written.

Polymorph policy (fixed in advance, mirrors
MaterialsProjectSnapshotProvider::energy_for's existing "most stable
known phase" convention exactly -- src/materials_project_adapter.rs):
among an OQMD composition's returned entries, excluding any row with a
non-null duplicate_entry_id, take the one with the lowest delta_e. This
is a modeling convention, not an experimental phase identification.

Run:
  python3 benchmarks/fetch_oqmd_coverage.py
Output: benchmarks/data/oqmd_coverage_snapshot.json (per-formula raw
        OQMD data, gitignored if large -- see ATTRIBUTION.md),
        benchmarks/data/oqmd_coverage_manifest.json (small, committed:
        real snapshot identity -- API meta, retrieval datetime(s), query,
        checksum -- plus the coverage numbers).
"""

import argparse
import datetime
import hashlib
import json
import os
import ssl
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

try:
    import certifi

    _SSL_CONTEXT = ssl.create_default_context(cafile=certifi.where())
except ImportError:
    _SSL_CONTEXT = None

OQMD_BASE = "https://oqmd.org/oqmdapi/formationenergy"
DATA_DIR = Path(__file__).parent / "data"
CLEAN_POPULATION_PATH = DATA_DIR / "thermodynamic_selectivity_clean_population.json"
# Only what condition 1's coverage decision + the polymorph policy need --
# requested explicitly via the API's own `fields` param so the raw
# snapshot never carries per-entry `sites`/`unit_cell` arrays (OQMD's
# default response shape), keeping payload size and any future commit
# decision manageable.
QUERY_FIELDS = ["name", "entry_id", "natoms", "volume", "delta_e", "spacegroup", "stability", "duplicate_entry_id"]
REQUIRED_ENTRY_FIELDS = {"name", "entry_id", "natoms", "volume", "delta_e"}
REQUIRED_RESPONSE_KEYS = {"data", "meta"}
# Bump whenever the resume cache's on-disk row shape changes incompatibly.
CACHE_SCHEMA_VERSION = 2
# Bump whenever select_polymorph's selection rule changes -- part of the
# cache fingerprint so a cache built under an old policy is never silently
# reused as if it reflected the current one.
POLYMORPH_POLICY_VERSION = "v2-lowest-delta_e-among-preferred-entries"


class OqmdFetchError(RuntimeError):
    pass


def _now_utc_iso():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def _urlopen(url, timeout):
    req = urllib.request.Request(url, headers={"User-Agent": "gugen-phase21b-research/1.0"})
    return urllib.request.urlopen(req, context=_SSL_CONTEXT, timeout=timeout)


def query_composition(formula, timeout=20, retries=4, backoff_seconds=(2, 5, 15, 30)):
    """Retries only on a transient condition (HTTP 429, HTTP 5xx, or a
    network/timeout error) -- observed live on 2026-08-16: two separate
    real runs each aborted around the same ~50th request, once with
    HTTP 429 and once with a plain read timeout, suggesting oqmd.org is
    still flaky in the days right after its extended outage rather than
    enforcing one simple fixed rate limit. A non-transient problem
    (malformed JSON, a response missing expected keys) is a real data
    issue retrying cannot fix, so it still raises immediately -- this
    keeps the script's core guarantee (abort and write nothing on any
    unresolved error) unchanged, only adding resilience to the specific
    failure modes actually observed to be transient.

    Worst case (all `retries` attempts time out): `retries * timeout` +
    the sum of `backoff_seconds` between attempts -- with the defaults
    below, 4*20 + (2+5+15) = 102s, not the ~52s an earlier draft of this
    change claimed (that number omitted the per-attempt timeout itself).
    """
    params = urllib.parse.urlencode({"composition": formula, "limit": 50, "fields": ",".join(QUERY_FIELDS)})
    url = f"{OQMD_BASE}?{params}"
    last_error = None
    for attempt in range(retries):
        try:
            with _urlopen(url, timeout) as resp:
                status = resp.status
                body = resp.read()
        except urllib.error.HTTPError as e:
            if e.code == 429 or 500 <= e.code < 600:
                last_error = f"HTTP {e.code} for composition={formula!r}: {url}"
                if attempt < retries - 1:
                    time.sleep(backoff_seconds[min(attempt, len(backoff_seconds) - 1)])
                    continue
                raise OqmdFetchError(f"{last_error} (gave up after {retries} attempts)") from e
            raise OqmdFetchError(f"HTTP {e.code} for composition={formula!r}: {url}") from e
        except (urllib.error.URLError, TimeoutError, OSError) as e:
            last_error = f"network error for composition={formula!r}: {e}"
            if attempt < retries - 1:
                time.sleep(backoff_seconds[min(attempt, len(backoff_seconds) - 1)])
                continue
            raise OqmdFetchError(f"{last_error} (gave up after {retries} attempts)") from e
        else:
            break

    if status != 200:
        raise OqmdFetchError(f"HTTP {status} (non-error-raising) for composition={formula!r}: {url}")

    try:
        payload = json.loads(body)
    except json.JSONDecodeError as e:
        raise OqmdFetchError(f"non-JSON response for composition={formula!r}: {e}") from e

    missing = REQUIRED_RESPONSE_KEYS - payload.keys()
    if missing:
        raise OqmdFetchError(f"response missing top-level keys {missing} for composition={formula!r}")

    return payload


def validate_first_response_schema(payload, formula):
    """Checks the response shape against REQUIRED_ENTRY_FIELDS. Returns
    True if it actually validated a non-empty entry, False if this
    response's `data` was empty (a legitimate "zero matches" outcome that
    proves nothing about the schema). The caller must only treat schema
    validation as done once this returns True -- see maybe_validate_schema."""
    data = payload["data"]
    if not data:
        return False
    missing = REQUIRED_ENTRY_FIELDS - data[0].keys()
    if missing:
        raise OqmdFetchError(
            f"first non-empty OQMD response (composition={formula!r}) is missing required "
            f"fields {missing} -- aborting rather than guessing a mapping for an unfamiliar schema"
        )
    return True


def maybe_validate_schema(already_validated, payload, formula):
    """Runs validate_first_response_schema at most until it actually
    validates a non-empty response. Fixes a bug where an early
    zero-match response (data=[]) would set the caller's "validated"
    flag permanently, silently skipping the check for every later
    response including the first non-empty one."""
    if already_validated:
        return True
    return validate_first_response_schema(payload, formula)


def select_polymorph(entries):
    """Fixed policy (docs/thermodynamic_selectivity_calibration.md §6.3,
    POLYMORPH_POLICY_VERSION above). OQMD's own restful.html docs define
    `duplicate_entry_id` as "the OQMD ID of the *preferred* entry with
    this same crystal structure" -- NOT a duplicate/not-duplicate flag.
    An entry is preferred (keep it as a polymorph candidate) when
    `duplicate_entry_id` is null (no dedup group recorded for it) or
    equals its own `entry_id` (self-referencing -- confirmed the common
    case, ~75% of entries in a real 714-formula sample); it is a
    non-preferred duplicate, safe to exclude, only when
    `duplicate_entry_id` is present and points to a *different*
    entry_id. An earlier version of this function treated any non-null
    `duplicate_entry_id` as "exclude," which silently discarded
    self-referencing preferred entries -- 475 of 714 formulas' matches in
    that same real sample, caught by comparing real fetched data against
    OQMD's actual field documentation rather than the field name alone.
    Also excludes rows with no usable delta_e (a real OQMD entry can
    exist with a null delta_e, e.g. an unconverged calculation -- a
    distinct, reportable abstention category from "no entry at all," not
    a crash). Takes lowest delta_e among what remains. Returns
    (chosen_or_None, n_excluded_as_non_preferred, n_null_energy_excluded)."""

    def is_preferred(e):
        entry_id = e.get("entry_id")
        if entry_id is None:
            return False  # can't identify this entry at all -- never a valid candidate,
            # regardless of duplicate_entry_id (guards against a coincidental
            # None == None match if entry_id were ever missing)
        dup = e.get("duplicate_entry_id")
        return dup is None or dup == entry_id

    preferred = [e for e in entries if is_preferred(e)]
    n_excluded_as_non_preferred = len(entries) - len(preferred)
    n_null_energy = sum(1 for e in preferred if e.get("delta_e") is None)
    candidates = [e for e in preferred if e.get("delta_e") is not None]
    chosen = min(candidates, key=lambda e: e["delta_e"]) if candidates else None
    return chosen, n_excluded_as_non_preferred, n_null_energy


def _load_cache(cache_path):
    """Resume support: a formula -> {"data": ..., "meta": ..., "fetched_at_utc":
    ...} map loaded from a previous, possibly-incomplete run's incremental
    cache (one JSON object per line). Rows written before CACHE_SCHEMA_VERSION
    2 have no "fetched_at_utc" key -- callers must use .get(...) and treat
    that as an honestly-unknown timestamp, never guess one.

    Tolerates exactly one failure shape without raising: the file's final
    line is present but incomplete (no trailing newline) and fails to
    parse -- the signature of a hard kill mid-write. That line is dropped,
    a warning is printed, and the file is truncated to the last known-good
    line so a later append doesn't leave a corrupt line stranded in the
    middle of the file. Any other parse failure (a non-final line, or a
    final line that *does* end with a newline -- i.e. was fully written)
    is real, unexplained corruption and raises OqmdFetchError rather than
    silently discarding data."""
    cached = {}
    if not cache_path.exists():
        return cached
    raw = cache_path.read_bytes()
    if not raw:
        return cached
    ends_with_newline = raw.endswith(b"\n")
    body = raw[:-1] if ends_with_newline else raw
    lines = body.split(b"\n") if body else []
    n_lines = len(lines)
    consumed = 0
    for idx, line_bytes in enumerate(lines):
        stripped = line_bytes.strip()
        line_span = len(line_bytes) + 1
        if not stripped:
            consumed += line_span
            continue
        is_last_line = idx == n_lines - 1
        try:
            row = json.loads(stripped)
        except json.JSONDecodeError as e:
            if is_last_line and not ends_with_newline:
                print(
                    f"warning: ignoring truncated final line in {cache_path} "
                    f"(no trailing newline -- likely an interrupted write): {e}",
                    file=sys.stderr,
                )
                try:
                    with cache_path.open("r+b") as f:
                        f.truncate(consumed)
                except OSError as trunc_err:
                    print(f"warning: could not truncate {cache_path}: {trunc_err}", file=sys.stderr)
                break
            raise OqmdFetchError(
                f"corrupt cache line {idx + 1} in {cache_path} (not an incomplete final "
                f"line -- this is real corruption, not a partial write): {e}"
            ) from e
        cached[row["formula"]] = row
        consumed += line_span
    return cached


def _cache_meta_path(cache_path):
    return Path(str(cache_path) + ".meta.json")


def _current_fingerprint(population_sha256):
    """Identifies exactly what a resume cache is valid for: the endpoint,
    the requested fields (a shape change would silently miss columns in
    old rows), which population the formula list was drawn from, and the
    polymorph-selection policy version. Any change to any of these makes
    an existing cache's rows unsafe to trust without re-verification."""
    return {
        "cache_schema_version": CACHE_SCHEMA_VERSION,
        "oqmd_endpoint": OQMD_BASE,
        "query_fields": QUERY_FIELDS,
        "clean_population_sha256": population_sha256,
        "polymorph_policy_version": POLYMORPH_POLICY_VERSION,
    }


def resolve_cache_fingerprint(cache_path, current_fingerprint, trust_legacy_cache):
    """Validates (and, if needed, writes) the cache's sidecar fingerprint
    file before any row in `cache_path` is trusted. Raises OqmdFetchError
    if an existing cache's fingerprint doesn't match the current run's, or
    if an existing cache has no fingerprint at all and --trust-legacy-cache
    wasn't passed -- a cache silently reused for a different query shape
    or population would corrupt the coverage result without any visible
    symptom. Always (re)writes the meta file with `current_fingerprint` on
    success, so a legacy cache only needs --trust-legacy-cache once."""
    meta_path = _cache_meta_path(cache_path)
    if meta_path.exists():
        existing = json.loads(meta_path.read_text())
        if existing != current_fingerprint:
            raise OqmdFetchError(
                f"cache fingerprint mismatch: {meta_path} was written for a different "
                f"endpoint/query-shape/population/policy than this run. Refusing to reuse "
                f"{cache_path} -- delete both files to start fresh if that's intended.\n"
                f"existing={json.dumps(existing, sort_keys=True)}\n"
                f"current={json.dumps(current_fingerprint, sort_keys=True)}"
            )
    elif cache_path.exists():
        if not trust_legacy_cache:
            raise OqmdFetchError(
                f"{cache_path} exists but has no fingerprint metadata ({meta_path} missing) -- "
                "cannot verify it matches this run's endpoint/query-shape/population/policy. "
                "Re-run with --trust-legacy-cache if you have manually verified this cache is "
                f"safe to reuse, or delete {cache_path} to start fresh."
            )
        print(
            f"--trust-legacy-cache: adopting {cache_path} without prior fingerprint metadata "
            f"(writing {meta_path} now, not needed again for this cache)",
            file=sys.stderr,
        )
    meta_path.write_text(json.dumps(current_fingerprint, indent=2, sort_keys=True))
    return current_fingerprint


def build_retrieval_metadata(fetch_timestamps, resumed_from_cache_count, unknown_timestamp_count, completed_at_utc):
    """Pure summary of when the data underlying a coverage manifest was
    actually fetched -- honest about a resumed run spanning multiple
    sessions rather than implying every formula was fetched at one
    instant (`completed_at_utc`, the old field's role)."""
    return {
        "first_fetch_at_utc": min(fetch_timestamps) if fetch_timestamps else None,
        "last_fetch_at_utc": max(fetch_timestamps) if fetch_timestamps else None,
        "completed_at_utc": completed_at_utc,
        "resumed_from_cache_count": resumed_from_cache_count,
        "across_multiple_runs": resumed_from_cache_count > 0,
        "unknown_fetch_timestamp_count": unknown_timestamp_count,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--sleep", type=float, default=0.3, help="seconds between requests (politeness)")
    parser.add_argument("--limit-formulas", type=int, default=None, help="dev-only: cap the number of formulas queried")
    parser.add_argument(
        "--cache-path",
        type=Path,
        default=DATA_DIR / ".oqmd_fetch_cache.jsonl",
        help="incremental per-formula cache so a run interrupted partway (e.g. a background "
        "process time limit) can resume instead of restarting -- gitignored, not a deliverable",
    )
    parser.add_argument(
        "--trust-legacy-cache",
        action="store_true",
        help="adopt an existing --cache-path that has no fingerprint metadata (e.g. written "
        "before this feature existed), after manually verifying it came from the same "
        "script/query-shape/population -- only needed once per such cache",
    )
    args = parser.parse_args()

    population_bytes = CLEAN_POPULATION_PATH.read_bytes()
    population = json.loads(population_bytes)
    population_sha256 = hashlib.sha256(population_bytes).hexdigest()
    formulas = set()
    for row in population:
        formulas.add(row["target"])
        formulas.update(row["route"])
    formulas = sorted(formulas)
    if args.limit_formulas:
        formulas = formulas[: args.limit_formulas]
    print(f"{len(formulas)} distinct formulas to query", file=sys.stderr)

    current_fingerprint = _current_fingerprint(population_sha256)
    resolve_cache_fingerprint(args.cache_path, current_fingerprint, args.trust_legacy_cache)

    cached = _load_cache(args.cache_path)
    if cached:
        print(f"resuming: {len(cached)} formula(s) already cached from a prior run", file=sys.stderr)

    raw_snapshot = {}
    coverage = {}
    api_meta_seen = None
    schema_validated = False
    resumed_from_cache_count = 0
    fetch_timestamps = []
    unknown_timestamp_count = 0

    try:
        with args.cache_path.open("a") as cache_file:
            for i, formula in enumerate(formulas):
                if formula in cached:
                    payload = cached[formula]
                    resumed_from_cache_count += 1
                else:
                    fetched = query_composition(formula)  # raises OqmdFetchError -> abort, see except below
                    payload = {
                        "formula": formula,
                        "data": fetched["data"],
                        "meta": fetched["meta"],
                        "fetched_at_utc": _now_utc_iso(),
                    }
                    cache_file.write(json.dumps(payload) + "\n")
                    cache_file.flush()
                    os.fsync(cache_file.fileno())
                    time.sleep(args.sleep)

                fetched_at_utc = payload.get("fetched_at_utc")
                if fetched_at_utc:
                    fetch_timestamps.append(fetched_at_utc)
                else:
                    unknown_timestamp_count += 1

                schema_validated = maybe_validate_schema(schema_validated, payload, formula)
                if api_meta_seen is None:
                    api_meta_seen = payload["meta"]

                entries = payload["data"]
                chosen, n_duplicate, n_null_energy = select_polymorph(entries)
                raw_snapshot[formula] = entries
                coverage[formula] = {
                    "n_candidate_entries": len(entries),
                    "n_duplicate_excluded": n_duplicate,
                    "n_null_energy_excluded": n_null_energy,
                    "matched": chosen is not None,
                    "chosen_entry_id": chosen["entry_id"] if chosen else None,
                    "delta_e_ev_per_atom": chosen["delta_e"] if chosen else None,
                    "volume_angstrom3_per_atom": (chosen["volume"] / chosen["natoms"]) if chosen else None,
                    "spacegroup": chosen.get("spacegroup") if chosen else None,
                }
                if (i + 1) % 50 == 0:
                    print(f"  {i + 1}/{len(formulas)} queried", file=sys.stderr)
    except OqmdFetchError:
        # Report only the updated cached count, never a partial coverage
        # percentage or anything resembling a gate result -- a line count
        # is used rather than _load_cache so a corrupt-cache warning can't
        # turn a clean abort into a confusing double failure.
        try:
            n_cached_now = sum(1 for line in args.cache_path.read_text().splitlines() if line.strip())
            print(
                f"stopped early: {n_cached_now}/{len(formulas)} formulas now cached in "
                f"{args.cache_path} (resume later with the same --cache-path to continue)",
                file=sys.stderr,
            )
        except OSError:
            pass
        raise

    n_matched = sum(1 for c in coverage.values() if c["matched"])
    print(f"per-formula coverage: {n_matched}/{len(formulas)} ({100 * n_matched / len(formulas):.1f}%)", file=sys.stderr)

    if args.limit_formulas:
        print(
            "--limit-formulas was set: this is a dev/smoke-test run, not a real "
            "coverage measurement. Refusing to write a manifest that would look "
            "like a complete run with a truncated denominator.",
            file=sys.stderr,
        )
        return

    raw_path = DATA_DIR / "oqmd_coverage_snapshot.json"
    raw_json = json.dumps(raw_snapshot, indent=2, sort_keys=True)
    raw_path.write_text(raw_json)
    checksum = hashlib.sha256(raw_json.encode("utf-8")).hexdigest()

    manifest = {
        "source": {
            "name": "OQMD (Open Quantum Materials Database)",
            "api_endpoint": OQMD_BASE,
            "queried_fields": QUERY_FIELDS,
            "license": "CC BY 4.0 (data), verified via Wayback Machine capture 20260803134040 while oqmd.org's live site was down -- see docs/thermodynamic_selectivity_calibration.md §6.1",
            "citation": [
                "Saal, Kirklin, Aykol, Meredig, Wolverton, JOM 65, 1501 (2013), doi:10.1007/s11837-013-0755-4",
                "Kirklin, Saal, Meredig, Thompson, Doak, Aykol, Ruhl, Wolverton, npj Computational Materials 1, 15010 (2015), doi:10.1038/npjcompumats.2015.10",
            ],
            "api_meta_from_first_response": api_meta_seen,
            "retrieval": build_retrieval_metadata(
                fetch_timestamps, resumed_from_cache_count, unknown_timestamp_count, _now_utc_iso()
            ),
        },
        "polymorph_policy": "keep entries where duplicate_entry_id is null or self-referencing (OQMD's own 'preferred entry' marker), take lowest delta_e among the rest -- see docs/thermodynamic_selectivity_calibration.md §6.3",
        "coverage_snapshot_sha256": checksum,
        "counts": {
            "distinct_formulas_queried": len(formulas),
            "formulas_matched": n_matched,
            "formulas_unmatched": len(formulas) - n_matched,
        },
        "coverage": coverage,
    }
    manifest_path = DATA_DIR / "oqmd_coverage_manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True))
    print(f"wrote {raw_path} and {manifest_path}", file=sys.stderr)

    # Only removed on a fully successful run (past every `return`/abort
    # path above) -- the resume cache's whole purpose is surviving an
    # interrupted run, so it must never be cleaned up on anything less
    # than complete success.
    args.cache_path.unlink(missing_ok=True)
    _cache_meta_path(args.cache_path).unlink(missing_ok=True)


if __name__ == "__main__":
    try:
        main()
    except OqmdFetchError as e:
        print(f"ABORTED, nothing written: {e}", file=sys.stderr)
        sys.exit(1)
