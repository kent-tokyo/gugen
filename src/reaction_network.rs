//! Phases 33-34: deterministic, caller-supplied reaction-network traversal
//! and versioned JSONL ingestion.
//!
//! This is an ingestion and search boundary, not an online client. Callers
//! supply validated [`BalancedReaction`] records from a network dataset or
//! another source. Traversal reuses [`SynthesisRoute`] validation, so this
//! module does not create a second reaction or conservation model.

use crate::composition::Composition;
use crate::multi_step::{RouteError, SynthesisRoute};
use crate::reaction::{BalancedReaction, ReactionSpecies};
use std::collections::{BTreeSet, VecDeque};
use thiserror::Error;

/// Version of the caller-supplied network record format.
pub const NETWORK_RECORD_SCHEMA_VERSION: u32 = 1;

/// Qualification state retained on every imported network record.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum NetworkReactionQualification {
    Qualified,
    Rejected { reason: String },
}

/// Provenance-bearing record used to import one network reaction.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NetworkReactionRecord {
    pub schema_version: u32,
    pub source_id: String,
    pub license: String,
    pub dataset_version: String,
    pub record_id: String,
    pub reaction: BalancedReaction,
    pub qualification: NetworkReactionQualification,
}

impl NetworkReactionRecord {
    pub fn new(
        source_id: String,
        license: String,
        dataset_version: String,
        record_id: String,
        reaction: BalancedReaction,
        qualification: NetworkReactionQualification,
    ) -> Result<Self, NetworkError> {
        if source_id.trim().is_empty()
            || license.trim().is_empty()
            || dataset_version.trim().is_empty()
            || record_id.trim().is_empty()
        {
            return Err(NetworkError::MissingProvenance);
        }
        if let NetworkReactionQualification::Rejected { reason } = &qualification
            && reason.trim().is_empty()
        {
            return Err(NetworkError::MissingRejectionReason);
        }
        Ok(Self {
            schema_version: NETWORK_RECORD_SCHEMA_VERSION,
            source_id,
            license,
            dataset_version,
            record_id,
            reaction,
            qualification,
        })
    }
}

/// Bounds for network traversal. All values must be positive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NetworkSearchBudget {
    pub max_steps: usize,
    pub max_states: usize,
    pub max_routes: usize,
}

impl NetworkSearchBudget {
    pub fn new(
        max_steps: usize,
        max_states: usize,
        max_routes: usize,
    ) -> Result<Self, NetworkError> {
        if max_steps == 0 || max_states == 0 || max_routes == 0 {
            return Err(NetworkError::InvalidBudget);
        }
        Ok(Self {
            max_steps,
            max_states,
            max_routes,
        })
    }
}

/// Errors specific to network indexing and traversal.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum NetworkError {
    #[error("reaction network must contain at least one reaction")]
    EmptyNetwork,
    #[error("network search budget values must all be positive")]
    InvalidBudget,
    #[error("network route assembly failed: {0}")]
    Route(#[from] RouteError),
    #[error("network record is missing required provenance")]
    MissingProvenance,
    #[error("network rejected record is missing a rejection reason")]
    MissingRejectionReason,
    #[error("malformed network record: {0}")]
    MalformedRecord(String),
    #[error("network record has unsupported schema version {0}")]
    UnsupportedSchemaVersion(u32),
}

/// Whether malformed JSONL records fail the whole import or are retained in
/// the load report while valid records continue through the import boundary.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkLoadMode {
    Strict,
    Lenient,
}

/// A single JSONL record rejected before it can enter the executable graph.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct RejectedNetworkRecord {
    /// Zero-based physical line number in the supplied JSONL text.
    pub line: usize,
    pub reason: String,
}

/// Counts and reasons produced by [`parse_network_records`].
#[cfg(feature = "serde")]
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize)]
pub struct NetworkLoadReport {
    /// Records that parsed and passed the record-level contract, including
    /// explicitly rejected qualifications (which remain audit evidence).
    pub accepted: usize,
    pub qualified: usize,
    pub explicitly_rejected: usize,
    pub duplicates_collapsed: usize,
    pub rejected: Vec<RejectedNetworkRecord>,
}

