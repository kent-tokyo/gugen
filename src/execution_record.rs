//! Phase 25: `SynthesisExecutionRecord` -- what actually happened when a
//! gugen-proposed plan was attempted in a real lab. Append-only,
//! versioned-schema, local JSON/JSONL persistence; provenance mandatory;
//! outcomes are never edited toward "success" after the fact.
//!
//! **Structurally separate from `Planner`/`score_plan`, by construction**
//! (same "reference-only" boundary `commercial_catalog.rs`/
//! `literature_evidence.rs` already establish): nothing in this module is
//! read by, or fed into, a plan's score or ranking. Surfacing these
//! records back into planning as reference-only evidence is Phase 26, not
//! this phase.
//!
//! **No file I/O in this module**, matching every other module in this
//! crate (`std::fs`/`File`/`OpenOptions` usage is confirmed a CLI-only
//! concern crate-wide) -- [`parse_execution_records`] takes an in-memory
//! `&str`; reading/writing/appending an actual file is left to the caller
//! (a future CLI or user code), the same boundary
//! `CommercialPrecursorCatalog::load_csv` already draws.

use crate::composition::Composition;
#[cfg(feature = "serde")]
use crate::error::ProviderError;
use crate::precursor::PrecursorId;
use crate::process::{
    Atmosphere, CharacterizationMethod, CoolingMode, FormingMethod, GrindingMethod, HeatingPurpose,
    MixingMethod, RouteFamily,
};
use crate::report::{PlanId, SynthesisPlan};
use std::collections::BTreeSet;

/// Namespaced and independently versioned -- deliberately distinct from
/// `report::SCHEMA_VERSION`, which `docs/api_stability_policy.md`
/// documents as *not* a strict shape guarantee for exactly this reason: a
/// `SynthesisExecutionRecord` is a long-lived, externally-persisted
/// artifact accumulated across gugen versions over months, unlike a
/// `SynthesisPlanningReport` generated fresh each run. Matches
/// `literature_observations.rs`'s own `CORPUS_SNAPSHOT_SCHEMA_VERSION`
/// precedent for this exact kind of artifact.
pub const EXECUTION_RECORD_SCHEMA_VERSION: &str = "gugen-synthesis-execution-record-v1";

/// A synthesis attempt's own long-lived identity -- self-describing
/// without needing the originating `SynthesisPlanningReport` file to still
/// exist. `PlanId` alone is not enough (it's an opaque hash, not
/// reversible back to route family/composition); Phase 26's own matching
/// criteria (target composition + canonical precursor set + route family
/// + ...) require these fields to be present on the record itself.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlanIdentity {
    pub plan_id: PlanId,
    pub route_family: RouteFamily,
    pub target_composition: Composition,
    /// Order-invariant, matching how a canonical precursor set is
    /// naturally compared: two plans with the same precursors listed in a
    /// different order are the same identity.
    pub precursor_compositions: BTreeSet<Composition>,
}

impl PlanIdentity {
    /// `target_composition` comes from the originating report's own
    /// `TargetSummary` -- it isn't carried on `SynthesisPlan` itself, so
    /// callers must supply it. `precursor_compositions` is empty when
    /// `plan.balanced_reaction` is `None` (a degraded plan) -- such a plan
    /// couldn't be physically attempted in a lab anyway.
    pub fn from_plan(target_composition: Composition, plan: &SynthesisPlan) -> Self {
        let precursor_compositions = plan
            .balanced_reaction
            .as_ref()
            .map(|reaction| {
                reaction
                    .reactants()
                    .iter()
                    .map(|species| species.composition.clone())
                    .collect()
            })
            .unwrap_or_default();
        Self {
            plan_id: plan.plan_id.clone(),
            route_family: plan.route_family,
            target_composition,
            precursor_compositions,
        }
    }
}

/// What was actually weighed for one precursor. Deliberately not
/// `process::MaterialAmount` -- that type's `formula_units: u64` is
/// required and `mass_grams` optional, the right shape for a *planned*
/// amount computed from stoichiometry; a lab log needs the reverse
/// emphasis, since an operator weighs grams, not formula units.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActualPrecursorAmount {
    pub precursor: PrecursorId,
    pub mass_grams: Option<f64>,
    pub formula_units: Option<u64>,
}

