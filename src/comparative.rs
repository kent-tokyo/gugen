//! Phase 39: a comparison-protocol boundary, not a comparison claim.
//!
//! This module records what would be needed for a fair comparison. It does
//! not execute external systems, fetch corpora, or synthesize performance
//! results.

use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// A comparator identified by an immutable, caller-pinned version.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ComparatorSpec {
    pub name: String,
    pub version: String,
    pub source: String,
}

impl ComparatorSpec {
    pub fn new(name: String, version: String, source: String) -> Result<Self, ComparativeError> {
        if name.trim().is_empty() {
            return Err(ComparativeError::BlankComparatorName);
        }
        if version.trim().is_empty() {
            return Err(ComparativeError::BlankComparatorVersion { name });
        }
        if source.trim().is_empty() {
            return Err(ComparativeError::BlankComparatorSource { name });
        }
        Ok(Self {
            name,
            version,
            source,
        })
    }
}

/// A group-aware split assignment for one evaluation case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EvaluationSplit {
    Development,
    Confirmation,
}

/// A corpus case with a group key that must remain within one split.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EvaluationCase {
    pub case_id: String,
    pub group_id: String,
    pub split: EvaluationSplit,
}

impl EvaluationCase {
    pub fn new(
        case_id: String,
        group_id: String,
        split: EvaluationSplit,
    ) -> Result<Self, ComparativeError> {
        if case_id.trim().is_empty() {
            return Err(ComparativeError::BlankCaseId);
        }
        if group_id.trim().is_empty() {
            return Err(ComparativeError::BlankGroupId { case_id });
        }
        Ok(Self {
            case_id,
            group_id,
            split,
        })
    }
}

/// A fixed comparison plan. Construction is intentionally offline and
/// contains no executor or score field.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ComparativeEvaluationProtocol {
    pub protocol_id: String,
    pub corpus_id: String,
    pub corpus_license: String,
    pub runtime_conditions: String,
    pub comparators: Vec<ComparatorSpec>,
    pub cases: Vec<EvaluationCase>,
}

impl ComparativeEvaluationProtocol {
    pub fn new(
        protocol_id: String,
        corpus_id: String,
        corpus_license: String,
        runtime_conditions: String,
        comparators: Vec<ComparatorSpec>,
        cases: Vec<EvaluationCase>,
    ) -> Result<Self, ComparativeError> {
        let protocol = Self {
            protocol_id,
            corpus_id,
            corpus_license,
            runtime_conditions,
            comparators,
            cases,
        };
        protocol.validate()?;
        Ok(protocol)
    }

    /// Re-checks protocol invariants after construction or deserialization.
    pub fn validate(&self) -> Result<(), ComparativeError> {
        if self.protocol_id.trim().is_empty() {
            return Err(ComparativeError::BlankProtocolId);
        }
        if self.corpus_id.trim().is_empty() {
            return Err(ComparativeError::BlankCorpusId);
        }
        if self.corpus_license.trim().is_empty() {
            return Err(ComparativeError::BlankCorpusLicense);
        }
        if self.runtime_conditions.trim().is_empty() {
            return Err(ComparativeError::BlankRuntimeConditions);
        }
        if self.comparators.is_empty() {
            return Err(ComparativeError::NoComparators);
        }
        let mut comparator_names = BTreeSet::new();
        for comparator in &self.comparators {
            ComparatorSpec::new(
                comparator.name.clone(),
                comparator.version.clone(),
                comparator.source.clone(),
            )?;
            if !comparator_names.insert(comparator.name.clone()) {
                return Err(ComparativeError::DuplicateComparatorName(
                    comparator.name.clone(),
                ));
            }
        }
        let mut case_ids = BTreeSet::new();
        for case in &self.cases {
            EvaluationCase::new(case.case_id.clone(), case.group_id.clone(), case.split)?;
            if !case_ids.insert(case.case_id.clone()) {
                return Err(ComparativeError::DuplicateCaseId(case.case_id.clone()));
            }
        }
        Ok(())
    }

    /// Audits whether any group crosses the development/confirmation split.
    /// This is an audit only; it does not report tool performance.
    pub fn leakage_audit(&self) -> ComparativeAudit {
        let mut groups: BTreeMap<String, BTreeSet<EvaluationSplit>> = BTreeMap::new();
        let mut cases_by_split = BTreeMap::new();
        for case in &self.cases {
            groups
                .entry(case.group_id.clone())
                .or_default()
                .insert(case.split);
            *cases_by_split.entry(case.split).or_insert(0usize) += 1;
        }
        let leaked_groups = groups
            .into_iter()
            .filter_map(|(group, splits)| (splits.len() > 1).then_some(group))
            .collect();
        ComparativeAudit {
            protocol_id: self.protocol_id.clone(),
            candidate_count: self.cases.len(),
            cases_by_split,
            leaked_groups,
            measurement_status: MeasurementStatus::ProtocolOnly,
        }
    }

