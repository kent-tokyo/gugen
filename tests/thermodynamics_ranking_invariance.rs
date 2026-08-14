//! Phase 19P's explicit non-goal, checked as a permanent regression
//! guard rather than left as "true because nothing calls it yet": this
//! module's raw physical quantities (`relative_solid_gibbs_ev_per_atom`,
//! `balanced_reaction_delta_ev_per_atom`, `decomposition_margin_ev_per_atom`)
//! must have zero effect on `Planner::plan`'s output. There is currently no
//! code path connecting `thermodynamics.rs` to `score.rs`/`planner.rs` at
//! all, so this is not expected to fail today -- its value is as a
//! tripwire: if a future phase wires them together without deliberately
//! updating this test, the test itself is the signal that a boundary the
//! owner drew (`thermodynamic_support` stays `None` until an independent-
//! label-calibrated phase unlocks it) was just crossed.

use gugen::{
    Composition, Element, InMemoryPrecursorCatalog, Kelvin, Planner, PlanningConfig,
    PlanningConstraints, PrecursorCandidate, PrecursorId, SolidThermodynamicEntry,
    TargetSpecification, ThermodynamicDatasetIdentity, balanced_reaction_delta_ev_per_atom,
    decomposition_margin_ev_per_atom, relative_solid_gibbs_ev_per_atom,
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

fn dataset() -> ThermodynamicDatasetIdentity {
    ThermodynamicDatasetIdentity {
        source: "test".to_string(),
        release: "2026.08".to_string(),
        compatibility_scheme: "test-scheme".to_string(),
        snapshot_checksum: "deadbeef".to_string(),
    }
}

#[test]
fn computing_phase_19p_quantities_does_not_change_planning_output() {
    let planner = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(barium_titanate_catalog()),
        PlanningConfig::default(),
    );
    let target_spec = target(composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]));

    let report_before = planner.plan(&target_spec, "2026-08-15T00:00:00Z").unwrap();

    // Independently exercise every Phase 19P entry point for this same
    // target -- entirely disconnected from `planner`/`score_plan`, but run
    // here specifically to prove that running them has no side effect on
    // subsequent planning output.
    let batio3 = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
    let bao = composition(&[("Ba", 1.0), ("O", 1.0)]);
    let tio2 = composition(&[("Ti", 1.0), ("O", 2.0)]);
    let target_entry = SolidThermodynamicEntry::new(batio3, None, -3.5, 60.0, dataset()).unwrap();
    let bao_entry = SolidThermodynamicEntry::new(bao, None, -2.0, 20.0, dataset()).unwrap();
    let tio2_entry = SolidThermodynamicEntry::new(tio2, None, -3.0, 30.0, dataset()).unwrap();
    let t = Kelvin::new(900.0).unwrap();

    let _ = relative_solid_gibbs_ev_per_atom(&target_entry, t);
    let toy_reaction = gugen::BalancedReaction::new(
        vec![gugen::ReactionSpecies {
            composition: bao_entry.composition.clone(),
            coefficient: 1,
        }],
        vec![gugen::ReactionSpecies {
            composition: bao_entry.composition.clone(),
            coefficient: 1,
        }],
    )
    .unwrap();
    let entries = [bao_entry.clone(), tio2_entry.clone()];
    let _ = balanced_reaction_delta_ev_per_atom(&toy_reaction, &entries, t);
    let _ =
        decomposition_margin_ev_per_atom(&target_entry, &[(bao_entry, 1.0), (tio2_entry, 1.0)], t);

    let report_after = planner.plan(&target_spec, "2026-08-15T00:00:00Z").unwrap();

    assert_eq!(
        report_before, report_after,
        "computing Phase 19P quantities must not affect planning output at all"
    );
    for plan in &report_after.plans {
        assert_eq!(
            plan.score.thermodynamic_support, None,
            "thermodynamic_support must stay None regardless of Phase 19P"
        );
    }
}
