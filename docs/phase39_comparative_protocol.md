# Phase 39: comparative evaluation protocol

Status: **protocol boundary implemented** (2026-09-08). No external system is
executed and no performance claim is produced.

`ComparatorSpec` records a comparator name, immutable version string, and
source. `EvaluationCase` records a case ID, a group ID, and either the
development or confirmation split. `ComparativeEvaluationProtocol` also
requires a corpus identifier, license note, and runtime conditions.

`leakage_audit()` reports split counts and groups that cross the split.
`validate_for_measurement()` fails closed on cross-split groups or a missing
split. The resulting status is always `ProtocolOnly`; scores and external
superiority claims are intentionally outside this API.

`EvaluationMetadata` records at least one named ablation, an explicit
uncertainty method/confidence level/replicate count, CPU and resource limits,
and a named failure taxonomy. `ComparativeEvaluationManifest` combines this
metadata with the protocol, but still contains no measured-result field.

The CLI adapter accepts that manifest as JSON and runs the same leakage audit:

```text
gugen validate-comparative manifest.json
```

It emits `measurement_ready`, the split audit, and a validation error when
present. Cross-split groups or a missing development/confirmation split are
printed as audit evidence and return a non-zero exit status. The command never
invokes a comparator or turns protocol metadata into a performance claim.
Nested metadata invariants are rechecked after JSON deserialization, so empty
ablation/failure fields, invalid confidence levels, and zero parallelism also
fail closed.

The repository includes a license-neutral example manifest at
`tests/fixtures/comparative_manifest_v1.json`; its round-trip test confirms
that serialization does not introduce score or measured-result fields.

This prevents a local fixture or a pinned version manifest from being
mistaken for the common, license-cleared corpus comparison required by the
roadmap. The actual comparison remains gated on obtaining that corpus and
independently running the named systems under recorded conditions.
