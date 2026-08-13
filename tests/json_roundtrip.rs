use gugen::{
    ApplicabilityAssessment, ApplicabilityLevel, Composition, Element, PlanId, PlanningProvenance,
    SCHEMA_VERSION, SynthesisPlan, SynthesisPlanningReport, TargetSummary, TemperatureRange,
};

fn sample_report() -> SynthesisPlanningReport {
    let composition = Composition::new([
        (Element::new("Ba").unwrap(), 1.0),
        (Element::new("Ti").unwrap(), 1.0),
        (Element::new("O").unwrap(), 3.0),
    ])
    .unwrap();

    SynthesisPlanningReport {
        schema_version: SCHEMA_VERSION,
        target: TargetSummary {
            composition,
            structure_present: false,
            desired_phase: None,
        },
        applicability: ApplicabilityAssessment {
            level: ApplicabilityLevel::InDomain,
            rationale: vec!["bulk inorganic, formula-only target".to_string()],
        },
        plans: vec![SynthesisPlan {
            plan_id: PlanId("plan-0001".to_string()),
        }],
        rejected_candidates: vec![],
        unresolved: vec![],
        warnings: vec![],
        provenance: PlanningProvenance {
            gugen_version: PlanningProvenance::gugen_version().to_string(),
            build_identifier: None,
            schema_version: SCHEMA_VERSION,
            chematic_crystal_version: None,
            mikiwame_version: None,
            precursor_catalog_version: None,
            thermodynamic_provider_version: None,
            process_template_version: None,
            ranking_config_digest: None,
            execution_timestamp: "2026-08-13T00:00:00Z".to_string(),
            deterministic_seed: 0,
            enabled_features: vec![],
        },
    }
}

#[test]
fn synthesis_planning_report_round_trips_through_json() {
    let report = sample_report();

    let json = serde_json::to_string_pretty(&report).expect("serialize");
    let round_tripped: SynthesisPlanningReport = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(report, round_tripped);
}

#[test]
fn inverted_temperature_range_is_rejected_on_deserialize() {
    let bad = r#"{"min_celsius": 900.0, "max_celsius": 700.0}"#;
    let result: Result<TemperatureRange, _> = serde_json::from_str(bad);
    assert!(result.is_err());
}

#[test]
fn negative_element_amount_is_rejected_on_deserialize() {
    let bad = r#"{"Ba": 1.0, "Ti": 1.0, "O": -3.0}"#;
    let result: Result<Composition, _> = serde_json::from_str(bad);
    assert!(result.is_err());
}
