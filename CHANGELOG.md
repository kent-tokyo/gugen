# Changelog

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
See [`tasks/todo.md`](tasks/todo.md) for full phase-by-phase detail behind
each entry.

## [Unreleased]

### Added

- **Phase 15A — route-suitability evidence model.** New `route_suitability`
  module: `SuitabilityVerdict` (`Supports`/`Contradicts`/`Unknown`,
  `#[non_exhaustive]`), `SuitabilityFinding`, and
  `RouteSuitabilityAssessment` (a `Vec<SuitabilityFinding>` per
  `(target, RouteFamily)` -- deliberately never an aggregated single
  verdict, so contradictory findings are never force-merged). New
  `RouteSuitabilityProvider` trait (`src/provider.rs`), a fourth provider
  trait following `ProcessEvidenceProvider`'s shape, and
  `InMemoryRouteSuitabilityProvider` backed by two hand-verified,
  literature-cited seed records (a `Supports` finding for BaTiO3 +
  Mechanochemical, DOI 10.3390/chemistry4020042; a `Contradicts` finding
  for Mg(OH)2 + ConventionalSolidState, DOI
  10.1080/21870764.2021.2019376, cross-checked against gugen's own
  already-curated MgAl2O4 firing-temperature record). New
  `SynthesisPlanningReport.route_suitability` field (report-level, one
  entry per route family assessed, not per plan) and
  `Planner::with_route_suitability_provider` constructor. **Deliberately
  not wired into ranking**: nothing in `score.rs` reads this data, no
  route is deprecated or reordered by a finding, and `score.rs`'s existing
  `PlanningAssumption` text about route-family suitability is unchanged --
  this phase builds the evidence vessel only, per the owner's explicit
  instruction not to touch any algorithm or score this phase. A dedicated
  test (`configuring_the_provider_does_not_change_any_plans_score_or_
  confidence`, `tests/route_suitability.rs`) asserts this directly:
  wiring the provider in produces byte-identical `score`/`confidence`/
  `applicability`/`evidence` to an `offline_minimal` run of the same
  target. Real negative-filtering rules and a broader curated-findings
  set are left to a future, separately-triggered phase.
- **Phase 15B — explainable route deprecation.** New `RouteRecommendation`
  enum (`Recommended`/`NotRecommended`/`InsufficientEvidence`/
  `ConflictingEvidence`, `#[non_exhaustive]`) and a pure function,
  `derive_recommendation`, deriving one from a `RouteSuitabilityAssessment`
  alone (no `Planner`/provider dependency). Decision matrix: any
  `Supports` and `Contradicts` coexisting -> `ConflictingEvidence` (a
  weak `Contradicts` is never silently outvoted by a `Supports` finding);
  a "strong" `Contradicts` (`strength != Weak` **and**
  `applicable_to == EvidenceScope::ExactTarget` -- an allow-list, not a
  denylist of weaker scopes, so `SimilarMaterial`/`GeneralRule` can never
  alone trigger exclusion) with zero counter-evidence -> `NotRecommended`;
  `Supports` only -> `Recommended` (a label, no ranking bonus); everything
  else -> `InsufficientEvidence`.
  `Planner::plan` now acts on `NotRecommended` specifically: a plan whose
  route family clears that bar is moved out of `plans` into a new
  `SynthesisPlanningReport.not_recommended: Vec<NotRecommendedPlan>`
  field, carrying the unmodified plan (same score/confidence/evidence --
  filtering happens after scoring, not instead of it) plus the specific
  `Contradicts` findings that triggered exclusion. Filtering runs before
  the existing rank/truncate step, so `SearchBudget::max_plans_returned`'s
  overflow message continues to describe only recommendable plans. When
  every generated plan is excluded this way, the report abstains
  explicitly via a new `unresolved` entry ("route selection") rather than
  returning an indistinguishable empty success; `applicability` is left
  unchanged in that case -- a deliberately different, weaker claim than
  `abstain()`'s own `ApplicabilityLevel::OutOfDomain` ("gugen built valid
  chemistry, evidence just contradicts every route tried" vs. "gugen
  cannot handle this material at all").
  A new regression test asserts no curated record targets bare `Fe2O3`
  (the maghemite/hematite polymorph trap from Phase 15A, made permanently
  checkable). **`score.rs` remains completely untouched**: no numeric
  score changes anywhere, and `curated_records()` was not expanded beyond
  Phase 15A's original two entries -- the `SimilarMaterial` safety
  condition is exercised via hand-built test data
  (`tests/route_suitability.rs`), not shipped curated content. CLI
  markdown rendering of `not_recommended` is out of scope (consistent
  with `route_suitability` itself not being rendered either).
