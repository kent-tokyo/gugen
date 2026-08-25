#!/usr/bin/env python3
"""Phase 31 PR 2: a second, additive-only extraction from the same
Kononova et al. 2019 corpus `fetch_kononova.py` already uses (CC BY
4.0, license re-verified live via the figshare API on every run) --
this time KEEPING the reactions that script discards for needing more
than `MAX_PRECURSORS = 4` distinct precursors, instead of discarding
them.

Why: gugen's existing single-step search structurally cannot reach a
target needing more than `SearchBudget::max_precursors_per_plan`
(default 4) precursors at once, regardless of catalog. These
high-arity real, literature-cited reactions are exactly the targets
Phase 31's two-step search (`search_two_step_routes`,
src/multi_step.rs) exists to help with. This script produces the real,
honest holdout set for measuring that -- see
`examples/exploration_two_step_arity_recall.rs` and
`docs/phase31_pr2_two_step_arity_recall.md`.

This script does NOT modify `fetch_kononova.py`, its existing output
(`kononova_sample.jsonl`), or anything that already depends on that
file's exact content -- it re-fetches the same raw dataset
independently and writes to a new, separate output file.

Run: python3 benchmarks/fetch_kononova_high_arity.py
     python3 benchmarks/fetch_kononova_high_arity.py --local /path/to/cached.json
Output: benchmarks/data/kononova_high_arity_sample.jsonl, appends to
        benchmarks/data/ATTRIBUTION.md
"""

import json
import sys
from pathlib import Path

from fetch_kononova import (
    EXCLUDED_ROUTES,
    EXPECTED_REACTION_COUNT,
    fetch_dataset,
    parseable_composition,
    route_key,
)

DATA_DIR = Path(__file__).parent / "data"
OUTPUT = DATA_DIR / "kononova_high_arity_sample.jsonl"
ATTRIBUTION = DATA_DIR / "ATTRIBUTION.md"
MAX_PRECURSORS = 4  # matches fetch_kononova.py's own constant/rationale


def main():
    import argparse

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--local",
        help="path to an already-downloaded copy of the dataset JSON (dev iteration only; "
        "the license is still checked live against the figshare API)",
    )
    args = parser.parse_args()

    reactions = fetch_dataset(args.local)
    print(f"{len(reactions)} raw reactions", file=sys.stderr)

    stats = {
        "unparseable_target": 0,
        "unparseable_precursor": 0,
        "zero_precursors": 0,
        "arity_4_or_fewer": 0,
        "excluded_leakage": 0,
    }
    pool = []

    for r in reactions:
        target = parseable_composition(r["target"])
        if target is None:
            stats["unparseable_target"] += 1
            continue
        precursors_raw = r.get("precursors") or []
        precursor_comps = []
        ok = True
        for p in precursors_raw:
            c = parseable_composition(p)
            if c is None:
                ok = False
                break
            precursor_comps.append((p.get("material_formula", ""), c))
        if not ok:
            stats["unparseable_precursor"] += 1
            continue
        precursor_comps = list({formula: comp for formula, comp in precursor_comps}.items())
        if len(precursor_comps) == 0:
            stats["zero_precursors"] += 1
            continue
        if len(precursor_comps) <= MAX_PRECURSORS:
            stats["arity_4_or_fewer"] += 1
            continue
        key = route_key(target, [c for _, c in precursor_comps])
        if key in EXCLUDED_ROUTES:
            stats["excluded_leakage"] += 1
            continue
        pool.append(
            {
                "doi": r.get("doi"),
                "target_formula": r["target"].get("material_formula"),
                "target_elements": target,
                "precursors": [
                    {"formula": formula, "elements": comp}
                    for formula, comp in precursor_comps
                ],
            }
        )

    for reason, count in stats.items():
        print(f"excluded ({reason}): {count}", file=sys.stderr)
    print(f"high-arity holdout: {len(pool)}", file=sys.stderr)

    pool.sort(key=lambda r: (r["doi"] or "", r["target_formula"] or ""))

    arity_breakdown = {}
    for r in pool:
        n = len(r["precursors"])
        arity_breakdown[n] = arity_breakdown.get(n, 0) + 1
    for n in sorted(arity_breakdown):
        print(f"  arity {n}: {arity_breakdown[n]}", file=sys.stderr)

    DATA_DIR.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT, "w") as f:
        for r in pool:
            f.write(json.dumps(r, sort_keys=True) + "\n")
    print(f"wrote {len(pool)} reactions to {OUTPUT}", file=sys.stderr)

    with open(ATTRIBUTION, "a") as f:
        f.write(
            "\n\n"
            "# Attribution: benchmarks/data/kononova_high_arity_sample.jsonl\n\n"
            "Derived from the same Kononova et al. 2019 corpus cited above "
            "(figshare DOI 10.6084/m9.figshare.9722159, CC BY 4.0, license "
            "verified live by this script on every run) -- the complement of "
            "`kononova_sample.jsonl`: reactions needing **more than 4** "
            "distinct precursors after de-duplication (gugen's default "
            "`SearchBudget::max_precursors_per_plan`), instead of discarding "
            "them. Used as the real, honest holdout for Phase 31 PR 2's "
            "two-step search recall measurement -- see "
            "`docs/phase31_pr2_two_step_arity_recall.md`.\n\n"
            "## How this sample was generated\n\n"
            f"`python3 benchmarks/fetch_kononova_high_arity.py`, filtering "
            f"{len(reactions)} raw reactions ({EXPECTED_REACTION_COUNT} expected). "
            "No downsampling -- every eligible high-arity reaction is kept.\n\n"
            "Exclusion counts (each reaction excluded for exactly one reason, "
            "in this check order; note this splits `fetch_kononova.py`'s own "
            "combined `zero_or_too_many_precursors` count into its two real "
            "components):\n\n"
            f"- Unparseable target: {stats['unparseable_target']}\n"
            f"- Unparseable precursor: {stats['unparseable_precursor']}\n"
            f"- Zero precursors after de-duplication: {stats['zero_precursors']}\n"
            f"- 4 or fewer precursors after de-duplication (already covered by "
            f"`kononova_sample.jsonl`'s own eligible pool): {stats['arity_4_or_fewer']}\n"
            f"- Leakage against a route already used by `tests/validation.rs` "
            f"or `src/literature_conditions.rs` (same `EXCLUDED_ROUTES` check "
            f"as `fetch_kononova.py`): {stats['excluded_leakage']}\n\n"
            f"High-arity holdout: **{len(pool)}** reactions. Arity breakdown: "
            + ", ".join(f"{n}={c}" for n, c in sorted(arity_breakdown.items()))
            + ".\n\n"
            "This file is regenerated by re-running the script above, not "
            "hand-edited.\n"
        )
    print(f"appended to {ATTRIBUTION}", file=sys.stderr)


if __name__ == "__main__":
    main()
