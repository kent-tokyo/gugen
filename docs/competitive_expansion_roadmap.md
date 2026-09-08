# Competitive expansion roadmap

Status: owner-requested track, started 2026-09-04.

The goal is to exceed three capability classes: reaction-network systems for
route discovery and large materials data, Retro-Rank-In/SyntMTE-style learned
candidate ordering, and SynthesisSimilarity-style similar-material retrieval.
These are workflow capability goals, not claims of higher synthesis success.

## Phase plan

### Phase 33 — reaction-network foundation (implemented)

- Store caller-supplied, already validated `BalancedReaction` records as a
  deterministic reaction hypergraph.
- Traverse from available base precursors with a bounded breadth-first search.
- Reuse `SynthesisRoute` for cross-stage connectivity and conservation checks.
- Deduplicate exact records and make output independent of network input order.
- Keep all network access outside the core crate.

Public API: `ReactionNetwork`, `NetworkSearchBudget`, `NetworkError`.

### Phase 34 — network ingestion contract (implemented)

`NetworkReactionRecord` carries source identifier, license, dataset version,
record identifier, reaction qualification, and an explicit rejection reason.
`parse_network_records` now parses one record per JSONL line with strict and
lenient modes, validates schema/provenance/qualification invariants, reports
rejected lines, and deterministically collapses exact duplicates.
`ReactionNetwork::from_records` imports qualified records only and rejects
unsupported schema versions. Import remains caller-controlled; no credentials,
network client, or silently bundled large corpus belongs in core.

The local fixture and `gugen validate-network` file adapter cover malformed,
schema-incompatible, duplicate, provenance-incomplete, and unbalanced input.
The command reports the import audit as JSON and never fetches or mutates the
source file.

Gate: malformed, schema-incompatible, duplicate, provenance-incomplete, and
unbalanced records are rejected or explicitly classified with fixture-level
counts. Qualified records alone enter the executable graph.

### Phase 35 — planner integration (implementation hold: patent-risk gate)

Expose qualified network routes in the existing report schema. Add route depth,
state, and result budgets; retain rejected routes and unresolved data as
evidence. Preserve the current depth-1/2 search as a regression baseline.

Gate: no route can bypass existing balance, safety, process, or provenance
validation; deterministic output under record and precursor permutation.

Patent gate result: **hold**. The preliminary chart in
[`patent_risk_phase35_claim_chart.md`](patent_risk_phase35_claim_chart.md)
found medium-to-high similarity between the proposed request-to-route report
shape and public US route-planning claims. JP/US/EP claim-by-claim review is
still open, so Phase 35 is not implemented or released.

### Phase 36 — similar-material retrieval (composition slice implemented)

Introduce typed, deterministic composition and optional structure fingerprints
with a top-k retrieval API. Keep similarity as retrieval context, not a
success-probability score. Use target-held-out and scaffold/material-family
grouped splits to prevent near-duplicate leakage.

Implemented slice: `CompositionFingerprint`,
`CompositionSimilarityIndex`, and deterministic top-k retrieval over
caller-supplied compositions using fixed normalized L1 distance. No structure
inference, learned embedding, property prediction, online index, or route
recommendation is included. The preliminary patent screen and design-around
are recorded in [`patent_risk_phase35_claim_chart.md`](patent_risk_phase35_claim_chart.md).

Remaining gate: a license-cleared real retrieval corpus, target-held-out /
scaffold-grouped splits, and missing-structure behavior for a future optional
structure layer. The pinned local fixture and Recall@k/MRR contract are in
[`phase36_similarity_retrieval.md`](phase36_similarity_retrieval.md); they do
not support a real-data or comparative claim.

### Phase 37 — learned candidate ranking (fixed boundary implemented)

Add a feature-vector and scorer boundary for pairwise/listwise candidate
ordering. Start with an auditable fixed heuristic baseline; learned weights are
optional and caller-supplied. Use group-held-out calibration and retain feature
values and model provenance in every ranking result.

