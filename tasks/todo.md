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

## Phase 2 — Reaction Balancing — DONE (2026-08-13)

- [x] **Amendment to Phase 1, landed first as its own commit** (not itself
      a Phase 2 checklist item, but required before Phase 2 could be exact
      rather than float-approximate): `Composition` now stores amounts as
      exact rationals (`Frac`, `src/frac.rs`), rationalized once at
      construction via continued-fraction convergents, not re-approximated
      on every downstream read. Public `f64` constructor/accessor
      signatures are unchanged.
- [x] exact composition representation — `Frac`-backed, see above
- [x] integer/rational balancing solver (`src/balance.rs`) — exact-rational
      Gauss-Jordan elimination to RREF over the element x species matrix
      (reactants positive, products negated), null-space basis vectors
      constructed from free columns, each checked for sign-validity
      (all-non-negative or all-non-positive, else skipped) and scaled to
      minimal integers via LCM/gcd. Dependency decision: no external
      rational-arithmetic crate — `Frac` is ~180 lines of checked-`i128`
      arithmetic, simpler than wrapping a crate's unchecked operators in
      overflow checks at every call site.
- [x] byproduct model — `curated_byproducts()` returns CO₂/H₂O/O₂ as real
      `Composition`s (AGENTS.md §10's curated set, not open-ended)
- [x] multiple solutions — verified against the classic Fe + O₂ ->
      {FeO, Fe₂O₃, Fe₃O₄} case: the solver independently returns all three
      as separate `BalancedReaction`s, not one arbitrary pick
- [x] normalization — gcd-reduced integer coefficients (tested: 4/2/4
      H₂/O₂/H₂O collapses to 2/1/2, not left as a common multiple),
      zero-coefficient species dropped from the output
- [x] exhaustive tests (`src/balance.rs`, 13 tests) — all of AGENTS.md
      §21.1's list: 1:1 reaction, carbonate→oxide+CO₂, O₂ as byproduct,
      O₂ as reactant, multi-precursor, no-solution (disjoint elements),
      multi-solution (iron oxide family), gcd normalization, element
      conservation (independently re-checked, not just trusting the
      solver), permutation invariance, large coefficients, and overflow
      (isolated at the `Frac`/scaling level with two near-`i128::MAX`
      denominators, since the public API's `Composition` bounds make
      overflow through realistic input essentially unreachable)
- [x] `gugen balance` CLI subcommand (`src/bin/gugen.rs`, clap-based,
      `required-features = ["serde", "clap"]`) — manually run end-to-end
      against real fixture files: single solution, 3-way multi-solution,
      no-solution (exit 0, empty array + stderr note — absence of a
      balance isn't a program error), missing file and malformed JSON
      (both exit 1 with a message, no panic)
- [x] independent commits, per AGENTS.md §26: the Composition/Frac
      amendment landed separately from the solver itself

**Locally verified, all green:** fmt, clippy -D warnings (workspace, all
targets, all features), test under `--all-features`, `--no-default-features`,
and `--features serde` (serde without clap, confirming the bin target is
correctly excluded rather than failing to build), doc -D warnings, cargo
audit (0 vulnerabilities, 31 crate dependencies — up from 14 after adding
clap's tree), wasm32-unknown-unknown check (library only, `--lib`; the CLI
binary is not expected to target wasm).

**No stop-and-report condition was triggered.**

**Known limitation, deliberate (see the `ponytail:` comment in
`src/balance.rs`):** only individual null-space basis vectors are checked
for sign-validity; a valid reaction requiring a *combination* of two or
more basis vectors is not searched for. This covers every §21.1 test case
and every realistic small-species-count solid-state reaction. Revisit with
a bounded combination search only if a real fixture surfaces a case that
needs one — don't build it speculatively.

## Phase 3 — Precursor Enumeration — DONE (2026-08-13)

- [x] in-memory precursor catalog (`InMemoryPrecursorCatalog`,
      `src/precursor.rs`) — implements `PrecursorCatalog`, sorts by
      `PrecursorId` and deduplicates same-id entries at construction
      regardless of insertion order (AGENTS.md §21.2/§21.4)
- [x] candidate generation from target elements — `candidates_for` scopes
      to catalog entries sharing at least one element with the target
      (§9's stated generation principle); combination search then applies
      the real filters below
- [x] the rest of AGENTS.md §9's applicable constraint filters:
      target-element coverage, forbidden elements (still the only field
      on `PlanningConstraints` — no new filter fields were invented beyond
      what's specified), and "target元素へ残らない元素の除去可能性" (an
      extra element introduced by a precursor must be removable via a
      curated byproduct, or the set is rejected)
- [x] bounded search respecting `SearchBudget` — deterministic
      size-then-lexicographic combination enumeration, all three budget
      fields honored (`max_precursors_per_plan` bounds combination size,
      `max_precursor_sets` bounds total combinations evaluated,
      `max_plans_returned` truncates the final accepted list)
- [x] rejection reasons — `MissingTargetElement`, `ForbiddenElementPresent`,
      `UnsupportedByproductRequired`, `NoStoichiometricBalance`,
      `SearchBudgetExhausted` are all reachable and tested.
      `PrecursorCountExceeded` and `DuplicatePlan` are unreachable *by
      construction* in Phase 3 (combinations never exceed the configured
      max size; index-based generation never revisits the same
      combination) — documented on `search_precursor_sets`, not silently
      unreachable. `AtmosphereConflict`, `HazardPolicyBlocked`,
      `ThermodynamicDataUnavailable`, `UserConstraintViolation` belong to
      later phases (atmosphere/process modeling, safety, ranking, and a
      richer `PlanningConstraints` respectively).
- [x] deterministic ordering — catalog order and combination-generation
      order are both fixed regardless of input order; tested by shuffling
      the catalog and asserting identical accepted sets
- [x] budget-exhaustion diagnostics — surfaced as a `RejectedCandidate`
      sentinel (`SearchBudgetExhausted` reason, empty `precursors`) rather
      than a separate flag, so it can't be silently dropped by a caller
      that only looks at one field; tested distinctly from "no candidates
      found"

**Byproduct-inclusion strategy, verified empirically before building the
search on top of it:** offering every curated byproduct as a candidate
product in one `balance()` call, rather than trying byproduct subsets
smallest-first, was suspected to risk defeating `balance()`'s
single-basis-vector heuristic (AGENTS.md §10's ambiguity-preservation
requirement colliding with the `ponytail:` limitation already documented
on `balance()`). Empirically, for the BaCO3 + TiO2 -> BaTiO3 + CO2 test
case, offering all three curated byproducts at once actually still found
the correct single answer (H2O/O2 naturally zeroed out) -- see
`balance::tests::all_curated_byproducts_at_once_happens_to_work_for_this_case_but_search_does_not_rely_on_it`.
That result does not prove the general case is safe, so
`search_precursor_sets` uses the strictly safer smallest-subset-first
strategy anyway; the cost is trivial (2^3 = 8 subsets for 3 curated
byproducts).

**Extended `PrecursorCandidate`** with `availability: Option<AvailabilityMetadata>`
(minimal placeholder — AGENTS.md §9 names availability as a filter
candidate without specifying a shape). Missing availability does not
block acceptance, tested explicitly. Redox-compatibility,
atmosphere-compatibility, and hazard/toxicity metadata are *not* added
yet — AGENTS.md §26's actual Phase 3 checklist doesn't list them (they
belong to later process/safety phases), correcting an over-broad forward
comment left in Phase 1.

**Locally verified, all green:** fmt, clippy -D warnings (workspace, all
targets, all features), test under `--all-features` (36 lib tests + 4
integration), `--no-default-features` (36 lib tests, no serde/clap
needed for search itself), and `--features serde`, doc -D warnings, cargo
audit (0 vulnerabilities).

**No stop-and-report condition was triggered.**

## Phase 4 — Solid-State Process Template — DONE

- [x] `RouteFamily` (single `ConventionalSolidState` variant, AGENTS.md §13)
- [x] `StepRequirement` (verbatim, AGENTS.md §11) and `Atmosphere` + `InertGas`/
      `ReducingAgent` (verbatim `Atmosphere`, minimal 2-variant gas enums,
      AGENTS.md §12)
- [x] `ProcessStep` (verbatim, AGENTS.md §6) and its method/purpose
      sub-enums (`MixingMethod`, `GrindingMethod`, `FormingMethod`,
      `HeatingPurpose`, `CoolingMode`, `CharacterizationMethod`) — not
      given verbatim, so each is kept to 2-3 standard techniques the §11
      template outline itself needs (calcination/sintering/annealing map
      directly onto §11's 仮焼/本焼成; XRD is the only characterization
      method §11 names). Documented as "add a variant only when a real
      Phase 8 fixture needs one," not pre-enumerated.
- [x] `MaterialAmount` (§6's `Weigh` step payload, not given verbatim) —
      `{ precursor, formula_units, mass_grams: Option<f64> }`.
      `mass_grams` is always `None`: gugen has no atomic-weight table, and
      no phase currently owns building one. Flagged as a real, currently
      unfilled gap rather than silently omitting the field.
- [x] `PlannedStep { requirement: StepRequirement, step: ProcessStep }` —
      §6 shows `SynthesisPlan.steps` as a bare `Vec<ProcessStep>`, but §11
      requires a per-step requirement marker with nowhere else to carry it.
      `SynthesisPlan.steps` is `Vec<PlannedStep>`, documented as a
      deliberate deviation from the literal §6 signature.
- [x] `evidence.rs`: `EvidenceKind` (verbatim, 9 variants), `PlanningEvidence`
      (verbatim fields, AGENTS.md §7) — first real use, not present in
      Phases 1-3. `EvidenceStrength` (categorical `Weak`/`Moderate`/`Strong`,
      not given verbatim) is kept explicitly distinct from Phase 5's
      `PlanScoreBreakdown.evidence_strength: Score01` (AGENTS.md §13) — a
      plan-level *aggregate* across many evidence items — so no enum→number
      mapping is invented before Phase 5 needs one and can document its
      rationale. `EvidenceScope` (not given verbatim, minimal
      `ExactTarget`/`SimilarMaterial`/`GeneralRule`).
- [x] `conventional_solid_state_template(target, &AcceptedPrecursorSet)`:
      weigh → mix → grind → optional form → [calcine, regrind — only if the
      balanced reaction releases a byproduct beyond the target] → sinter →
      cool → recommended XRD check. The regrind step (AGENTS.md §11's
      numbered step 6, 再粉砕) sits between calcination and final firing,
      matching the outline. Every condition without evidence (temperature,
      duration, ramp, atmosphere) is left `None` rather than guessed
      (AGENTS.md §4.1), with a `PlanningWarning` stating why.
      `StepRequirement::Unresolved` is emitted for exactly one case so far:
      `AcceptedPrecursorSet.precursors`/`reaction.reactants` length
      mismatch (see below) — a hand-built input the search path can no
      longer produce, but the generator is a public function over public
      fields and must not assume its caller went through the search.
- [x] Regression test (AGENTS.md §11 "すべての材料へ同じtemplateを適用し
      てはいけません"): the carbonate route to BaTiO3 (BaCO3 + TiO2 → BaTiO3
      + CO2) gets calcination + regrind steps; the oxide-only route (BaO +
      TiO2 → BaTiO3) to the *same target* does not — proves the template
      branches on real chemistry, not target identity alone.
- [x] Real bug found and fixed while designing the `Weigh` step:
      `AcceptedPrecursorSet.precursors` was built from the unfiltered
      candidate combo and assumed index-alignment with
      `reaction.reactants`, but `balance()` drops any reactant whose solved
      coefficient is zero — so a redundant precursor in the combo could
      silently desync the two lists. Fixed in `search_precursor_sets` by
      matching each reactant back to its precursor id by composition
      instead of assuming index alignment; regression test added.
- [x] Second bug found on advisor review of the first: even after that
      fix, `conventional_solid_state_template` itself `zip()`s
      `accepted.precursors` with `reaction.reactants`, which silently
      truncates on a length mismatch — reachable by anyone constructing
      `AcceptedPrecursorSet` by hand (a public struct with public fields),
      not just via search. Fixed by guarding the lengths: a mismatch now
      produces an empty `Weigh.materials` with
      `StepRequirement::Unresolved` and a `Severe` `PlanningWarning`,
      instead of a truncated materials list that reads as complete.
      Regression test added.
- [x] `SynthesisPlan` extended from the Phase 1 `{ plan_id }` stub to
      `{ plan_id, route_family, precursors, balanced_reaction, steps,
      evidence, warnings }` — everything Phases 2-4 now produce. `score`,
      `confidence`, per-plan `applicability`, `assumptions`, and per-plan
      `unresolved` remain deferred to Phase 5 (ranking/confidence don't
      exist yet). JSON round-trip test rebuilt around a real
      `conventional_solid_state_template()` output (not hand-authored) so
      the new nested enums/options actually get exercised through
      serialize/deserialize.

**Locally verified, all green:** fmt, clippy -D warnings (workspace, all
targets, all features), test under `--all-features` (39 lib tests + 4
integration) and `--no-default-features` (39 lib tests), doc -D warnings,
`cargo run --example balance_batio3` (unchanged output), `cargo build
--features serde,clap --bin gugen`, `cargo check --target
wasm32-unknown-unknown`, cargo audit (0 vulnerabilities).

**No stop-and-report condition was triggered.**

## Phase 5–9

Not started. Will be filled in with the same DONE/NOT STARTED tracking
style as each phase begins; see AGENTS.md §26 for phase content and §29 for
the v0.1 completion checklist.
