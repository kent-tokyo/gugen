//! AGENTS.md §21.4: results must be invariant to target element order,
//! precursor catalog order, provider return order, unrelated-precursor
//! addition, and JSON field order. What's allowed to vary must be
//! documented, not silently different.

use gugen::{
    BalancedReaction, Composition, Element, InMemoryPrecursorCatalog, Planner, PlanningConfig,
    PlanningConstraints, PrecursorCandidate, PrecursorId, PrecursorSelection, ProcessPrecedent,
    ProviderError, TargetSpecification, ThermodynamicConditions,
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

fn target(composition: Composition) -> TargetSpecification {
    TargetSpecification {
        composition,
        structure: None,
        desired_phase: None,
        constraints: PlanningConstraints::default(),
    }
}

/// Target element *insertion* order must not matter -- `Composition`
/// stores by `BTreeMap<Element, _>`, so this is close to a tautology, but
/// it is exactly what §21.4 asks for and worth pinning end-to-end through
/// `Planner` rather than trusting the type's internals never to change.
#[test]
fn target_element_order_does_not_affect_the_report() {
    let a = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
    let b = composition(&[("O", 3.0), ("Ba", 1.0), ("Ti", 1.0)]);
    assert_eq!(
        a, b,
        "Composition must already be order-invariant by construction"
    );

    let catalog = InMemoryPrecursorCatalog::new(barium_titanate_catalog());
    let planner = Planner::offline_minimal(catalog, PlanningConfig::default());
    let report_a = planner.plan(&target(a), "2026-08-14T00:00:00Z").unwrap();
    let report_b = planner.plan(&target(b), "2026-08-14T00:00:00Z").unwrap();
    assert_eq!(report_a, report_b);
}

/// Catalog insertion order must not affect which plans are produced or
/// how they're ranked -- `precursor.rs` already covers this at the
/// `search_precursor_sets` layer; this pins it through the full `Planner`
/// (ranking + plan_id assignment included).
#[test]
fn catalog_insertion_order_does_not_affect_the_ranked_report() {
    let target_spec = target(composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]));
    let mut reversed = barium_titanate_catalog();
    reversed.reverse();

    let forward = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(barium_titanate_catalog()),
        PlanningConfig::default(),
    )
    .plan(&target_spec, "2026-08-14T00:00:00Z")
    .unwrap();
    let backward = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(reversed),
        PlanningConfig::default(),
    )
    .plan(&target_spec, "2026-08-14T00:00:00Z")
    .unwrap();

    assert_eq!(forward, backward);
}

/// Adding a precursor irrelevant to the target must not change the plans
/// produced for it (only `rejected_candidates` may grow, since the new
/// candidate can enter -- and fail -- combinations of its own). Extends
/// `planner.rs`'s `plan_id_is_stable_when_an_unrelated_candidate_is_added`
/// (which checks `plan_id` alone) to the full plan content.
#[test]
fn adding_an_unrelated_precursor_does_not_change_the_target_relevant_plans() {
    let target_spec = target(composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]));
    let baseline = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(barium_titanate_catalog()),
        PlanningConfig::default(),
    )
    .plan(&target_spec, "2026-08-14T00:00:00Z")
    .unwrap();

    let mut augmented_candidates = barium_titanate_catalog();
    augmented_candidates.push(candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]));
    let augmented = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(augmented_candidates),
        PlanningConfig::default(),
    )
    .plan(&target_spec, "2026-08-14T00:00:00Z")
    .unwrap();

    assert_eq!(
        baseline.plans, augmented.plans,
        "an unrelated precursor must not change the target-relevant plans"
    );
}

/// A `ProcessEvidenceProvider` returning the same precedents in a
/// different order must not change the resulting score or confidence --
/// `evidence_strength` aggregates by minimum (order-independent) and
/// `evidence_coverage` is presence-only (score.rs), so order must be
/// invisible to both.
struct OrderedPrecedentProvider {
    descriptions: Vec<&'static str>,
}
impl gugen::ProcessEvidenceProvider for OrderedPrecedentProvider {
    fn precedents(
        &self,
        _target: &TargetSpecification,
        _precursors: &[PrecursorSelection],
    ) -> Result<Vec<ProcessPrecedent>, ProviderError> {
        Ok(self
            .descriptions
            .iter()
            .map(|d| ProcessPrecedent {
                description: d.to_string(),
            })
            .collect())
    }
}
struct NoThermodynamicData;
impl gugen::ThermodynamicProvider for NoThermodynamicData {
    fn reaction_energy(
        &self,
        _reaction: &BalancedReaction,
        _conditions: &ThermodynamicConditions,
    ) -> Result<Option<gugen::ReactionEnergy>, ProviderError> {
        Ok(None)
    }
}

