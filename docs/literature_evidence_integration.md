# v0.4.0 Integration: reference-only literature evidence in `Planner`

Connects Phase 20C's cross-DOI comparison (`docs/literature_observation_provider.md`'s
"Cross-DOI comparison" section) to `Planner`, per the owner's explicit
scope decision: Phase 20D's audit found the corpus's auto-applicable
fraction is effectively 0%, and Phase 20C found 53.6% of routes with
independent-DOI replication disagree on their own step count and ~78%
of comparable temperature step-groups conflict. Given that, Integration
is **not** condition auto-fill -- it is reference-only evidence and
warning surfacing, strictly.

## What was added to `Planner`

- **`SynthesisPlan.literature_evidence: Option<LiteratureRouteEvidence>`**
  -- populated when a `LiteratureEvidenceProvider` is configured and finds
  a match for that plan's exact `(target, precursors, route_family)`.
  `None` otherwise, including for every plan when no provider is
  configured (default, backward-compatible with every prior `Planner`
  constructor).
- **`Planner::with_literature_evidence_provider(catalog, provider, config)`**
  -- a new constructor mirroring `with_route_suitability_provider`'s
  shape exactly.
- **A disclosure warning** (`WarningSeverity::Info`) on every plan that
  gets `literature_evidence` attached at all -- not only when it shows
  `has_multiple_operation_shapes` or a field-level `Conflict`. A clean,
  unanimous `Agreement` gets the same base disclosure, e.g. *"literature
  evidence for this exact route: 2 independent DOI(s) found --
  reference-only, never applied to conditions or score"*, with extra
  clauses appended when there's shape diversity and/or a field
  conflict, e.g. *"...4 independent DOI(s) found, with differing
  reported step counts across DOIs, including a field-level
  disagreement among independent DOIs -- reference-only, never applied
  to conditions or score."* Unconditional by design (pre-commit advisor
  review finding): an Agreement-only match with no warning at all would
  have been the one case most likely to be misread as "the corpus
  endorses this temperature," precisely because it's silent.

## What was deliberately not added (the owner's explicit forbid-list)

- No `ProcessStep` temperature/duration/atmosphere auto-fill.
- No `uncertainty_penalty` reduction, no `confidence` increase, no
  `score`/ranking change of any kind.
- No conversion to `ConditionPrecedent`.
- No `HeatingPurpose` inference.
- No application to `Mechanochemical` (the corpus has zero evidence for
  it -- Phase 20A).
- No `ThermodynamicProvider`/`Score01` connection of any kind.

## Why this is structural, not a promise

`score_plan` (`score.rs`) takes exactly three inputs that can move
score/confidence: `evidence: &[PlanningEvidence]`,
`condition_conflicts: &[ConditionConflict]`, and
`process_evidence_provider_consulted: bool`. `LiteratureRouteEvidence`
is never placed into any of the three -- it is looked up and attached to
`SynthesisPlan` *after* `score_plan` has already returned, as its own,
separate field, one score_plan itself never receives as an argument.
This mirrors `literature_observations.rs`'s own "structural, not
unwired" argument for `heating_purpose`/`ConditionPrecedent`: the
non-connection isn't something a caller has to trust a comment about,
it's checkable from the function signature.

Verified two ways:
- A permanent unit test
  (`literature_evidence_provider_attaches_evidence_without_changing_score_or_steps`,
  `planner.rs`) configures a provider that *always* returns a real
  `Conflict` and `has_multiple_operation_shapes: true`, then asserts
  `score`, `confidence`, `steps`, and plan ranking order are byte-for-byte
  identical to a baseline run with no provider configured at all.
- The real-corpus benchmark below re-confirms this over 324 real plans,
  not just one synthetic case.

## Feature-gating design

`LiteratureEvidenceProvider` (`provider.rs`) and
[`LiteratureRouteEvidence`]/[`RouteObservationAssessment`]/[`StepGroupAssessment`]/[`StepGroupKey`]/[`CrossDoiFieldStatus`]/[`SourcedValue`]
(now in `src/literature_evidence.rs`, moved out of the
`literature_corpus`-gated `literature_observation_conflicts.rs` for
exactly this reason) are **always compiled** -- `Planner`'s public API
and `SynthesisPlan`'s JSON schema never change shape depending on which
crate features are enabled. Only the one real implementation,
`LiteratureObservationCorpusProvider` (backed by
`LiteratureObservationCorpus::cross_doi_comparisons`), lives behind
`literature_corpus` -- the same split `ThermodynamicProvider` already
has against the feature-gated `MaterialsProjectSnapshotProvider`.

`LiteratureObservationCorpusProvider::new(&corpus)` calls
`cross_doi_comparisons()` exactly once and indexes the result by route;
each `route_evidence()` call is an O(log n) `BTreeMap` lookup, not a
fresh corpus-wide pass -- this is what keeps the per-plan lookup cost
low (measured below).

## `Mechanochemical` guard, checked at two independent points

