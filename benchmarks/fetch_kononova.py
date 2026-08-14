#!/usr/bin/env python3
"""One-time, reproducible fetch + filter + downsample of the Kononova et
al. 2019 text-mined synthesis dataset (CC BY 4.0, license verified live
via the figshare API) into a small holdout benchmark corpus for Phase 11
(AGENTS.md §21/§23/§27: comparison code isolated to benchmarks/, never a
production dependency; holdout must not overfit to what gugen already
knows).

Run: python3 benchmarks/fetch_kononova.py
     python3 benchmarks/fetch_kononova.py --local /path/to/cached.json  (dev iteration, skips
       the 91MB download; the license check still runs. The local copy MUST be byte-identical
       to this script's own download from FIGSHARE_ARTICLE's download_url -- verify with a real
       fetch first. A file from any other source, including a differently-dated snapshot of the
       "same" dataset from the authors' own GitHub repo, is a DIFFERENT, unlicensed object; using
       one here was a real bug this script shipped with once, caught by actually running the live
       path -- see tasks/todo.md's Phase 11 §28 report.)
Output: benchmarks/data/kononova_sample.jsonl, benchmarks/data/ATTRIBUTION.md

Filter criteria (defined before looking at gugen's results on this data,
per AGENTS.md §27 "benchmarkを見てholdoutへ過適合しない"):
  - target formula must be parseable: no free variables (amounts_vars /
    elements_vars empty, exactly one composition entry) and every element
    amount a plain positive number.
  - every precursor's composition must be parseable the same way.
  - 1-4 distinct precursors after dedup (gugen's own
    SearchBudget::default().max_precursors_per_plan is 4, so a route
    needing more could never be found by the default planner regardless
    of catalog).
  - excludes any (target, precursor-set) pair -- matched by normalized
    elemental ratio, not formula string -- already used by
    tests/validation.rs's 5 fixtures or Phase 10's
    src/literature_conditions.rs curated records (6 routes total; see
    EXCLUDED_ROUTES below). This is the concrete leakage-prevention
    mechanism. Several of these routes are independently reported by
    dozens of DOIs in this same corpus (LaAlO3: 10, MgAl2O4: 16, BaTiO3:
    83, Zn3(PO4)2/ZnO+P2O5: 0 -- recounted directly against this exact
    corpus during Phase 11; tests/validation.rs's own citation text
    originally stated different, higher counts, traced to a
    different-provenance file, corrected in Phase 14; see
    tasks/todo.md's Phase 11 and Phase 14 sections), so excluding only
    the one "representative" DOI per route would leave near-duplicate
    leaked entries in the holdout set. Ratio
    normalization (not raw formula-unit scale) also correctly matches a
    route reported at a different
    formula-unit scale than the one gugen's fixtures happen to use.
  - deterministic fixed-seed downsample to a manageable size.

Every exclusion reason is counted and reported (stderr + ATTRIBUTION.md),
never silently dropped.
"""

import argparse
import json
import random
import ssl
import sys
import urllib.request
from pathlib import Path

try:
    # Some Python installs (notably python.org macOS builds without the
    # bundled "Install Certificates" step run) ship no usable CA trust
    # store for urllib's default SSL context. certifi's bundle is a
    # correct, standard fallback -- not a verification bypass. Optional:
    # falls back to urllib's own default context (fine on most Linux CI
    # images) if certifi isn't installed.
    import certifi

    _SSL_CONTEXT = ssl.create_default_context(cafile=certifi.where())
except ImportError:
    _SSL_CONTEXT = None


def _urlopen(url):
    return urllib.request.urlopen(url, context=_SSL_CONTEXT)

FIGSHARE_ARTICLE = "https://api.figshare.com/v2/articles/9722159"
# The dataset's own top level is a bare JSON list (not, e.g., a dict with
# a "reactions" key) -- verified by actually downloading and inspecting
# it, not assumed. 19,488 is what this exact figshare file
# (solid-state_dataset_2019-06-27_upd.json, version 3) contains, verified
# by downloading and counting it directly. The paper's own headline
# figure (~19,744, Kononova et al. 2019, Scientific Data 6, 203) differs
# slightly; the reason for that gap is not established here -- stated as
# an open discrepancy, not explained away with an unverified guess.
EXPECTED_REACTION_COUNT = 19488
SAMPLE_SIZE = 1500
SEED = 20260814  # fixed, arbitrary -- date this phase's corpus was drawn
MAX_PRECURSORS = 4
DATA_DIR = Path(__file__).parent / "data"
OUTPUT = DATA_DIR / "kononova_sample.jsonl"
ATTRIBUTION = DATA_DIR / "ATTRIBUTION.md"


