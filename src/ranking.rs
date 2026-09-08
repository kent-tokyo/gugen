//! Phase 37: an auditable, caller-supplied ranking boundary.
//!
//! The first slice is deliberately not a learned model. Callers provide
//! named finite features, fixed finite weights, and provenance for the scorer.
//! The result is an ordering aid only: it is not a probability, an execution
//! outcome, a route selector, or a replacement for chemical/safety gates.

use std::collections::BTreeMap;
use thiserror::Error;

/// A candidate's caller-supplied, named feature values.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CandidateFeatureVector {
    pub candidate_id: String,
    pub features: BTreeMap<String, f64>,
}

impl CandidateFeatureVector {
    pub fn new(
        candidate_id: String,
        features: impl IntoIterator<Item = (String, f64)>,
    ) -> Result<Self, RankingError> {
        if candidate_id.trim().is_empty() {
            return Err(RankingError::BlankCandidateId);
        }
        let mut values = BTreeMap::new();
        for (name, value) in features {
            if name.trim().is_empty() {
                return Err(RankingError::BlankFeatureName);
            }
            if !value.is_finite() {
                return Err(RankingError::NonFiniteFeature(name));
            }
            if values.insert(name.clone(), value).is_some() {
                return Err(RankingError::DuplicateFeatureName(name));
            }
        }
        Ok(Self {
            candidate_id,
            features: values,
        })
    }
}

/// A fixed linear scorer with caller-supplied weights and provenance.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FixedLinearScorer {
    pub scorer_id: String,
    pub weights: BTreeMap<String, f64>,
    pub bias: f64,
    pub model_provenance: String,
}

impl FixedLinearScorer {
    pub fn new(
        scorer_id: String,
        weights: impl IntoIterator<Item = (String, f64)>,
        bias: f64,
        model_provenance: String,
    ) -> Result<Self, RankingError> {
        if scorer_id.trim().is_empty() {
            return Err(RankingError::BlankScorerId);
        }
        if model_provenance.trim().is_empty() {
            return Err(RankingError::BlankModelProvenance);
        }
        if !bias.is_finite() {
            return Err(RankingError::NonFiniteWeight("<bias>".into()));
        }
        let mut fixed_weights = BTreeMap::new();
        for (name, weight) in weights {
            if name.trim().is_empty() {
                return Err(RankingError::BlankFeatureName);
            }
            if !weight.is_finite() {
                return Err(RankingError::NonFiniteWeight(name));
            }
            if fixed_weights.insert(name.clone(), weight).is_some() {
                return Err(RankingError::DuplicateFeatureName(name));
            }
        }
        if fixed_weights.is_empty() {
            return Err(RankingError::EmptyWeights);
        }
        Ok(Self {
            scorer_id,
            weights: fixed_weights,
            bias,
            model_provenance,
        })
    }

    /// Scores and orders candidates without mutating or validating their
    /// underlying chemistry. Every configured weight must have a matching
    /// feature so missing-data behavior is explicit rather than silently
    /// treating a missing feature as zero.
    pub fn rank(
        &self,
        candidates: &[CandidateFeatureVector],
    ) -> Result<Vec<RankedCandidate>, RankingError> {
        let mut ranked = Vec::with_capacity(candidates.len());
        for candidate in candidates {
            if self
                .weights
                .keys()
                .any(|name| !candidate.features.contains_key(name))
            {
                let missing = self
                    .weights
                    .keys()
                    .find(|name| !candidate.features.contains_key(*name))
                    .expect("the any() check found a missing feature");
                return Err(RankingError::MissingFeature {
                    candidate_id: candidate.candidate_id.clone(),
                    feature: missing.clone(),
                });
            }
            let score = self.bias
                + self
                    .weights
                    .iter()
                    .map(|(name, weight)| weight * candidate.features[name])
                    .sum::<f64>();
            if !score.is_finite() {
                return Err(RankingError::NonFiniteScore(candidate.candidate_id.clone()));
            }
            ranked.push(RankedCandidate {
                candidate_id: candidate.candidate_id.clone(),
                score,
                features: candidate.features.clone(),
                scorer_id: self.scorer_id.clone(),
                model_provenance: self.model_provenance.clone(),
            });
        }
        ranked.sort_by(|left, right| {
            right
                .score
                .total_cmp(&left.score)
                .then_with(|| left.candidate_id.cmp(&right.candidate_id))
        });
        Ok(ranked)
    }

