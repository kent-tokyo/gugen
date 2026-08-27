#!/usr/bin/env python3
"""Self-check for formula_shape_taxonomy.py and
build_phase32_qualification_input.py's classification logic.
Run: python3 benchmarks/test_phase32_qualification.py -v
"""

import unittest

from build_phase32_qualification_input import classify_pre_balance
from formula_shape_taxonomy import classify_formula_shape


class FormulaShapeTaxonomyTests(unittest.TestCase):
    def test_flat(self):
        bucket, _ = classify_formula_shape("BaTiO3")
        self.assertEqual(bucket, "flat")

    def test_flat_with_decimal_occupancy_flagged(self):
        bucket, detail = classify_formula_shape("Fe0.9Ni0.1")
        self.assertEqual(bucket, "flat")
        self.assertTrue(detail["has_decimal_occupancy"])

    def test_phase_prefix_stripped(self):
        bucket, detail = classify_formula_shape("α-Fe2O3")
        self.assertEqual(bucket, "phase_affix")
        self.assertEqual(detail["remainder"], "Fe2O3")

    def test_phase_suffix_stripped(self):
        bucket, detail = classify_formula_shape("TiO2(s)")
        self.assertEqual(bucket, "phase_affix")
        self.assertEqual(detail["remainder"], "TiO2")

    def test_nested_parentheses(self):
        bucket, detail = classify_formula_shape("((K0.5Na0.5)0.94Li0.06)0.97La0.01Nb0.9O3")
        self.assertEqual(bucket, "nested_parentheses")
        self.assertGreaterEqual(detail["max_depth"], 2)

    def test_shallow_parentheses(self):
        bucket, detail = classify_formula_shape("Fe(NO3)3")
        self.assertEqual(bucket, "parentheses")
        self.assertEqual(detail["max_depth"], 1)

    def test_hydrate_dot_no_parens(self):
        bucket, _ = classify_formula_shape("DyCl3·6H2O")
        self.assertEqual(bucket, "hydrate_dot")

    def test_hydrate_with_parens_is_parens_not_hydrate(self):
        # Al(NO3)3.9H2O has both a paren AND a hydrate-looking dot --
        # parens takes priority (matches Phase 21B's own "24 nested"
        # bucket, which already included this exact shape).
        bucket, _ = classify_formula_shape("Al(NO3)3.9H2O")
        self.assertEqual(bucket, "parentheses")

    def test_dash_separated_mixture_is_malformed(self):
        bucket, detail = classify_formula_shape("NaCl-KCl")
        self.assertEqual(bucket, "malformed")
        self.assertEqual(detail["reason"], "dash-separated mixture")

    def test_nonstoichiometry_suffix_flagged_not_recoverable(self):
        bucket, detail = classify_formula_shape("BaZr0.8Y0.2O3-δ")
        self.assertEqual(bucket, "malformed")
        self.assertIn("not recoverable", detail["reason"])

    def test_repeated_element_flagged_distinctly(self):
        bucket, detail = classify_formula_shape("NH4H2PO4")
        self.assertEqual(bucket, "malformed")
        self.assertIn("repeated element", detail["reason"])

    def test_acronym_is_malformed_unrecognized(self):
        bucket, detail = classify_formula_shape("PVA")
        self.assertEqual(bucket, "malformed")
        self.assertIn("unrecognized shape", detail["reason"])

    def test_empty_string(self):
        bucket, _ = classify_formula_shape("")
        self.assertEqual(bucket, "malformed")


