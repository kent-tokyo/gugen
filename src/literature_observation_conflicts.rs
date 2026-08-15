//! Phase 20C: cross-DOI field comparison across independent literature
//! reports of the same synthesis route -- the corpus-scale analogue of
//! Phase 19's `apply_condition_precedents` (`process.rs`), named as
//! deferred work in `literature_observations.rs`'s own module doc
//! comment. Detection and classification only: no averaging, no picking
//! a winner, no `ConditionPrecedent` promotion, no
//! `Planner`/`score_plan`/ranking connection -- this module never
//! imports from `planner.rs` or constructs a `ConditionPrecedent` at
//! all, so the non-connection is structural, not just unwired (checked
//! by a permanent regression test in
//! `tests/literature_observation_planner_invariance.rs`, alongside
//! Phase 20B's own). Even a unanimous [`CrossDoiFieldStatus::Agreement`]
//! across every independent DOI stays a reference-only signal in
//! v0.4.0; promotion is deferred to a separately-triggered future
//! "Integration" phase.
//!
//! **The comparison boundary.** Phase 20D (the manual extraction-accuracy
//! audit this design is built from) found that the dominant confirmed
//! cause of a within-paper temperature disagreement is step-segmentation
//! failure: a paper's genuine sequential or parallel multi-stage
//! treatment merged into one `HeatingOperation` by the upstream
//! (Kononova) extraction pipeline. Comparing two *different* papers'
//! heating steps by raw position alone would risk repeating that same
//! failure mode one level up -- e.g. treating a 1-step paper's only
//! firing as if it were a 2-step paper's calcination stage. So two
//! observations are only ever compared when they agree on *all* of:
//! exact target, exact precursor set, `ConventionalSolidState` route
//! family, independent DOI, total reported heating-operation count for
//! that record, and operation position within that record. Matching on
//! all of this is deliberately called *positional alignment*, never
//! "the same processing step" -- equality here is not a claim that both
//! entries describe the same real-world physical stage, only that each
//! sits at the same index within an equal-length heating sequence
//! reported for that route. See [`StepGroupKey`] and
//! [`StepGroupAssessment`].
//!
//! **DOI is the independence unit**, not the observation -- the same
//! rule Phase 20D's sampling design used and for the same reason: two
//! observations from the same paper share one extraction run over the
//! same source text, so are not independent evidence. A DOI contributing
//! more than one observation at the same alignment key (it can cover
//! multiple raw corpus records) collapses to its lowest
//! `corpus_record_index` entry, deterministically regardless of input
//! order -- the same canonicalization rule Phase 20D's sampler used.
//! Observations with `doi: None` never participate (independence can't
//! be established without an identifier).
//!
//! **Route-level step-count diversity vs. within-group agreement are
//! different facts about different things, and this module never
//! collapses one into the other.** A route where 3 independent DOIs
//! report a 1-step process agreeing on 900°C, and 4 different
//! independent DOIs report a 2-step process agreeing on 700°C/1100°C,
//! has real, useful agreement in *both* groups -- the route's step
//! *structure* being contested (`has_multiple_operation_shapes`,
//! [`RouteObservationAssessment`]) does not make either agreement fake.
//! Every [`StepGroupAssessment`] is a positional comparison conditioned
//! on its own operation shape, never a claim about "the" route.
//!
//! **`heating_operation_count`** is derived per `corpus_record_index` as
//! the number of distinct `operation_index` values that record reports
//! -- verified against the real local snapshot (13,982 observations,
//! 7,626 distinct `corpus_record_index` values, 2026-08-15) to be dense
//! `0..n` with zero exceptions, and every record's operations to share
//! identical target/precursors with zero exceptions, so this count is a
//! checked invariant, not an unverified assumption.
//!
//! **[`CrossDoiFieldStatus::SegmentationAmbiguous`] is never populated
//! by this module.** It is reserved for a narrowly-scoped future case --
//! a single observation or a step-group itself showing signs that
//! process separation failed (a `HeatingOperation` merging multiple
//! stages, a record's values not attributable to a specific heating
//! stage, `operation_index`'s meaning itself unclear) -- explicitly
//! distinct from a bare cross-DOI step-count difference, which is
//! route-level shape diversity (`has_multiple_operation_shapes`), a
//! different fact entirely. No field in [`CorpusHeatingObservation`]'s
//! current schema carries a per-observation segmentation-failure signal,
//! so no code path here can populate this variant; it exists as a
//! precisely-scoped placeholder for a schema signal that doesn't exist
//! yet, not a vague catch-all.
//!
//! **Not mechanically implementable at corpus scale, disclosed rather
//! than faked**: whether a specific record has an identity-audit
//! problem (only the 58 DOIs Phase 20D manually sampled have this label,
//! not merged into the corpus schema, not scalable to 6,370+ corpus
//! DOIs); whether a single observation's temperature and duration
//! actually come from the same experimental run (no schema signal
//! distinguishes this, inherited as-is from the raw corpus's own
//! per-operation grouping). Neither is approximated here.
//!
//! **Integration (v0.4.0)**: [`LiteratureObservationCorpusProvider`]
//! adapts this module's computation to the ungated
//! `LiteratureEvidenceProvider` trait (`provider.rs`) that `Planner`
//! consumes -- same architecture as `MaterialsProjectSnapshotProvider`
//! implementing the already-ungated `ThermodynamicProvider`. The output
//! types themselves ([`StepGroupKey`], [`SourcedValue`],
//! [`CrossDoiFieldStatus`], [`StepGroupAssessment`],
//! [`RouteObservationAssessment`]) live in the always-compiled
//! `literature_evidence` module, not here, precisely so `Planner`'s
//! report schema never changes shape depending on whether this feature
//! is enabled.

