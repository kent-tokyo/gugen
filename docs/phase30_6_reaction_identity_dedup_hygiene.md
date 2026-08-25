# Phase 30.6 — Reaction Identity and Search Dedup Hygiene

## Why this phase exists

Phase 30.5's investigation (see `docs/exploration_fusion_search_coupling.md`)
found and reproduced a real, narrow defect while root-causing an unrelated
benchmark bug: `evaluate_complete_state`'s dedup (`src/precursor.rs`) keys on
`BalancedReaction`'s derived `PartialEq`, which compares `reactants`/`products`
as **ordered vectors**. But `balance()` builds those vectors by zipping
positionally against its own input order (`vector_to_reaction`,
`src/balance.rs`), which itself tracks `chosen`'s ascending indices into
whichever candidate array the search was given. When the exact same
chemistry is discovered via two different combinations — a larger
combination's extra precursor solving to a zero coefficient and collapsing
to a smaller one already found, or two catalog entries sharing a
composition under different `PrecursorId`s (e.g. a corpus's "Fe2O3" /
"α-Fe2O3" polymorph-label duplicate) — and the two solves happen to order
their reactants differently, dedup fails to recognize them as the same
reaction. The same one real plan gets recorded as two `accepted` entries,
which can silently consume two slots of `SearchBudget::max_plans_returned`
in `Planner`'s ranked output for what is really one plan.

Phase 30.5 deliberately deferred fixing this to a separate phase, so that
a benchmark-harness fix and a production search-behavior change never
landed in the same diff — if both changed together, a difference in the
corrected benchmark numbers couldn't be attributed to either change
specifically. Phase 30.5 (PR #66) merged `main@17df256` with the
corrected benchmark; this phase is the deferred production fix, done in
isolation.

## Scope

**In scope**: fix `evaluate_complete_state`'s dedup identity to be
order-invariant, composition-based (not `PrecursorId`-based), and
coefficient-scale-invariant. Minimal regression test first, then measure
real impact on the `accepted` set across a real corpus sample.

**Out of scope, unconditionally**: `BalancedReaction`'s own public
`PartialEq` (unchanged — remains reactant/product vector-order-sensitive,
since this crate makes no promise about how a caller outside this module
might already rely on that today); `AcceptedPrecursorSet`'s public shape;
`max_plans_returned` truncation behavior itself; any fusion rule,
generator, or `Planner` change; version bump; release. `search_precursor_sets`'s
public signature and `SearchBudget`/`RejectionCode` are untouched.

## Design: `CanonicalReactionKey`, dedup-internal only

A new, module-private `CanonicalReactionKey` (`src/precursor.rs`) is used
**only** inside `evaluate_complete_state`'s dedup — never exposed, never a
replacement for `BalancedReaction`'s own `Eq`. Two `BalancedReaction`s
canonicalize to the same key exactly when they represent the same
chemical equation:

- **Reactant side and product side kept separate** (two independent
  `BTreeSet`s, never merged) — a reaction is never equal to itself with
  reactants and products swapped.
- **Keyed on `Composition`, not `PrecursorId`** — two candidates sharing
  a composition under different ids resolve to the same reactant slot,
  directly fixing the polymorph-synonym case above.
- **Coefficients reduced by their own GCD across both sides together**,
  computed explicitly inside `CanonicalReactionKey::from_reaction` rather
  than assumed from `balance()`'s own (already-GCD-reduced, per
  `scale_to_integers`) output — this type's own invariant doesn't
  silently depend on an upstream implementation detail holding forever.

`Composition` has no `Hash` impl (only `Ord`/`Eq`, exact via `Frac`
amounts — `src/composition.rs`), so `CanonicalReactionKey` uses
`BTreeSet`, matching the crate's existing convention for avoiding adding
`Hash` to a type just to use a set (the same reasoning the pre-existing
O(n) linear-scan dedup already used).

The dedup loop itself changes from:

```rust
accepted.iter().position(|a| a.reaction == candidate_set.reaction)
```

to:

```rust
let candidate_key = CanonicalReactionKey::from_reaction(&candidate_set.reaction);
accepted.iter().position(|a| CanonicalReactionKey::from_reaction(&a.reaction) == candidate_key)
```

The "keep whichever colliding `precursors` id list sorts lexicographically
smallest" tie-break (unchanged) still makes the surviving entry's identity
independent of which combination was evaluated first.

## Verification

