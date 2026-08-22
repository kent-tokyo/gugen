//! Phase 22.1 end-to-end golden fixture: a full plan -> catalog ->
//! assessment pipeline run against a larger (40-fictional-offer) catalog
//! than the small hand-picked fixtures elsewhere, compared against a
//! checked-in golden `CommercialPlanAssessment`. `CommercialPlanAssessment`
//! embeds no `env!("CARGO_PKG_VERSION")`-derived or timestamp-derived
//! field anywhere (confirmed by reading its definition and every type it
//! contains), and this test supplies a fixed plan timestamp, so it should
//! not need regeneration on ordinary version bumps -- unlike
//! `tests/fixtures/batio3_report.json`, which does embed a version string
//! elsewhere in the crate.

use gugen::{
    CommercialCatalogLoadMode, CommercialPlanningConfig, CommercialPlanningRequest,
    CommercialPrecursorCatalog, Composition, Element, InMemoryPrecursorCatalog, Planner,
    PlanningConfig, PlanningConstraints, PrecursorCandidate, PrecursorId, TargetSpecification,
    assess_commercial_precursors,
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

fn barium_titanate_plan() -> gugen::SynthesisPlan {
    let planner = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(vec![
            candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
        ]),
        PlanningConfig::default(),
    );
    let target_spec = TargetSpecification {
        composition: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
        structure: None,
        desired_phase: None,
        constraints: PlanningConstraints::default(),
    };
    let report = planner.plan(&target_spec, "2026-08-22T00:00:00Z").unwrap();
    report
        .plans
        .into_iter()
        .next()
        .expect("BaCO3 + TiO2 -> BaTiO3 must produce at least one plan")
}

#[test]
fn assess_commercial_precursors_matches_the_golden_fixture() {
    let plan = barium_titanate_plan();

    let csv_text = std::fs::read_to_string("tests/fixtures/commercial_catalog_golden.csv")
        .expect("golden CSV fixture must exist");
    let (catalog, report) =
        CommercialPrecursorCatalog::load_csv(&csv_text, CommercialCatalogLoadMode::Strict)
            .expect("the golden fixture must be well-formed and load cleanly in Strict mode");
    assert_eq!(report.rejected.len(), 0, "no row should be rejected");
    assert_eq!(report.accepted, 40, "all 40 fictional offers must load");

    let assessment = assess_commercial_precursors(
        &plan,
        &catalog,
        &CommercialPlanningRequest::default(),
        &CommercialPlanningConfig::default(),
    )
    .unwrap();

    // Sanity: the fixture actually exercises a non-trivial part of the
    // pipeline (checked before serializing, since CommercialPlanAssessment
    // is Serialize-only -- see its doc comment for why -- so a round-trip
    // deserialize isn't available here to check it after the fact).
    assert!(assessment.every_precursor_has_a_match);
    assert!(!assessment.combinations.is_empty());

    let actual = serde_json::to_string_pretty(&assessment).unwrap();
    let golden_path = "tests/fixtures/commercial_catalog_golden_assessment.json";
    let golden = std::fs::read_to_string(golden_path).unwrap_or_else(|_| {
        panic!(
            "golden fixture {golden_path} must exist -- generate it once from a verified-correct \
             assessment run, never hand-write it"
        )
    });

    assert_eq!(
        actual.trim_end(),
        golden.trim_end(),
        "assessment output drifted from the checked-in golden fixture -- if this is an intentional \
         behavior change, review the diff carefully, then regenerate {golden_path} from this test's \
         real output"
    );
}
