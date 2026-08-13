# gugen — Phase Checklist

Source of truth for phase content is AGENTS.md §26. This file tracks status
only; do not restate the requirements here beyond what's needed to check
work off.

## Phase 0 — Landscape and Architecture — DONE (2026-08-13)

- [x] Existing inorganic synthesis planners surveyed → `docs/competitors.md`
- [x] Precursor-selection research surveyed → `docs/competitors.md`
- [x] Phase-diagram-planning boundary noted (reaction-network/thermodynamic-
      selectivity work treated as a future `ThermodynamicProvider` source,
      not something core reimplements) → `docs/competitors.md`,
      `docs/scientific_scope.md`
- [x] Solid-state synthesis dataset surveyed (Kononova et al. 2019
      text-mined dataset) → `docs/competitors.md`. **License not yet
      checked** — do not bundle or cite as ground truth until confirmed.
      Carried forward as an open item below.
- [x] chematic-crystal API checked → not published anywhere yet (verified
      via crates.io + GitHub API). Trait-boundary path adopted.
      → `docs/integration.md`
- [x] mikiwame API checked → not published anywhere yet. Optional-feature
      path adopted, diagnostic trait boundary deferred to Phase 6.
      → `docs/integration.md`
- [x] crates.io/GitHub name collision checked for `gugen`,
      `chematic-crystal`, `mikiwame` → no collisions found.
      → `docs/competitors.md`
- [x] Data licensing checked for the one dataset surveyed so far → open
      item, not blocking (see below).
- [x] Scientific scope confirmed against AGENTS.md §2–4 →
      `docs/scientific_scope.md`
- [x] Report schema referenced (AGENTS.md §6 taken as-is; no redesign
      needed) → `docs/architecture.md`
- [x] Provider boundary designed (AGENTS.md §8 traits taken as-is; isolation
      rules restated) → `docs/architecture.md`
- [x] Reaction balancing method decided: exact rational linear algebra /
      null-space over the element×species matrix, `i128` checked arithmetic,
      dependency choice deferred to Phase 2 → `docs/architecture.md`

**No stop-and-report condition was triggered.** Name checks came back
clean; chematic-crystal/mikiwame unavailability is the *expected* path
AGENTS.md §5 already designs for, not an obstacle.

**Open item carried forward (not blocking):** Kononova et al. 2019
text-mined dataset license must be checked before it's used as a fixture
source or bundled in any form (Phase 8/§22). If the license turns out to be
incompatible or unclear, that becomes a real stop-and-report at that point
(AGENTS.md §28 "使用候補datasetのライセンスが不明").

## Phase 1 — Foundation — DONE (2026-08-13)

- [x] crate init (`Cargo.toml`, edition 2024, `#![forbid(unsafe_code)]`,
      `rust-version = "1.85"` — the edition-2024 floor; real MSRV alignment
      with chematic-crystal is still pending its release)
- [x] `error.rs` — `GugenError` (8 variants), `ProviderError` (3 variants),
      shared `require_finite` helper
- [x] `config.rs` — `SearchBudget`, `PlanningConfig` (documented default
      search-budget numbers as engineering knobs, not scientific claims)
- [x] validated numeric types — `TemperatureRange`, `DurationRange`,
      `PressureRange`, `RampRateRange` in `process.rs`, all min≤max/finite/
      non-negative-where-physical, generated via one `validated_range!`
      macro instead of four hand-duplicated impls
- [x] `composition.rs` — `Element` (validated against the 118 IUPAC
      symbols), `Composition` (finite, >0, non-empty, deterministic
      `BTreeMap`-backed iteration order)
- [x] `target.rs` — `TargetSpecification`, `PlanningConstraints`
      (minimal — forbidden_elements only, more filters land in Phase 3),
      `TargetMaterialView` boundary trait per `docs/integration.md`
      (simplified to return `&Composition`/`Option<&TargetStructure>`
      directly rather than a separate view-type indirection — no second
      implementor exists yet to justify one)
- [x] report schema types (`report.rs`) — `SynthesisPlanningReport`,
      `TargetSummary`, `ApplicabilityLevel`/`ApplicabilityAssessment`,
      `PlanningWarning`, `UnresolvedRequirement`, `PlanId`. `SynthesisPlan`
      is intentionally minimal (`plan_id` only) — `route_family`,
      `precursors`, `balanced_reaction`, `steps`, `score`, `confidence`,
      `evidence`, `warnings`, `assumptions` are added field-by-field as
      Phases 2-5 land the subsystems that produce them. No ranking/rejection
      *logic* was added, per the Phase 1 instruction in AGENTS.md §26.
