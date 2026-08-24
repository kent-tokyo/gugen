//! Phase 30.5 — Candidate Fusion × Search Coupling Audit. Diagnostic
//! only: isolates whether the ensemble's end-to-end recall gap (PR 1/PR 2
//! both failed to beat `catalog-exact` alone) is caused by generator
//! signal weakness, the min-rank fusion rule, candidate-order dependence
//! in `search_precursor_sets`'s frontier tie-break, or search budget.
//!
//! Pre-registered hypotheses, methodology, and every constant below are
//! committed in `docs/exploration_fusion_search_coupling.md` BEFORE this
//! script was ever run -- read that file first. This script does not
//! introduce a new `CandidateGenerator`, does not wire anything into
//! `Planner`, and does not change `search_precursor_sets`'s own real
//! behavior -- it only exercises the feature-gated, diagnostic-only
//! `search_precursor_sets_diagnostic` (`search_diagnostics` feature).
//!
//! Two-stage discipline, per the pre-registered decision gate: the full
//! factorial (candidate order/fusion × tie-break × budget) is explored on
//! the **development split only** (an explicit stride subsample of it,
//! for tractable runtime -- see `DEV_FACTORIAL_STRIDE`), then exactly the
//! single best-performing development-side policy is run once, in full,
//! against the **confirmation holdout split** for the GO/NO-GO verdict.
//! The holdout is never touched before that policy choice is locked.
//!
//! Run: `cargo run --release --example exploration_fusion_search_coupling_audit
//! --features "serde search_diagnostics"` after regenerating the
//! (gitignored) frozen catalog locally (see
//! `exploration_recall_baseline.rs`'s own doc comment for the exact
//! commands). `benchmarks/data/oqmd_coverage_manifest.json` is already
//! committed, no regeneration needed.
//!
//! Writes `benchmarks/data/exploration_fusion_search_audit_split_manifest.json`
//! and `benchmarks/data/exploration_fusion_search_audit_result.json`,
//! both new, both committed (small) -- never touches any PR 1/PR 2
//! result file.

use gugen::{
    CandidateGenerator, CatalogExactGenerator, Composition, Element, FrequencyPriorGenerator,
    GeneratedCandidate, InMemoryPrecursorCatalog, PlanningConstraints, PrecursorCandidate,
    PrecursorId, SearchBudget, ThermodynamicStabilityGenerator, TieBreakPolicy,
    search_precursor_sets_diagnostic,
};
use std::collections::{BTreeMap, BTreeSet};

const CATALOG_PATH: &str = "benchmarks/data/exploration_frozen_catalog_manifest.json";
const OQMD_MANIFEST_PATH: &str = "benchmarks/data/oqmd_coverage_manifest.json";
const SPLIT_MANIFEST_PATH: &str =
    "benchmarks/data/exploration_fusion_search_audit_split_manifest.json";
const RESULT_PATH: &str = "benchmarks/data/exploration_fusion_search_audit_result.json";

/// Cormack et al. 2009's own standard RRF constant -- literature default,
/// not tuned against this corpus. Pre-registered in
/// `docs/exploration_fusion_search_coupling.md`.
const RRF_K: f64 = 60.0;

/// `SearchBudget.max_precursor_sets` levels, pre-registered.
const BUDGETS: &[usize] = &[10, 20, 50, 100, 500, 100_000];

/// Development split is explored via every Nth row (deterministic
/// stride, same rationale as PR 1/PR 2's own budget-calibration stride
/// -- exploratory policy selection does not need every dev row, only a
/// representative sample; the confirmation holdout, by contrast, is
/// always run in full). Disclosed explicitly in every printed/written
/// summary, never silently implied to be the full dev split.
const DEV_FACTORIAL_STRIDE: usize = 5;

/// Target-group bootstrap resample count, pre-registered in
/// `docs/exploration_fusion_search_coupling.md`.
const BOOTSTRAP_RESAMPLES: usize = 10_000;

const RECIPROCAL_RANK_FUSION: &str = "reciprocal-rank-fusion";
const MIN_RANK: &str = "min-rank";
const MEAN_NORMALIZED_RANK: &str = "mean-normalized-rank";
const CONSENSUS_FIRST: &str = "consensus-first";
const ROUND_ROBIN: &str = "round-robin";
const CATALOG_ANCHORED: &str = "catalog-anchored";
const FUSION_RULES: &[&str] = &[
    MIN_RANK,
    RECIPROCAL_RANK_FUSION,
    MEAN_NORMALIZED_RANK,
    CONSENSUS_FIRST,
    ROUND_ROBIN,
    CATALOG_ANCHORED,
];

const ORDER_CATALOG_EXACT: &str = "A-catalog-exact";
const ORDER_REVERSE: &str = "B-reverse";
const ORDER_MIN_RANK_ENSEMBLE: &str = "D-min-rank-ensemble";
const ORDER_ORACLE: &str = "oracle";
const SHUFFLE_SEEDS: &[&str] = &[
    "shuffle-1",
    "shuffle-2",
    "shuffle-3",
    "shuffle-4",
    "shuffle-5",
    "shuffle-6",
    "shuffle-7",
    "shuffle-8",
    "shuffle-9",
    "shuffle-10",
];

const TIE_BREAK_INDEX_ORDER: &str = "T1-index-order";
const TIE_BREAK_FUSION_PRIORITY_SUM: &str = "T3-fusion-priority-sum";
const TIE_BREAK_MARGINAL_COVERAGE: &str = "T4-marginal-coverage";

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

fn try_composition(elements: &BTreeMap<String, f64>) -> Option<Composition> {
    let pairs: Option<Vec<(Element, f64)>> = elements
        .iter()
        .map(|(sym, amt)| Element::new(sym).ok().map(|e| (e, *amt)))
        .collect();
    Composition::new(pairs?).ok()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Split {
    Development,
    ConfirmationHoldout,
}

struct ParsedRow {
    target_formula: String,
    target: Composition,
    route: Vec<String>,
    candidates: Vec<PrecursorCandidate>,
    split: Split,
}

/// Deterministic, target-level 80/20 split: first byte of
/// `sha256_hex(target_formula)` mod 5 -- `0..=3` development, `4`
/// confirmation holdout. Pre-registered in
/// `docs/exploration_fusion_search_coupling.md`. Every row sharing a
/// `target_formula` lands in the same split by construction, since the
/// key is the formula string, not the row.
fn split_for(target_formula: &str) -> Split {
    let digest = sha256_hex(target_formula);
    let first_byte = u8::from_str_radix(&digest[0..2], 16).expect("sha256 hex must parse");
    if first_byte % 5 == 4 {
        Split::ConfirmationHoldout
    } else {
        Split::Development
    }
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
            split: split_for(&row.target_formula),
        });
    }
    (parsed, skipped)
}

