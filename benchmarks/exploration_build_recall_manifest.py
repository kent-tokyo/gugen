#!/usr/bin/env python3
"""Phase 28 (Exploration Benchmark Lock): builds the precursor-set recall
ground-truth manifest for gugen's exploration benchmarks, from two
datasets already committed to this repo -- no new fetch.

Run: python3 benchmarks/exploration_build_recall_manifest.py
Output: benchmarks/data/exploration_recall_manifest.json

Sources:
  - benchmarks/data/kononova_sample.jsonl (1500 rows, Phase 11,
    fetch_kononova.py) -- PRIMARY, unbiased with respect to route
    multiplicity. Schema per row: {"doi", "precursors": [{"elements",
    "formula"}], "target_elements", "target_formula"}. Element amounts
    are already parsed by the source -- used directly, never re-derived
    from the formula string.
  - benchmarks/data/thermodynamic_selectivity_clean_population.json
    (1692 route rows / 381 targets, Phase 21A,
    audit_thermodynamic_selectivity_dataset_feasibility.py) -- SECONDARY.
    Schema per row: {"dois": [...], "route": [formula, ...], "target":
    formula, "verdict": "pure"|"impure"}. Explicitly BIASED: pre-filtered
    to targets with >=2 routes and a differing pure/impure verdict
    (Phase 21A's "selectivity signal" criterion) -- not a representative
    sample of all known routes. Recorded in the manifest's own
    `source.bias_note`, never silently treated as equivalent to the
    primary source. Only formula *strings* are given; amounts are
    recovered by this script's own bounded formula parser (see
    `parse_formula_amounts`), null with a stated reason where that
    parser can't fully consume a string (hydrate notation, hyphenated
    polymorph prefixes like "gamma-Al2O3", disordered/free-variable
    notation, or nested parentheses beyond one level).

A "route" is one target's distinct, deduplicated, sorted precursor-
formula list -- the same definition
docs/thermodynamic_selectivity_dataset_feasibility.md already uses.
Ground truth is built per distinct (target_formula, route) pair: a route
reported by many DOIs must count once, not once per DOI (DOI lists are
unioned as route metadata, never used as a weight).

**Leakage exclusion happens here, by construction** -- reusing
fetch_kononova.py's exact EXCLUDED_ROUTES/route_key/canonical_ratio
mechanism (not reinvented), applied to each raw (target, route) pair's
*real* element amounts before merging. A pair is excluded outright, and
counted, whenever both its target and every route member have real,
resolvable amounts and the resulting route_key matches EXCLUDED_ROUTES.
A pair whose amounts can't be fully resolved is NOT excluded (there is
nothing to match against) -- it is instead counted separately as
`leakage_unchecked_pairs`, an honest "could not verify" bucket, never
silently treated as clean. `benchmarks/exploration_check_split_leakage.py`
independently re-verifies this against the built manifest, using the
same real element_amounts this script persists per row (not a weaker
re-parse of formula strings).

Split axes computed per manifest row, matching the owner's own named
list: target_formula, reduced_formula, chemical_system, material_family,
doi(s), publication year. `year` is always null with a stated reason --
absent from both source datasets; deliberately not resolved via a new
network dependency (a Crossref/DOI-resolver lookup) for the single least
load-bearing axis.
"""

import json
import re
import sys
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from fetch_kononova import EXCLUDED_ROUTES, route_key  # noqa: E402

DATA_DIR = Path(__file__).parent / "data"
KONONOVA_PATH = DATA_DIR / "kononova_sample.jsonl"
CLEAN_POPULATION_PATH = DATA_DIR / "thermodynamic_selectivity_clean_population.json"
MANIFEST_PATH = DATA_DIR / "exploration_recall_manifest.json"

# Same informal, reporting-only bucketing as analyze_oqmd_coverage_gate.py
# (kept as an independent copy, not an import, since the two scripts have
# no other shared dependency and this keeps each runnable standalone --
# matching this crate's existing precedent of small, self-contained
# benchmark scripts). NOT a chemical taxonomy; first matching rule wins.
_FAMILY_RULES = [
    ("Oxide", {"O"}),
    ("Sulfide/chalcogenide", {"S", "Se", "Te"}),
    ("Halide", {"F", "Cl", "Br", "I"}),
    ("Nitride", {"N"}),
    ("Phosphide/phosphate", {"P"}),
]
_ELEMENT_TOKEN_RE = re.compile(r"[A-Z][a-z]?")


def elements_in_formula(formula):
    """Crude element-symbol extraction: every capitalized 1-2-letter
    token, ignoring amounts/parentheses/punctuation entirely. Sufficient
    for chemical_system/material_family (element-set only, no
    stoichiometry needed) even for formulas `parse_formula_amounts`
    below can't fully parse."""
    return set(_ELEMENT_TOKEN_RE.findall(formula))


def classify_family(formula):
    elements = elements_in_formula(formula)
    for name, anions in _FAMILY_RULES:
        if elements & anions:
            return name
    return "Other"


def chemical_system(elements_iterable):
    return "-".join(sorted(elements_iterable))


