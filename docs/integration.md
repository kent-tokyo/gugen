# Integration Boundary: chematic-crystal and mikiwame (Phase 0)

## Status

Verified 2026-08-13; mikiwame re-verified 2026-08-14; chematic-crystal
re-verified 2026-08-14 (Phase 16, against the real published 0.15.0
source, not docs.rs summaries alone).

- **chematic-crystal**: published to crates.io as 0.15.0 on 2026-08-14
  (`kent-tokyo`). It is a **pure geometry** crate: `Lattice`,
  `PeriodicSite`/`SiteSpecies`/`Occupancy`, `PeriodicStructure`, neighbor/
  displacement/supercell math. Confirmed directly from the vendored
  source (`~/.cargo/registry/src/**/chematic-crystal-0.15.0/`), not
  guessed: **no symmetry** (space groups, Wyckoff positions, Niggli
  reduction), **no CIF parser**, **no composition-from-formula
  prediction**, **no polymorph identification**, and no `Composition` or
  structure-provenance type of its own (its own doc comment states this
  scope explicitly). This is narrower than AGENTS.md §5's original sketch
  (`PeriodicStructure`/`Lattice`/`PeriodicSite`/`composition`/`structure
  provenance`) in two respects -- no `composition` type, no provenance
  type -- but not a "significant divergence" triggering the AGENTS.md §28
  stop-and-report condition below: gugen's own `Composition` type already
  covers the first, and `PlanningEvidence.source_id` already covers the
  second. Phase 16 (`src/chematic_crystal_adapter.rs`) consumes it as an
  optional feature-gated bridge to `mikiwame` -- see the mikiwame section
  below for what that bridge does and does not unblock.
- **mikiwame**: published to crates.io as of 2026-08-14 (`mikiwame`
  v0.1.0, `kent-tokyo`). Phase 6 consumes it as an optional feature-gated
  adapter (`src/mikiwame_adapter.rs`) — see below for the implemented
  mapping and its scope. Does not itself depend on chematic-crystal; the
  two are fully independent structure representations (Phase 16's bridge
  exists precisely because of this).
- **Materials Project**: no publication check applies here — Phase 13's
  adapter consumes only pre-fetched, caller-supplied data (no live API
  call from gugen itself), so there is nothing to verify as "published" or
  "available"; see below for what was verified instead (real field names).

chematic-crystal's unavailability didn't block gugen development through
v0.1/v0.2. AGENTS.md §5 states exactly this contingency and prescribed the
trait-boundary path below; Phase 16 (below) is what actually happened once
the crate published, and it turned out narrower than the boundary was
designed to eventually absorb -- see that section.

## `TargetMaterialView`: the boundary built while chematic-crystal was unavailable

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
- **Not realized as sketched, once chematic-crystal actually published.**
  No `src/adapters/chematic.rs` implementing `TargetMaterialView` over
  chematic-crystal types exists. Phase 16 instead added
  `src/chematic_crystal_adapter.rs` (flat, matching `mikiwame_adapter.rs`'s
  and `materials_project_adapter.rs`'s established convention, not the
  nested path sketched here before either convention existed) -- a
  standalone conversion function, `to_mikiwame_structure`, bridging
  `chematic_crystal::PeriodicStructure` to `mikiwame::OwnedStructure` (see
  the mikiwame section below). `TargetMaterialView`/`TargetSpecification`
  were **not** changed: `TargetStructure` still carries free text only.
  Giving it a real geometry field was judged a separate, report-level
  wiring decision (how a report would represent per-target structural
  diagnostics), left out of Phase 16's scope -- see `src/target.rs`'s
  `TargetStructure` doc comment.
- Swapping a future adapter in must still not change any other module's
  public API — everything downstream of `target.rs` consumes
  `dyn TargetMaterialView` (or a generic bound), never a concrete chematic
  type directly.
- If chematic-crystal's eventual API diverges significantly from the
  `PeriodicStructure`/`Lattice`/`composition`/provenance shape sketched in
  AGENTS.md §5, that is a stop-and-report item ("chematic-crystal APIが
  未確定で重大な密結合が必要", AGENTS.md §28). Phase 16's re-check found a
  partial divergence -- 0.15.0 has no `composition` or provenance type of
  its own -- judged not severe enough to trigger this, since gugen's own
  `Composition` and `PlanningEvidence.source_id` already cover those two
  roles; see the Status section above.

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
- **Still not wired into `Planner::plan`, even after Phase 16.**
  `mikiwame::analyze` needs a real `mikiwame::PeriodicStructureView`
  (lattice + sites); gugen's own `TargetStructure` is still free text.
  Phase 16 closed the *construction* gap -- a caller with real
  chematic-crystal data no longer has to hand-build `mikiwame::Site`
  vectors, `to_mikiwame_structure` (`src/chematic_crystal_adapter.rs`,
  `chematic_crystal` feature) does it -- but `TargetSpecification` still
  has no field to carry that structure through the planning pipeline, so
  `Planner::plan` still can't run this automatically. `structural_effects()`
  is exposed as a standalone function for a caller with its own structure
  data (built directly, or via `to_mikiwame_structure`) to call directly
  and apply the result (e.g. to a `SynthesisPlan`'s `confidence`/
  `warnings`) themselves.

## chematic_crystal: optional feature, bridges to mikiwame (Phase 16)

```toml
[dependencies]
chematic-crystal = { version = "0.15.0", default-features = false, optional = true }

[features]
mikiwame = ["dep:mikiwame"]
chematic_crystal = ["dep:chematic-crystal", "mikiwame"]
```

- `chematic_crystal` implies `mikiwame` -- there is nothing useful gugen
  does with a `chematic_crystal::PeriodicStructure` on its own, since
  chematic-crystal itself has no structural diagnostics (see Status
  above). No `cfg(all(feature = ..., feature = ...))` needed anywhere as a
  result; `src/chematic_crystal_adapter.rs`'s `mod` declaration in
  `src/lib.rs` needs only `#[cfg(feature = "chematic_crystal")]`.
- `to_mikiwame_structure(&chematic_crystal::PeriodicStructure) ->
  mikiwame::OwnedStructure` is not a plain field-by-field copy -- two
  correctness details, both confirmed against the real 0.15.0/0.1.0
  source before being handled: (1) `chematic_crystal::PeriodicSite` allows
  multiple `SiteSpecies` for the same element at one position (disorder
  modeling) and a `0.0` occupancy is valid, which a naive flat-map would
  turn into a false-positive `SITE_DUPLICATE` finding in mikiwame -- same-
  element species are summed *within one `PeriodicSite` only* (never
  across separate sites) and dropped if the sum is exactly `0.0`; (2)
  `chematic_crystal::Lattice` accepts left-handed (negative-determinant)
  matrices as physically valid, which mikiwame treats as Critical
  `InvalidInput` -- corrected via an exact basis change (swap the `b`/`c`
  lattice rows and the matching fractional `y`/`z` component of every
  site), not a geometry-altering heuristic. Full detail and the test list
  proving both: `src/chematic_crystal_adapter.rs`'s own module doc
  comment.
- Not called automatically by `Planner::plan` -- same boundary as
  mikiwame's own section above.

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