fn build_frequency_table(rows: &[ParsedRow]) -> BTreeMap<String, u64> {
    let mut frequency = BTreeMap::new();
    for row in rows {
        for formula in &row.route {
            *frequency.entry(formula.clone()).or_insert(0u64) += 1;
        }
    }
    frequency
}

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

/// The three generators' own outputs for one row, materialized once and
/// reused across every candidate-order/fusion-rule/tie-break/budget cell
/// for that row -- generation itself is cheap; only the search calls are
/// the expensive part.
struct RowGeneratorOutputs {
    catalog_exact: Vec<GeneratedCandidate>,
    frequency_prior: Vec<GeneratedCandidate>,
    thermodynamic_stability: Vec<GeneratedCandidate>,
}

fn generator_outputs_for(
    row: &ParsedRow,
    frequency: &BTreeMap<String, u64>,
    formation_energy: &BTreeMap<String, f64>,
) -> RowGeneratorOutputs {
    RowGeneratorOutputs {
        catalog_exact: catalog_exact_for(row)
            .generate(&row.target, &PlanningConstraints::default())
            .expect("catalog-exact generation must not fail on a well-formed catalog row"),
        frequency_prior: frequency_prior_for(row, frequency)
            .generate(&row.target, &PlanningConstraints::default())
            .expect("frequency-prior generation must not fail on a well-formed catalog row"),
        thermodynamic_stability: thermodynamic_stability_for(row, formation_energy)
            .generate(&row.target, &PlanningConstraints::default())
            .expect("thermodynamic-stability generation must not fail on a well-formed row"),
    }
}

fn rank_map(
    generated: &[GeneratedCandidate],
) -> BTreeMap<PrecursorId, (PrecursorCandidate, usize)> {
    generated
        .iter()
        .map(|gc| (gc.candidate.id.clone(), (gc.candidate.clone(), gc.rank)))
        .collect()
}

/// `maps[0]` is always catalog-exact's own rank map, by this file's own
/// convention (every fusion function below relies on this ordering).
fn rank_maps(
    outputs: &RowGeneratorOutputs,
) -> Vec<BTreeMap<PrecursorId, (PrecursorCandidate, usize)>> {
    vec![
        rank_map(&outputs.catalog_exact),
        rank_map(&outputs.frequency_prior),
        rank_map(&outputs.thermodynamic_stability),
    ]
}

fn all_ids(maps: &[BTreeMap<PrecursorId, (PrecursorCandidate, usize)>]) -> BTreeSet<PrecursorId> {
    maps.iter().flat_map(|m| m.keys().cloned()).collect()
}

fn finish_sorted(mut scored: Vec<(PrecursorCandidate, f64)>) -> Vec<(PrecursorCandidate, f64)> {
    scored.sort_by(|(a, a_score), (b, b_score)| {
        a_score.total_cmp(b_score).then_with(|| a.id.0.cmp(&b.id.0))
    });
    scored
}

/// Fusion rule 1: MinRank -- production's own real rule
/// (`CandidateGeneratorEnsemble`), reimplemented here as a pure function
/// over already-materialized generator outputs so every fusion rule
/// shares one uniform `Vec<(PrecursorCandidate, f64)>` shape.
fn fuse_min_rank(
    maps: &[BTreeMap<PrecursorId, (PrecursorCandidate, usize)>],
) -> Vec<(PrecursorCandidate, f64)> {
    let scored: Vec<(PrecursorCandidate, f64)> = all_ids(maps)
        .into_iter()
        .filter_map(|id| {
            let mut best: Option<(PrecursorCandidate, usize)> = None;
            for m in maps {
                if let Some((c, r)) = m.get(&id) {
                    if best.as_ref().is_none_or(|(_, br)| r < br) {
                        best = Some((c.clone(), *r));
                    }
                }
            }
            best.map(|(c, r)| (c, r as f64))
        })
        .collect();
    finish_sorted(scored)
}

/// Fusion rule 2: Reciprocal Rank Fusion, `sum(1 / (RRF_K + rank + 1))`
/// across generators that proposed the id -- higher RRF score is better,
/// negated here so every fusion rule shares the same "lower score sorts
/// first" convention.
fn fuse_reciprocal_rank(
    maps: &[BTreeMap<PrecursorId, (PrecursorCandidate, usize)>],
) -> Vec<(PrecursorCandidate, f64)> {
    let scored: Vec<(PrecursorCandidate, f64)> = all_ids(maps)
        .into_iter()
        .filter_map(|id| {
            let mut sum = 0.0;
            let mut candidate: Option<PrecursorCandidate> = None;
            for m in maps {
                if let Some((c, r)) = m.get(&id) {
                    sum += 1.0 / (RRF_K + *r as f64 + 1.0);
                    if candidate.is_none() {
                        candidate = Some(c.clone());
                    }
                }
            }
            candidate.map(|c| (c, -sum))
        })
        .collect();
    finish_sorted(scored)
}

/// Fusion rule 3: mean of each generator's own rank, normalized to
/// `[0, 1]` by dividing by that generator's own maximum rank -- a
/// generator that never proposed a given id contributes the pre-defined
/// worst-case penalty, `1.0`, for that id (not silently excluded).
fn fuse_mean_normalized_rank(
    maps: &[BTreeMap<PrecursorId, (PrecursorCandidate, usize)>],
) -> Vec<(PrecursorCandidate, f64)> {
    let max_ranks: Vec<f64> = maps
        .iter()
        .map(|m| m.values().map(|(_, r)| *r).max().unwrap_or(0) as f64)
        .collect();
    let scored: Vec<(PrecursorCandidate, f64)> = all_ids(maps)
        .into_iter()
        .filter_map(|id| {
            let mut candidate: Option<PrecursorCandidate> = None;
            let mut normalized_sum = 0.0;
            for (m, &max_rank) in maps.iter().zip(max_ranks.iter()) {
                normalized_sum += match m.get(&id) {
                    Some((c, r)) => {
                        if candidate.is_none() {
                            candidate = Some(c.clone());
                        }
                        if max_rank > 0.0 {
                            *r as f64 / max_rank
                        } else {
                            0.0
                        }
                    }
                    None => 1.0,
                };
            }
            candidate.map(|c| (c, normalized_sum / maps.len() as f64))
        })
        .collect();
    finish_sorted(scored)
}

