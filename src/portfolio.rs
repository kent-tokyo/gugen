//! Phase 38: a standalone, caller-supplied diverse portfolio boundary.
//!
//! This is a deterministic engineering primitive, not a route selector or a
//! learned recommendation model. Callers provide relevance scores, finite
//! diversity features, optional role labels, and fixed selector weights.

use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// A candidate that can be placed in a diverse portfolio.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PortfolioCandidate {
    pub candidate_id: String,
    pub relevance_score: f64,
    pub diversity_features: BTreeMap<String, f64>,
    pub roles: BTreeSet<String>,
}

impl PortfolioCandidate {
    pub fn new(
        candidate_id: String,
        relevance_score: f64,
        diversity_features: impl IntoIterator<Item = (String, f64)>,
        roles: impl IntoIterator<Item = String>,
    ) -> Result<Self, PortfolioError> {
        if candidate_id.trim().is_empty() {
            return Err(PortfolioError::BlankCandidateId);
        }
        if !relevance_score.is_finite() {
            return Err(PortfolioError::NonFiniteRelevance);
        }
        let mut features = BTreeMap::new();
        for (name, value) in diversity_features {
            if name.trim().is_empty() {
                return Err(PortfolioError::BlankFeatureName);
            }
            if !value.is_finite() {
                return Err(PortfolioError::NonFiniteFeature(name));
            }
            if features.insert(name.clone(), value).is_some() {
                return Err(PortfolioError::DuplicateFeatureName(name));
            }
        }
        if features.is_empty() {
            return Err(PortfolioError::EmptyDiversityFeatures);
        }
        let roles = roles
            .into_iter()
            .map(|role| role.trim().to_owned())
            .filter(|role| !role.is_empty())
            .collect();
        Ok(Self {
            candidate_id,
            relevance_score,
            diversity_features: features,
            roles,
        })
    }
}

/// Fixed weights for the deterministic greedy portfolio rule.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DiversePortfolioSelector {
    pub max_size: usize,
    pub relevance_weight: f64,
    pub diversity_weight: f64,
    pub required_roles: BTreeSet<String>,
    pub selector_provenance: String,
}

impl DiversePortfolioSelector {
    pub fn new(
        max_size: usize,
        relevance_weight: f64,
        diversity_weight: f64,
        required_roles: impl IntoIterator<Item = String>,
        selector_provenance: String,
    ) -> Result<Self, PortfolioError> {
        if max_size == 0 {
            return Err(PortfolioError::ZeroMaxSize);
        }
        if !relevance_weight.is_finite()
            || !diversity_weight.is_finite()
            || relevance_weight < 0.0
            || diversity_weight < 0.0
            || (relevance_weight == 0.0 && diversity_weight == 0.0)
        {
            return Err(PortfolioError::InvalidWeights);
        }
        if selector_provenance.trim().is_empty() {
            return Err(PortfolioError::BlankSelectorProvenance);
        }
        let required_roles = required_roles
            .into_iter()
            .map(|role| role.trim().to_owned())
            .filter(|role| !role.is_empty())
            .collect();
        Ok(Self {
            max_size,
            relevance_weight,
            diversity_weight,
            required_roles,
            selector_provenance,
        })
    }

    /// Selects at most `max_size` candidates using fixed marginal diversity.
    ///
    /// The first item is selected by relevance. Each later item maximizes
    /// `relevance_weight * relevance + diversity_weight * minimum_L1_distance`
    /// to the selected set. Candidate IDs break exact ties. Role labels are
    /// reported for coverage auditing, but never silently override the score.
    pub fn select(
        &self,
        candidates: &[PortfolioCandidate],
    ) -> Result<PortfolioSelection, PortfolioError> {
        validate_candidates(candidates)?;
        if candidates.is_empty() {
            return Ok(PortfolioSelection {
                selected: Vec::new(),
                audit: PortfolioAudit {
                    candidate_count: 0,
                    selected_count: 0,
                    covered_roles: BTreeSet::new(),
                    uncovered_roles: self.required_roles.clone(),
                    selection_scores: BTreeMap::new(),
                    selector_provenance: self.selector_provenance.clone(),
                },
            });
        }
        let feature_names = candidates[0]
            .diversity_features
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for candidate in candidates.iter().skip(1) {
            if candidate
                .diversity_features
                .keys()
                .cloned()
                .collect::<Vec<_>>()
                != feature_names
            {
                return Err(PortfolioError::FeatureSchemaMismatch {
                    candidate_id: candidate.candidate_id.clone(),
                });
            }
        }

        let target_size = self.max_size.min(candidates.len());
        let mut remaining = candidates.to_vec();
        let mut selected = Vec::with_capacity(target_size);
        let mut selection_scores = BTreeMap::new();
        while selected.len() < target_size {
            let best_index = remaining
                .iter()
                .enumerate()
                .map(|(index, candidate)| {
                    let marginal_diversity = if selected.is_empty() {
                        0.0
                    } else {
                        selected
                            .iter()
                            .map(|chosen: &PortfolioCandidate| {
                                l1_distance(
                                    &candidate.diversity_features,
                                    &chosen.diversity_features,
                                )
                            })
                            .fold(f64::INFINITY, f64::min)
                    };
                    let score = self.relevance_weight * candidate.relevance_score
                        + self.diversity_weight * marginal_diversity;
                    (index, score, candidate.candidate_id.clone())
                })
                .max_by(|left, right| {
                    left.1
                        .total_cmp(&right.1)
                        .then_with(|| right.2.cmp(&left.2))
                })
                .expect("remaining is non-empty while target_size has not been reached");
            let chosen = remaining.swap_remove(best_index.0);
            selection_scores.insert(chosen.candidate_id.clone(), best_index.1);
            selected.push(chosen);
        }

        let covered_roles = selected
            .iter()
            .flat_map(|candidate| candidate.roles.iter().cloned())
            .collect::<BTreeSet<_>>();
        let uncovered_roles = self
            .required_roles
            .difference(&covered_roles)
            .cloned()
            .collect();
        Ok(PortfolioSelection {
            selected,
            audit: PortfolioAudit {
                candidate_count: candidates.len(),
                selected_count: target_size,
                covered_roles,
                uncovered_roles,
                selection_scores,
                selector_provenance: self.selector_provenance.clone(),
            },
        })
    }
}

