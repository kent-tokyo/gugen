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

## Phase 5 — Ranking and Explanation — DONE

- [x] `Score01` (validated newtype in `[0, 1]`; AGENTS.md only names the
      type, doesn't give a shape -- matches every other validated numeric
      type in this crate: rejects non-finite/out-of-range at construction).
- [x] `PlanScoreBreakdown` and `RankingWeights` (verbatim, AGENTS.md §13).
      `RankingWeights::default()` uses equal weight (`1.0`) on every
      dimension, documented as the only defensible default given §13's
      explicit "validation corpusを見て恣意的に調整しない" / "holdoutを見て
      weight変更しない" (no validation corpus exists yet -- that's Phase 8).
      `ranking_weights_digest()` (std `DefaultHasher`, not cryptographic --
      only needs to detect "did the config change") for
      `PlanningProvenance.ranking_config_digest`.
- [x] `ConfidenceAssessment` (verbatim, AGENTS.md §16) — kept as four
      independent dimensions rather than collapsed into `overall`, per
      §16's explicit "条件未確定でも反応式が確実なケースがあります。単一
      confidenceに潰さないでください."
- [x] `score_plan()`: computes both breakdowns from a plan's actual
      ingredients (target, applicability, balanced reaction, steps,
      evidence, weights). `thermodynamic_support` is always `None` (no
      `ThermodynamicProvider` wired in) and is excluded from the weighted
      average entirely, never treated as `0.0` (§13: "missing
      thermodynamic dataを自動的に失敗扱いしない") — regression test
      included. `evidence_strength` aggregates per-item `EvidenceStrength`
      by **minimum** (weakest link), not mean, specifically so one Strong
      entry can't outweigh several Weak template defaults; `0.0` if no
      evidence at all (§13: "evidenceなしのplanはconfidenceを下げる") —
      regression test included.
- [x] **Honest documented limitation, found via advisor review before
      push:** in v0.1, `stoichiometric_validity`, `precursor_coverage`,
      `safety_penalty`, and `uncertainty_penalty` are structurally constant
      across every plan the crate can currently produce (exact balancing,
      hard-filtered coverage, no hazard provider, no resolved conditions
      respectively), and `evidence_strength`'s weakest-link aggregate is
      also constant (`0.25`) because the generator always attaches at
      least one `Weak` entry. **`total_ranking_score` currently varies
      only with `process_simplicity`** — i.e. only with whether the route
      calcines. Stated explicitly in `PlanScoreBreakdown`'s doc comment
      rather than left for a reader to discover; this is the exact kind of
      overstated discrimination the §22/§29 false-confidence audit checks
      for, so it's flagged now rather than at Phase 8.
- [x] Real formula bug found by the crate's own tests, fixed before
      commit: the first draft subtracted a *weighted sum* penalty from a
      *weighted average* positive score -- different scales, so one maxed
      penalty (e.g. `uncertainty_penalty=1.0`, true for every v0.1 plan)
      could zero out a legitimately strong positive average. Fixed by
      normalizing the penalty side as a weighted average too before
      subtracting.
- [x] `manual_review_required: bool` added to `SynthesisPlan`, always
      `true` in v0.1 (AGENTS.md §15's `pub manual_review_required: bool`,
      a v0.1 completion criterion per §29 that AGENTS.md §26 doesn't
      actually assign to any phase). Paired with a mandatory `Severe`
      `PlanningWarning` stating that `safety_penalty=0.0` reflects "no
      hazard data source exists," not "assessed safe" (§15: "unknown
      hazardを安全と扱わない"). This was caught by advisor review, not the
      original Phase 5 checklist -- §29's other unowned safety criterion,
      "safety warningがある", is satisfied by this same warning; hazard
      *metadata on precursors* (toxic gas, volatile component, etc.) is
      still not modeled anywhere and remains open.
- [x] `PlanningAssumption` (shape not given verbatim). `score_plan` returns
      exactly one entry: that per-plan `applicability` is copied from the
      target-level assessment rather than independently assessed per
      route family, true only because v0.1 has a single route family.
      Everything else the generator assumes is already surfaced as
      `PlanningEvidence.limitations` or a `PlanningWarning`, so isn't
      duplicated here.
- [x] Per-plan `unresolved: Vec<UnresolvedRequirement>` populated from
      every `None` condition field across a plan's steps (temperature,
      duration, atmosphere, ramp on `Heat`; duration on `Grind`; pressure
      on `Form`), each with the same "no provider wired in yet" reason.
