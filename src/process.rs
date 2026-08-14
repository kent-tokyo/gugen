use crate::composition::Composition;
use crate::error::{GugenError, Result, require_finite};
use crate::evidence::{EvidenceKind, EvidenceScope, EvidenceStrength, PlanningEvidence};
use crate::precursor::{AcceptedPrecursorSet, PrecursorId};
use crate::report::{PlanningWarning, WarningSeverity};

/// Validated min/max condition ranges (AGENTS.md §6 "Conditions"): finite,
/// `min <= max`, and non-negative where physically required (duration,
/// pressure, ramp-rate magnitude — but not temperature, since a negative
/// Celsius value is physically ordinary, e.g. a cooling step).
macro_rules! validated_range {
    ($name:ident { $min:ident, $max:ident }, nonneg = $nonneg:expr) => {
        #[derive(Debug, Clone, Copy, PartialEq)]
        #[cfg_attr(feature = "serde", derive(serde::Serialize))]
        pub struct $name {
            pub $min: f64,
            pub $max: f64,
        }

        impl $name {
            pub fn new($min: f64, $max: f64) -> Result<Self> {
                require_finite(stringify!($min), $min)?;
                require_finite(stringify!($max), $max)?;
                if $nonneg && ($min < 0.0 || $max < 0.0) {
                    return Err(GugenError::NegativeMagnitude {
                        field: stringify!($name),
                        value: $min.min($max),
                    });
                }
                if $min > $max {
                    return Err(GugenError::InvalidRange {
                        min: $min,
                        max: $max,
                    });
                }
                Ok(Self { $min, $max })
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                #[derive(serde::Deserialize)]
                struct Raw {
                    $min: f64,
                    $max: f64,
                }
                let raw = Raw::deserialize(deserializer)?;
                $name::new(raw.$min, raw.$max).map_err(serde::de::Error::custom)
            }
        }
    };
}

validated_range!(
    TemperatureRange {
        min_celsius,
        max_celsius
    },
    nonneg = false
);
validated_range!(
    DurationRange {
        min_hours,
        max_hours
    },
    nonneg = true
);
validated_range!(PressureRange { min_kpa, max_kpa }, nonneg = true);
validated_range!(
    RampRateRange {
        min_celsius_per_hour,
        max_celsius_per_hour
    },
    nonneg = true
);

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

/// AGENTS.md §13: v0.1 supports exactly one route family. Do not
/// pre-add future variants speculatively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RouteFamily {
    ConventionalSolidState,
}

/// AGENTS.md §11: every step must declare how firmly it applies. A step
/// with unknown conditions is kept as `Unresolved`, never silently dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum StepRequirement {
    Required,
    Recommended,
    Optional,
    Unresolved,
}

/// AGENTS.md §12: atmosphere is never a bare string. v0.1 does not predict
/// precise oxygen partial pressure; these variants are a formal
/// oxidation-state / atmosphere-compatibility heuristic, not a guarantee of
/// real phase equilibrium.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Atmosphere {
    Air,
    OxygenRich,
    Inert { gas: InertGas },
    Reducing { agent: Option<ReducingAgent> },
    Vacuum,
    Controlled { description: String },
}

/// Not given verbatim by AGENTS.md §12 -- kept to the two gases a v0.1
/// atmosphere heuristic actually needs to name. Add a variant only when a
/// real curated fixture (Phase 8) needs one this doesn't cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum InertGas {
    Nitrogen,
    Argon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ReducingAgent {
    Hydrogen,
    CarbonMonoxide,
}

/// Method/purpose sub-enums `ProcessStep` references (AGENTS.md §6). Not
/// given verbatim -- each is kept to the standard techniques the §11
/// template outline itself needs, not an exhaustive taxonomy. Add a variant
/// only when a real curated fixture (Phase 8) needs one this doesn't cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum MixingMethod {
    DryMixing,
    WetMixing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GrindingMethod {
    MortarAndPestle,
    BallMilling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FormingMethod {
    UniaxialPressing,
    ColdIsostaticPressing,
}

/// Calcination/sintering/annealing map directly onto AGENTS.md §11's own
/// 仮焼/本焼成 outline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum HeatingPurpose {
    Calcination,
    Sintering,
    Annealing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CoolingMode {
    FurnaceCooling,
    AirCooling,
}

/// AGENTS.md §11 names XRD explicitly ("XRD等による中間確認" -- XRD etc.);
/// v0.1 only needs the one it names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CharacterizationMethod {
    Xrd,
}