/// The selected portfolio and the evidence needed to audit the selection.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PortfolioSelection {
    pub selected: Vec<PortfolioCandidate>,
    pub audit: PortfolioAudit,
}

/// Deterministic selection and role-coverage evidence.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PortfolioAudit {
    pub candidate_count: usize,
    pub selected_count: usize,
    pub covered_roles: BTreeSet<String>,
    pub uncovered_roles: BTreeSet<String>,
    pub selection_scores: BTreeMap<String, f64>,
    pub selector_provenance: String,
}

fn validate_candidates(candidates: &[PortfolioCandidate]) -> Result<(), PortfolioError> {
    let mut ids = BTreeSet::new();
    for candidate in candidates {
        if !ids.insert(candidate.candidate_id.clone()) {
            return Err(PortfolioError::DuplicateCandidateId(
                candidate.candidate_id.clone(),
            ));
        }
    }
    Ok(())
}

fn l1_distance(left: &BTreeMap<String, f64>, right: &BTreeMap<String, f64>) -> f64 {
    left.iter()
        .map(|(name, value)| (value - right[name]).abs())
        .sum()
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PortfolioError {
    #[error("candidate id must not be blank")]
    BlankCandidateId,
    #[error("candidate ids must be unique: {0}")]
    DuplicateCandidateId(String),
    #[error("candidate relevance score is not finite")]
    NonFiniteRelevance,
    #[error("diversity feature name must not be blank")]
    BlankFeatureName,
    #[error("diversity feature name is duplicated: {0}")]
    DuplicateFeatureName(String),
    #[error("candidate must contain at least one diversity feature")]
    EmptyDiversityFeatures,
    #[error("candidate diversity feature schema differs: {candidate_id}")]
    FeatureSchemaMismatch { candidate_id: String },
    #[error("diversity feature {0} is not finite")]
    NonFiniteFeature(String),
    #[error("portfolio max_size must be positive")]
    ZeroMaxSize,
    #[error("portfolio weights must be finite, non-negative, and not both zero")]
    InvalidWeights,
    #[error("selector provenance must not be blank")]
    BlankSelectorProvenance,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, relevance: f64, x: f64, role: &str) -> PortfolioCandidate {
        PortfolioCandidate::new(id.into(), relevance, [("x".into(), x)], [role.into()]).unwrap()
    }

    fn selector() -> DiversePortfolioSelector {
        DiversePortfolioSelector::new(
            2,
            1.0,
            1.0,
            ["oxide".into(), "salt".into()],
            "phase38-fixed-v1".into(),
        )
        .unwrap()
    }

    #[test]
    fn selects_relevant_then_diverse_candidates_deterministically() {
        let selection = selector()
            .select(&[
                candidate("near", 0.9, 0.1, "oxide"),
                candidate("best", 1.0, 0.0, "oxide"),
                candidate("far", 0.7, 10.0, "salt"),
            ])
            .unwrap();
        assert_eq!(
            selection
                .selected
                .iter()
                .map(|candidate| candidate.candidate_id.as_str())
                .collect::<Vec<_>>(),
            ["best", "far"]
        );
        assert_eq!(selection.audit.uncovered_roles, BTreeSet::new());
        assert_eq!(selection.audit.selector_provenance, "phase38-fixed-v1");
    }

    #[test]
    fn reports_uncovered_roles_and_is_input_order_invariant() {
        let first = selector()
            .select(&[
                candidate("b", 1.0, 0.0, "oxide"),
                candidate("a", 1.0, 0.0, "oxide"),
            ])
            .unwrap();
        let second = selector()
            .select(&[
                candidate("a", 1.0, 0.0, "oxide"),
                candidate("b", 1.0, 0.0, "oxide"),
            ])
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.audit.uncovered_roles, BTreeSet::from(["salt".into()]));
    }

    #[test]
    fn rejects_duplicate_ids_and_mismatched_feature_schema() {
        assert!(matches!(
            selector().select(&[
                candidate("a", 1.0, 0.0, "oxide"),
                candidate("a", 0.9, 1.0, "salt"),
            ]),
            Err(PortfolioError::DuplicateCandidateId(_))
        ));
        let mismatched =
            PortfolioCandidate::new("b".into(), 0.9, [("y".into(), 1.0)], ["salt".into()]).unwrap();
        assert!(matches!(
            selector().select(&[candidate("a", 1.0, 0.0, "oxide"), mismatched]),
            Err(PortfolioError::FeatureSchemaMismatch { .. })
        ));
    }
}
