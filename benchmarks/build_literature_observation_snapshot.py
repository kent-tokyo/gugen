#!/usr/bin/env python3
"""Builds a gugen-native literature-observation snapshot (Phase 20B) from
the Kononova et al. 2019 text-mined synthesis dataset (CC BY 4.0, license
verified live via the figshare API, same corpus Phase 20A audited and
Phase 11's fetch_kononova.py samples from).

This script does the ONE-TIME, corpus-specific extraction work so gugen's
own Rust crate never has to understand Kononova's raw schema (string-typed
element amounts, a free-text atmosphere vocabulary, `HeatingOperation`
entries that sometimes carry multiple disagreeing temperature readings).
Output is gugen's own snapshot schema
(`gugen::CORPUS_SNAPSHOT_SCHEMA_VERSION`) -- a manifest plus a flat list of
observations, each already shaped exactly like
`gugen::CorpusHeatingObservation`'s JSON representation, so
`LiteratureObservationCorpus::load` only ever parses gugen's own clean
schema and re-validates every value through gugen's existing constructors.

Run: python3 benchmarks/build_literature_observation_snapshot.py [--local PATH] [--limit N]
Output: benchmarks/data/literature_observation_snapshot.json (gitignored --
        this file is regenerable, not committed) and, always,
        benchmarks/data/ATTRIBUTION.md (regenerated to describe both this
        snapshot and fetch_kononova.py's sample, since both derive from the
        same corpus and this script runs second).

Extraction rules (defined here, not adjusted after looking at results,
mirroring fetch_kononova.py's/audit_kononova_corpus.py's own discipline):

  - Structural validity gate is IDENTICAL to audit_kononova_corpus.py's:
    parseable target, parseable precursors, 1-4 distinct precursors after
    dedup. A record failing this gate contributes zero observations.
  - One observation per `HeatingOperation` in a valid record (not one
    aggregate per record) -- `operation_index` is the operation's 0-based
    position among that record's own HeatingOperations, so multi-step
    records (calcine then sinter) are never flattened into one entry.
  - `heating_temperature`/`heating_time` are each a list of candidate
    readings (usually 0 or 1 entries; verified live: 2,377 of 31,998+2,706
    HeatingOperations in the full corpus have 2+ temperature entries).
    A single entry resolves to (min ?? max, max ?? min) -- point values
    fall back to a degenerate min==max range. Multiple entries that all
    agree exactly resolve the same way; multiple entries that disagree
    resolve to `None` rather than guessing which is authoritative (verified
    live: of those 2,377 multi-entry operations, 2,311 (97.2%) actually
    disagree -- this is not a rare edge case, it is the common case for
    multi-entry operations, hence the conservative rule). Every entry's
    `units` field is checked (`"C"` for temperature, `"h"` for duration);
    an unexpected unit voids that specific entry rather than being
    silently converted (verified live: 100% of the corpus's 31,027
    temperature entries and 24,954 duration entries already report "C"/"h"
    respectively, so this never fires today -- it is a forward-looking
    check against a future corpus release, not dead code against this one).
  - `atmosphere` is a list of free-text strings (verified live: only 10
    distinct strings across the entire 19,488-record corpus). Zero entries
    -> `None`. Exactly one entry, from the six unambiguous ones (air,
    oxygen, argon, nitrogen, hydrogen, carbon monoxide) -> the matching
    structured `Atmosphere` variant. Anything else (an unrecognized single
    string, e.g. "PbO"/"thermal carbon"/"carbon dioxide"/"ambient", or 2+
    reported strings) -> `Atmosphere::Controlled { description }`,
    preserving the original text verbatim rather than asserting an
    unverified equivalence (e.g. "ambient" is NOT assumed to mean "air").
  - Deduplication is NOT done here -- `LiteratureObservationCorpus::load`
    does it, deterministically, on the Rust side. This script emits every
    observation that parses, in raw corpus order; the manifest's
    `record_count` is simply the count of observations written.
"""

import argparse
import hashlib
import json
import ssl
import sys
import urllib.request
from collections import Counter
from pathlib import Path

try:
    import certifi

    _SSL_CONTEXT = ssl.create_default_context(cafile=certifi.where())
except ImportError:
    _SSL_CONTEXT = None


def _urlopen(url):
    return urllib.request.urlopen(url, context=_SSL_CONTEXT)


FIGSHARE_ARTICLE = "https://api.figshare.com/v2/articles/9722159"
EXPECTED_REACTION_COUNT = 19488
MAX_PRECURSORS = 4
DATA_DIR = Path(__file__).parent / "data"
OUTPUT = DATA_DIR / "literature_observation_snapshot.json"
ATTRIBUTION = DATA_DIR / "ATTRIBUTION.md"
SCHEMA_VERSION = "gugen-literature-observation-snapshot-v1"

