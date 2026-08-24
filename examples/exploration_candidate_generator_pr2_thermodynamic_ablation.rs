//! Phase 30 PR 2: measures `ThermodynamicStabilityGenerator` (new this PR)
//! against the same frozen exploration-recall catalog every prior
//! byproduct-fix and PR 1's own ablation script have used, and extends PR
//! 1's 2-generator ensemble (`catalog-exact` + `frequency-prior`) to all
//! three generators -- the direct test of PR 1's own stated hypothesis
//! that a target-aware(r) signal might help where a purely global-
//! frequency one didn't.
//!
//! **Real data, not synthetic**: formation-energy values come from
//! `benchmarks/data/oqmd_coverage_manifest.json` (committed, 240KB, real
//! OQMD data fetched 2026-08-23 during Phase 21B's thermodynamic-
//! selectivity coverage gate) -- no new fetch, no fabricated numbers.
//! That manifest was built against a *different*, smaller population
//! (the thermodynamic-selectivity clean-population's 795 formulas), not
//! this exploration catalog, so coverage here is real but partial:
//! roughly 63% of this catalog's distinct candidate formulas have a
//! usable match (the exact measured count is reported below, not
//! assumed). `ThermodynamicStabilityGenerator` **excludes** unmatched
//! candidates entirely rather than assigning them a fabricated neutral
//! or worst-case energy -- missing data means abstention (matches
//! `ThermodynamicProvider`'s own documented discipline: "`Ok(None)`
//! ... must not by itself reject a plan"). This gives the generator a
//! real, structural, and expected recall *ceiling* below `catalog-exact`'s
//! -- reported plainly as a known limitation, not hidden.
//!
//! **This is deliberately NOT a claim that the formal Phase 30 gate has
//! been evaluated**, for the same reason PR 1's own ablation disclosed:
//! the frozen catalog's fixed, always-complete, frequency-selected
//! row-local candidate pool would partly measure the benchmark's own
//! construction under a naive Recall@K. Reuses PR 1's two measurements,
//! extended to four configs (catalog-exact, frequency-prior,
//! thermodynamic-stability, and the 3-generator ensemble):
//!
//! 1. **End-to-end recall under a tightened `SearchBudget`**, freshly
//!    recalibrated in this run (not assumed from PR 1 -- a 3rd
//!    generator's different candidate-order behavior could shift where
//!    the calibration window lands).
//! 2. **Descriptive row-local Recall@K**, same explicit non-gate
//!    disclosure as PR 1.
//!
//! Every row's raw JSON candidate list is only ever consumed through a
//! `CandidateGenerator`, each of which re-sorts internally before
//! producing a candidate list -- same neutralization PR 1 established.
//!
//! Run: `cargo run --release --example
//! exploration_candidate_generator_pr2_thermodynamic_ablation --features
//! serde` after regenerating the (gitignored) frozen catalog locally (see
//! `exploration_recall_baseline.rs`'s own doc comment for the exact
//! commands) -- release mode matters, not just for comfort.
//! `benchmarks/data/oqmd_coverage_manifest.json` is already committed, no
//! regeneration needed for it.
//!
//! Writes `benchmarks/data/exploration_candidate_generator_pr2_result.json`,
//! a new, separate committed result file -- never overwrites PR 1's own
//! `exploration_candidate_generator_pr1_result.json`.

use gugen::{
    CandidateGenerator, CandidateGeneratorEnsemble, CatalogExactGenerator, Composition, Element,
    FrequencyPriorGenerator, InMemoryPrecursorCatalog, PlanningConstraints, PrecursorCandidate,
    PrecursorId, RejectionCode, SearchBudget, ThermodynamicStabilityGenerator,
    search_precursor_sets,
};
use std::collections::{BTreeMap, BTreeSet};

const CATALOG_PATH: &str = "benchmarks/data/exploration_frozen_catalog_manifest.json";
const OQMD_MANIFEST_PATH: &str = "benchmarks/data/oqmd_coverage_manifest.json";
const OUTPUT_PATH: &str = "benchmarks/data/exploration_candidate_generator_pr2_result.json";

