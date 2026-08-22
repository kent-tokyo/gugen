//! AGENTS.md §26 Phase 8 "adversarial examples": deliberately awkward
//! inputs, checked end to end through `Planner::plan`, which must never
//! panic on well-formed input (AGENTS.md §25) and must always return a
//! typed `Err` or an explained empty/rejected result rather than silently
//! doing the wrong thing.

use gugen::{
    Composition, Element, GugenError, InMemoryPrecursorCatalog, Planner, PlanningConfig,
    PlanningConstraints, PrecursorCandidate, PrecursorId, RejectionCode, TargetSpecification,
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

fn target(composition: Composition, constraints: PlanningConstraints) -> TargetSpecification {
    TargetSpecification {
        composition,
        structure: None,
        desired_phase: None,
        constraints,
    }
}

/// A target composition scaled far enough (10^25 formula units) to
/// overflow `Frac`'s exact `i128` arithmetic during Gauss-Jordan
/// elimination -- confirmed empirically (10^18 still balances exactly,
/// 10^25 does not) rather than assumed. `Planner::plan` must surface this
/// as a typed error, not panic.
#[test]
fn an_extreme_formula_unit_scale_overflows_cleanly_not_a_panic() {
    let scale = 1e25;
    let target_spec = target(
        composition(&[("Ba", scale), ("Ti", scale), ("O", 3.0 * scale)]),
        PlanningConstraints::default(),
    );
    let catalog = InMemoryPrecursorCatalog::new(vec![
        candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
        candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
    ]);
    let planner = Planner::offline_minimal(catalog, PlanningConfig::default());

    let result = planner.plan(&target_spec, "2026-08-14T00:00:00Z");
    assert_eq!(result, Err(GugenError::ArithmeticOverflow));
}

/// A catalog that covers none of the target's elements at all -- the
/// starkest form of "missing target element," through the full `Planner`
/// path (precursor.rs already covers this at the `search_precursor_sets`
/// layer alone).
#[test]
fn a_catalog_covering_no_target_element_is_explained_not_silently_empty() {
    let target_spec = target(
        composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
        PlanningConstraints::default(),
    );
    let catalog =
        InMemoryPrecursorCatalog::new(vec![candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)])]);
    let planner = Planner::offline_minimal(catalog, PlanningConfig::default());

    let report = planner.plan(&target_spec, "2026-08-14T00:00:00Z").unwrap();
    assert!(report.plans.is_empty());
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.message.contains("no candidates")),
        "{:?}",
        report.warnings
    );
}

/// A search budget too tight to evaluate every combination must report
/// `SearchBudgetExhausted` end to end through `Planner`, not just at the
/// `search_precursor_sets` unit level (precursor.rs already covers that
/// layer alone) -- and must not be confused with "no candidates" (AGENTS.md
/// §9).
#[test]
fn a_tight_search_budget_reports_exhaustion_not_absence_through_the_full_planner() {
    let target_spec = target(
        composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
        PlanningConstraints::default(),
    );
    let catalog = InMemoryPrecursorCatalog::new(vec![
        candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
        candidate("BaO", &[("Ba", 1.0), ("O", 1.0)]),
        candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
    ]);
    let tight_config = PlanningConfig {
        search_budget: gugen::SearchBudget {
            max_precursor_sets: 1,
            max_precursors_per_plan: 3,
            max_plans_returned: 20,
        },
        ..PlanningConfig::default()
    };
    let planner = Planner::offline_minimal(catalog, tight_config);

    let report = planner.plan(&target_spec, "2026-08-14T00:00:00Z").unwrap();
    assert!(
        report.rejected_candidates.iter().any(|r| r.reason_codes
            == vec![RejectionCode::SearchBudgetExhausted]
            && r.precursors.is_empty()),
        "{:?}",
        report.rejected_candidates
    );
}

