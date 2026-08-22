#!/usr/bin/env python3
"""Phase 21B condition 1: pre-registered coverage-gate and reporting-metric
computation. Run ONLY after a real, complete `benchmarks/fetch_oqmd_coverage.py`
run has produced `benchmarks/data/oqmd_coverage_manifest.json` (all 795
distinct formulas, no `--limit-formulas`). Pre-registered here, 2026-08-16,
before any such manifest exists -- same discipline as
docs/thermodynamic_selectivity_calibration.md §6.3's gate criterion, applied
one stage later so the *procedure* for scoring a real result can't be shaped
by having already seen it. Full context: same document's §6.5 (the
completion runbook this script implements step 4 of).

Computes report-only descriptive statistics plus the ONE pre-registered
gate (route-pair level, >=30 targets with >=2 fully-OQMD-covered,
outcome-disagreeing routes). Does NOT run any gugen src/ thermodynamic
function, does NOT compute a calibration, does NOT connect to
score_plan/RankingWeights/Planner ranking -- report-only, matching
condition 1's own scope exactly.

Deliberately does not judge coverage by the raw per-formula match rate
alone: §6.2.3 found that number can't be trusted as a gate proxy (a real
polymorph-selection bug made it read as an incorrect 15.1% before being
fixed). This script recomputes the actual pre-registered route-pair
criterion from the population + manifest directly.

Run:
  python3 benchmarks/analyze_oqmd_coverage_gate.py
Reads: benchmarks/data/thermodynamic_selectivity_clean_population.json,
       benchmarks/data/oqmd_coverage_manifest.json
Writes: benchmarks/data/oqmd_coverage_gate_result.json (small, committed)

Aborts (writes nothing) if the manifest doesn't exist, wasn't a complete
795-formula run, or is missing coverage data for any population
formula -- a partial or mismatched manifest must never be silently
scored as if it were the real, complete result.
"""

import json
import re
import sys
from collections import Counter
from pathlib import Path

DATA_DIR = Path(__file__).parent / "data"
POPULATION_PATH = DATA_DIR / "thermodynamic_selectivity_clean_population.json"
MANIFEST_PATH = DATA_DIR / "oqmd_coverage_manifest.json"
GATE_RESULT_PATH = DATA_DIR / "oqmd_coverage_gate_result.json"

# Pre-registered, docs/thermodynamic_selectivity_calibration.md §6.3: >=30
# targets must retain >=2 fully-OQMD-covered, gas-free, outcome-disagreeing
# routes. No other pre-registered gate exists.
GATE_TARGET_FLOOR = 30

# Informal, reporting-only chemical-family bucketing -- NOT a rigorous
# taxonomy and NEVER used for the gate itself. First matching rule wins,
# checked against crude element-symbol extraction (no stoichiometry or
# parenthesis parsing) over the target formula only.
_FAMILY_RULES = [
    ("Oxide", {"O"}),
    ("Sulfide/chalcogenide", {"S", "Se", "Te"}),
    ("Halide", {"F", "Cl", "Br", "I"}),
    ("Nitride", {"N"}),
    ("Phosphide/phosphate", {"P"}),
]
_ELEMENT_RE = re.compile(r"[A-Z][a-z]?")


class GateAnalysisError(RuntimeError):
    pass


def _elements_in_formula(formula):
    return set(_ELEMENT_RE.findall(formula))


def classify_family(formula):
    """Informal, reporting-only bucket -- see _FAMILY_RULES above. Not a
    chemical taxonomy: two compounds sharing an anion class can differ
    enormously in relevant chemistry. First matching rule wins; a
    formula matching none of the listed anions is "Other"."""
    elements = _elements_in_formula(formula)
    for name, anions in _FAMILY_RULES:
        if elements & anions:
            return name
    return "Other"


def species_covered(coverage, formula):
    entry = coverage.get(formula)
    return bool(entry and entry.get("matched"))


