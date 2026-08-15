#!/usr/bin/env python3
"""Phase 21A: does an independent, licensable dataset of experimentally
compared multi-route synthesis outcomes exist, at a scale that could
support a future thermodynamic-selectivity calibration study (Phase 21B,
gated on this script's own findings)? This script does not compute or
touch any gugen thermodynamic quantity -- it only characterizes a
candidate LABEL dataset. See docs/thermodynamic_selectivity_dataset_feasibility.md
for the full report this script's output feeds.

Dataset: Lee, Cruse, Baibakova, Ceder, Jain (2025), "Text-mined dataset of
solid-state syntheses with impurity phases using Large Language Model",
Scientific Data. Figshare DOI 10.6084/m9.figshare.30423274, license
verified live via the figshare API on every run (CC BY 4.0).

Run: python3 benchmarks/audit_thermodynamic_selectivity_dataset_feasibility.py
     python3 benchmarks/audit_thermodynamic_selectivity_dataset_feasibility.py --local /path/to/cached.json.gz
       (dev iteration, skips the 33.5MB download; the license check still
       runs against the live API. The local copy MUST be byte-identical to
       this script's own download -- this script verifies its md5 against
       the live API's computed_md5 either way, so a wrong/stale local copy
       is caught, not silently trusted.)
Output: benchmarks/data/thermodynamic_selectivity_dataset_feasibility_manifest.json,
        benchmarks/data/ATTRIBUTION.md (appended)

What this script does NOT do (Phase 21A's own scope boundary): it does not
query Materials Project or any thermodynamic-entry database for coverage,
does not run gugen's own thermodynamic functions, and does not compute or
propose a Score01 mapping. Those are explicitly Phase 21B's tasks, gated on
this script's GO/NO-GO finding.

Leakage exclusion: gugen's own curated validation-fixture targets
(tests/validation.rs, src/literature_conditions.rs) must never appear in a
future calibration label set -- reusing them would let this new phase
"validate against" targets gugen's own route-generation code was already
tuned/tested against. Matches benchmarks/fetch_kononova.py's identical
target-level exclusion discipline.
"""

import argparse
import gzip
import hashlib
import json
import ssl
import sys
import urllib.request
from collections import Counter, defaultdict
from pathlib import Path

try:
    # See benchmarks/fetch_kononova.py's identical fallback: some Python
    # installs ship no usable CA trust store for urllib's default SSL
    # context. certifi's bundle is a correct, standard fallback, not a
    # verification bypass.
    import certifi

    _SSL_CONTEXT = ssl.create_default_context(cafile=certifi.where())
except ImportError:
    _SSL_CONTEXT = None

FIGSHARE_ARTICLE = "https://api.figshare.com/v2/articles/30423274"
EXPECTED_LICENSE = "CC BY 4.0"
DATA_DIR = Path(__file__).parent / "data"

# Same 5 targets as benchmarks/fetch_kononova.py's EXCLUDED_ROUTES, at
# target-level (this phase compares whole targets' route sets, not single
# routes) -- these are gugen's own curated fixtures in
# tests/validation.rs and src/literature_conditions.rs.
LEAKAGE_EXCLUDE_TARGETS = {"LaAlO3", "MgAl2O4", "LiFePO4", "CaO", "BaTiO3"}

# Small-molecule gas species observed as byproducts/co-reactants in this
# corpus's extracted balanced reactions. A reaction naming any of these is
# not computable by gugen's existing balanced_reaction_delta_ev_per_atom
# (src/thermodynamics.rs): that function abstains with Ok(None) the moment
# any participating species has no SolidThermodynamicEntry, and a gas
# species (no crystal volume) never has one.
GAS_FORMULAS = {
    "O2", "CO2", "H2O", "N2", "H2", "NH3", "NO", "NO2", "N2O", "SO2",
    "SO3", "CO", "Cl2", "HCl", "H2S", "CH4", "F2", "HF", "Br2", "I2",
    "H2O2", "N2O5", "NO3",
}


def _fetch_json(url):
    with urllib.request.urlopen(url, context=_SSL_CONTEXT) as resp:
        return json.load(resp)


def verify_license_and_locate_file(article_url):
    meta = _fetch_json(article_url)
    license_name = meta.get("license", {}).get("name")
    if license_name != EXPECTED_LICENSE:
        sys.exit(
            f"REFUSING TO PROCEED: figshare article license is "
            f"{license_name!r}, expected {EXPECTED_LICENSE!r}. This "
            f"script only ever operates against a dataset it has just "
            f"live-verified as {EXPECTED_LICENSE}."
        )
    files = meta.get("files", [])
    if len(files) != 1:
        sys.exit(f"expected exactly 1 file on this article, found {len(files)}")
    f = files[0]
    print(
        f"license verified live: {license_name} ({meta.get('license', {}).get('url')})",
        file=sys.stderr,
    )
    return f["download_url"], f["computed_md5"], f["name"]


