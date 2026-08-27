#!/usr/bin/env python3
"""Phase 32, Section 3: formula-shape failure taxonomy.

Classifies every distinct formula string appearing in the Kononova and
thermodynamic-selectivity corpora into a shape bucket, and separately
reports how many *rows* each bucket touches. Purpose: measure before
building -- per the phase's own explicit instruction, do not add a
more general parser (e.g. parenthesis support) until the real number
of rows it would recover is known. This module only classifies string
*shape*; it never attempts to parse the harder shapes.

Buckets (mutually exclusive, checked in this priority order per
formula):

1. ``flat`` -- ``parse_flat_formula`` already succeeds. Decimal
   occupancy (e.g. ``Fe0.9Ni0.1``) is a sub-flag on this bucket, not a
   separate one -- it already parses, this only reports how common it
   is.
2. ``phase_affix`` -- a Greek-letter polymorph prefix (``alpha-Fe2O3``)
   or a trailing state marker (``(s)``/``(g)``/``(l)``/``(aq)``) that,
   once stripped, leaves a flat formula. Checked before the generic
   parenthesis buckets because a bare ``(s)`` suffix is structurally a
   one-line strip, not real grouping.
3. ``nested_parentheses`` -- contains a parenthesis nested inside
   another parenthesis (nesting depth >= 2).
4. ``parentheses`` -- contains parentheses, nesting depth == 1 (may
   still have multiple side-by-side groups, e.g. ``(PbS)1.18(TiS2)2``).
5. ``hydrate_dot`` -- a middle-dot or decimal-looking hydrate
   separator with no parentheses (``DyCl3·6H2O``, ``CuSO4.5H2O``).
6. ``charge_annotation`` -- an ionic charge suffix (``Fe3+``,
   ``VO4^3-``) that isn't a dash-separated flux mixture.
7. ``malformed`` -- catch-all: unknown element symbols, empty strings,
   dash-separated mixtures, or anything else not fitting above.

Run standalone: ``python3 benchmarks/formula_shape_taxonomy.py``.
"""

import json
import re
from collections import defaultdict
from pathlib import Path

from parse_flat_formula import parse_flat_formula

DATA_DIR = Path(__file__).parent / "data"

_GREEK_PREFIX_RE = re.compile(r"^(alpha|beta|gamma|delta|epsilon|zeta|eta|theta)-(.+)$", re.IGNORECASE)
_GREEK_UNICODE_PREFIX_RE = re.compile(r"^([α-ω])-(.+)$")
_PHASE_SUFFIX_RE = re.compile(r"^(.+)\((s|g|l|aq)\)$")
_DASH_MIXTURE_RE = re.compile(r"^[A-Z][A-Za-z0-9.]*-[A-Z][A-Za-z0-9.]*$")
_CHARGE_SUFFIX_RE = re.compile(r"^(.+?)\^?\d*[+-]$")
_NONSTOICHIOMETRY_SUFFIX_RE = re.compile(r"-[δxy]$")
_TOKEN_RE = re.compile(r"([A-Z][a-z]?)(\d+\.?\d*)?")
_FULL_FLAT_SHAPE_RE = re.compile(r"^([A-Z][a-z]?\d*\.?\d*)+$")


def _would_parse_if_repeats_summed(formula):
    """True if `formula` has flat *shape* (element+amount tokens only,
    no grouping) and every element symbol is individually valid, but
    it was rejected only because a symbol repeats (e.g. `NH4H2PO4`
    writes H twice by convention). Diagnostic only -- summing repeats
    is a real, distinct parser capability this does NOT implement, so
    Section 3's own "measure before building" rule can be applied to
    it specifically."""
    from parse_flat_formula import _VALID_ELEMENTS

    if not formula or not _FULL_FLAT_SHAPE_RE.match(formula):
        return False
    seen = set()
    any_repeat = False
    for symbol, amount_str in _TOKEN_RE.findall(formula):
        if not symbol:
            continue
        if symbol not in _VALID_ELEMENTS:
            return False
        amount = float(amount_str) if amount_str else 1.0
        if amount <= 0.0:
            return False
        if symbol in seen:
            any_repeat = True
        seen.add(symbol)
    return any_repeat


def _max_paren_depth(formula):
    depth = 0
    max_depth = 0
    for ch in formula:
        if ch == "(":
            depth += 1
            max_depth = max(max_depth, depth)
        elif ch == ")":
            depth -= 1
    return max_depth


