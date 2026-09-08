//! Phase 36: deterministic composition-only material retrieval.
//!
//! This is retrieval context, not a property predictor, success probability,
//! or route recommender. Callers provide the records and decide how to use
//! the returned distances. The intentionally narrow first slice uses a
//! normalized composition vector and fixed L1 distance; it has no model,
//! structure inference, online index, ANN dependency, or hidden corpus.

use crate::Element;
use crate::composition::Composition;
use std::collections::BTreeMap;
use thiserror::Error;

/// A normalized, deterministic composition fingerprint.
#[derive(Debug, Clone, PartialEq)]
pub struct CompositionFingerprint {
    fractions: BTreeMap<Element, f64>,
}

impl CompositionFingerprint {
    pub fn from_composition(composition: &Composition) -> Self {
        let total: f64 = composition.iter().map(|(_, amount)| amount).sum();
        let fractions = composition
            .iter()
            .map(|(element, amount)| (element, amount / total))
            .collect();
        Self { fractions }
    }

    pub fn fractions(&self) -> impl Iterator<Item = (Element, f64)> + '_ {
        self.fractions
            .iter()
            .map(|(&element, &fraction)| (element, fraction))
    }

    fn l1_distance(&self, other: &Self) -> f64 {
        let mut left = self.fractions.iter();
        let mut right = other.fractions.iter();
        let mut left_item = left.next();
        let mut right_item = right.next();
        let mut distance = 0.0;

        loop {
            match (left_item, right_item) {
                (Some((left_element, left_fraction)), Some((right_element, right_fraction))) => {
                    if left_element == right_element {
                        distance += (left_fraction - right_fraction).abs();
                        left_item = left.next();
                        right_item = right.next();
                    } else if left_element < right_element {
                        distance += *left_fraction;
                        left_item = left.next();
                    } else {
                        distance += *right_fraction;
                        right_item = right.next();
                    }
                }
                (Some((_, fraction)), None) | (None, Some((_, fraction))) => {
                    distance += *fraction;
                    if left_item.is_some() {
                        left_item = left.next();
                    } else {
                        right_item = right.next();
                    }
                }
                (None, None) => return distance,
            }
        }
    }
}

/// A caller-supplied material eligible for composition retrieval.
#[derive(Debug, Clone, PartialEq)]
pub struct SimilarityMaterial {
    pub id: String,
    pub composition: Composition,
}

/// One deterministic retrieval result. A lower distance means closer
/// composition; it is not an experimental-success estimate.
#[derive(Debug, Clone, PartialEq)]
pub struct SimilarityMatch {
    pub id: String,
    pub distance: f64,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SimilarityError {
    #[error("similarity index must contain at least one material")]
    EmptyIndex,
    #[error("similarity material id must not be blank")]
    BlankMaterialId,
    #[error("similarity material id is duplicated: {0}")]
    DuplicateMaterialId(String),
}

/// An order-independent, caller-supplied composition retrieval index.
#[derive(Debug, Clone, PartialEq)]
pub struct CompositionSimilarityIndex {
    materials: Vec<(String, CompositionFingerprint)>,
}

impl CompositionSimilarityIndex {
    pub fn new(materials: Vec<SimilarityMaterial>) -> Result<Self, SimilarityError> {
        if materials.is_empty() {
            return Err(SimilarityError::EmptyIndex);
        }
        let mut indexed = Vec::with_capacity(materials.len());
        for material in materials {
            if material.id.trim().is_empty() {
                return Err(SimilarityError::BlankMaterialId);
            }
            indexed.push((
                material.id,
                CompositionFingerprint::from_composition(&material.composition),
            ));
        }
        indexed.sort_by(|left, right| left.0.cmp(&right.0));
        for pair in indexed.windows(2) {
            if pair[0].0 == pair[1].0 {
                return Err(SimilarityError::DuplicateMaterialId(pair[0].0.clone()));
            }
        }
        Ok(Self { materials: indexed })
    }

    pub fn len(&self) -> usize {
        self.materials.len()
    }

    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }

    /// Returns at most `k` results, ordered by distance then material id.
    pub fn top_k(&self, target: &Composition, k: usize) -> Vec<SimilarityMatch> {
        if k == 0 {
            return Vec::new();
        }
        let target = CompositionFingerprint::from_composition(target);
        let mut matches: Vec<_> = self
            .materials
            .iter()
            .map(|(id, fingerprint)| SimilarityMatch {
                id: id.clone(),
                distance: fingerprint.l1_distance(&target),
            })
            .collect();
        matches.sort_by(|left, right| {
            left.distance
                .total_cmp(&right.distance)
                .then_with(|| left.id.cmp(&right.id))
        });
        matches.truncate(k);
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Element;

    fn composition(pairs: &[(&str, f64)]) -> Composition {
        Composition::new(
            pairs
                .iter()
                .map(|(symbol, amount)| (Element::new(symbol).unwrap(), *amount)),
        )
        .unwrap()
    }

    fn material(id: &str, pairs: &[(&str, f64)]) -> SimilarityMaterial {
        SimilarityMaterial {
            id: id.to_string(),
            composition: composition(pairs),
        }
    }

    #[test]
    fn normalized_fingerprint_is_scale_invariant() {
        let left =
            CompositionFingerprint::from_composition(&composition(&[("Ba", 1.0), ("O", 3.0)]));
        let right =
            CompositionFingerprint::from_composition(&composition(&[("Ba", 2.0), ("O", 6.0)]));
        assert_eq!(left, right);
        assert_eq!(left.fractions().count(), 2);
    }

    #[test]
    fn top_k_is_deterministic_and_ties_break_by_id() {
        let index = CompositionSimilarityIndex::new(vec![
            material("zeta", &[("Ba", 1.0), ("O", 3.0)]),
            material("alpha", &[("Ba", 1.0), ("O", 3.0)]),
            material("far", &[("Fe", 1.0)]),
        ])
        .unwrap();
        let target = composition(&[("Ba", 1.0), ("O", 3.0)]);
        let matches = index.top_k(&target, 2);
        assert_eq!(
            matches
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["alpha", "zeta"]
        );
        assert_eq!(matches[0].distance, 0.0);
    }

    #[test]
    fn input_order_does_not_change_results_and_k_is_capped() {
        let target = composition(&[("Li", 1.0), ("Fe", 1.0), ("P", 1.0), ("O", 4.0)]);
        let first = CompositionSimilarityIndex::new(vec![
            material("b", &[("Fe", 1.0)]),
            material("a", &[("Li", 1.0), ("Fe", 1.0), ("P", 1.0), ("O", 4.0)]),
        ])
        .unwrap();
        let second = CompositionSimilarityIndex::new(vec![
            material("a", &[("Li", 1.0), ("Fe", 1.0), ("P", 1.0), ("O", 4.0)]),
            material("b", &[("Fe", 1.0)]),
        ])
        .unwrap();
        assert_eq!(first.top_k(&target, 99), second.top_k(&target, 99));
        assert!(first.top_k(&target, 0).is_empty());
    }

    #[test]
    fn rejects_empty_and_duplicate_indexes() {
        assert_eq!(
            CompositionSimilarityIndex::new(Vec::new()),
            Err(SimilarityError::EmptyIndex)
        );
        assert_eq!(
            CompositionSimilarityIndex::new(vec![
                material("same", &[("Fe", 1.0)]),
                material("same", &[("O", 1.0)]),
            ]),
            Err(SimilarityError::DuplicateMaterialId("same".into()))
        );
    }
}
