#!/usr/bin/env python3
"""Phase 28 gate criterion #1: independently re-verifies that
benchmarks/data/exploration_recall_manifest.json contains zero rows
matching a route already used by tests/validation.rs's curated fixtures
or src/literature_conditions.rs's curated records.

exploration_build_recall_manifest.py already excludes leaked rows by
construction (see its own docstring) using the real element amounts it
resolves per row. This script re-runs the identical check as an
independent pass against the *committed* manifest -- using the same
real `target_amounts`/`route_amounts` the manifest already persists per
row, not a weaker re-parse of formula strings (an earlier version of
this script did that and produced a false positive on a polymorph-
prefixed formula the bounded parser couldn't handle; re-deriving from
strings when real amounts are already on hand was the bug, not the
leakage logic itself).

Run: python3 benchmarks/exploration_check_split_leakage.py
Exit code 0 and "LEAKAGE-CLEAN" iff every fully-resolved manifest row
was checked and none matched. Exit code 1 on any match, or if the
`leakage_unchecked_pairs` fraction the manifest itself reports is above
this script's own trust threshold.
"""

import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from exploration_build_recall_manifest import MANIFEST_PATH  # noqa: E402
from fetch_kononova import EXCLUDED_ROUTES, route_key  # noqa: E402

# If more than this fraction of pairs can't be fully resolved to real
# amounts (per the manifest's own leakage_unchecked_pairs count), the
# "leakage-clean" claim isn't trustworthy enough to gate on -- report
# and fail loudly rather than silently passing on a mostly-unchecked
# manifest.
MAX_UNCHECKED_FRACTION = 0.05


def is_fully_resolved(row):
    return row["target_amounts"] is not None and all(
        a is not None for a in row["route_amounts"]
    )


def main():
    manifest = json.loads(MANIFEST_PATH.read_text())
    rows = manifest["rows"]
    total = len(rows)

    checked = 0
    unchecked = 0
    leaked_rows = []
    for row in rows:
        if not is_fully_resolved(row):
            unchecked += 1
            continue
        checked += 1
        key = route_key(row["target_amounts"], row["route_amounts"])
        if key in EXCLUDED_ROUTES:
            leaked_rows.append(row)

    unchecked_fraction = unchecked / total if total else 0.0
    print(
        f"{total} row(s): {checked} fully re-checked against EXCLUDED_ROUTES, "
        f"{unchecked} not fully resolvable to real amounts ({unchecked_fraction:.1%})",
        file=sys.stderr,
    )

    if leaked_rows:
        for row in leaked_rows:
            print(
                f"LEAKAGE: target={row['target_formula']!r} route={row['route']!r} "
                "matches an EXCLUDED_ROUTES entry",
                file=sys.stderr,
            )
        print(f"FAILED: {len(leaked_rows)} leaked row(s)", file=sys.stderr)
        sys.exit(1)

    if unchecked_fraction > MAX_UNCHECKED_FRACTION:
        print(
            f"FAILED: {unchecked_fraction:.1%} of rows could not be fully resolved "
            f"to real amounts, above the {MAX_UNCHECKED_FRACTION:.0%} trust "
            "threshold -- investigate before trusting this manifest as "
            "leakage-clean",
            file=sys.stderr,
        )
        sys.exit(1)

    print("LEAKAGE-CLEAN", file=sys.stderr)


if __name__ == "__main__":
    main()
