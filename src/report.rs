use crate::composition::Composition;
use crate::evidence::PlanningEvidence;
use crate::literature_evidence::LiteratureRouteEvidence;
use crate::precursor::PrecursorSelection;
use crate::process::{PlannedStep, RouteFamily};
use crate::provenance::PlanningProvenance;
use crate::reaction::BalancedReaction;
use crate::rejection::RejectedCandidate;
use crate::route_suitability::{RouteSuitabilityAssessment, SuitabilityFinding};
use crate::score::{ConfidenceAssessment, PlanScoreBreakdown, PlanningAssumption};

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

/// A candidate synthesis plan (AGENTS.md §6). `steps` is `Vec<PlannedStep>`
/// rather than the bare `Vec<ProcessStep>` AGENTS.md §6 shows, so each step
/// can carry the `StepRequirement` §11 mandates. `manual_review_required`
/// isn't in §6's snippet, but §15 requires the v0.1 JSON plan to carry it
/// (or an equivalent) regardless -- see [`crate::score_plan`] for why it's
/// always `true` in v0.1.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SynthesisPlan {
    pub plan_id: PlanId,
    pub route_family: RouteFamily,
    pub precursors: Vec<PrecursorSelection>,
    pub balanced_reaction: Option<BalancedReaction>,
    pub steps: Vec<PlannedStep>,
    pub score: PlanScoreBreakdown,
    pub confidence: ConfidenceAssessment,
    pub applicability: ApplicabilityAssessment,
    pub evidence: Vec<PlanningEvidence>,
    pub warnings: Vec<PlanningWarning>,
    pub assumptions: Vec<PlanningAssumption>,
    pub unresolved: Vec<UnresolvedRequirement>,
    pub manual_review_required: bool,
    /// Reference-only cross-DOI literature evidence for this plan's exact
    /// (target, precursors, route_family) (v0.4.0 Integration), from a
    /// configured `LiteratureEvidenceProvider`. `None` whenever no
    /// provider is configured, the provider found no matching route, or
    /// this plan's route family isn't `ConventionalSolidState` (the only
    /// route family the underlying corpus has evidence for). Never
    /// derived from or fed into `score`, `confidence`, `evidence`, or
    /// `steps` -- see `literature_evidence.rs`'s module doc comment for
    /// why that's structural, not a convention this field happens to
    /// follow. A field/route showing `Conflict` or
    /// `has_multiple_operation_shapes` here is a *disclosure*, not a
    /// planning failure -- it is never auto-resolved, and its presence
    /// alone must never be read as "condition accuracy improved" (it
    /// means more literature coverage was found, not that any specific
    /// value here is correct).
    pub literature_evidence: Option<LiteratureRouteEvidence>,
}

/// A plan excluded from the recommended list by route-suitability findings
/// (Phase 15B) -- `plan` is unchanged from what would otherwise have
/// appeared in `SynthesisPlanningReport.plans` (same score/confidence/
/// evidence, since filtering happens after scoring, not instead of it);
/// `contradicting_findings` is just the `Contradicts` findings that
/// triggered exclusion (not the full assessment, which may also carry
/// `Supports`/`Unknown` findings for other purposes).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NotRecommendedPlan {
    pub plan: SynthesisPlan,
    pub contradicting_findings: Vec<SuitabilityFinding>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SynthesisPlanningReport {
    pub schema_version: u32,
    pub target: TargetSummary,
    pub applicability: ApplicabilityAssessment,
    /// One entry per `RouteFamily` variant a `RouteSuitabilityProvider` was
    /// asked about (Phase 15A) -- target-level, like `applicability`, not
    /// per-plan, since suitability doesn't depend on which precursor set a
    /// given `SynthesisPlan` used. Correlate a specific plan to its
    /// assessment via `SynthesisPlan.route_family`. Always empty when no
    /// provider is configured (e.g. `Planner::offline_minimal`) -- carries
    /// no ranking weight in this phase; nothing in `score.rs` reads it.
    pub route_suitability: Vec<RouteSuitabilityAssessment>,
    pub plans: Vec<SynthesisPlan>,
    /// Plans that were built (valid chemistry, a real balanced reaction and
    /// process template) but excluded from `plans` because
    /// `route_suitability::derive_recommendation` returned `NotRecommended`
    /// for that plan's route family (Phase 15B) -- kept here, with the
    /// specific findings that triggered exclusion, rather than dropped
    /// silently. Always empty when no `RouteSuitabilityProvider` is
    /// configured, or when every assessed route family clears the bar.
    pub not_recommended: Vec<NotRecommendedPlan>,
    pub rejected_candidates: Vec<RejectedCandidate>,
    pub unresolved: Vec<UnresolvedRequirement>,
    pub warnings: Vec<PlanningWarning>,
    pub provenance: PlanningProvenance,
}