use crate::composition::Composition;
use crate::error::ProviderError;
pub use crate::literature_evidence::{
    CrossDoiFieldStatus, LiteratureRouteEvidence, RouteObservationAssessment, SourcedValue,
    StepGroupAssessment, StepGroupKey, literature_evidence_limitations,
};
use crate::literature_observations::{CorpusHeatingObservation, LiteratureObservationCorpus};
use crate::process::{Atmosphere, RouteFamily};
use crate::provider::LiteratureEvidenceProvider;
use std::collections::{BTreeMap, BTreeSet};

type RouteKey = (Composition, BTreeSet<Composition>, RouteFamily);

/// Internal grouping key spanning the whole corpus in one pass -- see the
/// module doc comment for what equality on this key does and does not
/// mean. Not part of the public API: the public output nests
/// [`StepGroupKey`] inside [`RouteObservationAssessment`] instead, since
/// target/precursors/route_family are already fixed once per route.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct StepAlignmentKey {
    target: Composition,
    precursor_set: BTreeSet<Composition>,
    route_family: RouteFamily,
    heating_operation_count: usize,
    operation_index: usize,
}

/// Adapts [`LiteratureObservationCorpus::cross_doi_comparisons`] to the
/// ungated [`LiteratureEvidenceProvider`] trait. Computes
/// `cross_doi_comparisons()` exactly **once**, at construction time, and
/// indexes the result by route -- a per-plan `route_evidence()` call is
/// an O(log n) `BTreeMap` lookup, never a fresh corpus-wide pass. This is
/// what makes calling it once per candidate plan (as `Planner::plan`
/// does) affordable; measured for real in
/// `examples/literature_evidence_integration_report.rs`.
pub struct LiteratureObservationCorpusProvider {
    by_route: BTreeMap<RouteKey, RouteObservationAssessment>,
}

impl LiteratureObservationCorpusProvider {
    pub fn new(corpus: &LiteratureObservationCorpus) -> Self {
        let by_route = corpus
            .cross_doi_comparisons()
            .into_iter()
            .map(|assessment| {
                let key: RouteKey = (
                    assessment.target.clone(),
                    assessment.precursors.clone(),
                    assessment.route_family,
                );
                (key, assessment)
            })
            .collect();
        Self { by_route }
    }
}

impl LiteratureEvidenceProvider for LiteratureObservationCorpusProvider {
    fn route_evidence(
        &self,
        target: &Composition,
        route_family: RouteFamily,
        precursors: &[Composition],
    ) -> std::result::Result<Option<LiteratureRouteEvidence>, ProviderError> {
        // Mirrors LiteratureObservationCorpus::find_exact's own explicit
        // route-family gate -- defense in depth: even though the corpus
        // itself never contains non-ConventionalSolidState evidence
        // (Phase 20A's audit), this makes the restriction independently
        // checkable at the call site too, not just inherited silently
        // from upstream data.
        if route_family != RouteFamily::ConventionalSolidState {
            return Ok(None);
        }
        let precursor_set: BTreeSet<Composition> = precursors.iter().cloned().collect();
        let key: RouteKey = (target.clone(), precursor_set, route_family);
        Ok(self
            .by_route
            .get(&key)
            .map(|assessment| LiteratureRouteEvidence {
                limitations: literature_evidence_limitations(assessment),
                assessment: assessment.clone(),
            }))
    }
}

