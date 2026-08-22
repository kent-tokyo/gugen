//! v0.4.0 Integration: real-corpus coverage and non-interference
//! measurement for `LiteratureEvidenceProvider`/`LiteratureObservationCorpusProvider`.
//!
//! **Sampling method, stated honestly**: targets are drawn from
//! `LiteratureObservationCorpus::cross_doi_comparisons()`'s own output --
//! i.e. routes *already known* to have 2+ independent DOIs of evidence.
//! This is not a claim that this sample represents a typical planning
//! workload; it deliberately maximizes overlap with the corpus so the
//! Agreement/Conflict/shape-diversity counts below are measured against
//! real matches, not diluted by targets the corpus has nothing to say
//! about. The "1 DOI only" / "2+ independent DOI" route counts, by
//! contrast, are computed directly from the corpus (not sampled), since
//! `cross_doi_comparisons()` structurally only emits 2+-DOI routes and
//! cannot itself distinguish "1 DOI" from "0 DOI" -- see
//! `literature_observation_conflicts.rs`'s Phase 20C pre-commit review.
//!
//! Same local-only convention as the other `literature_observation_*`
//! examples: the full-scale snapshot is gitignored, not part of the
//! published crate. Regenerate it locally first:
//!
//!   python3 benchmarks/build_literature_observation_snapshot.py
//!   cargo run --release --example literature_evidence_integration_report --features literature_corpus

use gugen::{
    Composition, CrossDoiFieldStatus, InMemoryPrecursorCatalog, LiteratureEvidenceProvider,
    LiteratureObservationCorpus, LiteratureObservationCorpusProvider, LoadMode, Planner,
    PlanningConfig, PlanningConstraints, PrecursorCandidate, PrecursorId, ProviderError,
    RouteFamily, TargetSpecification,
};
use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
use std::time::Instant;

const SNAPSHOT_PATH: &str = "benchmarks/data/literature_observation_snapshot.json";
const SAMPLE_SIZE: usize = 200;

/// Example-local only: lets one already-built `LiteratureObservationCorpusProvider`
/// (expensive to construct -- it runs `cross_doi_comparisons()` once) be
/// shared across many `Planner` instances via a cheap `Rc::clone`, instead
/// of rebuilding the index per target. Not a library type -- adding a
/// blanket `Rc<T>` impl to the real trait for one benchmark script's
/// convenience isn't justified.
struct SharedProvider(Rc<LiteratureObservationCorpusProvider>);
impl LiteratureEvidenceProvider for SharedProvider {
    fn route_evidence(
        &self,
        target: &Composition,
        route_family: RouteFamily,
        precursors: &[Composition],
    ) -> std::result::Result<Option<gugen::LiteratureRouteEvidence>, ProviderError> {
        self.0.route_evidence(target, route_family, precursors)
    }
}

