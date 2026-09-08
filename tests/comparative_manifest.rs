#![cfg(feature = "serde")]

use gugen::{ComparativeEvaluationManifest, EvaluationSplit, MeasurementStatus};

#[test]
fn published_manifest_fixture_is_measurement_ready_protocol_only() {
    let manifest: ComparativeEvaluationManifest =
        serde_json::from_str(include_str!("fixtures/comparative_manifest_v1.json")).unwrap();

    manifest.validate_for_measurement().unwrap();
    let audit = manifest.protocol.leakage_audit();
    assert_eq!(audit.protocol_id, "phase39-local-v1");
    assert_eq!(audit.candidate_count, 2);
    assert_eq!(audit.cases_by_split[&EvaluationSplit::Development], 1);
    assert_eq!(audit.cases_by_split[&EvaluationSplit::Confirmation], 1);
    assert!(audit.leaked_groups.is_empty());
    assert_eq!(audit.measurement_status, MeasurementStatus::ProtocolOnly);
}

#[test]
fn manifest_round_trip_preserves_the_no_results_boundary() {
    let manifest: ComparativeEvaluationManifest =
        serde_json::from_str(include_str!("fixtures/comparative_manifest_v1.json")).unwrap();
    let encoded = serde_json::to_string(&manifest).unwrap();
    assert!(!encoded.contains("score"));
    assert!(!encoded.contains("result"));
    let decoded: ComparativeEvaluationManifest = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, manifest);
}
