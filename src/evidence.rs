/// AGENTS.md §7: every proposal must point at *why* it's there, and the
/// "why" is one of a closed set of kinds. If nothing here applies, the
/// evidence kind is `RuleBased` with `source_id: None` -- stated
/// explicitly, never a gap left implicit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EvidenceKind {
    StoichiometricBalance,
    RuleBased,
    ThermodynamicData,
    UserProvidedPrecedent,
    CuratedLiteratureRecord,
    SimilarComposition,
    SimilarStructure,
    ProcessTemplate,
    SafetyConstraint,
}

/// Categorical strength of one piece of evidence. Distinct from
/// `PlanScoreBreakdown.evidence_strength: Score01` (AGENTS.md §13), which is
/// a plan-level *aggregate* across many evidence items with a weighted,
/// documented rationale -- that aggregation is Phase 5 work. This type must
/// not grow an unstated enum-to-number mapping before then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EvidenceStrength {
    Weak,
    Moderate,
    Strong,
}

/// How directly a piece of evidence applies to the plan it's attached to,
/// versus having been generalized from a related case (AGENTS.md §7's
/// `SimilarComposition`/`SimilarStructure` evidence kinds exist precisely
/// because that distinction matters and must not be hidden).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EvidenceScope {
    ExactTarget,
    SimilarMaterial,
    GeneralRule,
}

/// AGENTS.md §7. `source_id` must never carry a fabricated DOI, paper
/// title, patent number, or URL -- only what an evidence provider actually
/// returned.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlanningEvidence {
    pub kind: EvidenceKind,
    pub source_id: Option<String>,
    pub statement: String,
    pub strength: EvidenceStrength,
    pub applicable_to: EvidenceScope,
    pub limitations: Vec<String>,
}
