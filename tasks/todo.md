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

**Open item carried forward, resolved in Phase 8:** Kononova et al. 2019
text-mined dataset license was checked before use (via the figshare API
the dataset is actually hosted at) -- CC BY 4.0, permitting reuse with
attribution. See Phase 8's section below for the verification and how it
was used.

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
      testing, not by a spec requirement. A second advisor pass after the
      first commit found the identical bug pre-existing in
      `examples/balance_batio3.rs`'s own `formula()` helper -- the
      README-example source AGENTS.md §20 requires output to be "copied
      verbatim from running" (per that file's own doc comment). Fixed the
      same way and both README/README_ja code blocks re-synced to the
      corrected real output (`1 Ba:1, O:1 + 1 O:2, Ti:1 -> 1 Ba:1, O:3,
      Ti:1`), landed as a small follow-up commit rather than left for
      Phase 9 since it directly contradicted the fix just committed.
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

## Phase 8 — Validation — DONE

**Kononova et al. 2019 dataset license, resolved.** Carried as an open
item since Phase 0 (line ~45 above). Checked via the figshare API
(`GET https://api.figshare.com/v2/articles/9722159`, the DOI the dataset
is actually hosted at): `license.name == "CC BY 4.0"`. Not assumed, not
inferred from the GitHub code repo (which has no LICENSE file --
`license: null` via `gh api`) -- the data and the code are hosted and
licensed separately, and only the data's license matters for fixture
sourcing. CC BY 4.0 permits reuse with attribution. Not a stop-and-report
trigger (AGENTS.md §28's "ライセンスが不明" doesn't apply once it's
known); resolved by using individual cited routes with attribution, not
bundling the dataset.

- [x] Curated fixtures (`tests/validation.rs`, AGENTS.md §21.3): 5
      fixtures spanning the phase's own candidate categories (perovskite
      oxide, spinel oxide, phosphate, simple binary oxide, carbonate
      precursor route), none written from memory:
      - LaAlO3 (perovskite): 0.5 La2O3 + 0.5 Al2O3 -> LaAlO3, cross-checked
        across 19 independent paper DOIs in the Kononova dataset.
      - MgAl2O4 (spinel): 1 Al2O3 + 1 MgO -> MgAl2O4, 20 independent DOIs.
      - Zn3(PO4)2 (phosphate): 3 ZnO + 1 P2O5 -> Zn3(PO4)2, 2 independent
        DOIs.
      - CaO (simple binary oxide): CaCO3 -> CaO + CO2 at 900°C. **Not**
        from Kononova -- querying the full 30,031-reaction dataset for any
        target matching a plain binary-oxide formula (NiO, Fe2O3, ZnO,
        CuO, CaO, ...) returns zero results, an empirical finding (that
        corpus's papers use commodity oxides as precursors, never as the
        reported synthesis target). Sourced instead from Seesanong et al.,
        "Low-Cost and Eco-Friendly Calcium Oxide Prepared via Thermal
        Decompositions of Calcium Carbonate and Calcium Acetate Precursors
        Derived from Waste Oyster Shells," *Materials* 17(15), 3875
        (2024), DOI 10.3390/ma17153875.
      - BaTiO3 (carbonate route): 1 BaCO3 + 1 TiO2 -> BaTiO3 + CO2, the
        strongest-attested route in the suite (88 independent DOIs) --
        also the same route this crate's own examples/tests have used
        since Phase 1, now independently cross-checked against real
        literature rather than only internal convention.
      Full dataset (30,031 reactions) fetched and queried locally to
      select/verify these, not bundled in the repo -- only the individual
      cited routes (with citations) live in `tests/validation.rs`.
- [x] Known-route recovery: `every_literature_route_is_recovered_exactly`
      -- 5/5 cited routes recovered. `a_valid_alternative_precursor_is_...`
      confirms a genuinely valid alternative (BaO instead of BaCO3) is
      accepted *alongside* the cited route, not hidden by it. While
      building the fixtures, a decoy meant to be a non-competing filler
      (La2(CO3)3 for the LaAlO3 fixture) turned out, on actually running
      it, to form a third real valid alternative route -- the fixture's
      comment was corrected to say so rather than left describing
      behavior that wasn't actually checked.
- [x] Metamorphic tests (`tests/metamorphic.rs`, AGENTS.md §21.4): target
      element order, catalog insertion order, unrelated-precursor
      addition, and provider return order are all confirmed invariant
      end-to-end through `Planner` (not just at the unit level individual
      modules already covered). JSON field order is covered by
      `tests/json_roundtrip.rs` (existing). **Equivalent formula
      normalization is NOT invariant** -- see the stop-and-report entry
      below; `formula_unit_scale_is_not_currently_normalized_a_documented_gap`
      pins the current (non-invariant) behavior as a real regression
      check, not a TODO comment.
