//! Phase 30 PR 1: measures `CatalogExactGenerator`, `FrequencyPriorGenerator`,
//! and their `CandidateGeneratorEnsemble` fusion against the same frozen
//! exploration-recall catalog every byproduct-fix script has used.
//!
//! **This is deliberately NOT a claim that the formal Phase 30 gate
//! ("ensemble Recall@K beats every single generator alone; per-generator
//! ablation published") has been evaluated.** The frozen catalog's
//! per-row candidate pool (`benchmarks/exploration_build_frozen_decoy_catalog.py`)
//! places the true route first, then fills up to 28 slots with decoys
//! drawn from the *same global frequency ranking* a frequency-prior
//! generator uses internally -- so a naive Recall@K over that fixed,
//! always-complete, frequency-selected pool would partly measure the
//! benchmark's own construction, not real generator behavior. This
//! script reports two things instead, both honestly labeled:
//!
//! 1. **End-to-end recall under a tightened `SearchBudget`** (calibrated
//!    empirically below, not hard-coded): a real, falsifiable,
//!    decoy-selection-independent comparison. Under a small budget,
//!    catalog-exact's unranked candidate order can exhaust search before
//!    finding a findable route; frequency-prior's narrower, ranked
//!    proposal can let the same bounded search succeed where catalog-
//!    exact didn't; the ensemble can win on both. Reuses the existing
//!    binary `route_recovered` harness unchanged.
//! 2. **Descriptive row-local Recall@K** (several K values): whether each
//!    generator's own top-K ranked output contains every true-route
//!    formula. Explicitly disclosed as confounded by the decoy pool's
//!    frequency-based construction -- useful signal, not a gate result.
//!
//! Every row's raw JSON candidate list is only ever consumed through
//! `CatalogExactGenerator`/`FrequencyPriorGenerator`, both of which
//! re-sort internally (id order / frequency order respectively) before
//! producing a candidate list -- so the row's raw true-precursors-first
//! JSON order, which `search_precursor_sets`'s own lexicographic `chosen`
//! tiebreak would otherwise silently favor, never reaches
//! `search_precursor_sets` unneutralized.
//!
//! Run: `cargo run --release --example exploration_candidate_generator_pr1_ablation
//! --features serde` after regenerating the (gitignored) frozen catalog
//! locally (see `exploration_recall_baseline.rs`'s own doc comment for
//! the exact commands) -- release mode matters, not just for comfort.
//!
//! Writes `benchmarks/data/exploration_candidate_generator_pr1_result.json`,
//! a new, separate committed result file. No new gitignore entry needed:
//! the frequency table is computed in-process from the already-gitignored
//! frozen catalog each run, no new large intermediate artifact.

use gugen::{
    CandidateGenerator, CandidateGeneratorEnsemble, CatalogExactGenerator, Composition, Element,
    FrequencyPriorGenerator, InMemoryPrecursorCatalog, PlanningConstraints, PrecursorCandidate,
    PrecursorId, RejectionCode, SearchBudget, search_precursor_sets,
};
use std::collections::{BTreeMap, BTreeSet};

const CATALOG_PATH: &str = "benchmarks/data/exploration_frozen_catalog_manifest.json";
const OUTPUT_PATH: &str = "benchmarks/data/exploration_candidate_generator_pr1_result.json";

/// Every Nth row (deterministic stride, not a prefix) is used to
/// calibrate the tight `SearchBudget` below -- never used for the final
/// reported numbers, which always run over the full catalog. A stride
/// avoids a real, measured bias: the catalog's row order is not random
/// (an early version of this calibration used a 300-row prefix and its
/// predicted exhaustion rate came out well below the full corpus's own
/// rate at the same budget -- confirmed a stride sample tracks the full
/// corpus far more closely before trusting any calibration result).
const CALIBRATION_STRIDE: usize = 10;

/// Candidates for the tightened budget's `max_precursor_sets`, tried
/// largest-first. Picks the largest value whose catalog-exact exhaustion
/// rate on the calibration sample lands in [0.30, 0.70] -- comfortably
/// inside the middle of the range, not just barely crossing an extreme,
/// so the 3-way comparison has real room to differ in either direction.
const CANDIDATE_BUDGETS: &[usize] = &[
    10_000, 5_000, 2_000, 1_000, 500, 200, 100, 50, 30, 20, 15, 10, 7, 5, 3,
];

