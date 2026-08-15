//! Phase 20C corpus-wide statistics: how much of `LiteratureObservationCorpus`
//! actually has independent-DOI replication to compare at all, and what
//! `cross_doi_comparisons()` finds when it does. Mechanically computed from
//! data already in the corpus -- not a manual-review document like Phase
//! 20D's audit.
//!
//! Same local-only convention as `examples/literature_observation_benchmark.rs`:
//! the full-scale snapshot is gitignored, not part of the published crate.
//! Regenerate it locally first:
//!
//!   python3 benchmarks/build_literature_observation_snapshot.py
//!   cargo run --release --example literature_observation_cross_doi_report --features literature_corpus

use gugen::{CrossDoiFieldStatus, LiteratureObservationCorpus, LoadMode};
use std::path::Path;

const SNAPSHOT_PATH: &str = "benchmarks/data/literature_observation_snapshot.json";

#[derive(Default)]
struct FieldTally {
    agreement: usize,
    conflict: usize,
    insufficient: usize,
    unresolved: usize,
    segmentation_ambiguous: usize,
}

impl FieldTally {
    fn record<T>(&mut self, status: &CrossDoiFieldStatus<T>) {
        match status {
            CrossDoiFieldStatus::Agreement { .. } => self.agreement += 1,
            CrossDoiFieldStatus::Conflict { .. } => self.conflict += 1,
            CrossDoiFieldStatus::InsufficientIndependentSources => self.insufficient += 1,
            CrossDoiFieldStatus::Unresolved => self.unresolved += 1,
            CrossDoiFieldStatus::SegmentationAmbiguous => self.segmentation_ambiguous += 1,
        }
    }

    fn print(&self, name: &str, total: usize) {
        println!(
            "  {name}: agreement={} conflict={} insufficient_independent_sources={} \
             unresolved={} segmentation_ambiguous={} (of {total} step groups)",
            self.agreement,
            self.conflict,
            self.insufficient,
            self.unresolved,
            self.segmentation_ambiguous
        );
    }
}

fn main() {
    if !Path::new(SNAPSHOT_PATH).exists() {
        eprintln!(
            "{SNAPSHOT_PATH} not found -- run:\n  \
             python3 benchmarks/build_literature_observation_snapshot.py\nfirst."
        );
        std::process::exit(1);
    }
    let json = std::fs::read_to_string(SNAPSHOT_PATH).expect("read snapshot file");
    let (corpus, report) =
        LiteratureObservationCorpus::load(&json, LoadMode::Lenient).expect("load snapshot");

    let missing_doi = corpus
        .observations()
        .iter()
        .filter(|o| o.doi.is_none())
        .count();

    println!("=== Phase 20C cross-DOI comparison (real, local run) ===");
    println!("snapshot file: {SNAPSHOT_PATH}");
    println!("corpus.len(): {}", corpus.len());
    println!(
        "observations with doi: None (excluded from all comparison): {missing_doi} ({:.1}%)",
        100.0 * missing_doi as f64 / corpus.len().max(1) as f64
    );
    assert!(
        report.rejected.is_empty(),
        "unexpected rejected entries in a real snapshot"
    );

    let assessments = corpus.cross_doi_comparisons();

    println!();
    println!(
        "routes with >=2 independent DOIs comparable somewhere: {}",
        assessments.len()
    );
    let multi_shape = assessments
        .iter()
        .filter(|a| a.has_multiple_operation_shapes)
        .count();
    println!(
        "  of which flagged has_multiple_operation_shapes: {multi_shape} ({:.1}%)",
        100.0 * multi_shape as f64 / assessments.len().max(1) as f64
    );

    let total_step_groups: usize = assessments.iter().map(|a| a.step_groups.len()).sum();
    println!("total step groups across all routes: {total_step_groups}");

    let mut temperature = FieldTally::default();
    let mut duration = FieldTally::default();
    let mut atmosphere = FieldTally::default();
    for route in &assessments {
        for group in &route.step_groups {
            temperature.record(&group.temperature);
            duration.record(&group.duration);
            atmosphere.record(&group.atmosphere);
        }
    }
    println!();
    println!("--- per-field status across all step groups ---");
    temperature.print("temperature", total_step_groups);
    duration.print("duration", total_step_groups);
    atmosphere.print("atmosphere", total_step_groups);

    // Named honestly per the pre-implementation advisor review: only 6
    // raw atmosphere strings map to a structured Atmosphere variant, so
    // this could turn out to be a near-empty signal -- worth stating
    // plainly, not left implicit.
    let atmosphere_verdicts = atmosphere.agreement + atmosphere.conflict;
    println!(
        "  atmosphere step groups reaching a real Agreement/Conflict verdict (i.e. not \
         excluded by the Controlled-free-text rule and not InsufficientIndependentSources/\
         Unresolved): {atmosphere_verdicts} of {total_step_groups} ({:.1}%)",
        100.0 * atmosphere_verdicts as f64 / total_step_groups.max(1) as f64
    );
}