## serde's default enum representation: a unit variant (no fields, e.g.
## `Atmosphere::Air`) serializes as a bare JSON string; a variant with
## fields serializes as `{"VariantName": {field: value}}`. Verified
## directly against this script's own output (`route_family` -- also a
## unit variant -- comes out as the bare string `"ConventionalSolidState"`,
## not `{"ConventionalSolidState": null}`).
ATMOSPHERE_MAP = {
    "air": "Air",
    "oxygen": "OxygenRich",
    "argon": {"Inert": {"gas": "Argon"}},
    "nitrogen": {"Inert": {"gas": "Nitrogen"}},
    "hydrogen": {"Reducing": {"agent": "Hydrogen"}},
    "carbon monoxide": {"Reducing": {"agent": "CarbonMonoxide"}},
}


def is_plain_positive_number(x):
    try:
        return float(x) > 0
    except (TypeError, ValueError):
        return False


def parseable_composition(material):
    """Same gate as audit_kononova_corpus.py's own function of this name --
    kept duplicated deliberately (this project's established per-script
    convention, e.g. audit_kononova_corpus.py duplicating rather than
    importing from fetch_kononova.py), not re-derived independently."""
    if material.get("amounts_vars") or material.get("elements_vars"):
        return None
    comps = material.get("composition") or []
    if len(comps) != 1:
        return None
    elements = comps[0].get("elements") or {}
    if not elements:
        return None
    if not all(is_plain_positive_number(v) for v in elements.values()):
        return None
    return {el: float(amt) for el, amt in elements.items()}


def fetch_dataset(local_path):
    with _urlopen(FIGSHARE_ARTICLE) as resp:
        meta = json.load(resp)
    license_name = (meta.get("license") or {}).get("name")
    if license_name != "CC BY 4.0":
        sys.exit(f"REFUSING: license is {license_name!r}, expected 'CC BY 4.0'")
    print(f"license OK ({license_name})", file=sys.stderr)

    if local_path:
        print(f"using local file {local_path}", file=sys.stderr)
        with open(local_path, "rb") as f:
            raw_bytes = f.read()
    else:
        files = meta.get("files") or []
        if len(files) != 1:
            sys.exit(f"REFUSING: expected exactly one file, found {len(files)}")
        download_url = files[0]["download_url"]
        print(f"downloading {download_url}", file=sys.stderr)
        with _urlopen(download_url) as resp:
            raw_bytes = resp.read()

    checksum = hashlib.sha256(raw_bytes).hexdigest()
    data = json.loads(raw_bytes)
    if len(data) != EXPECTED_REACTION_COUNT:
        sys.exit(
            f"REFUSING: expected {EXPECTED_REACTION_COUNT} reactions, got {len(data)} "
            "-- this is not the corpus version this script was built against"
        )
    print(f"{len(data)} raw reactions, sha256={checksum}", file=sys.stderr)
    return data, checksum


def resolved_range(entries, expected_units, unit_mismatches):
    """One `heating_temperature`/`heating_time`-shaped list -> `None` or a
    `(min, max)` tuple, per the module-doc-comment rules above."""
    candidates = []
    for entry in entries:
        units = entry.get("units")
        if units != expected_units:
            if units is not None:
                unit_mismatches[units] += 1
            continue
        mx = entry.get("max_value")
        mn = entry.get("min_value")
        if mx is None and mn is None:
            continue
        if mx is None:
            mx = mn
        if mn is None:
            mn = mx
        mx = float(mx)
        mn = float(mn)
        import math

        if not (math.isfinite(mx) and math.isfinite(mn)) or mn > mx:
            continue
        candidates.append((mn, mx))
    if not candidates:
        return None
    if len(set(candidates)) == 1:
        return candidates[0]
    return None  # disagreeing multi-entry reading -- don't guess


def atmosphere_for(strings):
    if not strings:
        return None
    if len(strings) == 1 and strings[0] in ATMOSPHERE_MAP:
        return ATMOSPHERE_MAP[strings[0]]
    description = ", ".join(sorted(set(strings)))
    return {"Controlled": {"description": description}}