#[derive(Default)]
struct FieldTally {
    agreement: usize,
    conflict: usize,
    insufficient: usize,
    unresolved: usize,
}
impl FieldTally {
    fn record<T>(&mut self, status: &CrossDoiFieldStatus<T>) {
        match status {
            CrossDoiFieldStatus::Agreement { .. } => self.agreement += 1,
            CrossDoiFieldStatus::Conflict { .. } => self.conflict += 1,
            CrossDoiFieldStatus::InsufficientIndependentSources => self.insufficient += 1,
            CrossDoiFieldStatus::Unresolved | CrossDoiFieldStatus::SegmentationAmbiguous => {
                self.unresolved += 1
            }
        }
    }
    fn print(&self, name: &str) {
        println!(
            "    {name}: agreement={} conflict={} insufficient_independent_sources={} unresolved={}",
            self.agreement, self.conflict, self.insufficient, self.unresolved
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
    let (corpus, _report) =
        LiteratureObservationCorpus::load(&json, LoadMode::Lenient).expect("load snapshot");

    // --- 1-DOI-only vs 2+-independent-DOI route counts (direct corpus
    // computation, not via the provider -- see module doc comment). ---
    let mut route_dois: BTreeMap<
        (Composition, Vec<Composition>, RouteFamily),
        std::collections::BTreeSet<String>,
    > = BTreeMap::new();
    for obs in corpus.observations() {
        let Some(doi) = &obs.doi else { continue };
        let mut precursors: Vec<Composition> = obs.precursors.iter().cloned().collect();
        precursors.sort();
        let key = (obs.target.clone(), precursors, obs.route_family);
        route_dois.entry(key).or_default().insert(doi.clone());
    }
    let one_doi_routes = route_dois.values().filter(|d| d.len() == 1).count();
    let multi_doi_routes = route_dois.values().filter(|d| d.len() >= 2).count();

    println!("=== v0.4.0 Integration: coverage and non-interference (real, local run) ===");
    println!("corpus.len(): {}", corpus.len());
    // Coarse per-route DOI-replication counts: any 2+ distinct DOIs
    // reporting the *same route at all* (target+precursors+route_family),
    // regardless of whether they share a comparable operation shape.
    // Looser than -- and therefore larger than -- Phase 20C's own
    // "619 routes with a shape-and-position-matched comparable group"
    // figure; these are two different metrics, not a contradiction.
    println!("routes with exactly 1 DOI (any operation shape): {one_doi_routes}");
    println!(
        "routes with 2+ independent DOIs (any operation shape, not necessarily shape-matched): \
        {multi_doi_routes}"
    );

    // --- Build the shared provider, timing index construction. ---
    let index_build_start = Instant::now();
    let provider = Rc::new(LiteratureObservationCorpusProvider::new(&corpus));
    let index_build_elapsed = index_build_start.elapsed();
    println!(
        "\nprovider index construction (cross_doi_comparisons, once): {index_build_elapsed:?}"
    );

    // --- Sample targets: the first SAMPLE_SIZE routes cross_doi_comparisons
    // itself emits, deterministic order. ---
    let assessments = corpus.cross_doi_comparisons();
    let sample: Vec<_> = assessments.iter().take(SAMPLE_SIZE).collect();
    println!(
        "\nsampled {} of {} cross-DOI-comparable routes for planning",
        sample.len(),
        assessments.len()
    );

    // --- Pure corpus-lookup timing: call the provider directly, bypassing
    // Planner, for a clean "corpus lookup runtime" number. ---
    let lookup_start = Instant::now();
    for a in &sample {
        let precursors: Vec<Composition> = a.precursors.iter().cloned().collect();
        provider
            .route_evidence(&a.target, a.route_family, &precursors)
            .expect("stub never errors");
    }
    let lookup_elapsed = lookup_start.elapsed();
    println!(
        "corpus lookup (direct route_evidence calls): {lookup_elapsed:?} total ({:?}/query)",
        lookup_elapsed / (sample.len() as u32).max(1)
    );

    let config = PlanningConfig::default();

    let mut temperature = FieldTally::default();
    let mut duration = FieldTally::default();
    let mut atmosphere = FieldTally::default();
    let mut plans_with_evidence = 0usize;
    let mut plans_with_multiple_shapes = 0usize;
    let mut total_plans_compared = 0usize;
    let mut score_inversions = 0usize;
    let mut step_changes = 0usize;
    let mut baseline_planning_time = std::time::Duration::ZERO;
    let mut with_evidence_planning_time = std::time::Duration::ZERO;
    let mut baseline_json_bytes = 0usize;
    let mut with_evidence_json_bytes = 0usize;

    for a in &sample {
        let target_spec = TargetSpecification {
            composition: a.target.clone(),
            structure: None,
            desired_phase: None,
            constraints: PlanningConstraints::default(),
        };

        // Catalog scoped to this route's own precursors only -- a real
        // caller's catalog for one planning call looks like this, not like
        // the whole corpus's precursor vocabulary. An earlier version of
        // this benchmark shared one corpus-wide catalog across all 200
        // targets, which let every target's plan reject thousands of
        // irrelevant candidates (anything sharing an element, e.g. O) and
        // inflated the JSON size numbers below by ~1000x -- see the
        // pre-commit advisor review referenced in
        // docs/literature_evidence_integration.md.
        let candidates: Vec<PrecursorCandidate> = a
            .precursors
            .iter()
            .enumerate()
            .map(|(i, p)| PrecursorCandidate {
                id: PrecursorId(format!("precursor-{i}")),
                composition: p.clone(),
                availability: None,
            })
            .collect();
        let catalog_for = || InMemoryPrecursorCatalog::new(candidates.clone());
        let baseline_planner = Planner::builder(catalog_for(), config.clone()).build();
        let with_evidence_planner = Planner::builder(catalog_for(), config.clone())
            .literature_evidence_provider(SharedProvider(Rc::clone(&provider)))
            .build();

        let t0 = Instant::now();
        let baseline_report = baseline_planner
            .plan(&target_spec, "2026-08-15T00:00:00Z")
            .expect("planning must not fail for well-formed input");
        baseline_planning_time += t0.elapsed();
        baseline_json_bytes += serde_json::to_string(&baseline_report).unwrap().len();

        let t1 = Instant::now();
        let with_evidence_report = with_evidence_planner
            .plan(&target_spec, "2026-08-15T00:00:00Z")
            .expect("planning must not fail for well-formed input");
        with_evidence_planning_time += t1.elapsed();
        with_evidence_json_bytes += serde_json::to_string(&with_evidence_report).unwrap().len();

        // --- Non-interference: same plan count, same plan_id order
        // (ranking), identical score/confidence/steps per plan. ---
        assert_eq!(
            baseline_report.plans.len(),
            with_evidence_report.plans.len(),
            "plan count changed for target {:?}",
            a.target
        );
        for (before, after) in baseline_report
            .plans
            .iter()
            .zip(with_evidence_report.plans.iter())
        {
            total_plans_compared += 1;
            if before.plan_id != after.plan_id {
                score_inversions += 1; // ranking order itself moved
            }
            if before.score != after.score || before.confidence != after.confidence {
                score_inversions += 1;
            }
            if before.steps != after.steps {
                step_changes += 1;
            }
            if let Some(evidence) = &after.literature_evidence {
                plans_with_evidence += 1;
                if evidence.assessment.has_multiple_operation_shapes {
                    plans_with_multiple_shapes += 1;
                }
                for group in &evidence.assessment.step_groups {
                    temperature.record(&group.temperature);
                    duration.record(&group.duration);
                    atmosphere.record(&group.atmosphere);
                }
            }
        }
    }

    println!(
        "\n--- planning runtime ({} targets, both planners) ---",
        sample.len()
    );
    println!(
        "baseline (offline_minimal): {baseline_planning_time:?} total ({:?}/target)",
        baseline_planning_time / (sample.len() as u32).max(1)
    );
    println!(
        "with literature evidence provider: {with_evidence_planning_time:?} total ({:?}/target)",
        with_evidence_planning_time / (sample.len() as u32).max(1)
    );
    println!(
        "delta: {:?}",
        with_evidence_planning_time.saturating_sub(baseline_planning_time)
    );

    println!("\n--- report JSON size ---");
    println!(
        "baseline total: {baseline_json_bytes} bytes ({} targets)",
        sample.len()
    );
    println!("with evidence total: {with_evidence_json_bytes} bytes");
    let delta_bytes = with_evidence_json_bytes.saturating_sub(baseline_json_bytes);
    println!(
        "delta: +{delta_bytes} bytes ({:.1}%)",
        100.0 * (with_evidence_json_bytes as f64 - baseline_json_bytes as f64)
            / baseline_json_bytes.max(1) as f64
    );
    println!(
        "delta per evidence-carrying plan: +{} bytes ({} plans had literature_evidence attached) \
        -- the portable number; the whole-sample percentage above is diluted by plans with no \
        match at all",
        delta_bytes / plans_with_evidence.max(1),
        plans_with_evidence
    );

    println!("\n--- coverage ---");
    println!("total plans compared: {total_plans_compared}");
    println!("plans with literature_evidence attached: {plans_with_evidence}");
    println!("  of which has_multiple_operation_shapes: {plans_with_multiple_shapes}");
    println!("  per-field status across those plans' step groups:");
    temperature.print("temperature");
    duration.print("duration");
    atmosphere.print("atmosphere");

    println!("\n--- non-interference (must both be zero) ---");
    println!("score/confidence/ranking inversions: {score_inversions}");
    println!("ProcessStep changes: {step_changes}");
    assert_eq!(
        score_inversions, 0,
        "a configured LiteratureEvidenceProvider must never change score, confidence, or \
        ranking order -- this is a correctness bug, not a benchmark finding"
    );
    assert_eq!(
        step_changes, 0,
        "a configured LiteratureEvidenceProvider must never auto-fill ProcessStep fields -- \
        this is a correctness bug, not a benchmark finding"
    );
    println!(
        "\nconfirmed: 0 inversions, 0 ProcessStep changes across {total_plans_compared} plans."
    );
}