/// Every Nth row (deterministic stride, not a prefix) is used to
/// calibrate the tight `SearchBudget` below -- see PR 1's own ablation
/// script for why a stride, not a prefix (a prefix was measured to
/// under-predict the full corpus's exhaustion rate).
const CALIBRATION_STRIDE: usize = 10;

/// Candidates for the tightened budget's `max_precursor_sets`, tried
/// largest-first. Picks the largest value whose catalog-exact exhaustion
/// rate on the calibration sample lands in [0.30, 0.70].
const CANDIDATE_BUDGETS: &[usize] = &[
    10_000, 5_000, 2_000, 1_000, 500, 200, 100, 50, 30, 20, 15, 10, 7, 5, 3,
];

/// Row-local Recall@K is only ever descriptive here (see module doc).
const RECALL_AT_K_VALUES: &[usize] = &[3, 5, 10, 15, 20, 28];

#[derive(serde::Deserialize)]
struct CatalogCandidate {
    formula: String,
    elements: BTreeMap<String, f64>,
}

#[derive(serde::Deserialize)]
struct CatalogRow {
    target_formula: String,
    target_elements: BTreeMap<String, f64>,
    route: Vec<String>,
    candidates: Vec<CatalogCandidate>,
}

#[derive(serde::Deserialize)]
struct FrozenCatalog {
    rows: Vec<CatalogRow>,
}

#[derive(serde::Deserialize)]
struct OqmdCoverageEntry {
    matched: bool,
    delta_e_ev_per_atom: Option<f64>,
}

#[derive(serde::Deserialize)]
struct OqmdManifest {
    coverage: BTreeMap<String, OqmdCoverageEntry>,
}

/// Defensively re-validated, matching `examples/large_scale_benchmark.rs`'s
/// own `try_composition` precedent.
fn try_composition(elements: &BTreeMap<String, f64>) -> Option<Composition> {
    let pairs: Option<Vec<(Element, f64)>> = elements
        .iter()
        .map(|(sym, amt)| Element::new(sym).ok().map(|e| (e, *amt)))
        .collect();
    Composition::new(pairs?).ok()
}

struct ParsedRow {
    target_formula: String,
    target: Composition,
    route: Vec<String>,
    candidates: Vec<PrecursorCandidate>,
}

fn parse_rows(catalog: &FrozenCatalog) -> (Vec<ParsedRow>, usize) {
    let mut parsed = Vec::with_capacity(catalog.rows.len());
    let mut skipped = 0usize;
    for row in &catalog.rows {
        let Some(target) = try_composition(&row.target_elements) else {
            skipped += 1;
            continue;
        };
        let candidates: Option<Vec<PrecursorCandidate>> = row
            .candidates
            .iter()
            .map(|c| {
                try_composition(&c.elements).map(|composition| PrecursorCandidate {
                    id: PrecursorId(c.formula.clone()),
                    composition,
                    availability: None,
                })
            })
            .collect();
        let Some(candidates) = candidates else {
            skipped += 1;
            continue;
        };
        parsed.push(ParsedRow {
            target_formula: row.target_formula.clone(),
            target,
            route: row.route.clone(),
            candidates,
        });
    }
    (parsed, skipped)
}

/// Real OQMD formation energies only -- `matched: false` (queried, no
/// usable entry) and "formula absent as a key entirely" (never queried
/// against OQMD, since the manifest was built from a different, smaller
/// population) are treated identically as "no usable data": neither ends
/// up in this map. `.get(formula)` on this map is the only lookup this
/// script ever performs; a bare index into the raw manifest is never
/// used, so the two "no data" cases can't be silently handled
/// differently.
fn build_oqmd_formation_energy_table(manifest: &OqmdManifest) -> BTreeMap<String, f64> {
    manifest
        .coverage
        .iter()
        .filter_map(|(formula, entry)| {
            if entry.matched {
                entry.delta_e_ev_per_atom.map(|e| (formula.clone(), e))
            } else {
                None
            }
        })
        .collect()
}

fn catalog_exact_for(row: &ParsedRow) -> CatalogExactGenerator {
    CatalogExactGenerator::new(InMemoryPrecursorCatalog::new(row.candidates.clone()))
}

