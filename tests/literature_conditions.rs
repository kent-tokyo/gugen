//! Phase 10: `InMemoryLiteratureConditionProvider` end to end through
//! `Planner::with_process_evidence_provider`, using the same real, cited
//! literature routes `tests/validation.rs` already recovers precursor
//! sets for -- these tests check that the *conditions* on those same
//! routes now actually resolve, not just the precursor set.

use gugen::{
    Composition, ConditionPrecedent, DurationRange, Element, EvidenceKind, EvidenceScope,
    EvidenceStrength, HeatingPurpose, InMemoryLiteratureConditionProvider,
    InMemoryPrecursorCatalog, Planner, PlanningConfig, PlanningConstraints, PrecursorCandidate,
    PrecursorId, PrecursorSelection, ProcessEvidenceProvider, ProcessPrecedent, ProcessStep,
    ProviderError, TargetSpecification, TemperatureRange,
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

fn plan_with_literature_conditions(
    target_spec: &TargetSpecification,
    catalog: Vec<PrecursorCandidate>,
) -> gugen::SynthesisPlanningReport {
    Planner::with_process_evidence_provider(
        InMemoryPrecursorCatalog::new(catalog),
        InMemoryLiteratureConditionProvider::from_curated_records(),
        PlanningConfig::default(),
    )
    .plan(target_spec, "2026-08-14T00:00:00Z")
    .unwrap()
}

/// CaCO3 -> CaO + CO2 releases a byproduct, so the template's Calcination
/// step is exactly what the curated CaO record (900 C, 1 h) targets.
#[test]
fn cao_calcination_temperature_and_duration_resolve_from_the_curated_record() {
    let target_spec = target(composition(&[("Ca", 1.0), ("O", 1.0)]));
    let report = plan_with_literature_conditions(
        &target_spec,
        vec![candidate("CaCO3", &[("Ca", 1.0), ("C", 1.0), ("O", 3.0)])],
    );

    assert!(!report.plans.is_empty());
    let plan = &report.plans[0];
    let calcination = plan
        .steps
        .iter()
        .find_map(|s| match &s.step {
            ProcessStep::Heat {
                purpose: gugen::HeatingPurpose::Calcination,
                temperature,
                duration,
                ..
            } => Some((temperature, duration)),
            _ => None,
        })
        .expect("carbonate route must have a Calcination step");
    assert_eq!(calcination.0.unwrap().min_celsius, 900.0);
    assert_eq!(calcination.1.unwrap().min_hours, 1.0);

    assert!(
        plan.evidence
            .iter()
            .any(|e| e.kind == EvidenceKind::CuratedLiteratureRecord
                && e.source_id.as_deref() == Some("10.3390/ma17153875")),
        "resolved condition must carry the real DOI, not a synthesized source_id: {:?}",
        plan.evidence
    );
    assert!(
        plan.confidence.overall.value() > 0.75,
        "a plan with a resolved condition must score a higher confidence.overall \
        than the structurally-constant 0.75 every unresolved plan gets: {:?}",
        plan.confidence
    );
}

/// MgO + Al2O3 -> MgAl2O4 has no byproduct, so the template has exactly
/// one Heat step (Sintering) -- the curated record's second-stage
/// temperature (1725 C, 6 h) is what should land there.
#[test]
fn mgal2o4_sintering_temperature_and_duration_resolve_from_the_curated_record() {
    let target_spec = target(composition(&[("Mg", 1.0), ("Al", 2.0), ("O", 4.0)]));
    let report = plan_with_literature_conditions(
        &target_spec,
        vec![
            candidate("MgO", &[("Mg", 1.0), ("O", 1.0)]),
            candidate("Al2O3", &[("Al", 2.0), ("O", 3.0)]),
        ],
    );

    assert!(!report.plans.is_empty());
    let plan = &report.plans[0];
    let sintering = plan
        .steps
        .iter()
        .find_map(|s| match &s.step {
            ProcessStep::Heat {
                purpose: gugen::HeatingPurpose::Sintering,
                temperature,
                duration,
                ..
            } => Some((temperature, duration)),
            _ => None,
        })
        .expect("oxide route must have a Sintering step");
    assert_eq!(sintering.0.unwrap().min_celsius, 1725.0);
    assert_eq!(sintering.1.unwrap().min_hours, 6.0);
}

/// La2O3 + Al2O3 -> LaAlO3 has no byproduct, so (like MgAl2O4) the
/// template has exactly one Heat step (Sintering).
#[test]
fn laalo3_sintering_temperature_and_duration_resolve_from_the_curated_record() {
    let target_spec = target(composition(&[("La", 1.0), ("Al", 1.0), ("O", 3.0)]));
    let report = plan_with_literature_conditions(
        &target_spec,
        vec![
            candidate("La2O3", &[("La", 2.0), ("O", 3.0)]),
            candidate("Al2O3", &[("Al", 2.0), ("O", 3.0)]),
        ],
    );

    assert!(!report.plans.is_empty());
    let plan = &report.plans[0];
    let sintering = plan
        .steps
        .iter()
        .find_map(|s| match &s.step {
            ProcessStep::Heat {
                purpose: gugen::HeatingPurpose::Sintering,
                temperature,
                duration,
                ..
            } => Some((temperature, duration)),
            _ => None,
        })
        .expect("oxide route must have a Sintering step");
    assert_eq!(sintering.0.unwrap().min_celsius, 1500.0);
    assert_eq!(sintering.1.unwrap().min_hours, 5.0);
}

/// BaCO3 + TiO2 -> BaTiO3 + CO2 releases a byproduct, so both Calcination
/// and Sintering steps exist. The curated record's sintering condition is
/// a genuine range (1200-1350 C, two parallel samples in the source, not
/// one point) with no reported duration -- that field must stay
/// unresolved, not filled in from thin air.
#[test]
fn batio3_calcination_and_sintering_temperature_resolve_from_the_curated_record() {
    let target_spec = target(composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]));
    let report = plan_with_literature_conditions(
        &target_spec,
        vec![
            candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
        ],
    );

    assert!(!report.plans.is_empty());
    let plan = &report.plans[0];
    for step in &plan.steps {
        if let ProcessStep::Heat {
            purpose,
            temperature,
            duration,
            ..
        } = &step.step
        {
            match purpose {
                gugen::HeatingPurpose::Calcination => {
                    assert_eq!(temperature.unwrap().min_celsius, 1000.0);
                    assert_eq!(duration.unwrap().min_hours, 2.0);
                }
                gugen::HeatingPurpose::Sintering => {
                    assert_eq!(temperature.unwrap().min_celsius, 1200.0);
                    assert_eq!(temperature.unwrap().max_celsius, 1350.0);
                    assert!(
                        duration.is_none(),
                        "sintering duration is not stated in the source and must stay \
                        unresolved, not fabricated: {duration:?}"
                    );
                }
                other => panic!("unexpected heating purpose: {other:?}"),
            }
        }
    }
}