Implemented slice: `CandidateFeatureVector`, `FixedLinearScorer`, and
deterministic ranking with retained feature values and scorer provenance. The
scorer is standalone and caller-supplied; it is not trained, personalized, or
wired into `Planner`.

Implemented follow-up: `rank_with_abstention` returns ranked results together
with deterministic score-margin audit fields and explicit abstention reasons
for empty input, a top-score tie, or an insufficient margin. No execution
labels are synthesized, and ranking does not alter chemical validity, balance,
safety gates, or existing Planner ordering.

Remaining gate: independent group-held-out calibration. The current margin is
only a separation diagnostic and must not be interpreted as a probability or
experimental-success estimate.

### Phase 38 — diverse portfolio selection

Combine network, retrieval, and learned signals with MMR/Pareto/role coverage.
Return a portfolio rather than a single opaque winner, with diversity and
Recall@k regressions visible.

Implemented slice: a standalone caller-supplied portfolio boundary with fixed
relevance/diversity inputs, deterministic greedy marginal-diversity selection,
optional role coverage, and explicit uncovered-role audit output. It is not
wired to network routes, retrieval indexes, learned scorers, or `Planner`.

Implemented follow-up: `ParetoCandidate`/`ParetoFrontier` return a sorted
non-dominated set and dominated-ID audit using caller-supplied objective
directions. No single winner is selected.

Remaining gate: common-corpus diversity/Recall@k evaluation, Pareto or
network-aware integration, and jurisdiction-specific review.

### Phase 39 — comparative validation gate

Compare against pinned versions of the three named systems on a common,
license-cleared corpus. Publish the split, leakage audit, ablations,
uncertainty, runtime/resource conditions, and failure taxonomy before making
any external “exceeds” claim.

Implemented slice: `ComparativeEvaluationProtocol` records pinned comparator
versions, corpus license/source notes, runtime conditions, grouped
development/confirmation cases, and a fail-closed leakage audit. Its status
is always `ProtocolOnly`; it does not execute competitors, calculate scores,
or produce an external comparison claim.

Implemented follow-up: `EvaluationMetadata` and
`ComparativeEvaluationManifest` require explicit ablation, uncertainty,
resource-condition, and failure-taxonomy metadata without accepting measured
results.
The `validate-comparative` CLI adapter emits the fail-closed audit and returns
a non-zero status for leakage or missing splits.

Remaining gate: obtain and license-clear the common corpus, run all systems
under matched conditions, and publish ablations, uncertainty, resource
conditions, and failure taxonomy.

## Non-goals and invariants

- No online data fetching or API credentials in gugen core.
- No claim that a ranking score predicts experimental success.
- No composition-only polymorph or structure-property inference.
- No version bump or release solely because a phase was added.

## Patent-risk gate (added 2026-09-04)

Patent risk is jurisdiction- and claim-specific; an algorithm name, paper
reference, or broad functional similarity is not an infringement finding.
Before implementing or releasing any Phase 35–39 feature, create a claim
chart for the intended jurisdictions and record the design-around decision.

- Keep core computation caller-supplied, deterministic, and offline.
- Do not add personal-preference learning, equipment/inventory-aware route
  recommendation, interactive preference feedback, or autonomous lab control
  without a separate FTO review.
- Do not interpret the current `Planner`/`ReactionNetwork` as cleared merely
  because it lacks online fetching or robotics; review the combination of
  target input, reaction enumeration, ranking, and display before commercial
  distribution.
- The existing SISSO correction is a license/attribution review item first;
  remove or replace it only if a concrete patent claim or license constraint
  is identified, not on speculation alone.
- If a claim chart finds a medium-or-higher risk feature already implemented,
  disable or remove that feature behind a focused change, preserve a tested
  lower-risk alternative, and document the reason in the changelog.