/// Parses one [`NetworkReactionRecord`] per non-blank JSONL line.
///
/// This function is deliberately pure and does no file I/O. In lenient mode,
/// malformed lines are reported and skipped; in strict mode, the first bad
/// line fails the import. Exact duplicate records are collapsed after parsing,
/// while qualified and explicitly rejected records are both retained so the
/// caller can audit the source before constructing [`ReactionNetwork`].
#[cfg(feature = "serde")]
pub fn parse_network_records(
    jsonl: &str,
    mode: NetworkLoadMode,
) -> Result<(Vec<NetworkReactionRecord>, NetworkLoadReport), NetworkError> {
    let mut records = Vec::new();
    let mut rejected = Vec::new();

    for (line, raw_line) in jsonl.lines().enumerate() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parsed = serde_json::from_str::<NetworkReactionRecord>(trimmed)
            .map_err(|error| error.to_string())
            .and_then(validate_network_record);
        match parsed {
            Ok(record) => records.push(record),
            Err(reason) => {
                if mode == NetworkLoadMode::Strict {
                    return Err(NetworkError::MalformedRecord(format!(
                        "line {line}: {reason}"
                    )));
                }
                rejected.push(RejectedNetworkRecord { line, reason });
            }
        }
    }

    records.sort_by_key(network_record_key);
    let parsed_count = records.len();
    records.dedup();
    let duplicates_collapsed = parsed_count - records.len();
    let qualified = records
        .iter()
        .filter(|record| {
            matches!(
                record.qualification,
                NetworkReactionQualification::Qualified
            )
        })
        .count();
    let explicitly_rejected = records.len() - qualified;
    let report = NetworkLoadReport {
        accepted: records.len(),
        qualified,
        explicitly_rejected,
        duplicates_collapsed,
        rejected,
    };
    Ok((records, report))
}

#[cfg(feature = "serde")]
fn validate_network_record(record: NetworkReactionRecord) -> Result<NetworkReactionRecord, String> {
    if record.schema_version != NETWORK_RECORD_SCHEMA_VERSION {
        return Err(NetworkError::UnsupportedSchemaVersion(record.schema_version).to_string());
    }
    if record.source_id.trim().is_empty()
        || record.license.trim().is_empty()
        || record.dataset_version.trim().is_empty()
        || record.record_id.trim().is_empty()
    {
        return Err(NetworkError::MissingProvenance.to_string());
    }
    if let NetworkReactionQualification::Rejected { reason } = &record.qualification
        && reason.trim().is_empty()
    {
        return Err(NetworkError::MissingRejectionReason.to_string());
    }
    Ok(record)
}

#[cfg(feature = "serde")]
fn network_record_key(record: &NetworkReactionRecord) -> String {
    format!(
        "{}\0{}\0{}\0{}\0{:?}\0{}",
        record.source_id,
        record.dataset_version,
        record.record_id,
        record.license,
        record.qualification,
        reaction_key(&record.reaction)
    )
}

/// A deterministic collection of caller-supplied reaction hyperedges.
#[derive(Debug, Clone, PartialEq)]
pub struct ReactionNetwork {
    reactions: Vec<BalancedReaction>,
}

impl ReactionNetwork {
    /// Sorts and removes exact duplicate reactions at construction.
    pub fn new(mut reactions: Vec<BalancedReaction>) -> Result<Self, NetworkError> {
        if reactions.is_empty() {
            return Err(NetworkError::EmptyNetwork);
        }
        reactions.sort_by_key(reaction_key);
        reactions.dedup();
        Ok(Self { reactions })
    }

    pub fn reactions(&self) -> &[BalancedReaction] {
        &self.reactions
    }

    /// Builds a network from qualified records, rejecting unsupported schema
    /// versions and retaining rejected records out of the executable graph.
    pub fn from_records(records: &[NetworkReactionRecord]) -> Result<Self, NetworkError> {
        let mut reactions = Vec::new();
        for record in records {
            if record.schema_version != NETWORK_RECORD_SCHEMA_VERSION {
                return Err(NetworkError::UnsupportedSchemaVersion(
                    record.schema_version,
                ));
            }
            if matches!(
                &record.qualification,
                NetworkReactionQualification::Qualified
            ) {
                reactions.push(record.reaction.clone());
            }
        }
        Self::new(reactions)
    }

