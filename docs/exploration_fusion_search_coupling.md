# Phase 30.5 — Candidate Fusion × Search Coupling Audit

**Status: methodology pre-registered below, before any factorial cell has
been run.** This section, and every constant in it, was written and
committed before `examples/exploration_fusion_search_coupling_audit.rs`
was executed. Results (once run) are appended in a clearly-labeled
section at the end, never edited into the hypotheses/methodology text
above them.

## Why this phase exists

PR 1 (`FrequencyPriorGenerator`) and PR 2 (`ThermodynamicStabilityGenerator`)
each added a real, non-fabricated candidate-ranking signal to the
`CandidateGeneratorEnsemble`, and both times the ensemble's end-to-end
recall under a tightened `SearchBudget` failed to beat `catalog-exact`
alone (PR 1: 42.99% vs. 44.35%; PR 2: 42.85% vs. 44.35%). The owner's
judgment: two consecutive negative results with genuinely different
signals is starting to look like a property of *how* candidates get
combined and searched, not the signal quality. PR 3 (prior-experiment)
and PR 4 (literature-analog) are paused until this is resolved. This
phase is a **diagnostic**, not a new generator: it isolates which of
(1) generator signal weakness, (2) the ensemble fusion rule (min-rank),
(3) candidate-order dependence in the search frontier, or (4) search
budget is actually responsible for the gap.

## The mechanism this phase investigates

`search_precursor_sets`'s best-first frontier (`src/precursor.rs`)
orders states by fewer missing target elements, then shallower depth,
then — as the final, always-decisive tie-break — a lexicographically
smaller `chosen` index-vector. No external priority signal (frequency,
thermodynamic rank, fused ensemble rank) reaches this tie-break at all
today. Which numeric index a given true-precursor set happens to land
on — hence how early its `chosen` vector sorts among ties — is entirely
a function of input candidate order, not of any ranking a generator
produced. Phase 30.5 adds a pluggable-tie-break variant of this exact
frontier (`search_precursor_sets_diagnostic`, `search_diagnostics`
feature, `src/precursor.rs`) to test this directly, without touching
`search_precursor_sets`'s own real, unchanged behavior.

## Pre-registered hypotheses

**H1**: Min-rank fusion over-promotes any single candidate one generator
ranked highly, ignoring disagreement among the other generators.

**H2**: `search_precursor_sets` does not treat generator priority as an
explicit search signal at all — candidate order becomes a *hidden*
search policy via the frontier's own index-based tie-break.

**H3**: `FrequencyPriorGenerator` and `ThermodynamicStabilityGenerator`
are both *not* directly target-reaction-aware (global frequency;
absolute, not reaction, stability) — the added signal itself may simply
be too weak to help, independent of fusion/search mechanics.

**H4**: Candidate-order sensitivity is large under a tight budget and
shrinks as the budget grows.

**H5**: Row-local candidate-list improvements (better Recall@K) do not
reliably convert into end-to-end route recovery — a fusion→search
conversion failure exists at the boundary between candidate generation
and the frontier search.

Any hypothesis added after seeing results is explicitly logged as
post-hoc, in the results section, never inserted above as if
pre-registered.

## What is already established, not re-litigated by this phase

- Global frequency ranking (PR 1) did not beat `catalog-exact` alone.
- Absolute thermodynamic stability ranking (PR 2) did not beat
  `catalog-exact` alone.
- Min-rank ensemble fusion has underperformed `catalog-exact` alone
  twice in a row, with two different added signals.
- The same candidate *set*, searched in different *orders*, produces a
  different end-to-end recall under a tight budget (PR 1/PR 2's own
  measurements, since `catalog-exact` and `frequency-prior`/
  `thermodynamic-stability` always operate over the identical per-row
  candidate set).
- PR 2's own row-local Recall@15/@20 showed the ensemble modestly
  *ahead* of `catalog-exact` (19.23% vs. its single-generator peers at
  K=15; 35.95% vs. 35.28% at K=20), while still losing end-to-end
  (42.85% vs. 44.35%) — a real, measured signal that candidate-list
  quality and end-to-end route recovery are not the same thing here.

## What is NOT established (do not overclaim)

- That a genuinely target-reaction-aware generator would also fail.
  `ThermodynamicStabilityGenerator` ranks by absolute stability, not
  reaction favorability toward a specific target — it is not a tested
  instance of a target-aware signal, and must not be treated as one when
  interpreting this phase's results.
