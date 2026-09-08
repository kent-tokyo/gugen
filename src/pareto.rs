//! Phase 38: a generic, caller-supplied Pareto-frontier audit boundary.
//!
//! Objective names and directions are data supplied by the caller. This
//! module does not infer chemistry, optimize routes, or select a single
//! preferred candidate.

use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// A candidate with finite objective values.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ParetoCandidate {
    pub candidate_id: String,
    pub objectives: BTreeMap<String, f64>,
}

impl ParetoCandidate {
    pub fn new(
        candidate_id: String,
        objectives: impl IntoIterator<Item = (String, f64)>,
    ) -> Result<Self, ParetoError> {
        if candidate_id.trim().is_empty() {
            return Err(ParetoError::BlankCandidateId);
        }
        let mut values = BTreeMap::new();
        for (name, value) in objectives {
            if name.trim().is_empty() {
                return Err(ParetoError::BlankObjectiveName);
            }
            if !value.is_finite() {
                return Err(ParetoError::NonFiniteObjective(name));
            }
            if values.insert(name.clone(), value).is_some() {
                return Err(ParetoError::DuplicateObjectiveName(name));
            }
        }
        if values.is_empty() {
            return Err(ParetoError::EmptyObjectives);
        }
        Ok(Self {
            candidate_id,
            objectives: values,
        })
    }
}

/// Direction for one objective in the Pareto comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ObjectiveDirection {
    Maximize,
    Minimize,
}

/// Fixed objective directions and provenance for one Pareto evaluation.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ParetoFrontier {
    pub directions: BTreeMap<String, ObjectiveDirection>,
    pub frontier_provenance: String,
}

impl ParetoFrontier {
    pub fn new(
        directions: impl IntoIterator<Item = (String, ObjectiveDirection)>,
        frontier_provenance: String,
    ) -> Result<Self, ParetoError> {
        if frontier_provenance.trim().is_empty() {
            return Err(ParetoError::BlankFrontierProvenance);
        }
        let mut values = BTreeMap::new();
        for (name, direction) in directions {
            if name.trim().is_empty() {
                return Err(ParetoError::BlankObjectiveName);
            }
            if values.insert(name.clone(), direction).is_some() {
                return Err(ParetoError::DuplicateObjectiveName(name));
            }
        }
        if values.is_empty() {
            return Err(ParetoError::EmptyDirections);
        }
        Ok(Self {
            directions: values,
            frontier_provenance,
        })
    }

    /// Returns the non-dominated candidates and a deterministic audit of the
    /// candidates excluded from the frontier. No candidate is selected as a
    /// winner and no domain-specific meaning is assigned to an objective.
    pub fn evaluate(
        &self,
        candidates: &[ParetoCandidate],
    ) -> Result<ParetoEvaluation, ParetoError> {
        validate_candidates(candidates, &self.directions)?;
        let mut non_dominated = Vec::new();
        let mut dominated_ids = Vec::new();
        for (index, candidate) in candidates.iter().enumerate() {
            let dominated = candidates.iter().enumerate().any(|(other_index, other)| {
                index != other_index && self.dominates(other, candidate)
            });
            if dominated {
                dominated_ids.push(candidate.candidate_id.clone());
            } else {
                non_dominated.push(candidate.clone());
            }
        }
        non_dominated.sort_by(|left, right| left.candidate_id.cmp(&right.candidate_id));
        dominated_ids.sort();
        Ok(ParetoEvaluation {
            non_dominated,
            dominated_ids,
            candidate_count: candidates.len(),
            frontier_provenance: self.frontier_provenance.clone(),
        })
    }

    fn dominates(&self, left: &ParetoCandidate, right: &ParetoCandidate) -> bool {
        let mut strictly_better = false;
        for (name, direction) in &self.directions {
            let left_value = left.objectives[name];
            let right_value = right.objectives[name];
            let no_worse = match direction {
                ObjectiveDirection::Maximize => left_value >= right_value,
                ObjectiveDirection::Minimize => left_value <= right_value,
            };
            if !no_worse {
                return false;
            }
            if left_value != right_value {
                strictly_better = true;
            }
        }
        strictly_better
    }
}

