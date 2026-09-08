# Phase 36: composition retrieval fixture

## Status

The first retrieval contract is implemented in `src/similarity.rs`:

- normalized composition fingerprint;
- fixed L1 distance;
- deterministic top-k ordering with material-ID tie-breaking;
- caller-supplied records only;
- no property, success, route, structure, model, or online-index semantics.

The pinned fixture is
`tests/fixtures/similarity_retrieval_v1.json`. Its `holdout_queries` use
groups absent from the reference-material groups, and the integration test
asserts that query IDs never enter the reference index. The current fixture
reports Recall@1 = 1.0 and MRR = 1.0 for two deliberately small cases. This
is a contract test, not a real-data quality claim.

Run it with:

```text
cargo test --all-features --test similarity_retrieval
```

The fixture is not a substitute for a license-cleared, target-held-out,
scaffold/material-family corpus. No external benchmark or comparative claim
is made until that corpus and its split are independently documented.
