#!/usr/bin/env python3
"""Phase 21B condition 2: draws a reproducible, DOI-independent sample of
individual synthesis-attempt records from the Phase 21A "clean" population
(gas-free-computable, artifact-filtered, selectivity-signal-contributing
routes) for manual pure/impure-label accuracy auditing against original
source papers. Mirrors benchmarks/sample_literature_observation_audit.py's
methodology exactly: DOI (not record) is the independence unit, since
multiple records from the same paper share one extraction run and are not
independent evidence about extraction accuracy.

This script does NOT judge accuracy -- it only decides which DOI-backed
records get checked, deterministically, so the sample can be regenerated
exactly from a seed. A reviewer (human or an agent with real web access)
then attempts to verify each one against its source paper and records a
judgment; this script has no opinion on what that judgment will be.

Strata (disjoint by DOI -- every route has exactly one verdict, so
`impure` and `pure` already partition the whole population; there is no
separate "baseline" stratum, unlike sample_literature_observation_audit.py,
since that script's `sparse`/`fully_resolved` distinction doesn't apply
here):
  1. impure -- a route whose sampled record reports >=1 impurity phase.
     Prioritized because "impure" is the minority class (23.2% of all
     records; see docs/thermodynamic_selectivity_dataset_feasibility.md
     §5) and a calibration study lives or dies on this class being real,
     not an artifact of looser reporting standards.
  2. pure -- a route whose sampled record reports zero impurity phases.

Run (pilot):
  python3 benchmarks/sample_thermodynamic_selectivity_audit.py \\
      --wave 0 --seed 20260815 --sizes impure=8,pure=7
Output: benchmarks/data/thermodynamic_selectivity_audit_manifest.json
        (DOI, target formula, precursor formulas, and gugen's own already-
        public field values only -- no raw paper text, matching Phase
        20D's redistributable-data constraint.)
"""

import argparse
import json
import random
import sys
from pathlib import Path

CLEAN_POPULATION_PATH = Path(__file__).parent / "data" / "thermodynamic_selectivity_clean_population.json"
MANIFEST_PATH = Path(__file__).parent / "data" / "thermodynamic_selectivity_audit_manifest.json"


def load_manifest():
    if MANIFEST_PATH.exists():
        return json.loads(MANIFEST_PATH.read_text())
    return {"waves": []}


def already_sampled_dois(manifest):
    seen = set()
    for wave in manifest["waves"]:
        for row in wave["rows"]:
            seen.add(row["doi"])
    return seen


def stratum_of(row):
    return "impure" if row["verdict"] == "impure" else "pure"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--wave", type=int, required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--sizes", required=True, help="e.g. impure=8,pure=7")
    args = parser.parse_args()
    sizes = dict(kv.split("=") for kv in args.sizes.split(","))
    sizes = {k: int(v) for k, v in sizes.items()}

    population = json.loads(CLEAN_POPULATION_PATH.read_text())
    print(f"clean population: {len(population)} route rows", file=sys.stderr)

    # Explode to one row per (route, DOI) -- the actual atomic unit a
    # reviewer checks is "does DOI X's paper support this route's outcome
    # for this target", and a route can carry multiple DOIs.
    exploded = []
    for row in population:
        for doi in row["dois"]:
            exploded.append(
                {
                    "target": row["target"],
                    "route": row["route"],
                    "verdict": row["verdict"],
                    "doi": doi,
                }
            )
    print(f"exploded to {len(exploded)} (route, DOI) rows", file=sys.stderr)

    manifest = load_manifest()
    excluded_dois = already_sampled_dois(manifest)

    rng = random.Random(args.seed)
    by_doi = {}
    for row in exploded:
        if row["doi"] in excluded_dois:
            continue
        by_doi.setdefault(row["doi"], []).append(row)

    strata = {"impure": [], "pure": []}
    for doi, rows in by_doi.items():
        # one row per DOI, deterministic pick: lowest (target, route) tuple
        rows_sorted = sorted(rows, key=lambda r: (r["target"], tuple(r["route"])))
        chosen = rows_sorted[0]
        strata[stratum_of(chosen)].append((doi, chosen))

    for s in strata.values():
        rng.shuffle(s)

    drawn_dois = set()
    wave_rows = []
    for stratum_name in list(sizes.keys()):
        n = sizes[stratum_name]
        pool = [item for item in strata[stratum_name] if item[0] not in drawn_dois]
        picked = pool[:n]
        for doi, row in picked:
            drawn_dois.add(doi)
            wave_rows.append(
                {
                    "doi": doi,
                    "target": row["target"],
                    "route": row["route"],
                    "claimed_verdict": row["verdict"],
                    "stratum": stratum_name,
                    "judgment": None,
                }
            )
        print(f"stratum {stratum_name}: requested {n}, drew {len(picked)} (pool had {len(pool)})", file=sys.stderr)

    manifest["waves"].append(
        {"wave": args.wave, "seed": args.seed, "sizes": sizes, "rows": wave_rows}
    )
    MANIFEST_PATH.write_text(json.dumps(manifest, indent=2, sort_keys=False))
    print(f"wrote {MANIFEST_PATH}: wave {args.wave}, {len(wave_rows)} rows, judgment=null pending review", file=sys.stderr)


if __name__ == "__main__":
    main()
