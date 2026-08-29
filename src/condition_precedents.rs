use crate::evidence::{EvidenceKind, EvidenceScope, EvidenceStrength, PlanningEvidence};
use crate::process::{
    Atmosphere, DurationRange, HeatingPurpose, PlannedStep, ProcessStep, RampRateRange,
    TemperatureRange,
};
use std::collections::BTreeMap;

/// `ProcessEvidenceProvider` output (AGENTS.md §8). `description` is free
/// text with no structure -- still valid on its own for a provider that
/// only has prose precedent to offer. `conditions` (Phase 10) carries
/// structured, per-purpose temperature/duration/atmosphere/ramp data, each
/// entry traceable to its own citation; empty for a prose-only precedent.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProcessPrecedent {
    pub description: String,
    pub conditions: Vec<ConditionPrecedent>,
}

/// One provider's structured, citable evidence for how a specific `Heat`
/// step's conditions should be resolved (Phase 10; AGENTS.md §7/§21.3).
/// Every field the provider doesn't actually have real, sourced data for
/// stays `None` -- never fabricated to fill a gap. `evidence_kind`,
/// `strength`, and `source_id` are set by whichever provider returns this,
/// not assumed by the planner: `ProcessEvidenceProvider` is also the trait
/// a user-supplied lab-precedent source implements
/// (`EvidenceKind::UserProvidedPrecedent`), so a curated-literature-only
/// assumption in the planner would mislabel provenance for every other
/// kind of implementation.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConditionPrecedent {
    pub purpose: HeatingPurpose,
    pub temperature: Option<TemperatureRange>,
    pub duration: Option<DurationRange>,
    pub atmosphere: Option<Atmosphere>,
    pub ramp: Option<RampRateRange>,
    pub evidence_kind: EvidenceKind,
    pub source_id: Option<String>,
    pub statement: String,
    pub strength: EvidenceStrength,
    pub applicable_to: EvidenceScope,
}

/// The four `ConditionConflict.field`/`format_conflict_reason` literals
/// this module produces, defined once so `score.rs`'s consumer-side
/// lookup (`condition_conflicts.iter().find(|c| c.field == field)`)
/// can't silently drift out of sync with the producer side here -- a
/// rename on either side becomes a compile error instead of a silent
/// fallback to a generic reason. Only `"temperature"` had a dedicated
/// regression test pinning this agreement before; the other three
/// relied on the two sides happening to stay textually identical.
pub(crate) const CONDITION_FIELD_TEMPERATURE: &str = "temperature";
pub(crate) const CONDITION_FIELD_DURATION: &str = "duration";
pub(crate) const CONDITION_FIELD_ATMOSPHERE: &str = "atmosphere";
pub(crate) const CONDITION_FIELD_RAMP_RATE: &str = "ramp rate";

/// One `Heat` step field where two or more matching `ConditionPrecedent`s
/// disagreed, so it was deliberately left unresolved rather than picking
/// one arbitrarily or averaging (Phase 19 -- the owner's explicit
/// "架空の平均値を作らず未解決として示す" directive). `step_index` is
/// this field's position in the `steps` slice `apply_condition_precedents`
/// was called with, so callers can attach `reason` to the right
/// `UnresolvedRequirement`. Disagreement is exact-value inequality only
/// (`PartialEq`) -- an overlapping-but-not-identical range (e.g. a point
/// value inside a wider reported range) still counts as a conflict, the
/// conservative reading, since `TemperatureRange`/`DurationRange`/
/// `RampRateRange` have no overlap/subsumption semantics and inventing
/// one is explicitly out of scope for this phase.
#[derive(Debug, Clone, PartialEq)]
pub struct ConditionConflict {
    pub step_index: usize,
    pub field: &'static str,
    pub reason: String,
}

/// Every matching precedent's value for one field, deduplicated by exact
/// equality. Distinguishing "no data" / "one agreed value" / "conflicting
/// values" is the whole point -- `Vec<(T, usize)>`'s length after
/// deduplication is the signal, not a side effect.
enum FieldResolution<T> {
    /// `.1` is which entries in the step's matching-precedent list (by
    /// index into that list, not into the caller's whole `precedents`
    /// slice) supplied this value -- possibly more than one, if two
    /// precedents happen to agree.
    Resolved(T, Vec<usize>),
    /// One `(value, source_id)` per distinct value found, in the order
    /// first encountered (deterministic: `precedents`' own order, not
    /// insertion into a hash structure).
    Conflict(Vec<(T, Option<String>)>),
}