    /// Fails closed when the protocol is not safe to measure.
    pub fn validate_for_measurement(&self) -> Result<(), ComparativeError> {
        let audit = self.leakage_audit();
        if !audit.leaked_groups.is_empty() {
            return Err(ComparativeError::GroupLeakage {
                groups: audit.leaked_groups,
            });
        }
        if !audit
            .cases_by_split
            .contains_key(&EvaluationSplit::Development)
            || !audit
                .cases_by_split
                .contains_key(&EvaluationSplit::Confirmation)
        {
            return Err(ComparativeError::MissingEvaluationSplit);
        }
        Ok(())
    }
}

/// Reproducibility metadata required before a comparison is measured.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EvaluationMetadata {
    pub ablations: Vec<AblationSpec>,
    pub uncertainty: UncertaintySpec,
    pub resources: ResourceConditions,
    pub failure_taxonomy: Vec<FailureCategory>,
}

impl EvaluationMetadata {
    pub fn new(
        ablations: Vec<AblationSpec>,
        uncertainty: UncertaintySpec,
        resources: ResourceConditions,
        failure_taxonomy: Vec<FailureCategory>,
    ) -> Result<Self, ComparativeError> {
        let metadata = Self {
            ablations,
            uncertainty,
            resources,
            failure_taxonomy,
        };
        metadata.validate()?;
        Ok(metadata)
    }

    /// Re-checks all nested invariants after construction or deserialization.
    /// Public fields and serde can bypass the `new` constructors, so manifest
    /// consumers must validate the complete metadata tree before measuring.
    pub fn validate(&self) -> Result<(), ComparativeError> {
        if self.ablations.is_empty() {
            return Err(ComparativeError::NoAblations);
        }
        validate_unique_ids(
            self.ablations.iter().map(|item| item.id.as_str()),
            ComparativeError::DuplicateAblationId,
        )?;
        for ablation in &self.ablations {
            if ablation.id.trim().is_empty() || ablation.description.trim().is_empty() {
                return Err(ComparativeError::BlankAblationField);
            }
        }
        UncertaintySpec::new(
            self.uncertainty.method.clone(),
            self.uncertainty.confidence_level,
            self.uncertainty.replicates,
        )?;
        ResourceConditions::new(
            self.resources.cpu_model.clone(),
            self.resources.memory_limit_mb,
            self.resources.time_limit_ms,
            self.resources.parallelism,
        )?;
        if self.failure_taxonomy.is_empty() {
            return Err(ComparativeError::NoFailureCategories);
        }
        validate_unique_ids(
            self.failure_taxonomy.iter().map(|item| item.id.as_str()),
            ComparativeError::DuplicateFailureCategoryId,
        )?;
        for category in &self.failure_taxonomy {
            if category.id.trim().is_empty() || category.description.trim().is_empty() {
                return Err(ComparativeError::BlankFailureCategoryField);
            }
        }
        Ok(())
    }
}

/// One explicitly named comparison ablation.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AblationSpec {
    pub id: String,
    pub description: String,
}

impl AblationSpec {
    pub fn new(id: String, description: String) -> Result<Self, ComparativeError> {
        if id.trim().is_empty() || description.trim().is_empty() {
            return Err(ComparativeError::BlankAblationField);
        }
        Ok(Self { id, description })
    }
}

/// A declared uncertainty procedure; it does not contain measured results.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct UncertaintySpec {
    pub method: String,
    pub confidence_level: f64,
    pub replicates: usize,
}

impl UncertaintySpec {
    pub fn new(
        method: String,
        confidence_level: f64,
        replicates: usize,
    ) -> Result<Self, ComparativeError> {
        if method.trim().is_empty() {
            return Err(ComparativeError::BlankUncertaintyMethod);
        }
        if !confidence_level.is_finite() || !(0.0..1.0).contains(&confidence_level) {
            return Err(ComparativeError::InvalidConfidenceLevel);
        }
        if replicates == 0 {
            return Err(ComparativeError::ZeroReplicates);
        }
        Ok(Self {
            method,
            confidence_level,
            replicates,
        })
    }
}

/// Resource limits and execution conditions to record alongside a run.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ResourceConditions {
    pub cpu_model: String,
    pub memory_limit_mb: Option<u64>,
    pub time_limit_ms: Option<u64>,
    pub parallelism: usize,
}

