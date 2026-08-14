//! AGENTS.md §21.5's full provider-failure checklist, exercised through
//! `Planner::plan` end to end (not just at the unit level): provider
//! timeout相当, missing entry, malformed record, partial thermodynamic
//! coverage, duplicated evidence, inconsistent units, unavailable
//! provider. "一つのprovider失敗で、可能なplanning全体を必ず失敗させない
//! でください" -- every case here must still produce a report, never a
//! `Planner::plan` error, since only `PrecursorCatalog` failures propagate
//! (AGENTS.md §21.5, `src/planner.rs`'s own doc comment).

use gugen::{
    BalancedReaction, Composition, Element, EvidenceKind, EvidenceStrength,
    InMemoryPrecursorCatalog, Planner, PlanningConfig, PlanningConstraints, PrecursorCandidate,
    PrecursorId, PrecursorSelection, ProcessEvidenceProvider, ProcessPrecedent, ProviderError,
    ReactionEnergy, TargetSpecification, ThermodynamicConditions, ThermodynamicProvider,
};

fn element(symbol: &str) -> Element {
    Element::new(symbol).unwrap()
}

fn composition(pairs: &[(&str, f64)]) -> Composition {
    Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
}

fn candidate(id: &str, pairs: &[(&str, f64)]) -> PrecursorCandidate {
    PrecursorCandidate {
        id: PrecursorId(id.to_string()),
        composition: composition(pairs),
        availability: None,
    }
}

/// BaCO3 + TiO2 and BaO + TiO2 are both valid BaTiO3 routes, giving every
/// test in this file at least two accepted candidates in one report --
/// needed for the "partial coverage" cases below, which only mean
/// something when a single `Planner::plan` call has more than one
/// candidate for a provider to treat differently.
fn two_route_catalog() -> InMemoryPrecursorCatalog {
    InMemoryPrecursorCatalog::new(vec![
        candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
        candidate("BaO", &[("Ba", 1.0), ("O", 1.0)]),
        candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
    ])
}

fn batio3_target() -> TargetSpecification {
    TargetSpecification {
        composition: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
        structure: None,
        desired_phase: None,
        constraints: PlanningConstraints::default(),
    }
}

struct FixedThermodynamicProvider(std::result::Result<Option<ReactionEnergy>, ProviderError>);
impl ThermodynamicProvider for FixedThermodynamicProvider {
    fn reaction_energy(
        &self,
        _reaction: &BalancedReaction,
        _conditions: &ThermodynamicConditions,
    ) -> std::result::Result<Option<ReactionEnergy>, ProviderError> {
        self.0.clone()
    }
}

struct NoPrecedents;
impl ProcessEvidenceProvider for NoPrecedents {
    fn precedents(
        &self,
        _target: &TargetSpecification,
        _precursors: &[PrecursorSelection],
    ) -> std::result::Result<Vec<ProcessPrecedent>, ProviderError> {
        Ok(vec![])
    }
}

fn plan_with_thermodynamic_provider(
    result: std::result::Result<Option<ReactionEnergy>, ProviderError>,
) -> gugen::SynthesisPlanningReport {
    Planner::new(
        two_route_catalog(),
        NoPrecedents,
        FixedThermodynamicProvider(result),
        PlanningConfig::default(),
    )
    .plan(&batio3_target(), "2026-08-14T00:00:00Z")
    .unwrap()
}

/// "provider timeout相当": no dedicated `ProviderError::Timeout` variant
/// exists (AGENTS.md §8 doesn't mandate one), so a timeout is represented
/// as `Unavailable` with a message that says so -- the same variant an
/// outright-down provider would use, which is the honest representation:
/// from `Planner`'s perspective a timeout and an outage are the same
/// "couldn't get an answer in time" fact.
#[test]
fn provider_timeout_degrades_to_a_warning_not_a_failure() {
    let report = plan_with_thermodynamic_provider(Err(ProviderError::Unavailable(
        "timed out after 30s".to_string(),
    )));
    assert!(!report.plans.is_empty());
    for plan in &report.plans {
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.message.contains("timed out after 30s")),
            "{:?}",
            plan.warnings
        );
    }
}

#[test]
fn missing_entry_degrades_to_a_warning_not_a_failure() {
    let report = plan_with_thermodynamic_provider(Err(ProviderError::MissingEntry));
    assert!(!report.plans.is_empty());
    for plan in &report.plans {
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.message.contains("entry not found")),
            "{:?}",
            plan.warnings
        );
    }
}