    /// Orders candidates and applies an explicit margin-only abstention rule.
    ///
    /// The margin is a separation diagnostic, not a calibrated probability.
    /// A tie at the top always abstains, even when `min_score_margin` is zero.
    /// The method does not select a route or alter any chemical validity gate.
    pub fn rank_with_abstention(
        &self,
        candidates: &[CandidateFeatureVector],
        min_score_margin: f64,
    ) -> Result<RankingDecision, RankingError> {
        if !min_score_margin.is_finite() || min_score_margin < 0.0 {
            return Err(RankingError::InvalidAbstentionMargin);
        }
        let ranked = self.rank(candidates)?;
        let audit = RankingAudit::from_ranked(&ranked);
        let abstention = if ranked.is_empty() {
            Some(RankingAbstention::EmptyCandidates)
        } else if audit.top_score_tied {
            Some(RankingAbstention::TopScoreTie)
        } else if let Some(margin) = audit.score_margin {
            (margin < min_score_margin).then_some(RankingAbstention::InsufficientScoreMargin {
                margin,
                required: min_score_margin,
            })
        } else {
            None
        };
        let accepted = abstention.is_none().then(|| ranked[0].clone());
        Ok(RankingDecision {
            ranked,
            audit,
            accepted,
            abstention,
        })
    }
}

/// A ranked result retaining the inputs needed to audit the ordering.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RankedCandidate {
    pub candidate_id: String,
    pub score: f64,
    pub features: BTreeMap<String, f64>,
    pub scorer_id: String,
    pub model_provenance: String,
}

/// Deterministic audit information about one ranking invocation.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RankingAudit {
    pub candidate_count: usize,
    pub top_score: Option<f64>,
    pub runner_up_score: Option<f64>,
    pub score_margin: Option<f64>,
    pub top_score_tied: bool,
}

impl RankingAudit {
    fn from_ranked(ranked: &[RankedCandidate]) -> Self {
        let top_score = ranked.first().map(|candidate| candidate.score);
        let runner_up_score = ranked.get(1).map(|candidate| candidate.score);
        let score_margin = top_score
            .zip(runner_up_score)
            .map(|(top, runner_up)| top - runner_up);
        let top_score_tied = score_margin == Some(0.0);
        Self {
            candidate_count: ranked.len(),
            top_score,
            runner_up_score,
            score_margin,
            top_score_tied,
        }
    }
}

/// A ranking result with an explicit acceptance or abstention outcome.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RankingDecision {
    pub ranked: Vec<RankedCandidate>,
    pub audit: RankingAudit,
    pub accepted: Option<RankedCandidate>,
    pub abstention: Option<RankingAbstention>,
}

