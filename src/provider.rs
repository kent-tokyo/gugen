use crate::composition::Composition;
use crate::error::ProviderError;
use crate::precursor::{PrecursorCandidate, PrecursorSelection};
use crate::process::ProcessPrecedent;
use crate::reaction::{BalancedReaction, ReactionEnergy, ThermodynamicConditions};
use crate::target::{PlanningConstraints, TargetSpecification};

/// Source of candidate precursor compounds for a target (AGENTS.md §8).
/// Core ships in-memory/JSON/fixture implementations only — no network
/// access lives in this crate (AGENTS.md §8, §25).
pub trait PrecursorCatalog {
    fn candidates_for(
        &self,
        target: &Composition,
        constraints: &PlanningConstraints,
    ) -> std::result::Result<Vec<PrecursorCandidate>, ProviderError>;
}

/// Source of reaction-energy estimates (AGENTS.md §8). Returning `Ok(None)`
/// means "no data available," which must not by itself reject a plan
/// (AGENTS.md §13, §14 — see `RejectionCode::ThermodynamicDataUnavailable`).
pub trait ThermodynamicProvider {
    fn reaction_energy(
        &self,
        reaction: &BalancedReaction,
        conditions: &ThermodynamicConditions,
    ) -> std::result::Result<Option<ReactionEnergy>, ProviderError>;
}

/// Source of process-condition precedent for a target/precursor combination
/// (AGENTS.md §8).
pub trait ProcessEvidenceProvider {
    fn precedents(
        &self,
        target: &TargetSpecification,
        precursors: &[PrecursorSelection],
    ) -> std::result::Result<Vec<ProcessPrecedent>, ProviderError>;
}