/// Zn3(PO4)2's curated record was authored from a ZnO + (NH4)2HPO4 route
/// (the real precursors the source paper used) -- a *different* precursor
/// combination than the ZnO + P2O5 route this test constructs directly.
/// (`tests/validation.rs` used to carry its own named Zn3(PO4)2/ZnO+P2O5
/// fixture too; Phase 14 replaced it with a different, better-attested
/// target after finding that specific route has zero independent
/// attestations in the correctly-licensed corpus -- this test's own
/// ZnO + P2O5 setup below is independent of that fixture and unaffected.)
/// Planning that ZnO + P2O5 route must still pick up the record's
/// Sintering condition (same target material), but scoped
/// `SimilarMaterial`, not `ExactTarget`, since the precursor set genuinely
/// doesn't match what the citation reports.
#[test]
fn zn3po42_from_a_different_precursor_route_resolves_as_similar_material_not_exact_target() {
    let target_spec = target(composition(&[("Zn", 3.0), ("P", 2.0), ("O", 8.0)]));
    let report = plan_with_literature_conditions(
        &target_spec,
        vec![
            candidate("ZnO", &[("Zn", 1.0), ("O", 1.0)]),
            candidate("P2O5", &[("P", 2.0), ("O", 5.0)]),
        ],
    );

    assert!(!report.plans.is_empty());
    let plan = &report.plans[0];
    let mut precursor_ids: Vec<&str> = plan
        .precursors
        .iter()
        .map(|p| p.precursor.0.as_str())
        .collect();
    precursor_ids.sort_unstable();
    assert_eq!(
        precursor_ids,
        vec!["P2O5", "ZnO"],
        "this test's SimilarMaterial claim only holds for the ZnO + P2O5 route; \
        plans[0] resolved to a different precursor set: {:?}",
        plan.precursors
    );
    let sintering = plan
        .steps
        .iter()
        .find_map(|s| match &s.step {
            ProcessStep::Heat {
                purpose: gugen::HeatingPurpose::Sintering,
                temperature,
                ..
            } => Some(temperature),
            _ => None,
        })
        .expect("ZnO + P2O5 route must have a Sintering step (no byproduct)");
    assert_eq!(
        sintering.unwrap().min_celsius,
        950.0,
        "the ZnO + (NH4)2HPO4-sourced record must still apply to this different, \
        same-target precursor route"
    );

    let resolved_evidence = plan
        .evidence
        .iter()
        .find(|e| {
            e.kind == EvidenceKind::CuratedLiteratureRecord
                && e.source_id.as_deref() == Some("10.3390/engproc2024067018")
        })
        .expect("resolved condition evidence must carry the real DOI");
    assert_eq!(
        resolved_evidence.applicable_to,
        EvidenceScope::SimilarMaterial,
        "a condition record from a different precursor combination must be scoped \
        SimilarMaterial, not ExactTarget: {resolved_evidence:?}"
    );
}