impl ResourceConditions {
    pub fn new(
        cpu_model: String,
        memory_limit_mb: Option<u64>,
        time_limit_ms: Option<u64>,
        parallelism: usize,
    ) -> Result<Self, ComparativeError> {
        if cpu_model.trim().is_empty() {
            return Err(ComparativeError::BlankCpuModel);
        }
        if parallelism == 0 {
            return Err(ComparativeError::ZeroParallelism);
        }
        Ok(Self {
            cpu_model,
            memory_limit_mb,
            time_limit_ms,
            parallelism,
        })
    }
}

/// A stable failure bucket name and its human-readable meaning.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FailureCategory {
    pub id: String,
    pub description: String,
}

impl FailureCategory {
    pub fn new(id: String, description: String) -> Result<Self, ComparativeError> {
        if id.trim().is_empty() || description.trim().is_empty() {
            return Err(ComparativeError::BlankFailureCategoryField);
        }
        Ok(Self { id, description })
    }
}

/// A protocol plus the metadata needed to make a future run reproducible.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ComparativeEvaluationManifest {
    pub protocol: ComparativeEvaluationProtocol,
    pub metadata: EvaluationMetadata,
}

impl ComparativeEvaluationManifest {
    pub fn new(protocol: ComparativeEvaluationProtocol, metadata: EvaluationMetadata) -> Self {
        Self { protocol, metadata }
    }

    pub fn validate_for_measurement(&self) -> Result<(), ComparativeError> {
        self.protocol.validate()?;
        self.metadata.validate()?;
        self.protocol.validate_for_measurement()
    }
}

fn validate_unique_ids<'a>(
    ids: impl IntoIterator<Item = &'a str>,
    duplicate: impl Fn(String) -> ComparativeError,
) -> Result<(), ComparativeError> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(duplicate(id.to_owned()));
        }
    }
    Ok(())
}

/// Evidence produced by protocol validation, deliberately without scores.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ComparativeAudit {
    pub protocol_id: String,
    pub candidate_count: usize,
    pub cases_by_split: BTreeMap<EvaluationSplit, usize>,
    pub leaked_groups: BTreeSet<String>,
    pub measurement_status: MeasurementStatus,
}