- That min-rank fusion is the sole cause.
- That `LiteratureAnalogGenerator` (the planned next real signal) would
  be ineffective.
- That the candidate-ensemble architecture itself (PR 1) is not viable.

## Pre-registered methodological choices

Fixed here, before any cell is measured — not tuned against results.

- **RRF constant**: `k = 60` (Cormack et al. 2009's own standard
  constant) — a literature default, not corpus-tuned.
- **T3 (`ExplicitFusionPriority`) primary aggregation**: **sum** of fused
  rank across a state's chosen candidates (rewards broader
  multi-generator support). Max-of-fused-rank is a secondary sensitivity
  check, not primary.
- **Primary K for conditional search conversion rate**: **K=20** — the
  row-local pool size where PR 1/PR 2's own descriptive Recall@K tables
  already showed the clearest ensemble-vs-`catalog-exact` divergence.
  Other K values reported for context, never as alternate primaries.
- **Bootstrap**: 10,000 resamples, 95% CI, resampled by **target group**
  (every row sharing a canonical `target_formula` moves together in each
  resample — many rows share a target formula in this catalog, up to
  dozens of times, so row-independence would be a real statistical
  error).
- **Dev/confirmation-holdout split**: deterministic, target-level,
  **80/20**. Rule: the first byte of `sha256_hex(target_formula)` mod 5
  — values `0`-`3` → development, value `4` → confirmation holdout.
  Every row sharing a `target_formula` lands in the same split by
  construction (the split key is the formula string, not the row).
  Recorded in `benchmarks/data/exploration_fusion_search_audit_split_manifest.json`
  with the rule, per-split target/row counts, and catalog checksum.
- **Shuffled candidate-order seeds (control E)**: 10 fixed, named seeds
  (`shuffle-1` .. `shuffle-10`), each a deterministic permutation via
  sorting candidates by `sha256_hex("{seed}:{id}")` — no `rand`
  dependency, fully reproducible.
- **Oracle order**: gold precursors first (by literal `route` membership,
  in their own listed order), then every other candidate in canonical
  `PrecursorId` order. Diagnostic-only — measures the search's own
  reachable ceiling under a tight budget, never used as a real ranking
  policy or reported as a candidate production default.

## Candidate-ordering / fusion-rule / tie-break axes under test

