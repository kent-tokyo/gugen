#!/usr/bin/env python3
"""Phase 21B calibration: a deliberately narrow chemical-formula parser.

gugen has no general formula parser anywhere in the crate (every other
corpus this project uses -- Kononova, the commercial catalog -- ships
pre-parsed per-element amounts; the calibration is the first time this
project needs to turn a bare formula *string*, e.g. "Ti3SiC2", into
element:amount pairs). Rather than build a general parser (nested
parentheses with multipliers, hydrate dots, etc.), a survey of the 273
Phase 21B gate-passing targets' own formulas found a flat parser --
element symbols each followed by an optional decimal amount, no
grouping -- covers 269/273 targets (98.5%) and 2351/2357 independent
pairwise comparisons (99.7%). The remaining 24 nested-parenthesis
formulas (mostly hydrate/nitrate precursors, e.g. "Al(NO3)3.9H2O",
"(PbS)1.18(TiS2)2") and 5 middle-dot-hydrate formulas are excluded, not
guessed at -- building recursive nested-parenthesis parsing for a 1.5%
target-count gain was judged not worth the added correctness risk in a
brand-new, previously-nonexistent parsing capability.

**Never a best-effort guess**: anything this parser can't parse exactly
returns `None` and must be counted as an exclusion by the caller, same
discipline as `Frac::from_f64` returning `None` rather than rounding
(src/frac.rs).

Run as a module (`from parse_flat_formula import parse_flat_formula`)
or standalone for a smoke check: `python3 benchmarks/parse_flat_formula.py`.
"""

import re
from collections import defaultdict

# One element symbol (capital + optional lowercase) followed by an
# optional amount (int or decimal, no exponents, no negative signs --
# real formula subscripts are never negative or exponential).
_TOKEN_RE = re.compile(r"([A-Z][a-z]?)(\d+\.?\d*)?")

# Guards against silently accepting a string this regex would partially
# match (e.g. leftover parentheses, a dash, a middle dot) -- the token
# regex alone would just skip unmatched characters, which is exactly
# the "silently drop unexpected content" failure mode this module must
# not have.
_FULL_FLAT_RE = re.compile(r"^([A-Z][a-z]?\d*\.?\d*)+$")

_VALID_ELEMENTS = {
    "H", "He", "Li", "Be", "B", "C", "N", "O", "F", "Ne", "Na", "Mg", "Al", "Si", "P", "S", "Cl",
    "Ar", "K", "Ca", "Sc", "Ti", "V", "Cr", "Mn", "Fe", "Co", "Ni", "Cu", "Zn", "Ga", "Ge", "As",
    "Se", "Br", "Kr", "Rb", "Sr", "Y", "Zr", "Nb", "Mo", "Tc", "Ru", "Rh", "Pd", "Ag", "Cd", "In",
    "Sn", "Sb", "Te", "I", "Xe", "Cs", "Ba", "La", "Ce", "Pr", "Nd", "Pm", "Sm", "Eu", "Gd", "Tb",
    "Dy", "Ho", "Er", "Tm", "Yb", "Lu", "Hf", "Ta", "W", "Re", "Os", "Ir", "Pt", "Au", "Hg", "Tl",
    "Pb", "Bi", "Po", "At", "Rn", "Fr", "Ra", "Ac", "Th", "Pa", "U", "Np", "Pu", "Am", "Cm", "Bk",
    "Cf", "Es", "Fm", "Md", "No", "Lr", "Rf", "Db", "Sg", "Bh", "Hs", "Mt", "Ds", "Rg", "Cn", "Nh",
    "Fl", "Mc", "Lv", "Ts", "Og",
}


def parse_flat_formula(formula):
    """Returns a {element_symbol: amount} dict, or None if `formula`
    isn't a flat (no parentheses/grouping) formula this parser can
    parse exactly: any unrecognized character, any element symbol not
    in the 118-symbol IUPAC table, or a repeated element (which would
    silently overwrite an amount) all return None rather than a
    partial/guessed result."""
    if not formula or not _FULL_FLAT_RE.match(formula):
        return None
    amounts = {}
    for symbol, amount_str in _TOKEN_RE.findall(formula):
        if symbol not in _VALID_ELEMENTS:
            return None
        amount = float(amount_str) if amount_str else 1.0
        if amount <= 0.0:
            return None
        if symbol in amounts:
            return None  # repeated element -- ambiguous, not silently summed
        amounts[symbol] = amount
    if not amounts:
        return None
    return amounts


def element_set(parsed):
    return set(parsed.keys())


def _smoke_check():
    cases = {
        "Ti3SiC2": {"Ti": 3.0, "Si": 1.0, "C": 2.0},
        "Fe0.9Ni0.1": {"Fe": 0.9, "Ni": 0.1},
        "CaTiO3": {"Ca": 1.0, "Ti": 1.0, "O": 3.0},
        "Bi2Te3": {"Bi": 2.0, "Te": 3.0},
        "Ag": {"Ag": 1.0},
    }
    for formula, expected in cases.items():
        got = parse_flat_formula(formula)
        assert got == expected, f"{formula}: expected {expected}, got {got}"
    # rejections -- must return None, never a partial/guessed parse
    for bad in ["(PbS)1.18(TiS2)2", "DyCl3·6H2O", "NaCl-KCl", "TiTi3", "", "Xx2O3"]:
        assert parse_flat_formula(bad) is None, f"expected None for {bad!r}, got {parse_flat_formula(bad)!r}"
    assert parse_flat_formula("CoNi2O") == {"Co": 1.0, "Ni": 2.0, "O": 1.0}  # legitimate flat formula
    print("parse_flat_formula smoke check: OK")


if __name__ == "__main__":
    _smoke_check()
