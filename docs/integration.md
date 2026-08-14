# Integration Boundary: chematic-crystal and mikiwame (Phase 0)

## Status, verified 2026-08-13; mikiwame re-verified 2026-08-14

- **chematic-crystal**: not published to crates.io, no matching GitHub repo
  found (search covered both crates.io's search API and GitHub's repository
  search API; see `docs/competitors.md` §1). The `kent-tokyo` org's existing
  `chematic` crate family (10 published crates) is organic/molecular
  cheminformatics — SMILES, SDF/MOL, fingerprints, force fields — not
  periodic/crystal structures. `chematic-crystal` is therefore genuinely
  unavailable right now, consistent with AGENTS.md §5's "並行開発中である
  可能性" framing, not a naming mistake on gugen's side.
- **mikiwame**: published to crates.io as of 2026-08-14 (`mikiwame`
  v0.1.0, `kent-tokyo`). Phase 6 consumes it as an optional feature-gated
  adapter (`src/mikiwame_adapter.rs`) — see below for the implemented
  mapping and its scope.
- **Materials Project**: no publication check applies here — Phase 13's
  adapter consumes only pre-fetched, caller-supplied data (no live API
  call from gugen itself), so there is nothing to verify as "published" or
  "available"; see below for what was verified instead (real field names).

chematic-crystal's unavailability doesn't block gugen development.
AGENTS.md §5 states exactly this contingency and prescribes the
trait-boundary path below.

## `TargetMaterialView`: the boundary while chematic-crystal is unavailable

```rust
pub trait TargetMaterialView {
    fn composition(&self) -> CompositionView<'_>;
    fn structure_metadata(&self) -> Option<StructureMetadataView<'_>>;
}
```

Rules for this boundary (Phase 1 implements it; stated here so the shape is
agreed before code exists):

- `gugen` defines the trait and the minimal `CompositionView` /
  `StructureMetadataView` types it needs (element, count/fraction, optional
  lattice summary) — nothing resembling a full crystal structure
  implementation. AGENTS.md §5 explicitly forbids gugen growing its own
  large structure implementation.
- A `chematic-crystal` adapter (`src/adapters/chematic.rs`, feature-gated)
  will implement this trait over the real `PeriodicStructure`/`Lattice`/
  `PeriodicSite`/`Composition` types once that crate exists and its API is
  confirmed. Until then, gugen's own minimal in-crate type implements the
  trait so the rest of the pipeline (precursor search, balancing, ranking)
  can be built and tested without waiting.
- Swapping the adapter in later must not change any other module's public
  API — everything downstream of `target.rs` consumes
  `dyn TargetMaterialView` (or a generic bound), never a concrete chematic
  type directly.
