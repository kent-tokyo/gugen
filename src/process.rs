use crate::composition::Composition;
use crate::error::{GugenError, Result, require_finite};
use crate::evidence::{EvidenceKind, EvidenceScope, EvidenceStrength, PlanningEvidence};
use crate::precursor::{AcceptedPrecursorSet, PrecursorId};
use crate::report::{PlanningWarning, WarningSeverity};
use std::collections::BTreeMap;

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

/// AGENTS.md §13. v0.1 shipped with exactly one route family; add a new
/// variant only with real literature grounding for its process structure
/// (never invented from memory, AGENTS.md §21.3), following the process
/// used for `Mechanochemical` (Phase 12) -- not pre-added speculatively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum RouteFamily {
    ConventionalSolidState,
    /// AGENTS.md §3 keeps *detailed* mechanochemical conditions (milling
    /// duration, ball-to-powder ratio, RPM, etc.) out of scope -- this is
    /// the structural route only, same discipline
    /// `conventional_solid_state_template` already applies to firing
    /// conditions.
    Mechanochemical,
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

/// Phase 12's second route family: weigh, then high-energy ball milling
/// (`GrindingMethod::BallMilling`) as one combined mixing-and-grinding
/// operation, optionally consolidated (pressed) into a bulk shape, with a
/// post-milling anneal only when the balanced reaction needs one.
///
/// Structurally grounded in two independently verified reviews, not
/// invented from memory (AGENTS.md §21.3):
///
/// Suryanarayana, "Mechanical alloying and milling," *Progress in
/// Materials Science* 46(1-2), 1-184 (2001), DOI
/// 10.1016/S0079-6425(99)00010-9: "the actual process of MA starts with
/// mixing of the powders in the right proportion and loading the powder
/// mix into the mill along with the grinding medium... milled for the
/// desired length of time... The milled powder is then consolidated into
/// a bulk shape and heat treated" (p.11) -- a single milling operation
/// performs the mixing and grinding conventional solid-state synthesis
/// splits into separate `Mix`/`Grind` steps, which is why this template
/// has no separate `Mix` step. The same review's Table 21 gives explicit
/// examples of byproduct-releasing compounds forming only via a
/// post-milling heat treatment ("gamma-Al2O3 formed only after heating
/// the milled powder to >300C"; "ZrO2 formed only after heating the
/// milled powder to >400C", p.127) -- the literature basis for making the
/// post-milling anneal conditional on `releases_byproduct`, mirroring how
/// `conventional_solid_state_template` conditions calcination the same
/// way, rather than making it unconditionally present or absent.
///
/// Qiang, Hu, Jiang, "Mechanochemical Synthesis of Advanced Materials for
/// All-Solid-State Battery (ASSB) Applications: A Review," *Polymers*
/// 17(17), 2340 (2025), DOI 10.3390/polym17172340: describes ball milling
/// explicitly as an alternative to "conventional high-temperature
/// solid-phase synthesis methods" for inorganic materials generally (not
/// a narrow niche technique), and gives a route milled, "pelletized, and
/// heat-treated" -- the basis for keeping an optional post-milling `Form`
/// step, mirroring `conventional_solid_state_template`'s own optional
/// `Form` step.
///
/// AGENTS.md §3 excludes only *detailed* mechanochemical conditions
/// (milling duration, ball-to-powder ratio, RPM); this template stays
/// entirely at the structural level those two reviews' step *sequence*
/// supports -- every numeric condition is left `None`, same discipline as
/// `conventional_solid_state_template`.
pub fn mechanochemical_template(
    target: &Composition,
    accepted: &AcceptedPrecursorSet,
) -> ProcessTemplateResult {
    const SURYANARAYANA_2001: &str = "10.1016/S0079-6425(99)00010-9";

    let reaction = &accepted.reaction;
    let releases_byproduct = reaction.products.iter().any(|p| &p.composition != target);

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
            step: ProcessStep::Grind {
                method: GrindingMethod::BallMilling,
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
        source_id: Some(SURYANARAYANA_2001.to_string()),
        statement: "weigh, then a single high-energy ball-milling step (which performs \
            mixing and grinding together, unlike the separate Mix/Grind steps of the \
            conventional solid-state template) is the fixed opening sequence of the \
            mechanochemical route template"
            .to_string(),
        strength: EvidenceStrength::Weak,
        applicable_to: EvidenceScope::GeneralRule,
        limitations: vec![
            "milling method (planetary/shaker/attritor), duration, and ball-to-powder \
                ratio are not selected from target- or precursor-specific data -- \
                detailed mechanochemical conditions are out of scope (AGENTS.md §3)"
                .to_string(),
        ],
    }];

    if releases_byproduct {
        steps.push(PlannedStep {
            requirement: StepRequirement::Required,
            step: ProcessStep::Heat {
                purpose: HeatingPurpose::Annealing,
                temperature: None,
                duration: None,
                atmosphere: None,
                ramp: None,
            },
        });
        evidence.push(PlanningEvidence {
            kind: EvidenceKind::StoichiometricBalance,
            source_id: Some(SURYANARAYANA_2001.to_string()),
            statement: "balanced reaction releases a byproduct beyond the target; ball \
                milling alone is not reliably sufficient to complete such a reaction at \
                room temperature, so a post-milling anneal is included -- the cited \
                review reports specific byproduct-releasing compounds (e.g. gamma-Al2O3, \
                ZrO2) that formed only after heating the as-milled powder"
                .to_string(),
            strength: EvidenceStrength::Moderate,
            applicable_to: EvidenceScope::GeneralRule,
            limitations: vec![
                "whether ball milling alone could complete this specific reaction at \
                    room temperature, without any anneal, is not determined per-target -- \
                    unlike conventional solid-state calcination, milling-induced \
                    mechanical activation can itself drive some decompositions, so this \
                    is Moderate, not Strong, evidence"
                    .to_string(),
            ],
        });
        steps.push(PlannedStep {
            requirement: StepRequirement::Required,
            step: ProcessStep::Cool {
                mode: CoolingMode::FurnaceCooling,
            },
        });
    }

    steps.push(PlannedStep {
        requirement: StepRequirement::Recommended,
        step: ProcessStep::IntermediateCharacterization {
            method: CharacterizationMethod::Xrd,
            purpose: "verify target-phase formation".to_string(),
        },
    });

    warnings.push(PlanningWarning {
        message: "grinding duration, forming pressure, and (if present) heating \
            temperature/duration/atmosphere/ramp are unresolved: gugen has no \
            thermodynamic or literature evidence provider wired in yet (AGENTS.md §4.1)"
            .to_string(),
        severity: WarningSeverity::Caution,
    });

    ProcessTemplateResult {
        route_family: RouteFamily::Mechanochemical,
        steps,
        evidence,
        warnings,
    }
}