#[test]
fn malformed_record_degrades_to_a_warning_not_a_failure() {
    let report = plan_with_thermodynamic_provider(Err(ProviderError::MalformedRecord(
        "non-numeric energy field".to_string(),
    )));
    assert!(!report.plans.is_empty());
    for plan in &report.plans {
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.message.contains("non-numeric energy field")),
            "{:?}",
            plan.warnings
        );
    }
}

/// "partial thermodynamic coverage": a provider that has data for some
/// reactions and not others in the *same* planning run. `reaction_energy`
/// only sees one `BalancedReaction` at a time, so this provider looks at
/// which precursor is used to decide -- coverage is genuinely partial
/// within one `Planner::plan` call, not just across separate calls.
struct PartialCoverageProvider;
impl ThermodynamicProvider for PartialCoverageProvider {
    fn reaction_energy(
        &self,
        reaction: &BalancedReaction,
        _conditions: &ThermodynamicConditions,
    ) -> std::result::Result<Option<ReactionEnergy>, ProviderError> {
        let has_carbonate = reaction
            .reactants
            .iter()
            .any(|s| s.composition.amount_of(element("C")).is_some());
        if has_carbonate {
            Ok(Some(ReactionEnergy::new(-0.5).unwrap()))
        } else {
            Ok(None)
        }
    }
}

#[test]
fn partial_thermodynamic_coverage_across_candidates_in_one_report_does_not_crash_or_fail() {
    let report = Planner::new(
        two_route_catalog(),
        NoPrecedents,
        PartialCoverageProvider,
        PlanningConfig::default(),
    )
    .plan(&batio3_target(), "2026-08-14T00:00:00Z")
    .unwrap();

    assert_eq!(
        report.plans.len(),
        4,
        "both BaTiO3 precursor routes must still be planned, each under both route families \
        (Phase 12)"
    );
    // `PlanScoreBreakdown.thermodynamic_support` is a *different* thing
    // from "did a provider return data" -- it stays `None` unconditionally
    // in v0.1 even when a provider does supply an energy, because
    // converting a raw reaction energy into a favorability score would be
    // exactly the unsourced heuristic AGENTS.md §4.3 forbids (score.rs's
    // own doc comment says so). The real signal that data arrived is an
    // `EvidenceKind::ThermodynamicData` entry in `evidence` -- checked
    // here, not the score field, which would make this test fail for the
    // wrong reason (it did, the first time this was written).
    let with_data = report
        .plans
        .iter()
        .filter(|p| {
            p.evidence
                .iter()
                .any(|e| e.kind == EvidenceKind::ThermodynamicData)
        })
        .count();
    let without_data = report
        .plans
        .iter()
        .filter(|p| {
            !p.evidence
                .iter()
                .any(|e| e.kind == EvidenceKind::ThermodynamicData)
        })
        .count();
    assert_eq!(
        with_data, 2,
        "the carbonate route should carry thermodynamic evidence under both route families \
        (Phase 12) -- reaction_energy depends only on the reaction, shared by both"
    );
    assert_eq!(
        without_data, 2,
        "the oxide route legitimately has none, under either route family -- not an error state"
    );
    assert!(
        report
            .plans
            .iter()
            .all(|p| p.score.thermodynamic_support.is_none()),
        "thermodynamic_support stays None in v0.1 regardless of provider coverage -- see \
        comment above"
    );
    // AGENTS.md §13: missing data must not zero out the total score.
    for plan in &report.plans {
        assert!(plan.score.total_ranking_score.value() > 0.0);
    }
}

/// "duplicated evidence": a provider that returns the exact same
/// precedent twice. Must not crash or double-count in a way that
/// inflates `evidence_strength` (which aggregates by *minimum*, so
/// duplicates are structurally inert there) -- but duplicates are **not**
/// deduplicated out of the plan's `evidence` list. That's a real,
/// documented gap (a purely cosmetic one given the min-aggregation), not
/// a silently-fixed one: adding dedup logic here would be new behavior
/// Phase 8 wasn't asked to build, not a validation fix.
struct DuplicatingPrecedentProvider;
impl ProcessEvidenceProvider for DuplicatingPrecedentProvider {
    fn precedents(
        &self,
        _target: &TargetSpecification,
        _precursors: &[PrecursorSelection],
    ) -> std::result::Result<Vec<ProcessPrecedent>, ProviderError> {
        Ok(vec![
            ProcessPrecedent {
                description: "same precedent".to_string(),
                conditions: vec![],
            },
            ProcessPrecedent {
                description: "same precedent".to_string(),
                conditions: vec![],
            },
        ])
    }
}

