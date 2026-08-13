use crate::composition::{Composition, Element};
use std::collections::BTreeSet;

/// Minimal structural hint about the target, standing in until a
/// `chematic-crystal` adapter can provide the real `PeriodicStructure`/
/// `Lattice` types. gugen does not implement its own crystal-structure
/// representation (AGENTS.md §5, docs/integration.md).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TargetStructure {
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PhaseRequirement {
    pub phase_name: String,
}

/// User-supplied constraints on the planning search. Deliberately minimal
/// in Phase 1 — Phase 3 (precursor enumeration) adds the rest of the
/// filters listed in AGENTS.md §9 alongside the search that consumes them.
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlanningConstraints {
    pub forbidden_elements: BTreeSet<Element>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TargetSpecification {
    pub composition: Composition,
    pub structure: Option<TargetStructure>,
    pub desired_phase: Option<PhaseRequirement>,
    pub constraints: PlanningConstraints,
}

/// Boundary trait gugen depends on instead of a concrete crystal-structure
/// type (AGENTS.md §5). Implemented directly by [`TargetSpecification`]
/// today; a future `chematic-crystal` adapter implements it over the real
/// structure types without changing anything downstream of `target.rs`
/// (docs/integration.md).
pub trait TargetMaterialView {
    fn composition(&self) -> &Composition;
    fn structure_metadata(&self) -> Option<&TargetStructure>;
}

impl TargetMaterialView for TargetSpecification {
    fn composition(&self) -> &Composition {
        &self.composition
    }

    fn structure_metadata(&self) -> Option<&TargetStructure> {
        self.structure.as_ref()
    }
}
