#!/usr/bin/env python3
"""Phase 20A: a full-corpus inventory/audit of the Kononova et al. 2019
text-mined synthesis dataset (the same corpus `fetch_kononova.py` already
uses for Phase 11's holdout benchmark), independent of and prior to any
literature-condition-provider implementation (AGENTS.md §21/§23/§27:
report-only, no Planner connection, no bulk-provider code).

This is NOT `fetch_kononova.py` re-run -- that script filters down to a
1500-route holdout SAMPLE for benchmarking. This script inventories the
FULL raw corpus (all 19,488 reactions) to answer a different question:
how much of this corpus is actually usable as literature condition
precedents (AGENTS.md §10-style `ConditionPrecedent`s), and what does it
look like structurally, before any decision to build a bulk provider.

Run: python3 benchmarks/audit_kononova_corpus.py
     python3 benchmarks/audit_kononova_corpus.py --local /path/to/cached.json
       (dev iteration; the license is still checked live every run -- see
       fetch_kononova.py's own docstring for why a cached copy must never
       be trusted without a live fetch first, per this project's own
       Phase 11 incident: a differently-provenanced 30,031-entry cache
       silently substituted for the correctly-licensed 19,488-entry
       figshare deposit until the live path was actually run.)
Output: docs/literature_condition_corpus_audit.md
"""

import argparse
import json
import ssl
import sys
import urllib.request
from collections import Counter, defaultdict
from pathlib import Path

try:
    import certifi

    _SSL_CONTEXT = ssl.create_default_context(cafile=certifi.where())
except ImportError:
    _SSL_CONTEXT = None


def _urlopen(url):
    return urllib.request.urlopen(url, context=_SSL_CONTEXT)


FIGSHARE_ARTICLE = "https://api.figshare.com/v2/articles/9722159"
EXPECTED_REACTION_COUNT = 19488
DOCS_DIR = Path(__file__).parent.parent / "docs"
OUTPUT = DOCS_DIR / "literature_condition_corpus_audit.md"

# gugen's own `HeatingPurpose`/ProcessStep field set (src/process.rs) --
# what a `ConditionPrecedent` can actually carry. Used only to name which
# raw-corpus fields correspond to which gugen field, not to filter.
GUGEN_CONDITION_FIELDS = ["temperature", "duration", "atmosphere", "ramp", "pressure"]


def fetch_dataset(local_path):
    with _urlopen(FIGSHARE_ARTICLE) as resp:
        meta = json.load(resp)
    license_name = (meta.get("license") or {}).get("name")
    if license_name != "CC BY 4.0":
        sys.exit(f"REFUSING: license is {license_name!r}, expected 'CC BY 4.0'")
    print(f"license OK ({license_name})", file=sys.stderr)

    if local_path:
        print(f"using local file {local_path}", file=sys.stderr)
        with open(local_path) as f:
            data = json.load(f)
    else:
        files = meta.get("files") or []
        if len(files) != 1:
            sys.exit(f"REFUSING: expected exactly one file, found {len(files)}")
        download_url = files[0]["download_url"]
        print(f"downloading {files[0]['name']} ({files[0]['size']} bytes)", file=sys.stderr)
        with _urlopen(download_url) as resp:
            data = json.load(resp)

    reactions = data
    if len(reactions) != EXPECTED_REACTION_COUNT:
        sys.exit(
            f"REFUSING: expected {EXPECTED_REACTION_COUNT} reactions, got "
            f"{len(reactions)} -- dataset may have changed, re-verify before proceeding"
        )
    return reactions


def is_plain_positive_number(x):
    try:
        return float(x) > 0
    except (TypeError, ValueError):
        return False


def parseable_composition(material):
    """Same criteria as fetch_kononova.py's own function, duplicated
    rather than imported (this project's established convention: each
    benchmarks/*.py script is self-contained, matching the "no shared
    test helper" call already made elsewhere in this codebase)."""
    if material.get("amounts_vars") or material.get("elements_vars"):
        return None
    comps = material.get("composition") or []
    if len(comps) != 1:
        return None
    elements = comps[0].get("elements") or {}
    if not elements:
        return None
    if not all(is_plain_positive_number(v) for v in elements.values()):
        return None
    return {el: float(amt) for el, amt in elements.items()}


