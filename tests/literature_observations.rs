//! Phase 20B: `LiteratureObservationCorpus` loader/query correctness.
//! `tests/fixtures/literature_observation_snapshot_sample.json` is a real
//! 11-entry slice of `benchmarks/build_literature_observation_snapshot.py`'s
//! own output against the live Kononova corpus (not hand-invented data),
//! deliberately including one exact-duplicate entry (corpus_record_index
//! 0, operation_index 0, appended twice) to exercise dedup against a real
//! artifact class, not just a synthetic one.

use gugen::{
    Atmosphere, Composition, CorpusHeatingObservation, Element, InertGas,
    LiteratureObservationCorpus, LoadMode, RouteFamily,
};

const FIXTURE: &str = include_str!("fixtures/literature_observation_snapshot_sample.json");

fn element(symbol: &str) -> Element {
    Element::new(symbol).unwrap()
}

fn composition(pairs: &[(&str, f64)]) -> Composition {
    Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
}

#[test]
fn small_fixture_loads_and_collapses_its_one_deliberate_duplicate() {
    let (corpus, report) = LiteratureObservationCorpus::load(FIXTURE, LoadMode::Lenient).unwrap();
    assert_eq!(report.rejected, vec![]);
    assert_eq!(
        report.duplicates_collapsed, 1,
        "the fixture's one deliberate duplicate must collapse"
    );
    assert_eq!(
        report.accepted, 11,
        "accepted is a pre-dedup count of successfully parsed entries"
    );
    assert_eq!(corpus.len(), 10);
    assert!(!corpus.is_empty());
    assert_eq!(
        corpus.manifest().schema_version,
        gugen::CORPUS_SNAPSHOT_SCHEMA_VERSION
    );
}

#[test]
fn find_exact_matches_target_and_precursor_set_order_independently() {
    let (corpus, _) = LiteratureObservationCorpus::load(FIXTURE, LoadMode::Lenient).unwrap();
    // corpus_record_index 0: target Sr1Fe12O19, precursors Fe2O3 + SrCO3.
    let target = composition(&[("Sr", 1.0), ("Fe", 12.0), ("O", 19.0)]);
    let fe2o3 = composition(&[("Fe", 2.0), ("O", 3.0)]);
    let srco3 = composition(&[("Sr", 1.0), ("C", 1.0), ("O", 3.0)]);

    let forward = corpus.find_exact(
        RouteFamily::ConventionalSolidState,
        &target,
        &[fe2o3.clone(), srco3.clone()],
    );
    let reversed = corpus.find_exact(
        RouteFamily::ConventionalSolidState,
        &target,
        &[srco3, fe2o3],
    );

    assert_eq!(forward.len(), reversed.len());
    assert_eq!(
        forward, reversed,
        "precursor order must not affect the match"
    );
}

#[test]
fn find_exact_returns_both_independent_operations_from_a_multi_step_record() {
    let (corpus, _) = LiteratureObservationCorpus::load(FIXTURE, LoadMode::Lenient).unwrap();
    let target = composition(&[("Sr", 1.0), ("Fe", 12.0), ("O", 19.0)]);
    let fe2o3 = composition(&[("Fe", 2.0), ("O", 3.0)]);
    let srco3 = composition(&[("Sr", 1.0), ("C", 1.0), ("O", 3.0)]);

    let matches = corpus.find_exact(
        RouteFamily::ConventionalSolidState,
        &target,
        &[fe2o3, srco3],
    );
    assert_eq!(
        matches.len(),
        2,
        "both HeatingOperations for this record must be returned, not merged"
    );
    let mut operation_indices: Vec<usize> = matches.iter().map(|o| o.operation_index).collect();
    operation_indices.sort_unstable();
    assert_eq!(operation_indices, vec![0, 1]);
    for m in &matches {
        assert_eq!(m.heating_purpose, None);
    }
}

#[test]
fn find_exact_is_explicitly_inapplicable_for_mechanochemical() {
    let (corpus, _) = LiteratureObservationCorpus::load(FIXTURE, LoadMode::Lenient).unwrap();
    let target = composition(&[("Sr", 1.0), ("Fe", 12.0), ("O", 19.0)]);
    let fe2o3 = composition(&[("Fe", 2.0), ("O", 3.0)]);
    let srco3 = composition(&[("Sr", 1.0), ("C", 1.0), ("O", 3.0)]);

    // The same query that returns 2 matches under ConventionalSolidState
    // must return exactly zero under Mechanochemical -- this corpus has no
    // evidence for that route family at all.
    let matches = corpus.find_exact(RouteFamily::Mechanochemical, &target, &[fe2o3, srco3]);
    assert_eq!(matches, Vec::<&CorpusHeatingObservation>::new());
}

