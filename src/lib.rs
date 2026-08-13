//! gugen (具現): explainable materials synthesis and process planning.
//!
//! This crate turns a target inorganic composition (and optionally a target
//! structure) into candidate precursor sets, balanced reactions, and
//! solid-state process plans — each with its evidence, assumptions, and
//! unresolved conditions kept explicit. It does not predict experimental
//! success. See `AGENTS.md` for the full specification and
//! `docs/scientific_scope.md` for what is and is not in scope.
//!
//! This implements Phases 1-2 of the roadmap in `AGENTS.md` §26: typed
//! errors, validated numeric types, composition, target specification, the
//! public report schema, provenance, provider trait boundaries, and exact
//! reaction balancing. Precursor search, process templating, and ranking
//! land in later phases (see `tasks/todo.md`).

#![forbid(unsafe_code)]

mod balance;
mod composition;
mod config;
mod error;
mod frac;
mod precursor;
mod process;
mod provenance;
mod provider;
mod reaction;
mod rejection;
mod report;
mod target;

pub use balance::{balance, curated_byproducts};
pub use composition::{Composition, ELEMENT_SYMBOLS, Element};
pub use config::{PlanningConfig, SearchBudget};
pub use error::{GugenError, ProviderError, Result};
pub use precursor::{PrecursorCandidate, PrecursorId, PrecursorSelection};
pub use process::{
    DurationRange, PressureRange, ProcessPrecedent, RampRateRange, TemperatureRange,
};
pub use provenance::PlanningProvenance;
pub use provider::{PrecursorCatalog, ProcessEvidenceProvider, ThermodynamicProvider};
pub use reaction::{BalancedReaction, ReactionEnergy, ReactionSpecies, ThermodynamicConditions};
pub use rejection::{RejectedCandidate, RejectionCode};
pub use report::{
    ApplicabilityAssessment, ApplicabilityLevel, PlanId, PlanningWarning, SCHEMA_VERSION,
    SynthesisPlan, SynthesisPlanningReport, TargetSummary, UnresolvedRequirement, WarningSeverity,
};
pub use target::{
    PhaseRequirement, PlanningConstraints, TargetMaterialView, TargetSpecification, TargetStructure,
};