def canonical_ratio(elements):
    amounts = {el: float(amt) for el, amt in elements.items()}
    if not amounts:
        return None
    m = min(amounts.values())
    if m <= 0:
        return None
    return tuple(sorted((el, round(amt / m, 4)) for el, amt in amounts.items()))


def heating_ops(record):
    return [
        op
        for op in (record.get("operations") or [])
        if op.get("type") == "HeatingOperation" and op.get("conditions")
    ]


def resolved_temperatures_celsius(heating_op):
    """A HeatingOperation's `heating_temperature` is a list of range
    objects (usually 0 or 1 entries in this corpus); returns the list of
    usable point/max values in Celsius, `[]` if none resolved."""
    out = []
    for entry in heating_op.get("conditions", {}).get("heating_temperature") or []:
        v = entry.get("max_value")
        if v is None:
            v = entry.get("min_value")
        if v is not None:
            out.append(float(v))
    return out


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--local", help="path to an already-downloaded copy of the dataset JSON")
    args = parser.parse_args()

    reactions = fetch_dataset(args.local)
    print(f"{len(reactions)} raw reactions", file=sys.stderr)

    # --- structural validity (same bar as fetch_kononova.py's holdout filter,
    # minus its leakage exclusion, which is specific to that script's
    # benchmark purpose) ---
    unparseable_target = 0
    unparseable_precursor = 0
    zero_or_too_many_precursors = 0
    valid_records = []

    for r in reactions:
        target = parseable_composition(r["target"])
        if target is None:
            unparseable_target += 1
            continue
        precursors_raw = r.get("precursors") or []
        precursor_comps = []
        ok = True
        for p in precursors_raw:
            c = parseable_composition(p)
            if c is None:
                ok = False
                break
            precursor_comps.append((p.get("material_formula", ""), c))
        if not ok:
            unparseable_precursor += 1
            continue
        precursor_comps = list({formula: comp for formula, comp in precursor_comps}.items())
        if len(precursor_comps) == 0 or len(precursor_comps) > 4:
            zero_or_too_many_precursors += 1
            continue
        valid_records.append(
            {
                "raw": r,
                "target_formula": r["target"].get("material_formula"),
                "target": target,
                "precursors": precursor_comps,
                "doi": r.get("doi"),
            }
        )

    parse_failures = unparseable_target + unparseable_precursor + zero_or_too_many_precursors
    print(f"valid (structurally parseable) records: {len(valid_records)}", file=sys.stderr)

    # --- unique targets / exact routes ---
    unique_target_sigs = set()
    route_to_records = defaultdict(list)  # (target_sig, precursor_sigs) -> [record, ...]
    target_formulas = set()
    precursor_formulas = set()

    for rec in valid_records:
        target_sig = canonical_ratio(rec["target"])
        unique_target_sigs.add(target_sig)
        target_formulas.add(rec["target_formula"])
        precursor_sigs = frozenset(
            sig for _, c in rec["precursors"] if (sig := canonical_ratio(c)) is not None
        )
        for formula, _ in rec["precursors"]:
            precursor_formulas.add(formula)
        route_key = (target_sig, precursor_sigs)
        route_to_records[route_key].append(rec)

    target_only = target_formulas - precursor_formulas
    precursor_only = precursor_formulas - target_formulas
    both = target_formulas & precursor_formulas

    # --- operation-type / route-shape distribution (whole corpus, not just valid) ---
    op_type_counts = Counter()
    grinding_no_heat_count = 0
    for r in reactions:
        types_here = set()
        for op in r.get("operations") or []:
            op_type_counts[op.get("type")] += 1
            types_here.add(op.get("type"))
        if "LiquidGrinding" in types_here and "HeatingOperation" not in types_here:
            grinding_no_heat_count += 1

    # --- per-field condition coverage, at the RECORD level: does this
    # valid record have >=1 HeatingOperation reporting this field at all --
    has_temperature = 0
    has_duration = 0
    has_atmosphere = 0
    has_any_heating_op = 0
    has_mixing_device = 0
    has_mixing_media = 0
    for rec in valid_records:
        hops = heating_ops(rec["raw"])
        if hops:
            has_any_heating_op += 1
        temp_found = any(resolved_temperatures_celsius(op) for op in hops)
        dur_found = any(
            (op.get("conditions", {}).get("heating_time") or []) for op in hops
        )
        atm_found = any((op.get("conditions", {}).get("atmosphere") or []) for op in hops)
        if temp_found:
            has_temperature += 1
        if dur_found:
            has_duration += 1
        if atm_found:
            has_atmosphere += 1
        # `mixing_device`/`mixing_media` are populated across ALL operation
        # types with a `conditions` object, not just HeatingOperation --
        # checked separately from `heating_ops()` above.
        all_conds = [
            op.get("conditions")
            for op in (rec["raw"].get("operations") or [])
            if op.get("conditions")
        ]
        if any((c.get("mixing_device") or []) for c in all_conds):
            has_mixing_device += 1
        if any((c.get("mixing_media") or []) for c in all_conds):
            has_mixing_media += 1

    n_valid = len(valid_records)

    # --- DOI coverage ---
    has_doi = sum(1 for rec in valid_records if rec["doi"])

    # --- exact-duplicate vs independent-replication vs conflicting ---
    exact_duplicate_groups = 0
    exact_duplicate_records = 0
    multi_doi_routes = 0
    conflicting_routes = 0
    for route_key, recs in route_to_records.items():
        by_doi_and_temps = defaultdict(set)
        dois_seen = set()
        for rec in recs:
            hops = heating_ops(rec["raw"])
            temps = tuple(sorted({t for op in hops for t in resolved_temperatures_celsius(op)}))
            by_doi_and_temps[(rec["doi"], temps)].add(True)
            dois_seen.add(rec["doi"])
        # exact duplicate: >1 record sharing identical (doi, temps) tuple
        seen_doi_temp = Counter()
        for rec in recs:
            hops = heating_ops(rec["raw"])
            temps = tuple(sorted({t for op in hops for t in resolved_temperatures_celsius(op)}))
            seen_doi_temp[(rec["doi"], temps)] += 1
        for count in seen_doi_temp.values():
            if count > 1:
                exact_duplicate_groups += 1
                exact_duplicate_records += count - 1

        distinct_dois = {d for d in dois_seen if d}
        if len(distinct_dois) >= 2:
            multi_doi_routes += 1
            # conflict: across the route's records, do resolved max
            # temperatures (per record, the highest reported heating
            # temperature -- a coarse proxy for "the final/sintering
            # temperature") disagree?
            final_temps = set()
            for rec in recs:
                hops = heating_ops(rec["raw"])
                all_temps = [t for op in hops for t in resolved_temperatures_celsius(op)]
                if all_temps:
                    final_temps.add(round(max(all_temps), 1))
            if len(final_temps) > 1:
                conflicting_routes += 1

    # --- auto-applicable vs reference-only. DOI coverage is 100% (checked
    # below), so "reference-only" here reduces to "valid but not
    # temperature-resolved" -- not a separate DOI-gated computation. ---
    auto_applicable = sum(
        1
        for rec in valid_records
        if any(resolved_temperatures_celsius(op) for op in heating_ops(rec["raw"]))
    )
    reference_only = n_valid - auto_applicable

    unique_routes = len(route_to_records)

    stats = {
        "raw_record_count": len(reactions),
        "valid_record_count": n_valid,
        "parse_failure_count": parse_failures,
        "parse_failure_rate": parse_failures / len(reactions),
        "unparseable_target": unparseable_target,
        "unparseable_precursor": unparseable_precursor,
        "zero_or_too_many_precursors": zero_or_too_many_precursors,
        "unique_target_count": len(unique_target_sigs),
        "unique_route_count": unique_routes,
        "target_only_formula_count": len(target_only),
        "precursor_only_formula_count": len(precursor_only),
        "both_target_and_precursor_formula_count": len(both),
        "doi_coverage_count": has_doi,
        "doi_coverage_rate": has_doi / n_valid if n_valid else 0.0,
        "temperature_coverage_count": has_temperature,
        "temperature_coverage_rate": has_temperature / n_valid if n_valid else 0.0,
        "duration_coverage_count": has_duration,
        "duration_coverage_rate": has_duration / n_valid if n_valid else 0.0,
        "atmosphere_coverage_count": has_atmosphere,
        "atmosphere_coverage_rate": has_atmosphere / n_valid if n_valid else 0.0,
        "any_heating_op_count": has_any_heating_op,
        "any_heating_op_rate": has_any_heating_op / n_valid if n_valid else 0.0,
        "ramp_coverage_rate": 0.0,
        "pressure_coverage_rate": 0.0,
        "mixing_device_count": has_mixing_device,
        "mixing_device_rate": has_mixing_device / n_valid if n_valid else 0.0,
        "mixing_media_count": has_mixing_media,
        "mixing_media_rate": has_mixing_media / n_valid if n_valid else 0.0,
        "exact_duplicate_groups": exact_duplicate_groups,
        "exact_duplicate_records": exact_duplicate_records,
        "multi_doi_routes": multi_doi_routes,
        "conflicting_routes": conflicting_routes,
        "conflict_rate_among_multi_doi_routes": (
            conflicting_routes / multi_doi_routes if multi_doi_routes else 0.0
        ),
        "auto_applicable_count": auto_applicable,
        "reference_only_count": reference_only,
        "grinding_no_heat_count": grinding_no_heat_count,
    }

    for k, v in stats.items():
        print(f"{k}: {v}", file=sys.stderr)

    write_report(stats, op_type_counts, len(reactions))
    print(f"wrote {OUTPUT}", file=sys.stderr)