/// One real process step actually performed. `planned_step_index` is the
/// position of the corresponding step in `SynthesisPlan.steps`, when this
/// step corresponds to one gugen proposed -- `None` for an ad-hoc step the
/// operator performed that wasn't in the plan (itself worth a
/// `Deviation { category: DeviationCategory::SequenceDeviation, .. }`).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ActualProcessStep {
    pub planned_step_index: Option<usize>,
    pub step: ActualStepDetail,
}

/// Mirrors `process::ProcessStep`'s variants and field *names* -- reusing
/// `process`'s own method/purpose enums directly, so "what was actually
/// done" is drawn from the same closed vocabulary as "what was planned,"
/// and a deviation is a straightforward field-by-field diff -- but with
/// point-value `Option<f64>` measurements instead of `process`'s validated
/// `min<=max` range types: a range describes a *planned* target window, a
/// real measurement is one number.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ActualStepDetail {
    Weigh {
        materials: Vec<ActualPrecursorAmount>,
    },
    Mix {
        method: MixingMethod,
    },
    Grind {
        method: GrindingMethod,
        duration_hours: Option<f64>,
    },
    Form {
        method: FormingMethod,
        pressure_kpa: Option<f64>,
    },
    Heat {
        purpose: HeatingPurpose,
        temperature_celsius: Option<f64>,
        duration_hours: Option<f64>,
        atmosphere: Option<Atmosphere>,
        ramp_celsius_per_hour: Option<f64>,
    },
    Cool {
        mode: CoolingMode,
    },
    IntermediateCharacterization {
        method: CharacterizationMethod,
        purpose: String,
        result_summary: Option<String>,
    },
}

/// A closed set of deviation kinds for querying, plus free text for
/// nuance -- mirrors `PlanningWarning { message, severity }`'s own
/// message-plus-category shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DeviationCategory {
    PrecursorSubstitution,
    AmountDeviation,
    TemperatureDeviation,
    DurationDeviation,
    AtmosphereDeviation,
    SequenceDeviation,
    EquipmentDeviation,
    Other,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Deviation {
    pub category: DeviationCategory,
    pub description: String,
}

/// A 7-state outcome, not success/failure (the owner's explicit
/// requirement) -- `NotMeasured` is itself a distinct, honest state, not
/// the same as omitting the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SynthesisOutcome {
    TargetPhaseObtained,
    PartialTargetPhase,
    CompetingPhaseObserved,
    NoReactionObserved,
    ProcessFailed,
    Inconclusive,
    NotMeasured,
}

/// Facts about the resulting material, as measured/reported -- every field
/// left `None` when not actually measured, never guessed. Plain `f64`, not
/// `commercial_catalog::PurityFraction`: that type only exists behind the
/// optional `commercial_catalog` feature, and this module is deliberately
/// feature-independent (see `SynthesisExecutionRecord.selected_commercial_offers`'s
/// own doc comment).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ExecutionCharacterization {
    pub phase_purity_fraction: Option<f64>,
    pub yield_fraction: Option<f64>,
    pub xrd_reference: Option<String>,
    pub measurement_method: Option<String>,
}

/// What "provenance mandatory" requires at minimum for a record entered by
/// a human after a real lab experiment, not generated by the deterministic
/// core.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ExecutionProvenance {
    pub gugen_version: String,
    pub recorded_by: Option<String>,
    /// Caller-supplied RFC3339 timestamp -- the core never reads the
    /// system clock (matches `PlanningProvenance.execution_timestamp`'s
    /// own rule).
    pub recorded_at: String,
}