/// Fusion rule 4: more proposing generators wins first, then lower mean
/// rank among those that proposed it, then `PrecursorId` -- converted to
/// a synthetic ascending score (its final rank position) so the return
/// shape matches every other fusion rule.
fn fuse_consensus_first(
    maps: &[BTreeMap<PrecursorId, (PrecursorCandidate, usize)>],
) -> Vec<(PrecursorCandidate, f64)> {
    let mut scored: Vec<(PrecursorCandidate, f64, usize)> = all_ids(maps)
        .into_iter()
        .filter_map(|id| {
            let mut candidate: Option<PrecursorCandidate> = None;
            let mut ranks = Vec::new();
            for m in maps {
                if let Some((c, r)) = m.get(&id) {
                    if candidate.is_none() {
                        candidate = Some(c.clone());
                    }
                    ranks.push(*r);
                }
            }
            candidate.map(|c| {
                let count = ranks.len();
                let mean_rank = ranks.iter().sum::<usize>() as f64 / count.max(1) as f64;
                (c, mean_rank, count)
            })
        })
        .collect();
    scored.sort_by(|(a, a_mean, a_count), (b, b_mean, b_count)| {
        b_count
            .cmp(a_count)
            .then_with(|| a_mean.total_cmp(b_mean))
            .then_with(|| a.id.0.cmp(&b.id.0))
    });
    scored
        .into_iter()
        .enumerate()
        .map(|(i, (c, _, _))| (c, i as f64))
        .collect()
}

/// Fusion rule 5: interleaves each generator's own ranked list, one
/// candidate at a time, skipping ids already taken -- converted to a
/// synthetic ascending score (output position).
fn fuse_round_robin(
    maps: &[BTreeMap<PrecursorId, (PrecursorCandidate, usize)>],
) -> Vec<(PrecursorCandidate, f64)> {
    let per_generator: Vec<Vec<(PrecursorId, PrecursorCandidate)>> = maps
        .iter()
        .map(|m| {
            let mut v: Vec<(usize, PrecursorId, PrecursorCandidate)> = m
                .iter()
                .map(|(id, (c, r))| (*r, id.clone(), c.clone()))
                .collect();
            v.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.0.cmp(&b.1.0)));
            v.into_iter().map(|(_, id, c)| (id, c)).collect()
        })
        .collect();
    let mut seen: BTreeSet<PrecursorId> = BTreeSet::new();
    let mut ordered: Vec<PrecursorCandidate> = Vec::new();
    let mut cursors = vec![0usize; per_generator.len()];
    loop {
        let mut advanced = false;
        for (gi, gen_list) in per_generator.iter().enumerate() {
            while cursors[gi] < gen_list.len() {
                let (id, c) = &gen_list[cursors[gi]];
                cursors[gi] += 1;
                if seen.insert(id.clone()) {
                    ordered.push(c.clone());
                    advanced = true;
                    break;
                }
            }
        }
        if !advanced {
            break;
        }
    }
    ordered
        .into_iter()
        .enumerate()
        .map(|(i, c)| (c, i as f64))
        .collect()
}

/// Fusion rule 6: keeps catalog-exact's own relative order verbatim,
/// appending only candidates *no* other generator... only *other*
/// generators proposed (never catalog-exact), ranked by their own best
/// rank among those other generators. In this benchmark's own setup,
/// catalog-exact is always given the full row candidate pool, so this
/// rule is expected to collapse to catalog-exact's own order exactly --
/// a deliberate negative control, not a bug if so.
fn fuse_catalog_anchored(
    maps: &[BTreeMap<PrecursorId, (PrecursorCandidate, usize)>],
) -> Vec<(PrecursorCandidate, f64)> {
    let catalog_exact = &maps[0];
    let mut catalog_ordered: Vec<(PrecursorId, usize)> = catalog_exact
        .iter()
        .map(|(id, (_, r))| (id.clone(), *r))
        .collect();
    catalog_ordered.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.0.cmp(&b.0.0)));

    let mut seen: BTreeSet<PrecursorId> = catalog_exact.keys().cloned().collect();
    let mut extras: Vec<(PrecursorId, PrecursorCandidate, usize)> = Vec::new();
    for m in &maps[1..] {
        for (id, (c, r)) in m {
            if seen.contains(id) {
                continue;
            }
            match extras.iter_mut().find(|(eid, _, _)| eid == id) {
                Some((_, _, best_r)) => {
                    if *r < *best_r {
                        *best_r = *r;
                    }
                }
                None => extras.push((id.clone(), c.clone(), *r)),
            }
        }
    }
    extras.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.0.cmp(&b.0.0)));

    let mut ordered: Vec<PrecursorCandidate> = catalog_ordered
        .into_iter()
        .map(|(id, _)| catalog_exact[&id].0.clone())
        .collect();
    for (id, c, _) in extras {
        seen.insert(id);
        ordered.push(c);
    }
    ordered
        .into_iter()
        .enumerate()
        .map(|(i, c)| (c, i as f64))
        .collect()
}

fn fuse(
    rule: &str,
    maps: &[BTreeMap<PrecursorId, (PrecursorCandidate, usize)>],
) -> Vec<(PrecursorCandidate, f64)> {
    match rule {
        MIN_RANK => fuse_min_rank(maps),
        RECIPROCAL_RANK_FUSION => fuse_reciprocal_rank(maps),
        MEAN_NORMALIZED_RANK => fuse_mean_normalized_rank(maps),
        CONSENSUS_FIRST => fuse_consensus_first(maps),
        ROUND_ROBIN => fuse_round_robin(maps),
        CATALOG_ANCHORED => fuse_catalog_anchored(maps),
        other => panic!("unknown fusion rule {other}"),
    }
}

/// Deterministic permutation via sorting by `sha256_hex("{seed}:{id}")`
/// -- no `rand` dependency, fully reproducible. Pre-registered control E.
fn shuffled_order(seed: &str, candidates: &[PrecursorCandidate]) -> Vec<PrecursorCandidate> {
    let mut keyed: Vec<(String, PrecursorCandidate)> = candidates
        .iter()
        .map(|c| (sha256_hex(&format!("{seed}:{}", c.id.0)), c.clone()))
        .collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.into_iter().map(|(_, c)| c).collect()
}

/// Diagnostic-only ceiling: gold precursors first (by `route` order),
/// then everything else canonically -- never a real ordering policy.
fn oracle_order(route: &[String], candidates: &[PrecursorCandidate]) -> Vec<PrecursorCandidate> {
    let mut gold: Vec<PrecursorCandidate> = Vec::new();
    let mut rest: Vec<PrecursorCandidate> = Vec::new();
    for formula in route {
        if let Some(c) = candidates.iter().find(|c| &c.id.0 == formula) {
            gold.push(c.clone());
        }
    }
    let gold_ids: BTreeSet<&str> = gold.iter().map(|c| c.id.0.as_str()).collect();
    for c in candidates {
        if !gold_ids.contains(c.id.0.as_str()) {
            rest.push(c.clone());
        }
    }
    rest.sort_by(|a, b| a.id.0.cmp(&b.id.0));
    gold.extend(rest);
    gold
}