#[test]
fn find_exact_does_not_match_a_different_formula_unit_scale() {
    let (corpus, _) = LiteratureObservationCorpus::load(FIXTURE, LoadMode::Lenient).unwrap();
    // Same ratio as the real Sr1Fe12O19 target, scaled x2 -- exact-scale
    // Composition equality must not match it (this crate's existing,
    // documented "no ratio normalization" convention, same as
    // `literature_conditions.rs`'s InMemoryLiteratureConditionProvider).
    let scaled_target = composition(&[("Sr", 2.0), ("Fe", 24.0), ("O", 38.0)]);
    let fe2o3 = composition(&[("Fe", 2.0), ("O", 3.0)]);
    let srco3 = composition(&[("Sr", 1.0), ("C", 1.0), ("O", 3.0)]);

    let matches = corpus.find_exact(
        RouteFamily::ConventionalSolidState,
        &scaled_target,
        &[fe2o3, srco3],
    );
    assert!(matches.is_empty());
}

#[test]
fn unrecognized_atmosphere_strings_are_preserved_verbatim_not_dropped_or_guessed() {
    let (corpus, _) = LiteratureObservationCorpus::load(FIXTURE, LoadMode::Lenient).unwrap();
    let has_ambient_preserved = corpus.observations().iter().any(|o| {
        matches!(
            &o.atmosphere,
            Some(Atmosphere::Controlled { description }) if description == "ambient"
        )
    });
    assert!(
        has_ambient_preserved,
        "an unmapped atmosphere string must survive as Controlled{{description}}, not be dropped"
    );

    let has_multi_string_preserved = corpus.observations().iter().any(|o| {
        matches!(
            &o.atmosphere,
            Some(Atmosphere::Controlled { description }) if description == "argon, hydrogen"
        )
    });
    assert!(
        has_multi_string_preserved,
        "multiple reported atmosphere strings must be preserved together, not collapsed to one or dropped"
    );

    let has_known_inert = corpus.observations().iter().any(|o| {
        matches!(
            &o.atmosphere,
            Some(Atmosphere::Inert {
                gas: InertGas::Nitrogen
            })
        )
    });
    assert!(
        has_known_inert,
        "a recognized single atmosphere string must map to its structured variant"
    );
}

#[test]
fn load_rejects_a_schema_version_mismatch() {
    let bad = FIXTURE.replace(
        "\"gugen-literature-observation-snapshot-v1\"",
        "\"some-other-schema-v0\"",
    );
    let result = LiteratureObservationCorpus::load(&bad, LoadMode::Lenient);
    assert!(
        result.is_err(),
        "a schema_version mismatch must be rejected unconditionally"
    );
}

#[test]
fn load_rejects_a_record_count_mismatch_even_in_lenient_mode() {
    let bad = FIXTURE.replacen("\"record_count\": 11", "\"record_count\": 999", 1);
    assert_ne!(
        bad, FIXTURE,
        "the replace must have actually matched something"
    );
    let result = LiteratureObservationCorpus::load(&bad, LoadMode::Lenient);
    assert!(
        result.is_err(),
        "a record_count mismatch must be rejected unconditionally, even in Lenient mode"
    );
}

fn minimal_manifest_snapshot(observations_json: &str) -> String {
    format!(
        r#"{{"manifest":{{"source":"test","release":"test","schema_version":"gugen-literature-observation-snapshot-v1","checksum":"deadbeef","record_count":{count}}},"observations":[{observations_json}]}}"#,
        count = if observations_json.is_empty() {
            0
        } else {
            observations_json.matches("\"target\"").count()
        },
    )
}

fn one_good_observation_json() -> &'static str {
    r#"{"target":{"Ba":1.0,"Ti":1.0,"O":3.0},"precursors":[{"Ba":1.0,"C":1.0,"O":3.0},{"Ti":1.0,"O":2.0}],"route_family":"ConventionalSolidState","operation_index":0,"temperature":{"min_celsius":900.0,"max_celsius":900.0},"duration":null,"atmosphere":"Air","doi":"10.1000/test","corpus_record_index":0}"#
}

#[test]
fn load_strict_mode_fails_the_whole_load_on_one_malformed_observation() {
    let malformed = r#"{"target":{"Ba":1.0,"Ti":1.0,"O":3.0},"precursors":[],"route_family":"Mechanochemical","operation_index":0,"temperature":null,"duration":null,"atmosphere":null,"doi":null,"corpus_record_index":1}"#;
    let json = minimal_manifest_snapshot(&format!("{},{}", one_good_observation_json(), malformed));

    let strict = LiteratureObservationCorpus::load(&json, LoadMode::Strict);
    assert!(
        strict.is_err(),
        "Strict mode must fail on the first malformed observation"
    );
}