    /// Finds routes whose stages are supplied by this network.
    ///
    /// A reaction is applicable when every reactant composition is currently
    /// available. Products are added for later stages. States are
    /// deduplicated, preventing cycles and making traversal deterministic.
    pub fn search_routes(
        &self,
        base_precursors: &[Composition],
        target: &Composition,
        budget: NetworkSearchBudget,
    ) -> Result<Vec<SynthesisRoute>, NetworkError> {
        let initial: BTreeSet<Composition> = base_precursors.iter().cloned().collect();
        let mut queue = VecDeque::from([(initial.clone(), Vec::<BalancedReaction>::new())]);
        let mut seen = BTreeSet::from([initial]);
        let mut routes = Vec::new();
        let mut visited_states = 0;

        while let Some((available, stages)) = queue.pop_front() {
            visited_states += 1;
            if visited_states > budget.max_states {
                break;
            }
            if stages.len() >= budget.max_steps {
                continue;
            }
            for reaction in &self.reactions {
                if !reaction
                    .reactants()
                    .iter()
                    .all(|s| available.contains(&s.composition))
                {
                    continue;
                }
                let mut next = available.clone();
                next.extend(reaction.products().iter().map(|s| s.composition.clone()));
                if next == available || !seen.insert(next.clone()) {
                    continue;
                }
                let mut next_stages = stages.clone();
                next_stages.push(reaction.clone());
                if reaction.products().iter().any(|s| &s.composition == target) {
                    routes.push(SynthesisRoute::new(
                        next_stages.clone(),
                        base_precursors,
                        target,
                    )?);
                    if routes.len() == budget.max_routes {
                        return Ok(routes);
                    }
                }
                queue.push_back((next, next_stages));
            }
        }
        Ok(routes)
    }
}

