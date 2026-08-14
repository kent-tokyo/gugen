# Architecture (Phase 0 design)

## Crate shape

Single crate, per AGENTS.md §17 — no `gugen-core`/`gugen-data`/`gugen-cli`
split until size actually forces it. The directory layout is fixed by
AGENTS.md §17 and is not re-derived here.

## Data flow

```
TargetSpecification
      │  (composition, optional structure, desired phase, constraints)
      ▼
PrecursorCatalog provider ──► bounded precursor-set search (src/precursor.rs)
      │
      ▼
BalancedReaction candidates (src/balance.rs — exact rational/integer solve)
      │
      ▼
Solid-state ProcessStep template (src/process.rs — Required/Recommended/
      │                            Optional/Unresolved per step)
      ▼
PlanScoreBreakdown + ConfidenceAssessment (src/ranking.rs)
      │
      ▼
SynthesisPlanningReport { plans, rejected_candidates, unresolved, warnings,
                           provenance }
```

Each arrow is a module boundary with a typed, serializable intermediate —
nothing is decided by string formatting until the final Markdown renderer.

## Module responsibilities (maps 1:1 to AGENTS.md §17's file list)

- `target.rs` — `TargetSpecification`, `TargetMaterialView` trait boundary
  (see `docs/integration.md`).
- `composition.rs` — element/stoichiometry types shared by target,
  precursor, and reaction. Thin; defers to chematic-crystal's `Composition`
  once available (see integration doc) rather than growing its own.
- `precursor.rs` — candidate generation, filtering, `SearchBudget`.
- `reaction.rs` / `balance.rs` — reaction representation vs. the balancing
  algorithm are separate files so the exact-arithmetic solver
  (`balance.rs`) can be tested and reasoned about independently of how a
  `BalancedReaction` is used downstream.
- `process.rs` — `ProcessStep`, `Atmosphere`, condition ranges.
- `evidence.rs` — `EvidenceKind`, `PlanningEvidence` (see
  `docs/evidence_model.md`).
- `provider.rs` — the three provider traits (§8), no implementations.
- `planner.rs` — orchestrates the pipeline above; the only module allowed
  to call all the others.
- `ranking.rs` — `PlanScoreBreakdown`, `RankingWeights`, scoring math.
- `rejection.rs` — `RejectedCandidate`, `RejectionCode`.
- `safety.rs` — hazard metadata, `manual_review_required` derivation.
- `report.rs` — `SynthesisPlanningReport` assembly + Markdown rendering.
- `provenance.rs` — version/config/seed capture.
- `adapters/chematic.rs`, `adapters/mikiwame.rs` — the only files allowed to
  depend on those external crates; everything else depends on the trait
  boundary only.

Phase 1 implements structural/error/config plumbing only — no ranking or
diagnostic logic yet, per AGENTS.md §26 Phase 1 note ("診断・planningロジッ
クを増やしすぎないでください").

## Determinism strategy

- No wall-clock time, no thread-count-dependent iteration order, no
  unseeded randomness anywhere in `core` (AGENTS.md §25).
- Precursor search order is defined by the catalog's deterministic ordering
  contract (documented in `precursor.rs`, tested as a metamorphic invariant
  per AGENTS.md §21.4), not HashMap iteration order — candidate containers
  use `BTreeMap`/`Vec`+explicit sort, never a source of nondeterministic
  iteration for anything that reaches public output.
- `PlanId` is derived deterministically from plan contents (e.g. a stable
  hash of route family + precursor set + balanced reaction), not an
  incrementing counter seeded by wall-clock or thread-scheduling order.

## Reaction balancing method (AGENTS.md §10, Phase 0 decision required)

Chosen approach: **exact rational linear algebra over the element ×
species stoichiometry matrix**, i.e. null-space / row-reduction using exact
fractions, not floating point.

- Represent each candidate reaction as a matrix where rows are elements and
  columns are precursor/product species; solve for the integer coefficient
  vector in the matrix's null space.
- Use exact rational arithmetic (numerator/denominator pairs with gcd
  reduction) throughout, only converting to the final integer coefficient
  vector by scaling to the LCM of denominators, then dividing by the gcd of
  the resulting integers (AGENTS.md §10's normalization requirement).
- `i128` is sufficient headroom for realistic solid-state formulas (element
  counts and stoichiometric coefficients in curated inorganic reactions
  don't approach `i64` limits, let alone `i128`); all arithmetic uses
  checked operations and returns a typed error on overflow rather than
  wrapping or panicking (AGENTS.md §25 "integer overflowを処理", §10
  "overflowを安全に処理").
- Dependency decision (final choice deferred to Phase 2, when the exact
  matrix shape is implemented against real fixtures): prefer a small,
  focused exact-rational-arithmetic crate over hand-rolling gcd/fraction
  reduction, per the "already-installed / minimal dependency" ladder — but
  only if row-reduction over that type is straightforward to implement
  ourselves. A rational-arithmetic *type* is worth depending on; a full
  linear-algebra *solver* is not, since the matrices involved are small
  (element count × precursor count, both realistically <20) and a from-
  scratch Bareiss/fraction-free elimination is a bounded amount of code
  we fully control and can prove deterministic.
- Null space may be >1-dimensional (multiple valid balances, e.g. when a
  byproduct choice is ambiguous). AGENTS.md §10 requires preserving this
  ambiguity rather than silently picking one solution — `balance.rs` must
  return `Vec<BalancedReaction>`, not a single best guess, when the null
  space has dimension >1.
- Byproducts are constrained to a curated allow-list (CO₂, H₂O, O₂ to
  start — AGENTS.md §10), and any byproduct assumption made during
  balancing is recorded as a `PlanningEvidence`/assumption on the plan, not
  silently baked into the reaction.

## Provider isolation

`provider.rs` traits are the only supported extension point for external
data. Core has zero network access (AGENTS.md §8, §25); `in-memory`, JSON,
and fixture providers ship in v0.1.

Refined in Phase 13, once a real precedent existed to refine it against:
an adapter that only *consumes* pre-fetched, caller-supplied external data
-- performing no network call itself, like `src/materials_project_adapter.rs`
-- may live in-crate, behind its own feature gate (the same shape
`src/mikiwame_adapter.rs` already uses for a dependency-backed adapter).
Only a client that would *fetch* live data itself (query an HTTP API,
hold credentials, decide when to refresh) stays out of this crate
entirely.

Phase 15A added a fourth provider trait, `RouteSuitabilityProvider`
(`src/provider.rs`), following the same in-memory/curated-record shape as
`ProcessEvidenceProvider` -- AGENTS.md §8's provider list is a floor
("at minimum"), not exhaustive; nothing about this extension point is
capped at three traits.