def canonical_ratio(elements):
    """Scale-invariant signature: divide every amount by the
    composition's own minimum amount, round to tolerate float noise, sort
    by element. Two compositions with the same elemental ratio produce
    the same signature regardless of formula-unit scale or which paper's
    convention wrote it -- unlike gugen's own `Composition::PartialEq`,
    which is exact-scale only (a documented gap, ROADMAP.md)."""
    amounts = {el: float(amt) for el, amt in elements.items()}
    if not amounts:
        return None
    m = min(amounts.values())
    if m <= 0:
        return None
    return tuple(sorted((el, round(amt / m, 4)) for el, amt in amounts.items()))


def route_key(target_elements, precursor_elements_list):
    target_sig = canonical_ratio(target_elements)
    precursor_sigs = frozenset(
        sig for p in precursor_elements_list if (sig := canonical_ratio(p)) is not None
    )
    return (target_sig, precursor_sigs)


# The 6 routes used by tests/validation.rs (5 fixtures) and
# src/literature_conditions.rs (Phase 10's curated records) as of Phase 11,
# when this corpus was generated -- read directly from those files at that
# time, not retyped from memory. Zn3(PO4)2 appears twice: the
# validation.rs route (ZnO + P2O5) and Phase 10's different, also-real
# substitute route (ZnO + (NH4)2HPO4).
#
# NOT kept in sync automatically: Phase 14 replaced tests/validation.rs's
# Zn3(PO4)2/ZnO+P2O5 fixture with a different target (LiFePO4/FePO4+Li2CO3,
# 6 independent DOIs in this corpus) without re-running this script or
# regenerating benchmarks/data/kononova_sample.jsonl (a large-diff,
# deterministic-reshuffle change out of scope for that phase's small
# evidence/wording fix). Checked directly that this specific gap is
# currently harmless -- the committed sample has zero rows matching the
# new route exactly -- but if this script is ever re-run, add
# `route_key({"Li":1,"Fe":1,"P":1,"O":4}, [{"Fe":1,"P":1,"O":4},
# {"Li":2,"C":1,"O":3}])` to EXCLUDED_ROUTES first (and update
# tests/large_scale_benchmark.rs's own mirror of this list to match).
EXCLUDED_ROUTES = frozenset(
    [
        route_key({"La": 1, "Al": 1, "O": 3}, [{"La": 2, "O": 3}, {"Al": 2, "O": 3}]),
        route_key({"Mg": 1, "Al": 2, "O": 4}, [{"Mg": 1, "O": 1}, {"Al": 2, "O": 3}]),
        route_key({"Zn": 3, "P": 2, "O": 8}, [{"Zn": 1, "O": 1}, {"P": 2, "O": 5}]),
        route_key({"Ca": 1, "O": 1}, [{"Ca": 1, "C": 1, "O": 3}]),
        route_key(
            {"Ba": 1, "Ti": 1, "O": 3}, [{"Ba": 1, "C": 1, "O": 3}, {"Ti": 1, "O": 2}]
        ),
        route_key(
            {"Zn": 3, "P": 2, "O": 8},
            [{"Zn": 1, "O": 1}, {"N": 2, "H": 9, "P": 1, "O": 4}],
        ),
    ]
)


def is_plain_positive_number(x):
    try:
        return float(x) > 0
    except (TypeError, ValueError):
        return False


def parseable_composition(material):
    """Returns a plain {element: float amount} dict if this material has
    no free variables, exactly one unambiguous composition entry, and
    every amount is a plain positive number; None otherwise (a
    doped/solid-solution/disordered formula gugen's `Composition::new`
    could not accept)."""
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