- **Phase 16 — chematic-crystal -> mikiwame structure bridge.** New
  optional `chematic_crystal` feature (implies `mikiwame`, since
  chematic-crystal alone has no structural diagnostics of its own) and
  `src/chematic_crystal_adapter.rs`: `to_mikiwame_structure(&chematic_
  crystal::PeriodicStructure) -> mikiwame::OwnedStructure`. Closes the
  specific gap `mikiwame_adapter.rs` had named since Phase 6 -- gugen's
  own `TargetStructure` is free text, and building a real structure
  "depends on chematic-crystal," which published its first version,
  0.15.0, on 2026-08-14. Verified live against both the docs.rs API and
  the real vendored source (not guessed, AGENTS.md §21.3) before
  implementation: chematic-crystal 0.15.0 is pure geometry -- no
  symmetry/Niggli reduction, no CIF parser, no composition-from-formula
  prediction, no polymorph identification -- so this phase is
  deliberately narrower than the route-suitability integration the
  original roadmap sketch assumed (see Non-goals below). Not a plain
  field-by-field copy -- two correctness fixes beyond a naive conversion,
  both confirmed against real source before being handled: (1)
  same-element `SiteSpecies` are consolidated *within one `PeriodicSite`
  only* (never across separate sites) and dropped if the summed occupancy
  is exactly `0.0`, preventing a false-positive `SITE_DUPLICATE` finding
  that a naive per-species flat-map would produce; (2) a left-handed
  (negative-determinant) input lattice, which chematic-crystal accepts as
  physically valid but mikiwame rejects as `InvalidInput`, is corrected
  via an exact basis change (swap the `b`/`c` lattice rows and the
  matching fractional `y`/`z` component of every site), verified by a
  direct Cartesian-invariance test, not a geometry-altering heuristic.
  9 new tests, including one proving a genuine cross-site duplicate is
  still caught by real `mikiwame::analyze` after conversion (the
  false-positive fix doesn't become a false negative), and one pinning
  that same-element consolidation can itself push a summed occupancy
  just past `1.0` -- accepted by chematic-crystal's own tolerance but
  flagged by mikiwame's stricter per-site range check, surfaced rather
  than silently clamped. **Non-goals**:
  not wired into `Planner::plan` (`TargetSpecification` still has no
  geometry field -- same caller-driven boundary as `mikiwame_adapter.rs`
  since Phase 6); no mapping into `route_suitability` (mikiwame answers
  "is this structure physically valid," route_suitability answers "which
  route family suits this target" -- conflating them without real
  literature backing would repeat the unsourced-heuristic mistake
  Phase 15A deliberately avoided); no `ApplicabilityLevel::InDomain`
  promotion (needs a bulk-inorganic-vs-MOF/thin-film classifier neither
  crate provides); no general polymorph-disambiguation mechanism (no
  symmetry detection means two representations of the same real structure
  can't be reliably compared by raw geometry -- Phase 15B's Fe2O3
  regression test stays a single documented case); `curated_records()`
  not expanded.
- **Phase 17 — route-suitability corpus audit + decision-policy
  evaluation.** New `examples/route_suitability_policy_audit.rs`,
  generating `docs/route_suitability_corpus_audit.md`. **Explicitly not a
  route-family prediction-accuracy benchmark** --
  `InMemoryRouteSuitabilityProvider` is a lookup over hand-verified
  literature evidence for specific `(target, RouteFamily)` pairs, not a
  generalizing classifier, so a holdout of unknown targets correctly
  returns `InsufficientEvidence` almost everywhere; reporting that as
  "accuracy" would misrepresent the system. Two parts instead: **17A**
  audits the existing `benchmarks/data/kononova_sample.jsonl` (1500 rows,
  already committed from Phase 11, no new fetch) for how much
  route-suitability-relevant evidence actually exists -- headline finding:
  route family, success/failure, and comparative-route-rejection are not
  present in this corpus and can't be derived without per-paper reading
  (deliberately not attempted via any keyword/operations heuristic, which
  would fabricate ground truth); evidence coverage against the shipped
  provider (4/1500) and a polymorph-ambiguity **floor** (against 7
  explicitly hand-listed, non-exhaustive well-known systems, never
  reported as a bare rate) are also measured. **17B** evaluates
  `derive_recommendation`'s conservative decision policy against a real,
  freshly hand-verified holdout record (BiFeO3 + ConventionalSolidState ->
  `Contradicts`, DOI 10.1111/jace.19702, open access, abstract read
  directly) gathered via a bounded literature search that found exactly
  one record meeting the bar of "documents a route's own real difficulty
  for a specific target" -- the low yield is itself part of the finding,
  not padded with weaker "route X was used" records. Two real categories
  (dev = the 2 existing `curated_records()` entries; holdout = this
  phase's 1 new record), not the three-way split originally sketched --
  `derive_recommendation`'s matrix was designed from stated principles,
  never fit against data, so "threshold-reference" is empty in gugen's
  actual history. Determinism (finding order does not change
  `derive_recommendation`'s output for the real holdout record) is a
  dedicated test in `tests/route_suitability.rs`, not a report line.
  **Non-goals**: no converting `Supports` counts into a success
  probability; no interpreting absence of success as `Contradicts`; no
  generalizing route suitability from composition/material-name alone; no
  converting chematic-crystal/mikiwame diagnostics into route
  recommendations without literature backing; no adjusting
  `derive_recommendation` based on holdout results; holdout record never
  added to `curated_records()`. Completion is explicitly not gated on
  strong numbers -- a mostly-abstaining result against real data is itself
  the valuable finding (what's needed next is a larger hand-verified
  negative-evidence corpus, not an algorithm change).

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