**Minimal regression test, built first** (per the same "small fixture
before touching real data" discipline Phase 30.5 used):
`duplicate_composition_candidates_keep_canonical_chemistry_order_invariant`
(`src/precursor.rs`) — the exact Phase 30.5 synthetic fixture that
originally *documented* the bug (`assert_eq!(outcome_a.accepted.len(), 2, ...)`)
now asserts the *fixed* behavior: both orderings collapse to exactly one
accepted entry, with the same lexicographically-smallest id list
(`["AFe2O3", "BaO"]`), regardless of which side of the third candidate the
duplicate-composition candidates land on. Confirmed to fail against the
pre-fix code before the fix was applied (`outcome_a.accepted.len() == 2`,
matching Phase 30.5's own finding exactly) and pass after.

Three new unit-level tests exercise `CanonicalReactionKey` directly,
independent of the full search:

- `canonical_reaction_key_ignores_reactant_vector_order_and_composition_synonym`
  — two `BalancedReaction`s built with deliberately reversed reactant
  vectors (mirroring what `balance()`'s positional zip produces under two
  different candidate array orderings) canonicalize to the same key, even
  though `BalancedReaction`'s own derived `PartialEq` (confirmed still
  vector-order-sensitive, asserted directly) says they're unequal.
- `canonical_reaction_key_does_not_conflate_different_reactions` — a
  negative control: two genuinely different reactions (different
  product) must not canonicalize to the same key.
- `canonical_reaction_key_is_coefficient_scale_invariant` — the same
  reaction solved at two different absolute integer scales (1x vs 2x
  every coefficient) canonicalizes to the same key, even though
  `BalancedReaction`'s own `PartialEq` treats them as different (asserted
  directly as a sanity check before asserting the canonical keys agree).

**Full workspace quality gate green**: `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo test --workspace` (both `--all-features` and
`--no-default-features`, 312 and 158 tests respectively, including the
golden fixture and every literature/commercial-catalog validation test —
zero regressions), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace
--all-features --no-deps`, `cargo semver-checks check-release
--baseline-version 0.6.0 --all-features` (no update required —
`CanonicalReactionKey` is module-private, no public API surface changed).

## Measured impact on the real corpus

`examples/phase30_6_dedup_impact_measurement.rs` (throwaway diagnostic,
not part of any locked methodology) ran `search_precursor_sets` over a
929-row stride sample of the real frozen catalog manifest (the same
corpus Phase 30.5 used), at a budget large enough to be non-exhausting
(`max_precursor_sets: 200_000`), once against the pre-fix code (via
`git stash` toggling `src/precursor.rs` only) and once against the fixed
code:

| | before (pre-fix) | after (fixed) | delta |
|---|---|---|---|
| total accepted entries across sample | 11,280 | 10,252 | **−1,028 (−9.1%)** |
| rows with >1 accepted entry | 288 | 287 | −1 |
| max accepted entries in a single row | 488 | 378 | −110 |
| mean accepted entries per row | 12.14 | 11.04 | −1.10 |

**Interpretation**: the fix removes 1,028 spurious duplicate `accepted`
entries across 929 real rows (9.1% of the pre-fix total) — a real,
measurable reduction in redundant plan-count inflation, concentrated
heavily in a small number of rows with large candidate pools and many
duplicate-composition synonyms (one row alone dropped by 110 entries).
`rows with >1 accepted entry` barely moved (288 → 287): most multi-entry
rows have genuinely different real routes, not just duplicates, so
removing spurious duplicates only flipped one row from "multiple
entries" to "exactly one." This is consistent with Phase 30.5's own
disclosure that this defect's measured recall impact was small (4/464
rows in an isolated exact-ID-recall test) — the effect here is on
`accepted`-set *hygiene* (plan-count inflation), not on whether the
correct chemistry gets found at all, exactly as originally scoped.

## What this phase does not claim

This phase does not claim, and did not measure, any change to
`Planner`'s final ranked plan *quality* — `Planner` ranks and truncates
`accepted` separately (`score.rs`), so removing duplicate entries changes
which plans compete for `max_plans_returned` slots, not how any
individual plan is scored. No claim is made about recall, since Phase
30.5 already established (and this phase's own regression test
reconfirms) that canonical chemistry recovery was already order-invariant
before this fix — this phase closes a plan-count-hygiene gap, not a
recall gap.

## Status

Implemented, tested, quality gate green, real-corpus impact measured.
Branch `feature/phase30-6-reaction-identity-dedup-hygiene`. **Not merged
— pending the owner's own explicit review and approval**, same discipline
as every prior phase in this arc.
