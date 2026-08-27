#!/usr/bin/env python3
"""Phase 21B calibration: builds the input file for the Rust harness
(`examples/exploration_phase21b_calibration.rs`) that actually calls
gugen's own `balance()` / `balanced_reaction_delta_ev_per_atom`.

Restricts `thermodynamic_selectivity_clean_population.json` to rows
whose target is one of condition 1's 273 gate-passing targets, whose
target and every route formula parses via `parse_flat_formula` (see
docs/phase21b_calibration_preregistration.md for why only the flat
case is supported), and whose target and every route formula has a
matched OQMD entry in `oqmd_coverage_manifest.json`. Everything else
this script does is pure data assembly -- no reaction is balanced and
no energy is computed here (that needs gugen's own Rust code); this
script's only job is handing the Rust harness exactly the element
amounts and OQMD data it needs, so the harness itself never has to
parse a formula string.

Run: python3 benchmarks/build_phase21b_calibration_input.py
Writes: benchmarks/data/phase21b_calibration_input.json
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from parse_flat_formula import parse_flat_formula  # noqa: E402

DATA_DIR = Path(__file__).parent / "data"
GATE_RESULT_PATH = DATA_DIR / "oqmd_coverage_gate_result.json"
POPULATION_PATH = DATA_DIR / "thermodynamic_selectivity_clean_population.json"
MANIFEST_PATH = DATA_DIR / "oqmd_coverage_manifest.json"
OUTPUT_PATH = DATA_DIR / "phase21b_calibration_input.json"


def is_covered(coverage, formula):
    entry = coverage.get(formula)
    return bool(entry and entry.get("matched"))


def main():
    gate = json.loads(GATE_RESULT_PATH.read_text())
    passing = set(gate["route_pair_gate"]["passing_targets"])
    population = json.loads(POPULATION_PATH.read_text())
    manifest = json.loads(MANIFEST_PATH.read_text())
    coverage = manifest["coverage"]

    rows = [r for r in population if r["target"] in passing]

    counts = {
        "candidate_rows": len(rows),
        "excluded_nonflat": 0,
        "excluded_uncovered": 0,
        "usable_rows": 0,
    }

    out_rows = []
    needed_formulas = set()
    for r in rows:
        target = r["target"]
        route = r["route"]
        target_parsed = parse_flat_formula(target)
        route_parsed = [parse_flat_formula(f) for f in route]
        if target_parsed is None or any(p is None for p in route_parsed):
            counts["excluded_nonflat"] += 1
            continue
        all_formulas = [target] + route
        if not all(is_covered(coverage, f) for f in all_formulas):
            counts["excluded_uncovered"] += 1
            continue
        counts["usable_rows"] += 1
        needed_formulas.update(all_formulas)
        out_rows.append(
            {
                "target_formula": target,
                "target_elements": target_parsed,
                "route": [
                    {"formula": f, "elements": p} for f, p in zip(route, route_parsed)
                ],
                "verdict": r["verdict"],
                "dois": r["dois"],
            }
        )

    thermo_entries = {}
    for f in sorted(needed_formulas):
        entry = coverage[f]
        thermo_entries[f] = {
            "delta_e_ev_per_atom": entry["delta_e_ev_per_atom"],
            "volume_angstrom3_per_atom": entry["volume_angstrom3_per_atom"],
        }

    output = {
        "description": (
            "Phase 21B calibration input -- rows restricted to condition 1's "
            "273 gate-passing targets, flat-formula-parseable, OQMD-covered. "
            "No reaction has been balanced and no energy computed here; see "
            "examples/exploration_phase21b_calibration.rs for that."
        ),
        "source_manifest_checksum": manifest.get("coverage_snapshot_sha256"),
        "counts": counts,
        "rows": out_rows,
        "thermodynamic_entries": thermo_entries,
    }
    OUTPUT_PATH.write_text(json.dumps(output, indent=2, sort_keys=True))

    print(f"candidate rows (target in 273 passing): {counts['candidate_rows']}")
    print(f"excluded (non-flat formula): {counts['excluded_nonflat']}")
    print(f"excluded (flat but not fully OQMD-covered): {counts['excluded_uncovered']}")
    print(f"usable rows handed to the Rust harness: {counts['usable_rows']}")
    print(f"distinct formulas with thermodynamic data: {len(thermo_entries)}")
    print(f"wrote {OUTPUT_PATH}")


if __name__ == "__main__":
    main()
