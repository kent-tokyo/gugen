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
"not measured in this phase") exists to prevent.

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
        real snapshot identity -- API meta, retrieval datetime, query,
        checksum -- plus the coverage numbers).
"""

import argparse
import datetime
import hashlib
import json
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


class OqmdFetchError(RuntimeError):
    pass


def _urlopen(url, timeout):
    req = urllib.request.Request(url, headers={"User-Agent": "gugen-phase21b-research/1.0"})
    return urllib.request.urlopen(req, context=_SSL_CONTEXT, timeout=timeout)


def query_composition(formula, timeout=20):
    params = urllib.parse.urlencode({"composition": formula, "limit": 50, "fields": ",".join(QUERY_FIELDS)})
    url = f"{OQMD_BASE}?{params}"
    try:
        with _urlopen(url, timeout) as resp:
            status = resp.status
            body = resp.read()
    except urllib.error.HTTPError as e:
        raise OqmdFetchError(f"HTTP {e.code} for composition={formula!r}: {url}") from e
    except (urllib.error.URLError, TimeoutError, OSError) as e:
        raise OqmdFetchError(f"network error for composition={formula!r}: {e}") from e

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
    data = payload["data"]
    if not data:
        return  # zero matches is a legitimate outcome, not a schema problem
    missing = REQUIRED_ENTRY_FIELDS - data[0].keys()
    if missing:
        raise OqmdFetchError(
            f"first non-empty OQMD response (composition={formula!r}) is missing required "
            f"fields {missing} -- aborting rather than guessing a mapping for an unfamiliar schema"
        )


def select_polymorph(entries):
    """Fixed policy (docs/thermodynamic_selectivity_calibration.md §6.3):
    exclude duplicate_entry_id rows and rows with no usable delta_e (a
    real OQMD entry can exist with a null delta_e, e.g. an unconverged
    calculation -- that is a distinct, reportable abstention category
    from "no entry at all," not a crash), take lowest delta_e among the
    rest. Returns (chosen_or_None, n_duplicate_excluded, n_null_energy_excluded)."""
    n_duplicate = sum(1 for e in entries if e.get("duplicate_entry_id"))
    after_dup = [e for e in entries if not e.get("duplicate_entry_id")]
    n_null_energy = sum(1 for e in after_dup if e.get("delta_e") is None)
    candidates = [e for e in after_dup if e.get("delta_e") is not None]
    chosen = min(candidates, key=lambda e: e["delta_e"]) if candidates else None
    return chosen, n_duplicate, n_null_energy


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--sleep", type=float, default=0.3, help="seconds between requests (politeness)")
    parser.add_argument("--limit-formulas", type=int, default=None, help="dev-only: cap the number of formulas queried")
    args = parser.parse_args()

    population = json.loads(CLEAN_POPULATION_PATH.read_text())
    formulas = set()
    for row in population:
        formulas.add(row["target"])
        formulas.update(row["route"])
    formulas = sorted(formulas)
    if args.limit_formulas:
        formulas = formulas[: args.limit_formulas]
    print(f"{len(formulas)} distinct formulas to query", file=sys.stderr)

    retrieved_at = datetime.datetime.now(datetime.timezone.utc).isoformat()
    raw_snapshot = {}
    coverage = {}
    api_meta_seen = None
    schema_validated = False

    for i, formula in enumerate(formulas):
        payload = query_composition(formula)  # raises OqmdFetchError -> script aborts, nothing written
        if not schema_validated:
            validate_first_response_schema(payload, formula)
            schema_validated = True
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
        time.sleep(args.sleep)

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
            "retrieved_at_utc": retrieved_at,
        },
        "polymorph_policy": "exclude duplicate_entry_id rows, take lowest delta_e among the rest -- see docs/thermodynamic_selectivity_calibration.md §6.3",
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


if __name__ == "__main__":
    try:
        main()
    except OqmdFetchError as e:
        print(f"ABORTED, nothing written: {e}", file=sys.stderr)
        sys.exit(1)
