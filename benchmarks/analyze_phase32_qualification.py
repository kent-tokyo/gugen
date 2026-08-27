#!/usr/bin/env python3
"""Phase 32, Sections 1/6/7/8: merges the Python pre-classification
pass and the real `balance()` results into final `ReactionRecordStatus`
records, computes the corpus funnel (Section 6), re-derives Phase
21B's own exact 1285-row/347-balanced population as a regression check
(the GO gate for this layer requires the existing 347 not be lost),
and draws the Section 7 manual-audit sample.

Run: python3 benchmarks/analyze_phase32_qualification.py
(after build_phase32_qualification_input.py and
`cargo run --release --example exploration_phase32_reaction_qualification --features serde`)
Writes: benchmarks/data/phase32_qualification_result.json
"""

import json
from pathlib import Path

DATA_DIR = Path(__file__).parent / "data"

STATUSES = [
    "BalancedAsDeclared",
    "BalanceableWithConservativeByproductCompletion",
    "FormulaUnsupported",
    "TargetPrecursorElementMismatch",
    "DopantHostAmbiguous",
    "MissingOrZeroCoefficientPrecursor",
    "Unbalanceable",
]

CONFIDENCE_CLASS = {
    "BalancedAsDeclared": "high",
    "FormulaUnsupported": "high",
    "TargetPrecursorElementMismatch": "high",
    "MissingOrZeroCoefficientPrecursor": "high",
    "Unbalanceable": "high",
    "DopantHostAmbiguous": "medium",  # threshold heuristic, disclosed
    "BalanceableWithConservativeByproductCompletion": "medium",  # inferred, not original data
}


def decide_status(row, balance_result):
    if row["stage"] == "terminal":
        return row["status"], row["reason_codes"], row.get("detail"), None

    outcome = balance_result["as_declared_outcome"]
    successful = balance_result["successful_byproduct_candidates"]

    # Section 7 manual audit found every single-precursor-route O2
    # completion in the sample suspicious (a spontaneous redox release
    # with no reducing agent, a formula that looks like a concatenation
    # artifact, and a fractional-notation formula) -- none outright
    # confirmed wrong, but per Section 8's rule ("if even ONE false
    # completion is found for a family, downgrade that family to
    # diagnostic-only"), single-precursor routes are excluded from
    # auto-completion entirely, for every byproduct, not just O2.
    # docs/phase32_reaction_record_qualification.md Section 7 has the
    # 3 flagged examples.
    if len(row["route_formulas"]) == 1 and successful:
        successful = []

    if outcome == "all_positive":
        return "BalancedAsDeclared", ["target_and_every_precursor_positive_no_byproduct_needed"], None, None
    if len(successful) == 1:
        return (
            "BalanceableWithConservativeByproductCompletion",
            [f"unique_conservative_completion:{successful[0]}"],
            None,
            successful[0],
        )
    if len(successful) > 1:
        return (
            "Unbalanceable",
            [f"byproduct_completion_ambiguous_multiple_candidates:{','.join(successful)}"],
            {"ambiguous_candidates": successful},
            None,
        )
    if outcome == "solution_not_all_positive":
        return "MissingOrZeroCoefficientPrecursor", ["balance_found_but_a_declared_precursor_dropped_to_zero"], None, None
    return "Unbalanceable", ["no_balance_found"], None, None