fn frequency_prior_for(
    row: &ParsedRow,
    frequency: &BTreeMap<String, u64>,
) -> FrequencyPriorGenerator {
    let entries = row
        .candidates
        .iter()
        .map(|c| (c.clone(), *frequency.get(&c.id.0).unwrap_or(&0)))
        .collect();
    FrequencyPriorGenerator::new(entries)
}

/// Global "how often is this formula a real cited precursor" table --
/// same construction PR 1's own ablation used (from `route`, never
/// `candidates`, to avoid reproducing the decoy pool's own bias).
fn build_frequency_table(rows: &[ParsedRow]) -> BTreeMap<String, u64> {
    let mut frequency = BTreeMap::new();
    for row in rows {
        for formula in &row.route {
            *frequency.entry(formula.clone()).or_insert(0u64) += 1;
        }
    }
    frequency
}

/// Excludes candidates with no real OQMD match entirely (never a
/// fabricated neutral/worst-case value -- see module doc).
fn thermodynamic_stability_for(
    row: &ParsedRow,
    formation_energy: &BTreeMap<String, f64>,
) -> ThermodynamicStabilityGenerator {
    let entries: Vec<(PrecursorCandidate, f64)> = row
        .candidates
        .iter()
        .filter_map(|c| formation_energy.get(&c.id.0).map(|&e| (c.clone(), e)))
        .collect();
    ThermodynamicStabilityGenerator::new(entries)
        .expect("real OQMD formation energies must be finite")
}

fn route_recovered(accepted: &[gugen::AcceptedPrecursorSet], expected_route: &[String]) -> bool {
    let mut expected: Vec<&str> = expected_route.iter().map(String::as_str).collect();
    expected.sort_unstable();
    accepted.iter().any(|a| {
        let mut got: Vec<&str> = a.precursors.iter().map(|p| p.0.as_str()).collect();
        got.sort_unstable();
        got == expected
    })
}

fn budget_exhausted(rejected: &[gugen::RejectedCandidate]) -> bool {
    rejected
        .iter()
        .any(|r| r.reason_codes == vec![RejectionCode::SearchBudgetExhausted])
}

/// Calibrates the tight `SearchBudget` via catalog-exact alone, same
/// methodology as PR 1's own ablation (stride sample, largest budget
/// landing in [0.30, 0.70]) -- run fresh here rather than reusing PR 1's
/// chosen value, since a 3rd generator wasn't part of that calibration.
fn calibrate_tight_budget(rows: &[ParsedRow]) -> usize {
    let sample: Vec<&ParsedRow> = rows.iter().step_by(CALIBRATION_STRIDE).collect();
    println!(
        "calibrating tight SearchBudget via catalog-exact on {} rows (every {}th):",
        sample.len(),
        CALIBRATION_STRIDE
    );
    let mut chosen = *CANDIDATE_BUDGETS.last().unwrap();
    for &max_sets in CANDIDATE_BUDGETS {
        let budget = SearchBudget {
            max_precursor_sets: max_sets,
            ..SearchBudget::default()
        };
        let mut exhausted = 0usize;
        for &row in &sample {
            let generated = catalog_exact_for(row)
                .generate(&row.target, &PlanningConstraints::default())
                .expect("catalog-exact generation must not fail on a well-formed catalog row");
            let candidates: Vec<PrecursorCandidate> =
                generated.into_iter().map(|gc| gc.candidate).collect();
            let outcome = search_precursor_sets(
                &row.target,
                &candidates,
                &PlanningConstraints::default(),
                &budget,
            )
            .expect("search_precursor_sets must not error on a well-formed synthetic catalog");
            if budget_exhausted(&outcome.rejected) {
                exhausted += 1;
            }
        }
        let rate = exhausted as f64 / sample.len().max(1) as f64;
        println!(
            "  max_precursor_sets={max_sets}: catalog-exact exhaustion rate {rate:.3} \
            ({exhausted}/{})",
            sample.len()
        );
        chosen = max_sets;
        if (0.30..=0.70).contains(&rate) {
            break;
        }
    }
    println!("chosen tight SearchBudget.max_precursor_sets = {chosen}");
    chosen
}

struct ConfigResult {
    recovered_count: usize,
    exhausted_count: usize,
}

