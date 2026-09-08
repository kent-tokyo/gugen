#![cfg(feature = "serde")]

use gugen::{DiversePortfolioSelector, PortfolioCandidate};
use serde::Deserialize;
use std::collections::BTreeSet;

#[derive(Deserialize)]
struct Fixture {
    fixture_version: String,
    max_size: usize,
    relevance_weight: f64,
    diversity_weight: f64,
    required_roles: Vec<String>,
    selector_provenance: String,
    candidates: Vec<CandidateRow>,
    expected_selected_ids: Vec<String>,
    expected_uncovered_roles: Vec<String>,
    minimum_mean_pairwise_l1: f64,
}

#[derive(Deserialize)]
struct CandidateRow {
    id: String,
    relevance: f64,
    features: std::collections::BTreeMap<String, f64>,
    roles: Vec<String>,
}

fn load_fixture() -> (Fixture, Vec<PortfolioCandidate>) {
    let fixture: Fixture =
        serde_json::from_str(include_str!("fixtures/portfolio_selection_v1.json")).unwrap();
    let candidates = fixture
        .candidates
        .iter()
        .map(|row| {
            PortfolioCandidate::new(
                row.id.clone(),
                row.relevance,
                row.features.clone(),
                row.roles.clone(),
            )
            .unwrap()
        })
        .collect();
    (fixture, candidates)
}

#[test]
fn pinned_local_contract_checks_selection_diversity_and_role_coverage() {
    let (fixture, candidates) = load_fixture();
    assert_eq!(fixture.fixture_version, "phase38-portfolio-v1");
    let selector = DiversePortfolioSelector::new(
        fixture.max_size,
        fixture.relevance_weight,
        fixture.diversity_weight,
        fixture.required_roles.clone(),
        fixture.selector_provenance.clone(),
    )
    .unwrap();
    let selection = selector.select(&candidates).unwrap();
    let selected_ids: Vec<&str> = selection
        .selected
        .iter()
        .map(|candidate| candidate.candidate_id.as_str())
        .collect();
    let expected_ids: Vec<&str> = fixture
        .expected_selected_ids
        .iter()
        .map(String::as_str)
        .collect();
    assert_eq!(selected_ids, expected_ids);
    assert_eq!(
        selection.audit.uncovered_roles,
        fixture
            .expected_uncovered_roles
            .into_iter()
            .collect::<BTreeSet<_>>()
    );
    assert_eq!(
        selection.audit.selector_provenance,
        fixture.selector_provenance
    );

    let mut pairwise_sum = 0.0;
    let mut pairwise_count = 0;
    for (index, left) in selection.selected.iter().enumerate() {
        for right in selection.selected.iter().skip(index + 1) {
            pairwise_sum += left
                .diversity_features
                .iter()
                .map(|(name, value)| (value - right.diversity_features[name]).abs())
                .sum::<f64>();
            pairwise_count += 1;
        }
    }
    let mean_pairwise_l1 = pairwise_sum / pairwise_count as f64;
    assert!(mean_pairwise_l1 >= fixture.minimum_mean_pairwise_l1);
}

#[test]
fn pinned_local_contract_is_invariant_to_input_permutation() {
    let (fixture, mut candidates) = load_fixture();
    let selector = DiversePortfolioSelector::new(
        fixture.max_size,
        fixture.relevance_weight,
        fixture.diversity_weight,
        fixture.required_roles,
        fixture.selector_provenance,
    )
    .unwrap();
    let first = selector.select(&candidates).unwrap();
    candidates.reverse();
    let second = selector.select(&candidates).unwrap();
    assert_eq!(first, second);
}