def build_qualified_records():
    input_rows = json.loads((DATA_DIR / "phase32_qualification_input.json").read_text())
    balance_results = {
        r["row_id"]: r
        for r in json.loads((DATA_DIR / "exploration_phase32_reaction_qualification_result.json").read_text())
    }

    records = []
    for row in input_rows:
        balance_result = balance_results.get(row["row_id"])
        status, reason_codes, detail, inferred_byproduct = decide_status(row, balance_result)
        equation = None
        if status in ("BalancedAsDeclared", "BalanceableWithConservativeByproductCompletion") and balance_result:
            equation = balance_result.get("balanced_equation")
        records.append(
            {
                "row_id": row["row_id"],
                "corpus": row["corpus"],
                "doi": row["doi"],
                "target_formula": row["target_formula"],
                "target_elements": row["target_elements"],
                "route_formulas": row["route_formulas"],
                "route_elements": row["route_elements"],
                "verdict": row["verdict"],
                "declared_byproducts": [],  # neither source corpus declares byproducts explicitly
                "inferred_byproduct": inferred_byproduct,  # never conflated with declared_byproducts
                "balanced_equation": equation,
                "status": status,
                "reason_codes": reason_codes,
                "detail": detail,
                "confidence_class": CONFIDENCE_CLASS[status],
                "provenance": "phase32_qualification_input.json + exploration_phase32_reaction_qualification.rs"
                if row["stage"] == "needs_balance"
                else "phase32_qualification_input.json (pre-balance classification, no balance() call needed)",
            }
        )
    return records


def reconstruct_phase21b_1285_row_ids():
    """Re-derives Phase 21B's own exact 1285-row population (condition
    1's 273 gate-passing targets, flat-parseable, OQMD-covered),
    preserving each row's original index into
    thermodynamic_selectivity_clean_population.json, so it can be
    matched 1:1 against this phase's `thermodynamic_selectivity:{i}`
    row ids -- a regression check, not a re-measurement."""
    import sys

    sys.path.insert(0, str(Path(__file__).parent))
    from parse_flat_formula import parse_flat_formula  # noqa: E402

    gate = json.loads((DATA_DIR / "oqmd_coverage_gate_result.json").read_text())
    passing = set(gate["route_pair_gate"]["passing_targets"])
    population = json.loads((DATA_DIR / "thermodynamic_selectivity_clean_population.json").read_text())
    manifest = json.loads((DATA_DIR / "oqmd_coverage_manifest.json").read_text())
    coverage = manifest["coverage"]

    def is_covered(formula):
        entry = coverage.get(formula)
        return bool(entry and entry.get("matched"))

    row_ids = []
    for i, r in enumerate(population):
        if r["target"] not in passing:
            continue
        target_parsed = parse_flat_formula(r["target"])
        route_parsed = [parse_flat_formula(f) for f in r["route"]]
        if target_parsed is None or any(p is None for p in route_parsed):
            continue
        all_formulas = [r["target"]] + list(r["route"])
        if not all(is_covered(f) for f in all_formulas):
            continue
        row_ids.append(f"thermodynamic_selectivity:{i}")
    return row_ids


def compute_corpus_funnel(records, corpus):
    rows = [r for r in records if r["corpus"] == corpus]
    total = len(rows)

    def count(status):
        return sum(1 for r in rows if r["status"] == status)

    parseable = total - count("FormulaUnsupported")
    balanced_as_declared = count("BalancedAsDeclared")
    balanceable_after_completion = count("BalanceableWithConservativeByproductCompletion")

    funnel = {
        "total_rows": total,
        "parseable_rows": parseable,
        "balanced_as_declared": balanced_as_declared,
        "balanceable_after_conservative_completion": balanceable_after_completion,
        "balanced_or_completable_total": balanced_as_declared + balanceable_after_completion,
        "formula_unsupported": count("FormulaUnsupported"),
        "dopant_host_ambiguous": count("DopantHostAmbiguous"),
        "unbalanceable": count("Unbalanceable"),
        "target_missing": 0,  # neither corpus has an empty/missing target field
        "precursor_dropped_to_zero": count("MissingOrZeroCoefficientPrecursor"),
    }

    if corpus == "thermodynamic_selectivity":
        manifest = json.loads((DATA_DIR / "oqmd_coverage_manifest.json").read_text())
        coverage = manifest["coverage"]

        def all_covered(r):
            formulas = [r["target_formula"]] + r["route_formulas"]
            return all(coverage.get(f, {}).get("matched") for f in formulas)

        oqmd_covered = [r for r in rows if all_covered(r)]
        funnel["oqmd_covered"] = len(oqmd_covered)
        funnel["oqmd_covered_and_balanceable"] = sum(
            1 for r in oqmd_covered if r["status"] == "BalancedAsDeclared"
        )

        by_target = {}
        for r in rows:
            if r["status"] != "BalancedAsDeclared" or r["verdict"] is None:
                continue
            by_target.setdefault(r["target_formula"], set()).add(r["verdict"])
        pairable_targets = sum(1 for verdicts in by_target.values() if {"pure", "impure"}.issubset(verdicts))
        funnel["independent_targets_with_pure_impure_pair"] = pairable_targets
    else:
        funnel["oqmd_covered"] = "N/A -- Kononova was never queried against OQMD"
        funnel["oqmd_covered_and_balanceable"] = "N/A"
        funnel["independent_targets_with_pure_impure_pair"] = (
            "N/A -- Kononova has no pure/impure verdict field"
        )

    return funnel