impl LiteratureObservationCorpus {
    /// Cross-DOI field comparison across the whole loaded corpus -- see
    /// the module doc comment for the full design. Deterministic order
    /// (sorted by route content, then by step-group key), independent of
    /// the corpus's own internal observation order.
    pub fn cross_doi_comparisons(&self) -> Vec<RouteObservationAssessment> {
        cross_doi_comparisons(self.observations())
    }
}

fn cross_doi_comparisons(
    observations: &[CorpusHeatingObservation],
) -> Vec<RouteObservationAssessment> {
    let mut record_operation_indices: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();
    for obs in observations {
        record_operation_indices
            .entry(obs.corpus_record_index)
            .or_default()
            .insert(obs.operation_index);
    }
    let record_operation_count: BTreeMap<usize, usize> = record_operation_indices
        .iter()
        .map(|(record, indices)| (*record, indices.len()))
        .collect();

    let doi_observations: Vec<&CorpusHeatingObservation> =
        observations.iter().filter(|o| o.doi.is_some()).collect();

    // Every distinct heating_operation_count reported for a route, one
    // vote per *independent DOI* -- independent of whether any given
    // shape ends up with enough replication for a listed step group.
    // Each DOI is canonicalized to its lowest-corpus_record_index shape
    // first (same tie-break as the step-group buckets below), so a
    // single DOI covering two records with different shapes for the
    // same route contributes exactly one shape here, not two -- that is
    // a within-paper artifact, not independent DOIs disagreeing, and
    // must never be able to set `has_multiple_operation_shapes` on its
    // own.
    let mut route_doi_shape: BTreeMap<RouteKey, BTreeMap<String, (usize, usize)>> = BTreeMap::new();
    for obs in &doi_observations {
        let route_key: RouteKey = (obs.target.clone(), obs.precursors.clone(), obs.route_family);
        let doi = obs.doi.clone().expect("filtered to Some above");
        let count = record_operation_count[&obs.corpus_record_index];
        let dois = route_doi_shape.entry(route_key).or_default();
        match dois.get(&doi) {
            Some(&(existing_record, _)) if existing_record <= obs.corpus_record_index => {}
            _ => {
                dois.insert(doi, (obs.corpus_record_index, count));
            }
        }
    }
    let route_operation_shapes: BTreeMap<RouteKey, BTreeSet<usize>> = route_doi_shape
        .into_iter()
        .map(|(route_key, dois)| {
            let counts: BTreeSet<usize> = dois.into_values().map(|(_, count)| count).collect();
            (route_key, counts)
        })
        .collect();

    // StepAlignmentKey -> doi -> canonical observation (lowest
    // corpus_record_index wins, order-independently: each insertion only
    // replaces the current holder if strictly better, so the converged
    // result never depends on processing order).
    let mut buckets: BTreeMap<StepAlignmentKey, BTreeMap<String, &CorpusHeatingObservation>> =
        BTreeMap::new();
    for obs in &doi_observations {
        let key = StepAlignmentKey {
            target: obs.target.clone(),
            precursor_set: obs.precursors.clone(),
            route_family: obs.route_family,
            heating_operation_count: record_operation_count[&obs.corpus_record_index],
            operation_index: obs.operation_index,
        };
        let doi = obs.doi.clone().expect("filtered to Some above");
        let bucket = buckets.entry(key).or_default();
        let replace = match bucket.get(&doi) {
            Some(existing) => obs.corpus_record_index < existing.corpus_record_index,
            None => true,
        };
        if replace {
            bucket.insert(doi, obs);
        }
    }

    let mut per_route: BTreeMap<RouteKey, Vec<StepGroupAssessment>> = BTreeMap::new();
    for (key, doi_map) in buckets {
        if doi_map.len() < 2 {
            continue;
        }
        let mut candidates: Vec<(String, &CorpusHeatingObservation)> =
            doi_map.into_iter().collect();
        candidates.sort_by(|a, b| a.0.cmp(&b.0));
        let source_dois: Vec<String> = candidates.iter().map(|(doi, _)| doi.clone()).collect();

        let temperature = field_status(
            candidates
                .iter()
                .filter_map(|(doi, obs)| obs.temperature.map(|t| (doi.clone(), t)))
                .collect(),
        );
        let duration = field_status(
            candidates
                .iter()
                .filter_map(|(doi, obs)| obs.duration.map(|d| (doi.clone(), d)))
                .collect(),
        );
        // Only the six structured Atmosphere variants contribute -- a
        // Controlled { description } free-text value is never a safe
        // agreement/conflict signal against another independently-
        // written free-text description, see the module doc comment.
        let atmosphere = field_status(
            candidates
                .iter()
                .filter_map(|(doi, obs)| match &obs.atmosphere {
                    Some(a) if !matches!(a, Atmosphere::Controlled { .. }) => {
                        Some((doi.clone(), a.clone()))
                    }
                    _ => None,
                })
                .collect(),
        );

        let route_key: RouteKey = (
            key.target.clone(),
            key.precursor_set.clone(),
            key.route_family,
        );
        per_route
            .entry(route_key)
            .or_default()
            .push(StepGroupAssessment {
                key: StepGroupKey {
                    heating_operation_count: key.heating_operation_count,
                    operation_index: key.operation_index,
                },
                source_dois,
                temperature,
                duration,
                atmosphere,
            });
    }

    per_route
        .into_iter()
        .map(|((target, precursors, route_family), step_groups)| {
            let observed =
                &route_operation_shapes[&(target.clone(), precursors.clone(), route_family)];
            RouteObservationAssessment {
                target,
                precursors,
                route_family,
                has_multiple_operation_shapes: observed.len() >= 2,
                observed_operation_counts: observed.iter().copied().collect(),
                step_groups,
            }
        })
        .collect()
}

