#!/usr/bin/env python3
"""Self-check for analyze_oqmd_coverage_gate.py's metric and gate logic.
Run: python3 benchmarks/test_analyze_oqmd_coverage_gate.py -v

All against synthetic population/manifest fixtures (no real
oqmd_coverage_manifest.json exists yet -- this is exactly the point of
pre-registering the gate script before real data exists). The one case
every scenario here must get right: a target with several fully-covered
routes that all agree on outcome must NOT count toward the gate --
that's the failure mode that would silently inflate a coverage-gate
result.
"""

import unittest

from analyze_oqmd_coverage_gate import GateAnalysisError, classify_family, compute_coverage_metrics


def _entry(matched=True, entry_id=1, delta_e=-1.0, volume=10.0, natoms=2, n_dup=0, n_null=0, spacegroup="Pm-3m"):
    n_candidates = 1 + n_dup + n_null
    return {
        "n_candidate_entries": n_candidates,
        "n_duplicate_excluded": n_dup,
        "n_null_energy_excluded": n_null,
        "matched": matched,
        "chosen_entry_id": entry_id if matched else None,
        "delta_e_ev_per_atom": delta_e if matched else None,
        "volume_angstrom3_per_atom": (volume / natoms) if matched else None,
        "spacegroup": spacegroup if matched else None,
    }


def _unmatched():
    return _entry(matched=False)


