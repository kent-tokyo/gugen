//! gugen (具現): explainable materials synthesis and process planning.
//!
//! This crate turns a target inorganic composition (and optionally a target
//! structure) into candidate precursor sets, balanced reactions, and
//! solid-state process plans — each with its evidence, assumptions, and
//! unresolved conditions kept explicit. It does not predict experimental
//! success. See `AGENTS.md` for the full specification and
//! `docs/scientific_scope.md` for what is and is not in scope.
//!
//! This implements all 9 phases of the roadmap in `AGENTS.md` §26: typed
//! errors, validated numeric types, composition, target specification, the
//! public report schema, provenance, provider trait boundaries, exact
//! reaction balancing, bounded precursor-set search, a solid-state process
//! template, plan scoring/confidence, end-to-end orchestration via
//! `Planner`, an optional `mikiwame` structural-diagnostics adapter, a
//! CLI (`src/bin/gugen.rs`: `plan`, `balance`, `explain`, `validate-target`,
//! `doctor`, `batch`), a validation suite against curated literature
//! fixtures (`tests/`, `docs/benchmark_report.md`), and v0.1 release
//! preparation (license files, docs.rs metadata, a dependency license
//! audit — see `tasks/todo.md`'s Phase 9 section). The `chematic-crystal`
//! adapter remains blocked on that crate's publication, and two
//! validation findings are documented rather than fixed (see
//! `tasks/todo.md`'s Phase 8 section). **v0.1.0 is published** (crates.io,
//! merged to `main`, tagged `v0.1.0`) — see `tasks/todo.md`'s Phase 9
//! section for the release record. Post-v0.1 development toward v0.2.0
//! (Phase 10 onward — real literature-sourced process conditions, a
//! large-scale blind benchmark, a second route family, a thermodynamic-
//! provider adapter boundary) is tracked from `tasks/todo.md`'s Phase 10
//! section onward, not `AGENTS.md` §26 (which only defines the original 9
//! phases).

#![forbid(unsafe_code)]

mod balance;
mod composition;
mod config;
mod error;
mod evidence;
mod frac;
mod literature_conditions;
#[cfg(feature = "mikiwame")]
mod mikiwame_adapter;
mod planner;
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
pub use literature_conditions::{CuratedConditionRecord, InMemoryLiteratureConditionProvider};
#[cfg(feature = "mikiwame")]
pub use mikiwame_adapter::{StructuralDiagnosticEffects, structural_effects};
pub use planner::Planner;
pub use precursor::{
    AcceptedPrecursorSet, AvailabilityMetadata, InMemoryPrecursorCatalog, PrecursorCandidate,
    PrecursorId, PrecursorSearchOutcome, PrecursorSelection, search_precursor_sets,
};
pub use process::{
    Atmosphere, CharacterizationMethod, ConditionPrecedent, CoolingMode, DurationRange,
    FormingMethod, GrindingMethod, HeatingPurpose, InertGas, MaterialAmount, MixingMethod,
    PlannedStep, PressureRange, ProcessPrecedent, ProcessStep, ProcessTemplateResult,
    RampRateRange, ReducingAgent, RouteFamily, StepRequirement, TemperatureRange,
    conventional_solid_state_template,
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