/// A target the curated set has no coverage for must leave every
/// condition unresolved, exactly like `Planner::offline_minimal` -- a
/// provider that finds nothing to resolve must not change what's
/// resolved. Its `UnresolvedRequirement.reason` text legitimately *does*
/// differ, on purpose (the `NO_PROVIDER_REASON` fix, score.rs): "a
/// provider was consulted and had nothing for this field" is a genuinely
/// different, more accurate fact than "no provider exists at all," so
/// this is not asserted equal to the offline report wholesale.
#[test]
fn a_target_with_no_curated_coverage_still_leaves_every_condition_unresolved() {
    let target_spec = target(composition(&[("Zn", 1.0), ("O", 1.0)]));
    let catalog = vec![candidate("ZnO", &[("Zn", 1.0), ("O", 1.0)])];

    let with_provider = plan_with_literature_conditions(&target_spec, catalog.clone());
    let offline = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(catalog),
        PlanningConfig::default(),
    )
    .plan(&target_spec, "2026-08-14T00:00:00Z")
    .unwrap();

    assert_eq!(with_provider.plans.len(), offline.plans.len());
    assert!(!with_provider.plans.is_empty());
    for (a, b) in with_provider.plans.iter().zip(&offline.plans) {
        assert_eq!(
            a.steps, b.steps,
            "an uncovered target's actual step conditions must be identical with or \
            without the provider configured"
        );
        assert_eq!(a.confidence, b.confidence);
        assert_eq!(
            a.unresolved.len(),
            b.unresolved.len(),
            "same fields are unresolved either way"
        );
        assert_ne!(
            a.unresolved, b.unresolved,
            "reason text must differ now that a provider was actually consulted \
            (see score.rs's NO_PROVIDER_REASON fix): {:?}",
            a.unresolved
        );
    }
}

