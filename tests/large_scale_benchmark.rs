//! Phase 11: cheap, in-`cargo test` checks against
//! `benchmarks/data/kononova_sample.jsonl` -- the full metric computation
//! lives in `examples/large_scale_benchmark.rs` (run separately, not part
//! of the test suite, mirroring `examples/benchmark_report.rs`'s own
//! split). This file's job is breadth and the anti-leakage guarantee:
//! every row plans without panicking, and zero rows overlap the routes
//! `tests/validation.rs`/`src/literature_conditions.rs` already use --
//! not full metric correctness (already covered at small scale by
//! `tests/validation.rs`, `tests/literature_conditions.rs`,
//! `tests/metamorphic.rs`).

use gugen::{
    Composition, Element, InMemoryPrecursorCatalog, Planner, PlanningConfig, PlanningConstraints,
    PrecursorCandidate, PrecursorId, TargetSpecification,
};
use std::collections::BTreeMap;

const CORPUS_JSONL: &str = include_str!("../benchmarks/data/kononova_sample.jsonl");
const EXPECTED_ROW_COUNT: usize = 1500;

#[derive(serde::Deserialize)]
struct CorpusPrecursor {
    formula: String,
    elements: BTreeMap<String, f64>,
}

#[derive(serde::Deserialize)]
struct CorpusRow {
    target_elements: BTreeMap<String, f64>,
    precursors: Vec<CorpusPrecursor>,
}

fn load_corpus() -> Vec<CorpusRow> {
    CORPUS_JSONL
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("kononova_sample.jsonl must be valid JSONL"))
        .collect()
}

fn try_composition(elements: &BTreeMap<String, f64>) -> Option<Composition> {
    let pairs: Option<Vec<(Element, f64)>> = elements
        .iter()
        .map(|(sym, amt)| Element::new(sym).ok().map(|e| (e, *amt)))
        .collect();
    Composition::new(pairs?).ok()
}

/// Composition equality is exact-scale, not ratio-normalized (a
/// documented gap, ROADMAP.md), so a route reported at a different
/// formula-unit scale than gugen's fixtures wouldn't match here even
/// though `benchmarks/fetch_kononova.py`'s ratio-based exclusion (the
/// authoritative filter) already caught it. This lower-fidelity Rust-side
/// check exists as a second, independent guard against the common case
/// (same scale), not to replace the Python filter's more thorough one.
fn composition_scale_exact(a: &Composition, b: &Composition) -> bool {
    a == b
}

/// The 6 routes excluded by `benchmarks/fetch_kononova.py` when
/// `benchmarks/data/kononova_sample.jsonl` was generated (Phase 11) --
/// same target/precursor definitions as `tests/validation.rs` and
/// `src/literature_conditions.rs` *at that time*, not retyped from memory.
/// A fixed snapshot of the exclusion set the checked-in corpus file was
/// actually built with, not a live mirror kept in sync with
/// `tests/validation.rs`'s current fixtures -- Phase 14 replaced
/// `tests/validation.rs`'s Zn3(PO4)2/ZnO+P2O5 fixture with a different
/// target (LiFePO4/FePO4+Li2CO3) without regenerating this corpus, so
/// that new route is *not* in this list or in `fetch_kononova.py`'s own
/// `EXCLUDED_ROUTES`. Checked directly (not assumed) that this specific
/// gap is currently harmless: the committed `kononova_sample.jsonl` has
/// zero rows matching the exact FePO4 + Li2CO3 -> LiFePO4 route today.
/// Update both this list and `fetch_kononova.py`'s before the next time
/// that corpus is regenerated, to keep the holdout genuinely leak-free
/// against gugen's then-current fixture set.
fn excluded_routes() -> Vec<(Composition, Vec<&'static str>)> {
    fn c(pairs: &[(&str, f64)]) -> Composition {
        Composition::new(pairs.iter().map(|&(s, a)| (Element::new(s).unwrap(), a))).unwrap()
    }
    vec![
        (
            c(&[("La", 1.0), ("Al", 1.0), ("O", 3.0)]),
            vec!["La2O3", "Al2O3"],
        ),
        (
            c(&[("Mg", 1.0), ("Al", 2.0), ("O", 4.0)]),
            vec!["MgO", "Al2O3"],
        ),
        (
            c(&[("Zn", 3.0), ("P", 2.0), ("O", 8.0)]),
            vec!["ZnO", "P2O5"],
        ),
        (c(&[("Ca", 1.0), ("O", 1.0)]), vec!["CaCO3"]),
        (
            c(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
            vec!["BaCO3", "TiO2"],
        ),
        (
            c(&[("Zn", 3.0), ("P", 2.0), ("O", 8.0)]),
            vec!["ZnO", "(NH4)2HPO4"],
        ),
    ]
}

#[test]
fn corpus_loads_at_the_expected_size() {
    let raw = load_corpus();
    assert_eq!(
        raw.len(),
        EXPECTED_ROW_COUNT,
        "benchmarks/data/kononova_sample.jsonl's row count changed -- re-run \
        benchmarks/fetch_kononova.py or update EXPECTED_ROW_COUNT deliberately, not silently"
    );
}

#[test]
fn no_holdout_row_exactly_matches_an_already_curated_route() {
    let raw = load_corpus();
    let excluded = excluded_routes();
    let mut leaked = Vec::new();
    for row in &raw {
        let Some(target) = try_composition(&row.target_elements) else {
            continue;
        };
        let precursor_ids: std::collections::BTreeSet<&str> =
            row.precursors.iter().map(|p| p.formula.as_str()).collect();
        for (excluded_target, excluded_precursors) in &excluded {
            if composition_scale_exact(&target, excluded_target)
                && precursor_ids == excluded_precursors.iter().copied().collect()
            {
                leaked.push(precursor_ids.clone());
            }
        }
    }
    assert!(
        leaked.is_empty(),
        "holdout corpus contains a row identical to an already-curated route: {leaked:?}"
    );
}

#[test]
fn every_row_plans_without_panicking_or_erroring() {
    let raw = load_corpus();
    let mut unparseable = 0;
    let mut planned = 0;
    for row in &raw {
        let Some(target) = try_composition(&row.target_elements) else {
            unparseable += 1;
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
            unparseable += 1;
            continue;
        }
        let catalog = InMemoryPrecursorCatalog::new(precursors);
        let target_spec = TargetSpecification {
            composition: target,
            structure: None,
            desired_phase: None,
            constraints: PlanningConstraints::default(),
        };
        let result = Planner::builder(catalog, PlanningConfig::default())
            .build()
            .plan(&target_spec, "2026-08-14T00:00:00Z");
        assert!(
            result.is_ok(),
            "planning must never return Err for a pre-validated target/precursor set: {result:?}"
        );
        planned += 1;
    }
    assert!(
        planned > 0,
        "no row planned successfully -- corpus or parsing is broken"
    );
    // Rust-side parseability is a defensive re-check, not the primary
    // filter (benchmarks/fetch_kononova.py's Python-side filter is) --
    // expected near-zero, not necessarily exactly zero. Measured against
    // rows actually loaded, not the expected-count constant, so this
    // guard's threshold tracks the real data rather than a second,
    // independently-changeable number.
    assert!(
        unparseable * 20 < raw.len(),
        "more than 5% of rows were unparseable by gugen's own types ({unparseable}/{}) \
        -- investigate whether fetch_kononova.py's filter has a real gap",
        raw.len()
    );
}