/// Every route family gugen currently supports, applied unconditionally to
/// the same accepted precursor set (Phase 12). One accepted precursor set
/// now generally becomes more than one ranked `SynthesisPlan` -- callers
/// (`Planner::plan`) turn each returned `ProcessTemplateResult` into its
/// own plan, rather than assuming exactly one template per accepted set.
pub fn applicable_route_family_templates(
    target: &Composition,
    accepted: &AcceptedPrecursorSet,
) -> Vec<ProcessTemplateResult> {
    vec![
        conventional_solid_state_template(target, accepted),
        mechanochemical_template(target, accepted),
    ]
}

/// Splices provider-supplied, cited condition data into `steps`'s `Heat`
/// fields (Phase 10). Only ever fills an already-`None` slot -- never
/// overwrites a field some other resolution source already set -- so this
/// composes with any future resolution source rather than one silently
/// clobbering another. Returns one `PlanningEvidence` entry per `Heat` step
/// a precedent actually changed, carrying that precedent's own
/// `evidence_kind`/`strength`/`source_id`/`applicable_to` rather than a
/// value this function invents.
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
                        contributed.entry(i).or_default().push("temperature");
                    }
                }
                Some(FieldResolution::Conflict(values)) => conflicts.push(ConditionConflict {
                    step_index,
                    field: "temperature",
                    reason: format_conflict_reason("temperature", &values),
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
                        contributed.entry(i).or_default().push("duration");
                    }
                }
                Some(FieldResolution::Conflict(values)) => conflicts.push(ConditionConflict {
                    step_index,
                    field: "duration",
                    reason: format_conflict_reason("duration", &values),
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
                        contributed.entry(i).or_default().push("atmosphere");
                    }
                }
                Some(FieldResolution::Conflict(values)) => conflicts.push(ConditionConflict {
                    step_index,
                    field: "atmosphere",
                    reason: format_conflict_reason("atmosphere", &values),
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
                        contributed.entry(i).or_default().push("ramp rate");
                    }
                }
                Some(FieldResolution::Conflict(values)) => conflicts.push(ConditionConflict {
                    step_index,
                    field: "ramp rate",
                    reason: format_conflict_reason("ramp rate", &values),
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

    /// Mirrors `template_differs_between_carbonate_and_oxide_routes_to_the_same_target`
    /// for `mechanochemical_template` (Phase 12): a byproduct-releasing
    /// route gets a required post-milling `Annealing` step (and the `Cool`
    /// that follows it), an oxide-only route doesn't -- grounded in
    /// Suryanarayana 2001's reported byproduct-releasing compounds that
    /// form only after heating the as-milled powder (see the function's own
    /// doc comment).
    #[test]
    fn mechanochemical_template_differs_between_carbonate_and_oxide_routes_to_the_same_target() {
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

        let carbonate_template = mechanochemical_template(&target, &carbonate_set);
        let oxide_template = mechanochemical_template(&target, &oxide_set);

        assert_eq!(
            carbonate_template.route_family,
            RouteFamily::Mechanochemical
        );

        let has_anneal = |result: &ProcessTemplateResult| {
            result.steps.iter().any(|planned| {
                matches!(
                    planned.step,
                    ProcessStep::Heat {
                        purpose: HeatingPurpose::Annealing,
                        ..
                    }
                )
            })
        };
        assert!(
            has_anneal(&carbonate_template),
            "byproduct-releasing route must include a post-milling anneal: {:?}",
            carbonate_template.steps
        );
        assert!(
            !has_anneal(&oxide_template),
            "oxide-only route must not include an anneal: {:?}",
            oxide_template.steps
        );
        assert_ne!(
            carbonate_template.steps.len(),
            oxide_template.steps.len(),
            "the two routes must not produce the same template"
        );

        let has_separate_mix_step = |result: &ProcessTemplateResult| {
            result
                .steps
                .iter()
                .any(|planned| matches!(planned.step, ProcessStep::Mix { .. }))
        };
        assert!(
            !has_separate_mix_step(&carbonate_template) && !has_separate_mix_step(&oxide_template),
            "ball milling performs mixing and grinding together -- no separate Mix step, \
            unlike conventional_solid_state_template"
        );
    }

    /// Phase 12: one accepted precursor set now produces a plan under every
    /// applicable route family, not just one -- `applicable_route_family_templates`
    /// is the integration point `Planner::plan` relies on for this.
    #[test]
    fn applicable_route_family_templates_yields_a_distinct_template_per_route_family() {
        use crate::composition::Element;

        let ba = Element::new("Ba").unwrap();
        let ti = Element::new("Ti").unwrap();
        let o = Element::new("O").unwrap();
        let c = Element::new("C").unwrap();

        let target = Composition::new([(ba, 1.0), (ti, 1.0), (o, 3.0)]).unwrap();
        let baco3 = Composition::new([(ba, 1.0), (c, 1.0), (o, 3.0)]).unwrap();
        let tio2 = Composition::new([(ti, 1.0), (o, 2.0)]).unwrap();
        let co2 = Composition::new([(c, 1.0), (o, 2.0)]).unwrap();
        let reaction = crate::balance::balance(&[baco3, tio2], &[target.clone(), co2])
            .unwrap()
            .into_iter()
            .next()
            .expect("BaCO3 + TiO2 -> BaTiO3 + CO2 must balance");
        let accepted = AcceptedPrecursorSet {
            precursors: vec![
                PrecursorId("BaCO3".to_string()),
                PrecursorId("TiO2".to_string()),
            ],
            reaction,
        };

        let templates = applicable_route_family_templates(&target, &accepted);
        let route_families: std::collections::BTreeSet<RouteFamily> =
            templates.iter().map(|t| t.route_family).collect();
        assert_eq!(
            templates.len(),
            route_families.len(),
            "no two templates should share a route family: {:?}",
            route_families
        );
        assert!(route_families.contains(&RouteFamily::ConventionalSolidState));
        assert!(route_families.contains(&RouteFamily::Mechanochemical));
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
        assert_eq!(temperature.unwrap().min_celsius, 900.0);
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
            duration.unwrap().min_hours,
            2.0,
            "duration agrees across both precedents and must still resolve"
        );
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].field, "temperature");
    }
}
