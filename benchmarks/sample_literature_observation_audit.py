#!/usr/bin/env python3
"""Phase 20D: draws a reproducible, DOI-independent sample of
`LiteratureObservationCorpus` observations for manual extraction-accuracy
auditing against original source papers.

This script does NOT judge accuracy -- it only decides *which* DOIs and
observations get checked, deterministically, so the sample can be
regenerated exactly from a seed. `audit_literature_observations.py`
consumes the judgments a reviewer produces for this manifest and computes
the actual accuracy metrics.

Why DOI, not observation, is the sampling unit: two observations from the
same paper share one extraction run over the same source text, so they are
not independent evidence about extraction accuracy -- see
`docs/literature_observation_accuracy_audit.md` for the full argument.
Each sampled DOI contributes exactly ONE observation to the manifest,
chosen by a fixed deterministic rule (lowest `(corpus_record_index,
operation_index)` among that DOI's observations qualifying for the drawn
stratum), so every manifest row is one independent Bernoulli trial per
field -- no per-DOI aggregation is needed downstream.

Strata (disjoint by DOI, priority order below -- a DOI qualifying for more
than one stratum is assigned to the highest-priority one it qualifies for):
  1. temp_disagree   -- has an observation with temperature=None because
                         the raw corpus reported 2+ disagreeing candidate
                         readings (not because it reported zero). This is
                         the stratum Phase 20D exists to measure -- see the
                         owner's Phase 20D charter in `tasks/todo.md`.
  2. atm_controlled  -- has an observation whose atmosphere fell through to
                         `Atmosphere::Controlled { description }` (an
                         unrecognized single string, or 2+ reported
                         strings for one operation).
  3. fully_resolved  -- has an observation with temperature, duration, AND
                         atmosphere all resolved. Checked specifically
                         because "looks complete" is not evidence of
                         "is correct" -- false confidence in clean-looking
                         data is its own risk.
  4. baseline        -- a plain, stratum-independent simple random sample
                         across ALL DOIs (not disjoint from 1-3 by
                         construction; a small overlap is fine and expected).
                         Exists so the audit's overall accuracy figure is
                         not conditioned on any single interesting-looking
                         field pattern -- the control group a reviewer
                         would look for to rule out cherry-picking.

A `sparse` stratum (all three fields None) was considered and dropped: with
abstract-only access -- expected to be the dominant access level -- there is
usually no way to distinguish "the paper reports no firing temperature"
from "the abstract just doesn't mention the firing temperature that's in
Table 2 of the full text." Nearly every sparse item would land in
`source_inaccessible` or `accessible_but_unstated`, consuming sampling
budget for an empty denominator. That budget goes to `temp_disagree`
instead.

Sampling proceeds in numbered WAVES, each with its own recorded seed, so a
low-accessibility wave 1 can be followed by a targeted wave 2 without
invalidating wave 1's draws or seed. A DOI already used in an earlier wave
is never redrawn. Wave 0 is always the 10-DOI `temp_disagree`-only pilot
used to measure real paper accessibility before sizing the remaining waves
(see docs/literature_observation_accuracy_audit.md, "Pilot" section).

Run (pilot):
  python3 benchmarks/sample_literature_observation_audit.py \\
      --wave 0 --seed 20260815 --sizes temp_disagree=10
Run (a sized wave, after the pilot informs sizes):
  python3 benchmarks/sample_literature_observation_audit.py \\
      --wave 1 --seed 20260815 \\
      --sizes temp_disagree=30,atm_controlled=15,fully_resolved=15,baseline=20
Output: appends to benchmarks/data/literature_observation_audit_manifest.json
        (IDs and gugen's own already-public field values only -- no raw
        corpus / paper text, per the owner's explicit redistributable-data
        constraint for this phase).
"""

import argparse
import json
import random
import ssl
import sys
import urllib.request
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
SNAPSHOT_DEFAULT = Path(__file__).parent / "data" / "literature_observation_snapshot.json"
MANIFEST_PATH = Path(__file__).parent / "data" / "literature_observation_audit_manifest.json"

STRATUM_PRIORITY = ["temp_disagree", "atm_controlled", "fully_resolved"]
ALL_STRATA = STRATUM_PRIORITY + ["baseline"]


