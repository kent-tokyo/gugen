//! Phase 26: reference-only prior-experiment evidence -- surfaces Phase
//! 25's [`SynthesisExecutionRecord`]s during planning, matched on a
//! plan's exact (target, canonical precursor set, route family). Same
//! "reference-only, by construction" boundary `literature_evidence.rs`/
//! `commercial_catalog.rs` already establish: nothing here is ever
//! passed to `score_plan`, and [`PriorExperimentEvidence`] is attached to
//! `SynthesisPlan` as its own field, never folded into `evidence`,
//! `condition_conflicts`, or any other `score_plan` input.
//!
//! **Ungated -- no new Cargo feature.** `execution_record.rs`'s plain
//! types carry no feature gate either (only its JSON-parsing functions
//! do, behind `serde`), so neither this module's types nor
//! [`InMemoryExecutionRecordProvider`] need one. This is a real
//! divergence from `LiteratureEvidenceProvider`'s own split (trait and
//! report types always compiled, but its one real implementation lives
//! behind `literature_corpus` because *that* implementation's corpus
//! loader needs it) -- here, the "corpus" is just a caller-supplied
//! `Vec<SynthesisExecutionRecord>`, already parsed by the caller via
//! Phase 25's own `parse_execution_records` before it ever reaches this
//! module.

use crate::composition::Composition;
use crate::error::ProviderError;
use crate::execution_record::{SynthesisExecutionRecord, SynthesisOutcome};
use crate::process::RouteFamily;
use crate::provider::PriorExperimentEvidenceProvider;
use std::collections::{BTreeMap, BTreeSet};

/// Every [`SynthesisExecutionRecord`] whose `plan_identity` matches one
/// plan's exact (target, canonical precursor set, route family) --
/// `Planner` attaches this to `SynthesisPlan` when a
/// [`PriorExperimentEvidenceProvider`] is configured and finds a match.
/// Records are kept in whatever order the provider returned them (an
/// append-only log's natural order is chronological) -- never sorted or
/// filtered here. Process conditions, selected commercial offers, and
/// catalog provenance differ freely between records and are shown as-is,
/// not compared against each other or against this plan's own (usually
/// unresolved) conditions -- see [`Self::outcome_tally`]'s own doc
/// comment for why this is never a success rate.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PriorExperimentEvidence {
    pub records: Vec<SynthesisExecutionRecord>,
}

impl PriorExperimentEvidence {
    /// Grouped by outcome, in `SynthesisOutcome`'s own declared variant
    /// order (via an internal `BTreeMap`, since `SynthesisOutcome`
    /// derives `Ord`) -- e.g. the owner's own example renders as
    /// `[(TargetPhaseObtained, 2), (CompetingPhaseObserved, 1),
    /// (Inconclusive, 1)]`.
    ///
    /// **Not a success rate.** These records are matched only on
    /// target/precursor-set/route-family identity -- their actual
    /// process conditions, grades, and catalogs are not required to
    /// agree with each other or with this plan's own, so a count of past
    /// outcomes describes what was recorded, never a probability of what
    /// a *new* attempt would produce.
    pub fn outcome_tally(&self) -> Vec<(SynthesisOutcome, usize)> {
        let mut counts: BTreeMap<SynthesisOutcome, usize> = BTreeMap::new();
        for record in &self.records {
            *counts.entry(record.outcome).or_insert(0) += 1;
        }
        counts.into_iter().collect()
    }
}

/// The one real [`PriorExperimentEvidenceProvider`] implementation this
/// crate ships: an in-memory index over a caller-supplied
/// `Vec<SynthesisExecutionRecord>` (already parsed by the caller, e.g.
/// via `parse_execution_records` -- this type performs no file I/O and
/// no JSON parsing itself). Mirrors
/// `LiteratureObservationCorpusProvider`'s own architecture: grouped
/// once at construction into a `BTreeMap` keyed by the exact identity
/// triple (`Composition` doesn't derive `Hash`, so `BTreeMap`, not
/// `HashMap`, is required here, same constraint that provider has).
pub struct InMemoryExecutionRecordProvider {
    by_identity:
        BTreeMap<(Composition, BTreeSet<Composition>, RouteFamily), Vec<SynthesisExecutionRecord>>,
}

impl InMemoryExecutionRecordProvider {
    pub fn new(records: Vec<SynthesisExecutionRecord>) -> Self {
        let mut by_identity: BTreeMap<
            (Composition, BTreeSet<Composition>, RouteFamily),
            Vec<SynthesisExecutionRecord>,
        > = BTreeMap::new();
        for record in records {
            let key = (
                record.plan_identity.target_composition.clone(),
                record.plan_identity.precursor_compositions.clone(),
                record.plan_identity.route_family,
            );
            by_identity.entry(key).or_default().push(record);
        }
        Self { by_identity }
    }
}

