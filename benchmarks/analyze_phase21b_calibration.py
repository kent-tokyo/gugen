#!/usr/bin/env python3
"""Phase 21B calibration: the pre-registered analysis
(docs/phase21b_calibration_preregistration.md), applied to
`examples/exploration_phase21b_calibration.rs`'s real, already-computed
per-row balance()/energy result. Computes no new chemistry -- every
number here is either a selection (which row represents a target) or
arithmetic over numbers gugen's own Rust code already produced.

Run: python3 benchmarks/analyze_phase21b_calibration.py
Reads: benchmarks/data/exploration_phase21b_calibration_result.json
Writes: benchmarks/data/phase21b_calibration_result.json
"""

import json
import re
import sys
from collections import Counter, defaultdict
from math import comb
from pathlib import Path

DATA_DIR = Path(__file__).parent / "data"
INPUT_PATH = DATA_DIR / "exploration_phase21b_calibration_result.json"
OUTPUT_PATH = DATA_DIR / "phase21b_calibration_result.json"

MIN_QUALIFYING_TARGETS = 30  # pre-registered floor, same as every prior phase's own

_FAMILY_RULES = [
    ("Oxide", {"O"}),
    ("Sulfide/chalcogenide", {"S", "Se", "Te"}),
    ("Halide", {"F", "Cl", "Br", "I"}),
    ("Nitride", {"N"}),
    ("Phosphide/phosphate", {"P"}),
]
_ELEMENT_RE = re.compile(r"[A-Z][a-z]?")


def classify_family(formula):
    elements = set(_ELEMENT_RE.findall(formula))
    for name, anions in _FAMILY_RULES:
        if elements & anions:
            return name
    return "Other"


def one_sided_binomial_p(successes, n):
    """Exact one-sided binomial test p-value for H0: p=0.5 vs H1: p>0.5.
    Stdlib-only (math.comb), no scipy dependency, matching this
    project's established convention (analyze_oqmd_coverage_gate.py,
    fetch_oqmd_coverage.py use no third-party libraries either)."""
    if n == 0:
        return None
    return sum(comb(n, k) for k in range(successes, n + 1)) / (2**n)


def doi_sort_key(dois):
    return min(dois) if dois else ""


