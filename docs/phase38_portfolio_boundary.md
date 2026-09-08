# Phase 38: diverse portfolio boundary

Status: **implemented slice** (2026-09-06). This is a generic, caller-supplied
selection primitive, not a chemistry or experimental-success predictor.

## Contract

`PortfolioCandidate` carries a finite relevance value, a finite named feature
vector, and optional role labels. `DiversePortfolioSelector` applies fixed
non-negative relevance and diversity weights. The first item is selected by
relevance; subsequent items maximize relevance plus the minimum L1 distance to
the selected set. Candidate IDs provide deterministic tie-breaking.

The result retains selected candidates, fixed selector provenance, selection
scores, covered roles, and required roles that remain uncovered. Role labels
are audit data only and do not silently override the fixed score.

The boundary rejects duplicate IDs, non-finite values, empty feature vectors,
and mismatched feature schemas. It is not connected to reaction networks,
similarity indexes, learned scorers, route selection, or `Planner`.

`ParetoCandidate` and `ParetoFrontier` provide the complementary audit-only
primitive: with caller-supplied objective directions, they return the sorted
non-dominated set and the sorted dominated IDs. No single winner is selected,
and objective names have no chemistry-specific meaning inside the module.

## Evidence

The unit tests cover relevance-plus-diversity selection, deterministic
input-order invariance, role-coverage reporting, duplicate IDs, and schema
validation:

```text
cargo test --all-features portfolio --lib
```

A pinned local contract additionally checks the selected IDs, mean pairwise
L1 diversity, role coverage, provenance, and permutation invariance:

```text
cargo test --all-features --test portfolio_selection
```

This fixture is a regression contract only; it is not a real-material
benchmark and does not support a comparative Recall@k claim.

Real-corpus diversity and Recall@k evaluation, Pareto/network-aware
integration, and jurisdiction-specific patent review remain release gates.

The local Pareto unit tests cover mixed minimize/maximize directions,
deterministic frontier ordering, duplicate IDs, and objective-schema errors:

```text
cargo test --all-features pareto --lib
```