/// A catalog entry whose composition is *exactly* the target (a compound
/// already made, offered back to itself as its own "precursor") -- a
/// degenerate but legal single-candidate case: `balance()` must solve the
/// trivial 1:1 identity, not treat reactant==product as an empty or
/// contradictory system.
#[test]
fn a_precursor_identical_to_the_target_plans_as_a_trivial_identity() {
    let target_spec = target(
        composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
        PlanningConstraints::default(),
    );
    let catalog = InMemoryPrecursorCatalog::new(vec![candidate(
        "BaTiO3",
        &[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)],
    )]);
    let planner = Planner::offline_minimal(catalog, PlanningConfig::default());

    let report = planner.plan(&target_spec, "2026-08-14T00:00:00Z").unwrap();
    // Since Phase 12, one accepted precursor set yields one plan per
    // applicable route family (currently 2: ConventionalSolidState and
    // Mechanochemical) -- both share the same trivial identity reaction.
    assert_eq!(report.plans.len(), 2, "{:?}", report.plans);
    for plan in &report.plans {
        let reaction = plan.balanced_reaction.as_ref().unwrap();
        assert_eq!(reaction.reactants().len(), 1);
        assert_eq!(reaction.reactants()[0].coefficient(), 1);
        assert_eq!(reaction.products()[0].coefficient(), 1);
    }
    let route_families: std::collections::BTreeSet<_> =
        report.plans.iter().map(|p| p.route_family).collect();
    assert_eq!(
        route_families.len(),
        2,
        "the two plans must be distinct route families, not two copies of the same one: {:?}",
        report.plans
    );
}

/// AGENTS.md §26 Phase 6's invalid-target handling, re-checked end to end
/// with a *multi*-element contradiction (both Ba and O forbidden, not just
/// one element) -- `planner.rs`'s own test only exercises a single
/// contradictory element.
#[test]
fn a_target_contradictory_on_multiple_elements_abstains_naming_all_of_them() {
    let mut constraints = PlanningConstraints::default();
    constraints.forbidden_elements.insert(element("Ba"));
    constraints.forbidden_elements.insert(element("O"));
    let target_spec = target(
        composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
        constraints,
    );
    let catalog = InMemoryPrecursorCatalog::new(vec![
        candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
        candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
    ]);
    let planner = Planner::offline_minimal(catalog, PlanningConfig::default());

    let report = planner.plan(&target_spec, "2026-08-14T00:00:00Z").unwrap();
    assert!(report.plans.is_empty());
    assert_eq!(
        report.applicability.level,
        gugen::ApplicabilityLevel::OutOfDomain
    );
    let rationale = report.applicability.rationale.join(" ");
    assert!(
        rationale.contains("Ba") && rationale.contains('O'),
        "{rationale}"
    );
}

/// `assess_applicability` can currently only reach `OutOfDomain` via
/// self-contradiction (planner.rs's doc comment on `assess_applicability`).
/// A target with a *structure* present but still no real classifier
/// backing it must stay `PartiallyInDomain`, not silently read as
/// `InDomain` just because a `TargetStructure` was supplied -- the exact
/// overclaim the Phase 6 advisor review caught and fixed once already;
/// this pins it against regression through the adversarial suite.
#[test]
fn a_target_with_unclassified_structure_never_overclaims_in_domain() {
    let target_spec = TargetSpecification {
        composition: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
        structure: Some(gugen::TargetStructure {
            description: "cubic perovskite, Pm-3m".to_string(),
        }),
        desired_phase: None,
        constraints: PlanningConstraints::default(),
    };
    let catalog = InMemoryPrecursorCatalog::new(vec![
        candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
        candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
    ]);
    let planner = Planner::offline_minimal(catalog, PlanningConfig::default());

    let report = planner.plan(&target_spec, "2026-08-14T00:00:00Z").unwrap();
    assert_eq!(
        report.applicability.level,
        gugen::ApplicabilityLevel::PartiallyInDomain
    );
}
