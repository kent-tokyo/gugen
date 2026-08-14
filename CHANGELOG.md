# Changelog

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
gugen has not reached a tagged release yet — everything below is
unreleased, in-progress v0.1 work on the `phase0/architecture-and-scope`
branch (draft PR: https://github.com/kent-tokyo/gugen/pull/1). See
[`tasks/todo.md`](tasks/todo.md) for full phase-by-phase detail behind
each entry.

## [Unreleased]

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
