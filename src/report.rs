use crate::composition::Composition;
use crate::evidence::PlanningEvidence;
use crate::precursor::PrecursorSelection;
use crate::process::{PlannedStep, RouteFamily};
use crate::provenance::PlanningProvenance;
use crate::reaction::BalancedReaction;
use crate::rejection::RejectedCandidate;

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TargetSummary {
    pub composition: Composition,
    pub structure_present: bool,
    pub desired_phase: Option<String>,
}

/// Whether this planner can meaningfully handle the target at all
/// (AGENTS.md §16). Distinct from per-plan confidence: applicability is
/// about domain fit, not about how well-evidenced any particular plan is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ApplicabilityLevel {
    InDomain,
    PartiallyInDomain,
    OutOfDomain,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ApplicabilityAssessment {
    pub level: ApplicabilityLevel,
    pub rationale: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum WarningSeverity {
    Info,
    Caution,
    Severe,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlanningWarning {
    pub message: String,
    pub severity: WarningSeverity,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UnresolvedRequirement {
    pub description: String,
    pub reason: String,
}

/// Deterministic plan identifier (AGENTS.md §20: "plan IDを決定的にする").
/// Phase 5 derives this from plan contents; Phase 1 only needs the type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlanId(pub String);

impl std::fmt::Display for PlanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A candidate synthesis plan (AGENTS.md §6). `score`, `confidence`,
/// per-plan `applicability`, `assumptions`, and per-plan `unresolved` are
/// still missing -- they land in Phase 5 once ranking/confidence exist (see
/// tasks/todo.md). `steps` is `Vec<PlannedStep>` rather than the bare
/// `Vec<ProcessStep>` AGENTS.md §6 shows, so each step can carry the
/// `StepRequirement` §11 mandates.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SynthesisPlan {
    pub plan_id: PlanId,
    pub route_family: RouteFamily,
    pub precursors: Vec<PrecursorSelection>,
    pub balanced_reaction: Option<BalancedReaction>,
    pub steps: Vec<PlannedStep>,
    pub evidence: Vec<PlanningEvidence>,
    pub warnings: Vec<PlanningWarning>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SynthesisPlanningReport {
    pub schema_version: u32,
    pub target: TargetSummary,
    pub applicability: ApplicabilityAssessment,
    pub plans: Vec<SynthesisPlan>,
    pub rejected_candidates: Vec<RejectedCandidate>,
    pub unresolved: Vec<UnresolvedRequirement>,
    pub warnings: Vec<PlanningWarning>,
    pub provenance: PlanningProvenance,
}
