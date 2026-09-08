# Phase 37: fixed ranking boundary

Status: **implemented slice** (2026-09-06). This document describes the
auditable boundary only; it is not evidence that a scorer predicts
experimental success.

## Contract

`CandidateFeatureVector` accepts a non-empty candidate ID and finite,
caller-supplied named features. `FixedLinearScorer` accepts finite fixed
weights, a finite bias, and explicit scorer provenance. `rank()` returns
descending score order with a deterministic candidate-ID tie-break and keeps
the input features and provenance in every `RankedCandidate`.

The boundary rejects blank or duplicate names, non-finite values, missing
features, and non-finite scores. It does not train, personalize, consume
execution labels, infer chemical properties, or modify `Planner` validity,
balance, safety, or provenance gates.

`rank_with_abstention` adds an explicit margin-only policy. It retains the full
ranking and returns audit fields (`top_score`, runner-up score, margin, and
tie state) plus a typed abstention reason for empty input, a top tie, or an
insufficient margin. The margin is a separation diagnostic, not a calibrated
probability.

## Evidence

The unit tests cover score ordering, tie-breaking, input-order invariance,
feature retention, provenance retention, and invalid/missing input rejection:

```text
cargo test --all-features ranking --lib
```

The feature is intentionally not wired into route selection or the public
planning report. Independent group-held-out calibration and jurisdiction-
specific patent review remain release gates.