fn resolve_field<T: PartialEq + Clone>(
    candidates: impl Iterator<Item = (usize, T, Option<String>)>,
) -> Option<FieldResolution<T>> {
    let mut distinct: Vec<(T, Vec<usize>, Option<String>)> = Vec::new();
    for (idx, value, source_id) in candidates {
        match distinct.iter_mut().find(|(v, _, _)| *v == value) {
            Some(entry) => entry.1.push(idx),
            None => distinct.push((value, vec![idx], source_id)),
        }
    }
    if distinct.is_empty() {
        return None;
    }
    if distinct.len() == 1 {
        let (value, idxs, _) = distinct.into_iter().next().expect("checked len == 1");
        return Some(FieldResolution::Resolved(value, idxs));
    }
    Some(FieldResolution::Conflict(
        distinct
            .into_iter()
            .map(|(value, _, source_id)| (value, source_id))
            .collect(),
    ))
}

fn format_conflict_reason<T: std::fmt::Debug>(
    field: &str,
    values: &[(T, Option<String>)],
) -> String {
    let sources: Vec<String> = values
        .iter()
        .map(|(v, source_id)| {
            let cited = source_id.as_deref().unwrap_or("uncited");
            format!("{v:?} ({cited})")
        })
        .collect();
    format!(
        "{} matching literature precedents disagree on {field}: {} -- left unresolved rather \
        than picking one or averaging",
        sources.len(),
        sources.join(" vs. "),
    )
}

