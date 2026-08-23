use crate::composition::Composition;
use crate::error::ProviderError;
use crate::literature_evidence::LiteratureRouteEvidence;
use crate::precursor::{PrecursorCandidate, PrecursorSelection};
use crate::prior_experiment_evidence::PriorExperimentEvidence;
use crate::process::{ProcessPrecedent, RouteFamily};
use crate::reaction::{BalancedReaction, CompetingPhase, ReactionEnergy, ThermodynamicConditions};
use crate::route_suitability::SuitabilityFinding;
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

    /// Formation energies of phases that might compete with `target` for
    /// the same elements (Phase 13) -- context only, never converted into a
    /// selectivity score (AGENTS.md §4.3, same separation `reaction_energy`
    /// already keeps). Default `Ok(Vec::new())` ("no data") so every
    /// existing `ThermodynamicProvider` implementor keeps compiling
    /// unchanged -- a non-breaking addition, unlike Phase 12's `score_plan`
    /// signature change.
    fn competing_phases(
        &self,
        _target: &Composition,
    ) -> std::result::Result<Vec<CompetingPhase>, ProviderError> {
        Ok(Vec::new())
    }
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

/// Source of route-suitability findings for a target/route-family
/// combination (Phase 15A; AGENTS.md §8 sketches providers as a floor, not
/// an exhaustive list -- Phase 13's `competing_phases` already extended
/// this pattern once). Deliberately narrower than `ProcessEvidenceProvider`:
/// suitability depends only on the target and route family, not on a
/// specific precursor set, since a route family's fitness for a material
/// (e.g. whether high-temperature firing is even reachable before the
/// target decomposes) doesn't change with which precursors were chosen to
/// reach it.
pub trait RouteSuitabilityProvider {
    fn assess(
        &self,
        target: &Composition,
        route_family: RouteFamily,
    ) -> std::result::Result<Vec<SuitabilityFinding>, ProviderError>;
}

/// Source of reference-only, cross-DOI literature evidence for an exact
/// target/precursor-set/route-family combination (v0.4.0 Integration).
/// Deliberately narrower in spirit than [`ProcessEvidenceProvider`]: this
/// trait's output ([`LiteratureRouteEvidence`]) is never applied to a
/// `ProcessStep`, never converted to a `ConditionPrecedent`, and never
/// passed to `score_plan` -- `Planner` attaches it to `SynthesisPlan` as
/// its own field, structurally isolated from every scoring input. Not
/// gated behind the `literature_corpus` feature: this trait and
/// [`LiteratureRouteEvidence`] are always compiled, so `Planner`'s public
/// API and report schema never change shape depending on which crate
/// features are enabled. The one real implementation
/// (`LiteratureObservationCorpusProvider`, backed by
/// `LiteratureObservationCorpus::cross_doi_comparisons`) lives behind
/// that feature -- same split `ThermodynamicProvider` already has against
/// `MaterialsProjectSnapshotProvider`.
pub trait LiteratureEvidenceProvider {
    fn route_evidence(
        &self,
        target: &Composition,
        route_family: RouteFamily,
        precursors: &[Composition],
    ) -> std::result::Result<Option<LiteratureRouteEvidence>, ProviderError>;
}

/// Source of reference-only prior-experiment evidence for an exact
/// target/precursor-set/route-family combination (Phase 26). Mirrors
/// [`LiteratureEvidenceProvider`]'s own exact-match contract exactly:
/// this trait's output ([`PriorExperimentEvidence`]) is never applied to
/// a `ProcessStep`, never converted to a `ConditionPrecedent`, and never
/// passed to `score_plan` -- `Planner` attaches it to `SynthesisPlan` as
/// its own field, structurally isolated from every scoring input. Not
/// gated behind any Cargo feature: unlike `LiteratureEvidenceProvider`
/// (whose one real implementation needs the `literature_corpus`
/// feature's corpus loader), [`crate::execution_record::SynthesisExecutionRecord`]
/// itself carries no feature gate, so neither does the one real
/// implementation of this trait
/// ([`crate::prior_experiment_evidence::InMemoryExecutionRecordProvider`]).
/// `Ok(Some(evidence))` implies `evidence.records` is non-empty -- an
/// empty match is `Ok(None)`, never a rendered "0 prior experiments"
/// disclosure. Unlike `LiteratureEvidenceProvider`, `Planner` does not
/// restrict which route families this provider is asked about (see
/// `docs/prior_experiment_evidence.md` for why that restriction doesn't
/// apply here).
pub trait PriorExperimentEvidenceProvider {
    fn prior_experiments(
        &self,
        target: &Composition,
        route_family: RouteFamily,
        precursors: &[Composition],
    ) -> std::result::Result<Option<PriorExperimentEvidence>, ProviderError>;
}