class CoverageMetricsTests(unittest.TestCase):
    def test_missing_species_raises(self):
        population = [{"target": "TiO2", "route": ["Ti", "O2"], "verdict": "pure"}]
        manifest = {"coverage": {"TiO2": _entry(), "Ti": _entry()}}  # O2 missing entirely
        with self.assertRaises(GateAnalysisError):
            compute_coverage_metrics(population, manifest)

    def test_basic_species_target_precursor_coverage(self):
        population = [
            {"target": "TiO2", "route": ["Ti", "O2"], "verdict": "pure"},
            {"target": "BaTiO3", "route": ["BaO", "TiO2"], "verdict": "pure"},
        ]
        manifest = {
            "coverage": {
                "TiO2": _entry(),
                "Ti": _entry(),
                "O2": _unmatched(),
                "BaTiO3": _unmatched(),
                "BaO": _entry(),
            }
        }
        result = compute_coverage_metrics(population, manifest)
        # species: TiO2, Ti, O2, BaTiO3, BaO = 5 total, 3 covered (TiO2, Ti, BaO)
        self.assertEqual(result["distinct_species_coverage"], {"total": 5, "covered": 3, "fraction": 0.6})
        # targets: TiO2, BaTiO3 = 2 total, 1 covered (TiO2)
        self.assertEqual(result["target_coverage"]["total"], 2)
        self.assertEqual(result["target_coverage"]["covered"], 1)
        # precursors: Ti, O2, BaO, TiO2 = 4 total, 3 covered (Ti, BaO, TiO2)
        self.assertEqual(result["precursor_coverage"]["total"], 4)
        self.assertEqual(result["precursor_coverage"]["covered"], 3)

    def test_all_agreeing_verdicts_do_not_count_toward_gate(self):
        # Three fully-covered routes for the same target, all "pure" --
        # this must NOT be an outcome-disagreeing comparable target.
        population = [
            {"target": "X", "route": ["A"], "verdict": "pure"},
            {"target": "X", "route": ["B"], "verdict": "pure"},
            {"target": "X", "route": ["C"], "verdict": "pure"},
        ]
        manifest = {"coverage": {n: _entry() for n in ["X", "A", "B", "C"]}}
        result = compute_coverage_metrics(population, manifest)
        self.assertEqual(result["targets_with_ge2_computable_routes"], 1)  # weaker count: still true
        self.assertEqual(result["outcome_disagreeing_comparable_targets"], 0)  # the actual gate metric
        self.assertEqual(result["independent_pairwise_comparisons"], 0)
        self.assertNotIn("X", result["route_pair_gate"]["passing_targets"])

    def test_disagreeing_verdicts_count_with_correct_pairwise_product(self):
        # 3 pure + 2 impure fully-covered routes -> 3*2 = 6 pairwise comparisons.
        population = (
            [{"target": "X", "route": [f"P{i}"], "verdict": "pure"} for i in range(3)]
            + [{"target": "X", "route": [f"I{i}"], "verdict": "impure"} for i in range(2)]
        )
        coverage = {"X": _entry()}
        for i in range(3):
            coverage[f"P{i}"] = _entry()
        for i in range(2):
            coverage[f"I{i}"] = _entry()
        result = compute_coverage_metrics(population, {"coverage": coverage})
        self.assertEqual(result["outcome_disagreeing_comparable_targets"], 1)
        self.assertEqual(result["independent_pairwise_comparisons"], 6)
        self.assertIn("X", result["route_pair_gate"]["passing_targets"])

    def test_route_not_fully_covered_is_excluded(self):
        # Target covered, but one route's precursor is unmatched -> not fully computable.
        population = [
            {"target": "X", "route": ["A"], "verdict": "pure"},
            {"target": "X", "route": ["B"], "verdict": "impure"},
        ]
        manifest = {"coverage": {"X": _entry(), "A": _entry(), "B": _unmatched()}}
        result = compute_coverage_metrics(population, manifest)
        self.assertEqual(result["fully_computable_routes"], 1)
        self.assertEqual(result["outcome_disagreeing_comparable_targets"], 0)  # only 1 covered route

    def test_gate_go_at_floor(self):
        population = []
        coverage = {}
        for t in range(30):
            name = f"T{t}"
            population.append({"target": name, "route": [f"{name}_pure"], "verdict": "pure"})
            population.append({"target": name, "route": [f"{name}_impure"], "verdict": "impure"})
            coverage[name] = _entry()
            coverage[f"{name}_pure"] = _entry()
            coverage[f"{name}_impure"] = _entry()
        result = compute_coverage_metrics(population, {"coverage": coverage})
        self.assertEqual(result["route_pair_gate"]["result"], "GO")
        self.assertEqual(result["route_pair_gate"]["passing_target_count"], 30)

    def test_gate_no_go_below_floor(self):
        population = []
        coverage = {}
        for t in range(29):
            name = f"T{t}"
            population.append({"target": name, "route": [f"{name}_pure"], "verdict": "pure"})
            population.append({"target": name, "route": [f"{name}_impure"], "verdict": "impure"})
            coverage[name] = _entry()
            coverage[f"{name}_pure"] = _entry()
            coverage[f"{name}_impure"] = _entry()
        result = compute_coverage_metrics(population, {"coverage": coverage})
        self.assertEqual(result["route_pair_gate"]["result"], "NO-GO")
        self.assertEqual(result["route_pair_gate"]["passing_target_count"], 29)

    def test_diagnostics_unmatched_invalid_volume_multi_polymorph(self):
        population = [{"target": "X", "route": ["A", "B", "C"], "verdict": "pure"}]
        manifest = {
            "coverage": {
                "X": _entry(),
                "A": _unmatched(),
                "B": _entry(n_dup=2, n_null=1),  # n_preferred_valid = 1+2+1 - 2 - 1 = 1 -> not multi
                "C": _entry(n_dup=0, n_null=0, volume=0.0),  # matched but invalid (zero) volume
            }
        }
        result = compute_coverage_metrics(population, manifest)
        diag = result["diagnostics"]
        self.assertEqual(diag["unmatched_species"], 1)  # A
        self.assertEqual(diag["invalid_volume_matched_species"], 1)  # C (volume 0.0)
        self.assertEqual(diag["multi_polymorph_matched_species"], 0)

    def test_multi_polymorph_counts_when_multiple_preferred_valid_candidates(self):
        # n_candidate_entries=4, n_dup=1, n_null=0 -> n_preferred_valid = 4-1-0=3 > 1
        entry = _entry()
        entry["n_candidate_entries"] = 4
        entry["n_duplicate_excluded"] = 1
        entry["n_null_energy_excluded"] = 0
        population = [{"target": "X", "route": [], "verdict": "pure"}]
        result = compute_coverage_metrics(population, {"coverage": {"X": entry}})
        self.assertEqual(result["diagnostics"]["multi_polymorph_matched_species"], 1)


class ClassifyFamilyTests(unittest.TestCase):
    def test_oxide(self):
        self.assertEqual(classify_family("TiO2"), "Oxide")

    def test_sulfide(self):
        self.assertEqual(classify_family("FeS2"), "Sulfide/chalcogenide")

    def test_halide(self):
        self.assertEqual(classify_family("NaCl"), "Halide")

    def test_nitride(self):
        self.assertEqual(classify_family("Si3N4"), "Nitride")

    def test_phosphide(self):
        self.assertEqual(classify_family("Ca3P2"), "Phosphide/phosphate")

    def test_other_when_no_recognized_anion(self):
        self.assertEqual(classify_family("W"), "Other")

    def test_oxide_takes_priority_over_later_rules(self):
        # Contains both O and N -- oxide rule is checked first.
        self.assertEqual(classify_family("TiON"), "Oxide")


if __name__ == "__main__":
    unittest.main()
