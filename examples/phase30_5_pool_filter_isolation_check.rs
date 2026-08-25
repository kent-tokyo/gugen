//! Phase 30.5 order-invariance investigation, corrected isolation check.
//!
//! The committed `benchmarks/data/exploration_fusion_search_audit_result.json`
//! shows, at budget=100,000 (exhaustion_rate=0 everywhere): every
//! fusion-sweep policy AND every order-sweep policy built from
//! `outputs.*` (catalog-exact, reverse, min-rank-ensemble) converges to
//! the exact same recall, 0.471264 -- while `oracle` and all 10
//! `shuffle-*` policies (built directly from `row.candidates`, bypassing
//! `InMemoryPrecursorCatalog::candidates_for`'s element-overlap filter)
//! sit at 0.510-0.515. This is the real signature of a FILTERED-vs-
//! UNFILTERED pool split, not a pure order effect.
//!
//! This file's earlier version tested filtering and ordering separately
//! and found ~0 effect from each in isolation -- but that version never
//! tested the actual real combination `outputs.catalog_exact` uses
//! (filter, THEN sort ascending by `PrecursorId`, via
//! `InMemoryPrecursorCatalog::new`). This version reproduces that real
//! combination directly, on an arbitrary deterministic ~80% stride
//! sample of the *full* corpus (this file's own FNV-1a scheme, NOT the
//! main harness's real `sha256_hex`-based dev/holdout split -- see
//! `is_arbitrary_80_percent_sample`'s own doc comment), to settle the
//! discrepancy empirically before writing up any conclusion. This ended
//! up finding the true root cause (a `target_formula`-keyed cache bug in
//! the main harness, not filtering or ordering) -- see
//! `docs/exploration_fusion_search_coupling.md`'s Correction section for
//! the full story; this file remains as the throwaway diagnostic that
//! found it, not as part of the locked methodology.

use gugen::{
    Composition, Element, PlanningConstraints, PrecursorCandidate, PrecursorId, SearchBudget,
    search_precursor_sets,
};
use std::collections::BTreeMap;

const CATALOG_PATH: &str = "benchmarks/data/exploration_frozen_catalog_manifest.json";

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

fn try_composition(elements: &BTreeMap<String, f64>) -> Option<Composition> {
    let pairs: Option<Vec<(Element, f64)>> = elements
        .iter()
        .map(|(sym, amt)| Element::new(sym).ok().map(|e| (e, *amt)))
        .collect();
    Composition::new(pairs?).ok()
}

fn fnv1a_hex(input: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn shuffled_order(seed: &str, candidates: &[PrecursorCandidate]) -> Vec<PrecursorCandidate> {
    let mut keyed: Vec<(u64, PrecursorCandidate)> = candidates
        .iter()
        .map(|c| (fnv1a_hex(&format!("{seed}:{}", c.id.0)), c.clone()))
        .collect();
    keyed.sort_by_key(|(k, _)| *k);
    keyed.into_iter().map(|(_, c)| c).collect()
}

/// Real `outputs.catalog_exact` equivalent: filter by element overlap
/// with target (`InMemoryPrecursorCatalog::candidates_for`), THEN sort
/// ascending by `PrecursorId` (`InMemoryPrecursorCatalog::new`) -- the
/// two steps combined, in the real order the production code applies
/// them, not in isolation.
fn catalog_exact_equivalent_order(
    target: &Composition,
    candidates: &[PrecursorCandidate],
) -> Vec<PrecursorCandidate> {
    let target_elements: std::collections::BTreeSet<Element> = target.elements().collect();
    let mut filtered: Vec<PrecursorCandidate> = candidates
        .iter()
        .filter(|c| {
            c.composition
                .elements()
                .any(|e| target_elements.contains(&e))
        })
        .cloned()
        .collect();
    filtered.sort_by(|a, b| a.id.0.cmp(&b.id.0));
    filtered
}

/// NOT the real dev/holdout split (`split_for` in
/// `exploration_fusion_search_coupling_audit.rs` uses `sha256_hex` mod 5;
/// this uses this file's own FNV-1a) -- an arbitrary, deterministic ~80%
/// stride sample of the corpus, sufficient for this file's own purpose
/// (isolating filtering/ordering effects on a real sample), but not
/// interchangeable with the main harness's actual split for any
/// dev/holdout-membership question.
fn is_arbitrary_80_percent_sample(target_formula: &str) -> bool {
    fnv1a_hex(target_formula) % 5 != 4
}

fn main() {
    let raw = std::fs::read_to_string(CATALOG_PATH)
        .unwrap_or_else(|e| panic!("could not read {CATALOG_PATH}: {e}"));
    let catalog: FrozenCatalog = serde_json::from_str(&raw).expect("manifest must be valid JSON");

    let budget = SearchBudget {
        max_precursor_sets: 200_000,
        ..SearchBudget::default()
    };

    let sampled_rows: Vec<&CatalogRow> = catalog
        .rows
        .iter()
        .filter(|r| is_arbitrary_80_percent_sample(&r.target_formula))
        .collect();
    let sample: Vec<&&CatalogRow> = sampled_rows.iter().step_by(5).collect();

    let policies = [
        "catalog-exact-equivalent",
        "on-disk-order-unfiltered",
        "shuffle-1-unfiltered",
    ];
    let mut total = 0u64;
    let mut recovered: BTreeMap<&str, u64> = policies.iter().map(|&p| (p, 0)).collect();
    let mut skipped = 0u64;

    for row in sample {
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
        let Some(on_disk_order) = candidates else {
            skipped += 1;
            continue;
        };

        let orderings: [(&str, Vec<PrecursorCandidate>); 3] = [
            (
                "catalog-exact-equivalent",
                catalog_exact_equivalent_order(&target, &on_disk_order),
            ),
            ("on-disk-order-unfiltered", on_disk_order.clone()),
            (
                "shuffle-1-unfiltered",
                shuffled_order("shuffle-1", &on_disk_order),
            ),
        ];

        let gold: std::collections::BTreeSet<&str> = row.route.iter().map(|s| s.as_str()).collect();

        total += 1;
        for (name, ordered) in &orderings {
            let outcome =
                search_precursor_sets(&target, ordered, &PlanningConstraints::default(), &budget)
                    .expect("search must not error");
            let is_recovered = outcome.accepted.iter().any(|a| {
                a.precursors.len() == gold.len()
                    && a.precursors.iter().all(|id| gold.contains(id.0.as_str()))
            });
            if is_recovered {
                *recovered.get_mut(name).unwrap() += 1;
            }
        }
    }

    println!("dev-ish sample rows: {total} (skipped {skipped})");
    for name in policies {
        println!(
            "recall [{name}]: {:.6} ({}/{})",
            recovered[name] as f64 / total as f64,
            recovered[name],
            total
        );
    }
}
