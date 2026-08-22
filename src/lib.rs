//! gugen (具現): explainable materials synthesis and process planning.
//!
//! This crate turns a target inorganic composition (and optionally a target
//! structure) into candidate precursor sets, balanced reactions, and
//! solid-state process plans — each with its evidence, assumptions, and
//! unresolved conditions kept explicit. It does not predict experimental
//! success. See `docs/scientific_scope.md` for what is and is not in
//! scope, and `CHANGELOG.md` for the user-facing capability list.
//!
//! This implements typed errors, validated numeric types, composition,
//! target specification, the public report schema, provenance, provider
//! trait boundaries, exact
//! reaction balancing, bounded precursor-set search, a solid-state process
//! template, plan scoring/confidence, end-to-end orchestration via
//! `Planner`, an optional `mikiwame` structural-diagnostics adapter, a
//! CLI (`src/bin/gugen.rs`: `plan`, `balance`, `explain`, `validate-target`,
//! `doctor`, `batch`), a validation suite against curated literature
//! fixtures (`tests/`, `docs/benchmark_report.md`), and v0.1 release
//! preparation (license files, docs.rs metadata, a dependency license
//! audit — see `tasks/todo.md`'s Phase 9 section). The `chematic-crystal`
//! adapter remained blocked on that crate's publication through v0.1
//! (Phase 16 below addresses part of this), and two
//! validation findings are documented rather than fixed (see
//! `tasks/todo.md`'s Phase 8 section). **v0.1.0 is published** (crates.io,
//! merged to `main`, tagged `v0.1.0`) — see `tasks/todo.md`'s Phase 9
//! section for the release record. Post-v0.1 development toward v0.2.0
//! (Phase 10-14 — real literature-sourced process conditions, a
//! large-scale blind benchmark, a second route family, a thermodynamic-
//! provider adapter boundary, and a validation-fixture citation repair) is
//! tracked from `tasks/todo.md`'s Phase 10 section onward, not `AGENTS.md`
//! §26 (which only defines the original 9 phases). **v0.2.0 is published**
//! (crates.io, tagged `v0.2.0`). Work toward v0.3.0 has begun with Phase
//! 15A (`route_suitability` module): a report-level evidence model for
//! whether a route family suits a target (`Supports`/`Contradicts`/
//! `Unknown` findings, never an aggregated score). Phase 15B added
//! `derive_recommendation`, a pure function deriving a discrete
//! `RouteRecommendation` from that evidence, and wired only its
//! `NotRecommended` state into `Planner::plan`: a plan with strong,
//! uncontested contradicting evidence is moved from `plans` into the new
//! `SynthesisPlanningReport.not_recommended` (kept, with its findings, not
//! dropped), and a target where every generated plan is excluded this way
//! abstains explicitly via `unresolved` rather than returning an empty
//! success. No numeric score is affected by either phase. Phase 16 added
//! an optional `chematic_crystal` feature: `to_mikiwame_structure`
//! converts a caller-supplied `chematic_crystal::PeriodicStructure` (now
//! published, 0.15.0) into a `mikiwame::OwnedStructure`, closing the
//! specific conversion gap the mikiwame adapter had named since Phase 6.
//! Still not auto-wired into `Planner::plan` -- `TargetSpecification` has
//! no geometry field, so a caller still applies the result themselves.
//! Phase 17 audited how much literature evidence for route suitability
//! actually exists in a real synthesis corpus, and evaluated
//! `derive_recommendation` against a hand-verified holdout record --
//! explicitly not a route-family prediction-accuracy benchmark (see
//! `docs/route_suitability_corpus_audit.md`); no production code changed.
//! Phases 15A/15B/16/17 together are v0.3.0's planned development work.
//! **v0.3.0 is published** (crates.io, tagged `v0.3.0`). Post-v0.3.0
//! development toward v0.4.0 added finite-temperature Gibbs-energy
//! estimation for gas-free solid systems (Phase 19P/19P.1), deliberately
//! not connected to ranking (`thermodynamic_support` stays `None`); a
//! bulk literature-corpus snapshot loader and exact-match observation
//! provider (Phase 20B); cross-DOI field comparison across independent
//! sources for that corpus (Phase 20C); a manual extraction-accuracy
//! audit against original source papers (Phase 20D); and Integration,
//! which surfaces that cross-DOI evidence on
//! `SynthesisPlan.literature_evidence` for reference-only display --
//! never auto-filling `ProcessStep` conditions and never affecting
//! `score`/`confidence`/ranking. Together these are v0.4.0's planned
//! development work: gas-free solid finite-temperature thermodynamic
//! primitives, a bulk literature observation snapshot API, cross-DOI
//! agreement/conflict classification, and reference-only literature
//! evidence in `Planner` -- an evidence-infrastructure release, not a
//! ranking-accuracy or synthesis-success-prediction claim.
//! **v0.4.2 is published** (crates.io, tagged `v0.4.2`) -- adds the
//! optional, off-by-default `commercial_catalog` feature (Commercial
//! Precursor Catalog: matches an existing `SynthesisPlan`'s precursors
//! against a caller-supplied catalog of commercial offers, as
//! post-planning processing that never affects the plan's score,
//! confidence, reaction, or process steps). See `CHANGELOG.md` for the
//! user-facing summary.

#![forbid(unsafe_code)]