- [x] "Alternatives" (§26's Phase 5 checklist item) needs no new type:
      already satisfied by `plans: Vec<SynthesisPlan>` existing and
      `score_plan` giving each a comparable `total_ranking_score` --
      *ranking* multiple plans (Phase 6's `Planner` job) is what turns
      that into "alternatives," not a new data type.
      "Rejected candidate explanations" needs no new code either: already
      satisfied by `RejectedCandidate.explanation` (Phase 1/3); no new
      rejection path was introduced by scoring.
- [x] `SynthesisPlan` now carries every field AGENTS.md §6 lists (plus
      `manual_review_required`, not in §6's snippet but required by §15).
      JSON round-trip test rebuilt around real `score_plan()` output.

**Locally verified, all green:** fmt, clippy -D warnings (workspace, all
targets, all features), test under `--all-features` (47 lib tests + 4
integration) and `--no-default-features` (47 lib tests), doc -D warnings,
`cargo run --example balance_batio3` (unchanged output), `cargo build
--features serde,clap --bin gugen`, `cargo check --target
wasm32-unknown-unknown`, cargo audit (0 vulnerabilities).

**No stop-and-report condition was triggered.**

## Phase 6 — Integration — DONE (chematic-crystal adapter blocked — unpublished)

**Mid-phase discovery:** `mikiwame` is now published on crates.io (v0.1.0,
owner `kent-tokyo`, published 2026-08-14, after the Phase 0 check that
found it unpublished). `chematic-crystal` is still unpublished (re-checked
the same day). This is a material change to the plan the phase started
with, so it's called out explicitly rather than folded in silently.

- [x] `Planner` (AGENTS.md §18): `new(catalog, process_evidence_provider,
      thermodynamic_provider, config)` and `offline_minimal(catalog,
      config)`. `plan(&target, execution_timestamp)` orchestrates catalog
      → `search_precursor_sets` → `conventional_solid_state_template` →
      `score_plan` → assembled `SynthesisPlanningReport`. One deliberate
      deviation from §18's illustrative single-argument `plan(&target)`:
      `execution_timestamp` is a required parameter, not read from the
      system clock (`PlanningProvenance.execution_timestamp` was already
      documented in Phase 1 as caller-supplied specifically so the
      deterministic core never touches wall-clock time; a Phase 7 CLI is
      what will supply a real one).
- [x] Real design/implementation gaps found and fixed via advisor review
      before committing:
      - `search_precursor_sets` used to truncate `accepted` to
        `max_plans_returned` in raw generation order, before any ranking
        existed. Moved that truncation into `Planner`, after sorting by
        `total_ranking_score` -- so the plans kept are actually the best
        ones, not whichever combination happened to be generated first.
        Overflow now produces an explained `RejectedCandidate`
        (`SearchBudgetExhausted`, reused rather than adding a 12th
        `RejectionCode` since `max_plans_returned` is itself a
        `SearchBudget` field).
      - `plan_id` is derived from precursor-set + reaction content (a
        `DefaultHasher` digest), not position, so it survives catalog
        reordering and ranking changes. The first version of the test for
        this didn't actually exercise position-independence (both
        catalogs it compared were pre-sorted identically by
        `InMemoryPrecursorCatalog`) -- rewritten to add an unrelated
        catalog entry instead and check the shared plans' ids didn't move.
      - `assess_applicability` initially returned `InDomain` whenever a
        target had *any* structure info, regardless of content. Since
        `TargetStructure` is free text with no classifier behind it, and
        AGENTS.md §16 lists both `InDomain` and `OutOfDomain` examples
        *with* structure present, this was optimistic without
        justification. Now `PartiallyInDomain` in both the formula-only
        and the structure-present-but-unclassified case.
- [x] Invalid-target handling: a target whose composition requires an
      element `PlanningConstraints.forbidden_elements` also forbids is
      self-contradictory (no plan could ever satisfy both) and gets an
      early abstention -- `OutOfDomain` applicability, empty `plans`, one
      `UnresolvedRequirement` explaining why -- rather than running a
      search doomed to reject every combination individually. An empty
      catalog result (no candidates share any element with the target) is
      a separate, non-domain outcome: a `PlanningWarning`, not an
      applicability downgrade.
- [x] §21.5 ("one provider失敗でplanning全体を失敗させない") implemented
      for the first time: `ThermodynamicProvider`/`ProcessEvidenceProvider`
      failures are caught per-candidate and degrade to an `Info`
      `PlanningWarning` on the affected plan; a `PrecursorCatalog` failure
      still propagates (`GugenError::Catalog`, new `#[from] ProviderError`
      conversion), since planning cannot proceed without one at all. Both
      provider failure paths are tested together in one `plan()` call.
      A working `ThermodynamicProvider` attaches its reaction energy as
      `EvidenceKind::ThermodynamicData` evidence -- explicitly *not*
      converted into a favorability score (AGENTS.md §4.3: thermodynamic
      favorability is not experimental likelihood), avoiding the exact
      unsourced-heuristic trap `score_plan` already sidestepped in Phase 5.
- [x] `PlanningConfig` gained `ranking_weights: RankingWeights` -- anticipated
      in its own Phase 1 doc comment ("grows... once Phase 5 lands
      ranking") but not actually added until now.
- [x] mikiwame adapter (`src/mikiwame_adapter.rs`, feature-gated):
      optional dependency (`mikiwame = { version = "0.1.0",
      default-features = false, optional = true }`), `structural_effects()`
      maps a real `mikiwame::MaterialDiagnosticReport` (from actual
      `mikiwame::analyze()` calls in tests, not a hand-rolled fixture) to
      `StructuralDiagnosticEffects { abstain_reason, warnings,
      confidence_penalty }` per `docs/integration.md`'s mapping. Not
      auto-wired into `Planner::plan` -- `TargetStructure` still can't
      produce a `mikiwame::PeriodicStructureView` (needs real lattice/site
      data that only `chematic-crystal` would supply, and it still doesn't
      exist). Callers with their own structure data call the adapter
      directly and apply the result to a `SynthesisPlan` themselves (module
      doc now spells out the order: check `abstain_reason` first, then fold
      `warnings` in, then use `confidence_penalty` to lower
      `ConfidenceAssessment` -- all target fields are public).
      `mikiwame::Verdict::StrongAnomalyDetected`/`InvalidInput` -> abstain;
      `Severity::High`/`Critical` -> `Severe` warning; low
      `ApplicabilityLevel` -> `confidence_penalty`, no hard reject; any
      other non-`Info` finding -> a `PlanningWarning`.
      `confidence_penalty` is `verdict_penalty.max(applicability_penalty)`,
      not a sum or average -- both signals can name the same underlying
      structural problem, so summing would double-penalize and averaging
      would let a mild applicability reading water down a severe verdict.
      Oxidation-state ambiguity is unreachable, not a no-op: mikiwame v0.1
      has no `FindingCode` for it yet.
      Bugs found and fixed before committing:
      - `E0689` ambiguous numeric type on `verdict_penalty.max(...)` --
        the match producing it had no type annotation; fixed with an
        explicit `(Option<String>, f64)` binding.
      - A defensive wildcard was added to the `Verdict` match assuming it
        was `#[non_exhaustive]` like mikiwame's other enums; it isn't
        (verified by reading mikiwame's actual `model.rs`), so rustc/clippy
        flagged the arm as unreachable. Removed it -- `Severity`,
        `ApplicabilityLevel`, and `FindingCode` are `#[non_exhaustive]` and
        do get defensive wildcards that degrade toward caution, `Verdict`
        is the one exception with an exhaustive match.
      - A test fixture meant to exercise "`Info`-only findings produce no
        warnings" used two elements at full occupancy on the same site,
        which also tripped `DisorderOccupancySumExceedsOne` (High
        severity, 1.0+1.0 > 1.0) -- an unplanned second finding that
        changed the verdict and failed the test. Fixed the fixture
        (0.5+0.5, sums to exactly 1.0) to actually test what it claimed,
        and kept the accidental discovery as its own dedicated test
        (`a_high_severity_finding_maps_to_a_severe_warning`) instead of
        just avoiding it.
- [ ] chematic-crystal adapter: not applicable yet, still unpublished.
      `TargetMaterialView` continues absorbing this per docs/integration.md.
      Revisit once it publishes -- this is the one Phase 6 deliverable that
      stays open for an external reason, not an internal gap.
- [x] Composition/structure handoff: `Planner` reads `target.composition`/
      `target.structure` directly rather than through `TargetMaterialView`
      accessor methods, since it also needs `constraints`/`desired_phase`,
      which aren't part of that trait. Revisit if `TargetMaterialView`
      grows to cover the full `TargetSpecification` surface.
- [x] Feature-gated builds: `--no-default-features` (mikiwame module
      absent from the compiled crate) and `--no-default-features --features
      mikiwame` (isolated) both verified green, alongside `--all-features`.

**Locally verified, all green (full phase, including mikiwame):** fmt,
clippy -D warnings (workspace, all features), test under `--all-features`
(59 lib tests + 4 integration), `--no-default-features` (54 lib tests), and
`--no-default-features --features mikiwame` (59 lib tests, isolated), doc
-D warnings, `cargo run --example balance_batio3` (unchanged output),
`cargo build --features serde,clap --bin gugen`, `cargo check --target
wasm32-unknown-unknown` (with and without `mikiwame`), cargo audit (0
vulnerabilities, 32 crates).

**No stop-and-report condition was triggered** by anything in this phase.
mikiwame's publication is a material plan change, flagged to the user, not
a stop-and-report trigger in AGENTS.md §28's sense (no name collision, no
license problem, no divergent API forcing tight coupling -- the opposite:
a previously-unavailable optional dependency became available).
`chematic-crystal` remaining unpublished is the exact contingency AGENTS.md
§5 anticipated, absorbed by the `TargetMaterialView` boundary already in
place since Phase 1 -- not a new blocker.

## Phase 7 — CLI and Batch — DONE

- [x] `gugen plan target.json --catalog precursors.json [--output report.json]
      [--format json|markdown]` (AGENTS.md §19): loads a `TargetSpecification`
      and a JSON array of `PrecursorCandidate` (reusing existing public types
      as the file formats rather than inventing wrapper schemas), builds a
      `Planner::offline_minimal` (no thermodynamic/process-evidence provider
      -- not shown in §19's CLI examples, so not added), and writes the
      report as pretty JSON or a rendered markdown document.
- [x] `gugen balance reaction.json` -- unchanged from Phase 2.
- [x] `gugen explain report.json --plan plan-001`: finds one plan by id in a
      previously generated report and prints its full detail (steps, score
      breakdown, confidence, evidence, warnings, assumptions, unresolved);
      errors listing the available ids if the given one isn't present.
- [x] `gugen validate-target target.json`: deserializes (which already
      validates via every type's custom `Deserialize`, e.g. duplicate
      elements, non-finite/negative amounts, invalid element symbols) and
      additionally checks for the same self-contradiction
      `Planner::plan` abstains on (an element both required by the
      composition and forbidden by constraints). Exits non-zero on a
      self-contradictory target -- a deliberate lint-style exit-code
      contract AGENTS.md §19 doesn't state explicitly, noted here as a
      decision rather than an accident.
- [x] `gugen doctor` (AGENTS.md §19's full field list): gugen version,
      schema version, chematic-crystal version (not integrated),
      mikiwame integration status (reads the compiled-in feature flag),
      enabled route families, precursor catalog version, thermodynamic/
      process evidence provider, ranking config digest, deterministic
      mode, supported domain, known limitations. No input file needed --
      static build/configuration diagnostics only.
- [x] `gugen batch input.json --catalog precursors.json [--output out.json]`:
      a JSON array of `TargetSpecification`, planned independently against
      one shared `Planner`. One target's `GugenError` becomes a `BatchEntry
      { index, report: None, error: Some(..) }` rather than aborting the
      rest (AGENTS.md §26's explicit "batchでは一件の失敗で全体を失敗させない"
      requirement) -- tested with a custom `PrecursorCatalog` that fails
      only for Sr-containing targets, since `InMemoryPrecursorCatalog`
      itself never errors and so can't exercise this path alone.
      `BatchEntry` has no separate `ok` flag -- `error.is_none()` already
      says that; a redundant field on public JSON output isn't worth it.
- [x] `execution_timestamp`: the CLI binary (`src/bin/gugen.rs`) is the one
      place in the crate allowed to read the system clock
      (`now_rfc3339`), per the contract `PlanningProvenance
      .execution_timestamp` was documented with since Phase 1. Implemented
      via Howard Hinnant's public-domain days-from-civil algorithm rather
      than a new date/time dependency -- one well-known ~15-line
      conversion, tested against fixed epoch-day values, not the live
      clock.
- [x] Markdown rendering (`render_report_markdown`/`render_plan_detail`,
      shared by `plan --format markdown` and `explain`) renders
      `Composition` as explicit `element:amount` pairs
      (`format_composition`), not a concatenated pseudo-formula --
      `Composition` iterates in alphabetical `BTreeMap` order, so
      concatenating symbols directly would print `BaO3Ti` for BaTiO3 and
      `O2Ti` for TiO2: chemically wrong-looking output from a tool whose
      whole premise is not fabricating things that look authoritative.
      Caught by actually reading the CLI's own real output during manual
      testing, not by a spec requirement.
- [x] Real bug found and fixed via advisor review before committing (a
      Phase 3 correctness bug, not a Phase 7 one -- surfaced only now
      because `gugen plan`'s CLI output is the first place anyone actually
      looked at a full multi-candidate report end to end):
      `search_precursor_sets` could silently **double-accept** a precursor
      set. A redundant Ba source (e.g. catalog has both BaCO3 and BaO)
      means a larger combination ({BaCO3, BaO, TiO2}) can balance with the
      redundant precursor's coefficient solved to zero -- `balance()` then
      drops it, collapsing the result to the exact same precursors and
      reaction a smaller combination ({BaO, TiO2}) already produced
      separately. Both were being pushed into `accepted`, so `Planner`
      would rank and return the *same plan twice* (identical `plan_id`,
      since `plan_id` is content-derived). Fixed at the root
      (`search_precursor_sets`, not downstream in `Planner`, so every
      caller benefits): the newly-found `AcceptedPrecursorSet` is compared
      against everything already in `accepted`, and an equal one is
      rejected as `RejectionCode::DuplicatePlan` (a code that already
      existed in the closed AGENTS.md §14 set but whose doc comment
      wrongly claimed it was "unreachable by construction in Phase 3" --
      corrected). Regression test:
      `a_redundant_larger_combination_is_rejected_as_a_duplicate_not_double_accepted`
      in `precursor.rs`, built on the exact BaCO3/BaO/TiO2 fixture that
      exposed it. Also added a `plan_id`-uniqueness assertion to an
      existing `Planner` test (`offline_minimal_produces_ranked_plans_...`)
      -- the check that would have caught this automatically instead of by
      eyeballing CLI output.

**Locally verified, all green:** fmt, clippy -D warnings (workspace, all
features), test under `--all-features` (60 lib tests + 8 bin tests + 4
integration), `--no-default-features` (55 lib tests), and
`--no-default-features --features mikiwame` (60 lib tests, isolated), doc
-D warnings, `cargo run --example balance_batio3` (unchanged output),
`cargo build --features serde,clap --bin gugen`, `cargo check --target
wasm32-unknown-unknown` (with and without `mikiwame`), cargo audit (0
vulnerabilities, 32 crates, no new dependencies this phase). Additionally
manually exercised every subcommand (`doctor`, `validate-target`, `plan`
json/markdown, `explain`, `batch`) against real fixture files, not just
the automated test suite -- this is what surfaced the duplicate-plan bug
above.

**No stop-and-report condition was triggered.** The duplicate-plan bug is
a real correctness fix, not a scope change: no new package name, no new
license, no new external dependency, no unresolved API divergence.

## Phase 8–9

Not started. Will be filled in with the same DONE/NOT STARTED tracking
style as each phase begins; see AGENTS.md §26 for phase content and §29 for
the v0.1 completion checklist.