- If chematic-crystal's eventual API diverges significantly from the
  `PeriodicStructure`/`Lattice`/`composition`/provenance shape sketched in
  AGENTS.md §5, that is a Phase 6 stop-and-report item ("chematic-crystal
  APIが未確定で重大な密結合が必要", AGENTS.md §28), not a Phase 0 blocker —
  the trait boundary is designed precisely to absorb that uncertainty.

## mikiwame: optional feature, not a dependency

```toml
[features]
mikiwame = ["dep:mikiwame"]
```

- Off by default. With the feature disabled, gugen builds, runs, and
  produces full reports with zero references to mikiwame types anywhere in
  the compiled core (verified: `--no-default-features` test run has no
  `mikiwame_adapter` module).
- Implemented in `src/mikiwame_adapter.rs` (Phase 6), mapping
  `mikiwame::MaterialDiagnosticReport` to gugen-native effects
  (AGENTS.md §5):
  - `InvalidInput` / `StrongAnomalyDetected` verdict → `abstain_reason`
    (`Some`), for the caller to stop planning for that target rather than
    return a low-confidence plan.
  - `Severity::High`/`Critical` finding → a `Severe` `PlanningWarning`.
  - Low `ApplicabilityLevel` → contributes to `confidence_penalty`
    (`Score01`), never a hard reject by itself.
  - Any non-`Info` finding → a `PlanningWarning` (severity mapped from
    mikiwame's `Severity`).
  - Oxidation-state ambiguity → mikiwame v0.1 has no `FindingCode` for
    this yet, so this integration point is unreachable, not implemented
    as a no-op. Revisit once mikiwame exposes it.
- **Not wired into `Planner::plan`.** `mikiwame::analyze` needs a real
  `mikiwame::PeriodicStructureView` (lattice + sites); gugen's own
  `TargetStructure` is still free text, and building one depends on
  `chematic-crystal`, which remains unpublished. `structural_effects()` is
  exposed as a standalone function for a caller with its own structure
  data to call directly and apply the result (e.g. to a `SynthesisPlan`'s
  `confidence`/`warnings`) themselves.

## Materials Project: pre-fetched snapshot only, no live client (Phase 13)

`src/materials_project_adapter.rs`, feature-gated (`materials_project`
Cargo feature, declaring zero new dependencies -- see the module's own doc
comment). `MaterialsProjectSnapshotProvider` implements
`ThermodynamicProvider` entirely over a `Vec<CompetingPhase>` the caller
already has; this crate never queries `api.materialsproject.org`, never
holds an API key, and has no notion of "refresh" or "stale."

- `reaction_energy`: arithmetic over the snapshot (weighted formula-unit
  energies, normalized per atom -- see the function's own doc comment for
  the exact convention), not a lookup. Returns `Ok(None)` -- never a
  partial sum -- the moment any reactant or product's exact `Composition`
  isn't in the snapshot.
- `competing_phases` (the new Phase 13 default method on
  `ThermodynamicProvider`): every snapshot entry sharing at least one
  element with the target, excluding the target's own composition.
  Evidence-only, like `reaction_energy` -- neither is converted into a
  selectivity or favorability score (AGENTS.md §4.3); `Planner::plan`
  attaches a non-empty result as one more `EvidenceKind::ThermodynamicData`
  entry with an explicit "does not account for kinetics, particle size, or
  atmosphere" limitation.

**No formula parser exists in gugen** (`Composition` has no `Display`/
`FromStr`, and none is planned -- see `composition.rs`'s own doc comment).
`CompetingPhase`'s input shape is explicit element/amount pairs, matching
`Composition::new`'s own shape, not a `formula_pretty: String` field --
converting a formula into that shape is the caller's job, during their own
pre-fetch step, before any of this data reaches gugen.

Field names below were verified directly against Materials Project's own
API documentation (`mp-api`'s `SummaryRester` reference and
`materialsproject/mapidoc`), not recalled from memory (AGENTS.md §21.3):
the summary endpoint's `formula_pretty` (e.g. `"Fe2O3"`) and
`formation_energy_per_atom` fields, both in eV/atom. A worked conversion
for one such entry, hand-written rather than through any string parser
(the exact reason gugen doesn't ship one -- Hill-notation parsing has
enough edge cases, e.g. implicit `1` subscripts and element-order
conventions, that it isn't a "few lines" the "already-installed
dependency" ladder would justify skipping a real parsing crate for):

```rust
// From an MP summary response with formula_pretty = "Fe2O3" and
// formation_energy_per_atom = -2.5:
use gugen::{CompetingPhase, Composition, Element};

let composition = Composition::new([
    (Element::new("Fe").unwrap(), 2.0),
    (Element::new("O").unwrap(), 3.0),
])
.unwrap();
let entry = CompetingPhase::new(composition, -2.5).unwrap();
```

**Not wired into any automatic fetch path** -- same non-goal as the
mikiwame section above. A caller constructs
`MaterialsProjectSnapshotProvider::from_entries(..)` from data they already
retrieved and passes it to `Planner::new`, exactly like any other
`ThermodynamicProvider`.

## What Phase 0 is *not* deciding

The exact field-level shape of `CompositionView`/`StructureMetadataView`
and the mikiwame diagnostic trait are Phase 1/Phase 6 work, respectively.
Phase 0's job was confirming *whether* to build against real types or a
boundary (boundary, confirmed above) — not finalizing that boundary's
fields before any downstream module has exercised it.