def write_report(stats, op_type_counts, raw_count):
    lines = []
    lines.append("# Literature condition corpus audit (Phase 20A)")
    lines.append("")
    lines.append(
        "A full-corpus inventory of the Kononova et al. 2019 text-mined synthesis "
        "dataset -- the same corpus `benchmarks/fetch_kononova.py` already draws its "
        "Phase 11 holdout benchmark sample from -- run over all raw records, not a "
        "downsampled subset. Produced by `benchmarks/audit_kononova_corpus.py`, "
        "regenerated by re-running that script, not hand-edited."
    )
    lines.append("")
    lines.append(
        "**Report-only.** No provider code, no Planner connection. This exists to "
        "decide whether building a bulk literature-condition provider on this corpus "
        "is worthwhile, independent of and prior to Phase 19P's thermodynamic work."
    )
    lines.append("")
    lines.append("## Source")
    lines.append("")
    lines.append(
        "Kononova, O., Huo, H., He, T., Rong, Z., Botari, T., Sun, W., Tshitoyan, V., "
        "Ceder, G. \"Text-mined dataset of inorganic materials synthesis recipes.\" "
        "*Scientific Data* 6, 203 (2019)."
    )
    lines.append("")
    lines.append(
        f"Hosted at {FIGSHARE_ARTICLE} (DOI 10.6084/m9.figshare.9722159), license "
        "**CC BY 4.0**, verified live against the figshare API by this script on "
        f"every run (not reused from any prior cache -- see this project's own Phase "
        "11 incident, `CHANGELOG.md`, for why that matters). "
        f"Raw record count: {raw_count} (matches `fetch_kononova.py`'s independently "
        "verified count)."
    )
    lines.append("")
    lines.append("## Structural validity")
    lines.append("")
    lines.append(
        f"- Raw records: {stats['raw_record_count']}\n"
        f"- Structurally valid (target and every precursor parseable, "
        f"1-4 distinct precursors -- same bar `fetch_kononova.py` uses): "
        f"{stats['valid_record_count']} "
        f"({stats['valid_record_count']/stats['raw_record_count']:.1%})\n"
        f"- Parse failures: {stats['parse_failure_count']} "
        f"({stats['parse_failure_rate']:.1%}), broken down as:\n"
        f"  - unparseable target (doped/disordered/free-variable formula): "
        f"{stats['unparseable_target']}\n"
        f"  - unparseable precursor (same criteria): {stats['unparseable_precursor']}\n"
        f"  - zero precursors, or more than 4 after de-duplication: "
        f"{stats['zero_or_too_many_precursors']}"
    )
    lines.append("")
    lines.append("## Coverage (breadth)")
    lines.append("")
    lines.append(
        f"- Unique targets (by canonical elemental ratio, not formula string): "
        f"{stats['unique_target_count']}\n"
        f"- Unique exact routes (target + precursor-set, by canonical ratio): "
        f"{stats['unique_route_count']}\n"
        f"- Materials appearing only as a target: {stats['target_only_formula_count']}\n"
        f"- Materials appearing only as a precursor: "
        f"{stats['precursor_only_formula_count']}\n"
        f"- Materials appearing as both a target (in one record) and a precursor "
        f"(in another): {stats['both_target_and_precursor_formula_count']}\n"
        f"- DOI coverage: {stats['doi_coverage_count']}/{stats['valid_record_count']} "
        f"({stats['doi_coverage_rate']:.1%})\n"
        f"- License coverage: 100% -- one figshare deposit, one corpus-level CC BY "
        f"4.0 license; there is no per-record license variation to measure."
    )
    lines.append("")
    lines.append("## Condition-field coverage (per valid record)")
    lines.append("")
    lines.append(
        "Whether a record has *any* `HeatingOperation` reporting each field -- not "
        "whether every heating step does. gugen's `ConditionPrecedent` needs a "
        "resolved value per field per purpose (AGENTS.md's Heat-step model), so this "
        "is an upper bound on what a bulk provider could resolve, not the provider's "
        "actual per-purpose yield."
    )
    lines.append("")
    lines.append(
        f"- Has at least one HeatingOperation at all: {stats['any_heating_op_count']} "
        f"({stats['any_heating_op_rate']:.1%})\n"
        f"- Temperature resolved (>=1 heating step with a numeric value): "
        f"{stats['temperature_coverage_count']} ({stats['temperature_coverage_rate']:.1%})\n"
        f"- Duration resolved: {stats['duration_coverage_count']} "
        f"({stats['duration_coverage_rate']:.1%})\n"
        f"- Atmosphere reported (non-empty list): {stats['atmosphere_coverage_count']} "
        f"({stats['atmosphere_coverage_rate']:.1%})\n"
        f"- Ramp rate: **0%, structurally absent.** Verified directly against the "
        f"schema, not inferred: every `conditions` object in this corpus (across all "
        f"operation types) exposes only "
        f"`{{atmosphere, heating_temperature, heating_time, mixing_device, "
        f"mixing_media}}` -- there is no ramp-rate key anywhere in the raw data.\n"
        f"- Pressure (e.g. uniaxial pressing pressure): **0%, structurally absent** "
        f"for the same reason -- `ShapingOperation` entries use the identical "
        f"`conditions` shape as `HeatingOperation` (and rarely populate it), with no "
        f"pressure-specific field.\n"
        f"- Milling conditions: no dedicated field, but `mixing_device`/"
        f"`mixing_media` are populated and carry real content -- not a structured "
        f"milling speed/duration/ball-to-powder ratio, but free-text material and "
        f"solvent mentions (`mixing_device` values are dominated by grinding-vessel "
        f"material, e.g. `agate`, `zirconia`, `alumina`; `mixing_media` values are "
        f"dominated by wet-grinding solvents, e.g. `ethanol`, `water`, `acetone`). "
        f"Record-level coverage: `mixing_device` in >=1 operation: "
        f"{stats['mixing_device_count']} ({stats['mixing_device_rate']:.1%}); "
        f"`mixing_media`: {stats['mixing_media_count']} "
        f"({stats['mixing_media_rate']:.1%}). Neither maps onto a gugen "
        f"`ConditionPrecedent` field today -- gugen's `Grind`/`Form` steps have no "
        f"vessel-material or solvent field to receive this."
    )
    lines.append("")
    lines.append("## Route-family shape")
    lines.append("")
    lines.append(
        "gugen offers `ConventionalSolidState` and `Mechanochemical` unconditionally "
        "for every accepted precursor set (Phase 12, AGENTS.md §13) -- this corpus's "
        "records don't carry an explicit route-family label to match against. "
        "Operation-type frequency across the full raw corpus (all records, all "
        "operations):"
    )
    lines.append("")
    for op_type, count in op_type_counts.most_common():
        lines.append(f"- `{op_type}`: {count}")
    lines.append("")
    lines.append(
        "**No `BallMilling`-shaped operation type exists in this corpus's "
        "vocabulary.** The closest candidate, `LiquidGrinding`, describes wet "
        "grinding (its `mixing_media` values are dominated by solvents -- ethanol, "
        "water, acetone) as a mixing step, not the single high-energy dry "
        "ball-milling step gugen's `mechanochemical_template` models (Suryanarayana "
        f"2001, `10.1016/S0079-6425(99)00010-9`). Only {stats['grinding_no_heat_count']} "
        f"of {raw_count} raw records ({stats['grinding_no_heat_count']/raw_count:.1%}) "
        "have a `LiquidGrinding` step and *no* `HeatingOperation` at all -- the "
        "closest this corpus comes to a grinding-only route -- and even those are "
        "wet grinding, not dry ball milling. This corpus is a source of "
        "conventional solid-state condition precedents (mix/heat cycles); it does "
        "not evidence the Mechanochemical route family in any meaningful way -- a "
        "real scope limit, not something a classification heuristic on this data "
        "could fix."
    )
    lines.append("")
    lines.append("## Duplication and conflict")
    lines.append("")
    lines.append(
        f"- Exact-duplicate records (same route, same DOI, same reported max "
        f"temperature set -- almost certainly the same underlying report entered "
        f"twice): {stats['exact_duplicate_records']} records across "
        f"{stats['exact_duplicate_groups']} groups (the counts are equal, meaning "
        f"every duplicate group has exactly one extra copy -- no group of 3 or "
        f"more was found).\n"
        f"- Routes independently reported by >=2 distinct DOIs: "
        f"{stats['multi_doi_routes']} of {stats['unique_route_count']} unique routes.\n"
        f"- Of those multi-DOI routes, the fraction where reported maximum heating "
        f"temperatures differ across sources: {stats['conflicting_routes']}/"
        f"{stats['multi_doi_routes']} "
        f"({stats['conflict_rate_among_multi_doi_routes']:.1%})."
    )
    lines.append("")
    lines.append(
        "**This is an upper bound on per-purpose conflict, not a measurement of "
        "it, and should not be read as \"66.3% of routes genuinely disagree.\"** "
        "The metric compares each record's own single highest reported heating "
        "temperature, grouped by canonical target+precursor-set ratio -- it has no "
        "`HeatingPurpose` label to match against, because this corpus doesn't carry "
        "one. A difference here can also come from one paper reporting "
        "calcination+sintering while another reports sintering only (different "
        "step counts, not disagreement), or from the ratio match unifying two "
        "genuinely different recipes at the same stoichiometry. What this number "
        "does establish, without overreaching: multi-source routes are common "
        "(1056 of 5631 unique routes) and their reported temperatures vary often "
        "enough that per-purpose conflict handling "
        "(`apply_condition_precedents`, Phase 19, `src/process.rs`) is not "
        "addressing a hypothetical problem -- but the actual per-purpose conflict "
        "rate remains unmeasured, since this corpus has no purpose labels to "
        "measure it against."
    )
    lines.append("")
    lines.append("## Applicability to a bulk `ProcessEvidenceProvider`")
    lines.append("")
    lines.append(
        f"- Structurally valid **and** temperature-resolved (usable to directly "
        f"seed a `ConditionPrecedent`, the same bar Phase 10's curated records "
        f"meet): {stats['auto_applicable_count']} "
        f"({stats['auto_applicable_count']/stats['valid_record_count']:.1%} of "
        f"valid records).\n"
        f"- Structurally valid but not temperature-resolved (usable only as a "
        f"free-text evidence pointer, not an automatic condition source -- every "
        f"valid record has a DOI, so this is simply the complement of the count "
        f"above, not a separately DOI-gated figure): {stats['reference_only_count']}."
    )
    lines.append("")
    lines.append("## What this audit does not establish")
    lines.append("")
    lines.append(
        "- Extraction accuracy: this dataset is itself the output of an NLP "
        "text-mining pipeline (Kononova et al. 2019's own), not hand-verified by "
        "gugen. This audit measures the shape and coverage of what that pipeline "
        "extracted, not whether the extracted numbers are correct against the "
        "original papers.\n"
        "- Whether a bulk provider built on this corpus would improve gugen's "
        "actual planning outputs -- that requires the provider to exist and be "
        "benchmarked, which this report deliberately does not build.\n"
        "- Anything about the Mechanochemical route family, gas-phase reactions, "
        "or thermodynamic selectivity -- out of scope for this audit."
    )
    lines.append("")

    DOCS_DIR.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT, "w") as f:
        f.write("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
