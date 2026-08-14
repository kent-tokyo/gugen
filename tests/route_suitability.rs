//! Phase 15A: `InMemoryRouteSuitabilityProvider` end to end through
//! `Planner::with_route_suitability_provider`. These tests check that
//! `SynthesisPlanningReport.route_suitability` is populated correctly and,
//! just as importantly, that wiring the provider in changes nothing else --
//! no numeric score, no ranking, no route-family selection (Phase 15A's
//! explicit, owner-instructed constraint).
//!
//! Phase 15B tests below use a hand-built `FixedRouteSuitabilityProvider`
//! rather than the shipped curated records: the real curated Mg(OH)2
//! record has no gugen-balanceable route from any small test catalog (no
//! precursor in a 1-2 candidate catalog supplies both O and H to reach
//! Mg(OH)2 -- confirmed empirically, not assumed), so there would be
//! nothing for NotRecommended filtering to actually filter. The
//! well-established BaTiO3/BaCO3+TiO2 route (proven elsewhere in this
//! suite to plan successfully) plus synthetic findings isolates the
//! filtering behavior itself from curated-data availability.

use gugen::{
    Composition, Element, EvidenceScope, EvidenceStrength, InMemoryPrecursorCatalog,
    InMemoryRouteSuitabilityProvider, Planner, PlanningConfig, PlanningConstraints,
    PrecursorCandidate, PrecursorId, ProviderError, RouteFamily, RouteSuitabilityProvider,
    SuitabilityFinding, SuitabilityVerdict, TargetSpecification,
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
    assert!(report.not_recommended.is_empty());
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

/// A hand-built provider returning fixed findings per route family,
/// independent of `curated_records()` -- lets Phase 15B's filtering tests
/// use the well-established BaTiO3/BaCO3+TiO2 route (proven elsewhere in
/// this suite to plan successfully under both route families) rather than
/// depending on whether a *curated* record happens to have a
/// gugen-balanceable route from a given test catalog.
struct FixedRouteSuitabilityProvider {
    findings_by_family: Vec<(RouteFamily, Vec<SuitabilityFinding>)>,
}

impl RouteSuitabilityProvider for FixedRouteSuitabilityProvider {
    fn assess(
        &self,
        _target: &Composition,
        route_family: RouteFamily,
    ) -> std::result::Result<Vec<SuitabilityFinding>, ProviderError> {
        Ok(self
            .findings_by_family
            .iter()
            .find(|(family, _)| *family == route_family)
            .map(|(_, findings)| findings.clone())
            .unwrap_or_default())
    }
}

fn strong_contradicts() -> SuitabilityFinding {
    SuitabilityFinding {
        verdict: SuitabilityVerdict::Contradicts,
        statement: "test-only strong, exact-target contradicting finding".to_string(),
        source_id: None,
        strength: EvidenceStrength::Strong,
        applicable_to: EvidenceScope::ExactTarget,
        limitations: vec![],
    }
}

fn batio3_target() -> TargetSpecification {
    target(composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]))
}

fn batio3_catalog() -> Vec<PrecursorCandidate> {
    vec![
        candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
        candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
    ]
}

fn plan_with_fixed_route_suitability(
    findings_by_family: Vec<(RouteFamily, Vec<SuitabilityFinding>)>,
) -> gugen::SynthesisPlanningReport {
    Planner::with_route_suitability_provider(
        InMemoryPrecursorCatalog::new(batio3_catalog()),
        FixedRouteSuitabilityProvider { findings_by_family },
        PlanningConfig::default(),
    )
    .plan(&batio3_target(), "2026-08-14T00:00:00Z")
    .unwrap()
}

