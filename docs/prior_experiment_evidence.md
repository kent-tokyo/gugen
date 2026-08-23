# Phase 26: reference-only prior-experiment evidence in `Planner`

Connects Phase 25's `SynthesisExecutionRecord` (`docs/synthesis_execution_record.md`)
to `Planner`, per the owner's own scope: "surface Phase 25 records as
reference-only evidence during planning... same discipline as the
existing literature-evidence integration: display-only, score unchanged,
ranking unchanged, conflicts never hidden, never described as a success
rate." Same non-interference architecture as
`docs/literature_evidence_integration.md`, mirrored deliberately -- read
that doc first if this one assumes something unfamiliar.

## What was added to `Planner`

- **`SynthesisPlan.prior_experiment_evidence: Option<PriorExperimentEvidence>`**
  -- populated when a `PriorExperimentEvidenceProvider` is configured and
  finds a match for that plan's exact `(target, precursors, route_family)`.
  `None` otherwise, including for every plan when no provider is
  configured (default, backward-compatible).
- **`PlannerBuilder::prior_experiment_evidence_provider(provider)`** -- a
  new builder method, the crate's 5th optional provider.
- **`PriorExperimentEvidence::outcome_tally()`** -- groups matched
  records by `SynthesisOutcome`, in that enum's own declared order (via
  an internal `BTreeMap`), e.g. the owner's own example renders as
  `[(TargetPhaseObtained, 2), (CompetingPhaseObserved, 1),
  (Inconclusive, 1)]`.
- **A disclosure warning** (`WarningSeverity::Info`) on every plan that
  gets `prior_experiment_evidence` attached at all, unconditionally --
  same "an all-success match is exactly the case most likely to be
  misread as endorsement if left silent" reasoning the literature-
  evidence warning already established. Wording states plainly: *"N
  prior experiment record(s) for this exact route: \<tally\> --
  reference-only; recorded conditions, grades and catalogs differ
  between records, so this is not a success rate, and none of it is
  applied to conditions or score."*

## What was deliberately not added (the owner's explicit forbid-list)

- No `ProcessStep` auto-fill of any kind.
- No `uncertainty_penalty` reduction, no `confidence` increase, no
  `score`/ranking change of any kind.
- No numeric condition-range/tolerance matching -- process conditions,
  selected commercial offers, and catalog provenance are shown as-is on
  each matched record, never compared against each other or against the
  new plan's own (usually unresolved) conditions. This crate has no
  existing primitive for "is this measured value approximately equal to
  that one," and inventing a tolerance the owner's spec never specified
  would be exactly the kind of fabricated structure this crate's own
  discipline (Phase 19's "no invented average, show unresolved instead")
  argues against.
- No `ThermodynamicProvider`/`Score01` connection of any kind.
- No CLI subcommand this phase (mirrors Phase 25's own "library-only for
  now" decision).

## Why this is structural, not a promise

Same argument as literature evidence, word for word: `score_plan`
(`score.rs`) takes exactly three inputs that can move score/confidence
(`evidence`, `condition_conflicts`, `process_evidence_provider_consulted`).
`PriorExperimentEvidence` is never placed into any of the three -- it is
looked up and attached to `SynthesisPlan` *after* `score_plan` has
already returned, as its own field `score_plan` itself never receives as
an argument.

Verified the same two ways literature evidence was:
- A permanent unit test
  (`prior_experiment_evidence_provider_attaches_evidence_without_changing_score_or_steps`,
  `planner.rs`) configures a provider that always returns evidence for
  every plan, then asserts `score`, `confidence`, `steps`, and plan
  ranking order are byte-for-byte identical to a baseline run with no
  provider configured.
- No corpus-wide benchmark exists for this phase (see "No real
  corpus-wide numbers" below) -- the unit test above is the only
  verification, honestly stated as such rather than implying a
  benchmark that wasn't run.

## Why no feature gate is needed here

Unlike `LiteratureEvidenceProvider` (whose report types are always
compiled, but whose one real implementation lives behind the
`literature_corpus` feature because that implementation's corpus loader
needs it), `PriorExperimentEvidenceProvider` and its one real
implementation, `InMemoryExecutionRecordProvider`, need **no feature
gate at all**. `SynthesisExecutionRecord` (Phase 25) itself carries no
feature gate -- only its own JSON-parsing function, `parse_execution_records`,
is gated behind `serde` -- so by the time a `Vec<SynthesisExecutionRecord>`
reaches `InMemoryExecutionRecordProvider::new`, it has already been
parsed by the caller; this module performs no file I/O and no JSON
parsing itself, so it inherits none of the constraints that gate its
literature-evidence counterpart.

`InMemoryExecutionRecordProvider::new(records)` groups once, at
construction, into a `BTreeMap` keyed by the exact identity triple
(target composition, canonical precursor set, route family) --
`Composition` doesn't derive `Hash`, so `BTreeMap`, not `HashMap`, same
constraint `LiteratureObservationCorpusProvider` has. Each
`prior_experiments()` call is an O(log n) lookup, not a fresh pass over
every record.

## No route-family guard, and why this diverges from literature-evidence

`LiteratureObservationCorpusProvider` returns `Ok(None)` for any
non-`ConventionalSolidState` route family, and `Planner::plan`'s call
site checks the same restriction independently -- both exist because the
literature corpus specifically has zero evidence for `Mechanochemical`
(Phase 20A). No such restriction exists for prior-experiment evidence,
deliberately: `route_family` is already part of the exact-match identity
key, so a `Mechanochemical` plan can only ever match `Mechanochemical`
records -- cross-family leakage is structurally impossible regardless of
whether the call site restricts which route families it asks about. A
dedicated test (`prior_experiment_evidence_surfaced_for_mechanochemical_plans`)
pins this divergence explicitly, rather than leaving it as something a
future contributor might "fix" into matching literature-evidence's own
gate by mistake.

## Why `PriorExperimentEvidence` carries whole records, not a lighter summary

`PriorExperimentEvidence.records: Vec<SynthesisExecutionRecord>` carries
every matched record in full (not a projected/lighter summary type).
This falls directly out of the owner's own spec: process conditions,
selected commercial offers, and catalog provenance must be visible on
each match for the reader to judge comparability themselves ("not
comparable across conditions/grades" -- the owner's own phrase). A
lighter summary type would need to guess in advance which fields a
reader might want to compare; carrying the whole record avoids that
guess entirely, and (`SynthesisExecutionRecord` already being
`Serialize`d whole elsewhere, Phase 25) costs no new type.

## No real corpus-wide numbers

Unlike literature evidence (backed by a 13,969-observation corpus with a
real benchmark example), there is no equivalent corpus for prior
experiments yet -- `SynthesisExecutionRecord`s only exist once real
labs actually log them (Phase 25 shipped the format; nothing has
populated it at scale). No benchmark is claimed here that wasn't run.

## What this does not establish

Not a claim that `prior_experiment_evidence`-carrying plans are more
likely to succeed than plans without it. Not a confidence or ranking
signal of any kind -- `score`/`confidence`/ranking are provably
unaffected (see above). Not a promotion mechanism -- Phase 27 (outcome
calibration), if it ever ships, is gated on independent-data
reproduction of a real, measured effect, not implied by this phase's
existence. **Never a success rate**: `outcome_tally()` groups what was
recorded, not what a new attempt would produce, since matched records
are not required to share conditions, grades, or catalogs with each
other or with the plan they're attached to.