- [x] `provenance.rs` — `PlanningProvenance`; `execution_timestamp` is
      caller-supplied (core never reads the system clock, to keep
      determinism a hard invariant rather than an aspiration)
- [x] provider traits (`provider.rs`) — `PrecursorCatalog`,
      `ThermodynamicProvider`, `ProcessEvidenceProvider`, verbatim from
      AGENTS.md §8. Their signatures needed minimal placeholder types with
      no assigned Phase-1 shape (`PrecursorCandidate`, `PrecursorSelection`
      in `precursor.rs`; `ThermodynamicConditions`, `ReactionEnergy` in
      `reaction.rs`; `ProcessPrecedent` in `process.rs`) — each is marked
      with a doc comment naming the phase that fleshes it out.
      `BalancedReaction`/`ReactionSpecies` in `reaction.rs` got a real
      (non-placeholder) shape because AGENTS.md §10 fully constrains it
      (integer coefficients, no zero coefficients) even without literal
      Rust code given. `RejectionCode` (`rejection.rs`) also got its real,
      final shape — AGENTS.md §14 enumerates all 11 variants explicitly, so
      there was nothing left to guess.
- [x] JSON round-trip test (`tests/json_roundtrip.rs`, gated via Cargo's
      `required-features = ["serde"]` so it's simply absent from
      `--no-default-features` builds rather than needing `#[cfg]` guards).
      Also covers the "invalid data must fail on deserialize, not just on
      construction" case for `TemperatureRange` and `Composition` — plain
      `#[derive(Deserialize)]` would have bypassed validation, so those
      types (plus `Element`, `ReactionEnergy`, the four range types) have
      hand-written `Deserialize` impls that re-run the validating
      constructor instead.
- [x] CI (`.github/workflows/ci.yml`) — fmt/clippy/test(all-features)/
      test(no-default-features)/doc/audit, matching AGENTS.md §25's gate
      list. No GitHub remote exists yet (local-git-only per Phase 0's
      scoping question), so this workflow has not run in CI itself yet —
      only verified by running the same commands locally (see below).

**Locally verified, all green:** `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo test --workspace --all-features` (8 passed), `cargo test --workspace
--no-default-features` (5 passed, JSON round-trip test correctly absent),
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps`,
`cargo audit` (0 vulnerabilities, 14 crate dependencies), and as an extra
(§25 "可能なら") check, `cargo check --target wasm32-unknown-unknown
--all-features`.

**No stop-and-report condition was triggered.**

**Known scope gaps, intentional, tracked for their owning phase:**
- `SynthesisPlan` has only `plan_id` — the rest of its fields arrive with
  Phases 2-5.
- `PlanningConstraints` only has `forbidden_elements` — Phase 3 adds the
  rest of AGENTS.md §9's filter list.
- `PrecursorCandidate`/`PrecursorSelection`/`ThermodynamicConditions`/
  `ReactionEnergy`/`ProcessPrecedent` are minimal placeholders without an
  AGENTS.md-given shape to build against yet.
- No `Planner` type exists yet (AGENTS.md §18's usage example doesn't
  compile against this crate yet) — that's Phase 6 (planner orchestrates
  all subsystems, which don't all exist yet).

## Phase 2 — Reaction Balancing — NOT STARTED

- [ ] exact composition representation (builds on `Composition`,
      `BalancedReaction`/`ReactionSpecies` from Phase 1)
- [ ] integer/rational balancing solver (exact-rational null-space per
      `docs/architecture.md`'s Phase 0 decision — finalize the
      rational-arithmetic dependency choice here)
- [ ] byproduct model (curated allow-list: CO₂, H₂O, O₂ to start,
      AGENTS.md §10)
- [ ] multiple solutions (preserve ambiguity when the null space has
      dimension > 1, don't silently pick one)
- [ ] normalization (gcd reduction, zero-coefficient removal)
- [ ] exhaustive tests (AGENTS.md §21.1: 1:1 reactions, carbonate→oxide+CO₂,
      O₂ as byproduct/reactant, multi-precursor, no-solution, multi-solution,
      gcd normalization, element conservation, permutation invariance, large
      coefficients, overflow)
- [ ] `gugen balance` CLI subcommand
- [ ] independent commit, per AGENTS.md §26 ("ここを独立commitにしてください")

## Phase 3–9

Not started. Will be filled in with the same DONE/NOT STARTED tracking
style as each phase begins; see AGENTS.md §26 for phase content and §29 for
the v0.1 completion checklist.