#[test]
fn load_lenient_mode_skips_and_reports_a_malformed_observation() {
    let malformed = r#"{"target":{"Ba":1.0,"Ti":1.0,"O":3.0},"precursors":[],"route_family":"Mechanochemical","operation_index":0,"temperature":null,"duration":null,"atmosphere":null,"doi":null,"corpus_record_index":1}"#;
    let json = minimal_manifest_snapshot(&format!("{},{}", one_good_observation_json(), malformed));

    let (corpus, report) = LiteratureObservationCorpus::load(&json, LoadMode::Lenient).unwrap();
    assert_eq!(corpus.len(), 1, "only the well-formed observation is kept");
    assert_eq!(report.accepted, 1);
    assert_eq!(report.duplicates_collapsed, 0);
    assert_eq!(report.rejected.len(), 1);
    assert_eq!(report.rejected[0].position, 1);
    assert_eq!(
        report.accepted + report.duplicates_collapsed + report.rejected.len(),
        2,
        "accepted + duplicates_collapsed + rejected.len() must equal the input observation count"
    );
}

#[test]
fn load_rejects_an_observation_that_tries_to_set_heating_purpose() {
    let bad = r#"{"target":{"Ba":1.0,"Ti":1.0,"O":3.0},"precursors":[],"route_family":"ConventionalSolidState","heating_purpose":"Calcination","operation_index":0,"temperature":null,"duration":null,"atmosphere":null,"doi":null,"corpus_record_index":1}"#;
    let json = minimal_manifest_snapshot(bad);

    let strict = LiteratureObservationCorpus::load(&json, LoadMode::Strict);
    assert!(
        strict.is_err(),
        "an observation setting heating_purpose must be rejected, not silently ignored"
    );
}

#[test]
fn load_rejects_a_non_finite_temperature_value() {
    // 1e400 overflows f64 to +infinity when parsed -- valid JSON syntax,
    // an invalid value TemperatureRange::new must reject.
    let bad = r#"{"target":{"Ba":1.0,"Ti":1.0,"O":3.0},"precursors":[],"route_family":"ConventionalSolidState","operation_index":0,"temperature":{"min_celsius":1e400,"max_celsius":1e400},"duration":null,"atmosphere":null,"doi":null,"corpus_record_index":1}"#;
    let json = minimal_manifest_snapshot(bad);

    let strict = LiteratureObservationCorpus::load(&json, LoadMode::Strict);
    assert!(
        strict.is_err(),
        "a non-finite temperature must be rejected, not silently accepted as Some(inf)"
    );
}

#[test]
fn load_rejects_an_inverted_temperature_range() {
    let bad = r#"{"target":{"Ba":1.0,"Ti":1.0,"O":3.0},"precursors":[],"route_family":"ConventionalSolidState","operation_index":0,"temperature":{"min_celsius":900.0,"max_celsius":300.0},"duration":null,"atmosphere":null,"doi":null,"corpus_record_index":1}"#;
    let json = minimal_manifest_snapshot(bad);

    let strict = LiteratureObservationCorpus::load(&json, LoadMode::Strict);
    assert!(strict.is_err(), "min > max must be rejected");
}

