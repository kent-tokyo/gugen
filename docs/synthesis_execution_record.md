# Synthesis Execution Record (Phase 25)

## Scope

Records what actually happened when a gugen-proposed `SynthesisPlan` was
attempted in a real lab -- append-only, versioned-schema, local JSON/JSONL
persistence, mandatory provenance. `outcome` is a 7-state enum, never
success/failure. Outcomes are never edited toward "success" after the
fact.

**Structurally separate from `Planner`/`score_plan`, by construction.**
Nothing in this module is read by, or feeds, a plan's score, confidence,
or ranking -- the same reference-only boundary `commercial_catalog.rs`/
`literature_evidence.rs` already establish. Surfacing these records back
into planning as reference-only evidence during future runs is a later
phase (Phase 26), not this one.

**Out of scope, deliberately, this phase**: no CLI subcommand (a future
`gugen record-execution`-style command is a natural follow-up, not built
here -- see `examples/execution_record_demo.rs` for the equivalent
library-level flow), no file I/O in the library itself (see below), no
connection whatsoever to `score_plan`/ranking.

## `SynthesisExecutionRecord`

```rust
pub struct SynthesisExecutionRecord {
    pub schema_version: String,
    pub plan_identity: PlanIdentity,
    pub commercial_catalog_source: Option<String>,
    pub selected_commercial_offers: Vec<String>,
    pub actual_precursor_amounts: Vec<ActualPrecursorAmount>,
    pub actual_process_conditions: Vec<ActualProcessStep>,
    pub deviations_from_plan: Vec<Deviation>,
    pub outcome: SynthesisOutcome,
    pub characterization: ExecutionCharacterization,
    pub operator_notes: Option<String>,
    pub experiment_date: Option<String>,
    pub batch_id: Option<String>,
    pub provenance: ExecutionProvenance,
}
```

`commercial_catalog_source` is one field beyond the owner's original
fixed field list, added deliberately: Phase 26's own stated matching
criteria name "catalog provenance" as one of its match keys, and the
original field list had no other home for it. Kept `Option` so it costs
nothing when no commercial catalog was involved in a given attempt.

## `PlanIdentity` -- self-describing, not just a `PlanId`

```rust
pub struct PlanIdentity {
    pub plan_id: PlanId,
    pub route_family: RouteFamily,
    pub target_composition: Composition,
    pub precursor_compositions: BTreeSet<Composition>,
}
```

`PlanId` alone is an opaque, non-reversible hash (`derive_plan_id` hashes
route family + reaction species/coefficients into a hex string) -- it
cannot answer "what was this plan for" once separated from the
originating `SynthesisPlanningReport` file, which may no longer exist by
the time an execution record is written or read. Phase 26's own matching
criteria (target composition + canonical precursor set + route family +
...) require these fields to be present directly on the record.
`PlanIdentity::from_plan(target_composition, plan: &SynthesisPlan)`
derives `precursor_compositions` from `plan.balanced_reaction`'s
reactants; it's empty for a degraded plan with no balanced reaction
(such a plan could never have been physically attempted).
`precursor_compositions` is a `BTreeSet`, not a `Vec`: order-invariant,
matching how a canonical precursor set is naturally compared.

## Why not `process::MaterialAmount`/`ProcessStep` directly