impl PriorExperimentEvidenceProvider for InMemoryExecutionRecordProvider {
    fn prior_experiments(
        &self,
        target: &Composition,
        route_family: RouteFamily,
        precursors: &[Composition],
    ) -> std::result::Result<Option<PriorExperimentEvidence>, ProviderError> {
        let key = (
            target.clone(),
            precursors.iter().cloned().collect::<BTreeSet<_>>(),
            route_family,
        );
        Ok(self
            .by_identity
            .get(&key)
            .map(|records| PriorExperimentEvidence {
                records: records.clone(),
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composition::Element;
    use crate::execution_record::EXECUTION_RECORD_SCHEMA_VERSION;
    use crate::execution_record::{ExecutionCharacterization, ExecutionProvenance, PlanIdentity};
    use crate::report::PlanId;

    fn element(symbol: &str) -> Element {
        Element::new(symbol).unwrap()
    }

    fn composition(pairs: &[(&str, f64)]) -> Composition {
        Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
    }

    fn record(
        plan_id: &str,
        target: Composition,
        precursors: &[Composition],
        route_family: RouteFamily,
        outcome: SynthesisOutcome,
    ) -> SynthesisExecutionRecord {
        SynthesisExecutionRecord {
            schema_version: EXECUTION_RECORD_SCHEMA_VERSION.to_string(),
            plan_identity: PlanIdentity {
                plan_id: PlanId(plan_id.to_string()),
                route_family,
                target_composition: target,
                precursor_compositions: precursors.iter().cloned().collect(),
            },
            commercial_catalog_source: None,
            selected_commercial_offers: Vec::new(),
            actual_precursor_amounts: Vec::new(),
            actual_process_conditions: Vec::new(),
            deviations_from_plan: Vec::new(),
            outcome,
            characterization: ExecutionCharacterization {
                phase_purity_fraction: None,
                yield_fraction: None,
                xrd_reference: None,
                measurement_method: None,
            },
            operator_notes: None,
            experiment_date: None,
            batch_id: None,
            provenance: ExecutionProvenance {
                gugen_version: "0.0.0-test".to_string(),
                recorded_by: None,
                recorded_at: "2026-08-23T00:00:00Z".to_string(),
            },
        }
    }

    #[test]
    fn outcome_tally_matches_owners_example_grouping() {
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let precursors = vec![
            composition(&[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            composition(&[("Ti", 1.0), ("O", 2.0)]),
        ];
        let evidence = PriorExperimentEvidence {
            records: vec![
                record(
                    "a",
                    target.clone(),
                    &precursors,
                    RouteFamily::ConventionalSolidState,
                    SynthesisOutcome::TargetPhaseObtained,
                ),
                record(
                    "b",
                    target.clone(),
                    &precursors,
                    RouteFamily::ConventionalSolidState,
                    SynthesisOutcome::TargetPhaseObtained,
                ),
                record(
                    "c",
                    target.clone(),
                    &precursors,
                    RouteFamily::ConventionalSolidState,
                    SynthesisOutcome::CompetingPhaseObserved,
                ),
                record(
                    "d",
                    target,
                    &precursors,
                    RouteFamily::ConventionalSolidState,
                    SynthesisOutcome::Inconclusive,
                ),
            ],
        };
        assert_eq!(
            evidence.outcome_tally(),
            vec![
                (SynthesisOutcome::TargetPhaseObtained, 2),
                (SynthesisOutcome::CompetingPhaseObserved, 1),
                (SynthesisOutcome::Inconclusive, 1),
            ]
        );
    }

    #[test]
    fn in_memory_provider_matches_on_exact_identity() {
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let other_target = composition(&[("Na", 1.0), ("Cl", 1.0)]);
        let precursors = vec![
            composition(&[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            composition(&[("Ti", 1.0), ("O", 2.0)]),
        ];
        let other_precursors = vec![composition(&[("Ba", 1.0), ("O", 1.0)])];

        let provider = InMemoryExecutionRecordProvider::new(vec![record(
            "a",
            target.clone(),
            &precursors,
            RouteFamily::ConventionalSolidState,
            SynthesisOutcome::TargetPhaseObtained,
        )]);

        assert!(
            provider
                .prior_experiments(&target, RouteFamily::ConventionalSolidState, &precursors)
                .unwrap()
                .is_some()
        );
        assert!(
            provider
                .prior_experiments(
                    &other_target,
                    RouteFamily::ConventionalSolidState,
                    &precursors
                )
                .unwrap()
                .is_none(),
            "a different target must not match"
        );
        assert!(
            provider
                .prior_experiments(
                    &target,
                    RouteFamily::ConventionalSolidState,
                    &other_precursors
                )
                .unwrap()
                .is_none(),
            "a different precursor set must not match"
        );
        assert!(
            provider
                .prior_experiments(&target, RouteFamily::Mechanochemical, &precursors)
                .unwrap()
                .is_none(),
            "a different route family must not match"
        );
    }

    #[test]
    fn in_memory_provider_never_returns_an_empty_some() {
        let provider = InMemoryExecutionRecordProvider::new(Vec::new());
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let result = provider
            .prior_experiments(&target, RouteFamily::ConventionalSolidState, &[])
            .unwrap();
        assert!(result.is_none());
    }
}
