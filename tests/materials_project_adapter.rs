//! End-to-end: `MaterialsProjectSnapshotProvider` plugged into
//! `Planner::new` as a `ThermodynamicProvider`, mirroring how
//! `tests/provider_failures.rs` exercises the trait through the full
//! `Planner::plan` pipeline rather than only at the unit level (Phase 13).

use gugen::{
    CompetingPhase, Composition, Element, EvidenceKind, InMemoryPrecursorCatalog,
    MaterialsProjectSnapshotProvider, Planner, PlanningConfig, PlanningConstraints,
    PrecursorCandidate, PrecursorId, PrecursorSelection, ProcessEvidenceProvider, ProcessPrecedent,
    ProviderError, TargetSpecification,
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

struct NoPrecedents;
impl ProcessEvidenceProvider for NoPrecedents {
    fn precedents(
        &self,
        _target: &TargetSpecification,
        _precursors: &[PrecursorSelection],
    ) -> std::result::Result<Vec<ProcessPrecedent>, ProviderError> {
        Ok(vec![])
    }
}

fn batio3_target() -> TargetSpecification {
    TargetSpecification {
        composition: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
        structure: None,
        desired_phase: None,
        constraints: PlanningConstraints::default(),
    }
}

/// A snapshot with every species BaCO3 + TiO2 -> BaTiO3 + CO2 needs, plus
/// one unrelated-but-element-sharing entry to exercise `competing_phases`.
/// Formation energies are synthetic (this test doesn't need them to be
/// real Materials Project values, only finite and self-consistent) --
/// AGENTS.md §3's "no fabricated numeric conditions" constrains *process*
/// conditions (temperature/time/atmosphere), not a test fixture's
/// thermodynamic input data, same distinction Phase 8's fixtures rely on.
fn snapshot_provider() -> MaterialsProjectSnapshotProvider {
    MaterialsProjectSnapshotProvider::from_entries(vec![
        CompetingPhase::new(composition(&[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]), -3.5).unwrap(),
        CompetingPhase::new(composition(&[("Ti", 1.0), ("O", 2.0)]), -9.7).unwrap(),
        CompetingPhase::new(composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]), -3.5).unwrap(),
        CompetingPhase::new(composition(&[("C", 1.0), ("O", 2.0)]), -4.1).unwrap(),
        // Shares Ba with the target, not part of the reaction itself.
        CompetingPhase::new(composition(&[("Ba", 1.0), ("O", 1.0)]), -3.0).unwrap(),
    ])
}

/// The carbonate route's every species is in the snapshot: `reaction_energy`
/// resolves to `Some`, and `competing_phases` finds exactly the BaO entry
/// (shares Ba, isn't the target itself) -- both become `EvidenceKind::
/// ThermodynamicData` entries on the resulting plans. Pins the count at
/// exactly 1, not just "non-empty": `snapshot_provider`'s other three
/// non-target entries (BaCO3, TiO2, CO2) are this reaction's own
/// precursors/byproduct, which `Planner::plan` must exclude from
/// "competing phase" evidence -- reporting a plan's own reaction
/// participants as if they were competing with it would be a
/// false-confidence-shaped claim (AGENTS.md §21 audit).
#[test]
fn a_full_snapshot_produces_both_reaction_energy_and_competing_phase_evidence() {
    let catalog = InMemoryPrecursorCatalog::new(vec![
        candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
        candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
    ]);
    let report = Planner::builder(catalog, PlanningConfig::default())
        .process_evidence_provider(NoPrecedents)
        .thermodynamic_provider(snapshot_provider())
        .build()
        .plan(&batio3_target(), "2026-08-14T00:00:00Z")
        .unwrap();

    assert_eq!(
        report.plans.len(),
        2,
        "one accepted precursor set, both route families (Phase 12)"
    );
    for plan in &report.plans {
        let thermodynamic: Vec<_> = plan
            .evidence
            .iter()
            .filter(|e| e.kind == EvidenceKind::ThermodynamicData)
            .collect();
        assert_eq!(
            thermodynamic.len(),
            2,
            "expected one reaction_energy entry and one competing_phases entry: {:?}",
            plan.evidence
        );
        assert!(
            thermodynamic
                .iter()
                .any(|e| e.statement.contains("reaction energy")),
            "{:?}",
            thermodynamic
        );
        assert!(
            thermodynamic
                .iter()
                .any(|e| e.statement.contains("1 competing phase(s)")),
            "expected exactly 1 competing phase (BaO) -- this reaction's own precursors \
            (BaCO3, TiO2) and byproduct (CO2) must be excluded, not counted as \"competing\": \
            {:?}",
            thermodynamic
        );
        // AGENTS.md §4.3: this data must never move the numeric score.
        assert!(plan.score.thermodynamic_support.is_none());
    }
}

/// A snapshot missing one reactant's exact composition: `reaction_energy`
/// must abstain (`Ok(None)`, no evidence) rather than silently averaging
/// over what it does have -- proven end to end here, not just at the
/// adapter's own unit-test level.
#[test]
fn a_snapshot_missing_one_species_produces_no_reaction_energy_evidence() {
    let catalog = InMemoryPrecursorCatalog::new(vec![
        candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
        candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
    ]);
    // No TiO2 entry -- reaction_energy cannot resolve for either route
    // family (they share the same underlying reaction).
    let incomplete_provider = MaterialsProjectSnapshotProvider::from_entries(vec![
        CompetingPhase::new(composition(&[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]), -3.5).unwrap(),
        CompetingPhase::new(composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]), -3.5).unwrap(),
        CompetingPhase::new(composition(&[("C", 1.0), ("O", 2.0)]), -4.1).unwrap(),
    ]);
    let report = Planner::builder(catalog, PlanningConfig::default())
        .process_evidence_provider(NoPrecedents)
        .thermodynamic_provider(incomplete_provider)
        .build()
        .plan(&batio3_target(), "2026-08-14T00:00:00Z")
        .unwrap();

    assert!(!report.plans.is_empty());
    for plan in &report.plans {
        assert!(
            !plan
                .evidence
                .iter()
                .any(|e| e.kind == EvidenceKind::ThermodynamicData
                    && e.statement.contains("reaction energy")),
            "missing TiO2 must suppress reaction-energy evidence entirely, not partially: {:?}",
            plan.evidence
        );
    }
}