`process::MaterialAmount { precursor, formula_units: u64, mass_grams:
Option<f64> }` is the right shape for a *planned* amount computed from
stoichiometry (an operator hasn't weighed anything yet). A lab log needs
the reverse emphasis -- an operator weighs grams, not formula units --
so `ActualPrecursorAmount` makes both fields optional, with `mass_grams`
the one an operator will actually fill in.

`ActualStepDetail` mirrors `ProcessStep`'s variants and field *names*
directly (reusing `process`'s own `MixingMethod`/`GrindingMethod`/
`FormingMethod`/`HeatingPurpose`/`Atmosphere`/`CoolingMode`/
`CharacterizationMethod` enums, not redefining them), so what was
actually done is drawn from the same closed vocabulary as what was
planned, and comparing the two is a straightforward field-by-field diff.
It uses point-value `Option<f64>` measurements instead of `process`'s
validated `min<=max` range types: a range describes a *planned* target
window, a real measurement is one number.

## Versioned schema

```rust
pub const EXECUTION_RECORD_SCHEMA_VERSION: &str = "gugen-synthesis-execution-record-v1";
```

Deliberately independent from `report::SCHEMA_VERSION` (a `u32`, bumped
only for `SynthesisPlanningReport`, and `docs/api_stability_policy.md`
documents it as *not* a strict shape guarantee). A `SynthesisExecutionRecord`
is a long-lived, externally-persisted artifact accumulated across gugen
versions over months, unlike a report generated fresh each run -- this
follows `literature_observations.rs`'s own `CORPUS_SNAPSHOT_SCHEMA_VERSION`
precedent for exactly this kind of artifact instead.

`schema_version` is checked **per line**, not via a single whole-file
header (unlike `CorpusManifest`'s own header-line precedent): a real
execution log accumulates appends across gugen versions over months, so
no single file-wide version gate fits its lifecycle, and a header line
would itself be a mutation hazard for an append-only file under
concurrent or crash-prone writers -- per-line self-description avoids
that entirely.

## Persistence: no file I/O in the library

`parse_execution_records(jsonl: &str, mode: ExecutionRecordLoadMode) ->
Result<(Vec<SynthesisExecutionRecord>, ExecutionRecordLoadReport), ProviderError>`
is pure -- it takes an in-memory JSONL string and never opens a file.
This matches a real, confirmed boundary across the entire crate: no
library module (only the CLI binary's `src/bin/gugen/commands.rs`)
ever calls `std::fs`/`File`/`OpenOptions`. Appending to, or reading, an
actual file is the caller's own few-line responsibility:

```rust
let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
writeln!(f, "{}", serde_json::to_string(&record)?)?;
```

See `examples/execution_record_demo.rs` for the complete flow (plan a
target, build a record, append it, read it back).

`ExecutionRecordLoadMode::{Strict, Lenient}` mirrors
`commercial_catalog::CommercialCatalogLoadMode`'s identical two-mode
precedent: `Strict` aborts the whole parse on the first malformed or
schema-mismatched line; `Lenient` collects `(line_number, reason)` pairs
into `ExecutionRecordLoadReport.rejected` and continues. Blank lines are
skipped, not counted as either accepted or rejected.

## Feature gating

Unconditional module, no new Cargo feature. The plain struct/enum
definitions (`SynthesisExecutionRecord`, `PlanIdentity`, `ActualStepDetail`,
`SynthesisOutcome`, etc.) always compile, `#[cfg_attr(feature = "serde",
derive(Serialize, Deserialize))]` on each, matching this crate's universal
pattern. `parse_execution_records`/`ExecutionRecordLoadMode`/
`ExecutionRecordLoadReport` are gated behind the existing `serde` feature
only, since JSON parsing structurally requires `serde_json` -- the same
gating `CommercialPrecursorCatalog::load_json` already uses.

Deliberately **not** gated behind the optional `commercial_catalog`
feature: `selected_commercial_offers` stores plain `Vec<String>` offer
ids, not `commercial_catalog::CommercialOfferId`, so this module stays
usable without that feature enabled. This also sidesteps a real
constraint: `CommercialOfferSelection`/`CommercialCombination` are
`Serialize`-only (no `Deserialize`, since both transitively carry
`Vec<&'static str>`), so a persisted, later-reloadable execution record
could not have embedded either type directly regardless.

## Non-goals, this phase

- No CLI subcommand.
- No wiring into `Planner`/`score_plan`/ranking -- that connection, if it
  ever ships, is Phase 27's, gated on Phase 25/26 data existing in
  sufficient volume, and only after independent-data reproduction of any
  claimed effect.
- No structured validation on `ExecutionCharacterization`'s numeric
  fields (plain `f64`, not a validated newtype) -- this module is
  library-only with no CLI/untrusted-input boundary yet; add validation
  once a real boundary exists to validate against.