fn fused_rank_lookup(scored: &[(PrecursorCandidate, f64)]) -> BTreeMap<PrecursorId, f64> {
    scored
        .iter()
        .enumerate()
        .map(|(rank, (c, _))| (c.id.clone(), rank as f64))
        .collect()
}

struct CellAccumulator {
    recovered: usize,
    exhausted: usize,
    /// Conditional search conversion rate numerator/denominator at the
    /// pre-registered primary K=20: rows where gold is present within
    /// the candidate order's own top-20, and whether it was recovered.
    gold_in_top_k_count: usize,
    gold_in_top_k_and_recovered: usize,
    total: usize,
    /// `(target_formula, recovered)` per row -- needed for target-group
    /// bootstrap CIs and gained/lost analysis on the final holdout
    /// comparison. Populated for every cell (cheap: one bool per row),
    /// not just the ones that end up compared.
    per_row: Vec<(String, bool)>,
}

impl CellAccumulator {
    fn new() -> Self {
        Self {
            recovered: 0,
            exhausted: 0,
            gold_in_top_k_count: 0,
            gold_in_top_k_and_recovered: 0,
            total: 0,
            per_row: Vec::new(),
        }
    }
    fn recall(&self) -> f64 {
        self.recovered as f64 / self.total.max(1) as f64
    }
    fn exhaustion_rate(&self) -> f64 {
        self.exhausted as f64 / self.total.max(1) as f64
    }
    fn conditional_conversion_rate(&self) -> f64 {
        self.gold_in_top_k_and_recovered as f64 / self.gold_in_top_k_count.max(1) as f64
    }
}

const PRIMARY_CONVERSION_K: usize = 20;

/// The tight, calibrated budget PR 1/PR 2 both measured their own
/// catalog-exact/frequency-prior/thermodynamic-stability/ensemble
/// comparisons at. Policy **selection** (development split) always
/// compares different orders/fusion-rules/tie-breaks against each other
/// at this one fixed budget -- comparing across different budgets would
/// trivially favor whichever cell used the largest budget, which is not
/// a claim about candidate order/fusion/tie-break at all. The full
/// `BUDGETS` sweep is still run and reported for every named policy, for
/// context and the AUC-style summary below, just never used to *select*.
const PRIMARY_BUDGET: usize = 20;

type FusedRanksFor<'a> = dyn Fn(&ParsedRow, &RowGeneratorOutputs) -> BTreeMap<PrecursorId, f64> + 'a;