def fetch_raw_corpus(local_path):
    """Live license + record-count check on every run, matching
    `fetch_kononova.py`/`audit_kononova_corpus.py`/
    `build_literature_observation_snapshot.py`'s own discipline -- a cached
    copy is never trusted without this (Phase 11 incident: a
    differently-provenanced 30,031-entry cache silently substituted for the
    correctly-licensed 19,488-entry deposit until the live path was run)."""
    with _urlopen(FIGSHARE_ARTICLE) as resp:
        meta = json.load(resp)
    license_name = (meta.get("license") or {}).get("name")
    if license_name != "CC BY 4.0":
        sys.exit(f"REFUSING: license is {license_name!r}, expected 'CC BY 4.0'")
    print(f"license OK ({license_name})", file=sys.stderr)

    if local_path:
        print(f"using local raw corpus {local_path}", file=sys.stderr)
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

    if len(data) != EXPECTED_REACTION_COUNT:
        sys.exit(f"REFUSING: expected {EXPECTED_REACTION_COUNT} raw records, got {len(data)}")
    return data


def load_snapshot(path):
    with open(path) as f:
        snapshot = json.load(f)
    return snapshot["observations"]


def raw_temperature_candidates(obs, raw_records):
    """The raw corpus's own candidate `(min, max)` Celsius readings for the
    `HeatingOperation` this observation was built from -- re-derives
    `resolved_range`'s C-unit filtering from
    `build_literature_observation_snapshot.py`. Returns `[]` if the raw
    corpus reported zero usable readings (a different, non-disagreement
    reason `obs.temperature` can be `None`)."""
    record = raw_records[obs["corpus_record_index"]]
    heating_ops = [op for op in (record.get("operations") or []) if op.get("type") == "HeatingOperation"]
    op = heating_ops[obs["operation_index"]]
    entries = (op.get("conditions") or {}).get("heating_temperature") or []
    candidates = set()
    for e in entries:
        if e.get("units") != "C":
            continue
        mn, mx = e.get("min_value"), e.get("max_value")
        if mn is None and mx is None:
            continue
        candidates.add((mn if mn is not None else mx, mx if mx is not None else mn))
    return sorted(candidates)


def temperature_is_disagreement(obs, raw_records):
    """True iff `obs.temperature is None` because the raw corpus reported
    2+ *disagreeing* candidate readings (not because it reported zero)."""
    if obs["temperature"] is not None:
        return False
    return len(raw_temperature_candidates(obs, raw_records)) >= 2


def is_atm_controlled(obs):
    return isinstance(obs["atmosphere"], dict) and "Controlled" in obs["atmosphere"]


def is_fully_resolved(obs):
    return obs["temperature"] is not None and obs["duration"] is not None and obs["atmosphere"] is not None


def assign_doi_strata(observations, raw_records):
    """DOI -> {stratum: [qualifying observations]}, plus the disjoint
    priority-assigned primary stratum for each DOI. `baseline` is not a
    membership stratum here; it draws from the full DOI universe directly."""
    by_doi = {}
    for obs in observations:
        doi = obs["doi"]
        if not doi:
            continue
        by_doi.setdefault(doi, {"temp_disagree": [], "atm_controlled": [], "fully_resolved": [], "all": []})
        entry = by_doi[doi]
        entry["all"].append(obs)
        if temperature_is_disagreement(obs, raw_records):
            entry["temp_disagree"].append(obs)
        if is_atm_controlled(obs):
            entry["atm_controlled"].append(obs)
        if is_fully_resolved(obs):
            entry["fully_resolved"].append(obs)

    doi_primary_stratum = {}
    for doi, entry in by_doi.items():
        for stratum in STRATUM_PRIORITY:
            if entry[stratum]:
                doi_primary_stratum[doi] = stratum
                break
    return by_doi, doi_primary_stratum


def canonical_observation(candidates):
    return min(candidates, key=lambda o: (o["corpus_record_index"], o["operation_index"]))