def main():
    data = json.loads(INPUT_PATH.read_text())
    rows = data["rows"]
    balanced_rows = [r for r in rows if r["balanced"]]

    by_target = defaultdict(list)
    for r in balanced_rows:
        by_target[r["target_formula"]].append(r)

    qualifying = []
    ties = 0
    residual_doi_targets = defaultdict(set)  # doi -> set of targets it was selected for

    for target, target_rows in by_target.items():
        pure_rows = sorted(
            (r for r in target_rows if r["verdict"] == "pure"),
            key=lambda r: (doi_sort_key(r["dois"]), tuple(r["route"])),
        )
        impure_rows = sorted(
            (r for r in target_rows if r["verdict"] == "impure"),
            key=lambda r: (doi_sort_key(r["dois"]), tuple(r["route"])),
        )
        if not pure_rows or not impure_rows:
            continue
        pure_rep = pure_rows[0]
        impure_rep = impure_rows[0]
        if pure_rep["delta_ev_per_atom_t300"] is None or impure_rep["delta_ev_per_atom_t300"] is None:
            continue  # abstained at 300K -- shouldn't happen per the Rust run, but never assumed

        pure_doi = doi_sort_key(pure_rep["dois"])
        impure_doi = doi_sort_key(impure_rep["dois"])
        residual_doi_targets[pure_doi].add(target)
        residual_doi_targets[impure_doi].add(target)

        qualifying.append(
            {
                "target": target,
                "pure_route": pure_rep["route"],
                "pure_doi": pure_rep["dois"],
                "impure_route": impure_rep["route"],
                "impure_doi": impure_rep["dois"],
                "delta_t300_pure": pure_rep["delta_ev_per_atom_t300"],
                "delta_t300_impure": impure_rep["delta_ev_per_atom_t300"],
                "delta_t1800_pure": pure_rep["delta_ev_per_atom_t1800"],
                "delta_t1800_impure": impure_rep["delta_ev_per_atom_t1800"],
            }
        )

    n_qualifying = len(qualifying)

    if n_qualifying < MIN_QUALIFYING_TARGETS:
        result = {
            "verdict": "NO-GO",
            "reason": f"insufficient sample: {n_qualifying} qualifying targets, floor is {MIN_QUALIFYING_TARGETS}",
            "n_qualifying_targets": n_qualifying,
        }
        OUTPUT_PATH.write_text(json.dumps(result, indent=2, sort_keys=True))
        print(json.dumps(result, indent=2, sort_keys=True))
        return

    def score(temp_key_pure, temp_key_impure):
        correct = 0
        tie_count = 0
        n_scored = 0
        for q in qualifying:
            p = q[temp_key_pure]
            i = q[temp_key_impure]
            if p is None or i is None:
                continue
            n_scored += 1
            if p < i:
                correct += 1
            elif p == i:
                tie_count += 1
        return correct, tie_count, n_scored

    correct_300, ties_300, n_300 = score("delta_t300_pure", "delta_t300_impure")
    n_effective_300 = n_300 - ties_300
    accuracy_300 = correct_300 / n_effective_300 if n_effective_300 else None
    p_value_300 = one_sided_binomial_p(correct_300, n_effective_300) if n_effective_300 else None

    correct_1800, ties_1800, n_1800 = score("delta_t1800_pure", "delta_t1800_impure")
    n_effective_1800 = n_1800 - ties_1800
    accuracy_1800 = correct_1800 / n_effective_1800 if n_effective_1800 else None
    p_value_1800 = one_sided_binomial_p(correct_1800, n_effective_1800) if n_effective_1800 else None

    if p_value_300 is None or p_value_300 >= 0.05:
        verdict = "NO-GO"
    elif accuracy_300 >= 0.70 and p_value_300 < 0.01:
        verdict = "STRONG GO"
    else:
        verdict = "GO"

    # Secondary: full un-deduplicated pairwise accuracy (explicitly
    # over-counts non-independent comparisons -- descriptive only).
    full_pairwise_correct = 0
    full_pairwise_ties = 0
    full_pairwise_total = 0
    for target, target_rows in by_target.items():
        pure_vals = [
            r["delta_ev_per_atom_t300"]
            for r in target_rows
            if r["verdict"] == "pure" and r["delta_ev_per_atom_t300"] is not None
        ]
        impure_vals = [
            r["delta_ev_per_atom_t300"]
            for r in target_rows
            if r["verdict"] == "impure" and r["delta_ev_per_atom_t300"] is not None
        ]
        for pv in pure_vals:
            for iv in impure_vals:
                full_pairwise_total += 1
                if pv < iv:
                    full_pairwise_correct += 1
                elif pv == iv:
                    full_pairwise_ties += 1
    full_pairwise_effective = full_pairwise_total - full_pairwise_ties
    full_pairwise_accuracy = (
        full_pairwise_correct / full_pairwise_effective if full_pairwise_effective else None
    )

    residual_doi_overlap = {
        doi: sorted(targets) for doi, targets in residual_doi_targets.items() if len(targets) > 1
    }

    family_counts = Counter(classify_family(q["target"]) for q in qualifying)

    result = {
        "verdict": verdict,
        "hypothesis": (
            "within a target, the route with the more negative "
            "balanced_reaction_delta_ev_per_atom is more often the one labeled pure"
        ),
        "funnel": {
            "gate_passing_targets": 273,
            "flat_parseable_targets": 269,
            "balanced_rows": len(balanced_rows),
            "qualifying_targets": n_qualifying,
            "min_qualifying_targets_floor": MIN_QUALIFYING_TARGETS,
        },
        "primary_t300k": {
            "n_qualifying_targets": n_qualifying,
            "n_scored_after_ties": n_effective_300,
            "ties_excluded": ties_300,
            "correct": correct_300,
            "accuracy": accuracy_300,
            "one_sided_binomial_p_vs_0.5": p_value_300,
            "gate": {
                "NO-GO": "p >= 0.05",
                "GO": "p < 0.05 and accuracy < 0.70",
                "STRONG GO": "p < 0.01 and accuracy >= 0.70",
            },
        },
        "secondary_sensitivity_t1800k": {
            "n_scored_after_ties": n_effective_1800,
            "ties_excluded": ties_1800,
            "correct": correct_1800,
            "accuracy": accuracy_1800,
            "one_sided_binomial_p_vs_0.5": p_value_1800,
            "note": "descriptive only, not gating -- checks whether the verdict changes at "
            "gugen's upper validated temperature bound",
        },
        "secondary_full_pairwise_not_independence_corrected": {
            "total_comparisons": full_pairwise_total,
            "ties_excluded": full_pairwise_ties,
            "correct": full_pairwise_correct,
            "accuracy": full_pairwise_accuracy,
            "note": "over-counts non-independent comparisons (multiple rows can share a DOI or "
            "a target) -- descriptive only, never gating, reported for comparison against the "
            "primary per-target metric",
        },
        "residual_cross_target_doi_overlap": residual_doi_overlap,
        "chemical_family_distribution_of_qualifying_targets_informal_not_a_taxonomy": dict(
            family_counts
        ),
        "qualifying_target_detail": qualifying,
    }

    OUTPUT_PATH.write_text(json.dumps(result, indent=2, sort_keys=True))
    print(
        json.dumps(
            {k: v for k, v in result.items() if k != "qualifying_target_detail"},
            indent=2,
            sort_keys=True,
        )
    )
    print(f"\nVerdict: {verdict}. Wrote {OUTPUT_PATH}", file=sys.stderr)


if __name__ == "__main__":
    main()
