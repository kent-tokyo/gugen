//! Phase 31 PR 3: measures whether adding hand-written, conservative
//! transformation grammars (`src/transformation_grammar.rs`) as an
//! intermediate-candidate source improves `search_two_step_routes`'s net-new
//! recall over the corpus-grounded `FrequencyPriorGenerator` baseline
//! established in Phase 31 PR 2 -- not "does adding many rules help", but
//! "does adding this specific chemistry information help, measured
//! honestly against a held-out split".
//!
//! **Split discipline**: `benchmarks/data/exploration_grammar_split_manifest.json`
//! was generated and committed (see `benchmarks/build_grammar_audit_split.py`)
//! *before* any grammar rule in `src/transformation_grammar.rs` was written.
//! The four grammars were designed by reasoning about general chemical
//! signatures (carbonate/hydroxide/nitrate ratios, one real acid+carbonate
//! case already on record from PR 2's own hand-trace -- see that module's
//! doc comment and the split manifest's `known_pre_split_contamination`
//! field for the one row this does not cleanly hold for). Individual
//! evaluation-side rows were not inspected while writing the grammars.
//!
//! **Four policies, same candidate cap (200) for a fair comparison** --
//! an earlier measurement (PR 2) found net-new recall highly sensitive to
//! the intermediate-candidate cap, so every policy here shares one ceiling
//! rather than comparing a capped policy to an uncapped one:
//! - `OneStepBaseline`: zero intermediates (sanity floor; net-new must be 0).
//! - `FrequencyOnly`: PR 2's `FrequencyPriorGenerator`, capped at 200.
//! - `GrammarOnly`: `transformation_grammar::propose_all` over the row's own
//!   real precursors, per-grammar cap 50, combined cap 200.
//! - `Union`: `FrequencyOnly` union `GrammarOnly`, deduplicated, capped at
//!   200 total (so grammar proposals can be crowded out by frequency ones
//!   at the shared ceiling -- reported honestly if it happens).
//!
//! Run: `cargo run --release --example exploration_grammar_audit --features serde`
//! Writes `benchmarks/data/exploration_grammar_audit_result.json`.

use gugen::{
    CandidateGenerator, Composition, Element, FrequencyPriorGenerator, PlanningConstraints,
    PrecursorCandidate, PrecursorId, SearchBudget, TransformationGrammar, default_grammars,
    propose_all, search_two_step_routes,
};
use std::collections::BTreeMap;

const LOW_ARITY_JSONL: &str = include_str!("../benchmarks/data/kononova_sample.jsonl");
const HIGH_ARITY_JSONL: &str = include_str!("../benchmarks/data/kononova_high_arity_sample.jsonl");
const SPLIT_MANIFEST_JSON: &str =
    include_str!("../benchmarks/data/exploration_grammar_split_manifest.json");
const OUTPUT_PATH: &str = "benchmarks/data/exploration_grammar_audit_result.json";

/// Shared ceiling across every policy -- see module doc for why a fixed,
/// equal cap matters for this comparison to mean anything.
const CANDIDATE_CAP: usize = 200;
const PER_GRAMMAR_CAP: usize = 50;

#[derive(serde::Deserialize)]
struct CorpusPrecursor {
    formula: String,
    elements: BTreeMap<String, f64>,
}

#[derive(serde::Deserialize)]
struct CorpusRow {
    doi: Option<String>,
    target_formula: Option<String>,
    target_elements: BTreeMap<String, f64>,
    precursors: Vec<CorpusPrecursor>,
}

#[derive(serde::Deserialize)]
struct SplitSide {
    row_indices: Vec<usize>,
}

#[derive(serde::Deserialize)]
struct SplitManifest {
    development: SplitSide,
    evaluation: SplitSide,
}

fn load_jsonl(raw: &str, source_hint: &str) -> Vec<CorpusRow> {
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str(l)
                .unwrap_or_else(|e| panic!("{source_hint} must be valid JSONL: {e}"))
        })
        .collect()
}

fn try_composition(elements: &BTreeMap<String, f64>) -> Option<Composition> {
    let pairs: Option<Vec<(Element, f64)>> = elements
        .iter()
        .map(|(sym, amt)| Element::new(sym).ok().map(|e| (e, *amt)))
        .collect();
    Composition::new(pairs?).ok()
}

