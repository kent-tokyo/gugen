//! Phase 20B's explicit non-goal, checked as a permanent regression guard
//! rather than left as "true because nothing calls it yet" (mirrors
//! `tests/thermodynamics_ranking_invariance.rs`'s own Phase 19P guard):
//! loading a corpus snapshot and running `find_exact`/`cross_doi_comparisons`
//! queries against it, with **no** `LiteratureEvidenceProvider`
//! configured, must have zero effect on `Planner::plan`'s output. Its
//! value is as a tripwire: if a future change wires this module into the
//! Planner's *scoring* path (as opposed to the v0.4.0 Integration
//! phase's reference-only `SynthesisPlan.literature_evidence` display
//! field, which deliberately never reaches `score_plan`, see
//! `docs/literature_evidence_integration.md`) without deliberately
//! updating this test, the test itself is the signal that the boundary
//! the owner drew (no automatic `ConditionPrecedent` promotion until
//! `HeatingPurpose` accuracy against this corpus is validated) was just
//! crossed. `planner.rs`'s own test module carries the *with-a-provider-
//! configured* analogue of this guard
//! (`literature_evidence_provider_attaches_evidence_without_changing_score_or_steps`),
//! since exercising that path needs a `LiteratureEvidenceProvider` test
//! double this file's `literature_corpus`-gated fixture-only setup
//! doesn't have.

use gugen::{
    Composition, Element, InMemoryPrecursorCatalog, LiteratureObservationCorpus, LoadMode, Planner,
    PlanningConfig, PlanningConstraints, PrecursorCandidate, PrecursorId, RouteFamily,
    TargetSpecification,
};

const FIXTURE: &str = include_str!("fixtures/literature_observation_snapshot_sample.json");

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

#[test]
fn loading_and_querying_the_corpus_does_not_change_planning_output() {
    let planner = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(barium_titanate_catalog()),
        PlanningConfig::default(),
    );
    let target_spec = target(composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]));

    let report_before = planner.plan(&target_spec, "2026-08-15T00:00:00Z").unwrap();

    // Independently exercise the whole Phase 20B surface for this same
    // target -- entirely disconnected from `planner`/`score_plan`, run
    // here specifically to prove it has no side effect on subsequent
    // planning output.
    let (corpus, report) = LiteratureObservationCorpus::load(FIXTURE, LoadMode::Lenient).unwrap();
    assert!(report.rejected.is_empty());
    let sr_ferrite = composition(&[("Sr", 1.0), ("Fe", 12.0), ("O", 19.0)]);
    let fe2o3 = composition(&[("Fe", 2.0), ("O", 3.0)]);
    let srco3 = composition(&[("Sr", 1.0), ("C", 1.0), ("O", 3.0)]);
    let matches = corpus.find_exact(
        RouteFamily::ConventionalSolidState,
        &sr_ferrite,
        &[fe2o3, srco3],
    );
    // A real, non-empty result -- this must be a genuinely exercised
    // query, not a no-op that happens to prove nothing either way.
    assert_eq!(matches.len(), 2);

    let report_after = planner.plan(&target_spec, "2026-08-15T00:00:00Z").unwrap();

    assert_eq!(
        report_before, report_after,
        "loading and querying the literature observation corpus must not affect planning output at all"
    );
}

#[test]
fn cross_doi_comparisons_does_not_change_planning_output() {
    let planner = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(barium_titanate_catalog()),
        PlanningConfig::default(),
    );
    let target_spec = target(composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]));

    let report_before = planner.plan(&target_spec, "2026-08-15T00:00:00Z").unwrap();

    let (corpus, report) = LiteratureObservationCorpus::load(FIXTURE, LoadMode::Lenient).unwrap();
    assert!(report.rejected.is_empty());
    // Exercise the whole Phase 20C surface -- entirely disconnected from
    // `planner`/`score_plan`, run here specifically to prove it has no
    // side effect on subsequent planning output.
    let _assessments = corpus.cross_doi_comparisons();

    let report_after = planner.plan(&target_spec, "2026-08-15T00:00:00Z").unwrap();

    assert_eq!(
        report_before, report_after,
        "cross_doi_comparisons() must not affect planning output at all"
    );
}
