# Changelog

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

## [0.6.0] - 2026-08-23

Commercial workflow surface, plus the first surfacing of past lab
outcomes back into planning -- together the first release closing the
loop **plan -> commercial precursors -> experiment -> reused result**: a
`gugen commercial-plan` CLI subcommand (Phase 24A), declarative CSV
column-name mapping for real-world supplier exports (Phase 24B), named
procurement ranking policies including a Pareto frontier (Phase 24C), an
append-only `SynthesisExecutionRecord` for what actually happened in the
lab (Phase 25), and reference-only prior-experiment evidence surfaced
during planning (Phase 26). Breaking (hence the minor version bump under
this crate's pre-1.0 SemVer convention) -- `cargo semver-checks
--baseline-version 0.5.0 --all-features`, run before this version bump
landed, flagged exactly one lint rule (`constructible_struct_adds_field`)
against exactly four fields and nothing else:
`CommercialCombination::min_purity`,
`CommercialCombination::total_excess_mass_grams`,
`CommercialOfferSelection::purity` (Phase 24C), and
`SynthesisPlan::prior_experiment_evidence` (Phase 26) -- expected
breaking findings only, no removed items, no signature changes, no
`#[non_exhaustive]`/trait/feature changes. Re-run after the version bump
landed (same command, current crate now at `0.6.0`) reports "no semver
update required," confirming the minor bump to `0.6.0` is exactly
sufficient. `report::SCHEMA_VERSION` moves `2 -> 3` for the identical
reason (`SynthesisPlan`'s new field). Full detail in the entries below.

### Added

- `gugen commercial-plan` (Phase 24A), behind the existing
  `commercial_catalog` feature: a new CLI subcommand exposing the
  `Planner` -> `SynthesisPlan` -> `assess_commercial_plans` flow (Phase
  22/22.1, previously only reachable from Rust code) without writing any
  code. Takes a target, a precursor catalog (`--catalog`, unchanged
  meaning from `plan`/`batch`), and a separate commercial-offers catalog
  (`--commercial-catalog`, CSV or JSON by file extension) -- kept as two
  distinct flags rather than one derived catalog, since the two inputs
  are structurally different data with no specified dedup rule between
  them. Assesses every plan in the target's report by default;
  `--plan-id` (an id from a prior `gugen plan` run) narrows to one.
  Optional `--target-mass-g`, `--min-purity`, `--max-lead-time-days`,
  `--max-total-cost`/`--currency`, `--allowed-manufacturer`/
  `--excluded-manufacturer` map onto `CommercialPlanningRequest`.
  `--format json|markdown|csv` (CSV is new: one row per ranked
  procurement combination). Purely additive -- no public library API
  change (`cargo semver-checks` confirms no breaking changes).
- Declarative CSV column-name mapping (Phase 24B), behind the same
  `commercial_catalog` feature: a real-world supplier export rarely uses
  gugen's exact canonical column names (`formula`, `manufacturer`, ...).
  New `CommercialCatalogColumnMap` type and
  `CommercialPrecursorCatalog::load_csv_with_column_map` (the existing
  `load_csv` is unchanged) rewrite the header row per a declarative
  canonical-name -> file-header-name map before parsing, so every row
  loads through the exact same logic as `load_csv` today -- no
  per-manufacturer adapters. `gugen commercial-plan`'s new
  `--commercial-catalog-column-map <path>` flag reads this from a small
  JSON file (only the columns that differ need an entry) and only
  applies to CSV catalogs (an error, not a silent no-op, if given
  alongside a `.json` `--commercial-catalog`). Purely additive: new
  public type and method, existing `load_csv` unchanged (`cargo
  semver-checks` confirms no breaking changes -- but unlike 24A, this
  phase does add public API, so the next release must be at least a
  minor version bump, e.g. `0.6.0`, not a patch release).
- Named procurement ranking policies (Phase 24C), behind the same
  `commercial_catalog` feature: a new `CommercialRankingPolicy` enum
  (`Balanced` -- today's original ranking, unchanged; `CostFirst`;
  `LeadTimeFirst`; `PurityFirst`; `MinimumUnresolvedData` -- identical to
  `Balanced` by construction, since unresolved-field count is already
  its primary key; `Pareto` -- the non-dominated set over cost, lead
  time, purity, and excess purchased mass). New sibling functions
  `assess_commercial_plans_with_policy`/
  `assess_commercial_precursors_with_policy`; the existing
  `assess_commercial_plans`/`assess_commercial_precursors` are unchanged
  thin wrappers always passing `Balanced` (pinned by a regression test).
  `gugen commercial-plan --ranking-policy <policy>` (default `balanced`).
  **This phase's new `min_purity`/`total_excess_mass_grams` fields on
  `CommercialCombination` and `purity` on `CommercialOfferSelection` ARE
  a breaking change**, confirmed by `cargo semver-checks`
  (`constructible_struct_adds_field`, both structs have no private
  fields so an external caller may already construct them via a full
  struct literal, which the new fields would break) -- semver-checks
  reports this as requiring a new major version, which for this
  pre-1.0 crate means the next release must bump the minor version
  (`0.6.0`), per Cargo's own SemVer compatibility rules for `0.y.z`.
  Not a patch release.
- `SynthesisExecutionRecord` (Phase 25): records what actually happened
  when a gugen-proposed plan was attempted in a real lab -- append-only,
  versioned-schema, local JSON/JSONL persistence, mandatory provenance,
  outcomes never edited toward "success" after the fact. `outcome` is a
  7-state enum (`TargetPhaseObtained`/`PartialTargetPhase`/
  `CompetingPhaseObserved`/`NoReactionObserved`/`ProcessFailed`/
  `Inconclusive`/`NotMeasured`), never success/failure. New unconditional
  module `src/execution_record.rs` (no new Cargo feature; the plain
  struct/enum definitions always compile, `parse_execution_records`/
  `ExecutionRecordLoadMode`/`ExecutionRecordLoadReport` are gated behind
  the existing `serde` feature since JSON parsing structurally needs it).
  **Structurally separate from `Planner`/`score_plan`, by construction**
  -- nothing in this module is read by, or feeds, a plan's score or
  ranking (same reference-only boundary `commercial_catalog`/
  `literature_evidence` already establish); surfacing these records back
  into planning as reference-only evidence is Phase 26, not this phase.
  `PlanIdentity` bundles `plan_id`/`route_family`/`target_composition`/
  `precursor_compositions` so a record is self-describing even without
  the originating `SynthesisPlanningReport` file still existing (a bare
  `PlanId` is an opaque hash, not reversible) -- matches Phase 26's own
  already-stated matching criteria. **No file I/O in this module**
  (confirmed no library module in this crate does any): `parse_execution_records`
  takes an in-memory JSONL `&str`; appending to/reading an actual file is
  the caller's own few-line responsibility (a future CLI, or user code --
  see the new `examples/execution_record_demo.rs`), the same boundary
  `CommercialPrecursorCatalog::load_csv` already draws. Purely additive;
  `cargo semver-checks` reports no new breaking changes from this module
  (the one pre-existing breaking finding is Phase 24C's, above,
  unrelated).
- Reference-only prior-experiment evidence in `Planner` (Phase 26):
  surfaces Phase 25's `SynthesisExecutionRecord`s during planning,
  matched on a plan's exact `(target, precursors, route_family)` --
  same discipline as the existing literature-evidence integration:
  display-only, score/ranking unchanged, conflicts never hidden, never
  described as a success rate. New `SynthesisPlan.prior_experiment_evidence:
  Option<PriorExperimentEvidence>`, new `PlannerBuilder::prior_experiment_evidence_provider(...)`
  (the crate's 5th optional provider), new `PriorExperimentEvidenceProvider`
  trait (`provider.rs`) and its one real implementation,
  `InMemoryExecutionRecordProvider` (unconditional module
  `src/prior_experiment_evidence.rs`, no new Cargo feature -- Phase 25's
  own types carry none either). `PriorExperimentEvidence::outcome_tally()`
  groups matched records by outcome (e.g. the owner's own example: "2
  TargetPhaseObtained, 1 CompetingPhaseObserved, 1 Inconclusive") --
  **explicitly not a success rate**, since matched records are not
  required to agree on process conditions, grades, or catalogs with each
  other or with the plan they're attached to; those fields are shown
  as-is on each record, never compared or averaged. Deliberately **not**
  restricted to `ConventionalSolidState` (unlike literature evidence,
  whose one real implementation happens to have no `Mechanochemical`
  data) -- `route_family` is already part of the exact-match key, so
  cross-family leakage can't happen regardless.
  **`SynthesisPlan.prior_experiment_evidence` is a breaking change**,
  the identical `constructible_struct_adds_field` class `cargo
  semver-checks` already flagged when `literature_evidence` was added
  (`report::SCHEMA_VERSION` bumped `2 -> 3` for the same reason it moved
  `1 -> 2` then). Reinforces, not newly establishes, that the next
  release must be at least `0.6.0`.

### Changed

- `report::SCHEMA_VERSION` moved `2 -> 3`: `SynthesisPlan` gained
  `prior_experiment_evidence: Option<PriorExperimentEvidence>` (Phase
  26), the same JSON/Markdown-shape-affecting class of change that moved
  it `1 -> 2` when `literature_evidence` was added.

### Breaking changes

- `CommercialCombination` (Phase 24C) gained two new public fields:
  `min_purity: Option<PurityFraction>` and
  `total_excess_mass_grams: Option<f64>`.
- `CommercialOfferSelection` (Phase 24C) gained one new public field:
  `purity: Option<PurityFraction>`.
- `SynthesisPlan` (Phase 26) gained one new public field:
  `prior_experiment_evidence: Option<PriorExperimentEvidence>`.

  All three structs have every field public and no `#[non_exhaustive]`,
  so an external caller may already construct them via a full struct
  literal -- adding a field breaks that construction. `cargo
  semver-checks` classifies this as `constructible_struct_adds_field`,
  which requires a new major version in strict SemVer terms; per Cargo's
  own compatibility rules for a pre-1.0 (`0.y.z`) crate, that maps to a
  required **minor** version bump, which is what this release is.

### Migration notes

- If you construct `CommercialCombination`, `CommercialOfferSelection`,
  or `SynthesisPlan` via a struct literal, add the new field(s) above
  (all three are `Option`, so `None` is always a valid value to add).
  Where practical, prefer consuming these types from gugen's own
  constructors/returned values instead of hand-building them, so future
  additive field growth doesn't require a source change on your side.
- `SynthesisPlanningReport`/`SynthesisPlan`'s JSON and Markdown report
  schema version moved `2 -> 3` (`report::SCHEMA_VERSION`). A strict
  deserializer that rejects unknown fields needs updating to account for
  `SynthesisPlan.prior_experiment_evidence`.
- `SynthesisPlan.prior_experiment_evidence` is
  `Option<PriorExperimentEvidence>` and is `None` whenever no
  `PriorExperimentEvidenceProvider` is configured on
  `Planner`/`PlannerBuilder` -- existing callers who don't opt in see no
  behavior change beyond the new field's presence.
- Prior-experiment evidence (Phase 26) and commercial-catalog data (Phase
  22/24) are both reference-only: neither is a synthesis-success
  probability, neither is learned from past outcomes, and neither
  changes a plan's `score`, `confidence`, or ranking order. Named
  commercial ranking policies (`CommercialRankingPolicy`, Phase 24C)
  change how *commercial offers* are ranked for procurement -- they
  never change the underlying scientific plan ranking `Planner::plan`
  produces.

## [0.5.0] - 2026-08-22

Validated Core API: closes a long-standing gap where `BalancedReaction`/
`ReactionSpecies` could be hand-constructed (or deserialized from
untrusted JSON) with a zero coefficient, an empty side, or a
non-element-conserving reaction, silently relying on downstream
consumers to catch it (Phase 23A); adds `Planner::builder(...)`
covering any combination of the 4 optional providers, deprecating the
5 named constructors that didn't (Phase 23B); extends the existing
provider-call dedup (v0.4.2, PR #37) to `ProcessEvidenceProvider::
precedents` (Phase 23C); and marks 5 enums `#[non_exhaustive]` where
their own doc comments already anticipated new variants, plus a new
`docs/api_stability_policy.md` (Phase 23D). Breaking (hence the minor
version bump under this crate's pre-1.0 SemVer convention) — `cargo
semver-checks` against the 0.4.2 baseline flags exactly 3 lint rules,
covering exactly the intended changes and nothing else: the 3 newly
private fields (Phase 23A, `BalancedReaction::reactants`/`products`,
`ReactionSpecies::coefficient`, reported under both
`struct_pub_field_missing` and `struct_pub_field_now_doc_hidden`) and
the 5 new `#[non_exhaustive]` enums (Phase 23D, `enum_marked_
non_exhaustive`) — `#[deprecated]` alone is correctly not flagged as
breaking. `SCHEMA_VERSION` stays `2` — no
`SynthesisPlanningReport`/`SynthesisPlan` shape change. Full detail in
the entries below and in `docs/api_stability_policy.md`.

### Changed (breaking, Phase 23A)

- `BalancedReaction`'s `reactants`/`products` fields and `ReactionSpecies`'s
  `coefficient` field are now private. Construct via `BalancedReaction::new`/
  `ReactionSpecies::new` (both already existed) and read via new
  `BalancedReaction::reactants()`/`products()` and
  `ReactionSpecies::coefficient()` accessors. `ReactionSpecies::composition`
  stays a public field (already a validated type).
- `BalancedReaction::new` now also rejects a reaction whose reactant- and
  product-side element totals don't conserve
  (`GugenError::UnbalancedReaction`, reusing the existing variant) — a
  category of chemically-invalid input it previously accepted silently,
  relying on downstream consumers
  (`thermodynamics::balanced_reaction_delta_ev_per_atom`,
  `MaterialsProjectSnapshotProvider::reaction_energy`) to catch it instead.
  Those downstream checks still run, now redundantly.
- JSON deserialization of `BalancedReaction`/`ReactionSpecies` now re-runs
  the same validation as the constructors (previously a derived
  `Deserialize` could construct an invalid value directly from untrusted
  JSON, bypassing every check `::new` performed).

### Added (Phase 23B)

- `Planner::builder(catalog, config)` returns a new `PlannerBuilder` with
  `.thermodynamic_provider(...)`, `.process_evidence_provider(...)`,
  `.route_suitability_provider(...)`, `.literature_evidence_provider(...)`,
  and `.build()` (infallible) — covers any combination of `Planner`'s 4
  optional providers, including combinations none of the 5 named
  constructors below covered.

### Deprecated (Phase 23B)

- `Planner::new`, `Planner::offline_minimal`,
  `Planner::with_process_evidence_provider`,
  `Planner::with_route_suitability_provider`, and
  `Planner::with_literature_evidence_provider` are deprecated in favor of
  `Planner::builder(...)` above. Each still works, unchanged, as a thin
  wrapper around the builder — no removal planned for a specific release
  yet.

### Fixed (Phase 23C)

- `Planner::plan` no longer calls `ProcessEvidenceProvider::precedents`
  once per route family sharing an accepted precursor set — it's now
  cached once per accepted set, the same way `ThermodynamicProvider::
  reaction_energy`/`competing_phases` were deduplicated in v0.4.2 (PR
  #37), which deliberately left this provider untouched at the time. No
  behavior change for a single-route-family plan; a target that produces
  plans under more than one route family (e.g. both
  ConventionalSolidState and Mechanochemical) now makes one
  `precedents` call per accepted set instead of one per resulting plan.

### Changed (breaking, Phase 23D)

- `RouteFamily`, `InertGas`, `MixingMethod`, `GrindingMethod`, and
  `FormingMethod` are now `#[non_exhaustive]` — each already had a doc
  comment stating "add a variant only when a real fixture needs one";
  this marks that intent in the type system. Exhaustive `match`
  expressions on these enums outside this crate need a wildcard `_`
  arm. Matches the existing `SuitabilityVerdict`/`RouteRecommendation`
  precedent (Phase 15A).
- New `docs/api_stability_policy.md`: states what `SCHEMA_VERSION` does
  and doesn't guarantee, what `#[non_exhaustive]` covers in this crate
  going forward, and a known `cargo semver-checks` blind spot.

## [0.4.2] - 2026-08-22

Adds the Commercial Precursor Catalog: an optional, off-by-default
`commercial_catalog` feature that matches an existing `SynthesisPlan`'s
precursors against a caller-supplied catalog of commercial offers, as a
post-planning stage that never affects the plan's chemical score,
confidence, reaction, or process steps (Phase 22, PR #42), followed
immediately by a stabilization pass (Phase 22.1, PR #43) splitting the
new module into a responsibility-driven layout, adding property-based
and golden-fixture test coverage, and closing a real compatibility gap
(the assessment-result types couldn't be serialized under the `serde`
feature at all). No changes to any previously-existing public API
(`cargo semver-checks` confirms no breaking changes against the 0.4.1
baseline), no `SCHEMA_VERSION` bump. Full detail in the entries below and
in `docs/commercial_precursor_catalog.md`.

### Added

- Commercial Precursor Catalog (Phase 22), behind a new optional
  `commercial_catalog` feature: `assess_commercial_precursors`/
  `assess_commercial_plans` match an existing `SynthesisPlan`'s precursors
  against a caller-supplied catalog of commercial offers (price, purity,
  package size, supplier, availability, lead time), as a standalone
  post-planning stage that never affects `SynthesisPlan.score`,
  `.confidence`, `.balanced_reaction`, or `.steps` (proved by two new
  invariance tests, `tests/commercial_catalog_planner_invariance.rs`).
  CSV/JSON catalog import with a structured accepted/rejected load report
  (`CommercialPrecursorCatalog::load_csv`/`load_json`, `Strict`/`Lenient`
  modes); a new chemical formula parser (`Composition` had none); commercial
  offer matching on a canonical, scale-invariant element-ratio key (exact
  rational arithmetic, not `Composition::eq` — `Fe2O3` matches `Fe4O6` as
  the same substance at a different formula-unit scale, while hydrate vs.
  anhydrous, different oxidation states, and different compounds stay
  distinct; no alias or substitute-precursor inference); hard commercial
  constraints; stoichiometric quantity calculation with
  purity-adjusted purchase mass and package-count rounding; currency-safe,
  overflow-checked cost totals (`Money` as integer minor units, never
  `f64`); a bounded search over complete purchasing combinations. See
  [`docs/commercial_precursor_catalog.md`](docs/commercial_precursor_catalog.md)
  for the full design and known limitations. No real catalog data ships
  with gugen. No changes to any existing public API
  (`cargo semver-checks` confirms no breaking changes), no `SCHEMA_VERSION`
  bump, and `SynthesisPlan`/`Planner`/`score_plan` are untouched except for
  two `pub(crate)` (crate-internal only) visibility changes, both reused
  rather than duplicated: `thermodynamics.rs`'s existing atomic-weight
  table, and `frac.rs`'s existing exact-integer GCD (used to reduce a
  composition's element ratio to lowest terms for canonical commercial
  offer matching — see "Composition matching policy" in the design doc).

- Commercial Precursor Catalog stabilization (Phase 22.1): the
  `commercial_catalog` module's assessment-result types now derive
  `Serialize` under the `serde` feature — `CommercialPlanAssessment`,
  `CommercialCombination`, `CommercialOfferSelection`, and
  `UnresolvedCommercialField` (`Serialize` only, since each carries a
  `&'static str` field that can't deserialize into a non-`'static`
  borrow, matching this crate's existing precedent for other output-only
  report types); `CommercialExclusion`, `CommercialWarning`,
  `CommercialExclusionCode`, and `SearchBudgetSummary` get full
  `Serialize`/`Deserialize`. Purely additive (`cargo semver-checks`
  confirms no update required) — this was a real gap (none of these types
  could be serialized at all before), not a behavior change. Also:
  `src/commercial_catalog.rs` (previously one ~4,200-line file) is now a
  responsibility-split directory module
  (`src/commercial_catalog/{model,formula,loader,matching,quantity,search,assessment}.rs`)
  with zero public API path changes; a `proptest`-based property test
  suite (formula-parser round-trip and no-panic properties, canonical-
  ratio-key scale-invariance and false-positive-match properties);
  boundary tests for package-count rounding and `Money` overflow;
  randomized exact-tier-vs-brute-force and heuristic-tier-budget
  invariant tests; and a 40-offer end-to-end golden-fixture test.

### Known limitations

- Commercial offer matching bridges scale only, not identity: it never
  infers aliases or substitute precursors, and a genuinely different
  ratio, a hydrate vs. its anhydrous form, or a different compound never
  match, even when chemically related.
- The bounded combination search's total-cost ranking is a best-effort
  heuristic, not a guaranteed global optimum, specifically when a catalog
  spans more than one currency *and* the full combination space is too
  large to enumerate exactly within the configured evaluation budget (the
  common case — a small-to-moderate space per precursor — is always
  ranked exactly).
- `CurrencyCode` is format-validated (3 uppercase ASCII letters), not
  checked against the real ISO 4217 list.
- CSV tags use a single `;`-separated cell; no nested/structured tag
  metadata.

## [0.4.1] - 2026-08-15

A patch release: 4 behavior-affecting `src/` bug fixes and 1
doc-comment correction, found by an owner-requested full-crate bug
check and refactoring sweep. No new features, no public API changes
(`cargo semver-checks` confirms no breaking changes), no `SCHEMA_VERSION`
bump.

### Fixed

- `scale_to_integers` (reaction balancing) now correctly abstains
  (`Ok(None)`) rather than erroring out the entire precursor search when
  integer scaling overflows during the multiply step, matching its own
  documented overflow contract.
- `MaterialsProjectSnapshotProvider::reaction_energy` now rejects a
  hand-built element-imbalanced reaction (`ProviderError::MalformedRecord`),
  matching the element-conservation check its `thermodynamics.rs` sibling
  function already performs.
- `PlanningProvenance::enabled_features()` now reports the
  `chematic_crystal` and `literature_corpus` Cargo features, which it
  was silently omitting.
- `score_plan` no longer panics when a caller-supplied `RankingWeights`
  component is non-finite or negative; such a weight now contributes
  `0.0` to its side's weighted average instead, matching the function's
  existing all-zero-`weight_sum` fallback. No public signature change.

### Documentation

- Corrected a false "complete accounting" invariant in `LoadReport`'s
  doc comment (`literature_observations.rs`).

## [0.4.0] - 2026-08-15

An evidence-infrastructure release: gas-free solid finite-temperature
thermodynamic primitives, a bulk literature observation snapshot API,
cross-DOI agreement/conflict classification, and reference-only
literature evidence surfaced in `Planner`. **Not** a claim of improved
ranking accuracy, synthesis-success prediction, or auto-filled
conditions -- see each item below and `docs/literature_evidence_integration.md`'s
"What this does not establish" section for what this release
deliberately does not claim.

### Migration notes

Everything below is also disclosed inline in Added/Fixed; this section
exists so an upgrading consumer has one place to check.

- **`score_plan` now takes 9 parameters instead of 8** (new
  `condition_conflicts: &[ConditionConflict]`). Any direct caller of
  `score_plan` (not just through `Planner`) must update its call site.
  Confirmed breaking by `cargo semver-checks`
  (`function_parameter_count_changed`).
- **`GugenError` gained 3 new variants**: `NonPositiveMagnitude`,
  `UnbalancedReaction`, `InconsistentThermodynamicDataset`.
  `GugenError` is not `#[non_exhaustive]`, so **any downstream `match`
  on `GugenError` that does not already end in a wildcard `_ => ...`
  arm will fail to compile** until arms for these three variants (or a
  wildcard) are added. Confirmed breaking by `cargo semver-checks`
  (`enum_variant_added`).
- **`balanced_reaction_delta_ev_per_atom` and
  `decomposition_margin_ev_per_atom` now return `Result<Option<f64>>`
  instead of `Option<f64>`.** `Ok(Some(value))` replaces a bare
  `Some(value)`; `Ok(None)` replaces a bare `None` (a legitimate
  abstention); `Err(...)` is new -- invalid caller input that previously
  had no way to be reported from these two functions. Any caller
  matching on the old `Option<f64>` shape needs an added `?` or `match`
  arm for `Err`. **Not caught by `cargo semver-checks`** (it has no
  lint for a bare return-type shape change like this one) -- disclosed
  here specifically because the tool stays silent on it.
- **`SynthesisPlan` gained a `literature_evidence:
  Option<LiteratureRouteEvidence>` field.** `SynthesisPlan` is not
  `#[non_exhaustive]` and has all-public fields, so any downstream
  struct-literal construction or exhaustive destructuring of it needs
  updating. Confirmed breaking by `cargo semver-checks`
  (`constructible_struct_adds_field`). `SCHEMA_VERSION` is bumped `1 ->
  2` to mark this addition. Note for consumers with their own strict
  JSON schema (e.g. a hand-written schema or a `serde` struct with
  `#[serde(deny_unknown_fields)]`) validating gugen's report output:
  `2` does not cleanly delimit "the v0.3.0 shape" from "the v0.4.0
  shape" -- `SCHEMA_VERSION` stayed `1` through v0.1.0/v0.2.0/v0.3.0
  even though the report gained fields in that span too (e.g.
  `route_suitability`/`not_recommended` in v0.3.0). A consumer that
  already tolerates unknown fields needs no change; a consumer that
  rejects them needs updating regardless of which schema_version number
  it's keyed on.
- **New optional `literature_corpus` Cargo feature** (depends on
  `serde`, off by default) -- new to this release (introduced in Phase
  20B, after v0.3.0). Existing optional features (`serde`, `clap`,
  `mikiwame`, `chematic_crystal`, `materials_project`) are unchanged.
  Enabling no new feature (the default build) is unaffected by this
  addition beyond the struct-literal/match-exhaustiveness changes
  above, which apply regardless of features enabled.
- No bulk literature-corpus data is bundled in the published package --
  `benchmarks/data/*.json`/`*.jsonl` stay excluded (see Fixed below);
  this is a packaging fact, not an API change, listed here for
  completeness.

### Fixed

- **Phase 19 — order-independent literature-condition conflict
  resolution.** `apply_condition_precedents` (`src/process.rs`) used to
  fill each `Heat` step field on a first-match-wins basis across whatever
  order its `ConditionPrecedent` slice happened to arrive in -- if two
  precedents ever supplied different values for the same field, whichever
  ran first silently won and the second was discarded with no record of
  the disagreement. Latent since `Planner::plan` called
  `apply_condition_precedents` once per `ProcessPrecedent` in a loop
  (rather than once over all of them together) and `curated_records()`
  has so far had only one record per target, so it never actually
  triggered -- but it would have as soon as a target ever had two
  differing curated (or otherwise provider-supplied) records, silently
  making a plan's firing temperature depend on corpus/provider ordering.
  Fixed before any condition-provider data expansion, per the owner's
  explicit directive that this fix must come first. Now:
  `Planner::plan` collects every `ConditionPrecedent` across every
  `ProcessPrecedent` a provider's single `precedents()` call returns
  into one flat list and applies them in a single, order-independent
  pass; each of the four fields (temperature/duration/atmosphere/ramp)
  on each step is resolved independently (field-granular, not
  precedent-granular -- two precedents disagreeing on temperature but
  agreeing on duration still resolve duration) by exact-value equality
  (no overlap/subsumption semantics for range types -- out of scope for
  this phase); a field with two or more distinct values is left
  unresolved rather than picking one or averaging, and its
  `UnresolvedRequirement.reason` names the conflict and cites every
  disagreeing source, replacing the generic (and, in this case, false)
  "no matching precedent" text. A related ordering hole surfaced during
  review of this same fix: the *resolved* (non-conflicting) case
  aggregated per-precedent evidence into a `BTreeMap` keyed by each
  precedent's position in the caller's slice, so two precedents with
  asymmetric field coverage that both resolved fields on one step could
  emit their `PlanningEvidence` entries in different sequence depending
  on provider return order -- same set, different order, which would
  have made a plan's serialized `evidence` array (and therefore any
  golden JSON snapshot) provider-order-dependent the moment a target
  ever had two contributing records for one step. Fixed by sorting each
  step's newly-resolved evidence entries by their own content
  (source/statement/limitations) before appending, rather than by
  precedent input position.
- **Phase 19P.1 — three correctness gaps in Phase 19P's finite-temperature
  module (`src/thermodynamics.rs`, PR #19, `fc60d07`), found by the
  owner's own review of the merged code rather than by either advisor
  review round during that phase.** (1) Neither
  `balanced_reaction_delta_ev_per_atom` nor
  `decomposition_margin_ev_per_atom` checked that the
  `SolidThermodynamicEntry` values they actually used shared a single
  `ThermodynamicDatasetIdentity` -- a caller could unknowingly mix
  entries from two different releases/correction schemes, or (worse) the
  private lowest-0K-energy tie-break in `most_stable_entry_for` could
  silently select across two different datasets' entries for the same
  composition. Both functions now reject (`GugenError::
  InconsistentThermodynamicDataset`) the moment the entries relevant to
  the call span more than one dataset identity, checked *before* any
  entry selection runs, so the tie-break itself is never given the
  chance to compare across datasets. (2) `decomposition_margin_ev_per_atom`'s
  `amount: f64` parameter had no validation at all -- a `NaN` amount in
  particular would silently pass the composition-conservation tolerance
  comparison (`(x - y).abs() > tol` is `false` for `NaN`) and produce
  `Some(NaN)`. Amounts are now required finite and strictly positive
  (`GugenError::NonFiniteValue`/`NonPositiveMagnitude`). (3)
  `balanced_reaction_delta_ev_per_atom` only documented, as an unchecked
  doc-comment precondition, that its `reaction` argument must be
  element-balanced; `BalancedReaction::new` itself only rejects an empty
  side or a zero coefficient, so a hand-built, unbalanced reaction could
  previously produce a plausible-but-meaningless per-atom delta. A
  runtime element-conservation check (reactant-side vs. product-side
  per-element totals) now rejects any mismatch
  (`GugenError::UnbalancedReaction`). None of these three gaps were
  reachable through any code path connecting to `Planner`/`score_plan`
  (Phase 19P's own ranking-invariance test already proved that
  boundary), so no plan output was ever affected -- these were latent
  correctness holes in new, not-yet-released public API, not a bug a
  user could have hit through the crate's existing planning surface.
- **Bulk data shipping in the published crate package (Phase 20B).**
  `Cargo.toml` had no `exclude`, so `cargo package`/`cargo publish` used
  its default `git ls-files`-based inclusion with nothing removed --
  `cargo package --list` confirms `benchmarks/data/kononova_sample.jsonl`
  (472 KB, committed since Phase 11 / v0.2.0) has been shipping in every
  published tarball through v0.3.0. Added `exclude =
  ["benchmarks/data/*.json", "benchmarks/data/*.jsonl"]` under
  `[package]`; `cargo package --list` before/after confirms this is now
  the only line removed from the package file list. Only small fixtures
  under `tests/fixtures/` (already included, always were) ship; bulk
  corpus data of any kind does not. Note on prior releases:
  `kononova_sample.jsonl` is CC BY 4.0 data, so shipping it was an
  attribution-obligation question, not only a size one --
  `benchmarks/data/ATTRIBUTION.md` (the file's citation/license text) was
  *also* in the package alongside it in every affected release, so the
  attribution obligation itself was met; this fix removes bulk data that
  didn't need to ship, it does not correct a prior missing-attribution
  problem.

### Added

- **Breaking**: `ConditionConflict` (`src/process.rs`, re-exported at
  the crate root) is a new public type: `{ step_index: usize, field:
  &'static str, reason: String }`, one entry per field that hit a
  conflict during condition resolution. `apply_condition_precedents`
  (crate-internal) now returns `(Vec<PlanningEvidence>,
  Vec<ConditionConflict>)` instead of `Vec<PlanningEvidence>` alone.
  `score_plan` (public API) gained a new required parameter,
  `condition_conflicts: &[ConditionConflict]`, threaded through to
  `collect_unresolved` so it can supply the conflict-specific reason --
  a genuine breaking signature change (9 parameters instead of 8),
  confirmed by `cargo semver-checks --baseline-rev v0.3.0` (fails
  `function_parameter_count_changed`, correctly requiring a new minor
  version before this can ship).
- **Phase 19P -- finite-temperature thermodynamics for gas-free,
  closed solid-phase systems.** New `SolidThermodynamicEntry` (a
  caller-supplied 0 K formation enthalpy + crystal-structure volume,
  plus a `ThermodynamicDatasetIdentity` naming its dataset/release/
  correction-scheme), `Kelvin` (validated to `[300, 1800]` K, the range
  Bartel et al. 2018 actually validated their SISSO Gibbs-energy
  descriptor against), and pure functions
  (`relative_solid_gibbs_ev_per_atom`, `balanced_reaction_delta_ev_per_atom`,
  `decomposition_margin_ev_per_atom`) estimating finite-temperature
  Gibbs-energy quantities from that data. Deliberately does not
  bundle any elemental-reference or gas-phase thermochemical data:
  every quantity computed is a same-total-composition comparison
  (a balanced reaction, or a decomposition margin against a
  caller-named alternative assemblage), and the elemental-reference
  term pymatgen's own equivalent implementation subtracts cancels
  exactly for any such comparison (verified both numerically, via
  synthetic pymatgen structures, and geometrically). New
  `ThermodynamicSelectivityAssessment`/`DecompositionComparison` types
  hold these raw results -- **not connected to `score_plan`, ranking,
  or `Score01` in any way**; `thermodynamic_support` stays `None`
  exactly as before, checked by a dedicated permanent regression test
  (`tests/thermodynamics_ranking_invariance.rs`). `CompetingPhase`
  (`reaction.rs`) is deliberately untouched -- this is new, separate
  API, not an extension of it.
- **Breaking**: three new `GugenError` variants (`GugenError` has no
  `#[non_exhaustive]`, so each is a genuine breaking addition, confirmed
  by `cargo semver-checks --baseline-rev v0.3.0`, `enum_variant_added`):
  `NonPositiveMagnitude { field, value }` (Phase 19P; returned by
  `SolidThermodynamicEntry::new` when `volume_angstrom3_per_atom` is not
  strictly positive, and now also by `decomposition_margin_ev_per_atom`
  for a non-positive `amount`), `UnbalancedReaction { element,
  imbalance }` and `InconsistentThermodynamicDataset(String)` (both
  Phase 19P.1, described above).
- **Breaking**: `balanced_reaction_delta_ev_per_atom` and
  `decomposition_margin_ev_per_atom` (Phase 19P.1) now return
  `Result<Option<f64>>` instead of `Option<f64>` -- `Ok(Some(value))` is
  a successful computation, `Ok(None)` is a legitimate abstention (a
  required entry simply wasn't supplied, or, for
  `decomposition_margin_ev_per_atom` only, the alternative assemblage
  doesn't conserve `target`'s composition), and `Err(...)` is invalid
  caller input (mixed dataset identity, a non-finite/non-positive
  `amount`, or a non-conserved reaction). `cargo semver-checks
  --baseline-rev v0.3.0` does not currently have a lint for a bare
  return-type signature change like this one (its return-type checks
  only cover narrower cases, e.g. a function that now returns `()`), so
  this change is disclosed here rather than machine-verified; every
  other change in this release is confirmed by the tool as noted.
- **Phase 20B -- bulk literature-corpus snapshot and observation
  provider**, new `literature_corpus` feature (depends on `serde`).
  `LiteratureObservationCorpus::load` parses a gugen-native JSON snapshot
  (built offline by the new `benchmarks/build_literature_observation_snapshot.py`,
  not part of this crate) into `CorpusHeatingObservation` values --
  target, precursor set, temperature/duration/atmosphere, DOI, and a
  corpus-position index -- queryable via `find_exact(route_family,
  target, precursors)` (exact composition equality, order-invariant
  precursor-set match, explicit empty result for any route family other
  than `ConventionalSolidState`, since the source corpus has zero
  evidence for any other route). **Not connected to `score_plan`,
  ranking, or `Planner` in any way** -- structurally impossible, not
  just unwired: `CorpusHeatingObservation::heating_purpose` is always
  `None` (no field for it exists in the wire schema at all), while
  `ConditionPrecedent::purpose` is a required, non-optional
  `HeatingPurpose`, so there is no lossless conversion from one to the
  other. Checked by a dedicated permanent regression test
  (`tests/literature_observation_planner_invariance.rs`), mirroring
  Phase 19P's own `thermodynamics_ranking_invariance.rs`. Deduplicates
  exact-duplicate corpus entries (same content excluding provenance,
  order-independent, lowest `corpus_record_index` survives) while never
  collapsing independently-reported entries that merely share the same
  conditions but differ in DOI. See `docs/literature_observation_provider.md`
  for the full schema, matching rules, corpus-wide coverage numbers, and
  a real local performance measurement (13,982 raw observations from
  Phase 20A's 9,045 structurally-valid records; 537 ms load, ~328
  µs/query linear-scan lookup, ~9.2 MB estimated memory, 4.7 MB
  reserialized). `Composition` gained `Eq`/`PartialOrd`/`Ord` derives (a
  purely additive, non-breaking change -- confirmed by `cargo
  semver-checks`; safe because `Composition` stores exact `Frac`
  amounts, not `f64`) so it can be used directly as a `BTreeSet` key for
  order-invariant precursor-set identity.
- **Phase 20D -- manual extraction-accuracy audit of
  `LiteratureObservationCorpus` against original source papers**, offline
  tooling only (`benchmarks/sample_literature_observation_audit.py`,
  `benchmarks/audit_literature_observations.py`), not part of the crate
  build. Answers the question Phase 20B's own text-mining pipeline
  couldn't answer about itself: when it reports a confident value, how
  often is it right, and when it disagrees with itself, is that a real
  experimental difference or a misextraction? DOI-level (not
  observation-level) sampling, seeded and reproducible (58 DOIs across a
  10-DOI pilot + 48-DOI main wave, stratified `temp_disagree`/
  `atm_controlled`/`fully_resolved` plus an unconditional `baseline`
  control group), each checked by an independent research review against
  real sources only (no paywall bypass, no mirror sites), plus a 5-item
  blind inter-reviewer overlap. Findings, all with Wilson 95% CIs and
  wide intervals given the small n: identity accuracy 70.7%
  (conservative) / 93.2% (excl. unverifiable, n=58); field accuracy where
  checkable was temperature 62.5% (n=8, one contested case), duration
  83.3% (n=6), atmosphere 100% (n=8); the dominant confirmed
  `temp_disagree` mechanism is a specific, nameable step-segmentation
  failure (a paper's genuine sequential/parallel multi-stage treatment
  merged into one `HeatingOperation` by the upstream pipeline), not
  random noise; 3 confirmed identity-level corpus errors (wrong DOI or
  reaction misgrouping), categorically worse than a value-level
  disagreement. No conflict resolver, promotion policy, or Planner
  connection added -- see `docs/literature_observation_accuracy_audit.md`
  for the full 15-item report and reproduction commands.
- **Phase 20C -- cross-DOI field comparison for
  `LiteratureObservationCorpus`**, the corpus-scale analogue of Phase
  19's `apply_condition_precedents`, new
  `LiteratureObservationCorpus::cross_doi_comparisons()`
  (`src/literature_observation_conflicts.rs`). Detection and
  classification only -- no averaging, no picking a winner, no
  `ConditionPrecedent`/`Planner`/ranking connection, structurally so
  (this module never imports from `planner.rs`), checked by a permanent
  regression test. Two observations are compared only when they agree on
  exact target, exact precursor set, `ConventionalSolidState`,
  independent DOI, total reported heating-operation count for their
  source record, *and* operation position within it -- called
  *positional alignment*, deliberately never "the same processing step,"
  since Phase 20D found step-segmentation (a paper's own multi-stage
  treatment merged into one `HeatingOperation` upstream) is the dominant
  confirmed disagreement mechanism, and comparing differently-shaped
  heating sequences by raw position would repeat that failure one level
  up. Per field (temperature/duration/atmosphere), reports `Agreement`,
  `Conflict`, `InsufficientIndependentSources` (exactly one independent
  DOI reported it), or `Unresolved` (none did) -- a lone value never
  resolves a field here, unlike Phase 19's own precedent-filling logic,
  since this module answers "do independent replications agree," not
  "do we have any data." A fifth status, `SegmentationAmbiguous`, exists
  in the type but is never populated in v1 -- reserved for a future
  per-observation segmentation-failure signal (a `HeatingOperation`
  merging multiple stages) that nothing in the corpus schema currently
  carries; a bare cross-DOI step-count difference is route-level shape
  diversity (`has_multiple_operation_shapes` below) and deliberately does
  not trigger it. Route-level `has_multiple_operation_shapes`
  (independent DOIs disagreeing on the route's own step count) is a
  separate, independent warning that never overrides a within-shape
  `Agreement`/`Conflict` -- real agreement within one operation shape
  stays real even when the route's step structure is itself contested.
  `Atmosphere::Controlled { description }` free-text values never
  contribute to a comparison, even against another `Controlled` value,
  since raw-string equality between independently-written descriptions
  isn't a safe signal. Real corpus-wide numbers (13,969-observation
  local snapshot): 619 routes have 2+ independent DOIs comparable
  somewhere, 332 of those (53.6%) flagged `has_multiple_operation_shapes`;
  among the 1,097 temperature step-groups with enough sources to reach a
  verdict, 859 (~78%) disagree; atmosphere reaches any real verdict for
  only 412 of 1,400 step groups (29.4%), since most atmosphere data is
  free text excluded by the `Controlled` rule. Even a unanimous
  `Agreement` stays a reference-only signal in v0.4.0 -- promotion is
  deferred to a separately-triggered future "Integration" phase. See
  `docs/literature_observation_provider.md`'s "Cross-DOI comparison"
  section for the full design and numbers.
- **v0.4.0 Integration -- reference-only literature evidence in
  `Planner`.** New `SynthesisPlan.literature_evidence:
  Option<LiteratureRouteEvidence>` and `Planner::with_literature_evidence_provider`,
  populated when a `LiteratureEvidenceProvider` finds a match for a
  plan's exact `(target, precursors, route_family)`. Scoped explicitly:
  given Phase 20D's ~0% auto-applicable finding and Phase 20C's 53.6%
  step-count-diversity / ~78% temperature-conflict rates, this is
  evidence and warning surfacing only -- no `ProcessStep` auto-fill, no
  `uncertainty_penalty`/`confidence`/`score`/ranking change of any kind,
  no `ConditionPrecedent` conversion, no `HeatingPurpose` inference, no
  application to `Mechanochemical`. Structural, not a promise:
  `LiteratureRouteEvidence` is never placed into any of `score_plan`'s
  three score-affecting inputs (`evidence`, `condition_conflicts`,
  `process_evidence_provider_consulted`) -- it is looked up and attached
  to `SynthesisPlan` only *after* `score_plan` has already returned, as
  its own field that function never receives. Verified over 324 real
  plans from a 200-route real-corpus sample: 0 score/confidence/ranking
  inversions, 0 `ProcessStep` changes, checked as a hard assertion in
  `examples/literature_evidence_integration_report.rs`, not just
  observed. `LiteratureEvidenceProvider` and its evidence types
  (`literature_evidence.rs`, moved out of the `literature_corpus`-gated
  module they originated in during Phase 20C) are always compiled --
  `Planner`'s public API and `SynthesisPlan`'s JSON schema never change
  shape depending on which crate features are enabled; only the real
  corpus-backed implementation (`LiteratureObservationCorpusProvider`)
  is feature-gated, same split `ThermodynamicProvider` already has
  against `MaterialsProjectSnapshotProvider`. An `Info`-severity
  `PlanningWarning` disclosure surfaces on *every* plan that gets
  `literature_evidence` attached, not only conflict/shape-diversity
  cases -- a clean unanimous `Agreement` is exactly the result most
  likely to be misread as endorsement if it were the one case left
  silent. See `docs/literature_evidence_integration.md` for the full
  design, real-corpus coverage numbers (161/324 sampled plans matched,
  49.7%; 65.8% of those flagged `has_multiple_operation_shapes`), and
  runtime/report-size measurements (~38.5 ms one-time index build,
  ~2.5 µs/query lookup, +3,327 bytes per evidence-carrying plan --
  measured with each sampled target planned against its own
  route-scoped precursor catalog, not a corpus-wide shared one).
  "More coverage" is never phrased here as "more accurate conditions"
  -- see that document's own explicit statement of what this does not
  establish.

### Known limitations

- Conflict detection is exact-value equality only. Two overlapping but
  non-identical ranges (e.g. a point value inside a wider reported
  range) are treated as a conflict, not silently reconciled -- the
  conservative reading, since `TemperatureRange`/`DurationRange`/
  `RampRateRange` have no overlap/subsumption semantics defined and
  designing one was explicitly out of scope for this phase.
- **Phase 19P.1**: `check_element_conservation`'s tolerance
  (`COMPOSITION_CONSERVATION_TOLERANCE`, `1e-6`) is an absolute bound on
  the coefficient-weighted per-element residual, not scaled by
  coefficient magnitude. It was calibrated against
  `decomposition_margin_ev_per_atom`'s *unweighted* composition sums; a
  `balanced_reaction_delta_ev_per_atom` reaction with large integer
  coefficients and fractional (non-integer) element amounts could in
  principle accumulate a genuinely-balanced residual past this bound and
  be rejected as `UnbalancedReaction`. Not reachable today -- nothing in
  this crate currently calls this function with such a reaction -- but
  worth revisiting if a future phase feeds `balance.rs` output (integer
  coefficients paired with literature-derived fractional amounts)
  through it directly.
- **Phase 19P**: gas-releasing/consuming reactions (e.g. any
  carbonate-decomposition route) are entirely out of scope -- they
  abstain automatically (a caller never has a `SolidThermodynamicEntry`
  for a gas species, since that type requires a crystal-structure
  volume), not via any gas-classification logic in this module. A
  future phase would need its own gas chemical-potential data source
  and model, deliberately not started here. No `Score01`/ranking
  connection of any kind -- unlocking `thermodynamic_support` requires
  an independently-calibrated eV/atom-to-`Score01` mapping this phase
  does not attempt. `decomposition_margin_ev_per_atom` compares against
  one caller-named alternative assemblage, never an
  automatically-searched "best" decomposition (no hull/combinatorial
  search) -- a deliberate scope narrowing from this phase's original
  sketch, since a search would require gugen itself to decide which
  candidates to enumerate, and a margin computed that way could be
  misread as an absolute stability claim rather than "no cheaper
  decomposition among what this caller supplied." Polymorph phase
  transitions are not predicted -- where more than one
  `SolidThermodynamicEntry` shares a composition, the lowest-0K-energy
  one is always used (order-independent, matching
  `MaterialsProjectSnapshotProvider`'s existing convention), never the
  lowest finite-temperature one, which the SISSO descriptor's
  volume-only structural input cannot reliably predict (Bartel et al.
  2018's own stated limitation).
- **Phase 20B**: a `HeatingOperation`'s temperature/duration is left
  unresolved (`None`) whenever the raw corpus reports 2+ disagreeing
  candidate readings for it, rather than guessing which is authoritative
  -- verified live against the full raw 19,488-record corpus, 2,311 of
  2,377 `HeatingOperation`s with 2+ candidate temperature readings
  disagree (97.2%); of the 13,982 observations that actually made it
  into the emitted snapshot, 1,063 have `temperature: None` for this
  reason specifically (see
  `docs/literature_observation_provider.md`), a real, material
  information loss disclosed as such. (Corrected 2026-08-15: an
  earlier draft of this entry misstated the snapshot-level figure as
  2,311, conflating it with the full-raw-corpus figure above.) Only 6 raw
  atmosphere strings map to a structured `Atmosphere` variant; everything
  else (including any string reported alongside another one for the same
  operation) is preserved verbatim as `Atmosphere::Controlled
  { description }` rather than guessed. `find_exact` is an unindexed
  linear scan over the whole corpus -- fine for single-target lookups at
  this corpus's ~14K-observation scale, not benchmarked for a bulk-query
  workload. Cross-DOI comparison (Phase 20C, added above) is detection
  and classification only, not resolution -- a `Conflict` is surfaced,
  never reconciled. `Planner` now surfaces this evidence for display
  only (v0.4.0 Integration, above) -- no promotion path to
  `ConditionPrecedent` exists, and nothing here is ever an auto-fill or
  scoring input, by design. Phase 20D's audit itself is
  small-n (58 DOIs,
  only 5 reached full text) and measures population-level base rates
  only -- it certifies no individual observation, and 100% of the corpus
  remains reference-only after this phase; see
  `docs/literature_observation_accuracy_audit.md` items 9-12 for the
  full limitations list.

## [0.3.0] - 2026-08-14

### Added

- **Route-family suitability evidence.** New `route_suitability` module:
  `SuitabilityFinding`/`SuitabilityVerdict` (`Supports`/`Contradicts`/
  `Unknown`) record literature evidence for a specific `(target,
  RouteFamily)` pair, never force-merged into a single aggregated
  verdict. New `RouteSuitabilityProvider` trait and
  `InMemoryRouteSuitabilityProvider`, backed by hand-verified,
  DOI-cited records. `SynthesisPlanningReport` gains a
  `route_suitability` field (**breaking change**: `SynthesisPlanningReport`
  has no `#[non_exhaustive]` and all-public fields, so this breaks any
  downstream struct-literal construction or exhaustive destructuring of
  it -- see Changed below for the second new field), listing the evidence
  considered for each route family. `Planner` gains
  `with_route_suitability_provider`.
- **Explainable route recommendations.** New `RouteRecommendation` enum
  (`Recommended`/`NotRecommended`/`InsufficientEvidence`/
  `ConflictingEvidence`) and `derive_recommendation`, a pure function
  turning suitability evidence into a recommendation. Conservative by
  design: a route is excluded only when contradicting evidence is both
  non-weak and specific to the exact target; supporting and
  contradicting evidence coexisting on the same route is reported as
  `ConflictingEvidence` rather than silently resolved either way.
- **Optional chematic-crystal structure bridge.** New `chematic_crystal`
  Cargo feature and `to_mikiwame_structure`, converting a
  `chematic_crystal::PeriodicStructure` into a `mikiwame::OwnedStructure`
  for structural-diagnostic checks (same-site occupancy consolidation,
  left-handed-lattice correction). Off by default, not wired into
  `Planner::plan`.
- **Route-suitability corpus audit.**
  `docs/route_suitability_corpus_audit.md` documents how much literature
  evidence for route suitability actually exists in a real 1500-record
  synthesis corpus already used by this crate's benchmarks, and
  evaluates the new decision policy against a hand-verified holdout
  record. Not a route-family prediction-accuracy benchmark -- see
  Known limitations below and the report itself for why.

### Changed

- `SynthesisPlanningReport` gains a `not_recommended: Vec<NotRecommendedPlan>`
  field (**breaking change**, same reason as `route_suitability` above:
  `SynthesisPlanningReport` is not `#[non_exhaustive]`, so any downstream
  code that constructs or exhaustively matches it needs updating). A
  plan whose route family has strong, uncontested contradicting
  evidence is moved out of `plans` into this field instead (original
  score/confidence preserved, alongside the specific findings that
  triggered exclusion) rather than silently dropped or left mixed in
  with recommendable plans. When every plan for a target is excluded
  this way, the report abstains explicitly.

### Known limitations

- `RouteSuitabilityProvider` is a lookup over hand-verified literature
  findings for specific `(target, route family)` pairs, not a
  generalizing model -- it does not predict suitability for materials it
  has no record of, and abstains (`InsufficientEvidence`) rather than
  guess.
- gugen's `Composition` type cannot distinguish polymorphs (e.g.
  hematite vs. maghemite Fe2O3); route-suitability evidence keyed only
  on composition can be silently wrong for a polymorph-ambiguous target.
  No general polymorph-disambiguation mechanism exists yet.
- The chematic-crystal/mikiwame structure bridge and route-suitability
  evidence are not connected: structural diagnostics (is this geometry
  physically valid) are never converted into a route recommendation
  without literature backing.
- The route-suitability corpus audit measured a 0/1500 polymorph-
  ambiguity floor in its sampled corpus. This reflects that corpus (its
  7 listed systems appear there only as precursor formulas, never as
  synthesis targets) -- not a claim that polymorph ambiguity is rare in
  synthesis literature generally.

## [0.2.0] - 2026-08-14

### Added

- **Phase 10 — literature condition provider.** `ProcessPrecedent` gains a
  `conditions: Vec<ConditionPrecedent>` field (breaking change, toward
  v0.2.0) carrying structured, per-purpose temperature/duration/
  atmosphere/ramp data with its own citation, evidence kind, and strength
  — no longer just a free-text `description`. New
  `InMemoryLiteratureConditionProvider` (`src/literature_conditions.rs`),
  backed by 5 hand-verified curated records (LaAlO3, MgAl2O4, Zn3(PO4)2,
  CaO, BaTiO3 — the same targets `tests/validation.rs`'s Phase 8 fixtures
  use), each traced to a real DOI actually read this phase, not recalled
  from memory (AGENTS.md §21.3). Two of the five representative DOIs
  `tests/validation.rs` already cited turned out, on inspection, to be
  confirmed topic mismatches (Zn3(PO4)2's is a zinc-phosphate glass paper
  made by melt-quenching; BaTiO3's is a NaNbO3-BaTiO3 solid-solution
  study) and a third (LaAlO3's) was fully paywalled with no accessible
  copy — a different, freely-accessible, independently verified paper was
  substituted for condition data specifically in each case, left
  documented rather than silently reusing an unreachable or wrong source.
  Zn3(PO4)2's substitute source used a different precursor combination
  (ZnO + (NH4)2HPO4, not ZnO + P2O5) than the existing fixture — recorded
  as the real route the source used; `InMemoryLiteratureConditionProvider`
  correctly scopes that data `EvidenceScope::SimilarMaterial` rather than
  `ExactTarget` when applied to a different, same-target precursor route
  (proven by a dedicated test, not just asserted). New
  `Planner::with_process_evidence_provider` constructor. New
  `apply_condition_precedents` (`src/process.rs`) splices resolved
  conditions into a plan's `Heat` steps before scoring — only ever fills
  an already-`None` field, never overwrites one some other source set.
  Fixed a false-claim bug this capability would otherwise introduce:
  `score.rs`'s `UnresolvedRequirement.reason` text was hardcoded to "no
  ... provider is wired in yet" for every unresolved field regardless of
  whether a provider actually existed — now distinguishes "no provider
  configured" from "a provider was consulted and had nothing for this
  field," so the reason text stays true once a provider that resolves
  *some* but not all conditions is wired in. `Planner::offline_minimal`
  and every fixture/test built on it (including the README's byte-for-
  byte worked example and both golden snapshots) are explicitly
  unaffected — new capability is opt-in via the new constructor, never
  retroactive.

### Known limitations

- `PlanScoreBreakdown.evidence_strength` (plan-level aggregate) stays
  pinned at `0.25` even for a plan with a resolved, `Moderate`- or
  `Strong`-strength condition, because it aggregates by weakest link and
  `conventional_solid_state_template` always attaches its own `Weak`
  template-default entry alongside any condition evidence. Not "fixed" by
  changing the aggregation rule — that would be an unsourced heuristic
  with no calibration data behind it (AGENTS.md §27), same reasoning as
  every other constant-aggregate finding this project has logged rather
  than silently patched.
- The curated condition set covers 5 targets with real, verified data;
  everything else still plans exactly as it did before Phase 10
  (conditions unresolved). This is a deliberately small, hand-checked set
  (AGENTS.md §21.3), not a literature-mining pipeline — a much larger,
  reproducibly-sourced corpus is Phase 11's job, at a different trust
  tier.

### Added

- **Phase 11 — large-scale blind benchmark.** New `benchmarks/` directory
  (AGENTS.md §23, comparison/corpus tooling isolated from the crate's own
  dependency tree): `benchmarks/fetch_kononova.py` reproducibly re-fetches
  the same licensed Kononova et al. 2019 corpus (CC BY 4.0, re-verified
  live against the figshare API on every run) and filters it down to a
  1500-reaction holdout sample (`benchmarks/data/kononova_sample.jsonl`,
  `benchmarks/data/ATTRIBUTION.md`) with every `(target, precursor-set)`
  route already used by `tests/validation.rs`'s 5 fixtures or Phase 10's
  curated records excluded — matched by normalized elemental ratio, not
  DOI, since several of those routes (e.g. BaTiO3's) are independently
  reported by dozens of DOIs in this same corpus, so DOI-only exclusion
  would have left near-duplicate leaked entries in the holdout set. New
  `tests/large_scale_benchmark.rs` (breadth + anti-leakage assertions,
  cheap, part of `cargo test`) and `examples/large_scale_benchmark.rs` →
  `docs/large_scale_benchmark_report.md` (full §22 metric computation,
  regenerated and diffed byte-identical before commit, same convention as
  `examples/benchmark_report.rs`).
- A fixed, corpus-derived decoy pool (the most frequent precursor
  formulas in the sample itself, filtered to share ≥1 element with each
  row's target, capped at 8 per row — the cap chosen by directly
  measuring `RejectionCode::SearchBudgetExhausted` against this sample,
  not assumed) is added to every row's catalog so precursor-set recovery
  is a real discrimination task rather than a single-candidate
  pass-through.

### Fixed

- **`benchmarks/fetch_kononova.py` used a wrong-provenance copy of the
  Kononova corpus while under development, caught before this phase's
  first commit by actually running the script's live (non-`--local`)
  path rather than trusting a cached dev copy.** The dataset genuinely
  hosted at the officially-cited figshare DOI (9722159, the one Phase 8
  license-checked as CC BY 4.0) is a bare JSON list of 19,488 reactions;
  the script's first draft assumed a `{"reactions": [...]}`-wrapped shape
  with 30,031 entries, matching a cached local file that turned out to
  have been downloaded, in an earlier session, from
  `CederGroupHub/text-mined-synthesis_public`'s GitHub repo — a
  *different*, later (2019-12-03 vs. the figshare deposit's 2019-06-27),
  differently-shaped snapshot with `license: null` per the GitHub API,
  never itself license-verified. Fixed by indexing the real top-level
  list directly (a shape mismatch now fails loudly, not silently) and
  correcting `EXPECTED_REACTION_COUNT` to 19,488. Full finding, including
  its effect on already-merged Phase 8/Phase 10 numeric claims, is a
  dedicated §28-format report in `tasks/todo.md`'s Phase 11 section — not
  something this phase silently patched and moved past.

### Known limitations

- **`RejectionCode::MissingTargetElement` dominates rejections at this
  scale (95.4% of all rejected candidates), not
  `UnsupportedByproductRequired` as originally expected going into this
  phase** — a mechanical consequence of `search_precursor_sets` checking
  element coverage before any byproduct/balance check, combined with the
  decoy pool generating many small subsets that don't happen to cover one
  specific target's exact elements. The originally-expected finding does
  hold once this combinatorial noise is factored out: among the
  combinations that *do* pass the element-coverage gate,
  `UnsupportedByproductRequired` accounts for 90.2% of what's left. Not
  widened reactively — see `docs/large_scale_benchmark_report.md`.
- Condition-resolution coverage from Phase 10's curated provider does not
  generalize beyond its 5 curated targets, confirmed at this scale (0/1651
  resolved among holdout rows whose target isn't one of those 5) — the 6
  rows that do share a Phase-10-covered target (via a different precursor
  route) resolve `EvidenceScope::SimilarMaterial`, not evidence of
  unseen-target prediction.
- This 1500-reaction sample is one fixed, seeded downsample of a larger
  eligible pool (8,936 rows after filtering); not re-sampled or tuned
  against gugen's own results on it (AGENTS.md §27).
- `tests/validation.rs`'s citation text states DOI-attestation counts (19/
  20/2/88 independent papers for LaAlO3/MgAl2O4/Zn3(PO4)2/BaTiO3) and a
  "30,031-reaction dataset" size that do not match the correctly-licensed
  corpus (recounted directly: 10/16/0/83, 19,488 reactions) — a
  consequence of the same wrong-provenance file above. Every
  representative DOI Phase 8 actually cites is still present and still
  reports the same route in the correct corpus (so the fixtures are not
  fabricated), except Zn3(PO4)2's: the correct corpus's own text-mining
  extraction for that DOI reports a complex doped-glass formula, not
  plain `Zn3(PO4)2` — independently corroborating Phase 10's separate
  finding that this DOI's paper is a Sm-doped phosphate glass study, not
  a `Zn3(PO4)2` synthesis. **Not corrected in this phase** — deliberately
  left as a flagged, unresolved item for whoever owns Phase 8's fixtures,
  since rewriting the counts (and deciding what the Zn3(PO4)2 fixture
  should even claim, given its route's zero attestation in the licensed
  data) is a fixture-design decision, not this phase's number to change
  unilaterally. Full detail in `tasks/todo.md`'s Phase 11 §28 report.

### Added

- **Phase 12 — Mechanochemical route family.** New `RouteFamily::Mechanochemical`
  and `mechanochemical_template` (`src/process.rs`), structurally grounded
  in two independently verified literature reviews (Suryanarayana 2001,
  DOI `10.1016/S0079-6425(99)00010-9`; Qiang, Hu, Jiang 2025, DOI
  `10.3390/polym17172340`) — not invented from memory (AGENTS.md §21.3).
  Weigh, then a single high-energy ball-milling step (`GrindingMethod::
  BallMilling`) performing mixing and grinding together (unlike the
  separate `Mix`/`Grind` steps of the conventional template), optionally
  followed by pressing, with a post-milling anneal only when the balanced
  reaction releases a byproduct — the cited review reports specific
  byproduct-releasing compounds (e.g. gamma-Al2O3, ZrO2) that formed only
  after heating the as-milled powder. New `applicable_route_family_templates`
  (`src/process.rs`) is the one integration point `Planner::plan` uses: one
  accepted precursor set now yields one plan per applicable route family
  (currently 2), not just one.
- `RouteFamily` gains `PartialOrd`/`Ord` (needed to key on it in a
  `BTreeMap`, e.g. for deduplicating plans by `(precursor set, route
  family)`).

### Fixed

- **Two real bugs a naive multi-route-family implementation would have
  introduced, both caught by design review before writing code, not found
  by a failing test afterward:**
  - `process_simplicity`'s step-count clamp
    (`MIN_TEMPLATE_STEPS`/`MAX_TEMPLATE_STEPS`) was a single global range
    derived from `conventional_solid_state_template`'s own achievable step
    counts (7-9). Applied unconditionally to `Mechanochemical`'s genuinely
    shorter templates (4-6 steps), every mechanochemical plan would have
    clamped to the global minimum and scored a perfect `process_simplicity
    = 1.0` on zero real evidence of relative merit, beating every
    conventional-route plan by construction. Fixed: `score_plan` gains a
    `route_family: RouteFamily` parameter and a new `step_bounds(route_family)
    -> (usize, usize)` lookup, each range derived the same way the original
    one was — from that family's own template's actual achievable step
    counts.
  - `derive_plan_id` (`src/planner.rs`) didn't hash `route_family`. Two
    plans built from the same accepted precursor set under different route
    families would have collided on `plan_id`. Fixed explicitly (route
    family is now part of the hash input) rather than discovered via the
    existing plan-id-uniqueness test failing.
  - A third issue was reasoned through but is not a "fix" — `score_plan`'s
    `applicability: target_applicability.clone()` was already documented as
    a stated assumption ("v0.1 has exactly one route family"), which Phase
    12 falsifies regardless of implementation. Rather than fabricate a
    numeric applicability penalty per route family (no route-suitability
    precedent exists to justify one — AGENTS.md §27), both route families
    are offered as separate ranked plans and the `PlanningAssumption` text
    is now route-family-specific ("no route-suitability precedent exists
    for this target under `{route_family}` specifically").
- `tests/fixtures/batio3_report.json`/`.md` (golden snapshots),
  `docs/benchmark_report.md`, `docs/large_scale_benchmark_report.md`, and
  the `README.md`/`README_ja.md` worked `gugen plan` example all
  regenerated from real runs: every one now legitimately shows 2 plans
  per accepted precursor set (one per route family) instead of 1. Several
  existing tests that hardcoded "exactly 1 plan" for a given precursor set
  (`plan_id_is_stable_when_an_unrelated_candidate_is_added_to_the_catalog`,
  `a_precursor_identical_to_the_target_plans_as_a_trivial_identity`,
  `formula_unit_scale_is_not_currently_normalized_a_documented_gap`, two
  `provider_failures.rs` cases) updated to the new, correct expectation —
  each one a real assumption Phase 12 broke, not busywork.

### Known limitations

- No route-suitability classifier exists: every applicable route family is
  offered unconditionally for every accepted precursor set, regardless of
  whether that route family is actually a sensible real-world choice for
  the specific target (e.g. mechanochemical synthesis of an air-sensitive
  or thermally unstable compound). Not fixed — no calibration data exists
  to justify a per-target route-family suitability score (AGENTS.md §27).
- `step_bounds`'s two ranges are each still derived the same way the
  original single global range was — from what each template's generator
  can currently produce, not an independent claim about real-world process
  complexity. A third route family with genuinely different achievable
  step counts needs its own range the same way, not a shared guess.

### Added

- **Phase 13 — `ThermodynamicProvider` adapter boundary.** New
  `CompetingPhase` (`src/reaction.rs`): a formation energy for a phase that
  might compete with a target for the same elements, additive to
  `ThermodynamicProvider` rather than to `ReactionEnergy` — that type's own
  doc comment forbids growing unrelated fields onto *it* specifically, not
  onto a sibling type for a genuinely different quantity. New
  `ThermodynamicProvider::competing_phases` default method (returns
  `Ok(Vec::new())`) — a **non-breaking** addition, unlike Phase 12's
  `score_plan` signature change: every existing `ThermodynamicProvider`
  implementor keeps compiling unchanged. `Planner::plan` calls it alongside
  the existing `reaction_energy` call; a non-empty result becomes one more
  `EvidenceKind::ThermodynamicData` entry with an explicit "does not
  account for kinetics, particle size, or atmosphere" limitation — like
  `reaction_energy`, never converted into a selectivity/favorability score
  (AGENTS.md §4.3).
- New `materials_project` Cargo feature (`[]` — zero new dependencies) and
  `src/materials_project_adapter.rs`: `MaterialsProjectSnapshotProvider`
  implements `ThermodynamicProvider` entirely over a caller-supplied
  `Vec<CompetingPhase>` snapshot — gugen performs no network call, holds no
  API key, and has no live-fetch code path anywhere in this module. `
  reaction_energy` computes ΔE per atom from the snapshot's weighted
  formula-unit energies (documented normalization convention, hand-checked
  in a unit test) and returns `Ok(None)` — never a partial sum — the
  moment any participating species' exact `Composition` isn't in the
  snapshot. `competing_phases` returns every snapshot entry sharing at
  least one element with the target, excluding the target's own
  composition. No formula parser exists in gugen (`Composition` has no
  `Display`/`FromStr`) — the adapter's input is explicit element/amount
  pairs, matching `Composition::new`'s own shape, not a `formula_pretty:
  String` field; converting a Materials Project formula into that shape is
  documented as the caller's job (`docs/integration.md`), grounded against
  Materials Project's real, verified field names (`formula_pretty`,
  `formation_energy_per_atom`, both eV/atom — confirmed via `mp-api`'s
  `SummaryRester` reference and `materialsproject/mapidoc`, not recalled
  from memory, AGENTS.md §21.3).
- `docs/architecture.md`'s Provider isolation section refined: an adapter
  that only *consumes* pre-fetched, caller-supplied external data (no
  network call performed by gugen itself) may live in-crate behind its own
  feature gate, same as `mikiwame_adapter.rs`'s existing shape — only a
  client that would *fetch* live data itself stays out of this crate
  entirely.

### Known limitations

- Once both Phase 12 and Phase 13 are configured together, `reaction_energy`/
  `competing_phases` are called once per route family sharing the same
  accepted precursor set, duplicating identical thermodynamic evidence
  across those plans (the underlying `BalancedReaction` is the same
  regardless of route family) — a known, accepted inefficiency, not a
  correctness issue (same category as Phase 12's own equivalent note about
  provider calls being duplicated per route family).
- `MaterialsProjectSnapshotProvider` matches species by exact `Composition`
  equality, the same convention `InMemoryLiteratureConditionProvider`
  (Phase 10) already uses — no fuzzy or near-composition matching.

### Fixed

- **Phase 14 — validation fixture citation repair (release blocker).**
  `tests/validation.rs`'s citation text carried DOI-attestation counts and
  a dataset-size claim measured against a wrong-provenance corpus
  (discovered but deliberately left unfixed in Phase 11 — see that
  section's own entry above). Corrected by live-refetching the correctly-
  licensed figshare corpus (19,488 reactions) and recounting every route
  directly: LaAlO3 19→10, MgAl2O4 20→16, BaTiO3 88→83 independent DOIs;
  the module doc comment's "30,031-reaction dataset" corrected to 19,488.
  Two representative DOIs were confirmed topic mismatches on direct
  reading (found while sourcing Phase 10's condition data) and replaced,
  not left standing on a count fix alone:
  - **BaTiO3**: the original representative DOI
    (`10.1111/j.1551-2916.2006.01172.x`) is a NaNbO3-BaTiO3 solid-solution
    study, not plain BaTiO3. Replaced with `10.3390/cryst14040304` (Qi et
    al., *Crystals* 14(4), 304 (2024), open access) — read directly,
    confirms exactly this route ("TiO2 ... and BaCO3 ... powders were
    mixed in a molar ratio of 1:1 and calcined"). This paper post-dates
    the 2019 Kononova corpus, so it is cited as an independently verified
    example, not as one of the 83 corpus attestations — a stronger
    evidentiary tier than naming an unread corpus entry.
  - **Zn3(PO4)2**: recounting found this route (ZnO + P2O5) has **zero**
    independent attestations in the correct corpus — not a count
    correction, a genuinely wrong fixture (its original representative
    DOI, `10.1016/j.jmmm.2015.06.001`, is a Sm-doped zinc-phosphate glass
    paper, not this reaction at all). Replaced the fixture entirely
    (rather than force-fitting a different precursor route onto the same
    target) with a different, well-attested phosphate found by querying
    the correct corpus directly: **LiFePO4** (a lithium-ion battery
    cathode material), route `FePO4 + Li2CO3 -> LiFePO4`, 6 independent
    DOIs. Verified `balance()` actually recovers this route within
    gugen's existing curated byproduct allow-list before adopting it
    (`4 FePO4 + 2 Li2CO3 -> 4 LiFePO4 + 2 CO2 + O2` — no allow-list
    widening needed). Representative entry: `10.1021/cm7027993` (Zaghib,
    Mauger, Gendron, Julien, *Chemistry of Materials*, 2008) — title/
    authors/venue/year confirmed via CrossRef and Semantic Scholar; the
    paper is paywalled, so (like this suite's existing LaAlO3 citation)
    its specific conditions were not independently read, only the
    corpus's attribution of the route to this DOI.
  - `examples/benchmark_report.rs`'s own duplicated fixture list (a
    separate compilation target from `tests/validation.rs`, mirroring the
    same data) updated to match; `docs/benchmark_report.md` regenerated
    from a real run and confirmed byte-identical on re-run.
  - `src/literature_conditions.rs`'s doc comments (which independently
    found and recorded these same two topic mismatches while sourcing
    Phase 10's condition data) updated to state that Phase 14 later acted
    on that finding, rather than continuing to describe it as
    forward-looking.
  - `benchmarks/fetch_kononova.py`'s `EXCLUDED_ROUTES` and
    `tests/large_scale_benchmark.rs`'s mirror of it were **not** updated
    to add the new LiFePO4 route or regenerate
    `benchmarks/data/kononova_sample.jsonl` — doing so would reshuffle
    the entire deterministic 1500-row sample (a large, unrelated diff)
    for a small evidence/wording fix. Checked directly that this is
    currently harmless (the committed sample has zero rows matching the
    new route's exact precursor set); flagged in both files as a known
    gap to close the next time that corpus is actually regenerated, not
    silently left inconsistent.

  No algorithm, scoring, or planning behavior changed — evidence and
  citation text only.

## [0.1.0] - 2026-08-14

Initial release. Published to [crates.io](https://crates.io/crates/gugen)
from `main` (PR [#1](https://github.com/kent-tokyo/gugen/pull/1), merge
commit `5419fd1`) via the manual-dispatch
[`publish.yml`](.github/workflows/publish.yml) workflow.

### Added

- **Phase 9 — v0.1 release preparation.** Went through AGENTS.md §29's
  completion checklist item by item rather than assuming it was already
  satisfied. Added `LICENSE-APACHE`/`LICENSE-MIT` (present in the crate
  layout AGENTS.md specifies and linked by both READMEs since Phase 0, but
  never actually created) — text matches the exact convention already used
  by gugen's own dependency tree (`rust-lang/rust`'s unmodified
  Apache-2.0 terms; the copyright-line-free MIT text `thiserror`/`serde`/
  `syn` ship). Added `repository`/`documentation`/`keywords`/`categories`
  to `Cargo.toml`, plus `[package.metadata.docs.rs] all-features = true` —
  without it, docs.rs would build with `default = []` and silently omit
  every `serde` impl and the whole `mikiwame` adapter module from the
  published docs. A full dependency license audit (30 locked crates,
  checked against their exact locked versions via the crates.io API, not
  assumed from crate name recognition): all MIT/Apache-2.0-compatible, no
  copyleft; `unicode-ident` additionally carries `Unicode-3.0` for its
  embedded data tables, a standard, widely-audited combination. Semver
  audit: no `cargo-semver-checks` baseline exists yet since gugen has never
  been published (confirmed by running it, not assumed); the public API
  surface was reviewed by hand instead and is a deliberately curated
  re-export list with no accidentally-public internals. Added a full
  worked `gugen plan` example (BaTiO3, the same fixture the golden
  snapshot tests use) to both READMEs, captured from a real run of the
  built CLI rather than written from memory.
- **Phase 8 — validation.** Curated, cited literature fixtures
  (`tests/validation.rs`) spanning perovskite oxide (LaAlO3), spinel oxide
  (MgAl2O4), phosphate (Zn3(PO4)2), simple binary oxide (CaO), and a
  carbonate precursor route (BaTiO3) — four sourced from the Kononova et
  al. 2019 text-mined dataset (license verified CC BY 4.0 via the
  figshare API), one independently sourced after finding that dataset has
  zero simple-binary-oxide-target entries. Known-route recovery (5/5),
  metamorphic invariance tests (`tests/metamorphic.rs`: target/catalog
  order, unrelated-precursor addition, provider return order — all
  confirmed invariant end-to-end through `Planner`), a full AGENTS.md
  §21.5 provider-failure suite (`tests/provider_failures.rs`: timeout,
  missing entry, malformed record, partial coverage, duplicated evidence,
  no unit-consistency check, unavailable provider), adversarial examples
  (`tests/adversarial.rs`: arithmetic overflow, uncovered target, tight
  search budget, trivial precursor==target identity, multi-element
  contradiction), a false-confidence audit, reproducibility tests, golden
  JSON/markdown snapshots (`tests/fixtures/batio3_report.{json,md}`), and
  a real, measured benchmark report (`examples/benchmark_report.rs` →
  `docs/benchmark_report.md`) covering AGENTS.md §22's metric list except
  differential validation (§23, explicitly "if possible", skipped for
  lack of a reference implementation) and temperature-specific metrics
  (undefined in v0.1, since no provider ever populates a temperature).
- **Phase 7 — CLI and batch.** `gugen plan target.json --catalog
  precursors.json [--output report.json] [--format json|markdown]`,
  `gugen explain report.json --plan plan-001`, `gugen validate-target
  target.json`, `gugen doctor`, and `gugen batch input.json --catalog
  precursors.json [--output out.json]` (AGENTS.md §19), alongside the
  existing `gugen balance`. Target/catalog/batch-input files reuse the
  existing public `TargetSpecification`/`PrecursorCandidate` JSON shapes
  rather than inventing wrapper formats. `batch` plans each target
  independently against one shared `Planner`; one target's failure becomes
  a per-entry error, not an aborted run (AGENTS.md §26). The CLI binary is
  the one place in the crate allowed to read the system clock (for
  `execution_timestamp`), via a small public-domain days-from-civil
  conversion rather than a new date/time dependency. Markdown rendering
  shows `Composition` as explicit `element:amount` pairs rather than a
  concatenated pseudo-formula, since `Composition` iterates alphabetically
  and would otherwise print something that looks like a real formula but
  isn't one.
- **Phase 6 — integration.** `Planner` (`new`/`offline_minimal`/`plan`)
  orchestrates catalog → `search_precursor_sets` →
  `conventional_solid_state_template` → `score_plan` into a ranked
  `SynthesisPlanningReport`, truncated to `SearchBudget.max_plans_returned`
  *after* ranking (overflow becomes an explained `RejectedCandidate`, not a
  silent drop of arbitrary combinations). `execution_timestamp` is a
  required caller-supplied argument, not read from the system clock. A
  self-contradictory target (an element both required and forbidden)
  abstains early with empty plans; an empty catalog result is a warning,
  not an applicability downgrade. `ThermodynamicProvider`/
  `ProcessEvidenceProvider` failures degrade to a per-plan `Info` warning
  rather than failing the whole report (§21.5); a `PrecursorCatalog`
  failure still propagates. `plan_id` is content-derived (precursor set +
  reaction), stable under catalog reordering. Optional `mikiwame` adapter
  (`src/mikiwame_adapter.rs`, feature-gated, off by default): maps a real
  `mikiwame::MaterialDiagnosticReport` to `abstain_reason`/`warnings`/
  `confidence_penalty`, not auto-wired into `Planner::plan` since gugen's
  `TargetStructure` can't yet supply the lattice/site data
  `mikiwame::analyze` needs (blocked on unpublished `chematic-crystal`) —
  exposed as a standalone function for callers with their own structure
  data.
- **Phase 5 — ranking and explanation.** `Score01` (validated `[0, 1]`
  newtype), `PlanScoreBreakdown`/`RankingWeights` (verbatim, AGENTS.md §13,
  equal-weight default with documented rationale), `ConfidenceAssessment`
  (verbatim, §16, four independent dimensions). New `score_plan()`:
  missing `thermodynamic_support` is excluded from the weighted average
  rather than treated as failure (§13); `evidence_strength` aggregates by
  weakest link, not mean. New `manual_review_required: bool` on
  `SynthesisPlan` (§15), always `true` in v0.1 since no hazard data source
  exists, paired with a mandatory `Severe` warning that `safety_penalty=0`
  is not a safety clearance. New `PlanningAssumption`, populated with the
  one real assumption `score_plan` makes (per-plan applicability mirrors
  target-level applicability, since v0.1 has one route family). Per-plan
  `unresolved` now populated from every unresolved condition field.
  `SynthesisPlan` carries every field AGENTS.md §6 lists.
- **Phase 4 — solid-state process template.** `RouteFamily`
  (`ConventionalSolidState`), `StepRequirement`, `Atmosphere` (verbatim from
  AGENTS.md), and a minimal, deliberately non-exhaustive set of method
  enums (`MixingMethod`, `GrindingMethod`, `FormingMethod`,
  `HeatingPurpose`, `CoolingMode`, `CharacterizationMethod`). `ProcessStep`
  and the new `evidence.rs` (`EvidenceKind`, `PlanningEvidence`). New
  `conventional_solid_state_template()` generates a
  weigh/mix/grind/form/heat/cool/characterize sequence from an accepted
  precursor set, branching on the actual balanced reaction (a byproduct
  release adds calcination and regrind steps, per AGENTS.md §11's outline)
  rather than applying one fixed template to every material. Temperature,
  duration, ramp rate, and atmosphere are left unresolved rather than
  guessed. A mismatched `AcceptedPrecursorSet` (precursors/reactants of
  different lengths) produces an `Unresolved` `Weigh` step with a `Severe`
  warning rather than a silently truncated materials list. `SynthesisPlan`
  grew from the Phase 1 `{ plan_id }` stub to carry `route_family`,
  `precursors`, `balanced_reaction`, `steps`, `evidence`, and `warnings`.
- **Phase 3 — precursor-set search.** `InMemoryPrecursorCatalog`
  (`PrecursorCatalog` impl, sorted and deduplicated by `PrecursorId`).
  `search_precursor_sets`: deterministic, budget-bounded combination
  search with target-element-coverage, forbidden-element, and
  byproduct-removability filters, backed by `balance()` for stoichiometric
  feasibility. Budget exhaustion is reported as a distinct rejection
  reason, never conflated with "no candidates found."
- **Phase 2 — exact reaction balancing.** `balance()`: exact-rational
  Gauss-Jordan elimination over the element × species matrix, returning
  every independent chemically valid balance (verified against the
  classic Fe + O₂ → {FeO, Fe₂O₃, Fe₃O₄} multi-solution case). Curated
  byproduct allow-list (CO₂, H₂O, O₂). `gugen balance` CLI subcommand.
  `Composition` now stores amounts as exact rationals internally
  (continued-fraction rationalized once at construction), so balancing
  never falls back to floating-point approximation.
- **Phase 1 — foundation.** Typed errors, validated numeric range types,
  `Composition`/`Element`, `TargetSpecification` and the
  `TargetMaterialView` boundary trait (stands in for `chematic-crystal`
  until it's published), the public report schema skeleton
  (`SynthesisPlanningReport` and friends), provenance, the three provider
  traits (`PrecursorCatalog`, `ThermodynamicProvider`,
  `ProcessEvidenceProvider`). JSON round-trip test, CI workflow.
- **Phase 0 — architecture.** Scientific scope, competitor landscape, and
  integration-boundary docs under `docs/`. Verified via crates.io/GitHub
  API (not memory): `gugen` package name is free; `chematic-crystal` and
  `mikiwame` are not yet published; `renkin` exists and is excluded as a
  dependency.

### Fixed

- **README's `gugen balance` JSON output example never matched real
  output.** Both READMEs showed hand-formatted, condensed single-line
  objects (`{ "composition": { "Ba": 1.0, "O": 1.0 }, "coefficient": 1 }`);
  the CLI actually renders via `serde_json::to_string_pretty`, which
  expands every field onto its own line. Caught by actually running the
  documented command against the documented input file and diffing the
  result against the README, per AGENTS.md §26 Phase 9's "README実例を
  実出力と同期" — not by re-reading the existing text and assuming it was
  still accurate.
- `search_precursor_sets` could silently double-accept a precursor set: a
  redundant element source (e.g. a catalog with both BaCO3 and BaO) lets a
  larger combination balance with the redundant precursor's coefficient
  solved to zero, collapsing to the exact same precursors and reaction a
  smaller combination already produced. Both were being accepted, so
  `Planner` could rank and return the *same plan twice*. Found via Phase
  7's `gugen plan` CLI output — the first place anyone looked at a full
  multi-candidate report end to end — not by a targeted test. Now detected
  and rejected as `RejectionCode::DuplicatePlan`; a doc comment that
  wrongly called this code "unreachable by construction" is corrected.

### Known limitations

- `balance()` only checks individual null-space basis vectors for sign
  validity; a valid reaction requiring a combination of two or more basis
  vectors is not found. Not hit by any case in the AGENTS.md §21.1 test
  list or by realistic small-species-count solid-state reactions so far.
- `PRECURSOR_COUNT_EXCEEDED` is unreachable by construction in the current
  search (combinations never exceed the configured max size).
  `DUPLICATE_PLAN` is reachable (see Fixed, above) but only via the one
  known mechanism (a redundant precursor collapsing a larger combination
  onto a smaller one's result) — not a general duplicate detector across
  every possible cause.
- `PlanningConstraints` only has `forbidden_elements`; the rest of AGENTS.md
  §9's filter list (redox/atmosphere compatibility, hazard metadata) lands
  in later phases.
- `StepRequirement::Unresolved` is only reachable via one case (a
  precursors/reactants length mismatch on a hand-built
  `AcceptedPrecursorSet`); it never reflects genuine uncertainty about
  whether a step applies given real conditions data, since no such data
  source is wired in yet.
- The method chosen for each `Mix`/`Grind`/`Form`/`Cool` step is a fixed
  template default, not selected from target- or precursor-specific
  evidence — v0.1 has exactly one route family and no per-target selection
  logic yet.
- `total_ranking_score` currently varies only with `process_simplicity`
  (whether the route calcines): `stoichiometric_validity`,
  `precursor_coverage`, `safety_penalty`, and `uncertainty_penalty` are
  structurally constant for every plan the crate can produce today, and
  `evidence_strength`'s weakest-link aggregate is constant too. Not a
  seven-dimensional judgment yet — see `PlanScoreBreakdown`'s doc comment.
- Hazard/safety metadata on precursors (toxic gas, volatile component,
  high-temperature, redox atmosphere warnings — AGENTS.md §15) is not
  modeled anywhere yet. `manual_review_required` and the accompanying
  `Severe` warning are the only safety-related output that exists so far.
- `chematic-crystal` remains unpublished, so the `mikiwame` adapter is not
  connected to `Planner::plan`, and `assess_applicability` can never return
  `InDomain` — only `PartiallyInDomain` or `OutOfDomain` are reachable,
  since there is no structural classifier behind `TargetStructure`'s free
  text.
- The mikiwame adapter's oxidation-state-ambiguity integration point
  (docs/integration.md) is unreachable: mikiwame v0.1 has no `FindingCode`
  for it yet.
- mikiwame's ordinal penalty values (e.g. `ReviewRecommended` → 0.3,
  `OutOfDomain` → 0.5) are placeholder severities, not calibrated against
  any outcome data — the same caveat `RankingWeights::default()` already
  states for its own equal weights.
- `gugen batch`'s isolation is per-target *planning* failure only: if the
  input file itself isn't a well-formed JSON array of
  `TargetSpecification`, the whole command fails before any target is
  attempted — batch isolation doesn't extend to malformed batch input.
- `gugen validate-target` exits non-zero on a self-contradictory target
  (lint-style semantics for scripting) — an exit-code contract AGENTS.md
  §19 doesn't specify, so treat it as this CLI's own convention, not a
  cross-tool guarantee.
- CLI markdown output (`--format markdown`, `gugen explain`) renders
  `PlanScoreBreakdown`/`ConfidenceAssessment` via `{:#?}` (Rust's pretty
  `Debug`) rather than a hand-formatted table — informative and exact, but
  not meant as a stable, parseable text format; use `--format json` (or
  `gugen plan`'s default) for anything that reads the output back.
- **`Composition` has no equivalent-formula-unit-scale normalization**
  (AGENTS.md §21.4's "equivalent formula normalization" invariance does
  not hold): `BaTiO3` and `Ba2Ti2O6` are the same real material but
  produce different `plan_id`s and reaction coefficients. Not fixable
  narrowly without breaking the deliberate, tested exact-amount
  preservation doped/solid-solution formulas need — a real design fork,
  not a bug; see `tasks/todo.md`'s Phase 8 stop-and-report entry for the
  full analysis. No fixture or known real usage currently supplies a
  non-minimal formula-unit scale.
- **`confidence.overall` is structurally constant at `0.75`** for every
  plan with a balanced reaction and non-empty evidence, since
  `process_conditions` is always `0.0` in v0.1 (no provider ever resolves
  a condition) — each sub-score is individually honest, but the average
  cannot yet discriminate between plans of genuinely different real
  uncertainty. Measured across Phase 8's full fixture suite, documented at
  `ConfidenceAssessment`'s definition (score.rs) and in
  `tasks/todo.md`'s Phase 8 stop-and-report entry. Same root cause and
  shape as `total_ranking_score`'s already-documented
  `process_simplicity`-only discrimination.
- `ThermodynamicProvider::reaction_energy` has no unit-consistency check:
  a provider returning wildly wrong-scale values (as if reporting kJ/mol
  where eV/atom is documented) is accepted identically to a correctly
  scaled one — proven directly by a dedicated test
  (`tests/provider_failures.rs`), not just asserted in prose.
- `ProcessEvidenceProvider` output is not deduplicated: a provider
  returning the same precedent twice produces two identical entries in a
  plan's `evidence` list. Cosmetic only (`evidence_strength` aggregates by
  minimum, so duplicates cannot inflate a score), and deliberately left
  as-is rather than adding dedup logic Phase 8 wasn't asked to build.