#[test]
fn exact_duplicates_collapse_deterministically_to_the_lowest_corpus_record_index_regardless_of_input_order()
 {
    // Non-empty precursors, deliberately: a realistic snapshot never has
    // an empty precursor set (the build script's structural-validity gate
    // requires 1-4), so a dedup test using `precursors: []` would prove
    // dedup collapses correctly without ever proving `find_exact` can
    // still locate the survivor through a real, non-trivial precursor-set
    // match.
    let a = r#"{"target":{"Ba":1.0,"Ti":1.0,"O":3.0},"precursors":[{"Ba":1.0,"C":1.0,"O":3.0},{"Ti":1.0,"O":2.0}],"route_family":"ConventionalSolidState","operation_index":0,"temperature":{"min_celsius":900.0,"max_celsius":900.0},"duration":null,"atmosphere":null,"doi":"10.1000/test","corpus_record_index":5}"#;
    let b = r#"{"target":{"Ba":1.0,"Ti":1.0,"O":3.0},"precursors":[{"Ba":1.0,"C":1.0,"O":3.0},{"Ti":1.0,"O":2.0}],"route_family":"ConventionalSolidState","operation_index":0,"temperature":{"min_celsius":900.0,"max_celsius":900.0},"duration":null,"atmosphere":null,"doi":"10.1000/test","corpus_record_index":2}"#;

    let forward = minimal_manifest_snapshot(&format!("{a},{b}"));
    let backward = minimal_manifest_snapshot(&format!("{b},{a}"));

    let (corpus_fwd, report_fwd) =
        LiteratureObservationCorpus::load(&forward, LoadMode::Lenient).unwrap();
    let (corpus_bwd, report_bwd) =
        LiteratureObservationCorpus::load(&backward, LoadMode::Lenient).unwrap();

    assert_eq!(
        report_fwd.accepted, 2,
        "both entries parse; accepted is a pre-dedup count"
    );
    assert_eq!(report_bwd.accepted, 2);
    assert_eq!(report_fwd.duplicates_collapsed, 1);
    assert_eq!(report_bwd.duplicates_collapsed, 1);
    assert_eq!(corpus_fwd.len(), 1);
    assert_eq!(corpus_bwd.len(), 1);

    let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
    let baco3 = composition(&[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]);
    let tio2 = composition(&[("Ti", 1.0), ("O", 2.0)]);
    let survivor_fwd = corpus_fwd.find_exact(
        RouteFamily::ConventionalSolidState,
        &target,
        &[baco3.clone(), tio2.clone()],
    );
    let survivor_bwd =
        corpus_bwd.find_exact(RouteFamily::ConventionalSolidState, &target, &[tio2, baco3]);
    assert_eq!(
        survivor_fwd.len(),
        1,
        "the real precursor set must still locate the deduped survivor"
    );
    assert_eq!(survivor_bwd.len(), 1);
    assert_eq!(
        survivor_fwd[0].corpus_record_index, 2,
        "the lower corpus_record_index must survive"
    );
    assert_eq!(
        survivor_bwd[0].corpus_record_index, 2,
        "input order must not change which one survives"
    );
}

#[test]
fn different_dois_reporting_identical_conditions_are_both_kept_not_collapsed() {
    let a = r#"{"target":{"Ba":1.0,"Ti":1.0,"O":3.0},"precursors":[],"route_family":"ConventionalSolidState","operation_index":0,"temperature":{"min_celsius":900.0,"max_celsius":900.0},"duration":null,"atmosphere":null,"doi":"10.1000/paper-a","corpus_record_index":0}"#;
    let b = r#"{"target":{"Ba":1.0,"Ti":1.0,"O":3.0},"precursors":[],"route_family":"ConventionalSolidState","operation_index":0,"temperature":{"min_celsius":900.0,"max_celsius":900.0},"duration":null,"atmosphere":null,"doi":"10.1000/paper-b","corpus_record_index":1}"#;
    let json = minimal_manifest_snapshot(&format!("{a},{b}"));

    let (corpus, report) = LiteratureObservationCorpus::load(&json, LoadMode::Lenient).unwrap();
    assert_eq!(
        report.duplicates_collapsed, 0,
        "different DOIs are independent evidence, never collapsed"
    );
    assert_eq!(corpus.len(), 2);
}

#[test]
fn reordering_the_snapshot_file_produces_byte_identical_reserialization() {
    let a = r#"{"target":{"Ba":1.0,"Ti":1.0,"O":3.0},"precursors":[{"Ti":1.0,"O":2.0}],"route_family":"ConventionalSolidState","operation_index":0,"temperature":{"min_celsius":900.0,"max_celsius":900.0},"duration":null,"atmosphere":null,"doi":"10.1000/a","corpus_record_index":9}"#;
    let b = r#"{"target":{"Fe":2.0,"O":3.0},"precursors":[],"route_family":"ConventionalSolidState","operation_index":0,"temperature":null,"duration":{"min_hours":2.0,"max_hours":2.0},"atmosphere":null,"doi":"10.1000/b","corpus_record_index":3}"#;
    let c = r#"{"target":{"Sr":1.0,"O":1.0},"precursors":[],"route_family":"ConventionalSolidState","operation_index":0,"temperature":null,"duration":null,"atmosphere":null,"doi":"10.1000/c","corpus_record_index":1}"#;

    let forward = minimal_manifest_snapshot(&format!("{a},{b},{c}"));
    let shuffled = minimal_manifest_snapshot(&format!("{c},{a},{b}"));

    let (corpus_a, _) = LiteratureObservationCorpus::load(&forward, LoadMode::Lenient).unwrap();
    let (corpus_b, _) = LiteratureObservationCorpus::load(&shuffled, LoadMode::Lenient).unwrap();

    let json_a = serde_json::to_string(corpus_a.observations()).unwrap();
    let json_b = serde_json::to_string(corpus_b.observations()).unwrap();
    assert_eq!(
        json_a, json_b,
        "row order in the input file must not affect the loaded corpus's own order"
    );
}