/// Row-local Recall@K is only ever descriptive here (see module doc) --
/// several K values across the pool-size range, not one hand-picked
/// "discriminating" value.
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
    /// Raw JSON order (true route first, then decoys) -- never fed
    /// directly to search; only ever consumed through a generator, which
    /// re-sorts it (see module doc).
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

/// Global "how often is this formula a real cited precursor" table, built
/// once from every row's `route` (never from `candidates`, which already
/// includes decoys chosen via this same ranking -- counting from
/// `candidates` would partly reproduce the decoy selection's own bias
/// rather than measuring an independent signal).
fn build_frequency_table(rows: &[ParsedRow]) -> BTreeMap<String, u64> {
    let mut frequency = BTreeMap::new();
    for row in rows {
        for formula in &row.route {
            *frequency.entry(formula.clone()).or_insert(0u64) += 1;
        }
    }
    frequency
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

/// Calibrates the tight `SearchBudget` by running catalog-exact alone
/// against every `CALIBRATION_STRIDE`th row (spread across the whole
/// catalog, not a prefix -- see `CALIBRATION_STRIDE`'s doc comment) at
/// each candidate budget (largest first), picking the largest whose
/// exhaustion rate lands in [0.30, 0.70].
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

fn top_k_contains_route(ranked_ids: &[String], route: &[String], k: usize) -> bool {
    let top_k: BTreeSet<&str> = ranked_ids.iter().take(k).map(String::as_str).collect();
    route.iter().all(|formula| top_k.contains(formula.as_str()))
}

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

    let tight_budget = SearchBudget {
        max_precursor_sets: calibrate_tight_budget(&rows),
        ..SearchBudget::default()
    };

    let mut catalog_exact_result = ConfigResult {
        recovered_count: 0,
        exhausted_count: 0,
    };
    let mut frequency_prior_result = ConfigResult {
        recovered_count: 0,
        exhausted_count: 0,
    };
    let mut ensemble_result = ConfigResult {
        recovered_count: 0,
        exhausted_count: 0,
    };

    // Descriptive row-local Recall@K accumulators: recall_at_k[config][k_index].
    let mut recall_at_k: BTreeMap<&'static str, Vec<usize>> = BTreeMap::new();
    recall_at_k.insert("catalog-exact", vec![0; RECALL_AT_K_VALUES.len()]);
    recall_at_k.insert("frequency-prior", vec![0; RECALL_AT_K_VALUES.len()]);
    recall_at_k.insert("ensemble", vec![0; RECALL_AT_K_VALUES.len()]);

    let mut rows_out: Vec<String> = Vec::with_capacity(rows.len());

    for row in &rows {
        let catalog_exact = catalog_exact_for(row);
        let frequency_prior = frequency_prior_for(row, &frequency);

        let catalog_exact_generated = catalog_exact
            .generate(&row.target, &PlanningConstraints::default())
            .expect("catalog-exact generation must not fail on a well-formed catalog row");
        let frequency_prior_generated = frequency_prior
            .generate(&row.target, &PlanningConstraints::default())
            .expect("frequency-prior generation must not fail on a well-formed catalog row");

        let ensemble = CandidateGeneratorEnsemble::new(vec![
            Box::new(catalog_exact_for(row)),
            Box::new(frequency_prior_for(row, &frequency)),
        ]);
        let ensemble_output =
            ensemble.generate_with_provenance(&row.target, &PlanningConstraints::default());

        let catalog_exact_ids: Vec<String> = catalog_exact_generated
            .iter()
            .map(|gc| gc.candidate.id.0.clone())
            .collect();
        let frequency_prior_ids: Vec<String> = frequency_prior_generated
            .iter()
            .map(|gc| gc.candidate.id.0.clone())
            .collect();
        let ensemble_ids: Vec<String> = ensemble_output
            .candidates
            .iter()
            .map(|c| c.id.0.clone())
            .collect();

        for (k_index, &k) in RECALL_AT_K_VALUES.iter().enumerate() {
            if top_k_contains_route(&catalog_exact_ids, &row.route, k) {
                recall_at_k.get_mut("catalog-exact").unwrap()[k_index] += 1;
            }
            if top_k_contains_route(&frequency_prior_ids, &row.route, k) {
                recall_at_k.get_mut("frequency-prior").unwrap()[k_index] += 1;
            }
            if top_k_contains_route(&ensemble_ids, &row.route, k) {
                recall_at_k.get_mut("ensemble").unwrap()[k_index] += 1;
            }
        }

        let catalog_exact_candidates: Vec<PrecursorCandidate> = catalog_exact_generated
            .into_iter()
            .map(|gc| gc.candidate)
            .collect();
        let frequency_prior_candidates: Vec<PrecursorCandidate> = frequency_prior_generated
            .into_iter()
            .map(|gc| gc.candidate)
            .collect();

        let catalog_exact_outcome = search_precursor_sets(
            &row.target,
            &catalog_exact_candidates,
            &PlanningConstraints::default(),
            &tight_budget,
        )
        .expect("search_precursor_sets must not error on a well-formed synthetic catalog");
        let frequency_prior_outcome = search_precursor_sets(
            &row.target,
            &frequency_prior_candidates,
            &PlanningConstraints::default(),
            &tight_budget,
        )
        .expect("search_precursor_sets must not error on a well-formed synthetic catalog");
        let ensemble_outcome = search_precursor_sets(
            &row.target,
            &ensemble_output.candidates,
            &PlanningConstraints::default(),
            &tight_budget,
        )
        .expect("search_precursor_sets must not error on a well-formed synthetic catalog");

        let catalog_exact_recovered = route_recovered(&catalog_exact_outcome.accepted, &row.route);
        let frequency_prior_recovered =
            route_recovered(&frequency_prior_outcome.accepted, &row.route);
        let ensemble_recovered = route_recovered(&ensemble_outcome.accepted, &row.route);

        if catalog_exact_recovered {
            catalog_exact_result.recovered_count += 1;
        }
        if budget_exhausted(&catalog_exact_outcome.rejected) {
            catalog_exact_result.exhausted_count += 1;
        }
        if frequency_prior_recovered {
            frequency_prior_result.recovered_count += 1;
        }
        if budget_exhausted(&frequency_prior_outcome.rejected) {
            frequency_prior_result.exhausted_count += 1;
        }
        if ensemble_recovered {
            ensemble_result.recovered_count += 1;
        }
        if budget_exhausted(&ensemble_outcome.rejected) {
            ensemble_result.exhausted_count += 1;
        }

        rows_out.push(format!(
            "    {{\"target_formula\": {:?}, \"route_arity\": {}, \"catalog_exact_recovered\": \
            {catalog_exact_recovered}, \"frequency_prior_recovered\": {frequency_prior_recovered}, \
            \"ensemble_recovered\": {ensemble_recovered}}}",
            row.target_formula,
            row.route.len(),
        ));
    }

    let total = rows.len();
    let recall = |r: &ConfigResult| r.recovered_count as f64 / total.max(1) as f64;
    let exhaustion_rate = |r: &ConfigResult| r.exhausted_count as f64 / total.max(1) as f64;

    println!();
    println!(
        "Phase 30 PR 1 candidate-generator ablation (tight budget, max_precursor_sets={})",
        tight_budget.max_precursor_sets
    );
    println!("total rows: {total} ({skipped_unrepresentable} skipped, unrepresentable)");
    println!(
        "  catalog-exact:    recall {}/{total} = {:.4}, exhaustion rate {:.4}",
        catalog_exact_result.recovered_count,
        recall(&catalog_exact_result),
        exhaustion_rate(&catalog_exact_result)
    );
    println!(
        "  frequency-prior:  recall {}/{total} = {:.4}, exhaustion rate {:.4}",
        frequency_prior_result.recovered_count,
        recall(&frequency_prior_result),
        exhaustion_rate(&frequency_prior_result)
    );
    println!(
        "  ensemble:         recall {}/{total} = {:.4}, exhaustion rate {:.4}",
        ensemble_result.recovered_count,
        recall(&ensemble_result),
        exhaustion_rate(&ensemble_result)
    );
    println!(
        "ensemble beats both alone: {}",
        ensemble_result.recovered_count >= catalog_exact_result.recovered_count
            && ensemble_result.recovered_count >= frequency_prior_result.recovered_count
    );

    println!();
    println!(
        "descriptive row-local Recall@K (NOT the Phase 30 gate -- see module doc: this \
        catalog's decoy pool was itself built from the same frequency ranking frequency-prior \
        uses, so this table partly reflects the benchmark's own construction):"
    );
    for (k_index, &k) in RECALL_AT_K_VALUES.iter().enumerate() {
        println!(
            "  K={k}: catalog-exact {:.4}, frequency-prior {:.4}, ensemble {:.4}",
            recall_at_k["catalog-exact"][k_index] as f64 / total.max(1) as f64,
            recall_at_k["frequency-prior"][k_index] as f64 / total.max(1) as f64,
            recall_at_k["ensemble"][k_index] as f64 / total.max(1) as f64,
        );
    }

    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(
        "  \"description\": \"Phase 30 PR 1 candidate-generator ablation -- catalog-exact vs. \
        frequency-prior vs. their ensemble, end-to-end recall under a tightened, empirically \
        calibrated SearchBudget, plus a descriptive (explicitly non-gate-bearing) row-local \
        Recall@K table. Does not claim the formal Phase 30 Recall@K gate; see this file's \
        generating script's own module doc for why.\",\n",
    );
    out.push_str(&format!("  \"catalog_path\": {CATALOG_PATH:?},\n"));
    out.push_str(&format!("  \"catalog_sha256\": {:?},\n", sha256_hex(&raw)));
    out.push_str(&format!("  \"total_rows\": {total},\n"));
    out.push_str(&format!(
        "  \"skipped_unrepresentable_rows\": {skipped_unrepresentable},\n"
    ));
    out.push_str(&format!(
        "  \"tight_search_budget_max_precursor_sets\": {},\n",
        tight_budget.max_precursor_sets
    ));
    out.push_str("  \"end_to_end_recall_under_tight_budget\": {\n");
    out.push_str(&format!(
        "    \"catalog_exact\": {{\"recovered\": {}, \"recall\": {:.6}, \"exhaustion_rate\": \
        {:.6}}},\n",
        catalog_exact_result.recovered_count,
        recall(&catalog_exact_result),
        exhaustion_rate(&catalog_exact_result)
    ));
    out.push_str(&format!(
        "    \"frequency_prior\": {{\"recovered\": {}, \"recall\": {:.6}, \"exhaustion_rate\": \
        {:.6}}},\n",
        frequency_prior_result.recovered_count,
        recall(&frequency_prior_result),
        exhaustion_rate(&frequency_prior_result)
    ));
    out.push_str(&format!(
        "    \"ensemble\": {{\"recovered\": {}, \"recall\": {:.6}, \"exhaustion_rate\": \
        {:.6}}}\n",
        ensemble_result.recovered_count,
        recall(&ensemble_result),
        exhaustion_rate(&ensemble_result)
    ));
    out.push_str("  },\n");
    out.push_str("  \"descriptive_row_local_recall_at_k_not_the_phase_30_gate\": {\n");
    let k_entries: Vec<String> = RECALL_AT_K_VALUES
        .iter()
        .enumerate()
        .map(|(k_index, &k)| {
            format!(
                "    \"{k}\": {{\"catalog_exact\": {:.6}, \"frequency_prior\": {:.6}, \
                \"ensemble\": {:.6}}}",
                recall_at_k["catalog-exact"][k_index] as f64 / total.max(1) as f64,
                recall_at_k["frequency-prior"][k_index] as f64 / total.max(1) as f64,
                recall_at_k["ensemble"][k_index] as f64 / total.max(1) as f64,
            )
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
/// other `exploration_*.rs` benchmark script's own copy (see e.g.
/// `exploration_result_acetate_byproduct.rs`'s doc comment for why each
/// `examples/*.rs` binary carries an independent copy rather than adding
/// a `sha2` dependency for one checksum use).
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
