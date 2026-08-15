use crate::composition::{Composition, Element};
use crate::error::{GugenError, Result, require_finite};
use crate::evidence::{EvidenceStrength, PlanningEvidence};
use crate::process::{
    CONDITION_FIELD_ATMOSPHERE, CONDITION_FIELD_DURATION, CONDITION_FIELD_RAMP_RATE,
    CONDITION_FIELD_TEMPERATURE, ConditionConflict, PlannedStep, ProcessStep, RouteFamily,
};
use crate::reaction::BalancedReaction;
use crate::report::{
    ApplicabilityAssessment, PlanningWarning, UnresolvedRequirement, WarningSeverity,
};
use std::collections::BTreeSet;

/// A validated score in `[0.0, 1.0]`. Not given a concrete shape by
/// AGENTS.md (only referenced as `Score01`) -- a rejecting newtype matches
/// every other validated numeric type in this crate (`TemperatureRange`,
/// `ReactionEnergy`, ...).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Score01(f64);

impl Score01 {
    pub const ZERO: Score01 = Score01(0.0);
    pub const ONE: Score01 = Score01(1.0);

    pub fn new(value: f64) -> Result<Self> {
        require_finite("Score01", value)?;
        if !(0.0..=1.0).contains(&value) {
            return Err(GugenError::ScoreOutOfRange { value });
        }
        Ok(Self(value))
    }

    pub fn value(&self) -> f64 {
        self.0
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Score01 {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Score01::new(value).map_err(serde::de::Error::custom)
    }
}

/// AGENTS.md §13, verbatim. Most of this breakdown is structurally
/// constant across every plan the crate can currently produce:
/// `stoichiometric_validity` and `precursor_coverage` are always `1.0`
/// (reaction balancing is exact and `search_precursor_sets` already
/// hard-filters on full element coverage -- both are re-derived defensively
/// here rather than assumed, but neither can discriminate between plans
/// yet); `thermodynamic_support` is always `None` -- with no
/// `ThermodynamicProvider` configured there's simply no data, and even
/// with one configured (Phase 13's `MaterialsProjectSnapshotProvider`, for
/// one) a resolved reaction energy still isn't converted into this score,
/// deliberately (see `score_plan`'s own doc comment); `safety_penalty` is
/// always `0.0` (no hazard data source exists -- see `manual_review_required`
/// on [`PlanAssessment`]). `uncertainty_penalty` was always `1.0` before
/// Phase 10 (no condition was ever resolved); with a
/// `ProcessEvidenceProvider` that actually resolves conditions (e.g.
/// `InMemoryLiteratureConditionProvider`) wired in, it varies for the
/// targets that provider has real cited coverage for -- still `1.0`
/// whenever no condition provider is configured, or when one is configured
/// but has no matching precedent for this target. `evidence_strength` uses
/// weakest-link aggregation (see `strength_value`) and is `0.25` for every
/// plan the current generator produces, since every route attaches at
/// least one `Weak` template-default entry -- this stays true regardless
/// of condition resolution, since resolved-condition evidence doesn't
/// remove the template's own baseline `Weak` entries. **`total_ranking_score`
/// varies with `process_simplicity` always, and with `uncertainty_penalty`
/// only for targets a condition provider actually covers.** Since Phase 12,
/// `process_simplicity` is computed against a per-`RouteFamily` step-count
/// range (`step_bounds`), not one shared range -- a plan's route family can
/// therefore change its `process_simplicity` (and so `total_ranking_score`)
/// relative to a same-precursor-set plan under a different route family,
/// but this is still the *same* one real driver, not a new independent
/// dimension: two plans that each happen to sit at their own family's
/// maximum step count still score identically (see the worked BaTiO3
/// example in `README.md`, where both route families tie at 0.0625). This
/// is the true extent of v0.1/v0.2.0's ranking discriminating power; it is
/// not a seven-dimensional judgment yet.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlanScoreBreakdown {
    pub stoichiometric_validity: Score01,
    pub precursor_coverage: Score01,
    pub thermodynamic_support: Option<Score01>,
    pub process_simplicity: Score01,
    pub evidence_strength: Score01,
    pub safety_penalty: Score01,
    pub uncertainty_penalty: Score01,
    pub total_ranking_score: Score01,
}

/// AGENTS.md §13, verbatim fields.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RankingWeights {
    pub stoichiometric_validity: f64,
    pub precursor_coverage: f64,
    pub thermodynamic_support: f64,
    pub process_simplicity: f64,
    pub evidence_strength: f64,
    pub safety_penalty: f64,
    pub uncertainty_penalty: f64,
}

impl Default for RankingWeights {
    /// Equal weight (`1.0`) on every dimension. AGENTS.md §13 explicitly
    /// forbids tuning weights against a validation corpus or holdout set,
    /// and v0.1 has neither (curated fixtures are Phase 8 work) -- equal
    /// weighting is the only default that doesn't smuggle in an
    /// unjustified claim about which dimension matters more.
    fn default() -> Self {
        Self {
            stoichiometric_validity: 1.0,
            precursor_coverage: 1.0,
            thermodynamic_support: 1.0,
            process_simplicity: 1.0,
            evidence_strength: 1.0,
            safety_penalty: 1.0,
            uncertainty_penalty: 1.0,
        }
    }
}