mod balance;
#[cfg(feature = "chematic_crystal")]
mod chematic_crystal_adapter;
#[cfg(feature = "commercial_catalog")]
mod commercial_catalog;
mod composition;
mod config;
mod error;
mod evidence;
mod frac;
mod literature_conditions;
mod literature_evidence;
#[cfg(feature = "literature_corpus")]
mod literature_observation_conflicts;
#[cfg(feature = "literature_corpus")]
mod literature_observations;
#[cfg(feature = "materials_project")]
mod materials_project_adapter;
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
mod route_suitability;
mod score;
mod target;
mod thermodynamics;

pub use balance::{balance, curated_byproducts};
#[cfg(feature = "chematic_crystal")]
pub use chematic_crystal_adapter::to_mikiwame_structure;
#[cfg(feature = "commercial_catalog")]
pub use commercial_catalog::{
    AvailabilityStatus, CasNumber, CommercialCatalogError, CommercialCatalogLoadMode,
    CommercialCatalogLoadReport, CommercialCombination, CommercialExclusion,
    CommercialExclusionCode, CommercialOfferId, CommercialOfferSelection, CommercialPlanAssessment,
    CommercialPlanningConfig, CommercialPlanningRequest, CommercialPrecursorCatalog,
    CommercialPrecursorOffer, CommercialSourceType, CommercialWarning, CurrencyCode,
    MissingCommercialDataPolicy, Money, OfferProvenance, PackageMass, ParticleSizeRangeUm,
    PurityFraction, RejectedOffer, SearchBudgetSummary, UnresolvedCommercialField,
    assess_commercial_plans, assess_commercial_precursors,
};
pub use composition::{Composition, ELEMENT_SYMBOLS, Element};
pub use config::{PlanningConfig, SearchBudget};
pub use error::{GugenError, ProviderError, Result};
pub use evidence::{EvidenceKind, EvidenceScope, EvidenceStrength, PlanningEvidence};
pub use literature_conditions::{CuratedConditionRecord, InMemoryLiteratureConditionProvider};
pub use literature_evidence::{
    CrossDoiFieldStatus, LiteratureRouteEvidence, RouteObservationAssessment, SourcedValue,
    StepGroupAssessment, StepGroupKey, literature_evidence_limitations,
};
#[cfg(feature = "literature_corpus")]
pub use literature_observation_conflicts::LiteratureObservationCorpusProvider;
#[cfg(feature = "literature_corpus")]
pub use literature_observations::{
    CORPUS_SNAPSHOT_SCHEMA_VERSION, CorpusHeatingObservation, CorpusManifest,
    LiteratureObservationCorpus, LoadMode, LoadReport, RejectedObservation,
};
#[cfg(feature = "materials_project")]
pub use materials_project_adapter::MaterialsProjectSnapshotProvider;
#[cfg(feature = "mikiwame")]
pub use mikiwame_adapter::{StructuralDiagnosticEffects, structural_effects};
pub use planner::{Planner, PlannerBuilder};
pub use precursor::{
    AcceptedPrecursorSet, AvailabilityMetadata, InMemoryPrecursorCatalog, PrecursorCandidate,
    PrecursorId, PrecursorSearchOutcome, PrecursorSelection, search_precursor_sets,
};
pub use process::{
    Atmosphere, CharacterizationMethod, ConditionConflict, ConditionPrecedent, CoolingMode,
    DurationRange, FormingMethod, GrindingMethod, HeatingPurpose, InertGas, MaterialAmount,
    MixingMethod, PlannedStep, PressureRange, ProcessPrecedent, ProcessStep, ProcessTemplateResult,
    RampRateRange, ReducingAgent, RouteFamily, StepRequirement, TemperatureRange,
    applicable_route_family_templates, conventional_solid_state_template, mechanochemical_template,
};
pub use provenance::PlanningProvenance;
pub use provider::{
    LiteratureEvidenceProvider, PrecursorCatalog, ProcessEvidenceProvider,
    RouteSuitabilityProvider, ThermodynamicProvider,
};
pub use reaction::{
    BalancedReaction, CompetingPhase, ReactionEnergy, ReactionSpecies, ThermodynamicConditions,
};
pub use rejection::{RejectedCandidate, RejectionCode};
pub use report::{
    ApplicabilityAssessment, ApplicabilityLevel, NotRecommendedPlan, PlanId, PlanningWarning,
    SCHEMA_VERSION, SynthesisPlan, SynthesisPlanningReport, TargetSummary, UnresolvedRequirement,
    WarningSeverity,
};
pub use route_suitability::{
    CuratedSuitabilityRecord, InMemoryRouteSuitabilityProvider, RouteRecommendation,
    RouteSuitabilityAssessment, SuitabilityFinding, SuitabilityVerdict, derive_recommendation,
};
pub use score::{
    ConfidenceAssessment, PlanAssessment, PlanScoreBreakdown, PlanningAssumption, RankingWeights,
    Score01, ranking_weights_digest, score_plan,
};
pub use target::{
    PhaseRequirement, PlanningConstraints, TargetMaterialView, TargetSpecification, TargetStructure,
};
pub use thermodynamics::{
    DecompositionComparison, Kelvin, SolidThermodynamicEntry, ThermodynamicDatasetIdentity,
    ThermodynamicSelectivityAssessment, balanced_reaction_delta_ev_per_atom,
    decomposition_margin_ev_per_atom, reduced_mass_amu, relative_solid_gibbs_ev_per_atom,
};
