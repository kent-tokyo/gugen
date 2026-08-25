# Phase 31 PR 1 — Two-Step Synthesis Route Search

## Why this phase exists

The owner gave the go-ahead ("phase31") to start Phase 31 (Reaction
Hypergraph) of the "Gugen Exploration Supremacy" vision (`ROADMAP.md`).
Phase 31 is listed there as "vision only": a proper multi-step/
intermediate representation (AND-OR search over a reaction hypergraph,
not a simple graph), with A*/k-shortest-paths/multi-objective search
over it, gated on beating "reaction-network's own reproduced result"
(McDermott et al., `materialsproject/reaction-network`) on a known
multi-step path.

Today's `search_precursor_sets` (`src/precursor.rs`) is structurally
single-step: it only ever looks for a combination of candidates from a
flat pool whose reaction balances directly against the final target.
There is no way to represent or search for a route where a precursor
combination first makes an intermediate, which then reacts further
toward the target — exactly what "multi-step" means.

## Scope

**In scope**: a new, additive orchestration layer (`src/multi_step.rs`)
adding a two-stage (precursors → intermediate → target) route search,
reusing the existing single-step search and reaction machinery
unchanged. Hand-built synthetic fixture verification. Two Step-0
findings (below), reported honestly rather than guessed.

**Out of scope, unconditionally**: any change to `search_precursor_sets`,
`balance()`, `BalancedReaction`, `Planner`, or any other existing public
type's behavior; depth > 2 (three or more stages); A*/k-shortest-paths/
multi-objective search; wiring into `Planner`/`SynthesisPlan`/
`ProcessStep` generation; the reaction-network comparison itself; any
real-corpus multi-step recall measurement; version bump.

## Design finding: this is an orchestration gap, not a data-model gap

`BalancedReaction::new(reactants: Vec<ReactionSpecies>, products:
Vec<ReactionSpecies>)` (`src/reaction.rs`) and `balance(reactants:
&[Composition], products: &[Composition])` (`src/balance.rs`) have no
concept of "target" baked in at all — `balance()` just solves reactants
vs. products for any composition lists. The "target" concept exists
only in `search_precursor_sets`'s own orchestration layer, which always
calls `balance()` with `products = [target, ...byproducts]`. A
`BalancedReaction` is already exactly the right hyperedge shape (many
reactants → many products) for a reaction-hypergraph node. So this PR
is new orchestration code that chains `search_precursor_sets` — reusing
`balance()`/`BalancedReaction` completely unchanged — not a new search
algorithm.

## Design: `SynthesisRoute` and `search_two_step_routes`