def download_and_verify(download_url, expected_md5, filename, local_override):
    if local_override:
        path = Path(local_override)
        print(f"using local file: {path} (still checking its md5 against the live API)", file=sys.stderr)
    else:
        path = DATA_DIR / filename
        print(f"downloading {download_url} -> {path}", file=sys.stderr)
        with urllib.request.urlopen(download_url, context=_SSL_CONTEXT) as resp:
            path.write_bytes(resp.read())

    actual_md5 = hashlib.md5(path.read_bytes()).hexdigest()
    if actual_md5 != expected_md5:
        sys.exit(
            f"REFUSING TO PROCEED: {path} md5 {actual_md5} != figshare's "
            f"reported computed_md5 {expected_md5}. Not the verified object."
        )
    print(f"md5 verified: {actual_md5}", file=sys.stderr)
    return path


def reaction_is_gas_free(target_reaction):
    if not target_reaction:
        return None  # no extracted reaction at all -- distinct from "gas-free"
    has_gas = False
    for entry in target_reaction:
        if not isinstance(entry, list) or len(entry) < 2 or not isinstance(entry[1], dict):
            continue
        for side in ("left", "right"):
            for formula in entry[1].get(side, {}):
                if formula in GAS_FORMULAS:
                    has_gas = True
    return not has_gas