/// The only status this offline protocol can honestly assert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MeasurementStatus {
    ProtocolOnly,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ComparativeError {
    #[error("comparator name must not be blank")]
    BlankComparatorName,
    #[error("comparator version must not be blank: {name}")]
    BlankComparatorVersion { name: String },
    #[error("comparator source must not be blank: {name}")]
    BlankComparatorSource { name: String },
    #[error("comparator names must be unique: {0}")]
    DuplicateComparatorName(String),
    #[error("protocol id must not be blank")]
    BlankProtocolId,
    #[error("corpus id must not be blank")]
    BlankCorpusId,
    #[error("corpus license must not be blank")]
    BlankCorpusLicense,
    #[error("runtime conditions must not be blank")]
    BlankRuntimeConditions,
    #[error("at least one comparator is required")]
    NoComparators,
    #[error("case id must not be blank")]
    BlankCaseId,
    #[error("group id must not be blank: {case_id}")]
    BlankGroupId { case_id: String },
    #[error("case ids must be unique: {0}")]
    DuplicateCaseId(String),
    #[error("group leakage across evaluation splits: {groups:?}")]
    GroupLeakage { groups: BTreeSet<String> },
    #[error("both development and confirmation splits are required")]
    MissingEvaluationSplit,
    #[error("at least one ablation is required")]
    NoAblations,
    #[error("ablation id and description must not be blank")]
    BlankAblationField,
    #[error("ablation ids must be unique: {0}")]
    DuplicateAblationId(String),
    #[error("uncertainty method must not be blank")]
    BlankUncertaintyMethod,
    #[error("confidence level must be finite and between 0 and 1")]
    InvalidConfidenceLevel,
    #[error("at least one uncertainty replicate is required")]
    ZeroReplicates,
    #[error("CPU model must not be blank")]
    BlankCpuModel,
    #[error("parallelism must be positive")]
    ZeroParallelism,
    #[error("at least one failure category is required")]
    NoFailureCategories,
    #[error("failure category id and description must not be blank")]
    BlankFailureCategoryField,
    #[error("failure category ids must be unique: {0}")]
    DuplicateFailureCategoryId(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comparator(name: &str) -> ComparatorSpec {
        ComparatorSpec::new(name.into(), "1.2.3".into(), "local-pinned-source".into()).unwrap()
    }

    fn case(id: &str, group: &str, split: EvaluationSplit) -> EvaluationCase {
        EvaluationCase::new(id.into(), group.into(), split).unwrap()
    }

    fn protocol(cases: Vec<EvaluationCase>) -> ComparativeEvaluationProtocol {
        ComparativeEvaluationProtocol::new(
            "phase39-local-v1".into(),
            "local-contract".into(),
            "fixture-only".into(),
            "single-process; no external execution".into(),
            vec![comparator("reaction-network"), comparator("retrieval")],
            cases,
        )
        .unwrap()
    }

    #[test]
    fn audit_is_protocol_only_and_accepts_grouped_split() {
        let protocol = protocol(vec![
            case("dev-1", "group-a", EvaluationSplit::Development),
            case("confirm-1", "group-b", EvaluationSplit::Confirmation),
        ]);
        let audit = protocol.leakage_audit();
        assert!(audit.leaked_groups.is_empty());
        assert_eq!(audit.measurement_status, MeasurementStatus::ProtocolOnly);
        assert!(protocol.validate_for_measurement().is_ok());
    }

    #[test]
    fn cross_split_group_fails_closed() {
        let protocol = protocol(vec![
            case("dev-1", "same-group", EvaluationSplit::Development),
            case("confirm-1", "same-group", EvaluationSplit::Confirmation),
        ]);
        assert_eq!(
            protocol.leakage_audit().leaked_groups,
            BTreeSet::from(["same-group".into()])
        );
        assert!(matches!(
            protocol.validate_for_measurement(),
            Err(ComparativeError::GroupLeakage { .. })
        ));
    }

    #[test]
    fn metadata_requires_explicit_reproducibility_fields_without_results() {
        let metadata = EvaluationMetadata::new(
            vec![AblationSpec::new("baseline".into(), "fixed local baseline".into()).unwrap()],
            UncertaintySpec::new("bootstrap".into(), 0.95, 10).unwrap(),
            ResourceConditions::new("local-cpu".into(), Some(1024), Some(1000), 1).unwrap(),
            vec![FailureCategory::new("invalid-input".into(), "input rejected".into()).unwrap()],
        )
        .unwrap();
        assert_eq!(metadata.uncertainty.replicates, 10);
        assert!(matches!(
            UncertaintySpec::new("bootstrap".into(), 1.0, 10),
            Err(ComparativeError::InvalidConfidenceLevel)
        ));
        assert!(matches!(
            EvaluationMetadata::new(
                vec![
                    AblationSpec::new("same".into(), "a".into()).unwrap(),
                    AblationSpec::new("same".into(), "b".into()).unwrap(),
                ],
                UncertaintySpec::new("none".into(), 0.9, 1).unwrap(),
                ResourceConditions::new("cpu".into(), None, None, 1).unwrap(),
                vec![FailureCategory::new("x".into(), "x".into()).unwrap()],
            ),
            Err(ComparativeError::DuplicateAblationId(_))
        ));
    }

    #[test]
    fn manifest_rechecks_nested_metadata_invariants() {
        let metadata = EvaluationMetadata {
            ablations: vec![AblationSpec {
                id: "baseline".into(),
                description: "".into(),
            }],
            uncertainty: UncertaintySpec {
                method: "bootstrap".into(),
                confidence_level: 0.95,
                replicates: 10,
            },
            resources: ResourceConditions {
                cpu_model: "local-cpu".into(),
                memory_limit_mb: None,
                time_limit_ms: None,
                parallelism: 1,
            },
            failure_taxonomy: vec![FailureCategory {
                id: "invalid-input".into(),
                description: "input rejected".into(),
            }],
        };
        let manifest = ComparativeEvaluationManifest::new(
            protocol(vec![
                case("dev-1", "group-a", EvaluationSplit::Development),
                case("confirm-1", "group-b", EvaluationSplit::Confirmation),
            ]),
            metadata,
        );
        assert_eq!(
            manifest.validate_for_measurement(),
            Err(ComparativeError::BlankAblationField)
        );
    }

    #[test]
    fn manifest_rechecks_protocol_invariants() {
        let protocol = ComparativeEvaluationProtocol {
            protocol_id: "phase39-local-v1".into(),
            corpus_id: "local-contract".into(),
            corpus_license: "fixture-only".into(),
            runtime_conditions: "single-process".into(),
            comparators: vec![ComparatorSpec {
                name: "".into(),
                version: "pinned".into(),
                source: "fixture".into(),
            }],
            cases: vec![
                case("dev-1", "group-a", EvaluationSplit::Development),
                case("confirm-1", "group-b", EvaluationSplit::Confirmation),
            ],
        };
        let metadata = EvaluationMetadata::new(
            vec![AblationSpec::new("baseline".into(), "fixed baseline".into()).unwrap()],
            UncertaintySpec::new("bootstrap".into(), 0.95, 2).unwrap(),
            ResourceConditions::new("local-cpu".into(), None, None, 1).unwrap(),
            vec![FailureCategory::new("invalid-input".into(), "rejected".into()).unwrap()],
        )
        .unwrap();
        let manifest = ComparativeEvaluationManifest::new(protocol, metadata);
        assert_eq!(
            manifest.validate_for_measurement(),
            Err(ComparativeError::BlankComparatorName)
        );
    }
}