fn run_cell(
    rows: &[&ParsedRow],
    generator_outputs: &BTreeMap<String, RowGeneratorOutputs>,
    ordered_candidates_for: impl Fn(&ParsedRow, &RowGeneratorOutputs) -> Vec<PrecursorCandidate>,
    fused_ranks_for: Option<&FusedRanksFor<'_>>,
    tie_break_name: &str,
    budget_max_sets: usize,
) -> CellAccumulator {
    let mut acc = CellAccumulator::new();
    let budget = SearchBudget {
        max_precursor_sets: budget_max_sets,
        ..SearchBudget::default()
    };
    for row in rows {
        let outputs = &generator_outputs[&row.target_formula];
        let ordered = ordered_candidates_for(row, outputs);
        let gold: Vec<PrecursorId> = row.route.iter().cloned().map(PrecursorId).collect();

        let tie_break = match tie_break_name {
            TIE_BREAK_INDEX_ORDER => TieBreakPolicy::IndexOrder,
            TIE_BREAK_MARGINAL_COVERAGE => TieBreakPolicy::MarginalCoverage,
            TIE_BREAK_FUSION_PRIORITY_SUM => {
                let ranks = fused_ranks_for
                    .expect("fusion-priority-sum tie-break requires a fused-rank lookup")(
                    row, outputs,
                );
                TieBreakPolicy::FusionPrioritySum(ranks)
            }
            other => panic!("unknown tie-break {other}"),
        };

        let trace = search_precursor_sets_diagnostic(
            &row.target,
            &ordered,
            &PlanningConstraints::default(),
            &budget,
            &tie_break,
            &gold,
        )
        .expect("search_precursor_sets_diagnostic must not error on a well-formed row");

        acc.total += 1;
        acc.per_row
            .push((row.target_formula.clone(), trace.recovered));
        if trace.recovered {
            acc.recovered += 1;
        }
        if trace.budget_exhausted {
            acc.exhausted += 1;
        }
        let top_k: BTreeSet<&str> = ordered
            .iter()
            .take(PRIMARY_CONVERSION_K)
            .map(|c| c.id.0.as_str())
            .collect();
        let gold_in_top_k = row.route.iter().all(|f| top_k.contains(f.as_str()));
        if gold_in_top_k {
            acc.gold_in_top_k_count += 1;
            if trace.recovered {
                acc.gold_in_top_k_and_recovered += 1;
            }
        }
    }
    acc
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

    let oqmd_raw = std::fs::read_to_string(OQMD_MANIFEST_PATH)
        .unwrap_or_else(|e| panic!("could not read committed {OQMD_MANIFEST_PATH}: {e}"));
    let oqmd_manifest: OqmdManifest = serde_json::from_str(&oqmd_raw)
        .expect("benchmarks/data/oqmd_coverage_manifest.json must be valid JSON");
    let formation_energy = build_oqmd_formation_energy_table(&oqmd_manifest);
    let frequency = build_frequency_table(&rows);

    let dev_rows: Vec<&ParsedRow> = rows
        .iter()
        .filter(|r| r.split == Split::Development)
        .collect();
    let holdout_rows: Vec<&ParsedRow> = rows
        .iter()
        .filter(|r| r.split == Split::ConfirmationHoldout)
        .collect();
    let dev_targets: BTreeSet<&str> = dev_rows.iter().map(|r| r.target_formula.as_str()).collect();
    let holdout_targets: BTreeSet<&str> = holdout_rows
        .iter()
        .map(|r| r.target_formula.as_str())
        .collect();

    println!("Phase 30.5 fusion x search coupling audit");
    println!(
        "total rows: {} ({} skipped, unrepresentable)",
        rows.len(),
        skipped_unrepresentable
    );
    println!(
        "development split: {} rows / {} distinct targets",
        dev_rows.len(),
        dev_targets.len()
    );
    println!(
        "confirmation holdout split: {} rows / {} distinct targets",
        holdout_rows.len(),
        holdout_targets.len()
    );

    // Split manifest (committed, small).
    let split_manifest = format!(
        "{{\n  \"description\": \"Phase 30.5 dev/confirmation-holdout split -- deterministic, \
        target-level, sha256_hex(target_formula) first byte mod 5 (0..=3 development, 4 \
        confirmation holdout). Pre-registered in docs/exploration_fusion_search_coupling.md \
        before any factorial cell was run.\",\n  \"catalog_path\": {CATALOG_PATH:?},\n  \
        \"catalog_sha256\": {:?},\n  \"total_rows\": {},\n  \
        \"skipped_unrepresentable_rows\": {},\n  \"development\": {{\"rows\": {}, \"targets\": \
        {}}},\n  \"confirmation_holdout\": {{\"rows\": {}, \"targets\": {}}}\n}}\n",
        sha256_hex(&raw),
        rows.len(),
        skipped_unrepresentable,
        dev_rows.len(),
        dev_targets.len(),
        holdout_rows.len(),
        holdout_targets.len(),
    );
    std::fs::write(SPLIT_MANIFEST_PATH, &split_manifest).expect("failed to write split manifest");
    println!("wrote {SPLIT_MANIFEST_PATH}");

    // Materialize the 3 base generators' outputs once per row, keyed by
    // target_formula (unique enough for this lookup's purposes since
    // rows sharing a target_formula also share the exact same candidate
    // pool construction in this frozen catalog).
    let mut generator_outputs: BTreeMap<String, RowGeneratorOutputs> = BTreeMap::new();
    for row in &rows {
        generator_outputs
            .entry(row.target_formula.clone())
            .or_insert_with(|| generator_outputs_for(row, &frequency, &formation_energy));
    }

    let dev_sample: Vec<&ParsedRow> = dev_rows
        .iter()
        .copied()
        .step_by(DEV_FACTORIAL_STRIDE)
        .collect();
    println!(
        "development factorial explored on {} of {} dev rows (every {}th, stride sample -- \
        see docs/exploration_fusion_search_coupling.md)",
        dev_sample.len(),
        dev_rows.len(),
        DEV_FACTORIAL_STRIDE
    );

    // ---- Candidate-order-only sweep (fixed T1 tie-break) ----
    println!("\n== candidate-order sweep (T1 index-order tie-break) ==");
    let mut order_cells: Vec<(String, usize, CellAccumulator)> = Vec::new();
    let order_names: Vec<String> = {
        let mut names = vec![
            ORDER_CATALOG_EXACT.to_string(),
            ORDER_REVERSE.to_string(),
            ORDER_MIN_RANK_ENSEMBLE.to_string(),
            ORDER_ORACLE.to_string(),
        ];
        names.extend(SHUFFLE_SEEDS.iter().map(|s| s.to_string()));
        names
    };
    for order_name in &order_names {
        for &budget_max_sets in BUDGETS {
            let order_name_owned = order_name.clone();
            let acc = run_cell(
                &dev_sample,
                &generator_outputs,
                |row, outputs| candidate_order(&order_name_owned, row, outputs),
                None,
                TIE_BREAK_INDEX_ORDER,
                budget_max_sets,
            );
            println!(
                "  order={order_name} budget={budget_max_sets}: recall {:.4} exhaustion {:.4} \
                conv@{PRIMARY_CONVERSION_K} {:.4} (n={})",
                acc.recall(),
                acc.exhaustion_rate(),
                acc.conditional_conversion_rate(),
                acc.total
            );
            order_cells.push((order_name.clone(), budget_max_sets, acc));
        }
    }

    // ---- Fusion-rule sweep (fixed T1 tie-break) ----
    println!("\n== fusion-rule sweep (T1 index-order tie-break) ==");
    let mut fusion_cells: Vec<(String, usize, CellAccumulator)> = Vec::new();
    for &rule in FUSION_RULES {
        for &budget_max_sets in BUDGETS {
            let acc = run_cell(
                &dev_sample,
                &generator_outputs,
                |_row, outputs| {
                    let maps = rank_maps(outputs);
                    fuse(rule, &maps).into_iter().map(|(c, _)| c).collect()
                },
                None,
                TIE_BREAK_INDEX_ORDER,
                budget_max_sets,
            );
            println!(
                "  fusion={rule} budget={budget_max_sets}: recall {:.4} exhaustion {:.4} \
                conv@{PRIMARY_CONVERSION_K} {:.4} (n={})",
                acc.recall(),
                acc.exhaustion_rate(),
                acc.conditional_conversion_rate(),
                acc.total
            );
            fusion_cells.push((rule.to_string(), budget_max_sets, acc));
        }
    }

    // ---- Tie-break sweep (fixed candidate order: min-rank ensemble) ----
    println!("\n== tie-break sweep (fixed order: min-rank ensemble) ==");
    let mut tie_break_cells: Vec<(String, usize, CellAccumulator)> = Vec::new();
    for tie_break_name in [
        TIE_BREAK_INDEX_ORDER,
        TIE_BREAK_FUSION_PRIORITY_SUM,
        TIE_BREAK_MARGINAL_COVERAGE,
    ] {
        for &budget_max_sets in BUDGETS {
            let acc = run_cell(
                &dev_sample,
                &generator_outputs,
                |row, outputs| candidate_order(ORDER_MIN_RANK_ENSEMBLE, row, outputs),
                Some(&|_row, outputs| {
                    let maps = rank_maps(outputs);
                    fused_rank_lookup(&fuse_min_rank(&maps))
                }),
                tie_break_name,
                budget_max_sets,
            );
            println!(
                "  tie_break={tie_break_name} budget={budget_max_sets}: recall {:.4} \
                exhaustion {:.4} conv@{PRIMARY_CONVERSION_K} {:.4} (n={})",
                acc.recall(),
                acc.exhaustion_rate(),
                acc.conditional_conversion_rate(),
                acc.total
            );
            tie_break_cells.push((tie_break_name.to_string(), budget_max_sets, acc));
        }
    }

    // ---- Policy selection (development split, stride sample), ALWAYS at
    // PRIMARY_BUDGET. Comparing across different budgets would trivially
    // favor the largest budget every time and answer a different
    // question (does a bigger budget help -- yes, obviously) than the one
    // this phase is actually asking (does candidate order/fusion/tie-
    // break matter at a fixed, tight budget). Shuffle-*/oracle are
    // diagnostic controls, never eligible to be "the selected policy" --
    // a fixed random permutation isn't a deployable production strategy.
    let mut all_named_cells: Vec<(&str, String, usize, f64, f64)> = Vec::new();
    for (name, budget, acc) in &order_cells {
        all_named_cells.push((
            "order",
            name.clone(),
            *budget,
            acc.recall(),
            acc.exhaustion_rate(),
        ));
    }
    for (name, budget, acc) in &fusion_cells {
        all_named_cells.push((
            "fusion",
            name.clone(),
            *budget,
            acc.recall(),
            acc.exhaustion_rate(),
        ));
    }
    for (name, budget, acc) in &tie_break_cells {
        all_named_cells.push((
            "tie_break",
            name.clone(),
            *budget,
            acc.recall(),
            acc.exhaustion_rate(),
        ));
    }

    let catalog_exact_baseline_recall = order_cells
        .iter()
        .find(|(name, budget, _)| name == ORDER_CATALOG_EXACT && *budget == PRIMARY_BUDGET)
        .map(|(_, _, acc)| acc.recall())
        .unwrap_or(0.0);

    let is_selectable_policy = |sweep: &str, name: &str| -> bool {
        !(sweep == "order" && (name == ORDER_ORACLE || SHUFFLE_SEEDS.contains(&name)))
            && !(sweep == "order" && name == ORDER_CATALOG_EXACT)
    };
    let mut selectable_at_primary_budget: Vec<(&str, String, usize, f64, f64)> = all_named_cells
        .iter()
        .filter(|(sweep, name, budget, _, _)| {
            *budget == PRIMARY_BUDGET && is_selectable_policy(sweep, name)
        })
        .cloned()
        .collect();
    selectable_at_primary_budget.sort_by(|a, b| {
        b.3.total_cmp(&a.3)
            .then_with(|| a.4.total_cmp(&b.4))
            .then_with(|| a.0.cmp(b.0))
            .then_with(|| a.1.cmp(&b.1))
    });
    let best = selectable_at_primary_budget.first().cloned();

    // Shuffle-order distribution percentile: what fraction of the 10
    // fixed shuffle seeds does catalog-exact's own recall beat, at the
    // same primary budget -- distinguishes "catalog-exact's real order
    // is specially good" from "catalog-exact just happens to land
    // somewhere in the random-order distribution."
    let shuffle_recalls_at_primary: Vec<f64> = order_cells
        .iter()
        .filter(|(name, budget, _)| {
            *budget == PRIMARY_BUDGET && SHUFFLE_SEEDS.contains(&name.as_str())
        })
        .map(|(_, _, acc)| acc.recall())
        .collect();
    let catalog_exact_shuffle_percentile =
        percentile_rank(catalog_exact_baseline_recall, &shuffle_recalls_at_primary);

    // Budget-recall AUC proxy: mean recall across the pre-registered
    // BUDGETS list (equal-weighted across budget points, not a true
    // trapezoidal-area-under-curve integral -- a deliberately simple,
    // disclosed proxy for the decision gate's "recall-vs-budget AUC"
    // criterion).
    let auc_proxy = |sweep: &str, name: &str, cells: &[(String, usize, CellAccumulator)]| -> f64 {
        let _ = sweep;
        let recalls: Vec<f64> = cells
            .iter()
            .filter(|(n, _, _)| n == name)
            .map(|(_, _, acc)| acc.recall())
            .collect();
        recalls.iter().sum::<f64>() / recalls.len().max(1) as f64
    };
    let catalog_exact_auc = auc_proxy("order", ORDER_CATALOG_EXACT, &order_cells);

    println!("\n== development-split policy selection (fixed at budget={PRIMARY_BUDGET}) ==");
    println!(
        "catalog-exact baseline dev-sample recall: {:.4} (AUC proxy: {:.4})",
        catalog_exact_baseline_recall, catalog_exact_auc
    );
    println!(
        "catalog-exact's own recall beats {:.0}% of the 10 fixed shuffle-order seeds at this \
        budget (50% would mean catalog-exact looks like a typical random order)",
        catalog_exact_shuffle_percentile * 100.0
    );
    if let Some((sweep, name, budget, recall, exhaustion)) = &best {
        let selected_auc = match *sweep {
            "order" => auc_proxy("order", name, &order_cells),
            "fusion" => auc_proxy("fusion", name, &fusion_cells),
            "tie_break" => auc_proxy("tie_break", name, &tie_break_cells),
            _ => 0.0,
        };
        println!(
            "selected policy: sweep={sweep} name={name} budget={budget} dev-sample recall={:.4} \
            exhaustion={:.4} AUC proxy={:.4}",
            recall, exhaustion, selected_auc
        );
    } else {
        println!(
            "no selectable (non-shuffle, non-oracle, non-baseline) policy beat catalog-exact -- NO_GO"
        );
    }

    // ---- Confirmation holdout: catalog-exact baseline and the selected
    // policy, both run across the FULL BUDGETS sweep (for the AUC/
    // ceiling picture) and both compared at PRIMARY_BUDGET specifically
    // for the actual GO/NO-GO verdict. Full holdout split, never
    // sampled, run exactly once after the development-side choice above
    // was locked. ----
    println!("\n== confirmation holdout (full split, not sampled) ==");
    let mut holdout_catalog_exact_by_budget: Vec<(usize, CellAccumulator)> = Vec::new();
    for &budget_max_sets in BUDGETS {
        let acc = run_cell(
            &holdout_rows,
            &generator_outputs,
            |row, outputs| candidate_order(ORDER_CATALOG_EXACT, row, outputs),
            None,
            TIE_BREAK_INDEX_ORDER,
            budget_max_sets,
        );
        println!(
            "  catalog-exact budget={budget_max_sets}: recall {:.4} exhaustion {:.4} (n={})",
            acc.recall(),
            acc.exhaustion_rate(),
            acc.total
        );
        holdout_catalog_exact_by_budget.push((budget_max_sets, acc));
    }
    let holdout_catalog_exact_primary_idx = holdout_catalog_exact_by_budget
        .iter()
        .position(|(b, _)| *b == PRIMARY_BUDGET)
        .expect("PRIMARY_BUDGET must be one of BUDGETS");

    let holdout_selected_by_budget: Option<Vec<(usize, CellAccumulator)>> =
        best.as_ref().map(|(sweep, name, _, _, _)| {
            BUDGETS
                .iter()
                .map(|&budget_max_sets| {
                    let acc = match *sweep {
                        "order" => run_cell(
                            &holdout_rows,
                            &generator_outputs,
                            |row, outputs| candidate_order(name, row, outputs),
                            None,
                            TIE_BREAK_INDEX_ORDER,
                            budget_max_sets,
                        ),
                        "fusion" => run_cell(
                            &holdout_rows,
                            &generator_outputs,
                            |_row, outputs| {
                                let maps = rank_maps(outputs);
                                fuse(name, &maps).into_iter().map(|(c, _)| c).collect()
                            },
                            None,
                            TIE_BREAK_INDEX_ORDER,
                            budget_max_sets,
                        ),
                        "tie_break" => run_cell(
                            &holdout_rows,
                            &generator_outputs,
                            |row, outputs| candidate_order(ORDER_MIN_RANK_ENSEMBLE, row, outputs),
                            Some(&|_row, outputs| {
                                let maps = rank_maps(outputs);
                                fused_rank_lookup(&fuse_min_rank(&maps))
                            }),
                            name,
                            budget_max_sets,
                        ),
                        other => panic!("unknown sweep {other}"),
                    };
                    println!(
                        "  selected policy ({sweep}={name}) budget={budget_max_sets}: recall {:.4} \
                    exhaustion {:.4} (n={})",
                        acc.recall(),
                        acc.exhaustion_rate(),
                        acc.total
                    );
                    (budget_max_sets, acc)
                })
                .collect()
        });

    let holdout_catalog_exact_auc = holdout_catalog_exact_by_budget
        .iter()
        .map(|(_, a)| a.recall())
        .sum::<f64>()
        / holdout_catalog_exact_by_budget.len().max(1) as f64;
    let holdout_selected_auc = holdout_selected_by_budget.as_ref().map(|cells| {
        cells.iter().map(|(_, a)| a.recall()).sum::<f64>() / cells.len().max(1) as f64
    });

    let holdout_catalog_exact_primary =
        &holdout_catalog_exact_by_budget[holdout_catalog_exact_primary_idx].1;
    let holdout_selected_primary = holdout_selected_by_budget
        .as_ref()
        .map(|cells| &cells[holdout_catalog_exact_primary_idx].1);

    // Gained/lost + target-group bootstrap CI, at PRIMARY_BUDGET, on the
    // full holdout split -- both accumulators iterated `holdout_rows` in
    // the same fixed order, so a positional zip pairs the same real row
    // both times.
    let bootstrap = holdout_selected_primary.map(|selected| {
        let paired: Vec<(String, bool, bool)> = holdout_catalog_exact_primary
            .per_row
            .iter()
            .zip(selected.per_row.iter())
            .map(|((target, a), (_, b))| (target.clone(), *a, *b))
            .collect();
        bootstrap_recall_diff_ci(&paired, BOOTSTRAP_RESAMPLES)
    });
    let gained_lost = holdout_selected_primary.map(|selected| {
        let mut gained = 0usize;
        let mut lost = 0usize;
        for ((_, a), (_, b)) in holdout_catalog_exact_primary
            .per_row
            .iter()
            .zip(selected.per_row.iter())
        {
            match (a, b) {
                (false, true) => gained += 1,
                (true, false) => lost += 1,
                _ => {}
            }
        }
        (gained, lost)
    });

    println!(
        "\n== holdout statistical treatment (budget={PRIMARY_BUDGET}, target-group bootstrap, {BOOTSTRAP_RESAMPLES} resamples) =="
    );
    println!(
        "catalog-exact holdout AUC proxy: {:.4}; selected policy holdout AUC proxy: {}",
        holdout_catalog_exact_auc,
        holdout_selected_auc
            .map(|a| format!("{a:.4}"))
            .unwrap_or_else(|| "n/a".to_string())
    );
    if let (Some((observed_diff, ci_lower, ci_upper)), Some((gained, lost))) =
        (&bootstrap, &gained_lost)
    {
        println!(
            "recall diff (selected - catalog-exact): {:.4} [95% CI {:.4}, {:.4}] -- gained {gained} \
            / lost {lost} targets",
            observed_diff, ci_lower, ci_upper
        );
    }

    let verdict = match (&holdout_selected_primary, &bootstrap, holdout_selected_auc) {
        (Some(selected), Some((observed_diff, ci_lower, _ci_upper)), Some(selected_auc)) => {
            let beats_catalog_exact = selected.recall() > holdout_catalog_exact_primary.recall();
            let ci_not_substantially_negative = *ci_lower > -0.02;
            let auc_non_inferior = selected_auc >= holdout_catalog_exact_auc - 0.005;
            let diff_points = observed_diff * 100.0;
            if beats_catalog_exact && ci_not_substantially_negative && auc_non_inferior {
                if diff_points >= 1.0 && selected_auc >= holdout_catalog_exact_auc {
                    "STRONG_GO"
                } else {
                    "GO"
                }
            } else {
                "NO_GO"
            }
        }
        _ => "NO_GO",
    };
    println!("\nverdict: {verdict}");
    println!(
        "(GO/STRONG_GO is a recommendation to the owner, not an automatic production change -- \
        search_precursor_sets's own default tie-break/order handling is unchanged regardless of \
        this verdict.)"
    );

    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!(
        "  \"description\": \"Phase 30.5 candidate fusion x search coupling audit -- \
        development-split stride-sample factorial ({DEV_FACTORIAL_STRIDE}) for policy \
        selection at a fixed primary budget ({PRIMARY_BUDGET}), single confirmation-holdout run \
        (full split, all budgets) for the verdict. Shuffle-order seeds and the oracle order are \
        diagnostic controls only, never eligible for selection. See \
        docs/exploration_fusion_search_coupling.md for the full pre-registered methodology.\",\n"
    ));
    out.push_str(&format!("  \"catalog_path\": {CATALOG_PATH:?},\n"));
    out.push_str(&format!("  \"catalog_sha256\": {:?},\n", sha256_hex(&raw)));
    out.push_str(&format!("  \"primary_budget\": {PRIMARY_BUDGET},\n"));
    out.push_str(&format!("  \"total_rows\": {},\n", rows.len()));
    out.push_str(&format!(
        "  \"development_rows\": {}, \"development_dev_sample_rows\": {}, \
        \"confirmation_holdout_rows\": {},\n",
        dev_rows.len(),
        dev_sample.len(),
        holdout_rows.len()
    ));
    out.push_str(&format!(
        "  \"catalog_exact_dev_sample_recall_at_primary_budget\": {:.6}, \
        \"catalog_exact_shuffle_percentile\": {:.6}, \"catalog_exact_dev_sample_auc_proxy\": \
        {:.6},\n",
        catalog_exact_baseline_recall, catalog_exact_shuffle_percentile, catalog_exact_auc
    ));
    out.push_str(&format!(
        "  \"catalog_exact_holdout_recall_at_primary_budget\": {:.6}, \
        \"catalog_exact_holdout_exhaustion_rate_at_primary_budget\": {:.6}, \
        \"catalog_exact_holdout_auc_proxy\": {:.6},\n",
        holdout_catalog_exact_primary.recall(),
        holdout_catalog_exact_primary.exhaustion_rate(),
        holdout_catalog_exact_auc
    ));
    if let Some((sweep, name, _, dev_recall, dev_exhaustion)) = &best {
        out.push_str(&format!(
            "  \"selected_policy\": {{\"sweep\": {sweep:?}, \"name\": {name:?}, \"budget\": \
            {PRIMARY_BUDGET}, \"dev_sample_recall\": {:.6}, \"dev_sample_exhaustion_rate\": \
            {:.6}}},\n",
            dev_recall, dev_exhaustion
        ));
    }
    if let Some(acc) = &holdout_selected_primary {
        out.push_str(&format!(
            "  \"selected_policy_holdout_recall_at_primary_budget\": {:.6}, \
            \"selected_policy_holdout_exhaustion_rate_at_primary_budget\": {:.6}, \
            \"selected_policy_holdout_auc_proxy\": {},\n",
            acc.recall(),
            acc.exhaustion_rate(),
            holdout_selected_auc
                .map(|a| format!("{a:.6}"))
                .unwrap_or_else(|| "null".to_string())
        ));
    }
    if let (Some((observed_diff, ci_lower, ci_upper)), Some((gained, lost))) =
        (&bootstrap, &gained_lost)
    {
        out.push_str(&format!(
            "  \"holdout_recall_diff\": {:.6}, \"holdout_recall_diff_ci95_lower\": {:.6}, \
            \"holdout_recall_diff_ci95_upper\": {:.6}, \"gained_targets\": {gained}, \
            \"lost_targets\": {lost}, \"bootstrap_resamples\": {BOOTSTRAP_RESAMPLES},\n",
            observed_diff, ci_lower, ci_upper
        ));
    }
    out.push_str(&format!("  \"verdict\": {verdict:?},\n"));
    out.push_str("  \"development_sweep_cells\": [\n");
    let mut cell_entries: Vec<String> = Vec::new();
    for (sweep, name, budget, recall, exhaustion) in &all_named_cells {
        cell_entries.push(format!(
            "    {{\"sweep\": {sweep:?}, \"name\": {name:?}, \"budget\": {budget}, \"recall\": \
            {:.6}, \"exhaustion_rate\": {:.6}}}",
            recall, exhaustion
        ));
    }
    out.push_str(&cell_entries.join(",\n"));
    out.push_str("\n  ],\n");
    out.push_str("  \"holdout_catalog_exact_by_budget\": [\n");
    let mut ce_entries: Vec<String> = Vec::new();
    for (budget, acc) in &holdout_catalog_exact_by_budget {
        ce_entries.push(format!(
            "    {{\"budget\": {budget}, \"recall\": {:.6}, \"exhaustion_rate\": {:.6}}}",
            acc.recall(),
            acc.exhaustion_rate()
        ));
    }
    out.push_str(&ce_entries.join(",\n"));
    out.push_str("\n  ],\n");
    out.push_str("  \"holdout_selected_policy_by_budget\": [\n");
    if let Some(cells) = &holdout_selected_by_budget {
        let mut sel_entries: Vec<String> = Vec::new();
        for (budget, acc) in cells {
            sel_entries.push(format!(
                "    {{\"budget\": {budget}, \"recall\": {:.6}, \"exhaustion_rate\": {:.6}}}",
                acc.recall(),
                acc.exhaustion_rate()
            ));
        }
        out.push_str(&sel_entries.join(",\n"));
    }
    out.push_str("\n  ]\n");
    out.push_str("}\n");
    std::fs::write(RESULT_PATH, &out).expect("failed to write result");
    println!("\nwrote {RESULT_PATH}");
}