/// One synthesis attempt's full record. See the module doc comment for the
/// append-only/versioned-schema/reference-only-later principles this type
/// exists under.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SynthesisExecutionRecord {
    /// Must equal [`EXECUTION_RECORD_SCHEMA_VERSION`] -- checked by
    /// [`parse_execution_records`] on load.
    pub schema_version: String,
    pub plan_identity: PlanIdentity,
    /// Which commercial catalog the offer ids below came from, if any.
    /// Phase 26's own matching criteria name "catalog provenance" as one
    /// of its match keys; the owner's fixed field list for this phase has
    /// no other home for it, so this is one deliberate, minimal addition
    /// beyond that list -- kept `Option` so it costs nothing when no
    /// commercial catalog was involved.
    pub commercial_catalog_source: Option<String>,
    /// Offer ids as they appeared in whatever catalog was used, not
    /// `commercial_catalog::CommercialOfferId` -- that type (and the
    /// `CommercialCombination`/`CommercialOfferSelection` types a
    /// selection would otherwise embed) only exist behind the optional
    /// `commercial_catalog` feature, and are `Serialize`-only besides (no
    /// `Deserialize`, so a persisted, later-reloadable record couldn't
    /// embed them regardless). Plain `String` keeps this module usable
    /// without that feature and fully round-trippable.
    pub selected_commercial_offers: Vec<String>,
    pub actual_precursor_amounts: Vec<ActualPrecursorAmount>,
    pub actual_process_conditions: Vec<ActualProcessStep>,
    pub deviations_from_plan: Vec<Deviation>,
    pub outcome: SynthesisOutcome,
    pub characterization: ExecutionCharacterization,
    pub operator_notes: Option<String>,
    pub experiment_date: Option<String>,
    pub batch_id: Option<String>,
    pub provenance: ExecutionProvenance,
}

/// Whether a malformed or schema-mismatched line aborts the whole parse
/// (`Strict`) or is skipped and reported (`Lenient`) -- mirrors
/// `commercial_catalog::CommercialCatalogLoadMode`'s identical two-mode
/// precedent.
#[cfg(feature = "serde")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionRecordLoadMode {
    Strict,
    Lenient,
}

#[cfg(feature = "serde")]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExecutionRecordLoadReport {
    pub accepted: usize,
    /// `(line_number, reason)` -- `line_number` is 0-based over
    /// non-blank lines' position in the input, matching `str::lines`'s
    /// own enumeration.
    pub rejected: Vec<(usize, String)>,
}

