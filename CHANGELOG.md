# Changelog

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
gugen has not reached a tagged release yet — everything below is
unreleased, in-progress v0.1 work on the `phase0/architecture-and-scope`
branch (draft PR: https://github.com/kent-tokyo/gugen/pull/1). See
[`tasks/todo.md`](tasks/todo.md) for full phase-by-phase detail behind
each entry.

## [Unreleased]

### Added

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

### Known limitations

- `balance()` only checks individual null-space basis vectors for sign
  validity; a valid reaction requiring a combination of two or more basis
  vectors is not found. Not hit by any case in the AGENTS.md §21.1 test
  list or by realistic small-species-count solid-state reactions so far.
- `PRECURSOR_COUNT_EXCEEDED` and `DUPLICATE_PLAN` rejection codes are
  unreachable by construction in the current search (combinations never
  exceed the configured max size or repeat).
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