#[test]
fn provider_return_order_does_not_affect_score_or_confidence() {
    let target_spec = target(composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]));
    let forward = Planner::new(
        InMemoryPrecursorCatalog::new(barium_titanate_catalog()),
        OrderedPrecedentProvider {
            descriptions: vec!["precedent A", "precedent B"],
        },
        NoThermodynamicData,
        PlanningConfig::default(),
    )
    .plan(&target_spec, "2026-08-14T00:00:00Z")
    .unwrap();
    let backward = Planner::new(
        InMemoryPrecursorCatalog::new(barium_titanate_catalog()),
        OrderedPrecedentProvider {
            descriptions: vec!["precedent B", "precedent A"],
        },
        NoThermodynamicData,
        PlanningConfig::default(),
    )
    .plan(&target_spec, "2026-08-14T00:00:00Z")
    .unwrap();

    assert_eq!(forward.plans.len(), backward.plans.len());
    for (a, b) in forward.plans.iter().zip(&backward.plans) {
        assert_eq!(
            a.score, b.score,
            "score must not depend on provider return order"
        );
        assert_eq!(
            a.confidence, b.confidence,
            "confidence must not depend on provider return order"
        );
    }
}

/// **Not invariant, and documented as such rather than silently left
/// untested (AGENTS.md §21.4: "変化してよい項目は文書化してください" --
/// document what's allowed to change; the corollary is that anything
/// *not* on that list which does change is worth stating plainly, not
/// hiding).
///
/// AGENTS.md §21.4 lists "equivalent formula normalization" as something
/// results should be invariant to. `BaTiO3` and `Ba2Ti2O6` are the same
/// real material at a different formula-unit scale. In gugen today they
/// are NOT treated as equivalent: `plan_id` and the literal reaction
/// coefficients differ between them.
///
/// This is not a bug in `balance()` (which correctly, exactly balances
/// whatever target `Composition` it is given) or in `derive_plan_id`
/// (which correctly reflects that a numerically different reaction system
/// was solved). It traces to a genuine, *load-bearing* design choice
/// tested since Phase 1
/// (`composition::tests::ordinary_decimal_amounts_round_trip_exactly`):
/// `Composition` preserves a caller's exact given amounts rather than
/// reducing them to a canonical GCD-minimal form. That preservation is
/// required for doped/solid-solution formulas (e.g. `La0.67Sr0.33MnO3`,
/// where the decimal *is* the scientifically meaningful doping level) --
/// blindly GCD-reducing every `Composition` at construction would rescale
/// those too, which would be worse, not better. There is no narrow fix
/// that adds formula-unit-scale equivalence without either breaking that
/// guarantee or inventing a second, decoupled notion of "canonical scale"
/// for `plan_id`/`BalancedReaction` -- a real design fork, not a bug fix,
/// and out of scope for this phase to decide unilaterally. See
/// `tasks/todo.md`'s Phase 8 section for the full AGENTS.md §28-format
/// report on this finding.
#[test]
fn formula_unit_scale_is_not_currently_normalized_a_documented_gap() {
    let one_unit = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
    let two_units = composition(&[("Ba", 2.0), ("Ti", 2.0), ("O", 6.0)]);
    assert_ne!(
        one_unit, two_units,
        "if this ever becomes equal, Composition gained normalization and this whole test \
        (and its accompanying tasks/todo.md report) needs to be revisited, not just deleted"
    );

    let planner_a = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(barium_titanate_catalog()),
        PlanningConfig::default(),
    );
    let planner_b = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(barium_titanate_catalog()),
        PlanningConfig::default(),
    );
    let report_a = planner_a
        .plan(&target(one_unit), "2026-08-14T00:00:00Z")
        .unwrap();
    let report_b = planner_b
        .plan(&target(two_units), "2026-08-14T00:00:00Z")
        .unwrap();

    assert_eq!(report_a.plans.len(), 1);
    assert_eq!(report_b.plans.len(), 1);
    assert_ne!(
        report_a.plans[0].plan_id, report_b.plans[0].plan_id,
        "documenting current (non-invariant) behavior -- see the doc comment above"
    );
    let reaction_b = report_b.plans[0].balanced_reaction.as_ref().unwrap();
    assert!(
        reaction_b.reactants.iter().any(|s| s.coefficient == 2),
        "the doubled target forces doubled reactant coefficients rather than being reduced \
        back to the minimal BaTiO3 system: {reaction_b:?}"
    );
}