/// `candidates` is one `(doi, value)` pair per distinct contributing DOI,
/// already sorted by DOI -- see [`CrossDoiFieldStatus`]'s doc comment
/// for why a lone value is `InsufficientIndependentSources`, not
/// resolved.
fn field_status<T: Clone + PartialEq>(candidates: Vec<(String, T)>) -> CrossDoiFieldStatus<T> {
    match candidates.len() {
        0 => CrossDoiFieldStatus::Unresolved,
        1 => CrossDoiFieldStatus::InsufficientIndependentSources,
        _ => {
            let mut distinct: Vec<(T, Vec<String>)> = Vec::new();
            for (doi, value) in candidates {
                match distinct.iter_mut().find(|(v, _)| *v == value) {
                    Some((_, dois)) => dois.push(doi),
                    None => distinct.push((value, vec![doi])),
                }
            }
            if distinct.len() == 1 {
                let (value, source_dois) = distinct.into_iter().next().expect("checked len == 1");
                CrossDoiFieldStatus::Agreement { value, source_dois }
            } else {
                CrossDoiFieldStatus::Conflict {
                    values: distinct
                        .into_iter()
                        .map(|(value, dois)| SourcedValue {
                            value,
                            doi: dois
                                .into_iter()
                                .next()
                                .expect("each distinct value has >=1 doi"),
                        })
                        .collect(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{DurationRange, TemperatureRange};

    fn element(symbol: &str) -> crate::composition::Element {
        crate::composition::Element::new(symbol).unwrap()
    }

    fn composition(pairs: &[(&str, f64)]) -> Composition {
        Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
    }

    type Conditions = (
        Option<TemperatureRange>,
        Option<DurationRange>,
        Option<Atmosphere>,
    );

    fn obs(
        corpus_record_index: usize,
        operation_index: usize,
        doi: &str,
        target: Composition,
        precursors: &[Composition],
        conditions: Conditions,
    ) -> CorpusHeatingObservation {
        let (temperature, duration, atmosphere) = conditions;
        CorpusHeatingObservation {
            target,
            precursors: precursors.iter().cloned().collect(),
            route_family: RouteFamily::ConventionalSolidState,
            heating_purpose: None,
            operation_index,
            temperature,
            duration,
            atmosphere,
            doi: Some(doi.to_string()),
            corpus_record_index,
        }
    }

    /// The common case in this test module: only a temperature, no
    /// duration/atmosphere.
    fn obs_temp(
        corpus_record_index: usize,
        operation_index: usize,
        doi: &str,
        target: Composition,
        precursors: &[Composition],
        temperature: TemperatureRange,
    ) -> CorpusHeatingObservation {
        obs(
            corpus_record_index,
            operation_index,
            doi,
            target,
            precursors,
            (Some(temperature), None, None),
        )
    }

    fn bto() -> (Composition, Vec<Composition>) {
        (
            composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
            vec![
                composition(&[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
                composition(&[("Ti", 1.0), ("O", 2.0)]),
            ],
        )
    }

    fn temp(c: f64) -> TemperatureRange {
        TemperatureRange::new(c, c).unwrap()
    }

    #[test]
    fn two_independent_dois_agreeing_on_a_single_step_report_agreement() {
        let (target, precursors) = bto();
        let observations = vec![
            obs_temp(0, 0, "10.1/a", target.clone(), &precursors, temp(900.0)),
            obs_temp(1, 0, "10.1/b", target, &precursors, temp(900.0)),
        ];
        let result = cross_doi_comparisons(&observations);
        assert_eq!(result.len(), 1);
        let route = &result[0];
        assert!(!route.has_multiple_operation_shapes);
        assert_eq!(route.observed_operation_counts, vec![1]);
        assert_eq!(route.step_groups.len(), 1);
        assert_eq!(
            route.step_groups[0].temperature,
            CrossDoiFieldStatus::Agreement {
                value: temp(900.0),
                source_dois: vec!["10.1/a".to_string(), "10.1/b".to_string()],
            }
        );
    }

    #[test]
    fn two_independent_dois_disagreeing_report_conflict() {
        let (target, precursors) = bto();
        let observations = vec![
            obs_temp(0, 0, "10.1/a", target.clone(), &precursors, temp(900.0)),
            obs_temp(1, 0, "10.1/b", target, &precursors, temp(950.0)),
        ];
        let result = cross_doi_comparisons(&observations);
        match &result[0].step_groups[0].temperature {
            CrossDoiFieldStatus::Conflict { values } => {
                assert_eq!(values.len(), 2);
            }
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn a_single_doi_never_produces_a_step_group() {
        let (target, precursors) = bto();
        let observations = vec![obs_temp(0, 0, "10.1/a", target, &precursors, temp(900.0))];
        assert_eq!(cross_doi_comparisons(&observations), vec![]);
    }

    #[test]
    fn same_doi_reporting_twice_at_the_same_key_is_not_independent_replication() {
        let (target, precursors) = bto();
        let observations = vec![
            obs_temp(0, 0, "10.1/a", target.clone(), &precursors, temp(900.0)),
            obs_temp(5, 0, "10.1/a", target, &precursors, temp(950.0)),
        ];
        // Both entries share DOI 10.1/a -- only one independent DOI total,
        // so this must never produce a step group at all, regardless of
        // the two entries' conflicting temperatures.
        assert_eq!(cross_doi_comparisons(&observations), vec![]);
    }

    #[test]
    fn same_doi_dedup_keeps_the_lowest_corpus_record_index_order_independently() {
        let (target, precursors) = bto();
        let a_low = obs_temp(0, 0, "10.1/a", target.clone(), &precursors, temp(900.0));
        let a_high = obs_temp(7, 0, "10.1/a", target.clone(), &precursors, temp(999.0));
        let b = obs_temp(1, 0, "10.1/b", target, &precursors, temp(900.0));

        let forward = cross_doi_comparisons(&[a_low.clone(), a_high.clone(), b.clone()]);
        let reversed = cross_doi_comparisons(&[b, a_high, a_low]);
        assert_eq!(forward, reversed);
        assert_eq!(
            forward[0].step_groups[0].temperature,
            CrossDoiFieldStatus::Agreement {
                value: temp(900.0),
                source_dois: vec!["10.1/a".to_string(), "10.1/b".to_string()],
            },
            "the lower corpus_record_index entry (900.0) must win, not the higher one (999.0)"
        );
    }

    #[test]
    fn different_operation_counts_never_get_compared_even_at_the_same_index() {
        let (target, precursors) = bto();
        // corpus_record_index 0: DOI a, a single-step record (count=1).
        let one_step = obs_temp(0, 0, "10.1/a", target.clone(), &precursors, temp(1200.0));
        // corpus_record_index 1: DOI b, a two-step record (count=2) --
        // its own index 0 and index 1.
        let two_step_0 = obs_temp(1, 0, "10.1/b", target.clone(), &precursors, temp(900.0));
        let two_step_1 = obs_temp(1, 1, "10.1/b", target.clone(), &precursors, temp(1300.0));
        // corpus_record_index 2: DOI c, another single-step record.
        let one_step_c = obs_temp(2, 0, "10.1/c", target, &precursors, temp(1250.0));

        let result = cross_doi_comparisons(&[one_step, two_step_0, two_step_1, one_step_c]);
        assert_eq!(result.len(), 1);
        let route = &result[0];
        assert!(route.has_multiple_operation_shapes);
        assert_eq!(route.observed_operation_counts, vec![1, 2]);
        // Only the count=1 shape has 2+ independent DOIs (a and c); the
        // count=2 shape has only DOI b, so it never produces a step
        // group -- the two shapes' temperatures (1200/1250 vs. 900/1300)
        // must never be compared against each other.
        assert_eq!(route.step_groups.len(), 1);
        assert_eq!(route.step_groups[0].key.heating_operation_count, 1);
        match &route.step_groups[0].temperature {
            CrossDoiFieldStatus::Conflict { values } => assert_eq!(values.len(), 2),
            other => panic!("expected Conflict among the two 1-step DOIs, got {other:?}"),
        }
    }

    #[test]
    fn a_single_dois_own_shape_disagreement_never_sets_the_route_flag() {
        let (target, precursors) = bto();
        // DOI "a" alone reports this route two different ways: a 1-step
        // record (corpus_record_index 0) and a 2-step record
        // (corpus_record_index 1) -- a within-paper artifact, not
        // independent evidence of route-level shape diversity.
        let a_one_step = obs_temp(0, 0, "10.1/a", target.clone(), &precursors, temp(900.0));
        let a_two_step_0 = obs_temp(1, 0, "10.1/a", target.clone(), &precursors, temp(700.0));
        let a_two_step_1 = obs_temp(1, 1, "10.1/a", target.clone(), &precursors, temp(1100.0));
        // A second, independent DOI agreeing with "a"'s canonical
        // (lowest corpus_record_index) 1-step shape.
        let b = obs_temp(2, 0, "10.1/b", target, &precursors, temp(900.0));

        let result = cross_doi_comparisons(&[a_one_step, a_two_step_0, a_two_step_1, b]);
        assert_eq!(result.len(), 1);
        let route = &result[0];
        assert!(
            !route.has_multiple_operation_shapes,
            "a single DOI's own internal shape disagreement must never set this flag"
        );
        assert_eq!(route.observed_operation_counts, vec![1]);
    }

    #[test]
    fn within_shape_agreement_survives_even_when_the_route_has_multiple_shapes() {
        let (target, precursors) = bto();
        // 1-step shape: 3 independent DOIs agreeing on 900.0.
        let one_step: Vec<CorpusHeatingObservation> = ["10.1/a", "10.1/b", "10.1/c"]
            .iter()
            .enumerate()
            .map(|(i, doi)| obs_temp(i, 0, doi, target.clone(), &precursors, temp(900.0)))
            .collect();
        // 2-step shape: 2 different independent DOIs agreeing on
        // 700.0/1100.0 across their own two steps.
        let two_step: Vec<CorpusHeatingObservation> = ["10.1/d", "10.1/e"]
            .iter()
            .enumerate()
            .flat_map(|(i, doi)| {
                let record = 10 + i;
                vec![
                    obs_temp(record, 0, doi, target.clone(), &precursors, temp(700.0)),
                    obs_temp(record, 1, doi, target.clone(), &precursors, temp(1100.0)),
                ]
            })
            .collect();

        let mut all = one_step;
        all.extend(two_step);
        let result = cross_doi_comparisons(&all);
        assert_eq!(result.len(), 1);
        let route = &result[0];
        assert!(route.has_multiple_operation_shapes);
        assert_eq!(route.observed_operation_counts, vec![1, 2]);
        assert_eq!(
            route.step_groups.len(),
            3,
            "1-step group + 2 steps of the 2-step group"
        );

        let one_step_group = route
            .step_groups
            .iter()
            .find(|g| g.key.heating_operation_count == 1)
            .unwrap();
        assert_eq!(
            one_step_group.temperature,
            CrossDoiFieldStatus::Agreement {
                value: temp(900.0),
                source_dois: vec![
                    "10.1/a".to_string(),
                    "10.1/b".to_string(),
                    "10.1/c".to_string()
                ],
            },
            "within-shape agreement must survive unchanged despite the route's shape diversity"
        );
    }

    #[test]
    fn controlled_atmosphere_free_text_never_contributes_a_candidate() {
        let (target, precursors) = bto();
        let observations = vec![
            obs(
                0,
                0,
                "10.1/a",
                target.clone(),
                &precursors,
                (
                    None,
                    None,
                    Some(Atmosphere::Controlled {
                        description: "flowing gas mixture".to_string(),
                    }),
                ),
            ),
            obs(
                1,
                0,
                "10.1/b",
                target,
                &precursors,
                (
                    None,
                    None,
                    Some(Atmosphere::Controlled {
                        description: "flowing gas mixture".to_string(),
                    }),
                ),
            ),
        ];
        // Two identical-looking free-text strings must still not be
        // treated as agreement -- see the module doc comment.
        let result = cross_doi_comparisons(&observations);
        assert_eq!(
            result[0].step_groups[0].atmosphere,
            CrossDoiFieldStatus::Unresolved
        );
    }

    #[test]
    fn structured_atmosphere_variants_do_compare() {
        let (target, precursors) = bto();
        let observations = vec![
            obs(
                0,
                0,
                "10.1/a",
                target.clone(),
                &precursors,
                (None, None, Some(Atmosphere::Air)),
            ),
            obs(
                1,
                0,
                "10.1/b",
                target,
                &precursors,
                (None, None, Some(Atmosphere::Air)),
            ),
        ];
        let result = cross_doi_comparisons(&observations);
        assert_eq!(
            result[0].step_groups[0].atmosphere,
            CrossDoiFieldStatus::Agreement {
                value: Atmosphere::Air,
                source_dois: vec!["10.1/a".to_string(), "10.1/b".to_string()],
            }
        );
    }

    #[test]
    fn exactly_one_doi_reporting_a_field_is_insufficient_not_resolved() {
        let (target, precursors) = bto();
        let observations = vec![
            obs_temp(0, 0, "10.1/a", target.clone(), &precursors, temp(900.0)),
            // Same shape/position, independent DOI, but no temperature
            // reported at all -- only 1 of the 2 independent DOIs in
            // this group actually has a temperature value.
            obs(1, 0, "10.1/b", target, &precursors, (None, None, None)),
        ];
        let result = cross_doi_comparisons(&observations);
        assert_eq!(
            result[0].step_groups[0].temperature,
            CrossDoiFieldStatus::InsufficientIndependentSources
        );
    }

    #[test]
    fn observations_missing_a_doi_never_participate() {
        let (target, precursors) = bto();
        let mut with_doi = obs_temp(0, 0, "10.1/a", target.clone(), &precursors, temp(900.0));
        let mut no_doi = obs_temp(1, 0, "10.1/b", target, &precursors, temp(900.0));
        no_doi.doi = None;
        with_doi.doi = Some("10.1/a".to_string());
        assert_eq!(cross_doi_comparisons(&[with_doi, no_doi]), vec![]);
    }

    #[test]
    fn order_independence_across_a_non_trivial_permutation() {
        let (target, precursors) = bto();
        let dois = ["10.1/a", "10.1/b", "10.1/c", "10.1/d", "10.1/e"];
        let temps = [900.0, 900.0, 950.0, 900.0, 950.0];
        let observations: Vec<CorpusHeatingObservation> = dois
            .iter()
            .zip(temps.iter())
            .enumerate()
            .map(|(i, (doi, t))| obs_temp(i, 0, doi, target.clone(), &precursors, temp(*t)))
            .collect();

        let forward = cross_doi_comparisons(&observations);

        let reversed: Vec<CorpusHeatingObservation> = observations.iter().rev().cloned().collect();
        assert_eq!(cross_doi_comparisons(&reversed), forward);

        // A non-trivial permutation, not just a reversal.
        let shuffled: Vec<CorpusHeatingObservation> = [3usize, 0, 4, 1, 2]
            .iter()
            .map(|&i| observations[i].clone())
            .collect();
        assert_eq!(cross_doi_comparisons(&shuffled), forward);
    }
}