class ClassifyPreBalanceTests(unittest.TestCase):
    def _row(self, target_elements, route_elements):
        return {"target_elements": target_elements, "route_elements": route_elements, "route_formulas": []}

    def test_formula_unsupported_when_target_unparseable(self):
        result = classify_pre_balance(self._row(None, [{"O": 2.0}]))
        self.assertEqual(result["status"], "FormulaUnsupported")

    def test_formula_unsupported_when_a_precursor_unparseable(self):
        result = classify_pre_balance(self._row({"Ti": 1.0, "O": 2.0}, [{"Ti": 1.0}, None]))
        self.assertEqual(result["status"], "FormulaUnsupported")

    def test_hard_mismatch_when_missing_element_is_major(self):
        # target is 50% Y by site fraction, absent from every precursor
        target = {"Y": 1.0, "O": 1.0}
        route = [{"Ba": 1.0, "O": 1.0}]
        result = classify_pre_balance(self._row(target, route))
        self.assertEqual(result["status"], "TargetPrecursorElementMismatch")

    def test_dopant_ambiguous_when_missing_element_is_minor(self):
        # target is a small-fraction dopant absent from the route --
        # Ca0.76Sr0.1Na0.07Eu0.01La0.02Nd0.02Pr0.02MoO4-style case
        target = {"Ca": 0.9, "Nd": 0.02, "Mo": 1.0, "O": 4.0}
        route = [{"Ca": 1.0, "O": 1.0}, {"Mo": 1.0, "O": 3.0}]
        result = classify_pre_balance(self._row(target, route))
        self.assertEqual(result["status"], "DopantHostAmbiguous")

    def test_dopant_ambiguous_when_route_has_extra_element_outside_allowlist(self):
        target = {"Sr": 1.0, "Ti": 1.0, "O": 3.0}
        route = [{"Ti": 1.0, "O": 2.0}, {"Sr": 1.0, "N": 2.0, "O": 6.0}]
        result = classify_pre_balance(self._row(target, route))
        self.assertEqual(result["status"], "DopantHostAmbiguous")
        self.assertIn("N", result["detail"]["extra_elements"])

    def test_needs_balance_when_element_sets_match_exactly(self):
        # O2 is still offered (oxygen participates), but no foreign
        # element is present so CO2/H2O never are.
        target = {"Ba": 1.0, "Zr": 1.0, "O": 3.0}
        route = [{"Ba": 1.0, "O": 1.0}, {"Zr": 1.0, "O": 2.0}]
        result = classify_pre_balance(self._row(target, route))
        self.assertEqual(result["stage"], "needs_balance")
        self.assertEqual(result["byproduct_candidates"], ["O2"])

    def test_co2_offered_when_route_carries_extra_carbon(self):
        target = {"Ba": 1.0, "Zr": 1.0, "O": 3.0}
        route = [{"Ba": 1.0, "C": 1.0, "O": 3.0}, {"Zr": 1.0, "O": 2.0}]
        result = classify_pre_balance(self._row(target, route))
        self.assertEqual(result["stage"], "needs_balance")
        self.assertIn("CO2", result["byproduct_candidates"])

    def test_o2_offered_even_when_element_sets_match_exactly(self):
        # No "extra" element at all (pure oxide redox case, e.g.
        # 2 Mn2O3 -> 4 MnO + O2) -- O2 must still be offered since it
        # introduces no foreign element, unlike CO2/H2O.
        target = {"Mn": 1.0, "O": 1.0}
        route = [{"Mn": 2.0, "O": 3.0}]
        result = classify_pre_balance(self._row(target, route))
        self.assertEqual(result["stage"], "needs_balance")
        self.assertIn("O2", result["byproduct_candidates"])
        self.assertNotIn("CO2", result["byproduct_candidates"])
        self.assertNotIn("H2O", result["byproduct_candidates"])

    def test_h2o_offered_when_route_carries_extra_hydrogen(self):
        target = {"Ba": 1.0, "Ti": 1.0, "O": 3.0}
        route = [{"Ba": 1.0, "H": 2.0, "O": 2.0}, {"Ti": 1.0, "O": 2.0}]
        result = classify_pre_balance(self._row(target, route))
        self.assertIn("H2O", result["byproduct_candidates"])
        self.assertNotIn("CO2", result["byproduct_candidates"])


if __name__ == "__main__":
    unittest.main()