- [x] Provider failure tests (`tests/provider_failures.rs`, AGENTS.md
      §21.5's full list), all through `Planner::plan` end-to-end: timeout
      相当 (`ProviderError::Unavailable` with a timeout message -- no
      dedicated `Timeout` variant exists, and a timeout and an outage are
      the same fact from `Planner`'s side), missing entry, malformed
      record, partial thermodynamic coverage (mixed `Ok(Some)`/`Ok(None)`
      across two candidates in one report), duplicated evidence (no
      crash; **not** deduplicated -- documented, not silently fixed, since
      `evidence_strength`'s min-aggregation makes duplicates structurally
      inert already), inconsistent units (no dedicated test infrastructure
      exists for this because **gugen has no unit-consistency check at
      all** -- `thermodynamic_provider_has_no_unit_consistency_check_a_documented_gap`
      proves the gap directly: a provider returning an eV/atom-vs-kJ/mol
      scale error is accepted identically to a correct one), unavailable
      provider (both providers down at once, multi-candidate).
- [x] Adversarial examples (`tests/adversarial.rs`): an extreme (10^25)
      formula-unit target surfaces `GugenError::ArithmeticOverflow`
      cleanly through `Planner::plan` (confirmed empirically that 10^18
      still balances exactly and 10^25 does not, rather than guessing a
      threshold); a catalog covering no target element; a search budget
      too tight to evaluate every combination, through the full `Planner`
      (precursor.rs already covered `search_precursor_sets` alone); a
      precursor identical to the target (trivial 1:1 identity reaction);
      a target contradictory on *two* elements at once (existing
      `planner.rs` test only covers one); a target with an unclassified
      structure re-confirming `assess_applicability` never overclaims
      `InDomain` (regression pin for the Phase 6 advisor-caught overclaim
      bug).
- [x] False-confidence audit (`tests/validation.rs`,
      `every_recovered_plan_still_requires_manual_review_...`,
      `confidence_overall_is_measured_not_assumed_to_be_constant`):
      `manual_review_required` and the accompanying `Severe` warning are
      present on every plan across the whole fixture suite, no exceptions.
      See the stop-and-report entry below for the constancy finding this
      audit surfaced.
- [x] Reproducibility (`tests/validation.rs`,
      `planning_is_reproducible_across_repeated_runs`; also
      `docs/benchmark_report.md`'s own measurement): every fixture produces
      a byte-for-byte identical `SynthesisPlanningReport` (`PartialEq`)
      across repeated runs with the same input/timestamp.
- [x] Snapshot/golden tests (AGENTS.md §21.6): `tests/fixtures/
      batio3_report.json` and `.md`, checked in from a real run (fixed
      timestamp, not `now_rfc3339`), compared byte-for-byte in
      `src/bin/gugen.rs`'s own test module (`json_output_matches_...`,
      `markdown_output_matches_...`) -- `render_report_markdown` is
      private to the bin crate, so the test lives there rather than in
      `tests/`, per advisor guidance to avoid a snapshot-testing
      dependency neither format needs.
- [x] Benchmark report (`examples/benchmark_report.rs` ->
      `docs/benchmark_report.md`, AGENTS.md §22): every metric on §22's
      list is measured from a real run against the fixture set (not
      estimated) except two, both explicitly logged as skipped rather than
      silently omitted:
      - [ ] §23 differential validation against another implementation:
            not attempted. §23 says 可能なら ("if possible"); no runnable
            reference synthesis-planning implementation exists in this
            workspace, and building one only to compare against would
            itself need the same literature verification this phase
            already did, without a clear independent source of truth.
            Revisit if a real reference implementation becomes available.
      - [ ] §22 temperature-specific metrics (predicted-range-contains-
            reference rate, evidence-covered-condition coverage,
            unsupported-exact-value rate): undefined in v0.1, not zero --
            `TemperatureRange` is always `None` (no provider ever
            populates it), so there is no predicted temperature to score
            against anything. Revisit once a real condition-evidence
            provider exists.
- [x] `docs/competitors.md`'s Kononova entry and this file's Phase 0
      "open item" both updated to reflect the license verification above.

### Stop-and-report: equivalent-formula-unit-scale invariance (AGENTS.md §21.4/§28)

判明した事実 (facts found): `BaTiO3` and `Ba2Ti2O6` are the identical real
material at a different formula-unit scale. Planning for them today
produces different `plan_id`s and literally different reactant
coefficients (1,1,1,1 vs 2,2,1,2) -- confirmed by running both through
`Planner::plan` (`tests/metamorphic.rs`,
`formula_unit_scale_is_not_currently_normalized_a_documented_gap`), not
assumed.

なぜ問題か (why it's a problem): AGENTS.md §21.4 explicitly lists
"equivalent formula normalization" as a required invariance. It does not
hold. This traces to a genuine, load-bearing Phase 1 design choice
(`composition::tests::ordinary_decimal_amounts_round_trip_exactly`):
`Composition` preserves a caller's exact given amounts rather than
reducing to canonical GCD-minimal form, because doped/solid-solution
formulas (e.g. `La0.67Sr0.33MnO3`) need their exact decimal doping level
preserved -- blindly GCD-reducing every `Composition` at construction
would rescale those too (checked: for a decimal composition like that
one, "reducing" means scaling *up* to large integers, not down, since the
given amounts are already smaller than their GCD-reduced integer form).

最小解決案 (minimal fix): none identified that is actually minimal.
Reducing `Composition` at construction breaks the tested doped-formula
guarantee. A narrower fix confined to `balance()`/`derive_plan_id` (solve
internally against the target's own GCD-reduced ratio, scale back up to
match the caller's exact composition for the returned `BalancedReaction`)
is possible in principle but changes what `plan_id` and the reaction's
coefficients mean, which is a design decision, not a bug fix.

代替案 (alternatives): (a) do nothing, document the gap (current state);
(b) add a *separate*, decoupled "canonical scale" concept used only for
`plan_id` derivation, leaving `Composition`/`BalancedReaction` untouched;
(c) require callers to supply targets in minimal formula-unit form as an
input contract, enforced by a new validation check that would reject
`Ba2Ti2O6`-style targets outright.

推奨案 (recommendation): (a) for now. No calibration or usage data exists
to justify (b)'s specific design, and (c) would be a real, potentially
surprising input-validation behavior change with no evidence it's needed
-- no fixture or real target in this phase's suite ever supplies a
non-minimal formula-unit scale. Revisit if a real caller (Phase 9 CLI
users, or a future batch workload) actually hits this.

作業量 (effort): (b) is roughly a half-day change plus new tests across
`balance.rs`/`planner.rs`/`derive_plan_id`; (c) is smaller but has
input-validation UX implications worth a separate discussion.

影響範囲 (impact scope): `Composition`, `balance()`, `derive_plan_id`,
every `plan_id` value in every existing report -- touches the crate's
core identity/determinism story, not a local module.

安全に継続できる作業 (safe to continue): everything else in Phase 8 and
beyond. No curated fixture in this suite (or any real target seen so far)
supplies a non-minimal formula-unit scale, so this gap does not affect
the known-route recovery results, the benchmark numbers, or CLI usage as
currently exercised.

### Stop-and-report: `confidence.overall` is structurally constant (AGENTS.md §21/§28)

判明した事実 (facts found): across every valid plan produced by this
phase's entire fixture and adversarial suite (8 plans, 5 fixtures),
`confidence.overall` is `0.75`, always
(`confidence_overall_is_measured_not_assumed_to_be_constant`, also
`docs/benchmark_report.md`'s "false confident plan rate" line: 1 distinct
value observed). Root cause, in `score.rs`: `overall` averages four
Score01 dimensions, and `process_conditions` is always exactly `0.0` in
v0.1 (no provider ever resolves a condition), so for any plan with a
balanced reaction and non-empty evidence the average is
`(1 + 1 + 0 + 1) / 4 == 0.75` regardless of how genuinely different two
plans' real uncertainty is. Same root cause and same shape as Phase 5's
already-documented `total_ranking_score` finding
(`process_simplicity`-only discrimination).

なぜ問題か (why it's a problem): AGENTS.md §28 lists "validation
corpusでfalse confident plansが多い" as a trigger. Measured literally: all
of them. Whether this counts as *false* confidence is a real question --
each of the four sub-scores is individually honest (not fabricated,
correctly computed, and `process_conditions: 0.0` sits right next to
`overall: 0.75` in the same struct, which is exactly why §16 mandated
keeping these four separate rather than collapsing to one number) -- but
the constancy means `confidence.overall` currently cannot discriminate
between two plans of different real quality, which a reader could
reasonably expect it to do.

最小解決案 (minimal fix): none that isn't itself an unsourced heuristic.
The only way to make `overall` vary today would be to invent a
weighting/threshold not backed by any calibration data -- exactly what
AGENTS.md §27 forbids ("科学的根拠がないheuristicを追加しない").

代替案 (alternatives): (a) document only (current state); (b) remove
`overall` from the public schema until it can vary meaningfully; (c) add
a prominent field-level doc comment/warning already visible wherever
`overall` is displayed (partially done: `score.rs`'s doc comment already
states most of this).

推奨案 (recommendation): (a), matching Phase 5's resolution for the analogous
`total_ranking_score` finding. Removing a field a future condition-evidence
provider could legitimately make meaningful (b) would be premature.
Strengthened the doc comment on `ConfidenceAssessment`/`score_plan`
(see score.rs) so this is stated at the definition site, not only here.

作業量 (effort): documentation only, done as part of this phase.

影響範囲 (impact scope): every `confidence.overall` value gugen has ever
produced or will produce until a real condition-evidence provider exists
-- a reporting/expectations issue, not a code defect.

安全に継続できる作業 (safe to continue): everything. No plan claims
anything the four sub-scores don't individually support, and
`manual_review_required`/the mandatory `Severe` safety warning already
prevent `confidence.overall` from being read as a green light.

**Locally verified, all green:** fmt, clippy -D warnings (workspace, all
features), test under `--all-features` (60 lib + 10 bin + 4 json_roundtrip
+ 6 adversarial + 5 metamorphic + 7 provider_failures + 5 validation = 97
tests), `--no-default-features` and `--no-default-features --features
mikiwame` (both green, same counts minus the bin/mikiwame-gated tests as
appropriate), doc -D warnings, both examples (`balance_batio3`,
`benchmark_report`), `cargo build --features serde,clap --bin gugen`,
`cargo check --target wasm32-unknown-unknown` (with and without
`mikiwame`), cargo audit (0 vulnerabilities, 32 crates, no new
dependencies this phase).

**Two stop-and-report items filed above**, both resolved as "document,
don't silently fix" per AGENTS.md §27's unsourced-heuristic rule --
consistent with how Phase 5 handled the analogous `total_ranking_score`
finding. Neither blocks continuing; both are logged for the record and
raised to the user explicitly, per §28's intent that this format surface
findings for judgment, not necessarily halt work.

## Phase 9 — v0.1 Release Preparation — DONE (2026-08-14)

AGENTS.md §26 lists this phase's checklist as: README実例を実出力と同期,
README_ja, changelog, docs.rs, package内容確認, dependency/license audit,
schema audit, semver audit, release checklist. Each is addressed below with
what was actually run, not assumed.

**README実例を実出力と同期 / README_ja.** Ran every documented command
against its documented input file and diffed the real output against the
README text, rather than re-reading the existing prose and trusting it:

- `gugen balance reaction.json`'s JSON output block was wrong in both
  READMEs — hand-formatted as condensed single-line objects
  (`{ "composition": {...}, "coefficient": 1 }`), but the CLI actually
  renders via `serde_json::to_string_pretty`, which puts every field on its
  own line. This had been wrong since Phase 7 introduced the CLI; nobody
  had diffed the README against a real run since. Fixed in both files.
- `examples/balance_batio3.rs`'s output line was already correct (verified
  again).
- The `gugen plan`/`explain`/`validate-target`/`doctor`/`batch` subcommand
  usage lines were checked against `gugen <subcommand> --help`'s real
  `clap`-generated usage strings — all match.
- Added a new worked-example section to both READMEs: a full `gugen plan
  --format markdown` run for BaTiO3 (BaCO3 + TiO2), using the same target/
  catalog as the golden snapshot fixture. The embedded output (title,
  target, applicability, plan header, steps, evidence, warnings) is a
  byte-for-byte copy of a real run's stdout, verified two ways: (1)
  extracting the same line ranges from a fresh `gugen plan` invocation and
  diffing against what's embedded in the README (only the intentionally-
  omitted "Score breakdown" heading differs); (2) diffing the *entire* real
  run's output against `tests/fixtures/batio3_report.md` directly (not
  just the excerpted lines) — the only difference anywhere in the full
  file is the trailing `_Generated <timestamp>..._` line, since the golden
  fixture pins a fixed timestamp and a live run reads the real clock.
  Confirms the README's claim that the embedded excerpt "is also
  `tests/fixtures/batio3_report.md`'s golden snapshot" holds for the whole
  document, not just the lines shown. Added at the user's request to give
  the library's actual value proposition — evidence-linked, inspectable plans,
  not a black-box score — visibility to a materials-science reader skimming
  the README, rather than leaving it undemonstrated behind a subcommand
  list. The `docs/benchmark_report.md` link already covered the
  aggregate/quantitative side; this covers the qualitative "what does one
  plan actually look like" side.

**changelog.** Reviewed for accuracy against the current code (spot-checked
several claims, not just read for typos). `[Unreleased]` stays
`[Unreleased]` — AGENTS.md §29 lists "merge/publishしていない" as a v0.1
*completion* criterion, so this phase prepares for release without cutting
one. Added a Phase 9 `### Added` entry and two `### Fixed` entries (the
README JSON mismatch above, and the missing LICENSE files below).

**docs.rs.** `default = []`, and most of the crate's real surface (every
`serde` impl, the entire `mikiwame_adapter` module) is behind non-default
features. Without `[package.metadata.docs.rs]`, docs.rs would have built
gugen's documentation with zero features enabled — a nearly bare crate.
Added `all-features = true` under that key. Re-verified
`RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps`
stays clean.

**package内容確認.** `cargo package --list` review found two real gaps:

1. **`LICENSE-APACHE`/`LICENSE-MIT` did not exist.** `Cargo.toml` declares
   `license = "MIT OR Apache-2.0"`, both READMEs link `LICENSE-APACHE` and
   `LICENSE-MIT`, and AGENTS.md's own crate-layout diagram (§ describing
   Phase 1's `crate初期化`) lists both files — but nobody had ever created
   them. Added both: `LICENSE-APACHE` is `rust-lang/rust`'s unmodified
   Apache-2.0 text (the terms only, no filled-in copyright appendix — the
   same convention `rust-lang/rust` itself uses); `LICENSE-MIT` is the
   exact text `dtolnay`'s crates (`thiserror`, `syn`, `quote`,
   `proc-macro2` — already gugen's own dependencies) ship, which omits the
   optional copyright-holder line. Chose these specific texts by fetching
   them from their real source repos rather than reconstructing the
   templates from memory, and matched gugen's own dependency tree's
   convention rather than picking an arbitrary alternative.
2. `cargo package --list --allow-dirty` printed `warning: manifest has no
   documentation, homepage or repository` — fixed by adding `repository`
   and `documentation` to `Cargo.toml` (see docs.rs, above, for the
   `[package.metadata.docs.rs]` addition made in the same pass).

Everything else in the package listing (`AGENTS.md`, `tasks/todo.md`,
`.github/workflows/ci.yml`) is deliberately public — both READMEs already
link `tasks/todo.md` for phase-by-phase status, so shipping it in the
package is consistent, not incidental inclusion.

**dependency/license audit.** Queried the crates.io API for the license
field of all 30 locked dependencies (`Cargo.lock`), at their exact locked
versions, not the latest version or a name-recognition assumption. All are
`MIT`, `Apache-2.0`, or `MIT OR Apache-2.0` — compatible with gugen's own
`MIT OR Apache-2.0` — except:

- `memchr` 2.8.3: `Unlicense OR MIT` (compatible; MIT arm satisfies).
- `strsim` 0.11.1: `MIT` only (compatible).
- `unicode-ident` 1.0.24: `(MIT OR Apache-2.0) AND Unicode-3.0` — the
  `Unicode-3.0` term covers embedded Unicode Consortium data tables. This
  is a standard, widely-audited combination present in nearly every Rust
  dependency tree that touches `proc-macro2`/`syn` (which gugen's `clap`
  and `thiserror` derive macros already pull in); not a new or unusual
  obligation introduced by gugen's own choices.

No copyleft (GPL/LGPL/AGPL) dependencies anywhere in the tree. `cargo audit`
re-run clean (no known security advisories).

**schema audit.** `SCHEMA_VERSION: u32 = 1` (report.rs), embedded on every
`SynthesisPlanningReport`, exercised by `tests/json_roundtrip.rs` alongside
negative-amount, inverted-temperature-range, and duplicate-JSON-key
rejection tests, and surfaced directly by `gugen doctor`. No schema change
in this phase, so the version number is unchanged.

**semver audit.** `cargo semver-checks check-release` was actually run
(not assumed inapplicable) — it needs a published-crate baseline, and
`gugen` has never been published, so it fails with "gugen not found in
registry" rather than producing a meaningful comparison. Confirms rather
than assumes there is no semver history yet to check. In its place, the
public API surface (`src/lib.rs`'s `pub use` list) was reviewed by hand:
every module is declared as private `mod` (never `pub mod`), so the only
public API is the deliberately curated re-export list — nothing internal
is accidentally reachable via `gugen::some_module::SomeType`. Version
stays `0.1.0`.

**release checklist (AGENTS.md §29).** Walked every line item; each was
verified this phase, not assumed carried over from earlier phases:

- [x] Rust libraryとして公開APIがある — `src/lib.rs`'s `pub use` list.
- [x] target compositionを受け取れる — `TargetSpecification`.
- [x] precursor catalogから候補生成できる — `search_precursor_sets`.
- [x] reaction equationを厳密にbalanceできる — `balance()`, exact-rational.
- [x] 複数候補planを生成できる — `Planner::plan`.
- [x] conventional solid-state routeを表現できる —
      `conventional_solid_state_template`.
- [x] process stepsが機械可読 — `ProcessStep` (serde-able under `serde`).
- [x] unknown conditionsをunresolvedとして保持できる — `StepRequirement::
      Unresolved`, `UnresolvedRequirement`.
- [x] evidenceとassumptionを分離できる — `PlanningEvidence` vs
      `PlanningAssumption`, separate fields on `SynthesisPlan`.
- [x] ranking breakdownを返せる — `PlanScoreBreakdown` (seven named
      sub-scores, not one collapsed number).
- [x] scoreを成功確率と表現していない — stated explicitly in both READMEs
      and every relevant doc comment.
- [x] confidenceとapplicabilityが分離されている — `ConfidenceAssessment`
      vs `ApplicabilityAssessment`, distinct types.
- [x] rejected candidateの理由を返せる — `RejectedCandidate` +
      `RejectionCode`.
- [x] safety warningがある — mandatory `Severe` warning alongside every
      plan (score.rs).
- [x] manual review requirementがある — `manual_review_required: bool`,
      always `true` in v0.1.
- [x] providerが交換可能 — `PrecursorCatalog`/`ThermodynamicProvider`/
      `ProcessEvidenceProvider` traits.
- [x] JSON schema versionがある — `SCHEMA_VERSION` (schema audit, above).
- [x] provenanceがある — `PlanningProvenance`.
- [x] deterministic — no system-clock or RNG read inside the library core
      (`execution_timestamp` is caller-supplied); `gugen doctor` reports
      "deterministic mode: yes"; Phase 8's `planning_is_reproducible_
      across_repeated_runs` proves it empirically.
- [x] batch APIとCLIがある — `gugen batch`, per-target failure isolation.
- [x] known-route validationがある — Phase 8's `tests/validation.rs`
      (5/5 literature-cited routes recovered exactly).
- [x] false-confidence auditがある — Phase 8's `every_recovered_plan_
      still_requires_manual_review...` plus the benchmark report's false-
      confident-plan-rate metric.
- [x] out-of-domain inputを棄却できる — `tests/adversarial.rs`'s
      out-of-domain/contradictory-target cases; benchmark report's
      abstention-rate metric.
- [x] chematic-crystal連携境界がある — `TargetMaterialView` trait.
- [x] mikiwame連携がoptional — feature-gated `mikiwame_adapter`, off by
      default.
- [x] README例が実出力と一致 — fixed this phase (see above); re-verified
      by direct diff, not re-reading.
- [x] fmt/clippy/test/doc/auditが通る — re-run after every change this
      phase, across `--all-features`, `--no-default-features`, and
      `--no-default-features --features mikiwame`; both `wasm32-unknown-
      unknown` checks (with/without `mikiwame`); `cargo audit`; all green.
- [x] draft PRがある — verified live via `gh pr view 1`:
      `isDraft: true`, `state: OPEN`,
      https://github.com/kent-tokyo/gugen/pull/1.
- [x] merge/publishしていない — verified: PR state is `OPEN` (not merged);
      `curl https://crates.io/api/v1/crates/gugen` returns
      `{"errors":[{"detail":"crate \`gugen\` does not exist"}]}` — checked
      directly, not inferred from the earlier `cargo semver-checks`
      registry-index error (that error is about the semver-checks baseline
      lookup, not a deliberate publish check, and isn't cited as one here).
- [x] working treeがclean — verified via `git status` before each commit
      this phase, same as every prior phase.

All 29 items verified true as of 2026-08-14. **This is a v0.1 candidate
per AGENTS.md §29's definition** — but per §26 ("所有者の明示的許可なく
publishしないでください") and §29's own "merge/publishしていない"
criterion, reaching candidate status is not itself permission to merge or
publish; that remains the owner's explicit call.

**GitHub repo metadata** (outside the git diff, done at the user's
request): repo description and topics were empty (`gh repo view` returned
`description: ""`, `repositoryTopics: null`). Set via `gh repo edit` to
match the new `Cargo.toml` keywords/categories for consistency:
description "Explainable materials synthesis and process planning, in
Rust.", topics `rust`, `chemistry`, `materials-science`, `synthesis`,
`planning`. `cheminformatics` was considered and deliberately left out —
it conventionally denotes molecular-level tooling (SMILES, QSAR, drug
discovery), and gugen explicitly positions itself against `renkin`
(molecular retrosynthesis) as the non-molecular, inorganic-materials
sibling; tagging it with a molecular-chemistry keyword risks
miscategorizing it for exactly the audience trying to distinguish the two.

## Phase 10 — Literature Condition Provider — DONE (2026-08-14)

Not part of AGENTS.md §26's original 9-phase roadmap. Follow-up work
toward v0.2.0, from an external competitive evaluation of v0.1.0 (see the
plan file `/Users/k_tanabe/.claude/plans/typed-sparking-storm.md`,
approved by the owner). Goal: close the evaluation's single largest
scored gap ("合成計画の機能幅") by resolving real, cited process
conditions instead of leaving every `Heat`/`Grind`/`Form` step's
temperature/duration/atmosphere/ramp field `None` unconditionally.

**Preamble**: re-verified `chematic-crystal`'s publish status (same
method as every prior check) — still does not exist on crates.io.
`mikiwame` confirmed still at `0.1.0`. No action; out of scope for this
plan as agreed.

**Type changes** (`src/process.rs`): `ProcessPrecedent` gains
`conditions: Vec<ConditionPrecedent>` (breaking change, accepted per the
owner's confirmed single-0.2.0-bump decision). New `ConditionPrecedent`
struct carries `purpose: HeatingPurpose`, four `Option` condition fields
reusing the existing validated range types, and its own
`evidence_kind`/`source_id`/`statement`/`strength`/`applicable_to` — set
by whichever provider returns it, not assumed by the planner (a
`ProcessEvidenceProvider` is also the trait a user-supplied lab-precedent
source implements, per AGENTS.md §7's `UserProvidedPrecedent`, so
hardcoding `CuratedLiteratureRecord` in the planner would mislabel
provenance for any other implementation).

**New provider** (`src/literature_conditions.rs`):
`InMemoryLiteratureConditionProvider`, backed by
`CuratedConditionRecord`s matched by exact `Composition` equality against
a target, with `EvidenceScope::ExactTarget` when the queried precursor-ID
set also matches a record's exactly, `SimilarMaterial` otherwise. A unit
test (`curated_records_have_no_duplicate_target_precursor_purpose_keys`)
enforces no two records ever claim the same
`(target, precursor_ids, purpose)` key, removing order-dependent
resolution by construction rather than runtime merge logic — backed by a
new `tests/metamorphic.rs` case
(`provider_return_order_does_not_affect_resolved_step_conditions`) that
shuffles a test double's returned precedents and asserts identical
resolved steps either order (AGENTS.md §21.4).

**Wiring** (`src/process.rs`/`src/planner.rs`): new
`apply_condition_precedents(steps, precedents) -> Vec<PlanningEvidence>`
splices resolved conditions into a plan's `Heat` steps — only ever fills
an already-`None` field, never overwrites one some other source already
set, so this composes with any future resolution source. Called from
`Planner::plan` right after `process_evidence_provider.precedents(...)`,
before `score_plan` runs (the mutation has to land before scoring, not
after — `score.rs`'s `resolved_condition_fraction` already had the right
shape to react to this, it was just never fed any resolved data before
Phase 10). New `Planner::with_process_evidence_provider` constructor
(catalog + process-evidence provider, no thermodynamic provider — the one
new two-provider combination this phase needs).

**Real bug found and fixed while wiring this in** (not anticipated at
plan-writing time, found by actually tracing what "provider consulted but
partially resolves" means for existing code): `score.rs`'s
`collect_unresolved` had a single hardcoded `NO_PROVIDER_REASON` constant
applied to every unresolved field regardless of whether a provider
existed. Once a provider is wired in and resolves *some* fields (e.g.
temperature) but not others (e.g. ramp rate), the old text ("no ...
provider is wired in yet") becomes a false machine-readable claim about
the still-unresolved field. Fixed by threading a
`process_evidence_provider_consulted: bool` through `collect_unresolved`
and `score_plan` (both signatures changed; all 9 call sites across
`src/score.rs`'s own tests, `tests/json_roundtrip.rs`, and
`src/planner.rs` updated), branching the reason text so
`Planner::offline_minimal`'s output stays byte-identical (`false`) while
a consulted-but-unmatched field gets accurate text. Proven by a dedicated
test (`a_target_with_no_curated_coverage_still_leaves_every_condition_
unresolved`, `tests/literature_conditions.rs`) that explicitly asserts
the reason text *must* differ between an offline and a provider-consulted
report for the same uncovered target, rather than assuming it wouldn't.

**Curated dataset — real research, not fabrication (AGENTS.md §21.3)**:
5 records, one per target `tests/validation.rs`'s Phase 8 fixtures
already cite (LaAlO3, MgAl2O4, Zn3(PO4)2, CaO, BaTiO3). Two research
passes (an initial pass against the exact representative DOIs
`tests/validation.rs` already cites, then a follow-up once that pass
found real problems):

- **MgAl2O4** and **CaO**: the existing representative DOIs
  (10.1007/s11663-014-0207-8, 10.3390/ma17153875) turned out to be real,
  accessible papers that also report the actual firing conditions, not
  just the precursor route — read directly (PMC/institutional open-access
  copies), condition data extracted from the same citation already in
  place.
- **LaAlO3**: the existing representative DOI (10.1149/2.053405jes) is
  the right paper but fully paywalled with no accessible copy found
  anywhere (checked Unpaywall, Semantic Scholar, IISc ePrints,
  ResearchGate, CORE.ac.uk, Wayback Machine). Substituted a different,
  freely-accessible paper (DOI 10.1039/d3ra03241h, RSC Advances, open
  access) reporting the same La2O3 + Al2O3 route.
- **Zn3(PO4)2** and **BaTiO3**: the existing representative DOIs
  (10.1016/j.jmmm.2015.06.001, 10.1111/j.1551-2916.2006.01172.x) are
  **confirmed topic mismatches** on inspection — not access problems.
  The Zn3(PO4)2 DOI is a Sm-doped zinc phosphate *glass* paper made by
  melt-quenching, not this reaction at all. The BaTiO3 DOI is a
  NaNbO3-BaTiO3 solid-solution ceramic study, not plain BaTiO3. This is
  worth stating plainly: the Kononova et al. 2019 text-mining pipeline
  (or its downstream citation in this repo) attributed these routes to
  papers that, read directly, don't actually report them. Does not
  invalidate `tests/validation.rs`'s own claims (which only assert a
  precursor set was recovered by gugen's search, not that a specific DOI
  was independently verified for condition data) — but it does mean
  neither DOI could be used as a condition-data source under its own
  name. Substituted different, freely-accessible, independently verified
  papers for condition data specifically (DOI 10.3390/engproc2024067018
  for Zn3(PO4)2; DOI 10.3390/cryst14040304 for BaTiO3), left documented
  in `src/literature_conditions.rs`'s own doc comment rather than
  silently swapped without a trace.
- Zn3(PO4)2's substitute source used a different real precursor
  combination than the existing fixture (ZnO + (NH4)2HPO4, not ZnO +
  P2O5) — recorded as the route the source actually used, not force-fit
  to match. This exercises the `SimilarMaterial`-vs-`ExactTarget` scoping
  logic for real (proven by
  `zn3po42_from_a_different_precursor_route_resolves_as_similar_material_
  not_exact_target`, not just asserted to work in theory).
- BaTiO3's sintering condition is recorded as a genuine range (1200-1350
  C) since the source paper reports two parallel samples at those two
  temperatures for comparison, not one recommended value — using
  `TemperatureRange`'s existing min/max shape honestly rather than
  picking one number arbitrarily. Its sintering duration is not stated in
  the source and stays `None`, not filled in from a corroborating
  (but methodologically different, sonochemically-activated) secondary
  source found during research.
- Zn3(PO4)2's substitute source is flagged internally as a short (~3
  page) conference-proceedings-tier paper with an internally
  inconsistent reported space group (Pnma, the hydrate family, versus the
  known anhydrous polymorphs) — reflected as `EvidenceStrength::Weak`
  rather than `Moderate`, not silently treated as equally reliable as the
  other four records.

**Tests** (`tests/literature_conditions.rs`, new; `tests/metamorphic.rs`,
extended; `src/process.rs`, extended): every one of the 5 curated
records' resolution is checked end to end through
`Planner::with_process_evidence_provider` (exact temperature/duration
values, real DOI in the resulting evidence, the `SimilarMaterial` scoping
case, the range-not-point case, the missing-duration-stays-unresolved
case), plus a targeted unit test for `apply_condition_precedents` itself
(only fills matching unset fields, ignores a precedent with no matching
step), plus the provider-return-order metamorphic case. 10 new tests
total.

**Explicit non-goals, stated up front and held to**:
`Planner::offline_minimal` and everything built on it (every Phase 8
fixture, the golden JSON/markdown snapshots, the README's worked example)
are unchanged — new capability is opt-in via the new constructor only.
`evidence_strength`'s plan-level aggregate stays pinned at `0.25`
regardless of condition resolution (weakest-link aggregation, template
always attaches a `Weak` entry) — not "fixed" with no calibration data to
justify a different aggregation rule (AGENTS.md §27).

**Docs updated**: `src/score.rs`'s `PlanScoreBreakdown`/
`ConfidenceAssessment` doc comments (no longer blanket "always" claims —
scoped to "when no provider resolves a condition"); `docs/evidence_
model.md` (`CuratedLiteratureRecord` now used); `docs/scientific_scope.md`
guardrail 1 (names the new provider as the first real satisfier);
`docs/benchmark_report.md`/`examples/benchmark_report.rs` (regenerated,
confirmed byte-identical apart from the one updated sentence — the
benchmark's own fixtures are `offline_minimal`-based and genuinely
unaffected); `CHANGELOG.md` (`[Unreleased]` entry); `README.md`/
`README_ja.md` status banners (also corrected two facts that had gone
stale since Phase 9 wrote them: "not published, not merged to main" was
no longer true once v0.1.0 actually shipped, and the draft-PR link was
dead since PR #1 is merged — real staleness caught by reading the
current banner against reality, not assumed still accurate);
`src/lib.rs`'s crate doc comment (same correction, plus notes Phase 10
onward is tracked from `tasks/todo.md`, not `AGENTS.md` §26, which only
defines the original 9 phases).

**Locally verified, all green**: `cargo fmt --all -- --check`, `cargo
clippy --workspace --all-targets --all-features -- -D warnings`, `cargo
test --workspace --all-features` (107 tests: 63 lib + 10 bin + 6
adversarial + 4 json_roundtrip + 6 literature_conditions + 6 metamorphic
+ 7 provider_failures + 5 validation), `cargo test --workspace
--no-default-features` and `--no-default-features --features mikiwame`
(both green, same counts minus the all-features-only bin/json_roundtrip
suites), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features
--no-deps`, `cargo build --features serde,clap --bin gugen`, both
`wasm32-unknown-unknown` checks, `cargo audit` (0 vulnerabilities),
`examples/balance_batio3`/`examples/benchmark_report` re-run and diffed
against checked-in output.

## Phase 11 — Large-Scale Blind Benchmark — DONE (2026-08-14)

**Goal** (per `/Users/k_tanabe/.claude/plans/typed-sparking-storm.md`):
close the "検証・科学的信頼性" competitive-evaluation gap with a real,
much larger holdout measurement, kept strictly separate from Phase 8's/
Phase 10's curated data, using the same licensed Kononova et al. 2019
corpus at a different trust tier (mined/filtered, not hand-verified).

### §28 report — wrong-provenance corpus, caught before commit

判明した事実 (facts found): while developing `benchmarks/fetch_kononova.py`
against a cached local copy (`--local`, added purely for dev-iteration
speed against the 91MB live download), the script was written assuming
the dataset's JSON top level is `{"reactions": [...], "release_date":
...}` with 30,031 entries — matching that cached copy exactly. Before
committing, the plan's own verification discipline called for running the
*live* (non-`--local`) path at least once; doing so failed immediately
with a `TypeError`, because the actual dataset hosted at the officially-
cited figshare DOI (9722159, `solid-state_dataset_2019-06-27_upd.json`,
the exact file Phase 8 license-checked as CC BY 4.0 via the figshare API)
is a **bare JSON list of 19,488 reactions**, not a dict, and not 30,031
entries. Tracing the cached file's origin through this session's own tool
history found it: an earlier session downloaded it not from figshare at
all, but from `https://raw.githubusercontent.com/CederGroupHub/
text-mined-synthesis_public/master/solid-state_dataset_2019-12-03.json.xz`
— the same research group's GitHub repo, but a **different, later
snapshot** (2019-12-03 vs. figshare's 2019-06-27 release) with a
different top-level shape and 30,031 records. `gh api repos/
CederGroupHub/text-mined-synthesis_public` reports `"license": null` —
this repo has no detected license at all. Phase 8's CC BY 4.0 verification
was real and correct, but for a *different file* than the one actually
used to derive Phase 8's citation counts, Phase 10's coverage
percentages, and (until this fix) Phase 11's entire corpus.

Recounting `tests/validation.rs`'s specific DOI-attestation claims
directly against the correct, licensed 19,488-reaction corpus:

| Route | `tests/validation.rs` claims | Correct corpus |
|---|---|---|
| LaAlO3 (La2O3+Al2O3) | 19 independent DOIs | 10 |
| MgAl2O4 (MgO+Al2O3) | 20 independent DOIs | 16 |
| Zn3(PO4)2 (ZnO+P2O5) | 2 independent DOIs | **0** |
| BaTiO3 (BaCO3+TiO2) | 88 independent DOIs | 83 |
| (dataset size) | "the full 30,031-reaction dataset" | 19,488 |
| (binary oxides) | "zero simple-binary-oxide-target entries" | still zero — holds |

Every representative DOI Phase 8 actually cites (`10.1149/2.053405jes`,
`10.1007/s11663-014-0207-8`, `10.1111/j.1551-2916.2006.01172.x`) is
present in the correct corpus and still reports the same route — the
fixtures are not fabricated, only the attestation *counts* are wrong.
Zn3(PO4)2 is the one qualitative discrepancy: its representative DOI
(`10.1016/j.jmmm.2015.06.001`) is present in the correct corpus exactly
once, reporting target `Sm2NixZn40P116-2*xO333-4*x` — a complex doped
formula, not `Zn3(PO4)2`/`Zn3P2O8` at all. The wrong-provenance (December
2019) file's re-run of the same text-mining pipeline against the same
paper produced a *different*, oversimplified extraction (`Zn3P2O8`) that
does not appear in the licensed data. This independently corroborates
Phase 10's separate finding (`src/literature_conditions.rs`'s doc
comment) that this DOI's actual paper is a Sm-doped zinc-phosphate glass
study, not a `Zn3(PO4)2` synthesis — Phase 10 already worked around it
with a different, correctly-verified source for condition data, but
Phase 8's *original* fixture (used for precursor-set recovery, a
different claim) still cites the DOI for a route the licensed corpus does
not actually attest to at all.

Phase 10's `tasks/todo.md` entry states ~80%/70%/44% temperature/time/
atmosphere coverage figures measured against "the already-licensed
Kononova dataset" — these were also measured against the wrong-provenance
file (same cached copy) and have not been recounted against the correct
one as part of this report.

なぜ問題か (why this matters): a merged, published crate (`gugen` v0.1.0
on crates.io) contains a test file (`tests/validation.rs`) with citation
counts that do not match the corpus they claim to summarize, and a
dataset-size claim ("30,031-reaction dataset") that is factually wrong.
This is exactly the class of unsourced/unverified numeric claim AGENTS.md
§21.3/§27 exist to prevent — the individual fixtures were genuinely
verified against real papers, but the *aggregate* framing around them
was not independently re-checked against the dataset it names, and
turned out to rest on an unlicensed, differently-processed file.

最小解決案 (minimal fix): what this phase already did — fix
`fetch_kononova.py` to use the correct dataset shape and count
(19,488), rebuild Phase 11's own corpus and every downstream artifact
from it, and document the finding without silently correcting
`tests/validation.rs`'s or Phase 10's already-published numbers in the
same commit.

代替案 (alternatives): (a) also rewrite `tests/validation.rs`'s citation
counts and module doc in this same phase; rejected — deciding what the
Zn3(PO4)2 fixture should even claim, given its cited route has zero
attestation in the licensed corpus, is a fixture-design question that
belongs to whoever owns Phase 8's fixtures, not a number swap to make
unilaterally while fixing an unrelated phase's corpus script. (b) attempt
to independently verify whether the GitHub repo's data is *also* covered
by the paper's CC BY 4.0 grant despite no repo-level license file (e.g.
by contacting the maintainers, `cedergroup-ml-team@lbl.gov`, per the repo
README) and keep using the larger December 2019 snapshot if so; rejected
for this phase — slow, uncertain, and unnecessary, since the officially-
cited figshare file is already verified, sufficient, and the paper's own
canonical distribution point.

推奨案 (recommendation): ship Phase 11 against the correct, verified
corpus (done); recommend a small, clearly-labeled follow-up correcting
`tests/validation.rs`'s citation text and module doc comment, owned as
its own decision rather than bundled here.

作業量 (effort): this fix, ~1 hour (script correction, corpus rebuild,
report regeneration, this write-up). A `tests/validation.rs` correction
would be small in size but needs a real decision about the Zn3(PO4)2
fixture's framing, not just a number edit.

影響範囲 (impact): `tests/validation.rs`'s citation text (5 DOI-count
claims, 1 dataset-size claim) and module doc comment; Phase 10's
`tasks/todo.md` entry's coverage percentages (unverified against the
correct corpus, not recounted here). Phase 11's own corpus, tests, and
report are unaffected by this report — they were rebuilt against the
correct data before this commit.

安全に継続できる作業 (safe to continue): Phase 11 exactly as rebuilt in
this commit. Phases 12/13 are unaffected (neither touches this corpus).

---

**Corpus.** New `benchmarks/` directory (AGENTS.md §23: comparison/corpus
tooling isolated from the crate's own production dependency tree — no new
Cargo dependency, Python is dev-only tooling).
`benchmarks/fetch_kononova.py`: fetches the same figshare-hosted dataset
Phase 8 licensed-checked (re-verifies `license.name == "CC BY 4.0"` live
against the figshare API on *every* run, not cached from a prior check),
also asserts the reaction count is the expected 19,488 (a real dataset
change would fail loudly, not silently reshape the sample — this exact
assertion, run for real rather than trusted, is what caught the §28
finding above). Supports `--local <path>` for dev iteration against an
already-downloaded copy (the license check still runs live even with
`--local`); its docstring now states plainly that the local copy must be
byte-identical to this script's own download, after the §28 finding
showed what happens when that invariant is silently violated.

Filter, applied before ever looking at gugen's results on this data
(AGENTS.md §27): target and every precursor must be a plain, non-doped,
single-composition-entry formula with positive numeric element amounts
(`Composition::new`-representable); 1-4 distinct precursors after
dedup (gugen's own `SearchBudget::default().max_precursors_per_plan`);
and — the leakage-prevention mechanism — excludes any `(target,
precursor-set)` pair matched by **normalized elemental ratio**, not DOI
or formula string, against the 6 routes already used by
`tests/validation.rs`'s 5 fixtures and Phase 10's curated records
(Zn3(PO4)2 appears twice: the validation.rs route and Phase 10's
different substitute route). DOI-only exclusion was considered and
rejected: several of these routes are independently reported by dozens of
DOIs in this same corpus (LaAlO3: 10, MgAl2O4: 16, BaTiO3: 83, recounted
directly against the correct corpus — see the §28 report above for why
these differ from `tests/validation.rs`'s own text) — excluding only the
one "representative" DOI per route would have left near-duplicate leaked
entries in the holdout set. The Zn3(PO4)2/ZnO+P2O5 exclusion entry
matches zero rows in the correct corpus (that route isn't attested at
all, per the §28 report) — kept anyway, since excluding a route that
happens not to appear is harmless and documents the intent. Ratio
normalization also correctly matches a route reported at a different
formula-unit scale than gugen's own fixtures happen to use (gugen's own
`Composition::PartialEq` is exact-scale only, a documented ROADMAP.md
gap — the benchmark's own exclusion logic is deliberately more robust
than that).

Result: 19,488 raw reactions -> 9,081 unparseable target, 954
unparseable precursor, 408 zero-or-too-many-precursors, 109 leakage
exclusions -> 8,936 eligible -> deterministic seeded (20260814)
downsample to 1,500, written to
`benchmarks/data/kononova_sample.jsonl` (473KB, checked file size before
committing per the plan's explicit ask — well within a reasonable repo
addition) and `benchmarks/data/ATTRIBUTION.md` (citation, license,
exclusion-count breakdown, both regenerated by re-running the script, not
hand-edited).

**Tests** (`tests/large_scale_benchmark.rs`, cheap, part of `cargo test`):
corpus loads at the expected row count; zero holdout rows exactly match
an already-curated route (Rust-side exact-`Composition`-equality
re-check, a lower-fidelity second guard alongside Python's authoritative
ratio-based one); every row plans without panicking or returning `Err`
(AGENTS.md §25) via `Planner::offline_minimal`, with a <5%-unparseable
budget as a regression guard on the Python filter itself.

**Full metrics** (`examples/large_scale_benchmark.rs` ->
`docs/large_scale_benchmark_report.md`, regenerated and diffed
byte-identical before commit): a fixed decoy pool (the 60 most frequent
precursor formulas *in this sample itself*, not hand-picked) is added to
every row's catalog, filtered to share >=1 element with that row's target
and capped at 8 per row — without this, "recovery" would be nearly
vacuous for the many rows with only one true precursor. The cap (8) was
chosen by directly measuring `RejectionCode::SearchBudgetExhausted`
against this sample rather than assumed; measured result: 3/1500 rows
exhausted `SearchBudget::default()`'s `max_precursor_sets: 10_000`,
confirming 8 is safely below the point where decoy-driven combinatorics
would start masking genuine chemistry rejections.

**Real finding, corrected mid-implementation rather than left wrong in
the checked-in report**: the plan document predicted
`RejectionCode::UnsupportedByproductRequired` would dominate rejections
at this scale. The first real run showed `MissingTargetElement`
dominating instead, at 95.4% of all 758,233 rejected candidates versus
`UnsupportedByproductRequired`'s 4.4% — the opposite of what was
written into the report's first draft before actually running it.
Investigated rather than silently reworded: `src/precursor.rs`'s
`search_precursor_sets` checks element coverage *before* any byproduct/
balance check, short-circuiting with `continue` on failure (confirmed by
reading the function directly, not assumed) — so most of the many small
decoy-augmented combinations get rejected for not covering the target's
exact elements before a byproduct check is ever reached for them. This
is a mechanical artifact of decoy augmentation, not a chemistry finding.
The originally-expected finding *does* hold once this noise is factored
out: among the 36,887 combinations that passed the element-coverage gate
(byproduct + no-balance + duplicate + accepted-and-kept),
`UnsupportedByproductRequired` accounts for 90.2% of what's left — the
byproduct allow-list (`CO2`/`H2O`/`O2` only) genuinely is the dominant
blocker once combinatorial noise is removed, exactly as the plan
anticipated, just not visible in the naive "share of all rejections"
framing. Both the naive and coverage-conditional figures are reported
in `docs/large_scale_benchmark_report.md`, not just the flattering one.
Not widened reactively either way (AGENTS.md §27).

**Other measured findings**: 1154/1500 rows produced >=1 plan; 1036/1500
recovered the cited route exactly; 263/1500 found a genuine additional
valid alternative route via the decoy pool; element-balance exactness
1659/1659 (100%, re-verified against each plan's own reaction, not
assumed); condition-resolution coverage split by Phase 10 overlap: 8/8
plans resolved among the 6 holdout rows whose target matches one of
Phase 10's 5 curated targets via a *different* precursor route (correctly
scoped `EvidenceScope::SimilarMaterial`, not `ExactTarget` — proof that
Phase 10's coverage generalizes across routes to a *known* target, not
evidence of unseen-target prediction), versus 0/1651 among the remaining
1494 rows; deterministic reproducibility confirmed by replanning the
entire 1500-row sample twice and comparing full report equality; planning
throughput stayed under 10ms/call even with decoy-augmented catalogs.

**Non-goals, explicit**: no widening of the curated byproduct allow-list
in response to this benchmark's own rejection numbers (AGENTS.md §27 —
would need independent literature grounding per species, not benchmark
pressure); no temperature MAE against this corpus's own reported
conditions (deliberately not embedded in the JSONL at all, to remove the
temptation — gugen still has no predicted temperature for any of these
targets, so there is nothing non-vacuous to score); no re-sampling or
seed-tuning after seeing gugen's results on the first draw.

**One §28-adjacent condition was found and investigated, not silently
worked around** — see the report at the top of this section. No other
stop-and-report condition triggered: no package-name collision, no
unresolved API dependency, no public schema breaking change (both new
files are additive-only: `examples/large_scale_benchmark.rs` and
`tests/large_scale_benchmark.rs` are new binaries, not changes to the
library's public API surface). The dataset's actual figshare-hosted
license is unchanged from Phase 8's original CC BY 4.0 finding; the issue
was that a different file had been used, not that the license of the
correct file was ever in doubt.

**Locally verified, all green**: `cargo fmt --all -- --check`, `cargo
clippy --workspace --all-targets --all-features -- -D warnings`, `cargo
test --workspace --all-features` (110 tests: 63 lib + 10 bin + 6
adversarial + 4 json_roundtrip + 6 literature_conditions + 3
large_scale_benchmark + 6 metamorphic + 7 provider_failures + 5
validation), `cargo test --workspace --no-default-features` and
`--no-default-features --features mikiwame`, `RUSTDOCFLAGS="-D warnings"
cargo doc --workspace --all-features --no-deps`, `cargo build --features
serde,clap --bin gugen`, both `wasm32-unknown-unknown` checks, `cargo
audit`, `examples/benchmark_report`/`examples/large_scale_benchmark`
re-run and diffed byte-identical against checked-in output (the latter
diffed twice in a row to also confirm its own reproducibility claim
holds outside the report's own re-run-internally check) — all re-run
again after the corpus rebuild described in the §28 report, not just
once before it.
