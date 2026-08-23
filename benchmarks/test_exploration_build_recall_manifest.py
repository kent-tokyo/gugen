#!/usr/bin/env python3
"""Self-check for exploration_build_recall_manifest.py's pure logic.
Run: python3 benchmarks/test_exploration_build_recall_manifest.py -v

All against synthetic fixtures -- no real dataset file is read. The one
case every scenario here must get right: a row whose formulas can't all
be resolved to real amounts must never be silently treated as clean
(see test_leakage_check_never_excludes_an_unresolvable_row) or silently
dropped from the manifest (see test_filter_leakage_keeps_unresolvable_rows).
"""

import unittest

from exploration_build_recall_manifest import (
    chemical_system,
    classify_family,
    filter_leakage,
    is_excluded,
    is_fully_resolved,
    merge_rows,
    parse_formula_amounts,
    reduced_formula_signature,
)


def _row(target_formula, target_amounts, route, route_amounts, dois, source_dataset):
    return {
        "target_formula": target_formula,
        "target_amounts": target_amounts,
        "route": route,
        "route_amounts": route_amounts,
        "dois": dois,
        "chemical_system": chemical_system((target_amounts or {}).keys()),
        "material_family": classify_family(target_formula),
        "reduced_formula": reduced_formula_signature(target_amounts),
        "reduced_formula_unavailable_reason": None if target_amounts else "unparseable",
        "source_dataset": source_dataset,
    }


class FormulaParsingTests(unittest.TestCase):
    def test_simple_formula(self):
        self.assertEqual(parse_formula_amounts("BaTiO3"), {"Ba": 1.0, "Ti": 1.0, "O": 3.0})

    def test_one_level_parenthesized_formula(self):
        self.assertEqual(
            parse_formula_amounts("Sc2(MoO4)3"), {"Sc": 2.0, "Mo": 3.0, "O": 12.0}
        )

    def test_two_separate_parenthesized_groups(self):
        self.assertEqual(
            parse_formula_amounts("(NH4)2Ce(NO3)6"),
            {"N": 8.0, "H": 8.0, "Ce": 1.0, "O": 18.0},
        )

    def test_hydrate_notation_is_unparseable(self):
        self.assertIsNone(parse_formula_amounts("Mg(NO3)2·6H2O"))

    def test_polymorph_prefix_is_unparseable(self):
        self.assertIsNone(parse_formula_amounts("γ-Al2O3"))

    def test_empty_string_is_unparseable(self):
        self.assertIsNone(parse_formula_amounts(""))


class ClassificationTests(unittest.TestCase):
    def test_oxide_family(self):
        self.assertEqual(classify_family("BaTiO3"), "Oxide")

    def test_halide_family(self):
        self.assertEqual(classify_family("NaCl"), "Halide")

    def test_other_family_when_no_known_anion(self):
        self.assertEqual(classify_family("Fe3Al"), "Other")

    def test_chemical_system_is_sorted_and_hyphenated(self):
        self.assertEqual(chemical_system(["Ti", "Ba", "O"]), "Ba-O-Ti")


class ReducedFormulaTests(unittest.TestCase):
    def test_scale_invariant(self):
        a = reduced_formula_signature({"Ba": 1.0, "Ti": 1.0, "O": 3.0})
        b = reduced_formula_signature({"Ba": 2.0, "Ti": 2.0, "O": 6.0})
        self.assertEqual(a, b)

    def test_none_on_empty_input(self):
        self.assertIsNone(reduced_formula_signature(None))
        self.assertIsNone(reduced_formula_signature({}))