- **Candidate order (A-E + oracle)**: A `catalog-exact` order; B reverse
  catalog order; C canonical `PrecursorId` order (also serves as tie-break
  T2's own test, per the finding below); D current min-rank ensemble
  fusion order; E ten fixed deterministic shuffles; oracle (diagnostic-
  only ceiling).
- **Fusion rule (1-6)**: MinRank (current); ReciprocalRankFusion (`k=60`);
  MeanNormalizedRank; ConsensusFirst; RoundRobin; CatalogAnchored (a
  negative control — expected to collapse to `catalog-exact`'s own order
  in this benchmark, since `catalog-exact` already has every candidate).
- **Search tie-break (T1/T3/T4)**: T1 `IndexOrder` (production, unchanged);
  T3 `FusionPrioritySum`; T4 `MarginalCoverage`. T2 (`CanonicalIdTieBreak`)
  needs no separate implementation — index-lexicographic tie-break over a
  `PrecursorId`-sorted candidate array *is* ID-lexicographic tie-break,
  so T2 = T1 + ordering C.
- **Search budget**: 10, 20, 50, 100, 500, and a sufficiently-large
  control (`max_precursor_sets: 100_000`).

## Decision gate (fixed before results, per the owner's own criteria)

Policy selected on the **development split only**; the **confirmation
holdout** is evaluated exactly once, after the development-side choice
is locked, never used to iterate.

- **GO**: beats `catalog-exact` on holdout end-to-end recall; bootstrap
  CI does not cross substantially into negative territory; recall-vs-
  budget AUC is non-inferior; hard validity (element conservation,
  byproduct allow-list, balance logic) is unaffected; deterministic;
  does not lose solutions available under a large/unlimited budget.
- **STRONG GO**: GO, plus beats `catalog-exact` by ≥1 percentage point
  absolute on holdout, AND wins on AUC, AND gained targets > lost
  targets.
- **NO-GO**: development-only improvement that doesn't reproduce on
  holdout; improvement isolated to one budget point; within the noise
  band of the random-order-shuffle distribution; or any validity/
  determinism regression.

A GO or STRONG GO result is a recommendation to the owner, not an
automatic production change — `search_precursor_sets`'s own default
tie-break is not switched as a result of this phase regardless of
verdict.

---

## Correction (2026-08-25): the original Results section below is retracted

The owner reviewed the 2026-08-24 results below and identified a
contradiction the original writeup missed: at `budget=100,000`
(`exhaustion_rate=0` everywhere — the search never hit the budget cap),
the docs claimed "every single policy converges to 0.4713," but the
committed result JSON itself shows `catalog-exact`/`B-reverse`/
`D-min-rank-ensemble` (and all 6 fusion rules) at exactly 0.471264 while
`oracle` and all 10 shuffle seeds sit at 0.510–0.515 — genuinely
different numbers, not a convergence. That contradiction, plus two
independently-identified Rust `Eq`/`Ord` contract bugs (`TotalF64`,
`SearchState`) and a methodology gap (the `catalog-anchored` negative
control was never excluded from policy selection, so the reported
"confirmation holdout" was comparing catalog-exact's ordering to itself),
were enough to block the merge and require a full root-cause
investigation before any conclusion could be trusted.

**Root cause, found via the owner's own mandated sequence (synthetic
fixture first, then a minimal real-corpus reproduction, then an isolation
check against the actual committed numbers): `run_cell`'s
`generator_outputs: BTreeMap<String, RowGeneratorOutputs>` cache
(`examples/exploration_fusion_search_coupling_audit.rs`, "materialize the
3 base generators' outputs once per row") is keyed by `target_formula`
alone, first-row-wins, on the explicit (and false) assumption that "rows
sharing a `target_formula` also share the exact same candidate pool
construction in this frozen catalog."** That assumption does not hold:
1,866 of 2,879 rows (65%) share a `target_formula` with at least one
other row, and 388 of those 442 duplicate-formula groups have genuinely
*different* candidate pools attached to different rows (the corpus
contains repeated targets with different decoy/candidate sets, not
exact duplicates). For 1,233 non-first rows in such a group the cache
silently serves a different row's candidate pool; for 803 of those, the
row's own gold route names a precursor that is entirely absent from the
served (wrong) pool — a structural, deterministic recovery failure that
has nothing to do with order, fusion, or tie-break.

`candidate_order()`'s `A-catalog-exact`/`B-reverse`/`D-min-rank-ensemble`
arms, and every one of the 6 fusion rules, all read from this cache
(`outputs.catalog_exact`/`outputs.frequency_prior`/
`outputs.thermodynamic_stability`, all built once per `target_formula`).
`oracle` and all 10 `shuffle-*` arms read `row.candidates` directly,
per-row, uncached. That is the entire explanation for the pattern in the
data: nine nominally-different policies converging to a byte-identical
0.471264 was never evidence of convergence — it was nine policies
silently evaluating the *same* (frequently wrong) cached content, while
oracle/shuffle evaluated the correct, row-specific content and scored
higher as a direct result.

This was verified, not just inferred from code reading:
`tests/phase30_5_order_invariance_real_row.rs` and the synthetic fixture
in `src/precursor.rs` (`duplicate_composition_candidates_keep_canonical_chemistry_order_invariant`)
confirm `search_precursor_sets` itself recovers the same canonical
chemistry regardless of candidate array order at a genuinely exhaustive
budget; a standalone reproduction (`examples/phase30_5_pool_filter_isolation_check.rs`)
that always uses each row's own correct candidate pool — bypassing the
cache entirely — found **no recall gap** between catalog-exact-equivalent
order and shuffled order on a 455-row real sample (0.4901 vs. 0.4901),
which is the expected result once the cache bug is not present to distort
the comparison.

**Retracted claims and why:**

| Original claim | Status | Reason |
|---|---|---|
| "catalog-exact's own order is 0th percentile among 10 shuffles" / "random ordering beats production order 10/10" | **Retracted** | Artifact of the cache serving mismatched candidate pools to the catalog-exact/reverse/min-rank-ensemble arms; oracle/shuffle read correct per-row pools. Not an order effect. |
| "H2 confirmed directly and quantitatively" (candidate order is a hidden search policy) | **Untested, not confirmed or refuted** | The order sweep never held content fixed across all its arms, so H2 was never actually tested by this run. |
| "H4 confirmed directly" (order-sensitivity shrinks as budget grows, "every single policy converges to 0.4713") | **Partially retracted** | True only for the 9 cache-fed policies (which share content, so of course they converge to each other); false as stated for the order sweep as a whole, since oracle/shuffle never converge toward 0.4713 at any budget in the raw data. |
| "Clean confirmation-holdout NO-GO" for `fusion=catalog-anchored` | **Retracted as independent evidence** | `catalog-anchored` was never excluded from policy selection despite being pre-registered as a negative control expected to tie catalog-exact exactly — it did, and the "holdout confirmation" was mathematically guaranteed to be null (comparing catalog-exact's ordering to itself), not a real test of anything. |

**Two genuine, independent findings survive, both real and both worth
fixing on their own merits, neither of which explains the observed gap
above:**

1. **Dedup-hygiene defect (production, `src/precursor.rs`)**:
   `evaluate_complete_state` dedups accepted plans by `BalancedReaction`'s
   derived `PartialEq`, which compares `reactants`/`products` as
   *ordered* vectors — but `balance()` builds those vectors by zipping
   positionally against its own input order, which tracks candidate
   array order via `chosen`'s ascending indices. When two candidates
   share a composition under different `PrecursorId`s (a real, common
   pattern in this corpus — 440/2,879 rows) and land on opposite sides of
   a third candidate in the array, the same chemistry gets recorded as
   two separate `accepted` entries instead of deduping to one. Reproduced
   directly in a synthetic fixture
   (`duplicate_composition_candidates_keep_canonical_chemistry_order_invariant`,
   `src/precursor.rs`). Measured impact on exact-ID recall specifically:
   small (4 of 464 real rows differ in an isolated test holding content
   fixed). Real impact: can burn two `SearchBudget::max_plans_returned`
   slots in `Planner`'s output for what is really one plan. **Not a
   recall-losing order-invariance violation** — canonical chemistry
   recovery is unaffected (see the synthetic fixture's own assertions).
2. **Synonym undercount (methodology, not production code)**: when a row
   has duplicate-composition candidates, `evaluate_complete_state`'s
   dedup deterministically keeps whichever `PrecursorId` sorts
   lexicographically smaller (by design, independent of discovery
   order — confirmed via `tests/phase30_5_order_invariance_real_row.rs`
   on a real corpus row). If a row's gold route happens to name the
   *other* (lexicographically larger) synonym — e.g. gold says
   `α-Al2O3`, dedup always keeps `Al2O3` — then exact-ID recovery is
   **permanently false for that row under every policy**, order-
   independent. This does not create a differential between policies,
   but it does systematically depress exact-ID recall for ~15% of rows
   uniformly, and is independent supporting evidence for the route-
   identity metric redesign (canonical composition/reaction identity as
   primary, not raw `PrecursorId`-set equality) the owner already
   requested.
3. **Two unrelated Rust `Eq`/`Ord` contract bugs** the owner identified
   independently of the above investigation (`TotalF64`, `SearchState` in
   `src/precursor.rs`) remain unfixed as of this correction and are
   tracked separately — real defects, not implicated in the recall gap
   either, since production `search_precursor_sets` only ever constructs
   `TieBreakKey::IndexOrder`.

**Bottom line on the question this correction exists to answer: is
production `search_precursor_sets` implicated in the observed
catalog-exact-vs-shuffle gap? No.** The dominant cause is a benchmark
data-wiring bug (a cache keyed on a non-unique field), not a search or
dedup correctness issue. The dedup-hygiene defect above is real and
worth fixing, but it is a small, non-differential, plan-count-inflation
issue — not the mechanism behind the ~4-point recall gap this document
originally reported and attributed to candidate order.

**Proposed fix scope (not yet implemented, pending owner sign-off — this
correction stops here per the owner's own explicit process gate: report
root cause and fix scope before bundling a large fix)**:

- Fix `generator_outputs`'s cache key to something unique per row (row
  index, or a hash of the row's own candidate id list + target_formula)
  instead of `target_formula` alone.
- Exclude `fusion=catalog-anchored` (and any other policy provably
  byte-identical to the baseline) from `is_selectable_policy`.
- Lock a canonical route-identity metric (composition-multiset or
  balanced-reaction identity, not raw `PrecursorId`-set equality) as
  primary, per the owner's own instruction, before re-running anything.
- Fix the `TotalF64`/`SearchState` `Eq`/`Ord` contract violations with
  dedicated tests, independent of this rerun.
- Fix the dedup-hygiene defect (`evaluate_complete_state`'s
  `BalancedReaction`-vector-order-sensitive equality) — separate,
  small, production change.
- Re-run the full development sweep exactly once with the cache fixed
  and the metric locked; only if a real, non-control policy beats the
  corrected baseline, draw a **freshly-unseen** confirmation holdout
  (the original holdout has now been looked at and cannot serve as an
  unseen split for any future candidate).
- Even after all of the above, this PR is not to be merged without the
  owner's own explicit re-approval.

The original, now-superseded Results section is kept below for the
audit trail (per this codebase's own discipline of not silently
rewriting history), with every retracted claim listed above and not to
be relied upon.

## Results (2026-08-24, superseded — see "Correction" above)

Run 2026-08-24 (`examples/exploration_fusion_search_coupling_audit.rs`).
**A real bug was found and fixed in this script's own first draft, before
trusting any number**: the initial policy-selection logic compared cells
across *different* budgets, which trivially always selects the largest
budget regardless of candidate order/fusion/tie-break — not a claim about
this phase's actual question. Fixed to compare every policy at the fixed
`PRIMARY_BUDGET=20` for selection (the full `BUDGETS` sweep is still run
and reported for context/AUC), and to exclude shuffle-order seeds and the
oracle order from being selectable "policies" at all, since a single
fixed random permutation isn't a deployable production strategy. Same
"verify the measurement before trusting it" discipline this whole arc has
used since the byproduct-fix scripts' own diff-by-`target_formula` bug.

### Development split (stride sample, n=435 of 2175 rows, budget=20)

| policy | recall | vs. catalog-exact |
|---|---|---|
| **catalog-exact (baseline)** | **0.4391** | — |
| B-reverse order | 0.4069 | −0.0322 |
| D-min-rank-ensemble order / fusion=min-rank (current production ensemble) | 0.4253 | −0.0138 |
| fusion=reciprocal-rank-fusion (k=60) | 0.4046 | −0.0345 |
| fusion=mean-normalized-rank | 0.4000 | −0.0391 |
| fusion=consensus-first | 0.4023 | −0.0368 |
| fusion=round-robin | 0.4253 | −0.0138 |
| fusion=catalog-anchored (negative control) | 0.4391 | 0.0000 (tie, as predicted) |
| tie-break T3 fusion-priority-sum | 0.4276 | −0.0115 |
| tie-break T4 marginal-coverage | 0.4230 | −0.0161 |

**No genuinely different, deployable policy beat catalog-exact at the
fixed comparison budget.** `catalog-anchored` ties exactly (0.4391),
confirming the pre-registered negative-control prediction: since
`catalog-exact` is always given the row's full candidate pool in this
benchmark, `catalog-anchored`'s "append only what other generators alone
proposed" logic has nothing to append, and it collapses to
`catalog-exact`'s own order byte-for-byte.

**A separate, striking, independently-verified finding**: `catalog-exact`'s
own recall (0.4391) is **lower than all 10 of the 10 fixed shuffle-order
seeds** (range 0.4483–0.4621) — 0th percentile. This was checked directly
against the raw per-seed numbers, not just the summary statistic. Every
single random reordering of the *exact same candidate set* did better
than the crate's actual production ordering (ascending `PrecursorId`) at
this budget. This is real evidence for H2 (candidate order is a hidden,
unmanaged search policy) — and specifically suggests alphabetical-by-id
ordering itself may have an identifiable structural disadvantage (e.g.
compounds sharing a leading element symbol clustering together in index
space), not just "any specific ordering deviates from optimal." This is
reported as an open, real, actionable lead for future investigation, not
something this phase further diagnosed or explained mechanistically.

### H4 confirmed directly: order/fusion/tie-break sensitivity shrinks as budget grows

At `budget=10`, dev-sample recall spans 0.3954–0.4437 across every
policy tested (a ~4.8-point range). At `budget=100_000` (the
"sufficiently large" control), **every single policy across all three
sweeps converges to 0.4713**, byte-identical to four decimal places.
Order/fusion/tie-break only matter when the budget is tight enough that
not everything gets explored — exactly H4's prediction, and exactly why
PR 1/PR 2's own tight-budget calibration methodology was the right lens
for this whole investigation.

### Confirmation holdout (full split, n=623, never touched before the development-side choice above was locked)

Selected policy: `fusion=catalog-anchored`, budget=20 (the dev-split
winner among selectable, non-control policies — which, per the finding
above, is identical to catalog-exact itself).

- `catalog-exact` holdout recall: **0.3692** (623 rows; lower than the
  dev-sample's 0.4391, as expected — dev and holdout are disjoint real
  target populations, not two views of the same rows).
- Selected policy holdout recall: **0.3692** (identical, since the
  selected policy *is* catalog-exact's own order in this benchmark).
- AUC proxy (mean recall across the 6 budget points): catalog-exact
  0.3887, selected policy 0.3887 (identical).
- Target-group bootstrap (10,000 resamples): recall diff **0.0000**,
  95% CI **[0.0000, 0.0000]**, **0 gained / 0 lost** targets — an exact,
  trivial null result, consistent with the selected policy being
  literally the same ordering as the baseline.

### Verdict: **NO-GO**

No candidate order, fusion rule, or search tie-break tested in this phase
beats `catalog-exact` alone at the pre-registered fixed comparison
budget, on either the development split or the confirmation holdout.
This is a genuine, informative NO-GO, not a failed phase — matching the
same framing already established for Phase 29 and the chloride decision.

### What this phase actually established, mapped to the pre-registered hypotheses

- **H1** (min-rank fusion over-promotes a single generator's top pick):
  not cleanly isolated as *the* cause — every structured reordering
  tested (not just min-rank) underperformed catalog-exact by a similar
  margin, so this looks like a broader "deviating from catalog-exact's
  specific order tends to hurt" effect, not something unique to min-rank
  fusion's own combination rule.
- **H2** (candidate order is a hidden, unmanaged search policy):
  **confirmed directly and quantitatively** — the exact same candidate
  set, reordered only, swings dev-sample recall from 0.4000 to 0.4621 at
  a fixed budget (a 6.2-point range), and catalog-exact's own order sits
  at the very bottom of that range, below every tested shuffle.
- **H3** (added signals aren't target-reaction-aware, may be too weak):
  **still open** — this phase did not test a genuinely target-reaction-
  aware signal (see the "what is NOT established" section above); not
  resolved either way.
- **H4** (order-sensitivity shrinks as budget grows): **confirmed
  directly** — see the dedicated section above.
- **H5** (row-local list quality doesn't reliably convert to end-to-end
  recovery): partially supported by the `conv@20` column in the raw
  sweep data (`benchmarks/data/exploration_fusion_search_audit_result.json`)
  — e.g. `reciprocal-rank-fusion`'s conditional conversion rate
  (0.5487) trails `catalog-exact`/`catalog-anchored`'s (0.6739) by more
  than their respective end-to-end recall gap alone would suggest — but
  this phase did not build the dedicated conversion-funnel analysis that
  would make this crisp; flagged as a real lead, not a closed finding.

### Recommendation to the owner

Per the pre-registered decision framing (§ "Decision gate"): the
byproduct-allow-list-style "add another signal" pattern is not what's
missing here — three separate structured-reordering attempts (PR 1
frequency, PR 2 thermodynamic stability, and this phase's 6 fusion rules
+ 3 tie-breaks) have now all failed to beat catalog-exact's plain order
at a fixed tight budget. The one lead this phase surfaced that the
byproduct arc's own pattern doesn't explain: **uninformed random
reordering beats catalog-exact's real order 10/10 times**, which the
structured alternatives don't reproduce. This suggests the next
productive step, if pursued, is investigating *why* alphabetical-by-id
ordering specifically underperforms an unstructured shuffle at the
frontier-mechanism level (e.g. a targeted trace on a handful of holdout
rows where catalog-exact fails but every shuffle succeeds) — not adding
a 4th generator (`PriorExperimentGenerator`) or 5th (`LiteratureAnalogGenerator`)
on the same premise the last three attempts already falsified. PR 3/PR 4
remain paused pending the owner's own decision on this recommendation.