# Bounded formula-amount parser: ELEMENT[AMOUNT] tokens and one level of
# (GROUP)[AMOUNT] parenthesization, e.g. "Sc2(MoO4)3" -> {Sc:2, Mo:3,
# O:12}. Does NOT handle hydrate dots ("·"), polymorph prefixes
# ("gamma-Al2O3"), disordered/free-variable notation, or nested
# parentheses beyond one level -- returns None on anything it can't
# fully consume, rather than a wrong or partial answer. This is
# intentionally narrower than gugen's own Rust FormulaParser
# (src/commercial_catalog/formula.rs); it exists only to recover amounts
# for the secondary dataset's string-only formulas, never library-facing.
_TOKEN_RE = re.compile(r"([A-Z][a-z]?)(\d*\.?\d*)")
_GROUP_RE = re.compile(r"\(([^()]*)\)(\d*\.?\d*)")


def parse_formula_amounts(formula):
    working = formula
    totals = defaultdict(float)

    def consume_flat(text, multiplier):
        pos = 0
        for match in _TOKEN_RE.finditer(text):
            if match.start() != pos:
                return False
            pos = match.end()
            symbol, amount_str = match.groups()
            amount = float(amount_str) if amount_str else 1.0
            totals[symbol] += amount * multiplier
        return pos == len(text)

    def replace_group(match):
        inner, amount_str = match.groups()
        amount = float(amount_str) if amount_str else 1.0
        if not consume_flat(inner, amount):
            raise ValueError("unparseable group")
        return ""

    try:
        working = _GROUP_RE.sub(replace_group, working)
    except ValueError:
        return None
    if "(" in working or ")" in working:
        return None  # nested/unbalanced parens beyond one level
    if not consume_flat(working, 1.0):
        return None
    if not totals:
        return None
    return dict(totals)


def reduced_formula_signature(amounts):
    """Scale-invariant signature: divide every amount by the minimum
    amount, round to tolerate float noise, sort by element -- same
    convention as fetch_kononova.py's canonical_ratio (kept as an
    independent copy for the reason stated above)."""
    if not amounts:
        return None
    m = min(amounts.values())
    if m <= 0:
        return None
    return [[el, round(amt / m, 4)] for el, amt in sorted(amounts.items())]