/// Fraction of `distribution` strictly less than `value` -- used to place
/// catalog-exact's own recall within the 10-shuffle-seed distribution
/// (0.5 would mean catalog-exact looks like a typical random order).
fn percentile_rank(value: f64, distribution: &[f64]) -> f64 {
    if distribution.is_empty() {
        return 0.5;
    }
    let below = distribution.iter().filter(|&&d| d < value).count();
    below as f64 / distribution.len() as f64
}

/// Target-group bootstrap: resamples whole target groups (not individual
/// rows) with replacement, `resamples` times, computing the recall
/// difference (selected - catalog-exact) each time. Returns
/// `(observed_diff, ci95_lower, ci95_upper)`. Deterministic xorshift64
/// PRNG, fixed seed -- reproducible, no `rand` dependency (matches this
/// script's own `sha256_hex`-based determinism convention for shuffled
/// candidate orders).
fn bootstrap_recall_diff_ci(paired: &[(String, bool, bool)], resamples: usize) -> (f64, f64, f64) {
    let mut groups: BTreeMap<&str, Vec<(bool, bool)>> = BTreeMap::new();
    for (target, catalog_exact_recovered, selected_recovered) in paired {
        groups
            .entry(target.as_str())
            .or_default()
            .push((*catalog_exact_recovered, *selected_recovered));
    }
    let group_list: Vec<&Vec<(bool, bool)>> = groups.values().collect();
    let n_groups = group_list.len();
    if n_groups == 0 {
        return (0.0, 0.0, 0.0);
    }

    let observed_diff = {
        let (a_sum, b_sum, n) = paired
            .iter()
            .fold((0usize, 0usize, 0usize), |(a, b, n), (_, ra, rb)| {
                (a + *ra as usize, b + *rb as usize, n + 1)
            });
        (b_sum as f64 - a_sum as f64) / n.max(1) as f64
    };

    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut next_index = |bound: usize| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state as usize) % bound
    };

    let mut diffs: Vec<f64> = Vec::with_capacity(resamples);
    for _ in 0..resamples {
        let mut a_sum = 0usize;
        let mut b_sum = 0usize;
        let mut n = 0usize;
        for _ in 0..n_groups {
            let group = group_list[next_index(n_groups)];
            for (ra, rb) in group {
                a_sum += *ra as usize;
                b_sum += *rb as usize;
                n += 1;
            }
        }
        diffs.push((b_sum as f64 - a_sum as f64) / n.max(1) as f64);
    }
    diffs.sort_by(|a, b| a.total_cmp(b));
    let lower_idx = ((0.025 * resamples as f64) as usize).min(resamples.saturating_sub(1));
    let upper_idx = ((0.975 * resamples as f64) as usize).min(resamples.saturating_sub(1));
    (observed_diff, diffs[lower_idx], diffs[upper_idx])
}

/// One row's candidate list under a named ordering policy.
fn candidate_order(
    name: &str,
    row: &ParsedRow,
    outputs: &RowGeneratorOutputs,
) -> Vec<PrecursorCandidate> {
    match name {
        ORDER_CATALOG_EXACT => outputs
            .catalog_exact
            .iter()
            .map(|gc| gc.candidate.clone())
            .collect(),
        ORDER_REVERSE => {
            let mut v: Vec<PrecursorCandidate> = outputs
                .catalog_exact
                .iter()
                .map(|gc| gc.candidate.clone())
                .collect();
            v.reverse();
            v
        }
        ORDER_MIN_RANK_ENSEMBLE => {
            let maps = rank_maps(outputs);
            fuse_min_rank(&maps).into_iter().map(|(c, _)| c).collect()
        }
        ORDER_ORACLE => oracle_order(&row.route, &row.candidates),
        seed if SHUFFLE_SEEDS.contains(&seed) => shuffled_order(seed, &row.candidates),
        other => panic!("unknown candidate order {other}"),
    }
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
