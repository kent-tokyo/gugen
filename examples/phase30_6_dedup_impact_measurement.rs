//! Phase 30.6 impact measurement: for a sample of real corpus rows, how
//! many accepted plans does `search_precursor_sets` return, and how many
//! rows have more than one accepted entry? Run once against the
//! pre-fix code (git stash) and once against the fixed code to measure
//! the real-world effect of `CanonicalReactionKey`-based dedup.
//!
//! Throwaway diagnostic, not part of any locked methodology.

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
    target_elements: BTreeMap<String, f64>,
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

fn main() {
    let raw = std::fs::read_to_string(CATALOG_PATH)
        .unwrap_or_else(|e| panic!("could not read {CATALOG_PATH}: {e}"));
    let catalog: FrozenCatalog = serde_json::from_str(&raw).expect("manifest must be valid JSON");

    let budget = SearchBudget {
        max_precursor_sets: 200_000,
        ..SearchBudget::default()
    };

    let sample: Vec<&CatalogRow> = catalog.rows.iter().step_by(3).collect();

    let mut total_accepted = 0u64;
    let mut rows_with_multiple_accepted = 0u64;
    let mut max_accepted_in_one_row = 0usize;
    let mut total_rows = 0u64;
    let mut skipped = 0u64;

    for row in &sample {
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

        let outcome = search_precursor_sets(
            &target,
            &candidates,
            &PlanningConstraints::default(),
            &budget,
        )
        .expect("search must not error");

        total_rows += 1;
        total_accepted += outcome.accepted.len() as u64;
        if outcome.accepted.len() > 1 {
            rows_with_multiple_accepted += 1;
        }
        max_accepted_in_one_row = max_accepted_in_one_row.max(outcome.accepted.len());
    }

    println!("sample rows: {total_rows} (skipped {skipped})");
    println!("total accepted entries across sample: {total_accepted}");
    println!("rows with >1 accepted entry: {rows_with_multiple_accepted}");
    println!("max accepted entries in a single row: {max_accepted_in_one_row}");
    println!(
        "mean accepted entries per row: {:.4}",
        total_accepted as f64 / total_rows.max(1) as f64
    );
}