/// Phase 15B's core guarantee: a strong, uncontested Contradicts finding
/// moves that route family's plan out of `plans` and into
/// `not_recommended`, with the triggering finding attached -- the other
/// route family's plan is completely unaffected.
#[test]
fn a_strong_exact_target_contradicts_moves_the_plan_to_not_recommended() {
    let baseline = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(batio3_catalog()),
        PlanningConfig::default(),
    )
    .plan(&batio3_target(), "2026-08-14T00:00:00Z")
    .unwrap();
    assert_eq!(
        baseline.plans.len(),
        2,
        "sanity check: BaCO3+TiO2 must plan successfully under both route families \
        with no provider configured at all: {:?}",
        baseline.plans
    );

    let report = plan_with_fixed_route_suitability(vec![(
        RouteFamily::ConventionalSolidState,
        vec![strong_contradicts()],
    )]);

    assert_eq!(
        report.plans.len(),
        1,
        "exactly one plan (Mechanochemical) must remain: {:?}",
        report.plans
    );
    assert_eq!(
        report.plans[0].route_family,
        RouteFamily::Mechanochemical,
        "the excluded route family must be ConventionalSolidState specifically, not \
        whichever plan happened to be built first"
    );

    assert_eq!(report.not_recommended.len(), 1);
    let excluded = &report.not_recommended[0];
    assert_eq!(
        excluded.plan.route_family,
        RouteFamily::ConventionalSolidState
    );
    assert_eq!(excluded.contradicting_findings.len(), 1);
    assert_eq!(
        excluded.contradicting_findings[0].verdict,
        SuitabilityVerdict::Contradicts
    );

    assert!(
        report.unresolved.is_empty(),
        "one route excluded but one plan still recommended -- not the all-excluded \
        abstention case: {:?}",
        report.unresolved
    );
    assert_eq!(
        report.applicability, baseline.applicability,
        "applicability reflects target-level domain fit, unaffected by route-suitability \
        filtering"
    );
}

/// Filtering happens after scoring, not instead of it -- the plan embedded
/// in `not_recommended` must carry the exact same score/confidence it
/// would have had in `plans`, not a stripped-down or re-derived one.
#[test]
fn the_not_recommended_plan_carries_its_real_unmodified_score_and_confidence() {
    let baseline = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(batio3_catalog()),
        PlanningConfig::default(),
    )
    .plan(&batio3_target(), "2026-08-14T00:00:00Z")
    .unwrap();
    let baseline_conventional = baseline
        .plans
        .iter()
        .find(|p| p.route_family == RouteFamily::ConventionalSolidState)
        .expect("baseline must include a ConventionalSolidState plan");

    let report = plan_with_fixed_route_suitability(vec![(
        RouteFamily::ConventionalSolidState,
        vec![strong_contradicts()],
    )]);
    let excluded_plan = &report.not_recommended[0].plan;

    assert_eq!(excluded_plan.score, baseline_conventional.score);
    assert_eq!(excluded_plan.confidence, baseline_conventional.confidence);
    assert_eq!(excluded_plan.evidence, baseline_conventional.evidence);
}

/// When every generated plan is excluded, the report must abstain
/// explicitly (a populated `unresolved` entry), not return an empty
/// `plans` indistinguishable from "search found nothing."
#[test]
fn all_routes_not_recommended_produces_an_explicit_abstention() {
    let report = plan_with_fixed_route_suitability(vec![
        (
            RouteFamily::ConventionalSolidState,
            vec![strong_contradicts()],
        ),
        (RouteFamily::Mechanochemical, vec![strong_contradicts()]),
    ]);

    assert!(report.plans.is_empty());
    assert_eq!(report.not_recommended.len(), 2);
    assert!(
        report
            .unresolved
            .iter()
            .any(|u| u.description == "route selection" && u.reason.contains("NotRecommended")),
        "must carry an explicit abstention reason, not a silent empty success: {:?}",
        report.unresolved
    );
    assert_eq!(
        report.applicability.level,
        gugen::ApplicabilityLevel::PartiallyInDomain,
        "applicability must NOT become OutOfDomain here -- gugen built valid chemistry \
        for BaTiO3, evidence just contradicts every route tried; that is a different, \
        weaker claim than 'gugen cannot handle this material' (abstain()'s own case)"
    );
}

/// A `SimilarMaterial`-scoped Contradicts, even at `Strong` strength, must
/// not alone exclude a route -- direct end-to-end confirmation of the
/// safety condition already unit-tested against `derive_recommendation`
/// in isolation (`src/route_suitability.rs`).
#[test]
fn a_similar_material_contradicts_does_not_exclude_a_plan_end_to_end() {
    let report = plan_with_fixed_route_suitability(vec![(
        RouteFamily::ConventionalSolidState,
        vec![SuitabilityFinding {
            applicable_to: EvidenceScope::SimilarMaterial,
            ..strong_contradicts()
        }],
    )]);

    assert_eq!(
        report.plans.len(),
        2,
        "SimilarMaterial-scoped evidence alone must not filter anything: {:?}",
        report.plans
    );
    assert!(report.not_recommended.is_empty());
}
