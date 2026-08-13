//! gugen (具現): explainable materials synthesis and process planning.
//!
//! This crate turns a target inorganic composition (and optionally a target
//! structure) into candidate precursor sets, balanced reactions, and
//! solid-state process plans — each with its evidence, assumptions, and
//! unresolved conditions kept explicit. It does not predict experimental
//! success. See `AGENTS.md` for the full specification and
//! `docs/scientific_scope.md` for what is and is not in scope.
//!
//! This implements Phases 1-5 of the roadmap in `AGENTS.md` §26: typed
//! errors, validated numeric types, composition, target specification, the
//! public report schema, provenance, provider trait boundaries, exact
//! reaction balancing, bounded precursor-set search, a solid-state process
//! template, and plan scoring/confidence. The `Planner` type that
//! orchestrates all of this end-to-end lands in Phase 6 (see
//! `tasks/todo.md`).

#![forbid(unsafe_code)]

mod balance;
mod composition;
mod config;
mod error;
mod evidence;
mod frac;
mod precursor;
mod process;
mod provenance;
mod provider;
mod reaction;
mod rejection;
mod report;
mod score;
mod target;

pub use balance::{balance, curated_byproducts};
pub use composition::{Composition, ELEMENT_SYMBOLS, Element};
pub use config::{PlanningConfig, SearchBudget};
pub use error::{GugenError, ProviderError, Result};
pub use evidence::{EvidenceKind, EvidenceScope, EvidenceStrength, PlanningEvidence};
pub use precursor::{
    AcceptedPrecursorSet, AvailabilityMetadata, InMemoryPrecursorCatalog, PrecursorCandidate,
    PrecursorId, PrecursorSearchOutcome, PrecursorSelection, search_precursor_sets,
};
pub use process::{
    Atmosphere, CharacterizationMethod, CoolingMode, DurationRange, FormingMethod, GrindingMethod,
    HeatingPurpose, InertGas, MaterialAmount, MixingMethod, PlannedStep, PressureRange,
    ProcessPrecedent, ProcessStep, ProcessTemplateResult, RampRateRange, ReducingAgent,
    RouteFamily, StepRequirement, TemperatureRange, conventional_solid_state_template,
};
pub use provenance::PlanningProvenance;
pub use provider::{PrecursorCatalog, ProcessEvidenceProvider, ThermodynamicProvider};
pub use reaction::{BalancedReaction, ReactionEnergy, ReactionSpecies, ThermodynamicConditions};
pub use rejection::{RejectedCandidate, RejectionCode};
pub use report::{
    ApplicabilityAssessment, ApplicabilityLevel, PlanId, PlanningWarning, SCHEMA_VERSION,
    SynthesisPlan, SynthesisPlanningReport, TargetSummary, UnresolvedRequirement, WarningSeverity,
};
pub use score::{
    ConfidenceAssessment, PlanAssessment, PlanScoreBreakdown, PlanningAssumption, RankingWeights,
    Score01, ranking_weights_digest, score_plan,
};
pub use target::{
    PhaseRequirement, PlanningConstraints, TargetMaterialView, TargetSpecification, TargetStructure,
};
