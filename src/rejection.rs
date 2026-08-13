use crate::precursor::PrecursorId;

/// Closed set of rejection reasons (AGENTS.md §14).
/// `ThermodynamicDataUnavailable` must not by itself force a reject —
/// callers may downgrade it to a warning or lowered confidence instead
/// (AGENTS.md §13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RejectionCode {
    NoStoichiometricBalance,
    MissingTargetElement,
    ForbiddenElementPresent,
    PrecursorCountExceeded,
    UnsupportedByproductRequired,
    AtmosphereConflict,
    UserConstraintViolation,
    HazardPolicyBlocked,
    ThermodynamicDataUnavailable,
    SearchBudgetExhausted,
    DuplicatePlan,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RejectedCandidate {
    pub precursors: Vec<PrecursorId>,
    pub reason_codes: Vec<RejectionCode>,
    pub explanation: String,
}