`LiteratureObservationCorpusProvider::route_evidence` returns `Ok(None)`
immediately for any non-`ConventionalSolidState` route family (mirrors
`find_exact`'s own gate). `Planner::plan`'s call site *also* checks
`template.route_family == RouteFamily::ConventionalSolidState` before
even calling the provider. A dedicated test
(`literature_evidence_provider_is_never_asked_about_mechanochemical`)
uses a provider that records every route family it's asked about and
asserts `Mechanochemical` never appears in that log -- a test that would
fail if either guard were ever removed, not one that merely happens to
pass because the corpus has nothing to say about that route family
anyway.

## `RouteObservationAssessment` repeats route identity -- kept, deliberately

`LiteratureRouteEvidence.assessment` carries its own
`target`/`precursors`/`route_family`, which duplicate the
`SynthesisPlan` fields it's attached to. Raised in pre-commit advisor
review as worth a stated decision rather than leaving unexamined, since
this is a public schema field and cheap to change now, expensive later.
Decision: **keep it**. `RouteObservationAssessment` is also
`cross_doi_comparisons()`'s own top-level return type -- a caller who
extracts just `plan.literature_evidence` (to log it, diff it, or hand
it to something that never saw the enclosing report) gets a
self-contained value that doesn't require the parent `SynthesisPlan`
for context. The duplication cost is small and known (part of the
~3.3 KB/plan delta measured below); the self-containment is worth it.

## Real corpus-wide numbers

`cargo run --release --example literature_evidence_integration_report --features literature_corpus`,
2026-08-15, against the 13,969-observation local snapshot.

**Sampling method, stated honestly**: the 200 sampled targets are drawn
from `cross_doi_comparisons()`'s own output -- routes *already known* to
have comparable cross-DOI evidence. This deliberately maximizes overlap
so the coverage numbers below are measured against real matches, not
diluted by targets the corpus has nothing to say about; it is not a
claim that this sample represents a typical planning workload.

**Catalog scoping, corrected during pre-commit advisor review**: each
sampled target is planned against a catalog containing only *that
route's own* precursors (2-4 candidates), matching what a real caller's
catalog for one planning call looks like. An earlier version of this
benchmark shared one corpus-wide catalog (built from the union of all
200 sampled routes' precursors) across every target, which let a target
whose own route needs e.g. just BaCO3 + TiO2 also see, and reject,
every other sampled route's precursors that happened to share an
element (almost everything shares O) -- inflating `rejected_candidates`
and therefore report size by roughly three orders of magnitude versus a
realistic catalog. The numbers below are the corrected, per-target-
scoped measurement.

- **Route-level DOI replication** (direct corpus computation, not
  sampled): 4,010 routes have exactly 1 DOI; 886 routes have 2+
  independent DOIs reporting *some* operation shape for that route
  (looser than, and therefore larger than, Phase 20C's own 619-route
  "shape-and-position-matched comparable group" figure -- two different
  metrics, not a contradiction; 619 is also exactly the population this
  benchmark's 200-target sample is drawn from).
- **Provider index construction** (`cross_doi_comparisons`, once):
  ~38.5 ms.
- **Corpus lookup** (200 direct `route_evidence` calls, bypassing
  `Planner`): ~494 µs total, ~2.47 µs/query.
- **Planning runtime** (200 targets, both planners, each against its
  own small route-scoped catalog): baseline (`offline_minimal`) ~3.6 ms
  total (~18.1 µs/target); with the literature evidence provider
  configured, ~3.9 ms total (~19.4 µs/target) -- delta ~256 µs across
  200 targets, noise-level in absolute terms.
- **Report JSON size**: 200 baseline reports serialize to 1,824,491
  bytes (~9.1 KB/report, realistic for a route-scoped catalog); with
  evidence attached, 2,360,151 bytes -- +535,660 bytes (+29.4% of the
  whole-sample total). That whole-sample percentage is diluted by plans
  with no literature match at all, so the portable number is the
  per-evidence-carrying-plan delta: **+3,327 bytes per plan that
  actually has `literature_evidence` attached** (161 such plans in this
  run).
- **Coverage**: 324 total plans generated across the 200 sampled
  targets (each target can produce more than one plan -- alternate
  precursor combinations from its own small catalog, or a second route
  family). 161 of 324 (49.7%) plans had `literature_evidence` attached
  -- the exact-match requirement (target *and* precursor set *and*
  route family, all exact) means a target's *other* generated plans
  (different precursor choices within its own small catalog) correctly
  get no match, this is not under-coverage. Of those 161: 106 (65.8%)
  flagged `has_multiple_operation_shapes`.
- **Per-field status across those 161 plans' step groups**: temperature
  44 agreement / 291 conflict / 71 insufficient-sources / 22 unresolved;
  duration 66 / 235 / 96 / 31; atmosphere 102 / 20 / 146 / 160.
  Consistent with Phase 20C's own corpus-wide figures (temperature
  conflicts far more often than it agrees; atmosphere reaches a real
  verdict for a minority of step groups, since most atmosphere data is
  free text excluded from comparison).
- **Non-interference, checked as a hard assertion in the benchmark
  itself, not just observed**: 0 score/confidence/ranking inversions, 0
  `ProcessStep` changes, across all 324 real plans.

**"Coverage increased" is never read here as "condition accuracy
improved."** More literature evidence being surfaced means more routes
now carry a disclosure (agreement, conflict, or shape diversity) --
it says nothing about whether any specific temperature/duration/
atmosphere value is correct. See `docs/literature_observation_accuracy_audit.md`
for what is actually known about extraction accuracy (small-n,
population-level base rates only).

## What this does not establish

Not a claim that `literature_evidence`-carrying plans are more likely to
succeed, more accurate, or more complete than plans without it -- see
"per-field status" above: even where evidence exists, it is conflict
far more often than agreement. Not a confidence or ranking signal of any
kind -- `score`/`confidence`/ranking are provably unaffected (see
above). Not a promotion mechanism -- no future phase's design is implied
by this one; "Integration" as scoped here is presentation of existing
Phase 20C data through `Planner`'s report, nothing more.