def load_kononova_rows():
    """Each raw row keeps real element amounts straight from the source
    (never re-derived from a formula string) for both the target and
    every precursor -- this dataset already parsed them once, correctly,
    including formulas this script's own bounded parser can't handle
    (e.g. polymorph-prefixed "gamma-Al2O3")."""
    rows = []
    with open(KONONOVA_PATH, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            record = json.loads(line)
            target_amounts = {el: float(amt) for el, amt in record["target_elements"].items()}
            route_amounts_by_formula = {}
            for p in record["precursors"]:
                route_amounts_by_formula[p["formula"]] = {
                    el: float(amt) for el, amt in p["elements"].items()
                }
            route = sorted(route_amounts_by_formula.keys())
            rows.append(
                {
                    "target_formula": record["target_formula"],
                    "target_amounts": target_amounts,
                    "route": route,
                    "route_amounts": [route_amounts_by_formula[f] for f in route],
                    "dois": [record["doi"]],
                    "chemical_system": chemical_system(target_amounts.keys()),
                    "material_family": classify_family(record["target_formula"]),
                    "reduced_formula": reduced_formula_signature(target_amounts),
                    "reduced_formula_unavailable_reason": None,
                    "source_dataset": "kononova",
                }
            )
    return rows


def load_clean_population_rows():
    with open(CLEAN_POPULATION_PATH, encoding="utf-8") as f:
        records = json.load(f)
    rows = []
    for record in records:
        target_formula = record["target"]
        route = sorted(set(record["route"]))
        target_amounts = parse_formula_amounts(target_formula)
        route_amounts = [parse_formula_amounts(f) for f in route]
        rows.append(
            {
                "target_formula": target_formula,
                "target_amounts": target_amounts,
                "route": route,
                "route_amounts": route_amounts,
                "dois": list(record["dois"]),
                "chemical_system": chemical_system(elements_in_formula(target_formula)),
                "material_family": classify_family(target_formula),
                "reduced_formula": (
                    reduced_formula_signature(target_amounts) if target_amounts else None
                ),
                "reduced_formula_unavailable_reason": (
                    None
                    if target_amounts
                    else "source dataset provides formula strings only; this string "
                    "was not parseable by this script's bounded formula parser"
                ),
                "source_dataset": "lee_thermodynamic_selectivity_2025",
            }
        )
    return rows


def is_fully_resolved(row):
    return row["target_amounts"] is not None and all(
        a is not None for a in row["route_amounts"]
    )


def is_excluded(row):
    """True only when every formula in this row resolved to real amounts
    AND the resulting route_key matches EXCLUDED_ROUTES. A row that
    can't be fully resolved is never excluded here -- see
    `is_fully_resolved` and the leakage_unchecked_pairs count."""
    if not is_fully_resolved(row):
        return False
    key = route_key(row["target_amounts"], row["route_amounts"])
    return key in EXCLUDED_ROUTES


def filter_leakage(raw_rows):
    kept, excluded, unchecked = [], 0, 0
    for row in raw_rows:
        if is_excluded(row):
            excluded += 1
            continue
        if not is_fully_resolved(row):
            unchecked += 1
        kept.append(row)
    return kept, excluded, unchecked


def merge_rows(raw_rows):
    """Groups by distinct (target_formula, route) pair -- a route
    reported by many DOIs/records counts once. DOI lists are unioned;
    source_dataset becomes a sorted list if the same pair appears in
    both sources; real amounts are kept whenever either contributor has
    them."""
    grouped = {}
    for row in raw_rows:
        key = (row["target_formula"], tuple(row["route"]))
        if key not in grouped:
            grouped[key] = {
                "target_formula": row["target_formula"],
                "target_amounts": row["target_amounts"],
                "route": row["route"],
                "route_amounts": row["route_amounts"],
                "chemical_system": row["chemical_system"],
                "material_family": row["material_family"],
                "reduced_formula": row["reduced_formula"],
                "reduced_formula_unavailable_reason": row["reduced_formula_unavailable_reason"],
                "publication_year": None,
                "publication_year_unavailable_reason": (
                    "absent from both source datasets (kononova_sample.jsonl and "
                    "thermodynamic_selectivity_clean_population.json carry no year "
                    "field); not resolved via a new Crossref/DOI-resolver network "
                    "dependency"
                ),
                "dois": set(),
                "source_datasets": set(),
            }
        entry = grouped[key]
        entry["dois"].update(row["dois"])
        entry["source_datasets"].add(row["source_dataset"])
        if entry["target_amounts"] is None and row["target_amounts"] is not None:
            entry["target_amounts"] = row["target_amounts"]
            entry["reduced_formula"] = row["reduced_formula"]
            entry["reduced_formula_unavailable_reason"] = row["reduced_formula_unavailable_reason"]
        if any(a is None for a in entry["route_amounts"]) and all(
            a is not None for a in row["route_amounts"]
        ):
            entry["route_amounts"] = row["route_amounts"]

    manifest_rows = []
    for entry in grouped.values():
        entry["dois"] = sorted(entry["dois"])
        entry["source_datasets"] = sorted(entry["source_datasets"])
        manifest_rows.append(entry)
    manifest_rows.sort(key=lambda r: (r["target_formula"], tuple(r["route"])))
    return manifest_rows


def build_manifest():
    kononova_rows = load_kononova_rows()
    clean_population_rows = load_clean_population_rows()
    all_rows = kononova_rows + clean_population_rows
    kept_rows, leakage_excluded, leakage_unchecked = filter_leakage(all_rows)
    manifest_rows = merge_rows(kept_rows)

    distinct_targets = {r["target_formula"] for r in manifest_rows}
    routes_per_target = defaultdict(int)
    for r in manifest_rows:
        routes_per_target[r["target_formula"]] += 1
    targets_with_multiple_routes = sum(1 for n in routes_per_target.values() if n > 1)
    unparseable_reduced_formula = sum(1 for r in manifest_rows if r["reduced_formula"] is None)

    return {
        "source": {
            "primary": {
                "file": "benchmarks/data/kononova_sample.jsonl",
                "row_count": len(kononova_rows),
                "bias_note": "unbiased with respect to route multiplicity -- a "
                "downsampled, leakage-filtered slice of the full Kononova et al. "
                "2019 corpus (Phase 11)",
            },
            "secondary": {
                "file": "benchmarks/data/thermodynamic_selectivity_clean_population.json",
                "row_count": len(clean_population_rows),
                "bias_note": "BIASED: pre-filtered to targets with >=2 routes and a "
                "differing pure/impure outcome verdict (Phase 21A's own "
                "'selectivity signal' criterion) -- not a representative sample "
                "of all known routes for a target. Never let this source alone "
                "inflate a multi-route-diversity number.",
            },
        },
        "counts": {
            "raw_rows_before_leakage_filter": len(all_rows),
            "leakage_excluded_pairs": leakage_excluded,
            "leakage_unchecked_pairs": leakage_unchecked,
            "distinct_target_route_pairs": len(manifest_rows),
            "distinct_targets": len(distinct_targets),
            "targets_with_multiple_known_routes": targets_with_multiple_routes,
            "rows_with_unavailable_reduced_formula": unparseable_reduced_formula,
        },
        "rows": manifest_rows,
    }


def main():
    manifest = build_manifest()
    MANIFEST_PATH.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    counts = manifest["counts"]
    print(
        f"wrote {MANIFEST_PATH}: {counts['distinct_target_route_pairs']} distinct "
        f"(target, route) pairs across {counts['distinct_targets']} targets "
        f"({counts['targets_with_multiple_known_routes']} with >=2 known routes); "
        f"{counts['leakage_excluded_pairs']} pair(s) excluded as leakage, "
        f"{counts['leakage_unchecked_pairs']} pair(s) not fully checkable for "
        f"leakage (kept, but flagged); {counts['rows_with_unavailable_reduced_formula']} "
        "row(s) with no computable reduced_formula",
        file=sys.stderr,
    )


if __name__ == "__main__":
    main()
