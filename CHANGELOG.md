# Changelog

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
gugen has not reached a tagged release yet — everything below is
unreleased, in-progress v0.1 work on the `phase0/architecture-and-scope`
branch (draft PR: https://github.com/kent-tokyo/gugen/pull/1). See
[`tasks/todo.md`](tasks/todo.md) for full phase-by-phase detail behind
each entry.

## [Unreleased]

### Added

- **Phase 4 — solid-state process template.** `RouteFamily`
  (`ConventionalSolidState`), `StepRequirement`, `Atmosphere` (verbatim from
  AGENTS.md), and a minimal, deliberately non-exhaustive set of method
  enums (`MixingMethod`, `GrindingMethod`, `FormingMethod`,
  `HeatingPurpose`, `CoolingMode`, `CharacterizationMethod`). `ProcessStep`
  and the new `evidence.rs` (`EvidenceKind`, `PlanningEvidence`). New
  `conventional_solid_state_template()` generates a
  weigh/mix/grind/form/heat/cool/characterize sequence from an accepted
  precursor set, branching on the actual balanced reaction (a byproduct
  release adds a calcination step) rather than applying one fixed template
  to every material. Temperature, duration, ramp rate, and atmosphere are
  left unresolved rather than guessed. `SynthesisPlan` grew from the Phase 1
  `{ plan_id }` stub to carry `route_family`, `precursors`,
  `balanced_reaction`, `steps`, `evidence`, and `warnings`.
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
- `StepRequirement::Unresolved` is defined but never emitted by
  `conventional_solid_state_template()` yet: the generator always knows
  enough to include a step (with unknown conditions left as `None` inside
  it), never a case where it doesn't know whether a step applies at all.
- The method chosen for each `Mix`/`Grind`/`Form`/`Cool` step is a fixed
  template default, not selected from target- or precursor-specific
  evidence — v0.1 has exactly one route family and no per-target selection
  logic yet.