New file `src/multi_step.rs`, mirroring Phase 30's own `src/
candidate_generator.rs` (a new orchestration layer calling existing
search as a black box, not a change to it).

`SynthesisRoute` is an ordered sequence of `BalancedReaction` stages,
smart-constructor validated (matching this crate's existing convention:
`Composition::new`, `ReactionSpecies::new`, `BalancedReaction::new`,
`CompetingPhase::new` all validate on construction). `SynthesisRoute::new`
checks: `stages` non-empty; the final stage's products include `target`;
every reactant of every stage is either a caller-supplied base
precursor or a product of a strictly earlier stage. Element
conservation *within* each stage is already guaranteed by
`BalancedReaction::new` itself — this constructor only adds the
cross-stage connectivity check, and generalizes to any depth, not just
two stages.

`search_two_step_routes(target, base_candidates,
intermediate_candidates, constraints, budget)`:
1. Depth-1 pass — `search_precursor_sets(target, base_candidates, ...)`
   unchanged. Every accepted set becomes a 1-stage route; a real
   single-step route always still wins where one exists.
2. For each caller-supplied intermediate composition `I`: a depth-1
   search targeting `I` instead of `target`, checking reachability from
   the base pool.
3. For every reachable `I`: splice a synthetic, produced-not-purchased
   `PrecursorCandidate` for `I` into an expanded pool, then search that
   pool against the real `target`. Any accepted set that actually
   consumes the synthetic candidate becomes a 2-stage `SynthesisRoute`
   — one that doesn't (already covered by the depth-1 pass) is skipped,
   so a route already reachable in one step is never duplicated as a
   spurious two-step one.

`intermediate_candidates` is caller-supplied, never computed or fetched
by this function — matching `FrequencyPriorGenerator`'s own
"caller-supplied, never computed by the crate" convention. A caller
with a `ThermodynamicProvider` could source it from
`competing_phases(target)`, but this function stays provider-agnostic
and trivially testable with plain fixtures.

Bounded to `1 + intermediate_candidates.len() + 1` calls to
`search_precursor_sets` — not combinatorial, since
`intermediate_candidates` is caller-bounded.

**Known simplification** (`ponytail:` comment in the source): when an
intermediate is reachable via more than one distinct stage-1
combination, only the first (search's own deterministic ordering) is
used to build a route — this returns at least one valid route per
reachable intermediate, not every combination of stage-1 × stage-2
routes. Revisit with exhaustive enumeration only if a real fixture
needs the alternates.

## Two Step-0 findings — neither resolved by this PR

Checked directly, before designing anything around them, per this
project's own "does an independent, licensable source even exist"
discipline (Phase 21A's own precedent):

1. **Reaction-network's own BaTiO3 result is not machine-readable.**
   `docs/thermodynamic_selectivity_dataset_feasibility.md` (Phase 21A,
   already verified) states McDermott et al.'s 9 synchrotron-XRD-verified
   BaTiO3 routes are real but "embedded in PDF figures, not a structured
   file." Comparing against them requires either transcribing them by
   hand from the paper's SI (with its own citation, respecting its CC BY
   4.0 terms) or installing and running the real
   `materialsproject/reaction-network` Python package — a new external
   tool, outside this repo's all-Rust-crate convention so far. Both are
   real Step-0 data-acquisition decisions, not resolved here.
2. **No real corpus in this repo has a multi-step/intermediate concept
   at all.** Checked directly: `benchmarks/data/kononova_sample.jsonl`
   and the frozen catalog manifests are all `{precursors: [...],
   target_elements: {...}, target_formula: "..."}` — single-reaction
   only. So there is currently no way to compute a "does multi-step
   search recover more known routes" number against real data in this
   repo, independent of the reaction-network comparison specifically.

Both are reported here as named findings, not guessed or quietly
skipped. This PR is capability-building only, verified by hand-built
synthetic fixtures — matching Phase 30 PR 1's own precedent of building
real infrastructure while explicitly not claiming a data-dependent gate.

## Verification

Hand-built synthetic fixtures in `src/multi_step.rs`'s own test module:

- `direct_search_cannot_reach_a_five_way_target_under_a_tight_budget` /
  `two_step_search_recovers_the_route_the_direct_search_cannot_reach` —
  a target needing all five of {Fe, Li, Na, K, O2} at once (arity 5)
  under `max_precursors_per_plan = 4`: the direct search structurally
  cannot find it, but via an intermediate `I = FeLiNa` (arity 3 to
  make, then arity 3 with I+K+O2), the two-step search does. This is a
  real, on-theme illustration of a limitation Phase 28 already
  documented (`max_precursors_per_plan` caps combination size
  "regardless of search algorithm") that multi-step search can help
  with, independent of any thermodynamic-driving-force question.
- `a_directly_reachable_target_is_not_duplicated_as_a_spurious_two_step_route`
  — a real single-step BaTiO3-style target with an unrelated candidate
  intermediate present: exactly one route returned, one stage.
- `an_unreachable_target_returns_no_routes_without_panicking` — an
  empty result, not a panic or false positive.
- `synthesis_route_rejects_empty_stages` /
  `synthesis_route_rejects_a_final_stage_that_does_not_produce_the_target`
  / `synthesis_route_rejects_a_stage_with_an_unexplained_reactant` —
  `SynthesisRoute::new`'s own validation.

**Full workspace quality gate green**: `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo test --workspace` (both `--all-features`, 319 tests, and
`--no-default-features`, 165 tests — 7 new tests, zero regressions,
golden `tests/fixtures/batio3_report.{json,md}` fixture confirmed
unaffected), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace
--all-features --no-deps`, `cargo semver-checks check-release
--baseline-version 0.6.0 --all-features` (no update required — purely
additive: a new module, a new error enum separate from `GugenError`,
two new public types, one new public function).

## Why `RouteError` is a new enum, not a new `GugenError` variant

`GugenError` is a public, non-`#[non_exhaustive]` enum — adding a
variant to it would be a breaking change for every downstream
exhaustive `match`, which `cargo semver-checks` would (correctly) flag
as requiring a major bump. `ProviderError` already establishes the "a
distinct concern gets its own error enum" precedent in this crate
(`src/error.rs`, "Deliberately separate from `GugenError`"); route
assembly is similarly distinct from core numeric/compositional
validation, so `RouteError` follows the same pattern. It wraps
`GugenError` via `#[from]` for propagating a genuine underlying search
failure, matching how `GugenError::Catalog` already wraps
`ProviderError` the same way.

## What this phase does not claim

No claim about `Planner`/`SynthesisPlan` integration — this PR adds a
new, additive search function only, not wired into planning. No claim
about beating reaction-network's own result, or about real-corpus
multi-step recall — both are named, open Step-0 findings, not
attempted. No claim about search-algorithm sophistication (A*/
k-shortest-paths/multi-objective) — this PR's depth-2 chaining is
exhaustive-but-bounded reuse of existing search, not a general graph
search.

## Status

Implemented, tested, quality gate green. Branch
`feature/phase31-pr1-two-step-route-search`.
