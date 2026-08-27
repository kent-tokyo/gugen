#!/usr/bin/env python3
"""Self-check for parse_flat_formula.py.
Run: python3 benchmarks/test_parse_flat_formula.py -v
"""

import unittest

from parse_flat_formula import element_set, parse_flat_formula


class ParseFlatFormulaTests(unittest.TestCase):
    def test_simple_binary(self):
        self.assertEqual(parse_flat_formula("Bi2Te3"), {"Bi": 2.0, "Te": 3.0})

    def test_ternary(self):
        self.assertEqual(parse_flat_formula("CaTiO3"), {"Ca": 1.0, "Ti": 1.0, "O": 3.0})

    def test_elemental_single_atom_implicit_amount(self):
        self.assertEqual(parse_flat_formula("Ag"), {"Ag": 1.0})

    def test_decimal_subscript(self):
        self.assertEqual(parse_flat_formula("Fe0.9Ni0.1"), {"Fe": 0.9, "Ni": 0.1})

    def test_two_letter_symbols_are_not_split_into_singles(self):
        # "Ba" must parse as barium, not as B + a-shaped garbage
        self.assertEqual(parse_flat_formula("BaTiO3"), {"Ba": 1.0, "Ti": 1.0, "O": 3.0})

    def test_rejects_nested_parentheses(self):
        self.assertIsNone(parse_flat_formula("(PbS)1.18(TiS2)2"))
        self.assertIsNone(parse_flat_formula("(Ba0.6K0.4)Fe2As2"))

    def test_rejects_middle_dot_hydrate_separator(self):
        self.assertIsNone(parse_flat_formula("DyCl3·6H2O"))

    def test_rejects_dash_separated_flux_mixture(self):
        self.assertIsNone(parse_flat_formula("NaCl-KCl"))

    def test_rejects_repeated_element_rather_than_summing(self):
        # A duplication artifact (matches Phase 21A's own excluded pattern)
        # must never be silently summed into one amount.
        self.assertIsNone(parse_flat_formula("Ti3Ti"))

    def test_rejects_unknown_element_symbol(self):
        self.assertIsNone(parse_flat_formula("Xx2O3"))

    def test_rejects_empty_string(self):
        self.assertIsNone(parse_flat_formula(""))

    def test_rejects_zero_or_negative_amount(self):
        self.assertIsNone(parse_flat_formula("Ti0O2"))

    def test_element_set_helper(self):
        self.assertEqual(element_set({"Ti": 3.0, "Si": 1.0, "C": 2.0}), {"Ti", "Si", "C"})


if __name__ == "__main__":
    unittest.main()
