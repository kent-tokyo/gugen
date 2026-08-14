//! Phase 15A: `InMemoryRouteSuitabilityProvider` end to end through
//! `Planner::with_route_suitability_provider`. These tests check that
//! `SynthesisPlanningReport.route_suitability` is populated correctly and,
//! just as importantly, that wiring the provider in changes nothing else --
//! no numeric score, no ranking, no route-family selection (this phase's
//! explicit, owner-instructed constraint).

use gugen::{
    Composition, Element, InMemoryPrecursorCatalog, InMemoryRouteSuitabilityProvider, Planner,
    PlanningConfig, PlanningConstraints, PrecursorCandidate, PrecursorId, RouteFamily,
    SuitabilityVerdict, TargetSpecification,
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

fn target(composition: Composition) -> TargetSpecification {
    TargetSpecification {
        composition,
        structure: None,
        desired_phase: None,
        constraints: PlanningConstraints::default(),
    }
}

fn plan_with_route_suitability(
    target_spec: &TargetSpecification,
    catalog: Vec<PrecursorCandidate>,
) -> gugen::SynthesisPlanningReport {
    Planner::with_route_suitability_provider(
        InMemoryPrecursorCatalog::new(catalog),
        InMemoryRouteSuitabilityProvider::from_curated_records(),
        PlanningConfig::default(),
    )
    .plan(target_spec, "2026-08-14T00:00:00Z")
    .unwrap()
}

/// BaTiO3 has a curated `Supports` finding for `Mechanochemical` -- the
/// report must carry a `RouteSuitabilityAssessment` for both route
/// families it evaluates unconditionally, with findings present only for
/// the one the curated record actually covers.
#[test]
fn matched_target_and_route_family_produces_real_findings() {
    let target_spec = target(composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]));
    let report = plan_with_route_suitability(
        &target_spec,
        vec![
            candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
        ],
    );

    assert_eq!(
        report.route_suitability.len(),
        2,
        "both currently-defined route families must be assessed unconditionally: {:?}",
        report.route_suitability
    );

    let mechanochemical = report
        .route_suitability
        .iter()
        .find(|a| a.route_family == RouteFamily::Mechanochemical)
        .expect("Mechanochemical must be one of the assessed route families");
    assert_eq!(mechanochemical.findings.len(), 1);
    assert_eq!(
        mechanochemical.findings[0].verdict,
        SuitabilityVerdict::Supports
    );
    assert_eq!(
        mechanochemical.findings[0].source_id.as_deref(),
        Some("10.3390/chemistry4020042")
    );

    let conventional = report
        .route_suitability
        .iter()
        .find(|a| a.route_family == RouteFamily::ConventionalSolidState)
        .expect("ConventionalSolidState must be one of the assessed route families");
    assert!(
        conventional.findings.is_empty(),
        "BaTiO3 has no curated ConventionalSolidState finding -- must be an empty vec, \
        not absent from route_suitability: {:?}",
        conventional.findings
    );
}

/// Mg(OH)2 has a curated `Contradicts` finding for `ConventionalSolidState`
/// specifically -- the inverse pairing from the BaTiO3/Mechanochemical case
/// above, exercising the other verdict and the other route family.
#[test]
fn a_different_target_and_route_family_pairing_also_resolves_correctly() {
    let target_spec = target(composition(&[("Mg", 1.0), ("O", 2.0), ("H", 2.0)]));
    let report = plan_with_route_suitability(
        &target_spec,
        vec![candidate("MgO", &[("Mg", 1.0), ("O", 1.0)])],
    );

    let conventional = report
        .route_suitability
        .iter()
        .find(|a| a.route_family == RouteFamily::ConventionalSolidState)
        .expect("ConventionalSolidState must be one of the assessed route families");
    assert_eq!(conventional.findings.len(), 1);
    assert_eq!(
        conventional.findings[0].verdict,
        SuitabilityVerdict::Contradicts
    );

    let mechanochemical = report
        .route_suitability
        .iter()
        .find(|a| a.route_family == RouteFamily::Mechanochemical)
        .expect("Mechanochemical must be one of the assessed route families");
    assert!(
        mechanochemical.findings.is_empty(),
        "Mg(OH)2 has no curated Mechanochemical finding: {:?}",
        mechanochemical.findings
    );
}

/// A target the curated set has no coverage for at all must still produce
/// an assessment per route family, each with empty findings -- absence of
/// evidence, never silently omitted from the list.
#[test]
fn a_target_with_no_curated_coverage_still_produces_empty_assessments() {
    let target_spec = target(composition(&[("Zn", 1.0), ("O", 1.0)]));
    let report = plan_with_route_suitability(
        &target_spec,
        vec![candidate("ZnO", &[("Zn", 1.0), ("O", 1.0)])],
    );

    assert_eq!(report.route_suitability.len(), 2);
    for assessment in &report.route_suitability {
        assert!(
            assessment.findings.is_empty(),
            "ZnO has no curated coverage for either route family: {assessment:?}"
        );
    }
}

/// `Planner::offline_minimal` never configures a route-suitability
/// provider -- `route_suitability` must be empty, not merely unpopulated
/// by coincidence.
#[test]
fn offline_minimal_report_has_no_route_suitability_assessments() {
    let target_spec = target(composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]));
    let catalog = vec![
        candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
        candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
    ];
    let report = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(catalog),
        PlanningConfig::default(),
    )
    .plan(&target_spec, "2026-08-14T00:00:00Z")
    .unwrap();

    assert!(report.route_suitability.is_empty());
}

/// The core Phase 15A guarantee: wiring a route-suitability provider in
/// must not change any plan's score, confidence, or applicability, even
/// for a target with real `Supports`/`Contradicts` findings. This is the
/// direct evidence that findings carry no ranking weight yet -- not an
/// absence-of-test-failure inference.
#[test]
fn configuring_the_provider_does_not_change_any_plans_score_or_confidence() {
    let target_spec = target(composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]));
    let catalog = vec![
        candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
        candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
    ];

    let with_provider = plan_with_route_suitability(&target_spec, catalog.clone());
    let offline = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(catalog),
        PlanningConfig::default(),
    )
    .plan(&target_spec, "2026-08-14T00:00:00Z")
    .unwrap();

    assert!(!with_provider.plans.is_empty());
    assert_eq!(with_provider.plans.len(), offline.plans.len());
    for (a, b) in with_provider.plans.iter().zip(&offline.plans) {
        assert_eq!(
            a.score, b.score,
            "route suitability findings must not affect PlanScoreBreakdown"
        );
        assert_eq!(
            a.confidence, b.confidence,
            "route suitability findings must not affect ConfidenceAssessment"
        );
        assert_eq!(
            a.applicability, b.applicability,
            "route suitability findings must not affect per-plan applicability"
        );
        assert_eq!(
            a.evidence, b.evidence,
            "route suitability findings must not be folded into plan-level evidence \
            (that vec feeds evidence_strength's weakest-link aggregate)"
        );
    }
    assert_ne!(
        with_provider.route_suitability, offline.route_suitability,
        "sanity check that the provider actually ran and populated the new field"
    );
}