/// Reasons the fixed margin policy refuses to accept the top result.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RankingAbstention {
    EmptyCandidates,
    TopScoreTie,
    InsufficientScoreMargin { margin: f64, required: f64 },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum RankingError {
    #[error("candidate id must not be blank")]
    BlankCandidateId,
    #[error("scorer id must not be blank")]
    BlankScorerId,
    #[error("model provenance must not be blank")]
    BlankModelProvenance,
    #[error("feature name must not be blank")]
    BlankFeatureName,
    #[error("feature name is duplicated: {0}")]
    DuplicateFeatureName(String),
    #[error("scorer must contain at least one weight")]
    EmptyWeights,
    #[error("feature {0} is not finite")]
    NonFiniteFeature(String),
    #[error("weight {0} is not finite")]
    NonFiniteWeight(String),
    #[error("candidate {candidate_id} is missing feature {feature}")]
    MissingFeature {
        candidate_id: String,
        feature: String,
    },
    #[error("candidate {0} produced a non-finite score")]
    NonFiniteScore(String),
    #[error("abstention margin must be finite and non-negative")]
    InvalidAbstentionMargin,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vector(id: &str, value: f64, tie: f64) -> CandidateFeatureVector {
        CandidateFeatureVector::new(
            id.into(),
            [("retrieval_distance".into(), value), ("tie".into(), tie)],
        )
        .unwrap()
    }

    fn scorer() -> FixedLinearScorer {
        FixedLinearScorer::new(
            "fixed-v1".into(),
            [("retrieval_distance".into(), -1.0), ("tie".into(), 0.1)],
            0.0,
            "caller fixture phase37-v1".into(),
        )
        .unwrap()
    }

    #[test]
    fn ranks_by_fixed_weights_and_breaks_ties_by_id() {
        let results = scorer()
            .rank(&[
                vector("zeta", 1.0, 0.0),
                vector("alpha", 1.0, 0.0),
                vector("best", 0.1, 0.0),
            ])
            .unwrap();
        assert_eq!(
            results
                .iter()
                .map(|result| result.candidate_id.as_str())
                .collect::<Vec<_>>(),
            ["best", "alpha", "zeta"]
        );
        assert_eq!(results[0].scorer_id, "fixed-v1");
        assert_eq!(results[0].model_provenance, "caller fixture phase37-v1");
        assert_eq!(results[0].features["retrieval_distance"], 0.1);
    }

    #[test]
    fn ordering_is_independent_of_input_order() {
        let first = scorer()
            .rank(&[vector("b", 2.0, 0.0), vector("a", 1.0, 0.0)])
            .unwrap();
        let second = scorer()
            .rank(&[vector("a", 1.0, 0.0), vector("b", 2.0, 0.0)])
            .unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn missing_features_and_invalid_metadata_are_rejected() {
        let missing = CandidateFeatureVector::new("a".into(), [("tie".into(), 1.0)]).unwrap();
        assert!(matches!(
            scorer().rank(&[missing]),
            Err(RankingError::MissingFeature { .. })
        ));
        assert_eq!(
            FixedLinearScorer::new("".into(), [("x".into(), 1.0)], 0.0, "p".into()),
            Err(RankingError::BlankScorerId)
        );
        assert_eq!(
            CandidateFeatureVector::new("a".into(), [("x".into(), f64::NAN)]),
            Err(RankingError::NonFiniteFeature("x".into()))
        );
    }

    #[test]
    fn abstention_reports_ties_and_margins_without_hiding_ranked_results() {
        let tied = scorer()
            .rank_with_abstention(&[vector("b", 1.0, 0.0), vector("a", 1.0, 0.0)], 0.0)
            .unwrap();
        assert_eq!(tied.audit.candidate_count, 2);
        assert!(tied.audit.top_score_tied);
        assert_eq!(tied.accepted, None);
        assert_eq!(tied.abstention, Some(RankingAbstention::TopScoreTie));
        assert_eq!(tied.ranked.len(), 2);

        let narrow = scorer()
            .rank_with_abstention(&[vector("a", 1.0, 0.0), vector("b", 1.2, 0.0)], 0.5)
            .unwrap();
        assert!((narrow.audit.score_margin.unwrap() - 0.2).abs() < 1e-12);
        assert!(matches!(
            narrow.abstention,
            Some(RankingAbstention::InsufficientScoreMargin { .. })
        ));
    }

    #[test]
    fn empty_and_invalid_abstention_inputs_are_explicit() {
        let empty = scorer().rank_with_abstention(&[], 0.0).unwrap();
        assert_eq!(empty.abstention, Some(RankingAbstention::EmptyCandidates));
        assert_eq!(empty.audit.top_score, None);
        assert_eq!(
            scorer().rank_with_abstention(&[], -1.0),
            Err(RankingError::InvalidAbstentionMargin)
        );
    }
}
