//! Worked example: plan a target, then log a `SynthesisExecutionRecord`
//! for a real attempt of that plan -- appending to, then reading back
//! from, a local JSONL file. `parse_execution_records` itself never opens
//! a file (no library function in this crate does); the appending and
//! reading here are the caller's own responsibility, exactly as the
//! module doc comment on `execution_record` states. Run with:
//!
//! ```text
//! cargo run --example execution_record_demo --features serde
//! ```

use gugen::{
    ActualPrecursorAmount, ActualProcessStep, ActualStepDetail, Composition, Deviation,
    DeviationCategory, EXECUTION_RECORD_SCHEMA_VERSION, Element, ExecutionCharacterization,
    ExecutionProvenance, ExecutionRecordLoadMode, InMemoryExecutionRecordProvider,
    InMemoryPrecursorCatalog, PlanIdentity, Planner, PlanningConfig, PlanningConstraints,
    PrecursorCandidate, PrecursorId, SynthesisExecutionRecord, SynthesisOutcome,
    TargetSpecification, parse_execution_records,
};
use std::io::Write;

fn element(symbol: &str) -> Element {
    Element::new(symbol).unwrap()
}

fn composition(pairs: &[(&str, f64)]) -> Composition {
    Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
}

fn main() {
    // Stage 1: chemical planning, entirely unaware execution records exist.
    let planner = Planner::builder(
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
    )
    .build();
    let target_composition = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
    let target = TargetSpecification {
        composition: target_composition.clone(),
        structure: None,
        desired_phase: None,
        constraints: PlanningConstraints::default(),
    };
    let report = planner.plan(&target, "2026-08-23T00:00:00Z").unwrap();
    let plan = report
        .plans
        .first()
        .expect("BaCO3 + TiO2 -> BaTiO3 must produce a plan");
    println!("Chemical plan {} was proposed.", plan.plan_id);

    // Stage 2: two real (fictional, for this demo) lab attempts of that
    // plan, each recorded as a SynthesisExecutionRecord -- structurally
    // separate from Planner/score_plan; nothing here feeds back into
    // plan.score. Different outcomes on purpose, to demonstrate that
    // Phase 26 surfaces every match, never collapsing them into one
    // "best" result.
    let make_record =
        |outcome: SynthesisOutcome, notes: &str, batch_id: &str| SynthesisExecutionRecord {
            schema_version: EXECUTION_RECORD_SCHEMA_VERSION.to_string(),
            plan_identity: PlanIdentity::from_plan(target_composition.clone(), plan),
            commercial_catalog_source: None,
            selected_commercial_offers: Vec::new(),
            actual_precursor_amounts: vec![
                ActualPrecursorAmount {
                    precursor: PrecursorId("BaCO3".to_string()),
                    mass_grams: Some(197.5),
                    formula_units: Some(1),
                },
                ActualPrecursorAmount {
                    precursor: PrecursorId("TiO2".to_string()),
                    mass_grams: Some(80.2),
                    formula_units: Some(1),
                },
            ],
            actual_process_conditions: vec![ActualProcessStep {
                planned_step_index: None,
                step: ActualStepDetail::Heat {
                    purpose: gugen::HeatingPurpose::Sintering,
                    temperature_celsius: Some(1180.0),
                    duration_hours: Some(4.0),
                    atmosphere: Some(gugen::Atmosphere::Air),
                    ramp_celsius_per_hour: None,
                },
            }],
            deviations_from_plan: vec![Deviation {
                category: DeviationCategory::TemperatureDeviation,
                description: "furnace ran 30C above the target setpoint".to_string(),
            }],
            outcome,
            characterization: ExecutionCharacterization {
                phase_purity_fraction: Some(0.95),
                yield_fraction: Some(0.88),
                xrd_reference: Some("XRD-2026-08-23-001".to_string()),
                measurement_method: Some("Rietveld refinement".to_string()),
            },
            operator_notes: Some(notes.to_string()),
            experiment_date: Some("2026-08-23".to_string()),
            batch_id: Some(batch_id.to_string()),
            provenance: ExecutionProvenance {
                gugen_version: env!("CARGO_PKG_VERSION").to_string(),
                recorded_by: Some("demo-operator".to_string()),
                recorded_at: "2026-08-23T12:00:00Z".to_string(),
            },
        };
    let record_a = make_record(
        SynthesisOutcome::TargetPhaseObtained,
        "slight discoloration on the crucible edge",
        "batch-042",
    );
    let record_b = make_record(
        SynthesisOutcome::CompetingPhaseObserved,
        "a second attempt, different batch -- competing phase observed instead",
        "batch-043",
    );

    // Appending is the caller's own 3-line responsibility -- the library
    // never opens a file itself.
    let path = std::env::temp_dir().join("gugen_execution_record_demo.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap();
    for record in [&record_a, &record_b] {
        writeln!(file, "{}", serde_json::to_string(record).unwrap()).unwrap();
    }
    println!("Appended 2 records to {}", path.display());

    let contents = std::fs::read_to_string(&path).unwrap();
    let (records, load_report) =
        parse_execution_records(&contents, ExecutionRecordLoadMode::Strict).unwrap();
    println!(
        "Read back {} record(s) ({} rejected) from {}.",
        load_report.accepted,
        load_report.rejected.len(),
        path.display()
    );

    // Stage 3: surface those records back during planning, as
    // reference-only evidence -- structurally separate from score_plan.
    let provider = InMemoryExecutionRecordProvider::new(records);
    let planner_with_history = Planner::builder(
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
    )
    .prior_experiment_evidence_provider(provider)
    .build();
    let report_with_history = planner_with_history
        .plan(&target, "2026-08-23T00:00:00Z")
        .unwrap();
    let plan_with_history = report_with_history
        .plans
        .iter()
        .find(|p| p.plan_id == plan.plan_id)
        .unwrap();
    match &plan_with_history.prior_experiment_evidence {
        Some(evidence) => {
            println!(
                "Prior experiment evidence for {}: {} record(s), outcome tally: {:?}",
                plan_with_history.plan_id,
                evidence.records.len(),
                evidence.outcome_tally()
            );
            println!(
                "score unchanged by this evidence: {}",
                plan_with_history.score == plan.score
            );
        }
        None => println!("No prior experiment evidence matched (unexpected for this demo)."),
    }

    std::fs::remove_file(&path).ok();
}