class LeakageExclusionTests(unittest.TestCase):
    def test_the_excluded_batio3_route_is_excluded(self):
        row = _row(
            "BaTiO3",
            {"Ba": 1.0, "Ti": 1.0, "O": 3.0},
            ["BaCO3", "TiO2"],
            [{"Ba": 1.0, "C": 1.0, "O": 3.0}, {"Ti": 1.0, "O": 2.0}],
            ["10.example/leaked"],
            "kononova",
        )
        self.assertTrue(is_fully_resolved(row))
        self.assertTrue(is_excluded(row))

    def test_a_different_route_to_the_same_excluded_target_is_not_excluded(self):
        # Real Phase 28 finding: MgAl2O4 via Mg(OH)2 + gamma-Al2O3 is a
        # genuinely different route from the excluded MgO + Al2O3 one --
        # target-name-only matching would wrongly flag this.
        row = _row(
            "MgAl2O4",
            {"Mg": 1.0, "Al": 2.0, "O": 4.0},
            ["Al2O3", "Mg(OH)2"],
            [{"Al": 2.0, "O": 3.0}, {"Mg": 1.0, "O": 2.0, "H": 2.0}],
            ["10.example/not-leaked"],
            "kononova",
        )
        self.assertTrue(is_fully_resolved(row))
        self.assertFalse(is_excluded(row))

    def test_an_unresolvable_row_is_never_treated_as_excluded(self):
        row = _row(
            "MgAl2O4",
            {"Mg": 1.0, "Al": 2.0, "O": 4.0},
            ["Al2O3", "gamma-Al2O3-not-a-real-formula"],
            [{"Al": 2.0, "O": 3.0}, None],
            ["10.example/unresolvable"],
            "kononova",
        )
        self.assertFalse(is_fully_resolved(row))
        self.assertFalse(is_excluded(row))

    def test_filter_leakage_keeps_unresolvable_rows_but_counts_them(self):
        resolvable_clean = _row(
            "Fe2O3", {"Fe": 2.0, "O": 3.0}, ["Fe"], [{"Fe": 1.0}], ["d1"], "kononova"
        )
        unresolvable = _row(
            "MgAl2O4",
            {"Mg": 1.0, "Al": 2.0, "O": 4.0},
            ["x"],
            [None],
            ["d2"],
            "kononova",
        )
        excluded = _row(
            "BaTiO3",
            {"Ba": 1.0, "Ti": 1.0, "O": 3.0},
            ["BaCO3", "TiO2"],
            [{"Ba": 1.0, "C": 1.0, "O": 3.0}, {"Ti": 1.0, "O": 2.0}],
            ["d3"],
            "kononova",
        )
        kept, excluded_count, unchecked_count = filter_leakage(
            [resolvable_clean, unresolvable, excluded]
        )
        self.assertEqual([r["target_formula"] for r in kept], ["Fe2O3", "MgAl2O4"])
        self.assertEqual(excluded_count, 1)
        self.assertEqual(unchecked_count, 1)


class MergeRowsTests(unittest.TestCase):
    def test_same_target_route_pair_from_two_sources_merges_dois_and_sources(self):
        a = _row(
            "TiO2", {"Ti": 1.0, "O": 2.0}, ["Ti"], [{"Ti": 1.0}], ["d1"], "kononova"
        )
        b = _row(
            "TiO2",
            {"Ti": 1.0, "O": 2.0},
            ["Ti"],
            [{"Ti": 1.0}],
            ["d2"],
            "lee_thermodynamic_selectivity_2025",
        )
        merged = merge_rows([a, b])
        self.assertEqual(len(merged), 1)
        self.assertEqual(merged[0]["dois"], ["d1", "d2"])
        self.assertEqual(
            merged[0]["source_datasets"], ["kononova", "lee_thermodynamic_selectivity_2025"]
        )

    def test_a_route_cited_by_many_records_counts_once(self):
        rows = [
            _row("BaTiO3", {"Ba": 1.0, "Ti": 1.0, "O": 3.0}, ["BaO", "TiO2"], [{}, {}], [f"d{i}"], "kononova")
            for i in range(10)
        ]
        merged = merge_rows(rows)
        self.assertEqual(len(merged), 1)
        self.assertEqual(len(merged[0]["dois"]), 10)

    def test_a_null_amounts_side_does_not_clobber_a_resolved_side(self):
        resolved = _row(
            "Fe2O3", {"Fe": 2.0, "O": 3.0}, ["Fe"], [{"Fe": 1.0}], ["d1"], "kononova"
        )
        unresolved = _row(
            "Fe2O3", None, ["Fe"], [{"Fe": 1.0}], ["d2"], "lee_thermodynamic_selectivity_2025"
        )
        merged = merge_rows([unresolved, resolved])
        self.assertEqual(len(merged), 1)
        self.assertIsNotNone(merged[0]["target_amounts"])
        self.assertIsNotNone(merged[0]["reduced_formula"])


if __name__ == "__main__":
    unittest.main()
