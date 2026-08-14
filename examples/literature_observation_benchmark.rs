//! Phase 20B performance measurement: load time, lookup time, a rough
//! memory estimate, and serialized size, against a real full-scale
//! snapshot built from the live Kononova corpus.
//!
//! Unlike `examples/large_scale_benchmark.rs`, this does NOT `include_str!`
//! a committed corpus file -- the full-scale snapshot
//! (`benchmarks/data/literature_observation_snapshot.json`, ~14K
//! observations from Phase 20A's 9,045 structurally-valid records) is
//! gitignored and not part of the published crate (owner's explicit
//! requirement: no bulk data in the package). Regenerate it locally first:
//!
//!   python3 benchmarks/build_literature_observation_snapshot.py
//!   cargo run --release --example literature_observation_benchmark --features literature_corpus
//!
//! This is a manual, locally-run measurement, not a CI-gated test --
//! CI never has the full-scale file, only the small committed fixture
//! (`tests/fixtures/literature_observation_snapshot_sample.json`), which
//! is exercised by `tests/literature_observations.rs` instead.

use gugen::{Composition, LiteratureObservationCorpus, LoadMode, RouteFamily};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::Instant;

const SNAPSHOT_PATH: &str = "benchmarks/data/literature_observation_snapshot.json";

/// A rough lower-bound heap-byte estimate (target/precursor compositions,
/// doi/atmosphere-description strings) -- not a rigorous allocator-level
/// measurement (no profiling dependency is justified for one phase's
/// benchmark), but real enough to compare against the serialized size and
/// sanity-check that memory use scales roughly linearly with observation
/// count, not something worse.
fn estimated_heap_bytes(observations: &[gugen::CorpusHeatingObservation]) -> usize {
    fn composition_bytes(c: &Composition) -> usize {
        // Each element entry: an Element (a &'static str, no owned bytes)
        // plus a Frac (two i128s) inside a BTreeMap node -- estimated as
        // 48 bytes/entry (rounded up for BTreeMap node overhead).
        c.iter().count() * 48
    }
    observations
        .iter()
        .map(|o| {
            composition_bytes(&o.target)
                + o.precursors.iter().map(composition_bytes).sum::<usize>()
                + o.doi.as_ref().map_or(0, |s| s.capacity())
                + match &o.atmosphere {
                    Some(gugen::Atmosphere::Controlled { description }) => description.capacity(),
                    _ => 0,
                }
                + std::mem::size_of::<gugen::CorpusHeatingObservation>()
        })
        .sum()
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
    let file_size_bytes = json.len();

    let load_start = Instant::now();
    let (corpus, report) =
        LiteratureObservationCorpus::load(&json, LoadMode::Lenient).expect("load snapshot");
    let load_elapsed = load_start.elapsed();

    println!("=== Phase 20B performance (real, local run) ===");
    println!("snapshot file: {SNAPSHOT_PATH}");
    println!(
        "manifest.record_count (input observations): {}",
        corpus.manifest().record_count
    );
    println!("accepted: {}", report.accepted);
    println!("rejected: {}", report.rejected.len());
    println!("duplicates_collapsed: {}", report.duplicates_collapsed);
    println!("final corpus.len(): {}", corpus.len());
    println!();
    println!("--- load time ---");
    println!(
        "{load_elapsed:?} total ({:?}/observation)",
        load_elapsed / (corpus.len() as u32).max(1)
    );

    // Lookup time: query every distinct (target, precursor-set) pair
    // present in the corpus once, then report average per-query time.
    // This exercises the real linear scan `find_exact` performs today
    // (see its own doc comment -- no index is built), at real corpus
    // scale, not a synthetic one.
    let mut distinct_queries: BTreeSet<(Composition, Vec<Composition>)> = BTreeSet::new();
    for obs in corpus.observations() {
        let precursors: Vec<Composition> = obs.precursors.iter().cloned().collect();
        distinct_queries.insert((obs.target.clone(), precursors));
    }
    println!();
    println!(
        "--- lookup time ({} distinct target+precursor-set queries) ---",
        distinct_queries.len()
    );
    let lookup_start = Instant::now();
    let mut total_matches = 0usize;
    for (target, precursors) in &distinct_queries {
        let matches = corpus.find_exact(RouteFamily::ConventionalSolidState, target, precursors);
        total_matches += matches.len();
    }
    let lookup_elapsed = lookup_start.elapsed();
    println!(
        "{lookup_elapsed:?} total ({:?}/query, {total_matches} total observations returned)",
        lookup_elapsed / (distinct_queries.len() as u32).max(1)
    );

    println!();
    println!("--- memory (rough estimate, see this file's doc comment) ---");
    let heap_estimate = estimated_heap_bytes(corpus.observations());
    println!(
        "~{:.1} MB for {} observations (~{:.0} bytes/observation)",
        heap_estimate as f64 / 1_048_576.0,
        corpus.len(),
        heap_estimate as f64 / corpus.len().max(1) as f64
    );

    println!();
    println!("--- serialized size ---");
    println!(
        "input snapshot file: {:.1} MB ({file_size_bytes} bytes)",
        file_size_bytes as f64 / 1_048_576.0
    );
    let reserialized = serde_json::to_string(corpus.observations()).expect("reserialize");
    println!(
        "loaded+deduped, re-serialized observations only: {:.1} MB ({} bytes)",
        reserialized.len() as f64 / 1_048_576.0,
        reserialized.len()
    );
}
