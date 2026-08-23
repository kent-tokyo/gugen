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
    ExecutionProvenance, ExecutionRecordLoadMode, InMemoryPrecursorCatalog, PlanIdentity, Planner,
    PlanningConfig, PlanningConstraints, PrecursorCandidate, PrecursorId, SynthesisExecutionRecord,
    SynthesisOutcome, TargetSpecification, parse_execution_records,
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

    // Stage 2: a real (fictional, for this demo) lab attempt of that plan,
    // recorded as a SynthesisExecutionRecord -- structurally separate from
    // Planner/score_plan; nothing here feeds back into plan.score.
    let record = SynthesisExecutionRecord {
        schema_version: EXECUTION_RECORD_SCHEMA_VERSION.to_string(),
        plan_identity: PlanIdentity::from_plan(target_composition, plan),
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
        outcome: SynthesisOutcome::TargetPhaseObtained,
        characterization: ExecutionCharacterization {
            phase_purity_fraction: Some(0.95),
            yield_fraction: Some(0.88),
            xrd_reference: Some("XRD-2026-08-23-001".to_string()),
            measurement_method: Some("Rietveld refinement".to_string()),
        },
        operator_notes: Some("slight discoloration on the crucible edge".to_string()),
        experiment_date: Some("2026-08-23".to_string()),
        batch_id: Some("batch-042".to_string()),
        provenance: ExecutionProvenance {
            gugen_version: env!("CARGO_PKG_VERSION").to_string(),
            recorded_by: Some("demo-operator".to_string()),
            recorded_at: "2026-08-23T12:00:00Z".to_string(),
        },
    };

    // Appending is the caller's own 3-line responsibility -- the library
    // never opens a file itself.
    let path = std::env::temp_dir().join("gugen_execution_record_demo.jsonl");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(file, "{}", serde_json::to_string(&record).unwrap()).unwrap();
    println!("Appended one record to {}", path.display());

    let contents = std::fs::read_to_string(&path).unwrap();
    let (records, load_report) =
        parse_execution_records(&contents, ExecutionRecordLoadMode::Strict).unwrap();
    println!(
        "Read back {} record(s) ({} rejected) from {}.",
        load_report.accepted,
        load_report.rejected.len(),
        path.display()
    );
    let last = records.last().unwrap();
    println!(
        "Most recent: plan {}, outcome {:?}, {} deviation(s) from plan.",
        last.plan_identity.plan_id,
        last.outcome,
        last.deviations_from_plan.len()
    );

    std::fs::remove_file(&path).ok();
}