/// AGENTS.md §6's `Weigh` step. `mass_grams` is `None` until gugen has an
/// atomic-weight table (not built yet, no §26 phase currently owns it --
/// see tasks/todo.md); formula units alone already say what's being
/// weighed relative to the rest of the plan.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MaterialAmount {
    pub precursor: PrecursorId,
    pub formula_units: u64,
    pub mass_grams: Option<f64>,
}

/// AGENTS.md §6's `ProcessStep`. Step meaning is kept as structured data;
/// only final display is stringified.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ProcessStep {
    Weigh {
        materials: Vec<MaterialAmount>,
    },
    Mix {
        method: MixingMethod,
    },
    Grind {
        method: GrindingMethod,
        duration: Option<DurationRange>,
    },
    Form {
        method: FormingMethod,
        pressure: Option<PressureRange>,
    },
    Heat {
        purpose: HeatingPurpose,
        temperature: Option<TemperatureRange>,
        duration: Option<DurationRange>,
        atmosphere: Option<Atmosphere>,
        ramp: Option<RampRateRange>,
    },
    Cool {
        mode: CoolingMode,
    },
    IntermediateCharacterization {
        method: CharacterizationMethod,
        purpose: String,
    },
}

/// Pairs a `ProcessStep` with its `StepRequirement`. AGENTS.md §11 mandates
/// this per-step distinction, but §6 shows `SynthesisPlan.steps` as a bare
/// `Vec<ProcessStep>` with nowhere to carry it -- `SynthesisPlan` uses
/// `Vec<PlannedStep>` instead to satisfy both.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlannedStep {
    pub requirement: StepRequirement,
    pub step: ProcessStep,
}

/// Output of [`conventional_solid_state_template`]: everything Phase 4 can
/// determine about a solid-state route for one accepted precursor set,
/// ready to be folded into a `SynthesisPlan`.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessTemplateResult {
    pub route_family: RouteFamily,
    pub steps: Vec<PlannedStep>,
    pub evidence: Vec<PlanningEvidence>,
    pub warnings: Vec<PlanningWarning>,
}

