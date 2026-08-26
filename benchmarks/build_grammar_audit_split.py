#!/usr/bin/env python3
"""Phase 31 PR 3: a deterministic, DOI-grouped development/evaluation
split of `benchmarks/data/kononova_high_arity_sample.jsonl` (the 408-row
high-arity holdout from Phase 31 PR 2), committed *before* any
transformation-grammar rule is written.

Why a DOI-grouped split, not a row-level split: 34 DOIs in this corpus
contribute more than one row (e.g. two NdSiAlON/YbSiAlON rows share DOI
10.1016/j.jmatprotec.2006.10.015). A row-level random split could put
two rows from the same paper on opposite sides, leaking that paper's
precursor/target chemistry (and any grammar pattern derived from it)
across the development/evaluation boundary. Splitting whole DOI groups
closes that leak.

Why commit this before grammar code exists: the owner's explicit
instruction is that grammar rules must be designed by looking only at
the development side, and the evaluation side's individual rows must
not be inspected before results are finalized. Committing the split
assignment (with a source-file checksum) before any grammar rule is
written makes that ordering independently verifiable from git history,
not just asserted in prose.

Split method: sort DOI groups deterministically (by DOI string), shuffle
with a fixed-seed `random.Random`, then greedily assign each group to
whichever side currently has fewer rows -- a simple, deterministic
balanced bin-packing that does not depend on group iteration order
beyond the seeded shuffle. Target is an even row-count split (not an
even DOI-count split, since group sizes vary 1-4).

Run: python3 benchmarks/build_grammar_audit_split.py
Output: benchmarks/data/exploration_grammar_split_manifest.json
"""

import hashlib
import json
import random
from collections import Counter, defaultdict
from pathlib import Path

DATA_DIR = Path(__file__).parent / "data"
SOURCE = DATA_DIR / "kononova_high_arity_sample.jsonl"
OUTPUT = DATA_DIR / "exploration_grammar_split_manifest.json"
SEED = 31  # matches the phase number; fixed and committed, never re-rolled

# Three rows were already inspected in detail during Phase 31 PR 2 (hand-traced
# route construction, one used to motivate this PR's grammar D design). Wherever
# this deterministic split lands them, that is disclosed here rather than hidden
# or used to justify moving them -- moving a row to keep it on the "development"
# side would be tuning the split around already-seen answers, which defeats the
# point of a held-out evaluation side. See docs/phase31_pr3_transformation_grammar_audit.md.
PREVIOUSLY_INSPECTED_DOIS = {
    "10.1016/j.tca.2014.08.028": "hand-traced in PR 2 (SiO2P2O5K2OMgOCaO route); "
    "directly motivated this PR's grammar D (acid+carbonate phosphate-type salt formation)",
    "10.1016/j.jmatprotec.2006.10.015": "named in PR 2's Discovered Work section "
    "(NdSiAlON/YbSiAlON search-defect rows, fixed in PR 78)",
    "10.1016/j.materresbull.2014.01.009": "named in PR 2's Discovered Work section "
    "(Na0.5Bi0.5Cu3Ti4O12 search-defect row, fixed in PR 78)",
}


def main():
    raw = SOURCE.read_bytes()
    checksum = hashlib.sha256(raw).hexdigest()

    rows = [json.loads(line) for line in raw.decode("utf-8").splitlines() if line.strip()]

    by_doi = defaultdict(list)
    for i, row in enumerate(rows):
        by_doi[row.get("doi") or f"__no_doi_row_{i}"].append(i)

    dois = sorted(by_doi.keys())
    rng = random.Random(SEED)
    rng.shuffle(dois)

    dev_rows, eval_rows = [], []
    dev_dois, eval_dois = [], []
    for doi in dois:
        idxs = by_doi[doi]
        if len(dev_rows) <= len(eval_rows):
            dev_rows.extend(idxs)
            dev_dois.append(doi)
        else:
            eval_rows.extend(idxs)
            eval_dois.append(doi)

    dev_rows.sort()
    eval_rows.sort()

    def arity_breakdown(idxs):
        return dict(sorted(Counter(len(rows[i]["precursors"]) for i in idxs).items()))

    dev_set = set(dev_rows)
    contamination = []
    for i, row in enumerate(rows):
        doi = row.get("doi")
        if doi in PREVIOUSLY_INSPECTED_DOIS:
            contamination.append(
                {
                    "row_index": i,
                    "doi": doi,
                    "target_formula": row.get("target_formula"),
                    "side": "development" if i in dev_set else "evaluation",
                    "note": PREVIOUSLY_INSPECTED_DOIS[doi],
                }
            )

    manifest = {
        "description": (
            "Phase 31 PR 3 -- deterministic DOI-grouped development/evaluation split of "
            "kononova_high_arity_sample.jsonl, committed before any transformation-grammar "
            "rule was written. Grammar design may inspect development rows in detail; "
            "evaluation rows must not be inspected before results are finalized."
        ),
        "source_file": "benchmarks/data/kononova_high_arity_sample.jsonl",
        "source_sha256": checksum,
        "split_rule": (
            "group rows by DOI (rows with no DOI form their own singleton group); sort "
            "group keys; shuffle with random.Random(seed); greedily assign each whole "
            "group to whichever side currently has fewer rows"
        ),
        "seed": SEED,
        "total_rows": len(rows),
        "total_dois": len(dois),
        "development": {
            "row_count": len(dev_rows),
            "doi_count": len(dev_dois),
            "row_indices": dev_rows,
            "arity_breakdown": arity_breakdown(dev_rows),
        },
        "evaluation": {
            "row_count": len(eval_rows),
            "doi_count": len(eval_dois),
            "row_indices": eval_rows,
            "arity_breakdown": arity_breakdown(eval_rows),
        },
        "known_pre_split_contamination": contamination,
    }

    OUTPUT.write_text(json.dumps(manifest, indent=2) + "\n")
    print(f"source: {SOURCE} sha256={checksum}")
    print(f"development: {len(dev_rows)} rows / {len(dev_dois)} DOIs")
    print(f"evaluation:  {len(eval_rows)} rows / {len(eval_dois)} DOIs")
    print(f"wrote {OUTPUT}")


if __name__ == "__main__":
    main()