def build_observations(data, limit, stats):
    observations = []
    for corpus_record_index, r in enumerate(data):
        target = parseable_composition(r.get("target") or {})
        if target is None:
            stats["unparseable_target"] += 1
            continue

        raw_precursors = [parseable_composition(p) for p in (r.get("precursors") or [])]
        if any(p is None for p in raw_precursors):
            stats["unparseable_precursor"] += 1
            continue

        # De-duplicate identical precursor compositions within one
        # reaction, same convention audit_kononova_corpus.py uses before
        # applying the [1, 4] bound.
        seen = []
        for p in raw_precursors:
            key = tuple(sorted(p.items()))
            if key not in [tuple(sorted(s.items())) for s in seen]:
                seen.append(p)
        precursors = seen
        if not (1 <= len(precursors) <= MAX_PRECURSORS):
            stats["zero_or_too_many_precursors"] += 1
            continue

        stats["valid_records"] += 1

        heating_ops = [
            op for op in (r.get("operations") or []) if op.get("type") == "HeatingOperation"
        ]
        doi = r.get("doi") or None
        for operation_index, op in enumerate(heating_ops):
            conditions = op.get("conditions") or {}
            temp = resolved_range(
                conditions.get("heating_temperature") or [], "C", stats["temp_unit_mismatches"]
            )
            dur = resolved_range(
                conditions.get("heating_time") or [], "h", stats["time_unit_mismatches"]
            )
            atmosphere = atmosphere_for(conditions.get("atmosphere") or [])

            stats["observations_emitted"] += 1
            observations.append(
                {
                    "target": target,
                    "precursors": precursors,
                    "route_family": "ConventionalSolidState",
                    "operation_index": operation_index,
                    "temperature": (
                        {"min_celsius": temp[0], "max_celsius": temp[1]} if temp else None
                    ),
                    "duration": ({"min_hours": dur[0], "max_hours": dur[1]} if dur else None),
                    "atmosphere": atmosphere,
                    "doi": doi,
                    "corpus_record_index": corpus_record_index,
                }
            )
            if limit and len(observations) >= limit:
                return observations
    return observations


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--local", help="path to an already-downloaded copy of the dataset JSON")
    parser.add_argument(
        "--limit",
        type=int,
        default=None,
        help="stop after emitting this many observations (small fixtures only)",
    )
    parser.add_argument(
        "--out", type=Path, default=OUTPUT, help="output path (default: benchmarks/data/...)"
    )
    args = parser.parse_args()

    data, checksum = fetch_dataset(args.local)

    stats = Counter()
    observations = build_observations(data, args.limit, stats)

    manifest = {
        "source": (
            "Kononova, O. et al. 'Text-mined dataset of inorganic materials synthesis "
            "recipes.' Scientific Data 6, 203 (2019), DOI 10.1038/s41597-019-0224-1 "
            "(figshare DOI 10.6084/m9.figshare.9722159, CC BY 4.0)"
        ),
        "release": f"figshare article 9722159, raw file sha256={checksum}",
        "schema_version": SCHEMA_VERSION,
        "checksum": checksum,
        "record_count": len(observations),
    }
    snapshot = {"manifest": manifest, "observations": observations}

    args.out.parent.mkdir(parents=True, exist_ok=True)
    with open(args.out, "w") as f:
        json.dump(snapshot, f)

    print(f"wrote {len(observations)} observations to {args.out}", file=sys.stderr)
    print(f"valid_records={stats['valid_records']}", file=sys.stderr)
    print(f"unparseable_target={stats['unparseable_target']}", file=sys.stderr)
    print(f"unparseable_precursor={stats['unparseable_precursor']}", file=sys.stderr)
    print(f"zero_or_too_many_precursors={stats['zero_or_too_many_precursors']}", file=sys.stderr)
    print(f"observations_emitted={stats['observations_emitted']}", file=sys.stderr)
    if stats["temp_unit_mismatches"]:
        print(f"temp_unit_mismatches={dict(stats['temp_unit_mismatches'])}", file=sys.stderr)
    if stats["time_unit_mismatches"]:
        print(f"time_unit_mismatches={dict(stats['time_unit_mismatches'])}", file=sys.stderr)

    if args.out == OUTPUT and not args.limit:
        marker = "# Attribution: benchmarks/data/literature_observation_snapshot.json"
        existing = ATTRIBUTION.read_text() if ATTRIBUTION.exists() else ""
        # Idempotent: strip any section this script previously appended
        # (from the marker heading to EOF) before appending a fresh one,
        # so re-running the script never duplicates content.
        base = existing.split(marker)[0].rstrip("\n")
        section = (
            f"{marker}\n\n"
            "Derived from the same Kononova et al. 2019 corpus cited above "
            "(figshare DOI 10.6084/m9.figshare.9722159, CC BY 4.0, license verified live "
            "by this script on every run).\n\n"
            "Generated by `python3 benchmarks/build_literature_observation_snapshot.py`. "
            "This file is gitignored (not committed, not part of the published crate) -- "
            "regenerate it locally to reproduce Phase 20B's performance measurements. "
            f"Raw source sha256: `{checksum}`.\n\n"
            f"- Valid records (structural validity gate, identical to "
            f"`audit_kononova_corpus.py`'s): {stats['valid_records']}\n"
            f"- Unparseable target: {stats['unparseable_target']}\n"
            f"- Unparseable precursor: {stats['unparseable_precursor']}\n"
            f"- Zero or >4 precursors after dedup: {stats['zero_or_too_many_precursors']}\n"
            f"- Observations emitted (one per `HeatingOperation` in a valid record): "
            f"{stats['observations_emitted']}\n"
        )
        ATTRIBUTION.write_text(f"{base}\n\n\n{section}")


if __name__ == "__main__":
    main()