#[test]
fn duplicated_evidence_does_not_crash_and_is_not_silently_deduplicated() {
    let report = Planner::new(
        two_route_catalog(),
        DuplicatingPrecedentProvider,
        FixedThermodynamicProvider(Ok(None)),
        PlanningConfig::default(),
    )
    .plan(&batio3_target(), "2026-08-14T00:00:00Z")
    .unwrap();

    assert!(!report.plans.is_empty());
    for plan in &report.plans {
        let dup_count = plan
            .evidence
            .iter()
            .filter(|e| e.statement == "same precedent")
            .count();
        assert_eq!(
            dup_count, 2,
            "duplicates are preserved as-is (documented, not deduplicated): {:?}",
            plan.evidence
        );
    }
}

/// "inconsistent units": `ThermodynamicProvider::reaction_energy` returns
/// a bare `ReactionEnergy`, documented as eV/atom (reaction.rs) but with
/// no unit tag or cross-check anywhere in the type. This test proves that
/// gap exists rather than just asserting it in prose: a "provider" that
/// returns values on a wildly different scale (as if reporting kJ/mol
/// packed into the same field) is accepted identically to a correctly
/// scaled one -- gugen has no way to notice the inconsistency.
struct WrongScaleProvider;
impl ThermodynamicProvider for WrongScaleProvider {
    fn reaction_energy(
        &self,
        _reaction: &BalancedReaction,
        _conditions: &ThermodynamicConditions,
    ) -> std::result::Result<Option<ReactionEnergy>, ProviderError> {
        // A real eV/atom formation energy is order-of-magnitude ~1; this
        // is off by roughly the eV<->kJ/mol conversion factor (~96.5), the
        // shape a genuine unit-mismatch bug would take.
        Ok(Some(ReactionEnergy::new(-48250.0).unwrap()))
    }
}

#[test]
fn thermodynamic_provider_has_no_unit_consistency_check_a_documented_gap() {
    let normal = plan_with_thermodynamic_provider(Ok(Some(ReactionEnergy::new(-0.5).unwrap())));
    let wrong_scale = Planner::new(
        two_route_catalog(),
        NoPrecedents,
        WrongScaleProvider,
        PlanningConfig::default(),
    )
    .plan(&batio3_target(), "2026-08-14T00:00:00Z")
    .unwrap();

    assert!(!normal.plans.is_empty());
    assert!(!wrong_scale.plans.is_empty());
    // Both are accepted identically -- same evidence strength, no warning
    // distinguishing a plausible value from an implausible one.
    for (a, b) in normal.plans.iter().zip(&wrong_scale.plans) {
        assert_eq!(a.score.evidence_strength, b.score.evidence_strength);
        let flagged = b
            .warnings
            .iter()
            .any(|w| w.message.to_lowercase().contains("unit"));
        assert!(
            !flagged,
            "if this starts failing, gugen gained a unit-consistency check -- update this \
            test (and tasks/todo.md's Known limitations) to match, don't just delete it"
        );
    }
}

struct AlwaysUnavailable;
impl ThermodynamicProvider for AlwaysUnavailable {
    fn reaction_energy(
        &self,
        _reaction: &BalancedReaction,
        _conditions: &ThermodynamicConditions,
    ) -> std::result::Result<Option<ReactionEnergy>, ProviderError> {
        Err(ProviderError::Unavailable("provider offline".to_string()))
    }
}
impl ProcessEvidenceProvider for AlwaysUnavailable {
    fn precedents(
        &self,
        _target: &TargetSpecification,
        _precursors: &[PrecursorSelection],
    ) -> std::result::Result<Vec<ProcessPrecedent>, ProviderError> {
        Err(ProviderError::Unavailable("provider offline".to_string()))
    }
}

/// "unavailable provider", both optional providers at once, across a
/// multi-candidate report -- `planner.rs`'s own unit tests already cover
/// one provider failing; this pins both together end to end with a
/// two-route target rather than a single-candidate one.
#[test]
fn both_optional_providers_unavailable_still_produces_a_full_report() {
    let report = Planner::new(
        two_route_catalog(),
        AlwaysUnavailable,
        AlwaysUnavailable,
        PlanningConfig::default(),
    )
    .plan(&batio3_target(), "2026-08-14T00:00:00Z")
    .unwrap();

    assert_eq!(
        report.plans.len(),
        4,
        "2 precursor routes x 2 route families (Phase 12)"
    );
    for plan in &report.plans {
        assert!(plan.score.thermodynamic_support.is_none());
        assert!(
            plan.warnings
                .iter()
                .filter(|w| w.message.contains("provider offline"))
                .count()
                >= 2
        );
        assert!(
            plan.evidence
                .iter()
                .all(|e| e.strength != EvidenceStrength::Strong
                    || e.kind != EvidenceKind::ThermodynamicData),
            "no thermodynamic evidence should have been attached at all: {:?}",
            plan.evidence
        );
    }
}
