#![cfg(feature = "serde")]

use gugen::{Composition, CompositionSimilarityIndex, SimilarityMaterial};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Deserialize)]
struct Fixture {
    fixture_version: String,
    reference_materials: Vec<MaterialRow>,
    holdout_queries: Vec<QueryRow>,
}

#[derive(Deserialize)]
struct MaterialRow {
    id: String,
    group: String,
    composition: Composition,
}

#[derive(Deserialize)]
struct QueryRow {
    id: String,
    group: String,
    composition: Composition,
    relevant_ids: Vec<String>,
}

#[test]
fn pinned_group_holdout_reports_recall_at_k_and_mrr_without_leakage() {
    let fixture: Fixture =
        serde_json::from_str(include_str!("fixtures/similarity_retrieval_v1.json")).unwrap();
    assert_eq!(fixture.fixture_version, "phase36-similarity-v1");

    let reference_groups: BTreeSet<&str> = fixture
        .reference_materials
        .iter()
        .map(|row| row.group.as_str())
        .collect();
    let index = CompositionSimilarityIndex::new(
        fixture
            .reference_materials
            .iter()
            .map(|row| SimilarityMaterial {
                id: row.id.clone(),
                composition: row.composition.clone(),
            })
            .collect(),
    )
    .unwrap();

    let mut recall_at_1 = 0.0;
    let mut reciprocal_rank = 0.0;
    for query in &fixture.holdout_queries {
        assert!(!reference_groups.contains(query.group.as_str()));
        assert!(
            fixture
                .reference_materials
                .iter()
                .all(|row| row.id != query.id)
        );
        let relevant: BTreeSet<&str> = query.relevant_ids.iter().map(String::as_str).collect();
        let results = index.top_k(&query.composition, 2);
        let first_relevant = results
            .iter()
            .position(|result| relevant.contains(result.id.as_str()));
        if first_relevant == Some(0) {
            recall_at_1 += 1.0;
        }
        if let Some(rank) = first_relevant {
            reciprocal_rank += 1.0 / (rank + 1) as f64;
        }
    }

    let query_count = fixture.holdout_queries.len() as f64;
    assert_eq!(recall_at_1 / query_count, 1.0);
    assert_eq!(reciprocal_rank / query_count, 1.0);
}