/// Splices provider-supplied, cited condition data into `steps`'s `Heat`
/// fields (Phase 10). Only ever fills an already-`None` slot -- never
/// overwrites a field some other resolution source already set -- so this
/// composes with any future resolution source rather than one silently
/// clobbering another. Returns one `PlanningEvidence` entry per `Heat` step
/// a precedent actually changed, carrying that precedent's own
/// `evidence_kind`/`strength`/`source_id`/`applicable_to` rather than a
/// value this function invents.
///
/// Order-independent (Phase 19): every matching precedent for a step's
/// purpose is evaluated against that field's *original* pre-call state,
/// never against a state some earlier precedent in `precedents` already
/// mutated -- so which precedent happens to come first in the slice can
/// no longer silently decide the outcome. Field-granular: precedents
/// agreeing on `temperature` but disagreeing on `duration` still resolve
/// `temperature`; only `duration` is left unresolved (per the owner's
/// explicit choice over discarding the whole precedent).
pub(crate) fn apply_condition_precedents(
    steps: &mut [PlannedStep],
    precedents: &[ConditionPrecedent],
) -> (Vec<PlanningEvidence>, Vec<ConditionConflict>) {
    let mut evidence = Vec::new();
    let mut conflicts = Vec::new();

    for (step_index, planned) in steps.iter_mut().enumerate() {
        let ProcessStep::Heat {
            purpose,
            temperature,
            duration,
            atmosphere,
            ramp,
        } = &mut planned.step
        else {
            continue;
        };
        let matching: Vec<&ConditionPrecedent> = precedents
            .iter()
            .filter(|p| p.purpose == *purpose)
            .collect();
        if matching.is_empty() {
            continue;
        }

        // Which fields each precedent (by its index into `matching`)
        // actually contributed to a successful resolution on this step --
        // built up per field below, then turned into one evidence entry
        // per contributing precedent afterward, matching the pre-Phase-19
        // "one entry per (step, precedent), fields joined by /" shape.
        let mut contributed: BTreeMap<usize, Vec<&'static str>> = BTreeMap::new();

        if temperature.is_none() {
            let candidates = matching
                .iter()
                .enumerate()
                .filter_map(|(i, p)| p.temperature.map(|t| (i, t, p.source_id.clone())));
            match resolve_field(candidates) {
                Some(FieldResolution::Resolved(value, idxs)) => {
                    *temperature = Some(value);
                    for i in idxs {
                        contributed
                            .entry(i)
                            .or_default()
                            .push(CONDITION_FIELD_TEMPERATURE);
                    }
                }
                Some(FieldResolution::Conflict(values)) => conflicts.push(ConditionConflict {
                    step_index,
                    field: CONDITION_FIELD_TEMPERATURE,
                    reason: format_conflict_reason(CONDITION_FIELD_TEMPERATURE, &values),
                }),
                None => {}
            }
        }
        if duration.is_none() {
            let candidates = matching
                .iter()
                .enumerate()
                .filter_map(|(i, p)| p.duration.map(|d| (i, d, p.source_id.clone())));
            match resolve_field(candidates) {
                Some(FieldResolution::Resolved(value, idxs)) => {
                    *duration = Some(value);
                    for i in idxs {
                        contributed
                            .entry(i)
                            .or_default()
                            .push(CONDITION_FIELD_DURATION);
                    }
                }
                Some(FieldResolution::Conflict(values)) => conflicts.push(ConditionConflict {
                    step_index,
                    field: CONDITION_FIELD_DURATION,
                    reason: format_conflict_reason(CONDITION_FIELD_DURATION, &values),
                }),
                None => {}
            }
        }
        if atmosphere.is_none() {
            let candidates = matching.iter().enumerate().filter_map(|(i, p)| {
                p.atmosphere
                    .as_ref()
                    .map(|a| (i, a.clone(), p.source_id.clone()))
            });
            match resolve_field(candidates) {
                Some(FieldResolution::Resolved(value, idxs)) => {
                    *atmosphere = Some(value);
                    for i in idxs {
                        contributed
                            .entry(i)
                            .or_default()
                            .push(CONDITION_FIELD_ATMOSPHERE);
                    }
                }
                Some(FieldResolution::Conflict(values)) => conflicts.push(ConditionConflict {
                    step_index,
                    field: CONDITION_FIELD_ATMOSPHERE,
                    reason: format_conflict_reason(CONDITION_FIELD_ATMOSPHERE, &values),
                }),
                None => {}
            }
        }
        if ramp.is_none() {
            let candidates = matching
                .iter()
                .enumerate()
                .filter_map(|(i, p)| p.ramp.map(|r| (i, r, p.source_id.clone())));
            match resolve_field(candidates) {
                Some(FieldResolution::Resolved(value, idxs)) => {
                    *ramp = Some(value);
                    for i in idxs {
                        contributed
                            .entry(i)
                            .or_default()
                            .push(CONDITION_FIELD_RAMP_RATE);
                    }
                }
                Some(FieldResolution::Conflict(values)) => conflicts.push(ConditionConflict {
                    step_index,
                    field: CONDITION_FIELD_RAMP_RATE,
                    reason: format_conflict_reason(CONDITION_FIELD_RAMP_RATE, &values),
                }),
                None => {}
            }
        }

        // `contributed` is keyed by index into `matching`, which is
        // `precedents`' own filtered order -- so iterating it directly
        // would make this step's slice of `evidence` swap order whenever
        // the caller's precedent order changes, even though the *set* of
        // fields credited to each precedent is unaffected. Sort by each
        // entry's own content (never by `precedent_idx`) so the emitted
        // order depends only on what was resolved, not on which precedent
        // the provider happened to list first. `resolved_fields.join("/")`
        // (embedded in `limitations` below) is itself already order-stable
        // -- the four field blocks above always run in fixed source order
        // (temperature/duration/atmosphere/ramp), never in precedent order.
        let mut step_evidence: Vec<PlanningEvidence> = contributed
            .into_iter()
            .map(|(precedent_idx, resolved_fields)| {
                let precedent = matching[precedent_idx];
                PlanningEvidence {
                    kind: precedent.evidence_kind,
                    source_id: precedent.source_id.clone(),
                    statement: precedent.statement.clone(),
                    strength: precedent.strength,
                    applicable_to: precedent.applicable_to,
                    limitations: vec![format!(
                        "resolved {} for the {:?} step from this precedent; other \
                        unresolved fields on this or other steps had no matching \
                        precedent data, or matching data that conflicted with another \
                        precedent",
                        resolved_fields.join("/"),
                        purpose,
                    )],
                }
            })
            .collect();
        step_evidence.sort_by(|a, b| {
            (&a.source_id, &a.statement, &a.limitations).cmp(&(
                &b.source_id,
                &b.statement,
                &b.limitations,
            ))
        });
        evidence.extend(step_evidence);
    }
    (evidence, conflicts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::StepRequirement;

    fn condition_precedent(purpose: HeatingPurpose) -> ConditionPrecedent {
        ConditionPrecedent {
            purpose,
            temperature: Some(TemperatureRange::new(900.0, 900.0).unwrap()),
            duration: Some(DurationRange::new(2.0, 2.0).unwrap()),
            atmosphere: Some(Atmosphere::Air),
            ramp: None,
            evidence_kind: EvidenceKind::CuratedLiteratureRecord,
            source_id: Some("10.0000/test".to_string()),
            statement: "test precedent".to_string(),
            strength: EvidenceStrength::Moderate,
            applicable_to: EvidenceScope::ExactTarget,
        }
    }

    /// Phase 10: only a step whose `HeatingPurpose` matches the precedent
    /// gets its fields filled; an already-resolved field is never
    /// overwritten; a step with no matching purpose is untouched.
    #[test]
    fn apply_condition_precedents_only_fills_matching_unset_fields() {
        let mut steps = vec![
            PlannedStep {
                requirement: StepRequirement::Required,
                step: ProcessStep::Heat {
                    purpose: HeatingPurpose::Calcination,
                    temperature: None,
                    duration: None,
                    atmosphere: None,
                    ramp: None,
                },
            },
            PlannedStep {
                requirement: StepRequirement::Required,
                step: ProcessStep::Heat {
                    purpose: HeatingPurpose::Sintering,
                    // Already resolved by some other source -- must survive
                    // untouched even though the precedent below also
                    // targets Sintering.
                    temperature: Some(TemperatureRange::new(1.0, 1.0).unwrap()),
                    duration: None,
                    atmosphere: None,
                    ramp: None,
                },
            },
        ];
        let precedents = vec![
            condition_precedent(HeatingPurpose::Calcination),
            condition_precedent(HeatingPurpose::Sintering),
        ];

        let (evidence, conflicts) = apply_condition_precedents(&mut steps, &precedents);
        assert!(
            conflicts.is_empty(),
            "no field had disagreeing precedents: {conflicts:?}"
        );

        let ProcessStep::Heat {
            temperature,
            duration,
            atmosphere,
            ..
        } = &steps[0].step
        else {
            panic!("expected Heat step");
        };
        assert_eq!(temperature.unwrap().min_celsius(), 900.0);
        assert_eq!(duration.unwrap().min_hours(), 2.0);
        assert!(matches!(atmosphere, Some(Atmosphere::Air)));

        let ProcessStep::Heat { temperature, .. } = &steps[1].step else {
            panic!("expected Heat step");
        };
        assert_eq!(
            temperature.unwrap().min_celsius(),
            1.0,
            "an already-resolved field must not be overwritten by a later precedent"
        );

        assert_eq!(
            evidence.len(),
            2,
            "one evidence entry per step a precedent actually changed: {evidence:?}"
        );
        for e in &evidence {
            assert_eq!(e.kind, EvidenceKind::CuratedLiteratureRecord);
            assert_eq!(e.source_id.as_deref(), Some("10.0000/test"));
        }
    }

    /// A precedent for a purpose no step has (e.g. `Annealing` when only
    /// `Calcination`/`Sintering` steps exist) must not panic or produce
    /// evidence -- it simply matches nothing.
    #[test]
    fn apply_condition_precedents_ignores_a_precedent_with_no_matching_step() {
        let mut steps = vec![PlannedStep {
            requirement: StepRequirement::Required,
            step: ProcessStep::Heat {
                purpose: HeatingPurpose::Calcination,
                temperature: None,
                duration: None,
                atmosphere: None,
                ramp: None,
            },
        }];
        let precedents = vec![condition_precedent(HeatingPurpose::Annealing)];

        let (evidence, conflicts) = apply_condition_precedents(&mut steps, &precedents);

        assert!(evidence.is_empty());
        assert!(conflicts.is_empty());
        let ProcessStep::Heat { temperature, .. } = &steps[0].step else {
            panic!("expected Heat step");
        };
        assert!(temperature.is_none());
    }

    fn calcination_step() -> PlannedStep {
        PlannedStep {
            requirement: StepRequirement::Required,
            step: ProcessStep::Heat {
                purpose: HeatingPurpose::Calcination,
                temperature: None,
                duration: None,
                atmosphere: None,
                ramp: None,
            },
        }
    }

    /// Phase 19: two precedents disagreeing on the same field must leave
    /// it unresolved rather than one arbitrarily overwriting the other --
    /// the owner's explicit "架空の平均値を作らず未解決として示す"
    /// directive, and the specific bug this phase exists to fix.
    #[test]
    fn two_conflicting_precedents_leave_the_field_unresolved() {
        let mut steps = vec![calcination_step()];
        let precedents = vec![
            ConditionPrecedent {
                temperature: Some(TemperatureRange::new(900.0, 900.0).unwrap()),
                source_id: Some("10.0000/first".to_string()),
                ..condition_precedent(HeatingPurpose::Calcination)
            },
            ConditionPrecedent {
                temperature: Some(TemperatureRange::new(1100.0, 1100.0).unwrap()),
                source_id: Some("10.0000/second".to_string()),
                ..condition_precedent(HeatingPurpose::Calcination)
            },
        ];

        let (evidence, conflicts) = apply_condition_precedents(&mut steps, &precedents);

        let ProcessStep::Heat { temperature, .. } = &steps[0].step else {
            panic!("expected Heat step");
        };
        assert!(
            temperature.is_none(),
            "disagreeing precedents must not resolve the field to either value"
        );
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].step_index, 0);
        assert_eq!(conflicts[0].field, "temperature");
        assert!(conflicts[0].reason.contains("10.0000/first"));
        assert!(conflicts[0].reason.contains("10.0000/second"));
        assert!(
            evidence
                .iter()
                .all(|e| !e.limitations.iter().any(|l| l.contains("temperature"))),
            "neither precedent may be credited with resolving temperature -- it conflicted: \
            {evidence:?}"
        );
    }

    /// The actual bug Phase 19 fixes: under the pre-Phase-19 implementation,
    /// whichever precedent happened to come first in the input slice would
    /// silently resolve the field, so the two orderings below would have
    /// disagreed with each other. Now both orderings must agree (a
    /// conflict, since the values genuinely differ).
    #[test]
    fn conflicting_precedent_detection_does_not_depend_on_input_order() {
        let forward = vec![
            ConditionPrecedent {
                temperature: Some(TemperatureRange::new(900.0, 900.0).unwrap()),
                ..condition_precedent(HeatingPurpose::Calcination)
            },
            ConditionPrecedent {
                temperature: Some(TemperatureRange::new(1100.0, 1100.0).unwrap()),
                ..condition_precedent(HeatingPurpose::Calcination)
            },
        ];
        let reversed: Vec<ConditionPrecedent> = forward.iter().cloned().rev().collect();

        let mut forward_steps = vec![calcination_step()];
        let (_, forward_conflicts) = apply_condition_precedents(&mut forward_steps, &forward);
        let mut reversed_steps = vec![calcination_step()];
        let (_, reversed_conflicts) = apply_condition_precedents(&mut reversed_steps, &reversed);

        let ProcessStep::Heat {
            temperature: forward_temp,
            ..
        } = &forward_steps[0].step
        else {
            panic!("expected Heat step");
        };
        let ProcessStep::Heat {
            temperature: reversed_temp,
            ..
        } = &reversed_steps[0].step
        else {
            panic!("expected Heat step");
        };
        assert_eq!(
            *forward_temp, *reversed_temp,
            "must agree regardless of input order"
        );
        assert!(forward_temp.is_none());
        assert_eq!(forward_conflicts.len(), reversed_conflicts.len());
        assert_eq!(forward_conflicts[0].field, reversed_conflicts[0].field);
    }

    /// Two precedents that happen to report the *same* value for a field
    /// are agreement, not a conflict -- both still get credited with
    /// their own evidence entry.
    #[test]
    fn two_agreeing_precedents_resolve_the_field_and_both_are_credited() {
        let mut steps = vec![calcination_step()];
        let precedents = vec![
            ConditionPrecedent {
                temperature: Some(TemperatureRange::new(900.0, 900.0).unwrap()),
                source_id: Some("10.0000/first".to_string()),
                ..condition_precedent(HeatingPurpose::Calcination)
            },
            ConditionPrecedent {
                temperature: Some(TemperatureRange::new(900.0, 900.0).unwrap()),
                source_id: Some("10.0000/second".to_string()),
                ..condition_precedent(HeatingPurpose::Calcination)
            },
        ];

        let (evidence, conflicts) = apply_condition_precedents(&mut steps, &precedents);

        let ProcessStep::Heat { temperature, .. } = &steps[0].step else {
            panic!("expected Heat step");
        };
        assert_eq!(temperature.unwrap().min_celsius(), 900.0);
        assert!(conflicts.is_empty());
        let sources: std::collections::BTreeSet<&str> = evidence
            .iter()
            .filter_map(|e| e.source_id.as_deref())
            .collect();
        assert_eq!(
            sources,
            std::collections::BTreeSet::from(["10.0000/first", "10.0000/second"]),
            "both agreeing sources should be credited, not just whichever ran first"
        );
    }

    /// The order-independence guarantee must cover the *resolved* case,
    /// not just the conflict case above -- two precedents with asymmetric
    /// field coverage (one supplies only `temperature`, the other supplies
    /// `temperature` and `duration`, agreeing on the overlap) must produce
    /// the same `evidence` *sequence*, not merely the same set, regardless
    /// of which precedent the provider lists first. Emitting evidence in
    /// `matching`-index order would make this flap the moment a corpus
    /// target ever has two precedents backing one step.
    #[test]
    fn resolved_evidence_order_does_not_depend_on_precedent_input_order() {
        let narrow = ConditionPrecedent {
            temperature: Some(TemperatureRange::new(900.0, 900.0).unwrap()),
            duration: None,
            atmosphere: None,
            source_id: Some("10.0000/narrow".to_string()),
            ..condition_precedent(HeatingPurpose::Calcination)
        };
        let wide = ConditionPrecedent {
            temperature: Some(TemperatureRange::new(900.0, 900.0).unwrap()),
            duration: Some(DurationRange::new(2.0, 2.0).unwrap()),
            atmosphere: None,
            source_id: Some("10.0000/wide".to_string()),
            ..condition_precedent(HeatingPurpose::Calcination)
        };

        let mut forward_steps = vec![calcination_step()];
        let (forward_evidence, _) =
            apply_condition_precedents(&mut forward_steps, &[narrow.clone(), wide.clone()]);
        let mut reversed_steps = vec![calcination_step()];
        let (reversed_evidence, _) =
            apply_condition_precedents(&mut reversed_steps, &[wide, narrow]);

        assert_eq!(
            forward_evidence, reversed_evidence,
            "evidence must come out in the same order regardless of precedent input order"
        );
    }

    /// Field-granular (Phase 19, owner's explicit choice over discarding a
    /// whole precedent on any single disagreement): precedents agreeing on
    /// `duration` but disagreeing on `temperature` must still resolve
    /// `duration`.
    #[test]
    fn a_conflict_on_one_field_does_not_block_resolution_of_an_agreeing_field() {
        let mut steps = vec![calcination_step()];
        let precedents = vec![
            ConditionPrecedent {
                temperature: Some(TemperatureRange::new(900.0, 900.0).unwrap()),
                duration: Some(DurationRange::new(2.0, 2.0).unwrap()),
                ..condition_precedent(HeatingPurpose::Calcination)
            },
            ConditionPrecedent {
                temperature: Some(TemperatureRange::new(1100.0, 1100.0).unwrap()),
                duration: Some(DurationRange::new(2.0, 2.0).unwrap()),
                ..condition_precedent(HeatingPurpose::Calcination)
            },
        ];

        let (_, conflicts) = apply_condition_precedents(&mut steps, &precedents);

        let ProcessStep::Heat {
            temperature,
            duration,
            ..
        } = &steps[0].step
        else {
            panic!("expected Heat step");
        };
        assert!(temperature.is_none(), "temperature genuinely conflicts");
        assert_eq!(
            duration.unwrap().min_hours(),
            2.0,
            "duration agrees across both precedents and must still resolve"
        );
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].field, "temperature");
    }
}
