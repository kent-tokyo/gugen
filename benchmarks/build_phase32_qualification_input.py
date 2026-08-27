#!/usr/bin/env python3
"""Phase 32, Sections 1/4/5: pure-Python pre-classification pass.

For every row in both corpora, does everything that does NOT need
gugen's real `balance()`: formula parsing (thermodynamic-selectivity
corpus only -- Kononova ships pre-parsed `elements`), target/precursor
element-set comparison, and the dopant/host-ambiguity heuristic
(Section 5). Rows that can be terminally classified here (formula
unsupported, target/precursor element mismatch, dopant/host ambiguous)
are marked as such and skip the Rust balance step entirely. Rows whose
element sets are compatible (route element set equals the target's, or
differs only by elements in the conservative {C, H, O} allow-list) are
handed to `examples/exploration_phase32_reaction_qualification.rs` for
a real `balance()` attempt, together with which single-species
byproduct candidates (from CO2/H2O/O2 only -- deliberately narrower
than `curated_byproducts()`'s six species) are even worth trying.

Never conflates an inferred completion with original data: every
candidate byproduct is a *candidate*, decided here by element-overlap
only -- whether it actually balances is answered downstream by real
`balance()`, not assumed here.
"""

import json
from pathlib import Path

from parse_flat_formula import parse_flat_formula

DATA_DIR = Path(__file__).parent / "data"

# Deliberately narrower than curated_byproducts() (src/balance.rs) --
# no NO2, CO, or acetone. A candidate is offered only when every one of
# its own elements is already present among the route's "extra"
# elements (elements the route has that the target does not) -- i.e.
# some real declared precursor already carries that element.
BYPRODUCT_ALLOWLIST = {
    "CO2": {"C", "O"},
    "H2O": {"H", "O"},
    "O2": {"O"},
}

# A missing target element is treated as a plausible dopant-notation
# ambiguity (Section 5) rather than a hard mismatch only when its own
# share of the target's total site occupancy is small. Disclosed
# heuristic, not a rigorous rule -- verified by manual audit (Section 7).
DOPANT_FRACTION_THRESHOLD = 0.15


def _load_kononova_rows():
    rows = []
    for path in ("kononova_sample.jsonl", "kononova_high_arity_sample.jsonl"):
        with open(DATA_DIR / path) as f:
            for i, line in enumerate(f):
                row = json.loads(line)
                rows.append(
                    {
                        "row_id": f"kononova:{path}:{i}",
                        "corpus": "kononova",
                        "doi": row["doi"],
                        "target_formula": row["target_formula"],
                        "target_elements": row["target_elements"],
                        "route_formulas": [p["formula"] for p in row["precursors"]],
                        "route_elements": [p["elements"] for p in row["precursors"]],
                        "verdict": None,
                    }
                )
    return rows


def _load_thermo_rows():
    with open(DATA_DIR / "thermodynamic_selectivity_clean_population.json") as f:
        data = json.load(f)
    rows = []
    for i, row in enumerate(data):
        target_elements = parse_flat_formula(row["target"])
        route_elements = [parse_flat_formula(f) for f in row["route"]]
        rows.append(
            {
                "row_id": f"thermodynamic_selectivity:{i}",
                "corpus": "thermodynamic_selectivity",
                "doi": sorted(row["dois"])[0],
                "target_formula": row["target"],
                "target_elements": target_elements,
                "route_formulas": list(row["route"]),
                "route_elements": route_elements,
                "verdict": row["verdict"],
            }
        )
    return rows


def classify_pre_balance(row):
    """Returns a dict with at least a `stage` key: "terminal" (status
    already decided, no balance() call needed) or "needs_balance"
    (hand to the Rust harness, with `byproduct_candidates` to try)."""
    target_elements = row["target_elements"]
    route_elements = row["route_elements"]

    if target_elements is None or any(e is None for e in route_elements):
        return {
            "stage": "terminal",
            "status": "FormulaUnsupported",
            "reason_codes": ["target_or_precursor_formula_not_parseable"],
        }

    target_set = set(target_elements)
    route_set = set()
    for d in route_elements:
        route_set |= set(d)

    missing = target_set - route_set
    extra = route_set - target_set

    if missing:
        total = sum(target_elements.values())
        dopant_like = total > 0 and all(target_elements[el] / total <= DOPANT_FRACTION_THRESHOLD for el in missing)
        if dopant_like:
            return {
                "stage": "terminal",
                "status": "DopantHostAmbiguous",
                "reason_codes": ["target_dopant_absent_from_every_route_precursor"],
                "detail": {"missing_elements": sorted(missing)},
            }
        return {
            "stage": "terminal",
            "status": "TargetPrecursorElementMismatch",
            "reason_codes": ["target_element_absent_from_every_route_precursor"],
            "detail": {"missing_elements": sorted(missing)},
        }

    if extra and not extra.issubset({"C", "H", "O"}):
        return {
            "stage": "terminal",
            "status": "DopantHostAmbiguous",
            "reason_codes": ["route_extra_element_outside_byproduct_allowlist"],
            "detail": {"extra_elements": sorted(extra)},
        }

    # A candidate's *non-oxygen* elements must come from `extra` (route
    # carries an element the target doesn't -- a real carbonate/hydrate
    # signal), but oxygen itself is not required to be "extra": oxygen
    # legitimately appears on both sides of almost every solid-state
    # oxide reaction, and an O2 release/uptake needs no foreign element
    # at all (e.g. `2 Mn2O3 -> 4 MnO + O2`). Gating O2 on "O in extra"
    # would silently miss every same-element-set redox case.
    all_elements = target_set | route_set
    candidates = []
    if "C" in extra:
        candidates.append("CO2")
    if "H" in extra:
        candidates.append("H2O")
    if "O" in all_elements:
        candidates.append("O2")
    return {
        "stage": "needs_balance",
        "byproduct_candidates": candidates,
        "extra_elements": sorted(extra),
    }


def main():
    rows = _load_kononova_rows() + _load_thermo_rows()

    output_rows = []
    needs_balance_count = 0
    for row in rows:
        classification = classify_pre_balance(row)
        output_row = {
            "row_id": row["row_id"],
            "corpus": row["corpus"],
            "doi": row["doi"],
            "target_formula": row["target_formula"],
            "target_elements": row["target_elements"],
            "route_formulas": row["route_formulas"],
            "route_elements": row["route_elements"],
            "verdict": row["verdict"],
            **classification,
        }
        if classification["stage"] == "needs_balance":
            needs_balance_count += 1
        output_rows.append(output_row)

    out_path = DATA_DIR / "phase32_qualification_input.json"
    with open(out_path, "w") as f:
        json.dump(output_rows, f)

    terminal_counts = {}
    for r in output_rows:
        if r["stage"] == "terminal":
            terminal_counts[r["status"]] = terminal_counts.get(r["status"], 0) + 1
    print(f"Total rows: {len(output_rows)}")
    print(f"Needs real balance() attempt: {needs_balance_count}")
    print("Terminal (pre-classified, no balance() needed):")
    for status, count in sorted(terminal_counts.items(), key=lambda kv: -kv[1]):
        print(f"  {status}: {count}")
    print(f"Wrote {out_path}")


if __name__ == "__main__":
    main()