/// A deterministic fingerprint of `weights`, for
/// `PlanningProvenance.ranking_config_digest` (AGENTS.md §13:
/// "weightをprovenanceに保存"). Not a cryptographic hash -- `DefaultHasher`
/// is enough to detect "did the ranking config change between two runs",
/// which is all provenance needs it for.
pub fn ranking_weights_digest(weights: &RankingWeights) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for w in [
        weights.stoichiometric_validity,
        weights.precursor_coverage,
        weights.thermodynamic_support,
        weights.process_simplicity,
        weights.evidence_strength,
        weights.safety_penalty,
        weights.uncertainty_penalty,
    ] {
        w.to_bits().hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// AGENTS.md §16, verbatim. Kept as four independent dimensions rather
/// than collapsed into `overall`, specifically because a reaction can be
/// stoichiometrically certain while its process conditions are completely
/// unresolved ("条件未確定でも反応式が確実なケースがあります。単一
/// confidenceに潰さないでください").
///
/// **`overall` was structurally constant at `0.75` for every plan with a
/// balanced reaction and non-empty evidence through v0.1** (Phase 8's
/// false-confidence audit, `tests/validation.rs`,
/// `confidence_overall_is_measured_not_assumed_to_be_constant`, and
/// `docs/benchmark_report.md`): it averages four `Score01` values, and
/// `process_conditions` was always `0.0` (no provider ever resolved a
/// condition), so `(1 + 1 + 0 + 1) / 4` was the only value this could
/// produce for a successfully planned route. Since Phase 10,
/// `process_conditions` can be nonzero when a `ProcessEvidenceProvider`
/// resolves a real, cited condition for that specific target -- `overall`
/// still stays `0.75` for every plan no such provider covers (including
/// every `Planner::offline_minimal` plan, unconditionally). Each sub-score
/// is individually honest; where it stays constant, that just means
/// `overall` cannot discriminate between plans of genuinely different
/// real uncertainty *for that plan*. Not "fixed" with an invented
/// weighting -- no calibration data exists to justify one (AGENTS.md §27).
/// See `tasks/todo.md`'s Phase 8 stop-and-report entry for the original
/// finding and Phase 10's entry for what changed.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ConfidenceAssessment {
    pub overall: Score01,
    pub stoichiometry: Score01,
    pub precursor_selection: Score01,
    pub process_conditions: Score01,
    pub evidence_coverage: Score01,
}

/// AGENTS.md §6's `SynthesisPlan.assumptions`. Not given a verbatim shape.
/// `score_plan` populates this only with premises that aren't already
/// surfaced as a `PlanningEvidence.limitations` entry or a
/// `PlanningWarning` (most of the v0.1 generator's defaults are -- e.g.
/// "method choice is a fixed template default" -- so this stays short).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlanningAssumption {
    pub statement: String,
}

/// Everything [`score_plan`] computes for one plan, ready to be merged into
/// the rest of a `SynthesisPlan`.
#[derive(Debug, Clone, PartialEq)]
pub struct PlanAssessment {
    pub score: PlanScoreBreakdown,
    pub confidence: ConfidenceAssessment,
    pub applicability: ApplicabilityAssessment,
    pub assumptions: Vec<PlanningAssumption>,
    pub unresolved: Vec<UnresolvedRequirement>,
    pub manual_review_required: bool,
    pub warnings: Vec<PlanningWarning>,
}

/// Weak/Moderate/Strong -> `[0, 1]`. Deliberately avoids the extremes: even
/// "Strong" evidence in this domain (e.g. an exact stoichiometric balance
/// requiring a step) isn't absolute certainty about the real world, and
/// "Weak" evidence (a template default) is still a stated, non-arbitrary
/// choice, not zero information.
///
/// `evidence_strength` aggregates by minimum (weakest link), and the
/// current `conventional_solid_state_template` always attaches at least
/// one `Weak` entry (its opening weigh/mix/grind/form justification) --
/// so `Moderate`/`Strong` are currently unreachable as the *aggregate*
/// value even though individual entries do use them. This table stays
/// three-valued because individual `PlanningEvidence.strength` values are
/// real, per-item information; only the aggregate is currently flat.
fn strength_value(strength: EvidenceStrength) -> f64 {
    match strength {
        EvidenceStrength::Weak => 0.25,
        EvidenceStrength::Moderate => 0.6,
        EvidenceStrength::Strong => 0.9,
    }
}