def classify_formula_shape(formula):
    """Returns (bucket: str, detail: dict). `detail` carries whatever
    extra fact justified the bucket (e.g. the stripped-flat remainder),
    for manual-audit spot checks -- never used to silently "fix" the
    formula."""
    if not formula:
        return "malformed", {"reason": "empty string"}

    if parse_flat_formula(formula) is not None:
        has_decimal = bool(re.search(r"\d\.\d", formula))
        return "flat", {"has_decimal_occupancy": has_decimal}

    m = _PHASE_SUFFIX_RE.match(formula)
    if m and parse_flat_formula(m.group(1)) is not None:
        return "phase_affix", {"kind": "suffix", "marker": m.group(2), "remainder": m.group(1)}
    m = _GREEK_PREFIX_RE.match(formula) or _GREEK_UNICODE_PREFIX_RE.match(formula)
    if m and parse_flat_formula(m.group(2)) is not None:
        return "phase_affix", {"kind": "prefix", "marker": m.group(1), "remainder": m.group(2)}

    if "(" in formula or ")" in formula:
        depth = _max_paren_depth(formula)
        if depth >= 2:
            return "nested_parentheses", {"max_depth": depth}
        return "parentheses", {"max_depth": depth}

    if "·" in formula or re.search(r"\d\.\d*H2O\b", formula) or re.search(r"\.\d*H2O\b", formula):
        return "hydrate_dot", {}

    if _DASH_MIXTURE_RE.match(formula):
        return "malformed", {"reason": "dash-separated mixture"}
    m = _CHARGE_SUFFIX_RE.match(formula)
    if m and parse_flat_formula(m.group(1)) is not None:
        return "charge_annotation", {"remainder": m.group(1)}

    if _NONSTOICHIOMETRY_SUFFIX_RE.search(formula) and parse_flat_formula(
        _NONSTOICHIOMETRY_SUFFIX_RE.sub("", formula)
    ):
        return "malformed", {"reason": "symbolic non-stoichiometry suffix (e.g. -delta), not recoverable"}
    if _would_parse_if_repeats_summed(formula):
        return "malformed", {"reason": "repeated element symbol, would parse if summed"}

    return "malformed", {"reason": "unrecognized shape (likely acronym/trade name or unknown symbol)"}


def _load_kononova_formulas():
    """Returns [(row_id, [formula, ...])] -- one entry per row, listing
    every formula string (target + all precursors) that row contains.
    Kononova ships pre-parsed `elements` too; this only surveys the
    formula *strings* for shape-taxonomy reporting, per Section 3 --
    it does not imply Kononova needs this parser to be usable."""
    rows = []
    for path in ("kononova_sample.jsonl", "kononova_high_arity_sample.jsonl"):
        with open(DATA_DIR / path) as f:
            for i, line in enumerate(f):
                row = json.loads(line)
                formulas = [row["target_formula"]] + [p["formula"] for p in row["precursors"]]
                rows.append((f"{path}:{i}", formulas))
    return rows


def _load_thermo_formulas():
    with open(DATA_DIR / "thermodynamic_selectivity_clean_population.json") as f:
        data = json.load(f)
    rows = []
    for i, row in enumerate(data):
        formulas = [row["target"]] + list(row["route"])
        rows.append((f"thermodynamic_selectivity_clean_population.json:{i}", formulas))
    return rows


def survey(rows):
    """rows: [(row_id, [formula, ...])]. Returns a report dict: per
    distinct formula's bucket, plus row-impact counts (how many rows
    have at least one formula in each bucket -- the number that
    actually matters for "would adding support recover rows")."""
    formula_bucket = {}
    formula_reason = {}
    formula_detail = {}
    for _, formulas in rows:
        for formula in formulas:
            if formula not in formula_bucket:
                bucket, detail = classify_formula_shape(formula)
                formula_bucket[formula] = bucket
                formula_detail[formula] = detail
                formula_reason[formula] = detail.get("reason") if bucket == "malformed" else None

    distinct_counts = defaultdict(int)
    for bucket in formula_bucket.values():
        distinct_counts[bucket] += 1

    row_impact = defaultdict(set)
    malformed_reason_row_impact = defaultdict(set)
    all_flat_rows = set()
    for row_id, formulas in rows:
        buckets_in_row = {formula_bucket[f] for f in formulas}
        for bucket in buckets_in_row:
            row_impact[bucket].add(row_id)
        for f in formulas:
            if formula_bucket[f] == "malformed":
                malformed_reason_row_impact[formula_reason[f]].add(row_id)
        if buckets_in_row == {"flat"}:
            all_flat_rows.add(row_id)

    examples = defaultdict(list)
    for formula, bucket in formula_bucket.items():
        if len(examples[bucket]) < 8:
            examples[bucket].append({"formula": formula, "detail": formula_detail[formula]})

    return {
        "total_rows": len(rows),
        "distinct_formulas": len(formula_bucket),
        "distinct_formula_counts_by_bucket": dict(distinct_counts),
        "rows_with_all_formulas_flat": len(all_flat_rows),
        "rows_touched_by_bucket": {b: len(rs) for b, rs in row_impact.items()},
        "rows_touched_by_malformed_reason": {r: len(rs) for r, rs in malformed_reason_row_impact.items()},
        "examples_by_bucket": {b: v for b, v in examples.items()},
    }


def main():
    kononova_rows = _load_kononova_formulas()
    thermo_rows = _load_thermo_formulas()

    report = {
        "kononova": survey(kononova_rows),
        "thermodynamic_selectivity": survey(thermo_rows),
    }

    out_path = DATA_DIR / "phase32_formula_shape_taxonomy.json"
    with open(out_path, "w") as f:
        json.dump(report, f, indent=2, sort_keys=True)

    for corpus, r in report.items():
        print(f"\n=== {corpus} ===")
        print(f"total_rows={r['total_rows']} distinct_formulas={r['distinct_formulas']}")
        print(f"rows_with_all_formulas_flat={r['rows_with_all_formulas_flat']}")
        print("rows_touched_by_bucket:")
        for bucket, count in sorted(r["rows_touched_by_bucket"].items(), key=lambda kv: -kv[1]):
            print(f"  {bucket}: {count}")
        if r["rows_touched_by_malformed_reason"]:
            print("rows_touched_by_malformed_reason:")
            for reason, count in sorted(r["rows_touched_by_malformed_reason"].items(), key=lambda kv: -kv[1]):
                print(f"  {reason}: {count}")
    print(f"\nWrote {out_path}")


if __name__ == "__main__":
    main()
