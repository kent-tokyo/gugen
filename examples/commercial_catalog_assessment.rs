//! Worked example: plan a target, then match its precursors against a
//! synthetic commercial catalog. Run with:
//!
//! ```text
//! cargo run --example commercial_catalog_assessment --features commercial_catalog
//! ```

use gugen::{
    CommercialCatalogLoadMode, CommercialPlanningConfig, CommercialPlanningRequest,
    CommercialPrecursorCatalog, Composition, Element, InMemoryPrecursorCatalog, Planner,
    PlanningConfig, PlanningConstraints, PrecursorCandidate, PrecursorId, TargetSpecification,
    assess_commercial_precursors,
};

const SAMPLE_CSV: &str = include_str!("../tests/fixtures/commercial_catalog_sample.csv");

fn element(symbol: &str) -> Element {
    Element::new(symbol).unwrap()
}

fn composition(pairs: &[(&str, f64)]) -> Composition {
    Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
}

fn main() {
    // Stage 1: chemical planning, entirely unaware commercial data exists.
    let planner = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(vec![
            PrecursorCandidate {
                id: PrecursorId("BaCO3".to_string()),
                composition: composition(&[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
                availability: None,
            },
            PrecursorCandidate {
                id: PrecursorId("TiO2".to_string()),
                composition: composition(&[("Ti", 1.0), ("O", 2.0)]),
                availability: None,
            },
        ]),
        PlanningConfig::default(),
    );
    let target = TargetSpecification {
        composition: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
        structure: None,
        desired_phase: None,
        constraints: PlanningConstraints::default(),
    };
    let report = planner.plan(&target, "2026-08-22T00:00:00Z").unwrap();
    let plan = report
        .plans
        .first()
        .expect("BaCO3 + TiO2 -> BaTiO3 must produce a plan");
    println!(
        "Chemical plan {} scored {:.2}; commercial matching never touches this score.",
        plan.plan_id,
        plan.score.total_ranking_score.value()
    );

    // Stage 2: catalog-matched commercial offers, a post-planning, purely
    // informational stage -- catalog values are supplied data, not
    // certified by gugen. See docs/commercial_precursor_catalog.md.
    let (catalog, load_report) =
        CommercialPrecursorCatalog::load_csv(SAMPLE_CSV, CommercialCatalogLoadMode::Lenient)
            .unwrap();
    println!(
        "Loaded {} candidate purchasable offers ({} rejected).",
        load_report.accepted,
        load_report.rejected.len()
    );

    let assessment = assess_commercial_precursors(
        plan,
        &catalog,
        &CommercialPlanningRequest::default(),
        &CommercialPlanningConfig::default(),
    )
    .unwrap();

    if !assessment.every_precursor_has_a_match {
        println!(
            "Not every precursor has a catalog-matched offer: {:?}",
            assessment.unmatched_precursors
        );
        return;
    }

    let best = &assessment.combinations[0];
    println!(
        "Best candidate procurement combination ({}):",
        best.combination_id
    );
    for selection in &best.selections {
        println!(
            "  {} <- offer {} (theoretical requirement: {:.3} g)",
            selection.precursor, selection.offer_id, selection.theoretical_pure_mass_required_grams
        );
    }
    match best.total_cost {
        Some(cost) => println!(
            "  Estimated subtotal: {} minor units {} (procurement-oriented estimate, not a purchase guarantee).",
            cost.minor_units(),
            cost.currency()
        ),
        None => println!(
            "  Total cost unknown (not every selected offer has a known price/package size)."
        ),
    }
    println!(
        "  Search budget: {} combination(s) evaluated, exhaustive: {}.",
        assessment.search_budget.combinations_evaluated, assessment.search_budget.is_exhaustive
    );
}