/// AGENTS.md §11's conventional solid-state route: weigh, mix, grind,
/// optionally form, calcine (only if the balanced reaction shows a
/// byproduct that needs decomposing off), sinter, cool, and characterize.
///
/// This does not apply the same template to every material (§11): whether
/// the calcination step appears depends on whether `accepted.reaction`
/// actually releases a byproduct beyond `target`, not on the target alone.
/// Every condition this crate has no evidence for yet -- temperature,
/// duration, ramp rate, atmosphere -- is left `None` rather than guessed
/// (§4.1); the returned warnings say so explicitly.
pub fn conventional_solid_state_template(
    target: &Composition,
    accepted: &AcceptedPrecursorSet,
) -> ProcessTemplateResult {
    let reaction = &accepted.reaction;
    let releases_byproduct = reaction.products.iter().any(|p| &p.composition != target);

    // `zip` silently truncates to the shorter side. `search_precursor_sets`
    // now guarantees equal lengths, but this is a public function taking a
    // public struct with public fields -- a hand-built `AcceptedPrecursorSet`
    // with mismatched lengths must not produce a `Weigh` step that quietly
    // omits materials (that reads as a complete list when it isn't).
    let materials_resolved = accepted.precursors.len() == reaction.reactants.len();
    let materials: Vec<MaterialAmount> = if materials_resolved {
        accepted
            .precursors
            .iter()
            .zip(&reaction.reactants)
            .map(|(id, species)| MaterialAmount {
                precursor: id.clone(),
                formula_units: species.coefficient,
                mass_grams: None,
            })
            .collect()
    } else {
        Vec::new()
    };

    let mut warnings = Vec::new();
    if !materials_resolved {
        warnings.push(PlanningWarning {
            message: format!(
                "AcceptedPrecursorSet.precursors ({} entries) and \
                reaction.reactants ({} entries) have different lengths; \
                the Weigh step's material list could not be determined",
                accepted.precursors.len(),
                reaction.reactants.len(),
            ),
            severity: WarningSeverity::Severe,
        });
    }

    let mut steps = vec![
        PlannedStep {
            requirement: if materials_resolved {
                StepRequirement::Required
            } else {
                StepRequirement::Unresolved
            },
            step: ProcessStep::Weigh { materials },
        },
        PlannedStep {
            requirement: StepRequirement::Required,
            step: ProcessStep::Mix {
                method: MixingMethod::DryMixing,
            },
        },
        PlannedStep {
            requirement: StepRequirement::Required,
            step: ProcessStep::Grind {
                method: GrindingMethod::MortarAndPestle,
                duration: None,
            },
        },
        PlannedStep {
            requirement: StepRequirement::Optional,
            step: ProcessStep::Form {
                method: FormingMethod::UniaxialPressing,
                pressure: None,
            },
        },
    ];

    let mut evidence = vec![PlanningEvidence {
        kind: EvidenceKind::ProcessTemplate,
        source_id: None,
        statement: "weigh/mix/grind/form are the fixed opening sequence of the \
            v0.1 conventional solid-state template"
            .to_string(),
        strength: EvidenceStrength::Weak,
        applicable_to: EvidenceScope::GeneralRule,
        limitations: vec![
            "method choice (dry mixing, mortar-and-pestle grinding, uniaxial \
                pressing) is a fixed template default, not selected from \
                target- or precursor-specific data"
                .to_string(),
        ],
    }];

    if releases_byproduct {
        steps.push(PlannedStep {
            requirement: StepRequirement::Required,
            step: ProcessStep::Heat {
                purpose: HeatingPurpose::Calcination,
                temperature: None,
                duration: None,
                atmosphere: None,
                ramp: None,
            },
        });
        evidence.push(PlanningEvidence {
            kind: EvidenceKind::StoichiometricBalance,
            source_id: None,
            statement: "balanced reaction releases a byproduct beyond the \
                target, indicating a decomposition (calcination) step is \
                needed before the final firing step"
                .to_string(),
            strength: EvidenceStrength::Strong,
            applicable_to: EvidenceScope::ExactTarget,
            limitations: vec![
                "calcination is included because the reaction requires it, \
                    not because a specific temperature or duration is known"
                    .to_string(),
            ],
        });
        // AGENTS.md §11 step 6, 再粉砕: a regrind between calcination and
        // final firing, present in the outline specifically for routes
        // that calcine.
        steps.push(PlannedStep {
            requirement: StepRequirement::Recommended,
            step: ProcessStep::Grind {
                method: GrindingMethod::MortarAndPestle,
                duration: None,
            },
        });
        evidence.push(PlanningEvidence {
            kind: EvidenceKind::ProcessTemplate,
            source_id: None,
            statement: "AGENTS.md §11's template outline places a regrind \
                between calcination and final firing"
                .to_string(),
            strength: EvidenceStrength::Weak,
            applicable_to: EvidenceScope::GeneralRule,
            limitations: vec![],
        });
    }

    steps.push(PlannedStep {
        requirement: StepRequirement::Required,
        step: ProcessStep::Heat {
            purpose: HeatingPurpose::Sintering,
            temperature: None,
            duration: None,
            atmosphere: None,
            ramp: None,
        },
    });
    steps.push(PlannedStep {
        requirement: StepRequirement::Required,
        step: ProcessStep::Cool {
            mode: CoolingMode::FurnaceCooling,
        },
    });
    steps.push(PlannedStep {
        requirement: StepRequirement::Recommended,
        step: ProcessStep::IntermediateCharacterization {
            method: CharacterizationMethod::Xrd,
            purpose: "verify target-phase formation".to_string(),
        },
    });

    warnings.push(PlanningWarning {
        message: "temperature, duration, ramp rate, and atmosphere are \
            unresolved for every heating step: gugen has no thermodynamic \
            or literature evidence provider wired in yet (AGENTS.md §4.1)"
            .to_string(),
        severity: WarningSeverity::Caution,
    });

    ProcessTemplateResult {
        route_family: RouteFamily::ConventionalSolidState,
        steps,
        evidence,
        warnings,
    }
}