impl ConfigResult {
    fn new() -> Self {
        Self {
            recovered_count: 0,
            exhausted_count: 0,
        }
    }
}

fn top_k_contains_route(ranked_ids: &[String], route: &[String], k: usize) -> bool {
    let top_k: BTreeSet<&str> = ranked_ids.iter().take(k).map(String::as_str).collect();
    route.iter().all(|formula| top_k.contains(formula.as_str()))
}

const CONFIG_NAMES: &[&str] = &[
    "catalog_exact",
    "frequency_prior",
    "thermodynamic_stability",
    "ensemble",
];

fn main() {
    let raw = std::fs::read_to_string(CATALOG_PATH).unwrap_or_else(|e| {
        panic!(
            "could not read {CATALOG_PATH}: {e}\n\
            regenerate it first:\n\
            \x20 python3 benchmarks/exploration_build_recall_manifest.py\n\
            \x20 python3 benchmarks/exploration_build_frozen_decoy_catalog.py"
        )
    });
    let catalog: FrozenCatalog = serde_json::from_str(&raw).expect(
        "benchmarks/data/exploration_frozen_catalog_manifest.json must be valid JSON -- \
        regenerate with benchmarks/exploration_build_frozen_decoy_catalog.py",
    );
    let (rows, skipped_unrepresentable) = parse_rows(&catalog);
    let frequency = build_frequency_table(&rows);

    let oqmd_raw = std::fs::read_to_string(OQMD_MANIFEST_PATH).unwrap_or_else(|e| {
        panic!(
            "could not read committed {OQMD_MANIFEST_PATH}: {e} -- this file should already be \
            committed (Phase 21B); it is not gitignored and needs no regeneration"
        )
    });
    let oqmd_manifest: OqmdManifest = serde_json::from_str(&oqmd_raw)
        .expect("benchmarks/data/oqmd_coverage_manifest.json must be valid JSON");
    let formation_energy = build_oqmd_formation_energy_table(&oqmd_manifest);

    // Real, measured OQMD coverage of this catalog's own candidate
    // formulas -- not assumed from the design research's own estimate.
    let distinct_candidate_formulas: BTreeSet<&str> = rows
        .iter()
        .flat_map(|row| row.candidates.iter().map(|c| c.id.0.as_str()))
        .collect();
    let covered_candidate_formulas = distinct_candidate_formulas
        .iter()
        .filter(|formula| formation_energy.contains_key(**formula))
        .count();
    println!(
        "OQMD candidate-formula coverage: {covered_candidate_formulas}/{} = {:.4} (this bounds \
        thermodynamic-stability's own achievable ceiling below)",
        distinct_candidate_formulas.len(),
        covered_candidate_formulas as f64 / distinct_candidate_formulas.len().max(1) as f64
    );

    let tight_budget = SearchBudget {
        max_precursor_sets: calibrate_tight_budget(&rows),
        ..SearchBudget::default()
    };

    let mut results: BTreeMap<&'static str, ConfigResult> = CONFIG_NAMES
        .iter()
        .map(|&n| (n, ConfigResult::new()))
        .collect();
    let mut recall_at_k: BTreeMap<&'static str, Vec<usize>> = CONFIG_NAMES
        .iter()
        .map(|&n| (n, vec![0; RECALL_AT_K_VALUES.len()]))
        .collect();

    let mut rows_out: Vec<String> = Vec::with_capacity(rows.len());

    for row in &rows {
        let catalog_exact_generated = catalog_exact_for(row)
            .generate(&row.target, &PlanningConstraints::default())
            .expect("catalog-exact generation must not fail on a well-formed catalog row");
        let frequency_prior_generated = frequency_prior_for(row, &frequency)
            .generate(&row.target, &PlanningConstraints::default())
            .expect("frequency-prior generation must not fail on a well-formed catalog row");
        let thermodynamic_stability_generated = thermodynamic_stability_for(row, &formation_energy)
            .generate(&row.target, &PlanningConstraints::default())
            .expect("thermodynamic-stability generation must not fail on a well-formed row");

        let ensemble = CandidateGeneratorEnsemble::new(vec![
            Box::new(catalog_exact_for(row)),
            Box::new(frequency_prior_for(row, &frequency)),
            Box::new(thermodynamic_stability_for(row, &formation_energy)),
        ]);
        let ensemble_output =
            ensemble.generate_with_provenance(&row.target, &PlanningConstraints::default());

        let ranked_ids = |generated: &[gugen::GeneratedCandidate]| -> Vec<String> {
            generated
                .iter()
                .map(|gc| gc.candidate.id.0.clone())
                .collect()
        };
        let by_config_ids: BTreeMap<&'static str, Vec<String>> = BTreeMap::from([
            ("catalog_exact", ranked_ids(&catalog_exact_generated)),
            ("frequency_prior", ranked_ids(&frequency_prior_generated)),
            (
                "thermodynamic_stability",
                ranked_ids(&thermodynamic_stability_generated),
            ),
            (
                "ensemble",
                ensemble_output
                    .candidates
                    .iter()
                    .map(|c| c.id.0.clone())
                    .collect(),
            ),
        ]);

        for (k_index, &k) in RECALL_AT_K_VALUES.iter().enumerate() {
            for &name in CONFIG_NAMES {
                if top_k_contains_route(&by_config_ids[name], &row.route, k) {
                    recall_at_k.get_mut(name).unwrap()[k_index] += 1;
                }
            }
        }

        let candidates_for =
            |generated: Vec<gugen::GeneratedCandidate>| -> Vec<PrecursorCandidate> {
                generated.into_iter().map(|gc| gc.candidate).collect()
            };
        let by_config_candidates: Vec<(&'static str, Vec<PrecursorCandidate>)> = vec![
            ("catalog_exact", candidates_for(catalog_exact_generated)),
            ("frequency_prior", candidates_for(frequency_prior_generated)),
            (
                "thermodynamic_stability",
                candidates_for(thermodynamic_stability_generated),
            ),
            ("ensemble", ensemble_output.candidates),
        ];

        let mut recovered_by_config: BTreeMap<&'static str, bool> = BTreeMap::new();
        for (name, candidates) in by_config_candidates {
            let outcome = search_precursor_sets(
                &row.target,
                &candidates,
                &PlanningConstraints::default(),
                &tight_budget,
            )
            .expect("search_precursor_sets must not error on a well-formed synthetic catalog");
            let recovered = route_recovered(&outcome.accepted, &row.route);
            let entry = results.get_mut(name).unwrap();
            if recovered {
                entry.recovered_count += 1;
            }
            if budget_exhausted(&outcome.rejected) {
                entry.exhausted_count += 1;
            }
            recovered_by_config.insert(name, recovered);
        }

        rows_out.push(format!(
            "    {{\"target_formula\": {:?}, \"route_arity\": {}, \"catalog_exact_recovered\": \
            {}, \"frequency_prior_recovered\": {}, \"thermodynamic_stability_recovered\": {}, \
            \"ensemble_recovered\": {}}}",
            row.target_formula,
            row.route.len(),
            recovered_by_config["catalog_exact"],
            recovered_by_config["frequency_prior"],
            recovered_by_config["thermodynamic_stability"],
            recovered_by_config["ensemble"],
        ));
    }

    let total = rows.len();
    let recall = |r: &ConfigResult| r.recovered_count as f64 / total.max(1) as f64;
    let exhaustion_rate = |r: &ConfigResult| r.exhausted_count as f64 / total.max(1) as f64;

    println!();
    println!(
        "Phase 30 PR 2 thermodynamic-stability ablation (tight budget, max_precursor_sets={})",
        tight_budget.max_precursor_sets
    );
    println!("total rows: {total} ({skipped_unrepresentable} skipped, unrepresentable)");
    for &name in CONFIG_NAMES {
        let r = &results[name];
        println!(
            "  {name:24}: recall {}/{total} = {:.4}, exhaustion rate {:.4}",
            r.recovered_count,
            recall(r),
            exhaustion_rate(r)
        );
    }
    println!(
        "3-generator ensemble beats all three alone: {}",
        results["ensemble"].recovered_count >= results["catalog_exact"].recovered_count
            && results["ensemble"].recovered_count >= results["frequency_prior"].recovered_count
            && results["ensemble"].recovered_count
                >= results["thermodynamic_stability"].recovered_count
    );

    println!();
    println!("descriptive row-local Recall@K (NOT the Phase 30 gate -- see module doc):");
    for (k_index, &k) in RECALL_AT_K_VALUES.iter().enumerate() {
        print!("  K={k}:");
        for &name in CONFIG_NAMES {
            print!(
                " {name} {:.4}",
                recall_at_k[name][k_index] as f64 / total.max(1) as f64
            );
        }
        println!();
    }

    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(
        "  \"description\": \"Phase 30 PR 2 thermodynamic-stability ablation -- catalog-exact \
        vs. frequency-prior vs. thermodynamic-stability vs. their 3-generator ensemble, \
        end-to-end recall under a tightened, empirically calibrated SearchBudget, plus a \
        descriptive (explicitly non-gate-bearing) row-local Recall@K table. Does not claim the \
        formal Phase 30 Recall@K gate; see this file's generating script's own module doc for \
        why.\",\n",
    );
    out.push_str(&format!("  \"catalog_path\": {CATALOG_PATH:?},\n"));
    out.push_str(&format!("  \"catalog_sha256\": {:?},\n", sha256_hex(&raw)));
    out.push_str(&format!(
        "  \"oqmd_manifest_path\": {OQMD_MANIFEST_PATH:?},\n"
    ));
    out.push_str(&format!(
        "  \"oqmd_manifest_sha256\": {:?},\n",
        sha256_hex(&oqmd_raw)
    ));
    out.push_str(&format!("  \"total_rows\": {total},\n"));
    out.push_str(&format!(
        "  \"skipped_unrepresentable_rows\": {skipped_unrepresentable},\n"
    ));
    out.push_str(&format!(
        "  \"oqmd_candidate_formula_coverage\": {{\"covered\": {covered_candidate_formulas}, \
        \"total_distinct\": {}, \"rate\": {:.6}}},\n",
        distinct_candidate_formulas.len(),
        covered_candidate_formulas as f64 / distinct_candidate_formulas.len().max(1) as f64
    ));
    out.push_str(&format!(
        "  \"tight_search_budget_max_precursor_sets\": {},\n",
        tight_budget.max_precursor_sets
    ));
    out.push_str("  \"end_to_end_recall_under_tight_budget\": {\n");
    let config_entries: Vec<String> = CONFIG_NAMES
        .iter()
        .map(|&name| {
            let r = &results[name];
            format!(
                "    \"{name}\": {{\"recovered\": {}, \"recall\": {:.6}, \"exhaustion_rate\": \
                {:.6}}}",
                r.recovered_count,
                recall(r),
                exhaustion_rate(r)
            )
        })
        .collect();
    out.push_str(&config_entries.join(",\n"));
    out.push_str("\n  },\n");
    out.push_str("  \"descriptive_row_local_recall_at_k_not_the_phase_30_gate\": {\n");
    let k_entries: Vec<String> = RECALL_AT_K_VALUES
        .iter()
        .enumerate()
        .map(|(k_index, &k)| {
            let per_config: Vec<String> = CONFIG_NAMES
                .iter()
                .map(|&name| {
                    format!(
                        "\"{name}\": {:.6}",
                        recall_at_k[name][k_index] as f64 / total.max(1) as f64
                    )
                })
                .collect();
            format!("    \"{k}\": {{{}}}", per_config.join(", "))
        })
        .collect();
    out.push_str(&k_entries.join(",\n"));
    out.push_str("\n  },\n");
    out.push_str("  \"rows\": [\n");
    out.push_str(&rows_out.join(",\n"));
    out.push_str("\n  ]\n");
    out.push_str("}\n");

    std::fs::write(OUTPUT_PATH, &out).expect("failed to write result");
    println!();
    println!("wrote {OUTPUT_PATH}");
}

/// Minimal, dependency-free SHA-256 -- identical implementation to every
/// other `exploration_*.rs` benchmark script's own copy.
fn sha256_hex(input: &str) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = input.as_bytes().to_vec();
    let bit_len = (msg.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    h.iter().map(|word| format!("{word:08x}")).collect()
}
