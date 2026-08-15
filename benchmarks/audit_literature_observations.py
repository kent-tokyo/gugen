#!/usr/bin/env python3
"""Phase 20D: computes extraction-accuracy metrics from the judgments file
produced against `sample_literature_observation_audit.py`'s manifest.

Does NOT touch the network or re-derive judgments -- it only aggregates
`benchmarks/data/literature_observation_audit_judgments.json` (one row per
manifest item, produced by independent research review; see
`docs/literature_observation_accuracy_audit.md` for the review method).

Independence unit is the DOI, not the observation (see the sampler's module
doc comment for why): the manifest draws exactly one observation per sampled
DOI, so every judgment row is one independent Bernoulli trial per field.
Confidence intervals are Wilson score intervals over that trial count --
deliberately not a fancier method, so every number here is checkable by
hand against a standard reference, per this project's anti-fabrication
discipline (no invented statistical machinery).

`source_inaccessible` rows are excluded from every field-accuracy
denominator (not counted as errors -- they are an audit limitation, not a
corpus finding). `multi_entry_cause` is tallied only over `temp_disagree`
rows that reached `full_text` access, denominator reported separately from
every other metric, per the owner's own review requirement that this
classification stay evidence-based.

Run: python3 benchmarks/audit_literature_observations.py
Output: benchmarks/data/literature_observation_audit_summary.json (metrics,
        machine-readable) and prints a human-readable summary to stdout.
"""

import json
import math
from collections import Counter, defaultdict
from pathlib import Path

DATA_DIR = Path(__file__).parent / "data"
JUDGMENTS_PATH = DATA_DIR / "literature_observation_audit_judgments.json"
SECOND_PASS_PATH = DATA_DIR / "literature_observation_audit_second_pass.json"
SUMMARY_PATH = DATA_DIR / "literature_observation_audit_summary.json"

AGREEMENT_FIELDS = ["access_level", "identity_match", "temperature_verdict", "duration_verdict", "atmosphere_verdict"]

FIELD_VERDICTS = ["temperature_verdict", "duration_verdict", "atmosphere_verdict"]
ACCESSIBLE_LEVELS = {"full_text", "abstract_only"}


def wilson_interval(k, n, z=1.96):
    """95% Wilson score interval for a binomial proportion k/n. Returns
    (point_estimate, low, high), or (None, None, None) if n == 0."""
    if n == 0:
        return None, None, None
    p = k / n
    denom = 1 + z * z / n
    center = (p + z * z / (2 * n)) / denom
    margin = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / denom
    return p, max(0.0, center - margin), min(1.0, center + margin)


def load():
    with open(JUDGMENTS_PATH) as f:
        return json.load(f)["judgments"]


def accessibility_table(judgments):
    by_stratum = defaultdict(Counter)
    for j in judgments:
        by_stratum[j["stratum"]][j["access_level"]] += 1
        by_stratum["__all__"][j["access_level"]] += 1
    return by_stratum


def identity_accuracy(judgments):
    """Two deliberately different framings, both reported (never just one --
    picking a single framing here is exactly the kind of denominator choice
    a reviewer should be able to catch, so both are shown):

    - conservative: match / (match + mismatch + unverifiable). Treats "we
      could not confirm this is even the right paper/record" as a failure
      to establish accuracy, not a missing data point -- most
      `unverifiable` rows still had a title/Crossref record to judge from,
      so this isn't the same exclusion as field-level `not_checked`.
    - excl_unverifiable: match / (match + mismatch). What the rate looks
      like among only the rows that reached a definite verdict either way.
    """
    checked = [j for j in judgments if j["identity_match"] in ("match", "mismatch", "unverifiable")]
    matches = sum(1 for j in checked if j["identity_match"] == "match")
    mismatch_rows = [j for j in checked if j["identity_match"] == "mismatch"]
    unverifiable_n = sum(1 for j in checked if j["identity_match"] == "unverifiable")
    p, lo, hi = wilson_interval(matches, len(checked))
    p2, lo2, hi2 = wilson_interval(matches, matches + len(mismatch_rows))
    return {
        "n": len(checked),
        "matches": matches,
        "mismatches": len(mismatch_rows),
        "unverifiable": unverifiable_n,
        "conservative": {"point_estimate": p, "ci95_low": lo, "ci95_high": hi},
        "excl_unverifiable": {"point_estimate": p2, "ci95_low": lo2, "ci95_high": hi2},
        "mismatch_dois": [j["doi"] for j in mismatch_rows],
    }


def field_accuracy(judgments):
    """Per field, per stratum: Wilson CI on match-rate among rows with
    accessible source AND a value gugen actually had to check (i.e.
    excluding not_checked/not_applicable rows, which carry no evidence
    either way)."""
    out = {}
    for field in FIELD_VERDICTS:
        out[field] = {}
        for stratum in ["temp_disagree", "atm_controlled", "fully_resolved", "baseline", "__all__"]:
            rows = judgments if stratum == "__all__" else [j for j in judgments if j["stratum"] == stratum]
            evaluable = [j for j in rows if j[field] not in ("not_checked", "not_applicable")]
            matches = sum(1 for j in evaluable if j[field] == "match")
            mismatches = sum(1 for j in evaluable if j[field] in ("mismatch", "unit_error", "contamination"))
            missed = sum(1 for j in evaluable if j[field] == "missed_value")
            unverifiable = sum(1 for j in evaluable if j[field] == "accessible_but_unstated")
            # Wilson CI is computed on the strict match/non-match trial set
            # (match + mismatch-family + missed_value); accessible_but_unstated
            # rows are excluded from the CI itself (no evidence either way)
            # but reported alongside it.
            trials = matches + mismatches + missed
            p, lo, hi = wilson_interval(matches, trials)
            out[field][stratum] = {
                "n_accessible_and_checkable": len(evaluable),
                "matches": matches,
                "mismatches_incl_unit_and_contamination": mismatches,
                "missed_value": missed,
                "accessible_but_unstated": unverifiable,
                "ci_trials_n": trials,
                "point_estimate": p,
                "ci95_low": lo,
                "ci95_high": hi,
            }
    return out


