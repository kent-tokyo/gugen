//! Phase 22 §12: Commercial Precursor Catalog is a structurally separate,
//! post-planning stage. Mirrors
//! `tests/literature_observation_planner_invariance.rs`'s exact shape:
//! exercise the whole new subsystem, entirely disconnected from
//! `planner`/`score_plan`, and prove it has no effect on planning output.

use gugen::{
    AvailabilityStatus, CommercialOfferId, CommercialPlanningConfig, CommercialPlanningRequest,
    CommercialPrecursorCatalog, CommercialPrecursorOffer, CommercialSourceType, Composition,
    CurrencyCode, Element, InMemoryPrecursorCatalog, Money, OfferProvenance, PackageMass, Planner,
    PlanningConfig, PlanningConstraints, PrecursorCandidate, PrecursorId, PurityFraction,
    TargetSpecification, assess_commercial_precursors,
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

fn barium_titanate_catalog() -> Vec<PrecursorCandidate> {
    vec![
        candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
        candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
    ]
}

fn target() -> TargetSpecification {
    TargetSpecification {
        composition: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
        structure: None,
        desired_phase: None,
        constraints: PlanningConstraints::default(),
    }
}

fn commercial_offer(id: &str, formula: &str, price_minor_units: u64) -> CommercialPrecursorOffer {
    CommercialPrecursorOffer {
        offer_id: CommercialOfferId(id.to_string()),
        manufacturer: "Example Materials Ltd.".to_string(),
        product_name: "Demo Oxide Grade A".to_string(),
        composition: composition(match formula {
            "BaCO3" => &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)],
            "TiO2" => &[("Ti", 1.0), ("O", 2.0)],
            other => panic!("unexpected test formula {other}"),
        }),
        provenance: OfferProvenance {
            source_type: CommercialSourceType::SyntheticFixture,
            source_identifier: id.to_string(),
            retrieved_at: None,
            supplied_by: None,
            license_or_terms: None,
            checksum: None,
        },
        formula: formula.to_string(),
        catalog_number: None,
        cas_number: None,
        grade: None,
        purity: Some(PurityFraction::new(0.99).unwrap()),
        package_mass: Some(PackageMass::new(100.0).unwrap()),
        unit_price: Some(Money::new(
            price_minor_units,
            CurrencyCode::new("USD").unwrap(),
        )),
        availability: Some(AvailabilityStatus::InStock),
        lead_time_days: Some(5),
        physical_form: None,
        particle_size_range_um: None,
        country_region: None,
        product_url: None,
        tags: Default::default(),
        notes: None,
    }
}

#[test]
fn assessing_commercial_precursors_does_not_change_planning_output() {
    let planner = Planner::builder(
        InMemoryPrecursorCatalog::new(barium_titanate_catalog()),
        PlanningConfig::default(),
    )
    .build();
    let target_spec = target();

    let report_before = planner.plan(&target_spec, "2026-08-22T00:00:00Z").unwrap();

    // Independently exercise the whole Phase 22 surface for this same plan
    // -- entirely disconnected from `planner`/`score_plan`, run here
    // specifically to prove it has no side effect on subsequent planning
    // output.
    let (catalog, load_report) = CommercialPrecursorCatalog::from_offers(vec![
        commercial_offer("BACO3-1", "BaCO3", 1000),
        commercial_offer("TIO2-1", "TiO2", 800),
    ]);
    assert_eq!(load_report.rejected.len(), 0);
    let plan = report_before
        .plans
        .first()
        .expect("must produce at least one plan");
    let assessment = assess_commercial_precursors(
        plan,
        &catalog,
        &CommercialPlanningRequest::default(),
        &CommercialPlanningConfig::default(),
    )
    .unwrap();
    // A real, non-empty result -- this must be a genuinely exercised
    // assessment, not a no-op that happens to prove nothing either way.
    assert!(assessment.every_precursor_has_a_match);
    assert!(!assessment.combinations.is_empty());

    let report_after = planner.plan(&target_spec, "2026-08-22T00:00:00Z").unwrap();

    assert_eq!(
        report_before, report_after,
        "assessing commercial precursors must not affect planning output at all"
    );
}

#[test]
fn the_same_plan_under_two_different_catalogs_stays_byte_identical() {
    let planner = Planner::builder(
        InMemoryPrecursorCatalog::new(barium_titanate_catalog()),
        PlanningConfig::default(),
    )
    .build();
    let report = planner.plan(&target(), "2026-08-22T00:00:00Z").unwrap();
    let plan = report
        .plans
        .first()
        .expect("must produce at least one plan");
    let plan_clone = plan.clone();

    let (catalog_a, _) = CommercialPrecursorCatalog::from_offers(vec![
        commercial_offer("BACO3-CHEAP", "BaCO3", 500),
        commercial_offer("TIO2-CHEAP", "TiO2", 300),
    ]);
    let (catalog_b, _) = CommercialPrecursorCatalog::from_offers(vec![
        commercial_offer("BACO3-EXPENSIVE", "BaCO3", 99_999),
        commercial_offer("TIO2-EXPENSIVE", "TiO2", 88_888),
    ]);

    let request = CommercialPlanningRequest::default();
    let config = CommercialPlanningConfig::default();
    let assessment_a = assess_commercial_precursors(plan, &catalog_a, &request, &config).unwrap();
    let assessment_b = assess_commercial_precursors(plan, &catalog_b, &request, &config).unwrap();
    assert_ne!(
        assessment_a.combinations[0].total_cost, assessment_b.combinations[0].total_cost,
        "the two catalogs must genuinely differ, or this test proves nothing"
    );

    assert_eq!(
        *plan, plan_clone,
        "the same SynthesisPlan assessed against two different catalogs must stay byte-for-byte identical"
    );
}