struct ParsedRow {
    doi: String,
    target_formula: String,
    target: Composition,
    precursors: Vec<PrecursorCandidate>,
}

fn parse_rows(raw: Vec<CorpusRow>) -> Vec<ParsedRow> {
    let mut parsed = Vec::with_capacity(raw.len());
    for row in raw {
        let Some(target) = try_composition(&row.target_elements) else {
            continue;
        };
        let mut precursors = Vec::new();
        let mut ok = true;
        for p in &row.precursors {
            let Some(composition) = try_composition(&p.elements) else {
                ok = false;
                break;
            };
            precursors.push(PrecursorCandidate {
                id: PrecursorId(p.formula.clone()),
                composition,
                availability: None,
            });
        }
        if !ok || precursors.is_empty() {
            continue;
        }
        parsed.push(ParsedRow {
            doi: row.doi.unwrap_or_default(),
            target_formula: row.target_formula.unwrap_or_default(),
            target,
            precursors,
        });
    }
    parsed
}

fn build_frequency_table(rows: &[ParsedRow]) -> Vec<(PrecursorCandidate, u64)> {
    let mut counts: BTreeMap<String, (u64, Composition)> = BTreeMap::new();
    for row in rows {
        for p in &row.precursors {
            counts
                .entry(p.id.0.clone())
                .and_modify(|(n, _)| *n += 1)
                .or_insert((1, p.composition.clone()));
        }
    }
    counts
        .into_iter()
        .map(|(formula, (count, composition))| {
            (
                PrecursorCandidate {
                    id: PrecursorId(formula),
                    composition,
                    availability: None,
                },
                count,
            )
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Policy {
    OneStepBaseline,
    FrequencyOnly,
    GrammarOnly,
    Union,
}

impl Policy {
    fn label(&self) -> &'static str {
        match self {
            Policy::OneStepBaseline => "one_step_baseline",
            Policy::FrequencyOnly => "frequency_only",
            Policy::GrammarOnly => "grammar_only",
            Policy::Union => "union",
        }
    }
}

fn frequency_candidates(
    generator: &FrequencyPriorGenerator,
    target: &Composition,
    base_compositions: &[Composition],
) -> Vec<Composition> {
    let target_arity = target.iter().count();
    generator
        .generate(target, &PlanningConstraints::default())
        .expect("frequency-prior generation must not fail on a well-formed target")
        .into_iter()
        .map(|gc| gc.candidate.composition)
        .filter(|c| c.iter().count() < target_arity)
        .filter(|c| !base_compositions.contains(c))
        .collect()
}

fn grammar_candidates(
    grammars: &[Box<dyn TransformationGrammar>],
    target: &Composition,
    row_precursor_compositions: &[Composition],
    base_compositions: &[Composition],
) -> Vec<Composition> {
    let target_arity = target.iter().count();
    propose_all(
        grammars,
        row_precursor_compositions,
        PER_GRAMMAR_CAP,
        CANDIDATE_CAP,
    )
    .into_iter()
    .map(|d| d.composition)
    .filter(|c| c.iter().count() < target_arity)
    .filter(|c| !base_compositions.contains(c))
    .collect()
}

fn intermediates_for(
    policy: Policy,
    generator: &FrequencyPriorGenerator,
    grammars: &[Box<dyn TransformationGrammar>],
    row: &ParsedRow,
) -> Vec<Composition> {
    let base_compositions: Vec<Composition> = row
        .precursors
        .iter()
        .map(|c| c.composition.clone())
        .collect();
    match policy {
        Policy::OneStepBaseline => Vec::new(),
        Policy::FrequencyOnly => {
            let mut v = frequency_candidates(generator, &row.target, &base_compositions);
            v.truncate(CANDIDATE_CAP);
            v
        }
        Policy::GrammarOnly => grammar_candidates(
            grammars,
            &row.target,
            &base_compositions,
            &base_compositions,
        ),
        Policy::Union => {
            let mut freq = frequency_candidates(generator, &row.target, &base_compositions);
            let grammar = grammar_candidates(
                grammars,
                &row.target,
                &base_compositions,
                &base_compositions,
            );
            for c in grammar {
                if !freq.contains(&c) {
                    freq.push(c);
                }
            }
            freq.truncate(CANDIDATE_CAP);
            freq
        }
    }
}

struct SplitMetrics {
    split_name: &'static str,
    policy: Policy,
    evaluated: usize,
    search_errors: usize,
    one_step_recovered: usize,
    truly_unreachable: usize,
    two_step_found_any_route: usize,
    two_step_recovered_net_new: usize,
    by_arity: BTreeMap<usize, (usize, usize)>,
    net_new_rows: Vec<(String, String, usize)>, // (doi, target_formula, arity)
}

fn run_split(
    split_name: &'static str,
    policy: Policy,
    holdout: &[ParsedRow],
    row_indices: &[usize],
    generator: &FrequencyPriorGenerator,
    grammars: &[Box<dyn TransformationGrammar>],
) -> SplitMetrics {
    let budget = SearchBudget::default();
    let mut evaluated = 0usize;
    let mut search_errors = 0usize;
    let mut one_step_recovered = 0usize;
    let mut two_step_found_any_route = 0usize;
    let mut two_step_recovered_net_new = 0usize;
    let mut by_arity: BTreeMap<usize, (usize, usize)> = BTreeMap::new();
    let mut net_new_rows = Vec::new();

    for &idx in row_indices {
        let Some(row) = holdout.get(idx) else {
            continue;
        };
        let intermediates = intermediates_for(policy, generator, grammars, row);
        let routes = match search_two_step_routes(
            &row.target,
            &row.precursors,
            &intermediates,
            &PlanningConstraints::default(),
            &budget,
        ) {
            Ok(routes) => routes,
            Err(_) => {
                search_errors += 1;
                continue;
            }
        };
        evaluated += 1;

        let two_step_hit = routes.iter().any(|r| {
            r.stages().len() >= 2
                && r.final_reaction()
                    .products()
                    .iter()
                    .any(|s| s.composition == row.target)
        });
        let one_step_hit = routes.iter().any(|r| {
            r.stages().len() == 1
                && r.final_reaction()
                    .products()
                    .iter()
                    .any(|s| s.composition == row.target)
        });

        if one_step_hit {
            one_step_recovered += 1;
        }
        if two_step_hit {
            two_step_found_any_route += 1;
        }
        if two_step_hit && !one_step_hit {
            two_step_recovered_net_new += 1;
            net_new_rows.push((
                row.doi.clone(),
                row.target_formula.clone(),
                row.precursors.len(),
            ));
        }
        if !one_step_hit {
            let arity = row.precursors.len();
            let entry = by_arity.entry(arity).or_insert((0, 0));
            entry.0 += 1;
            if two_step_hit {
                entry.1 += 1;
            }
        }
    }

    let truly_unreachable = evaluated - one_step_recovered;
    SplitMetrics {
        split_name,
        policy,
        evaluated,
        search_errors,
        one_step_recovered,
        truly_unreachable,
        two_step_found_any_route,
        two_step_recovered_net_new,
        by_arity,
        net_new_rows,
    }
}

fn print_metrics(m: &SplitMetrics) {
    let recall = m.two_step_recovered_net_new as f64 / m.truly_unreachable.max(1) as f64;
    println!(
        "[{}] {}: evaluated={} ({} search errors) one_step_recovered={} truly_unreachable={} \
        two_step_any={} net_new={} recall={:.4}",
        m.split_name,
        m.policy.label(),
        m.evaluated,
        m.search_errors,
        m.one_step_recovered,
        m.truly_unreachable,
        m.two_step_found_any_route,
        m.two_step_recovered_net_new,
        recall
    );
}

fn metrics_json(m: &SplitMetrics) -> String {
    let recall = m.two_step_recovered_net_new as f64 / m.truly_unreachable.max(1) as f64;
    let by_arity: Vec<String> = m
        .by_arity
        .iter()
        .map(|(arity, (total, recovered))| {
            format!("\"{arity}\": {{\"total\": {total}, \"recovered\": {recovered}}}")
        })
        .collect();
    let net_new: Vec<String> = m
        .net_new_rows
        .iter()
        .map(|(doi, formula, arity)| {
            format!("{{\"doi\": {doi:?}, \"target_formula\": {formula:?}, \"arity\": {arity}}}")
        })
        .collect();
    format!(
        "{{\n  \"split\": {:?},\n  \"policy\": {:?},\n  \"evaluated\": {},\n  \
        \"search_errors\": {},\n  \"one_step_recovered\": {},\n  \"truly_unreachable\": {},\n  \
        \"two_step_found_any_route\": {},\n  \"two_step_recovered_net_new\": {},\n  \
        \"recall_net_new_of_truly_unreachable\": {:.6},\n  \"by_arity\": {{{}}},\n  \
        \"net_new_rows\": [{}]\n}}",
        m.split_name,
        m.policy.label(),
        m.evaluated,
        m.search_errors,
        m.one_step_recovered,
        m.truly_unreachable,
        m.two_step_found_any_route,
        m.two_step_recovered_net_new,
        recall,
        by_arity.join(", "),
        net_new.join(", ")
    )
}

/// For the mandatory manual audit: every `GrammarOnly`-policy row where a
/// 2-stage route was found, with the actual intermediate composition used
/// and every grammar (by id) whose individual `propose()` output contains
/// that exact composition -- not `propose_all`'s deduplicated view, so
/// attribution reflects the real per-grammar source, not the ensemble's
/// merged evidence class. Capped at `per_grammar_sample_cap` examples per
/// grammar so the printed sample stays small enough to actually read by
/// hand (this is not a benchmark figure, just an audit trail).
struct AuditSample {
    split_name: &'static str,
    doi: String,
    target_formula: String,
    precursor_formulas: Vec<String>,
    intermediate: Composition,
    net_new: bool,
    contributing_grammars: Vec<&'static str>,
}

fn collect_audit_samples(
    holdout: &[ParsedRow],
    splits: &[(&'static str, &[usize])],
    grammars: &[Box<dyn TransformationGrammar>],
    per_grammar_sample_cap: usize,
) -> Vec<AuditSample> {
    let budget = SearchBudget::default();
    let mut by_grammar_count: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut samples = Vec::new();

    for &(split_name, row_indices) in splits {
        for &idx in row_indices {
            let Some(row) = holdout.get(idx) else {
                continue;
            };
            let base_compositions: Vec<Composition> = row
                .precursors
                .iter()
                .map(|c| c.composition.clone())
                .collect();
            let intermediates = grammar_candidates(
                grammars,
                &row.target,
                &base_compositions,
                &base_compositions,
            );
            if intermediates.is_empty() {
                continue;
            }
            let routes = match search_two_step_routes(
                &row.target,
                &row.precursors,
                &intermediates,
                &PlanningConstraints::default(),
                &budget,
            ) {
                Ok(routes) => routes,
                Err(_) => continue,
            };
            let one_step_hit = routes.iter().any(|r| {
                r.stages().len() == 1
                    && r.final_reaction()
                        .products()
                        .iter()
                        .any(|s| s.composition == row.target)
            });
            let Some(route) = routes.iter().find(|r| {
                r.stages().len() >= 2
                    && r.final_reaction()
                        .products()
                        .iter()
                        .any(|s| s.composition == row.target)
            }) else {
                continue;
            };
            let stage0_products: Vec<Composition> = route.stages()[0]
                .products()
                .iter()
                .map(|s| s.composition.clone())
                .collect();
            let stage1_reactants: Vec<Composition> = route.stages()[1]
                .reactants()
                .iter()
                .map(|s| s.composition.clone())
                .collect();
            let Some(intermediate) = stage0_products
                .into_iter()
                .find(|c| stage1_reactants.contains(c))
            else {
                continue;
            };

            let mut contributing = Vec::new();
            for g in grammars {
                if g.propose(&base_compositions)
                    .iter()
                    .any(|p| p.composition == intermediate)
                {
                    contributing.push(g.id().0);
                }
            }
            if contributing.is_empty() {
                continue; // e.g. frequency-sourced only; not this pass's concern
            }
            if contributing
                .iter()
                .all(|g| *by_grammar_count.get(g).unwrap_or(&0) >= per_grammar_sample_cap)
            {
                continue;
            }
            for g in &contributing {
                *by_grammar_count.entry(g).or_insert(0) += 1;
            }
            samples.push(AuditSample {
                split_name,
                doi: row.doi.clone(),
                target_formula: row.target_formula.clone(),
                precursor_formulas: row.precursors.iter().map(|p| p.id.0.clone()).collect(),
                intermediate,
                net_new: !one_step_hit,
                contributing_grammars: contributing,
            });
        }
    }
    samples
}

fn main() {
    let low_arity = parse_rows(load_jsonl(LOW_ARITY_JSONL, "kononova_sample.jsonl"));
    let holdout = parse_rows(load_jsonl(
        HIGH_ARITY_JSONL,
        "kononova_high_arity_sample.jsonl",
    ));
    let manifest: SplitManifest = serde_json::from_str(SPLIT_MANIFEST_JSON)
        .expect("exploration_grammar_split_manifest.json must be valid JSON");

    let frequency_table = build_frequency_table(&low_arity);
    let generator = FrequencyPriorGenerator::new(frequency_table);
    let grammars = default_grammars();

    let splits: [(&'static str, &[usize]); 2] = [
        ("development", &manifest.development.row_indices),
        ("evaluation", &manifest.evaluation.row_indices),
    ];
    let policies = [
        Policy::OneStepBaseline,
        Policy::FrequencyOnly,
        Policy::GrammarOnly,
        Policy::Union,
    ];

    println!("Phase 31 PR 3: grammar-vs-frequency net-new two-step recall, dev/eval split");
    println!(
        "candidate cap: {CANDIDATE_CAP} (shared across every policy); per-grammar cap: {PER_GRAMMAR_CAP}"
    );

    let mut all_metrics = Vec::new();
    for (split_name, row_indices) in splits {
        for policy in policies {
            let m = run_split(
                split_name,
                policy,
                &holdout,
                row_indices,
                &generator,
                &grammars,
            );
            print_metrics(&m);
            all_metrics.push(m);
        }
    }

    println!();
    println!("manual audit samples (>=1 per grammar where available, capped at 3 each):");
    let audit_samples = collect_audit_samples(&holdout, &splits, &grammars, 3);
    for s in &audit_samples {
        println!(
            "  [{}] {:?} <- precursors {:?}; target={:?} arity={}; net_new={}; grammars={:?}",
            s.split_name,
            s.intermediate,
            s.precursor_formulas,
            s.target_formula,
            s.precursor_formulas.len(),
            s.net_new,
            s.contributing_grammars
        );
        println!("      doi: {}", s.doi);
    }
    if audit_samples.is_empty() {
        println!("  (none -- no grammar-sourced intermediate contributed to any 2-stage route)");
    }

    let body = all_metrics
        .iter()
        .map(metrics_json)
        .collect::<Vec<_>>()
        .join(",\n");
    let audit_json: Vec<String> = audit_samples
        .iter()
        .map(|s| {
            format!(
                "{{\"split\": {:?}, \"doi\": {:?}, \"target_formula\": {:?}, \
                \"precursor_formulas\": {:?}, \"intermediate\": {:?}, \"net_new\": {}, \
                \"contributing_grammars\": {:?}}}",
                s.split_name,
                s.doi,
                s.target_formula,
                s.precursor_formulas,
                format!("{:?}", s.intermediate),
                s.net_new,
                s.contributing_grammars
            )
        })
        .collect();
    let out = format!(
        "{{\n  \"description\": \"Phase 31 PR 3 -- net-new two-step recall by policy \
        (one_step_baseline/frequency_only/grammar_only/union) on the deterministic dev/eval \
        split of kononova_high_arity_sample.jsonl. Every policy shares candidate_cap={CANDIDATE_CAP} \
        for a fair comparison. one_step_baseline's net_new is expected to be 0 by construction \
        (sanity floor). audit_samples is a manual-review aid (per-grammar attribution of the \
        GrammarOnly policy's own 2-stage routes, capped at 3 examples per grammar), not a \
        benchmark figure.\",\n  \"candidate_cap\": {CANDIDATE_CAP},\n  \"per_grammar_cap\": {PER_GRAMMAR_CAP},\n  \"results\": [\n{body}\n  ],\n  \"audit_samples\": [\n{}\n  ]\n}}\n",
        audit_json.join(",\n")
    );
    std::fs::write(OUTPUT_PATH, &out).expect("failed to write result");
    println!("wrote {OUTPUT_PATH}");
}