def fetch_dataset(local_path):
    # License is always checked live against the figshare API, even with
    # --local, so a local dev copy can never mask a license change.
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
        print(
            f"downloading {files[0]['name']} ({files[0]['size']} bytes)", file=sys.stderr
        )
        with _urlopen(download_url) as resp:
            data = json.load(resp)

    # The dataset's top level IS the reaction list -- no wrapping dict.
    # Deliberately not written as an isinstance(data, dict) branch that
    # also accepts a {"reactions": [...]} shape: that permissiveness is
    # exactly what let a wrong-provenance file (a differently-shaped,
    # differently-sized snapshot from an unlicensed source) pass through
    # undetected during this script's own development. A shape mismatch
    # here must fail loudly, not be silently accommodated.
    reactions = data
    if len(reactions) != EXPECTED_REACTION_COUNT:
        sys.exit(
            f"REFUSING: expected {EXPECTED_REACTION_COUNT} reactions, got "
            f"{len(reactions)} -- dataset may have changed, re-verify before proceeding"
        )
    return reactions


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--local",
        help="path to an already-downloaded copy of the dataset JSON (dev iteration only; "
        "the license is still checked live against the figshare API)",
    )
    args = parser.parse_args()

    reactions = fetch_dataset(args.local)
    print(f"{len(reactions)} raw reactions", file=sys.stderr)

    stats = {
        "unparseable_target": 0,
        "unparseable_precursor": 0,
        "zero_or_too_many_precursors": 0,
        "excluded_leakage": 0,
    }
    pool = []

    for r in reactions:
        target = parseable_composition(r["target"])
        if target is None:
            stats["unparseable_target"] += 1
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
            stats["unparseable_precursor"] += 1
            continue
        # De-duplicate identical precursor formulas within one reaction.
        precursor_comps = list({formula: comp for formula, comp in precursor_comps}.items())
        if len(precursor_comps) == 0 or len(precursor_comps) > MAX_PRECURSORS:
            stats["zero_or_too_many_precursors"] += 1
            continue
        key = route_key(target, [c for _, c in precursor_comps])
        if key in EXCLUDED_ROUTES:
            stats["excluded_leakage"] += 1
            continue
        pool.append(
            {
                "doi": r.get("doi"),
                "target_formula": r["target"].get("material_formula"),
                "target_elements": target,
                "precursors": [
                    {"formula": formula, "elements": comp}
                    for formula, comp in precursor_comps
                ],
            }
        )

    for reason, count in stats.items():
        print(f"excluded ({reason}): {count}", file=sys.stderr)
    print(f"eligible pool: {len(pool)}", file=sys.stderr)

    rng = random.Random(SEED)
    sample = pool if len(pool) <= SAMPLE_SIZE else rng.sample(pool, SAMPLE_SIZE)
    sample.sort(key=lambda r: (r["doi"] or "", r["target_formula"] or ""))

    DATA_DIR.mkdir(parents=True, exist_ok=True)
    with open(OUTPUT, "w") as f:
        for r in sample:
            f.write(json.dumps(r, sort_keys=True) + "\n")
    print(f"wrote {len(sample)} reactions to {OUTPUT}", file=sys.stderr)

    with open(ATTRIBUTION, "w") as f:
        f.write(
            "# Attribution: benchmarks/data/kononova_sample.jsonl\n\n"
            "Derived from: Kononova, O., Huo, H., He, T., Rong, Z., Botari, T., Sun, W., "
            "Tshitoyan, V., Ceder, G. \"Text-mined dataset of inorganic materials synthesis "
            "recipes.\" *Scientific Data* 6, 203 (2019).\n\n"
            f"Hosted at {FIGSHARE_ARTICLE} (DOI 10.6084/m9.figshare.9722159), "
            "license **CC BY 4.0** (https://creativecommons.org/licenses/by/4.0/), "
            "verified live against the figshare API by this script on every run.\n\n"
            "## How this sample was generated\n\n"
            f"`python3 benchmarks/fetch_kononova.py` (seed {SEED}), filtering "
            f"{len(reactions)} raw reactions down to {len(pool)} eligible entries, then a "
            f"deterministic downsample to {len(sample)}.\n\n"
            "Exclusion counts (each reaction excluded for exactly one reason, in this "
            "check order):\n\n"
            f"- Unparseable target (free-variable/doped/disordered formula, or a "
            f"non-numeric/non-positive amount): {stats['unparseable_target']}\n"
            f"- Unparseable precursor (same criteria, any precursor): "
            f"{stats['unparseable_precursor']}\n"
            f"- Zero precursors, or more than gugen's default "
            f"`SearchBudget::max_precursors_per_plan` (4) after de-duplication: "
            f"{stats['zero_or_too_many_precursors']}\n"
            f"- Leakage against a route already used by `tests/validation.rs` or "
            f"`src/literature_conditions.rs`'s curated records (matched by normalized "
            f"elemental ratio, not formula string or DOI -- several of these routes are "
            f"independently reported by dozens of DOIs in this corpus, so DOI-only "
            f"exclusion would have left near-duplicates in the holdout set): "
            f"{stats['excluded_leakage']}\n\n"
            "This file and `kononova_sample.jsonl` are regenerated by re-running the "
            "script above, not hand-edited.\n"
        )
    print(f"wrote {ATTRIBUTION}", file=sys.stderr)


if __name__ == "__main__":
    main()
