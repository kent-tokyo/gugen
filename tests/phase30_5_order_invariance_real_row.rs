//! Phase 30.5 order-invariance investigation: minimal real-row
//! reproduction (owner-mandated sequencing, step 2 -- after the synthetic
//! fixture in `src/precursor.rs`'s own test module, before any fix or
//! full-corpus rerun).
//!
//! Row hardcoded from `benchmarks/data/exploration_frozen_catalog_manifest.json`,
//! target `"(Ca0.995Eu0.005)Al2O4"` -- one of the 440/2879 corpus rows
//! (Phase 30.5's own duplicate-composition audit) whose candidate pool
//! contains two `PrecursorId`s sharing an identical composition
//! (`"Al2O3"` / `"α-Al2O3"`, both Al:2.0 O:3.0), where the gold route
//! specifically names `"α-Al2O3"`.
//!
//! This asserts what should actually hold for recall: canonical
//! composition-multiset gold recovery is order-invariant between
//! catalog-exact order and a reversed order, at a budget
//! (`max_precursor_sets = 200_000`) that is provably exhaustive for this
//! row's 28-candidate, arity<=4 combinatorial space (at most
//! sum_{k=1}^{4} C(28,k) = 24,157 states, confirmed by direct
//! calculation against every row in the frozen catalog -- max pool size
//! 28, well under any `BUDGETS` entry used by the audit harness).
#![cfg(feature = "serde")]

use gugen::{
    Composition, Element, PlanningConstraints, PrecursorCandidate, PrecursorId, SearchBudget,
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

/// The real row's full 28-candidate pool, catalog-exact (on-disk JSON)
/// order.
fn real_row_candidates_catalog_order() -> Vec<PrecursorCandidate> {
    vec![
        candidate("CaCO3", &[("C", 1.0), ("Ca", 1.0), ("O", 3.0)]),
        candidate("Eu2O3", &[("Eu", 2.0), ("O", 3.0)]),
        candidate("α-Al2O3", &[("Al", 2.0), ("O", 3.0)]),
        candidate("TiO2", &[("O", 2.0), ("Ti", 1.0)]),
        candidate("Al2O3", &[("Al", 2.0), ("O", 3.0)]),
        candidate("SiO2", &[("O", 2.0), ("Si", 1.0)]),
        candidate("La2O3", &[("La", 2.0), ("O", 3.0)]),
        candidate("ZnO", &[("O", 1.0), ("Zn", 1.0)]),
        candidate("Y2O3", &[("O", 3.0), ("Y", 2.0)]),
        candidate("Bi2O3", &[("Bi", 2.0), ("O", 3.0)]),
        candidate("Nb2O5", &[("Nb", 2.0), ("O", 5.0)]),
        candidate("Fe2O3", &[("Fe", 2.0), ("O", 3.0)]),
        candidate("SrCO3", &[("C", 1.0), ("O", 3.0), ("Sr", 1.0)]),
        candidate("MgO", &[("Mg", 1.0), ("O", 1.0)]),
        candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
        candidate("Li2CO3", &[("C", 1.0), ("Li", 2.0), ("O", 3.0)]),
        candidate("CeO2", &[("Ce", 1.0), ("O", 2.0)]),
        candidate("ZrO2", &[("O", 2.0), ("Zr", 1.0)]),
        candidate("Ga2O3", &[("Ga", 2.0), ("O", 3.0)]),
        candidate("Al", &[("Al", 1.0)]),
        candidate("Gd2O3", &[("Gd", 2.0), ("O", 3.0)]),
        candidate("CuO", &[("Cu", 1.0), ("O", 1.0)]),
        candidate("WO3", &[("O", 3.0), ("W", 1.0)]),
        candidate("Na2CO3", &[("C", 1.0), ("Na", 2.0), ("O", 3.0)]),
        candidate("Ta2O5", &[("O", 5.0), ("Ta", 2.0)]),
        candidate("Yb2O3", &[("O", 3.0), ("Yb", 2.0)]),
        candidate("MoO3", &[("Mo", 1.0), ("O", 3.0)]),
        candidate("V2O5", &[("O", 5.0), ("V", 2.0)]),
    ]
}

fn target_composition() -> Composition {
    composition(&[("Al", 2.0), ("Ca", 0.995), ("Eu", 0.005), ("O", 4.0)])
}

/// Provably exhaustive for this row: 28 candidates, `max_precursors_per_plan`
/// = 4 (default) -> at most sum_{k=1}^{4} C(28,k) = 24,157 states
/// genuinely considered, far under this budget.
fn exhaustive_budget() -> SearchBudget {
    SearchBudget {
        max_precursor_sets: 200_000,
        ..SearchBudget::default()
    }
}

fn canonical_label(id: &str) -> &'static str {
    match id {
        "Al2O3" | "α-Al2O3" => "Al-oxide-comp",
        "CaCO3" => "CaCO3-comp",
        "Eu2O3" => "Eu2O3-comp",
        other => panic!("unexpected id in this row's gold-relevant output: {other}"),
    }
}

#[test]
fn real_row_gold_canonical_recovery_is_order_invariant_at_exhaustive_budget() {
    use gugen::search_precursor_sets;
    use std::collections::BTreeSet;

    let target = target_composition();
    let budget = exhaustive_budget();
    // Gold route from the real corpus: ["CaCO3", "Eu2O3", "α-Al2O3"]
    // (the α-labeled polymorph specifically, not "Al2O3").
    let gold_canonical: BTreeSet<&str> = ["CaCO3-comp", "Eu2O3-comp", "Al-oxide-comp"]
        .into_iter()
        .collect();

    let catalog_order = real_row_candidates_catalog_order();
    let mut reversed_order = catalog_order.clone();
    reversed_order.reverse();

    for (name, candidates) in [
        ("catalog-exact", &catalog_order),
        ("reversed", &reversed_order),
    ] {
        let outcome = search_precursor_sets(
            &target,
            candidates,
            &PlanningConstraints::default(),
            &budget,
        )
        .expect("search must not error on a well-formed real row");
        assert!(
            !outcome.rejected.iter().any(|r| matches!(
                r.reason_codes.first(),
                Some(gugen::RejectionCode::SearchBudgetExhausted)
            )),
            "{name}: fixture budget must be genuinely exhaustive for this row"
        );

        let canonical_recovered = outcome.accepted.iter().any(|a| {
            let labels: BTreeSet<&str> = a
                .precursors
                .iter()
                .map(|p| canonical_label(p.0.as_str()))
                .collect();
            labels == gold_canonical
        });
        assert!(
            canonical_recovered,
            "{name}: canonical composition-multiset gold route must be recoverable \
            at exhaustive budget regardless of candidate array order. accepted={:?}",
            outcome.accepted
        );

        let exact_id_recovered = outcome.accepted.iter().any(|a| {
            let ids: BTreeSet<&str> = a.precursors.iter().map(|p| p.0.as_str()).collect();
            ids == BTreeSet::from(["CaCO3", "Eu2O3", "α-Al2O3"])
        });
        eprintln!(
            "{name}: accepted_count={}, exact_id(α-Al2O3)_recovered={}, accepted={:?}",
            outcome.accepted.len(),
            exact_id_recovered,
            outcome.accepted
        );
    }
}