def draw_manual_audit_sample(records):
    """Section 7: >=10 per major status, >=50 total, deterministic
    (sorted by row_id, not random) so the sample is reproducible."""
    sample = []
    by_status = {}
    for r in records:
        by_status.setdefault(r["status"], []).append(r)
    for status in STATUSES:
        rows = sorted(by_status.get(status, []), key=lambda r: r["row_id"])
        take = rows[:10]
        sample.extend(take)
    return sample


def main():
    records = build_qualified_records()

    phase21b_row_ids = set(reconstruct_phase21b_1285_row_ids())
    phase21b_records = [r for r in records if r["row_id"] in phase21b_row_ids]
    phase21b_regression = {
        "expected_row_count": 1285,
        "actual_row_count": len(phase21b_records),
        "expected_balanced_as_declared": 347,
        "actual_balanced_as_declared": sum(1 for r in phase21b_records if r["status"] == "BalancedAsDeclared"),
    }
    phase21b_regression["row_count_matches"] = (
        phase21b_regression["actual_row_count"] == phase21b_regression["expected_row_count"]
    )
    phase21b_regression["balanced_count_matches"] = (
        phase21b_regression["actual_balanced_as_declared"] == phase21b_regression["expected_balanced_as_declared"]
    )

    funnel = {
        "kononova": compute_corpus_funnel(records, "kononova"),
        "thermodynamic_selectivity": compute_corpus_funnel(records, "thermodynamic_selectivity"),
    }

    manual_audit_sample = draw_manual_audit_sample(records)

    status_counts_overall = {s: sum(1 for r in records if r["status"] == s) for s in STATUSES}

    output = {
        "description": (
            "Phase 32: Reaction Record Qualification & Corpus Integrity. "
            "Every row from both corpora classified into one of 7 "
            "ReactionRecordStatus values. See docs/phase32_reaction_record_qualification.md."
        ),
        "total_records": len(records),
        "status_counts_overall": status_counts_overall,
        "corpus_funnel": funnel,
        "phase21b_baseline_regression_check": phase21b_regression,
        "manual_audit_sample_row_ids": [r["row_id"] for r in manual_audit_sample],
    }

    out_path = DATA_DIR / "phase32_qualification_result.json"
    with open(out_path, "w") as f:
        json.dump(output, f, indent=2, sort_keys=True)

    records_out_path = DATA_DIR / "phase32_qualified_records.json"
    with open(records_out_path, "w") as f:
        json.dump(records, f)

    print("=== Phase 32 qualification result ===")
    print(f"total_records={len(records)}")
    print("status_counts_overall:")
    for s, c in status_counts_overall.items():
        print(f"  {s}: {c}")
    print("\nphase21b_baseline_regression_check:")
    print(json.dumps(phase21b_regression, indent=2))
    print("\ncorpus_funnel:")
    print(json.dumps(funnel, indent=2))
    print(f"\nWrote {out_path}")
    print(f"Wrote {records_out_path}")


if __name__ == "__main__":
    main()