def manifest_row(wave, stratum, doi, obs, raw_records):
    row = {
        "wave": wave,
        "stratum": stratum,
        "doi": doi,
        "corpus_record_index": obs["corpus_record_index"],
        "operation_index": obs["operation_index"],
        "route_family": obs["route_family"],
        "target": obs["target"],
        "precursors": obs["precursors"],
        "gugen_temperature": obs["temperature"],
        "gugen_duration": obs["duration"],
        "gugen_atmosphere": obs["atmosphere"],
    }
    if stratum == "temp_disagree":
        # The raw corpus's own disagreeing candidate readings, so a reviewer
        # can check "does the source confirm A, B, both, or neither" instead
        # of only "was there a conflict" -- a gap the wave-0 pilot surfaced
        # (see docs/literature_observation_accuracy_audit.md, "Pilot").
        row["raw_temperature_candidates_celsius"] = raw_temperature_candidates(obs, raw_records)
    return row


def draw_wave(observations, raw_records, wave, seed, sizes, already_used_dois):
    by_doi, doi_primary_stratum = assign_doi_strata(observations, raw_records)
    rng = random.Random(f"{seed}:{wave}")
    rows = []

    for stratum in STRATUM_PRIORITY:
        n = sizes.get(stratum, 0)
        if not n:
            continue
        pool = sorted(
            doi for doi, primary in doi_primary_stratum.items()
            if primary == stratum and doi not in already_used_dois
        )
        if len(pool) < n:
            sys.exit(f"REFUSING: stratum {stratum!r} pool has only {len(pool)} unused DOIs, need {n}")
        drawn = rng.sample(pool, n)
        for doi in drawn:
            obs = canonical_observation(by_doi[doi][stratum])
            rows.append(manifest_row(wave, stratum, doi, obs, raw_records))
            already_used_dois.add(doi)

    n = sizes.get("baseline", 0)
    if n:
        pool = sorted(doi for doi in by_doi if doi not in already_used_dois)
        if len(pool) < n:
            sys.exit(f"REFUSING: baseline pool has only {len(pool)} unused DOIs, need {n}")
        drawn = rng.sample(pool, n)
        for doi in drawn:
            obs = canonical_observation(by_doi[doi]["all"])
            rows.append(manifest_row(wave, "baseline", doi, obs, raw_records))
            already_used_dois.add(doi)

    return rows


def parse_sizes(spec):
    sizes = {}
    for part in spec.split(","):
        part = part.strip()
        if not part:
            continue
        stratum, _, count = part.partition("=")
        if stratum not in ALL_STRATA:
            sys.exit(f"REFUSING: unknown stratum {stratum!r}, expected one of {ALL_STRATA}")
        sizes[stratum] = int(count)
    return sizes


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--snapshot", type=Path, default=SNAPSHOT_DEFAULT)
    parser.add_argument("--local", help="path to an already-downloaded copy of the raw dataset JSON")
    parser.add_argument("--wave", type=int, required=True)
    parser.add_argument("--seed", type=int, required=True)
    parser.add_argument("--sizes", required=True, help="e.g. temp_disagree=10,atm_controlled=5,baseline=5")
    parser.add_argument("--out", type=Path, default=MANIFEST_PATH)
    args = parser.parse_args()

    if not args.snapshot.exists():
        sys.exit(
            f"REFUSING: {args.snapshot} not found -- build it first with "
            "benchmarks/build_literature_observation_snapshot.py"
        )

    observations = load_snapshot(args.snapshot)
    raw_records = fetch_raw_corpus(args.local)
    sizes = parse_sizes(args.sizes)

    existing = {"waves": [], "rows": []}
    if args.out.exists():
        with open(args.out) as f:
            existing = json.load(f)
    if any(w["wave"] == args.wave for w in existing["waves"]):
        sys.exit(f"REFUSING: wave {args.wave} already exists in {args.out} -- pick a new wave number")

    already_used_dois = {row["doi"] for row in existing["rows"]}
    new_rows = draw_wave(observations, raw_records, args.wave, args.seed, sizes, already_used_dois)

    existing["waves"].append({"wave": args.wave, "seed": args.seed, "sizes": sizes})
    existing["rows"].extend(new_rows)

    args.out.parent.mkdir(parents=True, exist_ok=True)
    with open(args.out, "w") as f:
        json.dump(existing, f, indent=2, sort_keys=True)

    print(f"wave {args.wave}: drew {len(new_rows)} rows ({sizes})", file=sys.stderr)
    print(f"manifest now has {len(existing['rows'])} rows across {len(existing['waves'])} waves", file=sys.stderr)


if __name__ == "__main__":
    main()
