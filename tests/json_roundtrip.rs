use gugen::{
    AcceptedPrecursorSet, ApplicabilityAssessment, ApplicabilityLevel, Composition, Element,
    PlanId, PlanningProvenance, PrecursorId, PrecursorSelection, RankingWeights, SCHEMA_VERSION,
    SynthesisPlan, SynthesisPlanningReport, TargetSummary, TemperatureRange, balance,
    conventional_solid_state_template, score_plan,
};

fn sample_report() -> SynthesisPlanningReport {
    let ba = Element::new("Ba").unwrap();
    let ti = Element::new("Ti").unwrap();
    let o = Element::new("O").unwrap();
    let c = Element::new("C").unwrap();

    let target = Composition::new([(ba, 1.0), (ti, 1.0), (o, 3.0)]).unwrap();
    let baco3 = Composition::new([(ba, 1.0), (c, 1.0), (o, 3.0)]).unwrap();
    let tio2 = Composition::new([(ti, 1.0), (o, 2.0)]).unwrap();
    let co2 = Composition::new([(c, 1.0), (o, 2.0)]).unwrap();

    // Real output of balance(), not hand-authored -- BaCO3 + TiO2 -> BaTiO3
    // + CO2, the same worked example used elsewhere in this crate.
    let reaction = balance(&[baco3, tio2], &[target.clone(), co2])
        .unwrap()
        .into_iter()
        .next()
        .expect("BaCO3 + TiO2 -> BaTiO3 + CO2 must balance");
    let accepted = AcceptedPrecursorSet {
        precursors: vec![
            PrecursorId("BaCO3".to_string()),
            PrecursorId("TiO2".to_string()),
        ],
        reaction: reaction.clone(),
    };
    let template = conventional_solid_state_template(&target, &accepted);
    let precursors: Vec<PrecursorSelection> = accepted
        .precursors
        .iter()
        .zip(&reaction.reactants)
        .map(|(id, species)| PrecursorSelection {
            precursor: id.clone(),
            formula_units: species.coefficient,
        })
        .collect();

    let applicability = ApplicabilityAssessment {
        level: ApplicabilityLevel::InDomain,
        rationale: vec!["bulk inorganic, formula-only target".to_string()],
    };
    let assessment = score_plan(
        &target,
        &applicability,
        Some(&reaction),
        &template.steps,
        &template.evidence,
        false,
        &RankingWeights::default(),
    );
    let mut warnings = template.warnings;
    warnings.extend(assessment.warnings);

    SynthesisPlanningReport {
        schema_version: SCHEMA_VERSION,
        target: TargetSummary {
            composition: target,
            structure_present: false,
            desired_phase: None,
        },
        applicability: applicability.clone(),
        plans: vec![SynthesisPlan {
            plan_id: PlanId("plan-0001".to_string()),
            route_family: template.route_family,
            precursors,
            balanced_reaction: Some(reaction),
            steps: template.steps,
            score: assessment.score,
            confidence: assessment.confidence,
            applicability: assessment.applicability,
            evidence: template.evidence,
            warnings,
            assumptions: assessment.assumptions,
            unresolved: assessment.unresolved,
            manual_review_required: assessment.manual_review_required,
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

#[test]
fn duplicate_json_key_is_rejected_rather_than_silently_merged() {
    // A naive `BTreeMap<Element, f64>::deserialize` would silently keep the
    // last "Ba" value here; Composition's manual visitor must not.
    let bad = r#"{"Ba": 1.0, "Ba": 2.0, "Ti": 1.0, "O": 3.0}"#;
    let result: Result<Composition, _> = serde_json::from_str(bad);
    assert!(result.is_err());
}