/// Parses a JSONL document (one `SynthesisExecutionRecord` per non-blank
/// line) -- pure, no file I/O (see the module doc comment). Every line's
/// `schema_version` is checked against [`EXECUTION_RECORD_SCHEMA_VERSION`]
/// here, per line rather than via a single whole-file header: a real
/// execution log accumulates appends across gugen versions over months, so
/// no one file-wide version gate fits its lifecycle, and a header line
/// would itself be a mutation hazard for an append-only file under
/// concurrent or crash-prone writers that per-line self-description
/// avoids entirely.
#[cfg(feature = "serde")]
pub fn parse_execution_records(
    jsonl: &str,
    mode: ExecutionRecordLoadMode,
) -> std::result::Result<(Vec<SynthesisExecutionRecord>, ExecutionRecordLoadReport), ProviderError>
{
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();

    for (line_number, line) in jsonl.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let result: std::result::Result<SynthesisExecutionRecord, String> =
            serde_json::from_str::<SynthesisExecutionRecord>(trimmed)
                .map_err(|e| e.to_string())
                .and_then(|record| {
                    if record.schema_version != EXECUTION_RECORD_SCHEMA_VERSION {
                        Err(format!(
                            "schema_version {:?} does not match expected {EXECUTION_RECORD_SCHEMA_VERSION:?}",
                            record.schema_version
                        ))
                    } else {
                        Ok(record)
                    }
                });
        match result {
            Ok(record) => accepted.push(record),
            Err(reason) => {
                if mode == ExecutionRecordLoadMode::Strict {
                    return Err(ProviderError::MalformedRecord(format!(
                        "line {line_number}: {reason}"
                    )));
                }
                rejected.push((line_number, reason));
            }
        }
    }

    let report = ExecutionRecordLoadReport {
        accepted: accepted.len(),
        rejected,
    };
    Ok((accepted, report))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composition::Element;

    fn element(symbol: &str) -> Element {
        Element::new(symbol).unwrap()
    }

    fn composition(pairs: &[(&str, f64)]) -> Composition {
        Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
    }

    #[cfg(feature = "serde")]
    fn sample_record() -> SynthesisExecutionRecord {
        SynthesisExecutionRecord {
            schema_version: EXECUTION_RECORD_SCHEMA_VERSION.to_string(),
            plan_identity: PlanIdentity {
                plan_id: PlanId("plan-0000000000000000".to_string()),
                route_family: RouteFamily::ConventionalSolidState,
                target_composition: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
                precursor_compositions: [
                    composition(&[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
                    composition(&[("Ti", 1.0), ("O", 2.0)]),
                ]
                .into_iter()
                .collect(),
            },
            commercial_catalog_source: Some("example_catalog.csv".to_string()),
            selected_commercial_offers: vec!["BACO3-CHEAP".to_string(), "TIO2-CHEAP".to_string()],
            actual_precursor_amounts: vec![ActualPrecursorAmount {
                precursor: PrecursorId("BaCO3".to_string()),
                mass_grams: Some(200.0),
                formula_units: Some(1),
            }],
            actual_process_conditions: vec![ActualProcessStep {
                planned_step_index: Some(4),
                step: ActualStepDetail::Heat {
                    purpose: HeatingPurpose::Sintering,
                    temperature_celsius: Some(1150.0),
                    duration_hours: Some(4.0),
                    atmosphere: Some(Atmosphere::Air),
                    ramp_celsius_per_hour: None,
                },
            }],
            deviations_from_plan: vec![Deviation {
                category: DeviationCategory::TemperatureDeviation,
                description: "furnace overshot target by 30C".to_string(),
            }],
            outcome: SynthesisOutcome::TargetPhaseObtained,
            characterization: ExecutionCharacterization {
                phase_purity_fraction: Some(0.95),
                yield_fraction: Some(0.88),
                xrd_reference: Some("XRD-2026-08-23-001".to_string()),
                measurement_method: Some("Rietveld refinement".to_string()),
            },
            operator_notes: Some("slight discoloration on the crucible edge".to_string()),
            experiment_date: Some("2026-08-23".to_string()),
            batch_id: Some("batch-042".to_string()),
            provenance: ExecutionProvenance {
                gugen_version: "0.5.0".to_string(),
                recorded_by: Some("operator-alice".to_string()),
                recorded_at: "2026-08-23T00:00:00Z".to_string(),
            },
        }
    }

    fn bare_synthesis_plan(
        balanced_reaction: Option<crate::reaction::BalancedReaction>,
    ) -> SynthesisPlan {
        use crate::report::{ApplicabilityAssessment, ApplicabilityLevel};
        use crate::score::{ConfidenceAssessment, PlanScoreBreakdown, Score01};

        SynthesisPlan {
            plan_id: PlanId("plan-test".to_string()),
            route_family: RouteFamily::ConventionalSolidState,
            precursors: Vec::new(),
            balanced_reaction,
            steps: Vec::new(),
            score: PlanScoreBreakdown {
                stoichiometric_validity: Score01::ZERO,
                precursor_coverage: Score01::ZERO,
                thermodynamic_support: None,
                process_simplicity: Score01::ZERO,
                evidence_strength: Score01::ZERO,
                safety_penalty: Score01::ZERO,
                uncertainty_penalty: Score01::ZERO,
                total_ranking_score: Score01::ZERO,
            },
            confidence: ConfidenceAssessment {
                overall: Score01::ZERO,
                stoichiometry: Score01::ZERO,
                precursor_selection: Score01::ZERO,
                process_conditions: Score01::ZERO,
                evidence_coverage: Score01::ZERO,
            },
            applicability: ApplicabilityAssessment {
                level: ApplicabilityLevel::InDomain,
                rationale: Vec::new(),
            },
            evidence: Vec::new(),
            warnings: Vec::new(),
            assumptions: Vec::new(),
            unresolved: Vec::new(),
            manual_review_required: true,
            literature_evidence: None,
        }
    }

    #[test]
    fn plan_identity_from_plan_derives_precursor_compositions_from_the_balanced_reaction() {
        use crate::reaction::{BalancedReaction, ReactionSpecies};

        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let baco3 = composition(&[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]);
        let tio2 = composition(&[("Ti", 1.0), ("O", 2.0)]);
        let co2 = composition(&[("C", 1.0), ("O", 2.0)]);
        let reaction = BalancedReaction::new(
            vec![
                ReactionSpecies::new(baco3, 1).unwrap(),
                ReactionSpecies::new(tio2, 1).unwrap(),
            ],
            vec![
                ReactionSpecies::new(target.clone(), 1).unwrap(),
                ReactionSpecies::new(co2, 1).unwrap(),
            ],
        )
        .unwrap();
        let plan = bare_synthesis_plan(Some(reaction));

        let identity = PlanIdentity::from_plan(target.clone(), &plan);
        assert_eq!(identity.route_family, RouteFamily::ConventionalSolidState);
        assert_eq!(identity.target_composition, target);
        assert_eq!(identity.precursor_compositions.len(), 2);
    }

    #[test]
    fn plan_identity_from_plan_is_empty_precursor_set_for_a_degraded_plan_with_no_reaction() {
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let plan = bare_synthesis_plan(None);
        let identity = PlanIdentity::from_plan(target, &plan);
        assert!(identity.precursor_compositions.is_empty());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn synthesis_execution_record_round_trips_through_json() {
        let record = sample_record();
        let json = serde_json::to_string(&record).unwrap();
        let round_tripped: SynthesisExecutionRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(record, round_tripped);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn parse_execution_records_appends_and_reads_back_in_order() {
        let records = vec![sample_record(), sample_record(), sample_record()];
        let jsonl = records
            .iter()
            .map(|r| serde_json::to_string(r).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        let (parsed, report) =
            parse_execution_records(&jsonl, ExecutionRecordLoadMode::Strict).unwrap();
        assert_eq!(parsed, records);
        assert_eq!(report.accepted, 3);
        assert!(report.rejected.is_empty());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn parse_execution_records_empty_input_yields_empty() {
        let (parsed, report) =
            parse_execution_records("", ExecutionRecordLoadMode::Strict).unwrap();
        assert!(parsed.is_empty());
        assert_eq!(report.accepted, 0);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn parse_execution_records_tolerates_blank_lines() {
        let one = serde_json::to_string(&sample_record()).unwrap();
        let jsonl = format!("\n{one}\n\n{one}\n");
        let (parsed, report) =
            parse_execution_records(&jsonl, ExecutionRecordLoadMode::Strict).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(report.accepted, 2);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn parse_execution_records_rejects_malformed_line_strict() {
        let result = parse_execution_records("not-json", ExecutionRecordLoadMode::Strict);
        assert!(result.is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn parse_execution_records_skips_malformed_line_lenient() {
        let one = serde_json::to_string(&sample_record()).unwrap();
        let jsonl = format!("not-json\n{one}\n");
        let (parsed, report) =
            parse_execution_records(&jsonl, ExecutionRecordLoadMode::Lenient).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(report.accepted, 1);
        assert_eq!(report.rejected.len(), 1);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn parse_execution_records_rejects_schema_mismatch_strict() {
        let mut record = sample_record();
        record.schema_version = "some-other-schema-v0".to_string();
        let jsonl = serde_json::to_string(&record).unwrap();
        let result = parse_execution_records(&jsonl, ExecutionRecordLoadMode::Strict);
        assert!(result.is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn parse_execution_records_skips_schema_mismatch_lenient() {
        let mut mismatched = sample_record();
        mismatched.schema_version = "some-other-schema-v0".to_string();
        let jsonl = format!(
            "{}\n{}\n",
            serde_json::to_string(&mismatched).unwrap(),
            serde_json::to_string(&sample_record()).unwrap()
        );
        let (parsed, report) =
            parse_execution_records(&jsonl, ExecutionRecordLoadMode::Lenient).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(report.rejected.len(), 1);
        assert!(report.rejected[0].1.contains("schema_version"));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn parse_execution_records_accepted_plus_rejected_equals_nonblank_lines() {
        let one = serde_json::to_string(&sample_record()).unwrap();
        let jsonl = format!("not-json\n{one}\ngarbage\n{one}\n");
        let (parsed, report) =
            parse_execution_records(&jsonl, ExecutionRecordLoadMode::Lenient).unwrap();
        assert_eq!(parsed.len() + report.rejected.len(), 4);
        assert_eq!(report.accepted, parsed.len());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn outcome_is_a_required_field_not_defaulted() {
        let record = sample_record();
        let mut value: serde_json::Value = serde_json::to_value(&record).unwrap();
        value.as_object_mut().unwrap().remove("outcome");
        let result: std::result::Result<SynthesisExecutionRecord, _> =
            serde_json::from_value(value);
        assert!(
            result.is_err(),
            "a record missing `outcome` must fail to deserialize, not silently pick a variant"
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn operator_notes_with_embedded_newline_serializes_to_one_jsonl_line() {
        let mut record = sample_record();
        record.operator_notes = Some("line one\nline two".to_string());
        let json = serde_json::to_string(&record).unwrap();
        assert_eq!(
            json.lines().count(),
            1,
            "serde_json::to_string must escape embedded newlines, not emit them literally -- \
             a literal newline here would silently corrupt JSONL framing"
        );
    }
}