fn reaction_key(reaction: &BalancedReaction) -> String {
    fn species_key(species: &ReactionSpecies) -> String {
        let composition = species
            .composition
            .iter()
            .map(|(element, amount)| format!("{element}:{amount:?}"))
            .collect::<Vec<_>>()
            .join(",");
        format!("{composition}*{}", species.coefficient())
    }
    let side = |species: &[ReactionSpecies]| {
        species
            .iter()
            .map(species_key)
            .collect::<Vec<_>>()
            .join("|")
    };
    format!(
        "{}->{}",
        side(reaction.reactants()),
        side(reaction.products())
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Element, ReactionSpecies};

    fn c(pairs: &[(&str, f64)]) -> Composition {
        Composition::new(pairs.iter().map(|(e, n)| (Element::new(e).unwrap(), *n))).unwrap()
    }

    fn r(
        reactants: &[(&[(&str, f64)], u64)],
        products: &[(&[(&str, f64)], u64)],
    ) -> BalancedReaction {
        BalancedReaction::new(
            reactants
                .iter()
                .map(|(p, n)| ReactionSpecies::new(c(p), *n).unwrap())
                .collect(),
            products
                .iter()
                .map(|(p, n)| ReactionSpecies::new(c(p), *n).unwrap())
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn finds_connected_route_independent_of_network_order() {
        let a = c(&[("Fe", 1.0)]);
        let b = c(&[("Li", 1.0)]);
        let o = c(&[("O", 1.0)]);
        let intermediate = c(&[("Fe", 1.0), ("Li", 1.0)]);
        let target = c(&[("Fe", 1.0), ("Li", 1.0), ("O", 1.0)]);
        let first = r(
            &[(&[("Fe", 1.0)], 1), (&[("Li", 1.0)], 1)],
            &[(&[("Fe", 1.0), ("Li", 1.0)], 1)],
        );
        let second = r(
            &[(&[("Fe", 1.0), ("Li", 1.0)], 1), (&[("O", 1.0)], 1)],
            &[(&[("Fe", 1.0), ("Li", 1.0), ("O", 1.0)], 1)],
        );
        let budget = NetworkSearchBudget::new(3, 20, 5).unwrap();
        let first_routes = ReactionNetwork::new(vec![second.clone(), first.clone(), first.clone()])
            .unwrap()
            .search_routes(&[a.clone(), b.clone(), o.clone()], &target, budget)
            .unwrap();
        let second_routes = ReactionNetwork::new(vec![first, second])
            .unwrap()
            .search_routes(&[a, b, o], &target, budget)
            .unwrap();
        assert_eq!(first_routes, second_routes);
        assert_eq!(first_routes.len(), 1);
        assert_eq!(first_routes[0].stages().len(), 2);
        assert_eq!(
            intermediate,
            first_routes[0].stages()[0].products()[0].composition
        );
    }

    #[test]
    fn rejects_empty_network_and_budget() {
        assert_eq!(
            ReactionNetwork::new(vec![]),
            Err(NetworkError::EmptyNetwork)
        );
        assert_eq!(
            NetworkSearchBudget::new(0, 1, 1),
            Err(NetworkError::InvalidBudget)
        );
    }

    #[test]
    fn provenance_and_qualification_control_network_import() {
        let reaction = r(&[(&[("Fe", 1.0)], 1)], &[(&[("Fe", 1.0)], 1)]);
        let qualified = NetworkReactionRecord::new(
            "local-fixture".into(),
            "CC0-1.0".into(),
            "test-1".into(),
            "r1".into(),
            reaction.clone(),
            NetworkReactionQualification::Qualified,
        )
        .unwrap();
        let rejected = NetworkReactionRecord::new(
            "local-fixture".into(),
            "CC0-1.0".into(),
            "test-1".into(),
            "r2".into(),
            reaction,
            NetworkReactionQualification::Rejected {
                reason: "fixture negative control".into(),
            },
        )
        .unwrap();
        assert_eq!(
            ReactionNetwork::from_records(&[qualified, rejected])
                .unwrap()
                .reactions()
                .len(),
            1
        );
        assert_eq!(
            NetworkReactionRecord::new(
                "".into(),
                "CC0".into(),
                "v1".into(),
                "r".into(),
                r(&[(&[("Fe", 1.0)], 1)], &[(&[("Fe", 1.0)], 1)]),
                NetworkReactionQualification::Qualified,
            ),
            Err(NetworkError::MissingProvenance)
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn parses_jsonl_with_audit_counts_and_deterministic_deduplication() {
        let reaction = r(&[(&[("Fe", 1.0)], 1)], &[(&[("Fe", 1.0)], 1)]);
        let record = NetworkReactionRecord::new(
            "fixture".into(),
            "CC0-1.0".into(),
            "v1".into(),
            "r1".into(),
            reaction.clone(),
            NetworkReactionQualification::Qualified,
        )
        .unwrap();
        let rejected = NetworkReactionRecord::new(
            "fixture".into(),
            "CC0-1.0".into(),
            "v1".into(),
            "r2".into(),
            reaction,
            NetworkReactionQualification::Rejected {
                reason: "unbalanced source row".into(),
            },
        )
        .unwrap();
        let jsonl = format!(
            "{}\n{}\n{}\n{{bad json}}\n",
            serde_json::to_string(&rejected).unwrap(),
            serde_json::to_string(&record).unwrap(),
            serde_json::to_string(&record).unwrap()
        );
        let (records, report) = parse_network_records(&jsonl, NetworkLoadMode::Lenient).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(report.accepted, 2);
        assert_eq!(report.qualified, 1);
        assert_eq!(report.explicitly_rejected, 1);
        assert_eq!(report.duplicates_collapsed, 1);
        assert_eq!(report.rejected.len(), 1);
        assert_eq!(report.rejected[0].line, 3);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn strict_jsonl_import_rejects_schema_and_provenance_errors() {
        let reaction = r(&[(&[("Fe", 1.0)], 1)], &[(&[("Fe", 1.0)], 1)]);
        let valid = NetworkReactionRecord::new(
            "fixture".into(),
            "CC0-1.0".into(),
            "v1".into(),
            "r1".into(),
            reaction,
            NetworkReactionQualification::Qualified,
        )
        .unwrap();
        let mut record = serde_json::to_value(valid).unwrap();
        record["schema_version"] = serde_json::json!(99);
        let error = parse_network_records(
            &serde_json::to_string(&record).unwrap(),
            NetworkLoadMode::Strict,
        )
        .unwrap_err();
        assert!(
            matches!(error, NetworkError::MalformedRecord(reason) if reason.contains("unsupported schema version"))
        );
    }
}
