# Integration Boundary: chematic-crystal and mikiwame (Phase 0)

## Status, verified 2026-08-13

- **chematic-crystal**: not published to crates.io, no matching GitHub repo
  found (search covered both crates.io's search API and GitHub's repository
  search API; see `docs/competitors.md` §1). The `kent-tokyo` org's existing
  `chematic` crate family (10 published crates) is organic/molecular
  cheminformatics — SMILES, SDF/MOL, fingerprints, force fields — not
  periodic/crystal structures. `chematic-crystal` is therefore genuinely
  unavailable right now, consistent with AGENTS.md §5's "並行開発中である
  可能性" framing, not a naming mistake on gugen's side.
- **mikiwame**: not published to crates.io, no matching repo under
  `kent-tokyo`. One unrelated same-named GitHub repo exists elsewhere with
  no description or affiliation — not a collision, just noted for later
  re-checking before any publish.

Neither blocks gugen development. AGENTS.md §5 states exactly this
contingency and prescribes the trait-boundary path below.

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

- Off by default. With the feature disabled, gugen must build, run, and
  produce full reports with zero references to mikiwame types anywhere in
  the compiled core.
- Integration points (AGENTS.md §5), all *consuming* mikiwame's diagnostic
  output, never reimplementing its diagnostic logic:
  - `InvalidInput` finding → stop planning for that target, return a
    rejection/abstention rather than a low-confidence plan.
  - Severe site overlap → stop planning.
  - Oxidation-state ambiguity → propagate into plan branching (multiple
    candidate oxidation-state assumptions become multiple plans/assumptions,
    not a silently resolved single guess).
  - Low applicability (from mikiwame) → lower `ConfidenceAssessment`, do not
    hard-reject by itself.
  - Structural anomaly → carried through as a `PlanningWarning`.
- Because mikiwame doesn't exist yet either, the adapter
  (`src/adapters/mikiwame.rs`) is written against the same kind of minimal
  trait boundary pattern once mikiwame's diagnostic-output shape is known
  well enough to define one — tracked in `tasks/todo.md` as a Phase 6 item,
  not designed further in Phase 0 since guessing at an unpublished crate's
  API shape now would likely just be thrown away.

## What Phase 0 is *not* deciding

The exact field-level shape of `CompositionView`/`StructureMetadataView`
and the mikiwame diagnostic trait are Phase 1/Phase 6 work, respectively.
Phase 0's job was confirming *whether* to build against real types or a
boundary (boundary, confirmed above) — not finalizing that boundary's
fields before any downstream module has exercised it.