def analyze(records):
    counts = Counter()
    counts["total_records"] = len(records)

    by_target = defaultdict(list)
    for r in records:
        target_list = r.get("target") or []
        tf = target_list[0].get("material_formula") if target_list else None
        if tf is None:
            counts["excluded_null_target"] += 1
            continue
        if tf in LEAKAGE_EXCLUDE_TARGETS:
            counts["excluded_leakage_target_record"] += 1
            continue
        precursor_set = tuple(sorted(p["material_formula"] for p in r.get("precursors", [])))
        outcome = "pure" if len(r.get("impurity_phase", [])) == 0 else "impure"
        gas_free = reaction_is_gas_free(r.get("target_reaction"))
        by_target[tf].append((precursor_set, outcome, gas_free, r.get("DOI")))

    counts["distinct_targets"] = len(by_target)
    counts["outcome_pure_records"] = sum(
        1 for recs in by_target.values() for (_, o, _, _) in recs if o == "pure"
    )
    counts["outcome_impure_records"] = sum(
        1 for recs in by_target.values() for (_, o, _, _) in recs if o == "impure"
    )
    counts["records_with_no_extracted_reaction"] = sum(
        1 for recs in by_target.values() for (_, _, g, _) in recs if g is None
    )
    counts["records_gas_free_reaction"] = sum(
        1 for recs in by_target.values() for (_, _, g, _) in recs if g is True
    )
    counts["records_gas_present_reaction"] = sum(
        1 for recs in by_target.values() for (_, _, g, _) in recs if g is False
    )

    # Two label-aggregation definitions, both computed and both reported
    # (advisor pre-commit finding, Phase 21A): the lenient one ("any_pure":
    # a route counts pure if *any* reported attempt was pure;
    # "gas_free=any": a route counts gas-free-computable if *any* of its
    # extracted reactions was gas-free) is optimistic for a route reported
    # many times. The strict one ("majority": route verdict is whichever
    # outcome most reports agree on, ties excluded; "gas_free=all": every
    # extracted reaction for that route must be gas-free) is the
    # conservative alternative. Both are computed so the headline number
    # is never silently dependent on an undisclosed choice.
    selectivity_signal_targets = []
    lenient_signal = lenient_gas_free = strict_signal = strict_gas_free = 0
    for t, recs in by_target.items():
        route_outcomes = defaultdict(list)
        route_gas_free = defaultdict(list)
        route_dois = defaultdict(set)
        for precset, outcome, gas_free, doi in recs:
            route_outcomes[precset].append(outcome)
            route_gas_free[precset].append(gas_free)
            if doi:
                route_dois[precset].add(doi)

        if len(route_outcomes) < 2:
            continue

        lenient_verdict = {
            route: ("pure" if "pure" in outs else "impure")
            for route, outs in route_outcomes.items()
        }
        strict_verdict = {}
        for route, outs in route_outcomes.items():
            n_pure, n_impure = outs.count("pure"), outs.count("impure")
            if n_pure > n_impure:
                strict_verdict[route] = "pure"
            elif n_impure > n_pure:
                strict_verdict[route] = "impure"
            else:
                strict_verdict[route] = "tie"

        lenient_has_signal = len(set(lenient_verdict.values())) >= 2
        strict_real_verdicts = {v for v in strict_verdict.values() if v != "tie"}
        strict_has_signal = len(strict_real_verdicts) >= 2

        if not (lenient_has_signal or strict_has_signal):
            continue

        all_dois = {d for dois in route_dois.values() for d in dois}
        gas_free_routes_any = {r for r, gf in route_gas_free.items() if True in gf}
        gas_free_routes_all = {
            r for r, gf in route_gas_free.items()
            if [g for g in gf if g is not None] and all(g is True for g in gf if g is not None)
        }

        if lenient_has_signal:
            lenient_signal += 1
            gf_v = {lenient_verdict[r] for r in gas_free_routes_any}
            if len(gas_free_routes_any) >= 2 and len(gf_v) >= 2:
                lenient_gas_free += 1
        if strict_has_signal:
            strict_signal += 1
            gf_v = {strict_verdict[r] for r in gas_free_routes_all if strict_verdict[r] != "tie"}
            if len(gas_free_routes_all) >= 2 and len(gf_v) >= 2:
                strict_gas_free += 1

        if lenient_has_signal:
            gas_free_routes = gas_free_routes_any
            gas_free_verdicts = {lenient_verdict[r] for r in gas_free_routes}
            selectivity_signal_targets.append(
                {
                    "target": t,
                    "n_routes": len(route_outcomes),
                    "n_distinct_dois": len(all_dois),
                    "multi_doi": len(all_dois) >= 2,
                    "gas_free_computable": len(gas_free_routes) >= 2 and len(gas_free_verdicts) >= 2,
                }
            )

    counts["targets_with_selectivity_signal"] = lenient_signal
    counts["targets_with_selectivity_signal_multi_doi"] = sum(
        1 for t in selectivity_signal_targets if t["multi_doi"]
    )
    counts["targets_with_selectivity_signal_gas_free_computable"] = lenient_gas_free
    counts["sensitivity_majority_vote_outcome__selectivity_signal"] = strict_signal
    counts["sensitivity_majority_vote_outcome_and_all_gas_free__gas_free_computable"] = strict_gas_free

    present_leakage_targets = sorted(
        t for t in LEAKAGE_EXCLUDE_TARGETS
        if any((r.get("target") or [{}])[0].get("material_formula") == t for r in records)
    )

    return counts, selectivity_signal_targets, present_leakage_targets


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--local", help="path to a pre-downloaded .json.gz (md5 still verified live)")
    args = parser.parse_args()

    download_url, expected_md5, filename = verify_license_and_locate_file(FIGSHARE_ARTICLE)
    path = download_and_verify(download_url, expected_md5, filename, args.local)

    print(f"loading {path}...", file=sys.stderr)
    with gzip.open(path, "rt") as f:
        records = json.load(f)

    counts, selectivity_signal_targets, present_leakage_targets = analyze(records)

    manifest = {
        "source": {
            "citation": "Lee, Cruse, Baibakova, Ceder, Jain (2025), Scientific Data",
            "figshare_doi": "10.6084/m9.figshare.30423274",
            "license": EXPECTED_LICENSE,
            "file_md5": expected_md5,
        },
        "sample_gate": {
            "minimum_targets_required": 30,
            "minimum_routes_per_target": 2,
        },
        "counts": dict(counts),
        "leakage_exclusion": {
            "excluded_targets": sorted(LEAKAGE_EXCLUDE_TARGETS),
            "present_in_dataset": present_leakage_targets,
        },
        "selectivity_signal_targets_sample": sorted(
            selectivity_signal_targets, key=lambda t: -t["n_routes"]
        )[:50],
    }

    DATA_DIR.mkdir(exist_ok=True)
    manifest_path = DATA_DIR / "thermodynamic_selectivity_dataset_feasibility_manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True))
    print(f"wrote {manifest_path}", file=sys.stderr)

    for k, v in counts.items():
        print(f"{k}: {v}")
    print(f"leakage targets present (excluded from all counts above): {present_leakage_targets}")
    lenient = counts["targets_with_selectivity_signal_gas_free_computable"]
    strict = counts["sensitivity_majority_vote_outcome_and_all_gas_free__gas_free_computable"]
    gate_pass = lenient >= 30
    strict_gate_pass = strict >= 30
    print(f"\nsample gate (>=30 targets, >=2 gas-free-computable routes, differing outcome):")
    print(f"  lenient (any_pure outcome, any_gas_free route):    {'PASS' if gate_pass else 'FAIL'} ({lenient} found)")
    print(f"  strict (majority-vote outcome, all_gas_free route): {'PASS' if strict_gate_pass else 'FAIL'} ({strict} found)")


if __name__ == "__main__":
    main()