/// A provider whose `precedents()` call returns two `ProcessPrecedent`s
/// disagreeing on the same field for the same purpose -- the scenario
/// Phase 19 exists for. `curated_records()` currently has only one record
/// per target, so this can't be exercised through
/// `InMemoryLiteratureConditionProvider` alone; this hand-rolled provider
/// stands in for what a future, richer condition provider (out of scope
/// here) would eventually be able to trigger for real.
struct ConflictingConditionsProvider;

impl ProcessEvidenceProvider for ConflictingConditionsProvider {
    fn precedents(
        &self,
        _target: &TargetSpecification,
        _precursors: &[PrecursorSelection],
    ) -> std::result::Result<Vec<ProcessPrecedent>, ProviderError> {
        let base = ConditionPrecedent {
            purpose: HeatingPurpose::Sintering,
            temperature: None,
            duration: Some(DurationRange::new(1.0, 1.0).unwrap()),
            atmosphere: None,
            ramp: None,
            evidence_kind: EvidenceKind::CuratedLiteratureRecord,
            source_id: None,
            statement: "test precedent".to_string(),
            strength: EvidenceStrength::Moderate,
            applicable_to: EvidenceScope::ExactTarget,
        };
        Ok(vec![
            ProcessPrecedent {
                description: String::new(),
                conditions: vec![ConditionPrecedent {
                    temperature: Some(TemperatureRange::new(900.0, 900.0).unwrap()),
                    source_id: Some("10.0000/first".to_string()),
                    ..base.clone()
                }],
            },
            ProcessPrecedent {
                description: String::new(),
                conditions: vec![ConditionPrecedent {
                    temperature: Some(TemperatureRange::new(1100.0, 1100.0).unwrap()),
                    source_id: Some("10.0000/second".to_string()),
                    ..base
                }],
            },
        ])
    }
}

/// End-to-end through `Planner::plan`: two `ProcessPrecedent`s returned
/// from a single `precedents()` call, disagreeing on temperature but
/// agreeing on duration, must resolve duration, leave temperature
/// unresolved with a conflict-specific reason citing both sources, and
/// must not depend on which `ProcessPrecedent` the provider happened to
/// list first.
#[test]
fn conflicting_precedents_from_one_provider_call_leave_the_field_unresolved_end_to_end() {
    let target_spec = target(composition(&[("Mg", 1.0), ("Al", 2.0), ("O", 4.0)]));
    let catalog = vec![
        candidate("MgO", &[("Mg", 1.0), ("O", 1.0)]),
        candidate("Al2O3", &[("Al", 2.0), ("O", 3.0)]),
    ];
    let report = Planner::with_process_evidence_provider(
        InMemoryPrecursorCatalog::new(catalog),
        ConflictingConditionsProvider,
        PlanningConfig::default(),
    )
    .plan(&target_spec, "2026-08-14T00:00:00Z")
    .unwrap();

    assert!(!report.plans.is_empty());
    let plan = &report.plans[0];
    let sintering = plan
        .steps
        .iter()
        .find_map(|s| match &s.step {
            ProcessStep::Heat {
                purpose: HeatingPurpose::Sintering,
                temperature,
                duration,
                ..
            } => Some((temperature, duration)),
            _ => None,
        })
        .expect("oxide route must have a Sintering step");
    assert!(
        sintering.0.is_none(),
        "temperature disagrees between the two precedents and must stay unresolved: {:?}",
        sintering.0
    );
    assert_eq!(
        sintering.1.unwrap().min_hours,
        1.0,
        "duration agrees across both precedents and must still resolve"
    );

    let temperature_unresolved = plan
        .unresolved
        .iter()
        .find(|u| u.description == "Sintering heating step temperature")
        .expect("temperature must be reported as unresolved");
    assert!(
        temperature_unresolved.reason.contains("10.0000/first")
            && temperature_unresolved.reason.contains("10.0000/second"),
        "the conflict reason must cite both disagreeing sources, not the generic \
        no-matching-precedent text: {:?}",
        temperature_unresolved.reason
    );
}
