use crate::composition::Composition;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PrecursorId(pub String);

impl std::fmt::Display for PrecursorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Minimal Phase 1 placeholder. Phase 3 adds availability, hazard/toxicity,
/// redox-compatibility, and atmosphere-compatibility metadata (AGENTS.md
/// §9, §15).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PrecursorCandidate {
    pub id: PrecursorId,
    pub composition: Composition,
}

/// One precursor's role in a candidate plan.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PrecursorSelection {
    pub precursor: PrecursorId,
    /// Integer formula units, matching `BalancedReaction`'s coefficient
    /// convention (AGENTS.md §10).
    pub formula_units: u64,
}