def multi_entry_cause_tally(judgments):
    """Only over temp_disagree rows with full_text access AND a real
    classification (never insufficient_evidence) -- see module doc comment."""
    candidates = [
        j for j in judgments
        if j["stratum"] == "temp_disagree" and j["access_level"] == "full_text"
    ]
    classified = [j for j in candidates if j["multi_entry_cause"] != "insufficient_evidence"]
    tally = Counter(j["multi_entry_cause"] for j in classified)
    return {
        "temp_disagree_full_text_n": len(candidates),
        "classified_n": len(classified),
        "unclassified_insufficient_evidence_n": len(candidates) - len(classified),
        "tally": dict(tally),
        "classified_dois": [(j["doi"], j["multi_entry_cause"]) for j in classified],
    }


def inter_reviewer_agreement():
    """Raw agreement between the first-pass and a blind second-pass review
    on a 5-item overlap subset drawn from accessible (full_text/
    abstract_only) first-pass items -- see module doc comment on why
    overlapping only accessible items matters (an inaccessible item's
    second review trivially "agrees" on not_checked and measures nothing)."""
    if not SECOND_PASS_PATH.exists():
        return None
    with open(SECOND_PASS_PATH) as f:
        reviews = json.load(f)["overlap_reviews"]
    per_field = {}
    for field in AGREEMENT_FIELDS:
        agree = sum(1 for r in reviews if r["first_pass"][field] == r["second_pass"][field])
        per_field[field] = {"agree": agree, "n": len(reviews), "rate": agree / len(reviews)}
    disagreements = [
        {"doi": r["doi"], "field": field, "first_pass": r["first_pass"][field], "second_pass": r["second_pass"][field]}
        for r in reviews
        for field in AGREEMENT_FIELDS
        if r["first_pass"][field] != r["second_pass"][field]
    ]
    return {"n_overlap_items": len(reviews), "per_field": per_field, "disagreements": disagreements}


def main():
    judgments = load()
    summary = {
        "n_judgments": len(judgments),
        "accessibility": {k: dict(v) for k, v in accessibility_table(judgments).items()},
        "identity_accuracy": identity_accuracy(judgments),
        "inter_reviewer_agreement": inter_reviewer_agreement(),
        "field_accuracy": field_accuracy(judgments),
        "multi_entry_cause": multi_entry_cause_tally(judgments),
    }

    SUMMARY_PATH.write_text(json.dumps(summary, indent=2))

    print(f"n_judgments: {summary['n_judgments']}")
    print("\naccessibility (all strata):", summary["accessibility"]["__all__"])
    ident = summary["identity_accuracy"]
    c, e = ident["conservative"], ident["excl_unverifiable"]
    print(
        f"\nidentity accuracy: {ident['matches']} match / {ident['mismatches']} mismatch / "
        f"{ident['unverifiable']} unverifiable (n={ident['n']})"
    )
    print(f"  conservative (unverifiable=failure): {c['point_estimate']:.1%} (95% CI [{c['ci95_low']:.1%}, {c['ci95_high']:.1%}])")
    print(f"  excl. unverifiable: {e['point_estimate']:.1%} (95% CI [{e['ci95_low']:.1%}, {e['ci95_high']:.1%}])")
    if ident["mismatch_dois"]:
        print(f"  mismatches: {ident['mismatch_dois']}")
    print("\nfield accuracy (__all__ stratum):")
    for field in FIELD_VERDICTS:
        row = summary["field_accuracy"][field]["__all__"]
        if row["ci_trials_n"]:
            print(
                f"  {field}: {row['matches']}/{row['ci_trials_n']} = "
                f"{row['point_estimate']:.1%} (95% CI [{row['ci95_low']:.1%}, {row['ci95_high']:.1%}]), "
                f"+{row['accessible_but_unstated']} accessible_but_unstated excluded from CI"
            )
        else:
            print(f"  {field}: no evaluable trials")
    mec = summary["multi_entry_cause"]
    print(
        f"\nmulti_entry_cause: {mec['classified_n']}/{mec['temp_disagree_full_text_n']} "
        f"full-text temp_disagree items classified: {mec['tally']}"
    )
    agr = summary["inter_reviewer_agreement"]
    if agr:
        print(f"\ninter-reviewer agreement (n={agr['n_overlap_items']} overlap items):")
        for field, row in agr["per_field"].items():
            print(f"  {field}: {row['agree']}/{row['n']} = {row['rate']:.0%}")
        if agr["disagreements"]:
            print("  disagreements:")
            for d in agr["disagreements"]:
                print(f"    {d['doi']} [{d['field']}]: {d['first_pass']!r} vs {d['second_pass']!r}")
    print(f"wrote {SUMMARY_PATH}")


if __name__ == "__main__":
    main()