def compute_coverage_metrics(population, manifest):
    """Returns the full descriptive-statistics + gate report. See the
    module docstring and docs/thermodynamic_selectivity_calibration.md
    §6.3/§6.5 for what each field means and why it's defined this way.
    Raises GateAnalysisError if the manifest doesn't cover every species
    this population needs -- never silently scores a partial manifest."""
    coverage = manifest["coverage"]

    all_targets = set()
    all_precursors = set()
    all_species = set()
    for row in population:
        all_targets.add(row["target"])
        all_precursors.update(row["route"])
        all_species.add(row["target"])
        all_species.update(row["route"])

    missing = all_species - set(coverage.keys())
    if missing:
        raise GateAnalysisError(
            f"manifest is missing coverage data for {len(missing)} formula(s) this population "
            f"needs (e.g. {sorted(missing)[:5]}) -- refusing to score an incomplete manifest"
        )

    n_species_covered = sum(1 for s in all_species if species_covered(coverage, s))
    n_targets_covered = sum(1 for t in all_targets if species_covered(coverage, t))
    n_precursors_covered = sum(1 for p in all_precursors if species_covered(coverage, p))

    by_target = {}
    for row in population:
        target = row["target"]
        fully_covered = species_covered(coverage, target) and all(
            species_covered(coverage, f) for f in row["route"]
        )
        by_target.setdefault(target, []).append((fully_covered, row["verdict"]))

    n_fully_covered_routes = sum(1 for rows in by_target.values() for covered, _ in rows if covered)

    targets_with_ge2_covered_routes = 0
    outcome_disagreeing_targets = []
    n_pairwise_comparisons = 0
    for target, rows in by_target.items():
        covered_verdicts = [v for covered, v in rows if covered]
        if len(covered_verdicts) >= 2:
            targets_with_ge2_covered_routes += 1
        n_pure = sum(1 for v in covered_verdicts if v == "pure")
        n_impure = sum(1 for v in covered_verdicts if v == "impure")
        if n_pure >= 1 and n_impure >= 1:
            outcome_disagreeing_targets.append(target)
            n_pairwise_comparisons += n_pure * n_impure

    gate_pass_count = len(outcome_disagreeing_targets)
    gate_result = "GO" if gate_pass_count >= GATE_TARGET_FLOOR else "NO-GO"

    n_unmatched = 0
    n_null_energy_excluded_total = 0
    n_invalid_volume = 0
    n_multi_polymorph = 0
    for s in all_species:
        entry = coverage[s]
        n_null_energy_excluded_total += entry.get("n_null_energy_excluded", 0)
        if not entry.get("matched"):
            n_unmatched += 1
            continue
        volume = entry.get("volume_angstrom3_per_atom")
        if volume is None or volume <= 0:
            n_invalid_volume += 1
        n_preferred_valid = (
            entry.get("n_candidate_entries", 0)
            - entry.get("n_duplicate_excluded", 0)
            - entry.get("n_null_energy_excluded", 0)
        )
        if n_preferred_valid > 1:
            n_multi_polymorph += 1

    family_distribution = Counter(classify_family(t) for t in all_targets)

    return {
        "distinct_species_coverage": {
            "total": len(all_species),
            "covered": n_species_covered,
            "fraction": n_species_covered / len(all_species) if all_species else None,
        },
        "target_coverage": {
            "total": len(all_targets),
            "covered": n_targets_covered,
            "fraction": n_targets_covered / len(all_targets) if all_targets else None,
        },
        "precursor_coverage": {
            "total": len(all_precursors),
            "covered": n_precursors_covered,
            "fraction": n_precursors_covered / len(all_precursors) if all_precursors else None,
        },
        "fully_computable_routes": n_fully_covered_routes,
        "targets_with_ge2_computable_routes": targets_with_ge2_covered_routes,
        "outcome_disagreeing_comparable_targets": gate_pass_count,
        "independent_pairwise_comparisons": n_pairwise_comparisons,
        "diagnostics": {
            "unmatched_species": n_unmatched,
            "null_energy_excluded_entries_total": n_null_energy_excluded_total,
            "invalid_volume_matched_species": n_invalid_volume,
            "multi_polymorph_matched_species": n_multi_polymorph,
        },
        "chemical_family_distribution_informal_not_a_taxonomy": dict(family_distribution),
        "route_pair_gate": {
            "floor": GATE_TARGET_FLOOR,
            "passing_target_count": gate_pass_count,
            "result": gate_result,
            "passing_targets": sorted(outcome_disagreeing_targets),
        },
        "other_pre_registered_gates": "none beyond the route-pair floor above (docs/thermodynamic_selectivity_calibration.md §6.3)",
    }


def main():
    if not MANIFEST_PATH.exists():
        print(
            f"ABORTED: {MANIFEST_PATH} does not exist -- run a real, complete "
            "benchmarks/fetch_oqmd_coverage.py first (no --limit-formulas).",
            file=sys.stderr,
        )
        sys.exit(1)

    population = json.loads(POPULATION_PATH.read_text())
    manifest = json.loads(MANIFEST_PATH.read_text())

    expected_species = set()
    for row in population:
        expected_species.add(row["target"])
        expected_species.update(row["route"])
    queried = manifest.get("counts", {}).get("distinct_formulas_queried")
    if queried != len(expected_species):
        print(
            f"ABORTED: manifest reports {queried} formulas queried, population needs "
            f"{len(expected_species)} -- this looks like a partial or stale run, refusing to "
            "score it as if it were complete.",
            file=sys.stderr,
        )
        sys.exit(1)

    try:
        result = compute_coverage_metrics(population, manifest)
    except GateAnalysisError as e:
        print(f"ABORTED: {e}", file=sys.stderr)
        sys.exit(1)

    result_with_identity = {
        "source_manifest_checksum": manifest.get("coverage_snapshot_sha256"),
        **result,
    }
    GATE_RESULT_PATH.write_text(json.dumps(result_with_identity, indent=2, sort_keys=True))

    gate = result["route_pair_gate"]
    print(json.dumps(result_with_identity, indent=2, sort_keys=True))
    print(
        f"\nGate result: {gate['result']} ({gate['passing_target_count']}/{gate['floor']} floor). "
        f"Wrote {GATE_RESULT_PATH}",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