/// A Pareto frontier plus deterministic audit evidence.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ParetoEvaluation {
    pub non_dominated: Vec<ParetoCandidate>,
    pub dominated_ids: Vec<String>,
    pub candidate_count: usize,
    pub frontier_provenance: String,
}

fn validate_candidates(
    candidates: &[ParetoCandidate],
    directions: &BTreeMap<String, ObjectiveDirection>,
) -> Result<(), ParetoError> {
    let mut ids = BTreeSet::new();
    for candidate in candidates {
        if !ids.insert(candidate.candidate_id.clone()) {
            return Err(ParetoError::DuplicateCandidateId(
                candidate.candidate_id.clone(),
            ));
        }
        if candidate.objectives.keys().collect::<Vec<_>>() != directions.keys().collect::<Vec<_>>()
        {
            return Err(ParetoError::ObjectiveSchemaMismatch {
                candidate_id: candidate.candidate_id.clone(),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ParetoError {
    #[error("candidate id must not be blank")]
    BlankCandidateId,
    #[error("candidate ids must be unique: {0}")]
    DuplicateCandidateId(String),
    #[error("objective name must not be blank")]
    BlankObjectiveName,
    #[error("objective name is duplicated: {0}")]
    DuplicateObjectiveName(String),
    #[error("candidate must contain at least one objective")]
    EmptyObjectives,
    #[error("objective {0} is not finite")]
    NonFiniteObjective(String),
    #[error("frontier must contain at least one objective direction")]
    EmptyDirections,
    #[error("frontier provenance must not be blank")]
    BlankFrontierProvenance,
    #[error("candidate objective schema differs: {candidate_id}")]
    ObjectiveSchemaMismatch { candidate_id: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, cost: f64, quality: f64) -> ParetoCandidate {
        ParetoCandidate::new(
            id.into(),
            [("cost".into(), cost), ("quality".into(), quality)],
        )
        .unwrap()
    }

    fn frontier() -> ParetoFrontier {
        ParetoFrontier::new(
            [
                ("cost".into(), ObjectiveDirection::Minimize),
                ("quality".into(), ObjectiveDirection::Maximize),
            ],
            "phase38-pareto-v1".into(),
        )
        .unwrap()
    }

    #[test]
    fn returns_non_dominated_tradeoffs_and_sorted_audit() {
        let evaluation = frontier()
            .evaluate(&[
                candidate("balanced", 2.0, 8.0),
                candidate("cheap", 1.0, 5.0),
                candidate("quality", 3.0, 10.0),
                candidate("dominated", 2.5, 7.0),
            ])
            .unwrap();
        assert_eq!(
            evaluation
                .non_dominated
                .iter()
                .map(|candidate| candidate.candidate_id.as_str())
                .collect::<Vec<_>>(),
            ["balanced", "cheap", "quality"]
        );
        assert_eq!(evaluation.dominated_ids, ["dominated"]);
        assert_eq!(evaluation.frontier_provenance, "phase38-pareto-v1");
    }

    #[test]
    fn rejects_duplicate_ids_and_schema_mismatch() {
        assert!(matches!(
            frontier().evaluate(&[candidate("a", 1.0, 1.0), candidate("a", 2.0, 2.0)]),
            Err(ParetoError::DuplicateCandidateId(_))
        ));
        let mismatched = ParetoCandidate::new("b".into(), [("cost".into(), 1.0)]).unwrap();
        assert!(matches!(
            frontier().evaluate(&[candidate("a", 1.0, 1.0), mismatched]),
            Err(ParetoError::ObjectiveSchemaMismatch { .. })
        ));
    }
}