/// Per-route-family achievable step-count range, each derived the same way
/// the original single global range was: from that family's own
/// template's actual achievable step counts, not a shared guess (Phase 12
/// fix -- a single global range made a `Mechanochemical` plan's genuinely
/// shorter step count compare unfairly against `ConventionalSolidState`'s
/// own range, since `process_simplicity` is computed from the raw count
/// clamped into whatever range was passed in).
///
/// `ConventionalSolidState` (AGENTS.md §11's numbered outline has 9 steps):
/// the generator either includes calcination+regrind or doesn't, so 7 and
/// 9 are the only step counts it can currently produce.
///
/// `Mechanochemical` (`mechanochemical_template`): weigh + ball-milling
/// (combined mix+grind) + optional form + characterize is 4 steps; a
/// byproduct-releasing reaction adds a required anneal + cool, for 6.
fn step_bounds(route_family: RouteFamily) -> (usize, usize) {
    match route_family {
        RouteFamily::ConventionalSolidState => (7, 9),
        RouteFamily::Mechanochemical => (4, 6),
    }
}

fn resolved_condition_fraction(steps: &[PlannedStep]) -> Score01 {
    let mut total = 0u32;
    let mut resolved = 0u32;
    for planned in steps {
        match &planned.step {
            ProcessStep::Heat {
                temperature,
                duration,
                atmosphere,
                ramp,
                ..
            } => {
                for slot in [
                    temperature.is_some(),
                    duration.is_some(),
                    atmosphere.is_some(),
                    ramp.is_some(),
                ] {
                    total += 1;
                    resolved += slot as u32;
                }
            }
            ProcessStep::Grind { duration, .. } => {
                total += 1;
                resolved += duration.is_some() as u32;
            }
            ProcessStep::Form { pressure, .. } => {
                total += 1;
                resolved += pressure.is_some() as u32;
            }
            _ => {}
        }
    }
    if total == 0 {
        return Score01::ONE;
    }
    Score01::new(f64::from(resolved) / f64::from(total))
        .expect("resolved <= total, so the ratio is within [0, 1]")
}

fn collect_unresolved(
    steps: &[PlannedStep],
    process_evidence_provider_consulted: bool,
    condition_conflicts: &[ConditionConflict],
) -> Vec<UnresolvedRequirement> {
    const NO_PROVIDER_REASON: &str =
        "no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)";
    const CONSULTED_NO_MATCH_REASON: &str = "a process evidence provider was consulted but had \
        no matching precedent for this field";
    // Phase 10: once a provider is wired in, a still-unresolved field is a
    // genuinely different fact from "no provider exists at all" -- the old
    // blanket NO_PROVIDER_REASON text becomes false for that case (the
    // provider WAS consulted; it simply had nothing for this specific
    // field). `apply_condition_precedents` only ever fills an unset field,
    // so if a field is still `None` here after a consulted provider ran,
    // that provider genuinely had no matching data for it -- unless
    // `condition_conflicts` (Phase 19) says otherwise: it *did* have data,
    // but two or more precedents disagreed, so the field was deliberately
    // left unresolved rather than picking one or averaging.
    let reason = if process_evidence_provider_consulted {
        CONSULTED_NO_MATCH_REASON
    } else {
        NO_PROVIDER_REASON
    };
    let mut unresolved = Vec::new();
    for (step_index, planned) in steps.iter().enumerate() {
        let conflict_reason = |field: &str| {
            condition_conflicts
                .iter()
                .find(|c| c.step_index == step_index && c.field == field)
                .map(|c| c.reason.clone())
        };
        match &planned.step {
            ProcessStep::Heat {
                purpose,
                temperature,
                duration,
                atmosphere,
                ramp,
                ..
            } => {
                let named = [
                    (temperature.is_none(), CONDITION_FIELD_TEMPERATURE),
                    (duration.is_none(), CONDITION_FIELD_DURATION),
                    (atmosphere.is_none(), CONDITION_FIELD_ATMOSPHERE),
                    (ramp.is_none(), CONDITION_FIELD_RAMP_RATE),
                ];
                for (is_unresolved, field) in named {
                    if is_unresolved {
                        unresolved.push(UnresolvedRequirement {
                            description: format!("{purpose:?} heating step {field}"),
                            reason: conflict_reason(field).unwrap_or_else(|| reason.to_string()),
                        });
                    }
                }
            }
            ProcessStep::Grind { duration, .. } if duration.is_none() => {
                unresolved.push(UnresolvedRequirement {
                    description: "grinding duration".to_string(),
                    reason: reason.to_string(),
                });
            }
            ProcessStep::Form { pressure, .. } if pressure.is_none() => {
                unresolved.push(UnresolvedRequirement {
                    description: "forming pressure".to_string(),
                    reason: reason.to_string(),
                });
            }
            _ => {}
        }
    }
    unresolved
}

/// `RankingWeights` has fully public `f64` fields and no validating
/// constructor (matching this crate's existing precedent for other
/// caller-tunable weight/config types), so a caller can hand `score_plan`
/// a non-finite or negative weight. Treating such a weight as
/// contributing `0.0` (i.e. excluding it from its side's weighted
/// average) mirrors `score_plan`'s own existing graceful-degradation
/// pattern for an all-zero `weight_sum`, and is what keeps a malformed
/// weight from turning into a `NaN` that would otherwise reach
/// `Score01::new(...).expect(...)` and panic --
/// `f64::INFINITY * a_finite_value / f64::INFINITY` is `NaN`, and
/// `NaN.clamp(0.0, 1.0)` is still `NaN` (documented `f64::clamp`
/// behavior), not a value `Score01::new` would accept.
///
/// `RankingWeights`' seventh field, `thermodynamic_support`, is not read
/// by `score_plan` today (see its doc comment below) and so is never
/// passed here -- if a future phase wires it into the average, that read
/// needs this same treatment.
fn sanitize_weight(weight: f64) -> f64 {
    if weight.is_finite() && weight >= 0.0 {
        weight
    } else {
        0.0
    }
}