/// Splices provider-supplied, cited condition data into `steps`'s `Heat`
/// fields (Phase 10). Only ever fills an already-`None` slot -- never
/// overwrites a field some other resolution source already set -- so this
/// composes with any future resolution source rather than one silently
/// clobbering another. Returns one `PlanningEvidence` entry per `Heat` step
/// a precedent actually changed, carrying that precedent's own
/// `evidence_kind`/`strength`/`source_id`/`applicable_to` rather than a
/// value this function invents.
pub(crate) fn apply_condition_precedents(
    steps: &mut [PlannedStep],
    precedents: &[ConditionPrecedent],
) -> Vec<PlanningEvidence> {
    let mut evidence = Vec::new();
    for precedent in precedents {
        for planned in steps.iter_mut() {
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
            if *purpose != precedent.purpose {
                continue;
            }

            let mut resolved_fields = Vec::new();
            if temperature.is_none() {
                if let Some(t) = precedent.temperature {
                    *temperature = Some(t);
                    resolved_fields.push("temperature");
                }
            }
            if duration.is_none() {
                if let Some(d) = precedent.duration {
                    *duration = Some(d);
                    resolved_fields.push("duration");
                }
            }
            if atmosphere.is_none() {
                if let Some(a) = &precedent.atmosphere {
                    *atmosphere = Some(a.clone());
                    resolved_fields.push("atmosphere");
                }
            }
            if ramp.is_none() {
                if let Some(r) = precedent.ramp {
                    *ramp = Some(r);
                    resolved_fields.push("ramp rate");
                }
            }

            if !resolved_fields.is_empty() {
                evidence.push(PlanningEvidence {
                    kind: precedent.evidence_kind,
                    source_id: precedent.source_id.clone(),
                    statement: precedent.statement.clone(),
                    strength: precedent.strength,
                    applicable_to: precedent.applicable_to,
                    limitations: vec![format!(
                        "resolved {} for the {:?} step from this precedent; other \
                        unresolved fields on this or other steps had no matching \
                        precedent data",
                        resolved_fields.join("/"),
                        purpose,
                    )],
                });
            }
        }
    }
    evidence
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_inverted_and_non_finite_ranges() {
        assert!(TemperatureRange::new(900.0, 700.0).is_err());
        assert!(TemperatureRange::new(f64::NAN, 700.0).is_err());
        assert!(TemperatureRange::new(-10.0, 20.0).is_ok());
    }

    #[test]
    fn rejects_negative_duration() {
        assert!(DurationRange::new(-1.0, 2.0).is_err());
        assert!(DurationRange::new(1.0, 2.0).is_ok());
    }

    /// AGENTS.md §11: "すべての材料へ同じtemplateを適用してはいけません" --
    /// a carbonate route (releases CO2, needs calcination) and an
    /// oxide-only route (no byproduct) to the same target must not produce
    /// the same step sequence.
    #[test]
    fn template_differs_between_carbonate_and_oxide_routes_to_the_same_target() {
        use crate::composition::Element;

        let ba = Element::new("Ba").unwrap();
        let ti = Element::new("Ti").unwrap();
        let o = Element::new("O").unwrap();
        let c = Element::new("C").unwrap();

        let target = Composition::new([(ba, 1.0), (ti, 1.0), (o, 3.0)]).unwrap();
        let tio2 = Composition::new([(ti, 1.0), (o, 2.0)]).unwrap();

        let baco3 = Composition::new([(ba, 1.0), (c, 1.0), (o, 3.0)]).unwrap();
        let co2 = Composition::new([(c, 1.0), (o, 2.0)]).unwrap();
        let carbonate_reaction =
            crate::balance::balance(&[baco3, tio2.clone()], &[target.clone(), co2])
                .unwrap()
                .into_iter()
                .next()
                .expect("BaCO3 + TiO2 -> BaTiO3 + CO2 must balance");
        let carbonate_set = AcceptedPrecursorSet {
            precursors: vec![
                PrecursorId("BaCO3".to_string()),
                PrecursorId("TiO2".to_string()),
            ],
            reaction: carbonate_reaction,
        };

        let bao = Composition::new([(ba, 1.0), (o, 1.0)]).unwrap();
        let oxide_reaction = crate::balance::balance(&[bao, tio2], std::slice::from_ref(&target))
            .unwrap()
            .into_iter()
            .next()
            .expect("BaO + TiO2 -> BaTiO3 must balance");
        let oxide_set = AcceptedPrecursorSet {
            precursors: vec![
                PrecursorId("BaO".to_string()),
                PrecursorId("TiO2".to_string()),
            ],
            reaction: oxide_reaction,
        };

        let carbonate_template = conventional_solid_state_template(&target, &carbonate_set);
        let oxide_template = conventional_solid_state_template(&target, &oxide_set);

        let has_calcination = |result: &ProcessTemplateResult| {
            result.steps.iter().any(|planned| {
                matches!(
                    planned.step,
                    ProcessStep::Heat {
                        purpose: HeatingPurpose::Calcination,
                        ..
                    }
                )
            })
        };

        assert!(
            has_calcination(&carbonate_template),
            "carbonate route must include a calcination step: {:?}",
            carbonate_template.steps
        );
        assert!(
            !has_calcination(&oxide_template),
            "oxide-only route must not include a calcination step: {:?}",
            oxide_template.steps
        );
        assert_ne!(
            carbonate_template.steps.len(),
            oxide_template.steps.len(),
            "the two routes must not produce the same template"
        );
    }

    /// AGENTS.md §11: an unresolvable condition is kept as `Unresolved`,
    /// never silently dropped or truncated. A hand-built
    /// `AcceptedPrecursorSet` (not routed through `search_precursor_sets`,
    /// which guarantees alignment) with a `precursors` list shorter than
    /// `reaction.reactants` must not produce a `Weigh` step that quietly
    /// omits materials.
    #[test]
    fn mismatched_precursor_and_reactant_lengths_produce_unresolved_weigh_not_a_truncated_one() {
        use crate::composition::Element;

        let ba = Element::new("Ba").unwrap();
        let ti = Element::new("Ti").unwrap();
        let o = Element::new("O").unwrap();

        let target = Composition::new([(ba, 1.0), (ti, 1.0), (o, 3.0)]).unwrap();
        let bao = Composition::new([(ba, 1.0), (o, 1.0)]).unwrap();
        let tio2 = Composition::new([(ti, 1.0), (o, 2.0)]).unwrap();
        let reaction = crate::balance::balance(&[bao, tio2], std::slice::from_ref(&target))
            .unwrap()
            .into_iter()
            .next()
            .expect("BaO + TiO2 -> BaTiO3 must balance");

        let mismatched_set = AcceptedPrecursorSet {
            precursors: vec![PrecursorId("BaO".to_string())], // only 1, reaction has 2 reactants
            reaction,
        };

        let result = conventional_solid_state_template(&target, &mismatched_set);

        let weigh = result
            .steps
            .first()
            .expect("template must still include a Weigh step");
        match &weigh.step {
            ProcessStep::Weigh { materials } => {
                assert!(
                    materials.is_empty(),
                    "materials must be empty, not truncated: {materials:?}"
                );
            }
            other => panic!("expected Weigh as the first step, got {other:?}"),
        }
        assert_eq!(weigh.requirement, StepRequirement::Unresolved);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.severity == WarningSeverity::Severe),
            "a length mismatch must surface a Severe warning: {:?}",
            result.warnings
        );
    }

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

        let evidence = apply_condition_precedents(&mut steps, &precedents);

        let ProcessStep::Heat {
            temperature,
            duration,
            atmosphere,
            ..
        } = &steps[0].step
        else {
            panic!("expected Heat step");
        };
        assert_eq!(temperature.unwrap().min_celsius, 900.0);
        assert_eq!(duration.unwrap().min_hours, 2.0);
        assert!(matches!(atmosphere, Some(Atmosphere::Air)));

        let ProcessStep::Heat { temperature, .. } = &steps[1].step else {
            panic!("expected Heat step");
        };
        assert_eq!(
            temperature.unwrap().min_celsius,
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

        let evidence = apply_condition_precedents(&mut steps, &precedents);

        assert!(evidence.is_empty());
        let ProcessStep::Heat { temperature, .. } = &steps[0].step else {
            panic!("expected Heat step");
        };
        assert!(temperature.is_none());
    }
}
