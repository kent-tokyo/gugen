#!/usr/bin/env python3
"""Phase 28: builds a deliberately large, frozen decoy-augmented catalog
per (target, route) pair in exploration_recall_manifest.json, sized to
create *real* SearchBudget::default() pressure (max_precursor_sets:
10_000) -- the opposite tuning from examples/large_scale_benchmark.rs's
own decoy pool, which that file's own comment says "was chosen by
measuring this exact rate at several candidate values and picking the
largest that kept it negligible." Reusing that catalog here would give
Phase 29 ~zero headroom to demonstrate improvement; this script exists
specifically to avoid that mistake -- do not reuse or merge with
large_scale_benchmark's decoy logic.

Run: python3 benchmarks/exploration_build_frozen_decoy_catalog.py
Output: benchmarks/data/exploration_frozen_catalog_manifest.json

Per row, the catalog is: the route's own real precursors (so the known
answer is always findable) plus up to TARGET_CATALOG_SIZE -
len(route) deterministically-chosen decoys, drawn from a global
frequency-ranked pool of precursor formulas seen elsewhere in the
manifest (most-cited-across-all-routes first, formula-string tiebreak),
filtered to decoys sharing >=1 element with the target -- decoys sharing
zero elements would be filtered out by search_precursor_sets's own
MissingTargetElement/forbidden-element checks before consuming any real
combination-generation budget, so they would add catalog size without
adding budget pressure.

At TARGET_CATALOG_SIZE=28, C(28,1)+C(28,2)+C(28,3)+C(28,4) = 24,157 --
comfortably above the default 10,000-combination budget, so most rows
should exhaust the budget under today's dictionary-order search before
ever reaching a 4-precursor combination (today's generator truncates
from the high-arity end -- see src/precursor.rs's own
generate_combinations). This is the real headroom Phase 29 needs to
demonstrate against.
"""

import json
import sys
from collections import Counter
from pathlib import Path

DATA_DIR = Path(__file__).parent / "data"
MANIFEST_PATH = DATA_DIR / "exploration_recall_manifest.json"
CATALOG_PATH = DATA_DIR / "exploration_frozen_catalog_manifest.json"

TARGET_CATALOG_SIZE = 28


def build_global_formula_pool(rows):
    """formula -> elements dict, plus a frequency count, over every
    resolved route member across the whole manifest (not just one
    row) -- this is also usable as Phase 28's own reproducible
    "frequency prior" baseline (the owner's spec names one to
    reproduce), computed here once rather than twice."""
    elements_by_formula = {}
    frequency = Counter()
    for row in rows:
        for formula, amounts in zip(row["route"], row["route_amounts"]):
            if amounts is None:
                continue
            elements_by_formula.setdefault(formula, amounts)
            frequency[formula] += 1
    return elements_by_formula, frequency


def decoys_for_target(target_elements, route_formulas, elements_by_formula, ranked_pool, count):
    target_element_set = set(target_elements.keys())
    route_set = set(route_formulas)
    chosen = []
    for formula in ranked_pool:
        if len(chosen) >= count:
            break
        if formula in route_set:
            continue
        if set(elements_by_formula[formula].keys()) & target_element_set:
            chosen.append(formula)
    return chosen


def build_catalog():
    manifest = json.loads(MANIFEST_PATH.read_text())
    rows = manifest["rows"]
    elements_by_formula, frequency = build_global_formula_pool(rows)
    # Deterministic global rank: most-frequent first, formula string as
    # a stable tiebreak (never insertion order, which would depend on
    # dict-building/iteration details this script doesn't want to load-
    # bear on).
    ranked_pool = sorted(elements_by_formula.keys(), key=lambda f: (-frequency[f], f))

    catalog_rows = []
    exhaustion_headroom_examples = 0
    for row in rows:
        if row["target_amounts"] is None or any(a is None for a in row["route_amounts"]):
            continue  # can't build a real catalog without real amounts
        route_entries = [
            {"formula": f, "elements": a}
            for f, a in zip(row["route"], row["route_amounts"])
        ]
        needed = max(TARGET_CATALOG_SIZE - len(route_entries), 0)
        decoy_formulas = decoys_for_target(
            row["target_amounts"], row["route"], elements_by_formula, ranked_pool, needed
        )
        decoy_entries = [
            {"formula": f, "elements": elements_by_formula[f]} for f in decoy_formulas
        ]
        candidates = route_entries + decoy_entries
        if len(candidates) >= 23:  # C(23,1..4) = 10,902 -- exceeds default budget
            exhaustion_headroom_examples += 1
        catalog_rows.append(
            {
                "target_formula": row["target_formula"],
                "target_elements": row["target_amounts"],
                "route": row["route"],
                "candidates": candidates,
            }
        )

    return {
        "config": {
            "target_catalog_size": TARGET_CATALOG_SIZE,
            "decoy_pool_size": len(ranked_pool),
            "note": "deliberately large -- see this script's own module docstring "
            "for why examples/large_scale_benchmark.rs's decoy pool must not be "
            "reused here",
        },
        "counts": {
            "rows": len(catalog_rows),
            "rows_with_candidate_count_exceeding_10902_combination_threshold": (
                exhaustion_headroom_examples
            ),
        },
        "rows": catalog_rows,
    }


def main():
    catalog = build_catalog()
    CATALOG_PATH.write_text(json.dumps(catalog, indent=2, sort_keys=True) + "\n")
    counts = catalog["counts"]
    print(
        f"wrote {CATALOG_PATH}: {counts['rows']} row(s), "
        f"{counts['rows_with_candidate_count_exceeding_10902_combination_threshold']} "
        "with a candidate count already exceeding the theoretical "
        "combination-count threshold for SearchBudget::default()",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