/// Scores one plan's ingredients (AGENTS.md §13/§16). `route_family`
/// (Phase 12) only affects `process_simplicity`, via `step_bounds` --
/// every other dimension is computed the same way regardless of route
/// family.
///
/// `thermodynamic_support` is always `None` -- this is not "no data source
/// exists" (Phase 13 added one, `MaterialsProjectSnapshotProvider`) but a
/// deliberate choice: a resolved `ReactionEnergy`/`CompetingPhase` becomes
/// `PlanningEvidence` only, never a numeric score (AGENTS.md §4.3 keeps
/// thermodynamic favorability separate from experimental likelihood, and
/// no calibration data exists to justify converting eV/atom into a [0, 1]
/// support score). `None` is excluded from the weighted average entirely
/// rather than treated as `0.0` either way (AGENTS.md §13: "missing
/// thermodynamic dataを自動的に失敗扱いしない").
///
/// `manual_review_required` is always `true`: no hazard/safety data source
/// exists yet (AGENTS.md §15's `PrecursorCandidate` hazard metadata isn't
/// built), so `safety_penalty` staying at `0.0` must not be read as a
/// safety clearance -- "unknown hazardを安全と扱わない". A `Severe`
/// warning says so explicitly.
///
/// `process_evidence_provider_consulted` (Phase 10) only changes the
/// *reason text* on any `UnresolvedRequirement` this call still produces --
/// never which fields end up resolved, which is entirely a function of
/// `steps` (already mutated by `apply_condition_precedents`, if at all,
/// before this is called). Pass `false` for `Planner::offline_minimal`, so
/// its output stays byte-identical to pre-Phase-10 behavior.
///
/// Eight parameters, each independently meaningful and already documented
/// above -- bundling any subset into an artificial struct just to satisfy
/// clippy's default argument-count lint would add a layer of indirection
/// without improving clarity, so that lint is explicitly disabled here
/// rather than routed around.
#[allow(clippy::too_many_arguments)]
pub fn score_plan(
    target: &Composition,
    target_applicability: &ApplicabilityAssessment,
    balanced_reaction: Option<&BalancedReaction>,
    steps: &[PlannedStep],
    evidence: &[PlanningEvidence],
    process_evidence_provider_consulted: bool,
    condition_conflicts: &[ConditionConflict],
    route_family: RouteFamily,
    weights: &RankingWeights,
) -> PlanAssessment {
    let stoichiometric_validity = if balanced_reaction.is_some() {
        Score01::ONE
    } else {
        Score01::ZERO
    };

    let precursor_coverage = match balanced_reaction {
        Some(reaction) => {
            let target_elements: BTreeSet<Element> = target.elements().collect();
            let covered: BTreeSet<Element> = reaction
                .reactants
                .iter()
                .flat_map(|s| s.composition.elements())
                .collect();
            if target_elements.is_subset(&covered) {
                Score01::ONE
            } else {
                Score01::ZERO
            }
        }
        None => Score01::ZERO,
    };

    let (min_template_steps, max_template_steps) = step_bounds(route_family);
    let step_count = steps.len().clamp(min_template_steps, max_template_steps);
    let process_simplicity = Score01::new(
        1.0 - (step_count - min_template_steps) as f64
            / (max_template_steps - min_template_steps) as f64,
    )
    .expect("step_count is clamped to [min_template_steps, max_template_steps]");

    // Weakest-link, not average: one Strong entry alongside several Weak
    // template defaults must not be allowed to outweigh how weak most of
    // the plan's justification actually is.
    let evidence_strength = evidence
        .iter()
        .map(|e| strength_value(e.strength))
        .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |a| a.min(v))))
        .map(|v| Score01::new(v).expect("strength_value is within [0, 1]"))
        .unwrap_or(Score01::ZERO);

    let safety_penalty = Score01::ZERO;
    let uncertainty_penalty = Score01::new(1.0 - resolved_condition_fraction(steps).value())
        .expect("1.0 minus a Score01 in [0, 1] is within [0, 1]");

    let positive_components: Vec<(f64, f64)> = vec![
        (
            sanitize_weight(weights.stoichiometric_validity),
            stoichiometric_validity.value(),
        ),
        (
            sanitize_weight(weights.precursor_coverage),
            precursor_coverage.value(),
        ),
        (
            sanitize_weight(weights.process_simplicity),
            process_simplicity.value(),
        ),
        (
            sanitize_weight(weights.evidence_strength),
            evidence_strength.value(),
        ),
    ];
    let weight_sum: f64 = positive_components.iter().map(|(w, _)| w).sum();
    let positive_average = if weight_sum > 0.0 {
        positive_components.iter().map(|(w, v)| w * v).sum::<f64>() / weight_sum
    } else {
        0.0
    };
    // `positive_average` is already a weighted *average* (divided by its
    // weight_sum), so it's in [0, 1] regardless of weight magnitude. The
    // penalty side must be normalized the same way before subtracting --
    // otherwise a single maxed-out penalty (e.g. uncertainty_penalty=1.0,
    // structurally true for every v0.1 plan with no thermodynamic provider)
    // can swamp a legitimately strong positive_average just because it
    // wasn't divided by anything.
    let penalty_components: Vec<(f64, f64)> = vec![
        (
            sanitize_weight(weights.safety_penalty),
            safety_penalty.value(),
        ),
        (
            sanitize_weight(weights.uncertainty_penalty),
            uncertainty_penalty.value(),
        ),
    ];
    let penalty_weight_sum: f64 = penalty_components.iter().map(|(w, _)| w).sum();
    let penalty_average = if penalty_weight_sum > 0.0 {
        penalty_components.iter().map(|(w, v)| w * v).sum::<f64>() / penalty_weight_sum
    } else {
        0.0
    };
    let total_ranking_score = Score01::new((positive_average - penalty_average).clamp(0.0, 1.0))
        .expect("clamped to [0, 1]");

    let score = PlanScoreBreakdown {
        stoichiometric_validity,
        precursor_coverage,
        thermodynamic_support: None,
        process_simplicity,
        evidence_strength,
        safety_penalty,
        uncertainty_penalty,
        total_ranking_score,
    };

    let process_conditions =
        Score01::new(1.0 - uncertainty_penalty.value()).expect("clamped to [0, 1]");
    let evidence_coverage = if evidence.is_empty() {
        Score01::ZERO
    } else {
        Score01::ONE
    };
    let overall = Score01::new(
        (stoichiometric_validity.value()
            + precursor_coverage.value()
            + process_conditions.value()
            + evidence_coverage.value())
            / 4.0,
    )
    .expect("average of four Score01 values is within [0, 1]");

    let confidence = ConfidenceAssessment {
        overall,
        stoichiometry: stoichiometric_validity,
        precursor_selection: precursor_coverage,
        process_conditions,
        evidence_coverage,
    };

    let warnings = vec![PlanningWarning {
        message: "no hazard or safety data source is wired in yet: safety_penalty \
            carries no real safety information, and this is not a safety \
            clearance (AGENTS.md §15 \"unknown hazardを安全と扱わない\")"
            .to_string(),
        severity: WarningSeverity::Severe,
    }];

    // AGENTS.md §29's "evidenceとassumptionを分離できる" is only
    // demonstrable if a real assumption the code makes is machine-readable,
    // not just a code comment. Per-plan applicability is still a copy of
    // the target-level assessment, not an independent per-route judgment
    // (Phase 12 does not add a real route-suitability classifier -- doing
    // so with no calibration data would be exactly the unsourced heuristic
    // AGENTS.md §27 forbids). Since Phase 12, that gap is stated as a
    // route-family-specific fact rather than "v0.1 has exactly one route
    // family," which stopped being true once `Mechanochemical` shipped.
    let assumptions = vec![PlanningAssumption {
        statement: format!(
            "applicability is copied from the target-level assessment, not \
            independently evaluated per route family: no route-suitability \
            precedent exists for this target under {route_family:?} \
            specifically (every applicable route family is offered \
            unconditionally, AGENTS.md §13)"
        ),
    }];

    PlanAssessment {
        score,
        confidence,
        applicability: target_applicability.clone(),
        assumptions,
        unresolved: collect_unresolved(
            steps,
            process_evidence_provider_consulted,
            condition_conflicts,
        ),
        manual_review_required: true,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::precursor::{AcceptedPrecursorSet, PrecursorId};
    use crate::report::ApplicabilityLevel;

    fn in_domain() -> ApplicabilityAssessment {
        ApplicabilityAssessment {
            level: ApplicabilityLevel::InDomain,
            rationale: vec!["bulk inorganic, formula-only target".to_string()],
        }
    }

    /// Real generator output (not hand-authored), matching the fixture used
    /// in `process.rs`'s own template-differentiation test.
    fn carbonate_and_oxide_routes() -> (Composition, ProcessTemplateResultPair) {
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

        let mechanochemical_carbonate =
            crate::process::mechanochemical_template(&target, &carbonate_set);
        let mechanochemical_oxide = crate::process::mechanochemical_template(&target, &oxide_set);

        let carbonate = crate::process::conventional_solid_state_template(&target, &carbonate_set);
        let oxide = crate::process::conventional_solid_state_template(&target, &oxide_set);
        (
            target,
            ProcessTemplateResultPair {
                carbonate_reaction: carbonate_set.reaction.clone(),
                carbonate_steps: carbonate.steps,
                carbonate_evidence: carbonate.evidence,
                oxide_reaction: oxide_set.reaction.clone(),
                oxide_steps: oxide.steps,
                oxide_evidence: oxide.evidence,
                mechanochemical_carbonate_reaction: carbonate_set.reaction,
                mechanochemical_carbonate_steps: mechanochemical_carbonate.steps,
                mechanochemical_carbonate_evidence: mechanochemical_carbonate.evidence,
                mechanochemical_oxide_reaction: oxide_set.reaction,
                mechanochemical_oxide_steps: mechanochemical_oxide.steps,
                mechanochemical_oxide_evidence: mechanochemical_oxide.evidence,
            },
        )
    }

    struct ProcessTemplateResultPair {
        carbonate_reaction: BalancedReaction,
        carbonate_steps: Vec<PlannedStep>,
        carbonate_evidence: Vec<PlanningEvidence>,
        oxide_reaction: BalancedReaction,
        oxide_steps: Vec<PlannedStep>,
        oxide_evidence: Vec<PlanningEvidence>,
        mechanochemical_carbonate_reaction: BalancedReaction,
        mechanochemical_carbonate_steps: Vec<PlannedStep>,
        mechanochemical_carbonate_evidence: Vec<PlanningEvidence>,
        mechanochemical_oxide_reaction: BalancedReaction,
        mechanochemical_oxide_steps: Vec<PlannedStep>,
        mechanochemical_oxide_evidence: Vec<PlanningEvidence>,
    }

    #[test]
    fn score01_rejects_out_of_range_and_non_finite() {
        assert!(Score01::new(-0.01).is_err());
        assert!(Score01::new(1.01).is_err());
        assert!(Score01::new(f64::NAN).is_err());
        assert!(Score01::new(0.0).is_ok());
        assert!(Score01::new(1.0).is_ok());
    }

    /// AGENTS.md §13: "missing thermodynamic dataを自動的に失敗扱いしない" --
    /// `thermodynamic_support` is always absent in v0.1, but the total score
    /// must still be a sensible positive value, not zeroed out by the gap.
    #[test]
    fn missing_thermodynamic_support_does_not_zero_the_total_score() {
        let (target, routes) = carbonate_and_oxide_routes();
        let assessment = score_plan(
            &target,
            &in_domain(),
            Some(&routes.oxide_reaction),
            &routes.oxide_steps,
            &routes.oxide_evidence,
            false,
            &[],
            RouteFamily::ConventionalSolidState,
            &RankingWeights::default(),
        );
        assert_eq!(assessment.score.thermodynamic_support, None);
        assert!(
            assessment.score.total_ranking_score.value() > 0.0,
            "missing thermodynamic data must not zero the total score: {:?}",
            assessment.score
        );
    }

    /// AGENTS.md §13: "evidenceなしのplanはconfidenceを下げる".
    #[test]
    fn a_plan_with_no_evidence_scores_lower_than_one_with_evidence() {
        let (target, routes) = carbonate_and_oxide_routes();
        let with_evidence = score_plan(
            &target,
            &in_domain(),
            Some(&routes.oxide_reaction),
            &routes.oxide_steps,
            &routes.oxide_evidence,
            false,
            &[],
            RouteFamily::ConventionalSolidState,
            &RankingWeights::default(),
        );
        let without_evidence = score_plan(
            &target,
            &in_domain(),
            Some(&routes.oxide_reaction),
            &routes.oxide_steps,
            &[],
            false,
            &[],
            RouteFamily::ConventionalSolidState,
            &RankingWeights::default(),
        );

        assert!(!routes.oxide_evidence.is_empty());
        assert_eq!(without_evidence.score.evidence_strength, Score01::ZERO);
        assert_eq!(without_evidence.confidence.evidence_coverage, Score01::ZERO);
        assert!(with_evidence.confidence.evidence_coverage.value() > 0.0);
        assert!(
            with_evidence.confidence.overall.value() > without_evidence.confidence.overall.value()
        );
    }

    /// AGENTS.md §15: no hazard data source exists yet, so every v0.1 plan
    /// requires manual review, and safety_penalty=0 must not read as a
    /// safety clearance -- both facts must be present together.
    #[test]
    fn every_plan_requires_manual_review_with_an_explicit_warning() {
        let (target, routes) = carbonate_and_oxide_routes();
        let assessment = score_plan(
            &target,
            &in_domain(),
            Some(&routes.oxide_reaction),
            &routes.oxide_steps,
            &routes.oxide_evidence,
            false,
            &[],
            RouteFamily::ConventionalSolidState,
            &RankingWeights::default(),
        );
        assert!(assessment.manual_review_required);
        assert_eq!(assessment.score.safety_penalty, Score01::ZERO);
        assert!(
            assessment
                .warnings
                .iter()
                .any(|w| w.severity == WarningSeverity::Severe),
            "safety_penalty=0 must be paired with an explicit Severe warning: {:?}",
            assessment.warnings
        );
    }

    #[test]
    fn no_balanced_reaction_means_zero_stoichiometric_and_coverage_scores() {
        let (target, routes) = carbonate_and_oxide_routes();
        let assessment = score_plan(
            &target,
            &in_domain(),
            None,
            &routes.oxide_steps,
            &routes.oxide_evidence,
            false,
            &[],
            RouteFamily::ConventionalSolidState,
            &RankingWeights::default(),
        );
        assert_eq!(assessment.score.stoichiometric_validity, Score01::ZERO);
        assert_eq!(assessment.score.precursor_coverage, Score01::ZERO);
        assert_eq!(assessment.confidence.stoichiometry, Score01::ZERO);
        assert_eq!(assessment.confidence.precursor_selection, Score01::ZERO);
    }

    /// AGENTS.md §11: unresolved conditions are kept, not deleted -- every
    /// `None` condition field on the real generator's Heat steps must
    /// surface as an `UnresolvedRequirement`.
    #[test]
    fn collects_one_unresolved_entry_per_none_condition_field() {
        let (target, routes) = carbonate_and_oxide_routes();
        let assessment = score_plan(
            &target,
            &in_domain(),
            Some(&routes.carbonate_reaction),
            &routes.carbonate_steps,
            &routes.carbonate_evidence,
            false,
            &[],
            RouteFamily::ConventionalSolidState,
            &RankingWeights::default(),
        );
        // Every condition-bearing step is None in v0.1: 2 Heat steps
        // (calcination + sintering) x 4 fields, 2 Grind steps (initial +
        // regrind) x 1 field, 1 Form step x 1 field -- see the carbonate
        // route in process.rs.
        let expected: usize = routes
            .carbonate_steps
            .iter()
            .map(|p| match &p.step {
                ProcessStep::Heat { .. } => 4,
                ProcessStep::Grind { .. } | ProcessStep::Form { .. } => 1,
                _ => 0,
            })
            .sum();
        assert_eq!(assessment.unresolved.len(), expected);
        assert_eq!(assessment.confidence.process_conditions, Score01::ZERO);
    }

    /// Phase 19: a field left unresolved because of a genuine conflict
    /// between two literature precedents must say so specifically, not
    /// fall back to the generic "no matching precedent" text -- that text
    /// is false in this case (there *was* matching data; it disagreed).
    #[test]
    fn a_conflicted_field_uses_its_specific_reason_not_the_generic_one() {
        let (target, routes) = carbonate_and_oxide_routes();
        let calcination_index = routes
            .carbonate_steps
            .iter()
            .position(|p| {
                matches!(
                    &p.step,
                    ProcessStep::Heat {
                        purpose: crate::process::HeatingPurpose::Calcination,
                        ..
                    }
                )
            })
            .expect("carbonate route has a Calcination step");
        let conflicts = vec![ConditionConflict {
            step_index: calcination_index,
            field: "temperature",
            reason: "2 matching literature precedents disagree on temperature: 900.0 (10.0/a) \
                vs. 1100.0 (10.0/b) -- left unresolved rather than picking one or averaging"
                .to_string(),
        }];
        let assessment = score_plan(
            &target,
            &in_domain(),
            Some(&routes.carbonate_reaction),
            &routes.carbonate_steps,
            &routes.carbonate_evidence,
            true,
            &conflicts,
            RouteFamily::ConventionalSolidState,
            &RankingWeights::default(),
        );
        let temperature_entry = assessment
            .unresolved
            .iter()
            .find(|u| u.description == "Calcination heating step temperature")
            .expect("temperature must still be unresolved");
        assert_eq!(temperature_entry.reason, conflicts[0].reason);
        // Every other still-unresolved field on the same plan keeps the
        // ordinary reason -- the conflict is specific to this one field.
        let duration_entry = assessment
            .unresolved
            .iter()
            .find(|u| u.description == "Calcination heating step duration")
            .expect("duration must still be unresolved too");
        assert_ne!(duration_entry.reason, conflicts[0].reason);
    }

    /// AGENTS.md §11: "すべての材料へ同じtemplateを適用してはいけません" --
    /// the carbonate route's extra steps must be visible in
    /// `process_simplicity`, not just in the step list itself.
    #[test]
    fn process_simplicity_differs_between_carbonate_and_oxide_routes() {
        let (target, routes) = carbonate_and_oxide_routes();
        let carbonate = score_plan(
            &target,
            &in_domain(),
            Some(&routes.carbonate_reaction),
            &routes.carbonate_steps,
            &routes.carbonate_evidence,
            false,
            &[],
            RouteFamily::ConventionalSolidState,
            &RankingWeights::default(),
        );
        let oxide = score_plan(
            &target,
            &in_domain(),
            Some(&routes.oxide_reaction),
            &routes.oxide_steps,
            &routes.oxide_evidence,
            false,
            &[],
            RouteFamily::ConventionalSolidState,
            &RankingWeights::default(),
        );
        // Exact counts, not just the relative inequality above: `step_count`
        // is silently clamped into `step_bounds`'s `[min, max]` range in
        // `score_plan`, so a future accidental extra step in
        // `conventional_solid_state_template` would still pass the
        // inequality check (both routes would just shift together) --
        // mirrors `mechanochemical_process_simplicity_is_scored_against_its_own_family_range`'s
        // own exact-count assertions for its sibling route family.
        assert_eq!(
            routes.oxide_steps.len(),
            7,
            "oxide route must be at ConventionalSolidState's own minimum"
        );
        assert_eq!(
            routes.carbonate_steps.len(),
            9,
            "carbonate route (extra CO2-releasing decomposition step) must be \
             at ConventionalSolidState's own maximum"
        );
        assert_eq!(oxide.score.process_simplicity, Score01::ONE);
        assert_eq!(carbonate.score.process_simplicity, Score01::ZERO);
        assert!(
            carbonate.score.process_simplicity.value() < oxide.score.process_simplicity.value()
        );
    }

    /// Phase 12: `step_bounds` is per-`RouteFamily`, not one shared global
    /// range (see that function's doc comment for the bug this fixes). The
    /// oxide-only mechanochemical route (4 steps, `Mechanochemical`'s own
    /// minimum) must score `process_simplicity == 1.0` on its *own* scale --
    /// same as `ConventionalSolidState`'s 7-step minimum scores 1.0 on
    /// *its* scale, exercised above. A route sitting at its family's
    /// minimum step count scoring 1.0 in both families is the intended,
    /// symmetric per-family normalization, not a bug: `process_simplicity`
    /// measures a route's simplicity relative to what its own family can
    /// achieve, never across families.
    #[test]
    fn mechanochemical_process_simplicity_is_scored_against_its_own_family_range() {
        let (target, routes) = carbonate_and_oxide_routes();
        let carbonate = score_plan(
            &target,
            &in_domain(),
            Some(&routes.mechanochemical_carbonate_reaction),
            &routes.mechanochemical_carbonate_steps,
            &routes.mechanochemical_carbonate_evidence,
            false,
            &[],
            RouteFamily::Mechanochemical,
            &RankingWeights::default(),
        );
        let oxide = score_plan(
            &target,
            &in_domain(),
            Some(&routes.mechanochemical_oxide_reaction),
            &routes.mechanochemical_oxide_steps,
            &routes.mechanochemical_oxide_evidence,
            false,
            &[],
            RouteFamily::Mechanochemical,
            &RankingWeights::default(),
        );

        assert_eq!(
            routes.mechanochemical_oxide_steps.len(),
            4,
            "oxide-only mechanochemical route must be at Mechanochemical's own minimum"
        );
        assert_eq!(
            routes.mechanochemical_carbonate_steps.len(),
            6,
            "byproduct-releasing mechanochemical route must be at Mechanochemical's own maximum"
        );
        assert_eq!(oxide.score.process_simplicity, Score01::ONE);
        assert_eq!(carbonate.score.process_simplicity, Score01::ZERO);
        assert!(
            carbonate.score.process_simplicity.value() < oxide.score.process_simplicity.value()
        );
    }

    #[test]
    fn ranking_weights_digest_is_deterministic_and_sensitive_to_changes() {
        let a = ranking_weights_digest(&RankingWeights::default());
        let b = ranking_weights_digest(&RankingWeights::default());
        assert_eq!(a, b);

        let changed = RankingWeights {
            evidence_strength: 2.0,
            ..RankingWeights::default()
        };
        let c = ranking_weights_digest(&changed);
        assert_ne!(a, c);
    }

    /// `RankingWeights` has no validating constructor, so nothing stops a
    /// caller from constructing one with a non-finite component. Before
    /// the fix this made `weight_sum` infinite, `positive_average` a
    /// `NaN` (`inf * finite / inf`), and
    /// `Score01::new(NaN.clamp(0.0, 1.0)).expect(...)` panic --
    /// `score_plan` must instead treat the malformed weight as
    /// contributing nothing and still return a valid, finite score.
    #[test]
    fn score_plan_does_not_panic_on_a_non_finite_weight() {
        let (target, routes) = carbonate_and_oxide_routes();
        let weights = RankingWeights {
            stoichiometric_validity: f64::INFINITY,
            ..RankingWeights::default()
        };
        let assessment = score_plan(
            &target,
            &in_domain(),
            Some(&routes.oxide_reaction),
            &routes.oxide_steps,
            &routes.oxide_evidence,
            false,
            &[],
            RouteFamily::ConventionalSolidState,
            &weights,
        );
        assert!(
            assessment.score.total_ranking_score.value().is_finite(),
            "a non-finite weight must not make the total score non-finite: {:?}",
            assessment.score
        );
    }

    /// Same hazard, the other sign: a negative weight (e.g. a caller
    /// mistake, not overflow) must also be excluded rather than
    /// silently flipping a component's contribution to negative.
    #[test]
    fn score_plan_does_not_panic_on_a_negative_weight() {
        let (target, routes) = carbonate_and_oxide_routes();
        let weights = RankingWeights {
            uncertainty_penalty: -1.0,
            ..RankingWeights::default()
        };
        let assessment = score_plan(
            &target,
            &in_domain(),
            Some(&routes.oxide_reaction),
            &routes.oxide_steps,
            &routes.oxide_evidence,
            false,
            &[],
            RouteFamily::ConventionalSolidState,
            &weights,
        );
        assert!(
            assessment.score.total_ranking_score.value().is_finite(),
            "a negative weight must not make the total score non-finite: {:?}",
            assessment.score
        );
    }
}
