# Phase 21B calibration — result

**Verdict: NO-GO** (not statistically significant at the pre-registered
threshold). See `docs/phase21b_calibration_preregistration.md` for the
full design, committed before any energy was computed. This document
reports what that design produced, honestly, per this project's own
established discipline (Phase 30's ensemble ablation, Phase 31 PR 3's
grammar audit) — a NO-GO here does not itself close the question, it
answers exactly the question that was pre-registered.

## The funnel, every stage counted

| Stage | Count |
|---|---|
| Condition 1 gate-passing targets | 273 |
| After flat-formula-parser restriction (target + every route formula) | 269 targets / 1285 rows handed to the Rust harness |
| Rows where `balance()` finds a genuine, byproduct-free reaction (target *and* every declared precursor survives with a positive coefficient) | 347 / 1285 |
| Targets with ≥1 successfully-balanced row in *both* the `pure` and `impure` verdict classes | **54** |

54 is well above the pre-registered ≥30 floor, so a verdict is computed
(not "insufficient sample").

**Why balance-ability cut so much (1285 → 347)**: condition 1's own
coverage gate only checked that each species individually has an OQMD
entry, never that a route's precursors actually combine into a valid
reaction. Checked directly while designing this phase: 929/1286
parseable rows have at least one element in the route not present in
the target — either a real byproduct the dataset's own "gas-free"
extraction failed to list, or a doped-compound-under-host-formula
labeling ambiguity (`docs/thermodynamic_selectivity_calibration.md`
§2's own finding). `balanced_reaction_delta_ev_per_atom`'s explicit
gas-free, solid-only scope means neither case is fixable by allowing a
`curated_byproducts()` search — that would only ever abstain to
`Ok(None)` anyway, since this pipeline sources thermodynamic entries
only from OQMD (a solid-state database) for the actual route/target
formulas, never a fabricated solid-phase entry for a gas or liquid
byproduct.

## Primary result (T = 300 K, pre-registered)

| Metric | Value |
|---|---|
| Qualifying targets | 54 |
| Correct (pure route has the more negative reaction energy) | 32 |
| Accuracy | **59.3%** |
| One-sided exact binomial p (H0: 0.5, H1: >0.5) | **0.110** |
| Verdict | **NO-GO** (p ≥ 0.05) |

The direction is consistent with the hypothesis (59.3% > 50%), but at
n=54 this is not distinguishable from chance at the pre-registered
significance level.

## Secondary, descriptive-only metrics

- **Sensitivity at T = 1800 K (gugen's upper validated bound)**:
  accuracy and p-value are **identical** to the 300 K result (32/54,
  p=0.110) — the finite-temperature Gibbs correction
  (`relative_solid_gibbs_ev_per_atom`) never flipped which of the two
  routes had the lower reaction energy for any of the 54 comparisons.
  Plausible, not surprising: the correction's typical magnitude (tens
  of meV/atom) is small next to the real formation-energy differences
  between distinct precursor routes for the same target (often
  hundreds of meV/atom or more) — reported as a real finding, not
  assumed in advance.
- **Full un-deduplicated pairwise comparison** (94 comparisons across
  all `n_pure × n_impure` pairs per target, explicitly *not*
  independence-corrected): accuracy **51.1%** (48/94) — much closer to
  chance than the primary per-target metric, showing the
  DOI-deduplication in the primary design was not a cosmetic choice;
  the raw pairwise pool dilutes the signal.
- **Residual cross-target DOI overlap** (disclosed, not hidden): 3 DOIs
  are each selected as a representative row for two different targets
  — `10.1016/j.jallcom.2015.10.232` and `10.1039/C4DT03773A` for
  `Yb5Ga2Sb6`/`Yb5In2Sb6`, and `10.1021/cm300520w` for
  `Ca5Ga2Sb6`/`Ca5In2Sb6` — consistent with a single paper studying an
  isostructural family (Ga/In substitution) across related targets. The
  per-target design already prevents *within*-target DOI reuse; this is
  the weaker across-target case, small (3 of 54 targets touch it) and
  disclosed rather than assumed away.
- **Chemical family distribution of the 54 qualifying targets**
  (informal, first-matching-anion heuristic, not a taxonomy):
  Sulfide/chalcogenide 17, Other 22, Oxide 13, Phosphide/phosphate 2.

## Hand-verified, not just trusted from the aggregate count

`Ti3SiC2` (a well-known MAX-phase ceramic) was traced by hand:

- **Pure-labeled route**: `Si + Ti + TiC -> Ti3SiC2`. Balance:
  `1 Si + 1 Ti + 2 TiC -> Ti3SiC2` (Ti: 1+2=3 ✓, Si: 1 ✓, C: 2 ✓) —
  matches `balance()`'s own output.
- **Impure-labeled route**: `SiC + Ti + TiC -> Ti3SiC2`. Balance:
  `1 SiC + 2 Ti + 1 TiC -> Ti3SiC2` (Ti: 2+1=3 ✓, Si: 1 ✓, C: 1+1=2 ✓) —
  also matches.

Both are real, sensible solid-state combination reactions for a
well-known material — not an artifact of the pipeline. For this
specific target, the *impure*-labeled route had the more negative
reaction energy (−0.518 eV/atom vs. −0.319 eV/atom for the pure route
at 300 K) — one of the 22 comparisons where the hypothesis's direction
did not hold, included honestly in the 59.3% figure, not excluded.

## What this does not claim

No claim that thermodynamics doesn't predict synthesis purity — only
that, at n=54, in this specific narrow slice of the corpus (flat-formula-parseable,
OQMD-covered, byproduct-free-balanceable targets), the signal wasn't
strong enough to distinguish from chance at the pre-registered
threshold. No claim about the 1286 − 347 = 938 rows excluded for
failing to balance — most likely involve a real byproduct or a
doped-compound labeling ambiguity this phase's own gas-free, solid-only
scope was never going to resolve, not a claim that thermodynamics is
irrelevant to them. No claim about temperature: 300 K and 1800 K are
fixed conventions (the low and high ends of gugen's own validated
range), never a claim about any route's real synthesis temperature —
no per-route temperature exists anywhere in this corpus. Per this
project's own standing instruction, carried forward from Phase 21B
condition 2: the pure/impure label itself is noisy and unverified at
scale (real accessibility and route-representation caveats found in a
15-item audit) — a positive result here would have been evidence about
this label's relationship to reaction energetics, not a validated
purity predictor, and this NO-GO result inherits the same caveat in
reverse: it does not mean the underlying physical mechanism is absent,
only that this specific measurement didn't detect it significantly.

## Non-goals (unchanged, honored)

No `score_plan` connection, no `RankingWeights` change, no default
ranking change, no success-probability claim, no automatic temperature
selection, no literature-condition promotion, no version bump, no
public API change of any kind — the NO-GO verdict makes this moot for
now, but it would have held even on a GO or STRONG GO, per
`docs/thermodynamic_selectivity_calibration.md` §6.5 Step 5 / §7 and
this phase's own pre-registration.

## Status

Implemented and measured. New files: `benchmarks/parse_flat_formula.py`
(+ tests), `benchmarks/build_phase21b_calibration_input.py`,
`examples/exploration_phase21b_calibration.rs`,
`benchmarks/analyze_phase21b_calibration.py`,
`benchmarks/data/phase21b_calibration_result.json` (committed, ~46KB).
Two intermediate files gitignored as regenerable-in-seconds (see
`.gitignore`'s own comment). No `src/` change. Root quality gate
(`cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets
--all-features -- -D warnings`, `cargo test --workspace --all-features`
/ `--no-default-features`, `RUSTDOCFLAGS="-D warnings" cargo doc
--all-features --no-deps`) green; Python test suites
(`test_parse_flat_formula.py`, plus every pre-existing `benchmarks/test_*.py`)
green. **Stopping here per the pre-registration's own restated
non-goals — no further action without a new, separate, explicit owner
trigger.**
