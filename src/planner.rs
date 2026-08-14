use crate::composition::Element;
use crate::config::PlanningConfig;
use crate::error::Result;
use crate::evidence::{EvidenceKind, EvidenceScope, EvidenceStrength, PlanningEvidence};
use crate::precursor::{PrecursorId, PrecursorSelection, search_precursor_sets};
use crate::process::{RouteFamily, applicable_route_family_templates, apply_condition_precedents};
use crate::provenance::PlanningProvenance;
use crate::provider::{
    PrecursorCatalog, ProcessEvidenceProvider, RouteSuitabilityProvider, ThermodynamicProvider,
};
use crate::reaction::{BalancedReaction, ThermodynamicConditions};
use crate::rejection::{RejectedCandidate, RejectionCode};
use crate::report::{
    ApplicabilityAssessment, ApplicabilityLevel, PlanId, PlanningWarning, SCHEMA_VERSION,
    SynthesisPlan, SynthesisPlanningReport, TargetSummary, UnresolvedRequirement, WarningSeverity,
};
use crate::route_suitability::RouteSuitabilityAssessment;
use crate::score::{ranking_weights_digest, score_plan};
use crate::target::TargetSpecification;

/// Orchestrates every subsystem built in Phases 2-5 into the single public
/// entry point AGENTS.md §18 illustrates: catalog lookup, bounded precursor
/// search, process templating, and scoring, assembled into one
/// [`SynthesisPlanningReport`].
///
/// `thermodynamic_provider` and `process_evidence_provider` are optional
/// (AGENTS.md §18's `Planner::offline_minimal`); `catalog` is not, since
/// there is nothing to plan from without it. A failure from either optional
/// provider degrades to a `PlanningWarning` on the affected plan rather
/// than failing the whole report (AGENTS.md §21.5); a catalog failure
/// propagates, since planning cannot proceed without one at all.
pub struct Planner {
    catalog: Box<dyn PrecursorCatalog>,
    thermodynamic_provider: Option<Box<dyn ThermodynamicProvider>>,
    process_evidence_provider: Option<Box<dyn ProcessEvidenceProvider>>,
    route_suitability_provider: Option<Box<dyn RouteSuitabilityProvider>>,
    config: PlanningConfig,
}

impl Planner {
    /// Full configuration: a catalog plus both optional providers.
    pub fn new(
        catalog: impl PrecursorCatalog + 'static,
        process_evidence_provider: impl ProcessEvidenceProvider + 'static,
        thermodynamic_provider: impl ThermodynamicProvider + 'static,
        config: PlanningConfig,
    ) -> Self {
        Self {
            catalog: Box::new(catalog),
            thermodynamic_provider: Some(Box::new(thermodynamic_provider)),
            process_evidence_provider: Some(Box::new(process_evidence_provider)),
            route_suitability_provider: None,
            config,
        }
    }

    /// Catalog only -- no thermodynamic or process-evidence provider.
    /// AGENTS.md §18: "providerがなくても最低限のstoichiometric planningを
    /// 実行できる構成" -- but conditions are still never fabricated to make
    /// up for the missing providers; they stay unresolved instead.
    pub fn offline_minimal(
        catalog: impl PrecursorCatalog + 'static,
        config: PlanningConfig,
    ) -> Self {
        Self {
            catalog: Box::new(catalog),
            thermodynamic_provider: None,
            process_evidence_provider: None,
            route_suitability_provider: None,
            config,
        }
    }

    /// Catalog plus a process-evidence provider (e.g.
    /// `InMemoryLiteratureConditionProvider`, Phase 10) -- no thermodynamic
    /// provider. The one new two-provider combination Phase 10 needs; see
    /// `new`/`offline_minimal` for the other two.
    pub fn with_process_evidence_provider(
        catalog: impl PrecursorCatalog + 'static,
        process_evidence_provider: impl ProcessEvidenceProvider + 'static,
        config: PlanningConfig,
    ) -> Self {
        Self {
            catalog: Box::new(catalog),
            thermodynamic_provider: None,
            process_evidence_provider: Some(Box::new(process_evidence_provider)),
            route_suitability_provider: None,
            config,
        }
    }

    /// Catalog plus a route-suitability provider (e.g.
    /// `InMemoryRouteSuitabilityProvider`, Phase 15A) -- no thermodynamic or
    /// process-evidence provider. Mirrors `with_process_evidence_provider`'s
    /// shape; see `new`/`offline_minimal` for the other combinations.
    pub fn with_route_suitability_provider(
        catalog: impl PrecursorCatalog + 'static,
        route_suitability_provider: impl RouteSuitabilityProvider + 'static,
        config: PlanningConfig,
    ) -> Self {
        Self {
            catalog: Box::new(catalog),
            thermodynamic_provider: None,
            process_evidence_provider: None,
            route_suitability_provider: Some(Box::new(route_suitability_provider)),
            config,
        }
    }

    /// Plans for `target`, returning a complete report -- never a partial
    /// or panicking result for well-formed input (AGENTS.md §25).
    ///
    /// `execution_timestamp` is a parameter, not read from the system
    /// clock internally: `PlanningProvenance.execution_timestamp` is
    /// documented as caller-supplied precisely so the deterministic core
    /// never touches wall-clock time (AGENTS.md §25). This is one
    /// deliberate deviation from AGENTS.md §18's illustrative
    /// single-argument `plan(&target)` signature -- an unset provenance
    /// field would fail §29's "provenanceがある" completion criterion for
    /// every report the crate produces.
    pub fn plan(
        &self,
        target: &TargetSpecification,
        execution_timestamp: &str,
    ) -> Result<SynthesisPlanningReport> {
        let composition = &target.composition;
        let provenance = self.provenance(execution_timestamp);

        let contradictory = contradictory_elements(target);
        if !contradictory.is_empty() {
            return Ok(abstain(target, &contradictory, provenance));
        }

        let applicability = assess_applicability(target);
        let candidates = self
            .catalog
            .candidates_for(composition, &target.constraints)?;

        let mut warnings = Vec::new();
        if candidates.is_empty() {
            warnings.push(PlanningWarning {
                message: "the precursor catalog returned no candidates sharing any \
                    element with the target"
                    .to_string(),
                severity: WarningSeverity::Caution,
            });
        }

        // Phase 15A: computed once per report, independent of which
        // precursor sets get accepted below -- suitability is a
        // (target, route_family) property, not a per-plan one. Correlate a
        // specific SynthesisPlan back to its assessment via
        // SynthesisPlan.route_family. Same two variants
        // applicable_route_family_templates calls unconditionally
        // (process.rs); update both together if a third route family is
        // ever added.
        let mut route_suitability = Vec::new();
        if let Some(provider) = &self.route_suitability_provider {
            for route_family in [
                RouteFamily::ConventionalSolidState,
                RouteFamily::Mechanochemical,
            ] {
                match provider.assess(composition, route_family) {
                    Ok(findings) => route_suitability.push(RouteSuitabilityAssessment {
                        route_family,
                        findings,
                    }),
                    Err(err) => warnings.push(PlanningWarning {
                        message: format!(
                            "route suitability provider failed for {route_family:?}, \
                            continuing without it: {err}"
                        ),
                        severity: WarningSeverity::Info,
                    }),
                }
            }
        }

        let outcome = search_precursor_sets(
            composition,
            &candidates,
            &target.constraints,
            &self.config.search_budget,
        )?;

        let mut plans: Vec<SynthesisPlan> = Vec::with_capacity(outcome.accepted.len());
        for accepted in &outcome.accepted {
            // Phase 12: one accepted precursor set can now produce a plan
            // under more than one route family (e.g. ConventionalSolidState
            // and Mechanochemical both apply unconditionally). Each gets its
            // own full pass through provider lookups below -- duplicating a
            // thermodynamic/process-evidence provider call per route family
            // sharing the same reaction is a known, accepted inefficiency
            // (see the plan's cross-phase notes), not a correctness issue.
            for mut template in applicable_route_family_templates(composition, accepted) {
                let mut evidence = std::mem::take(&mut template.evidence);
                let mut provider_warnings = Vec::new();
                let process_evidence_provider_consulted = self.process_evidence_provider.is_some();

                if let Some(provider) = &self.thermodynamic_provider {
                    match provider
                        .reaction_energy(&accepted.reaction, &ThermodynamicConditions::default())
                    {
                        Ok(Some(energy)) => evidence.push(PlanningEvidence {
                            kind: EvidenceKind::ThermodynamicData,
                            source_id: None,
                            statement: format!(
                                "reaction energy {:.4} eV/atom from the configured \
                            ThermodynamicProvider",
                                energy.value_ev_per_atom()
                            ),
                            strength: EvidenceStrength::Moderate,
                            applicable_to: EvidenceScope::ExactTarget,
                            limitations: vec![
                                "a raw reaction energy is not converted into a favorability \
                                judgment: thermodynamic favorability is not experimental \
                                likelihood (AGENTS.md §4.3)"
                                    .to_string(),
                            ],
                        }),
                        Ok(None) => {}
                        Err(err) => provider_warnings.push(PlanningWarning {
                            message: format!(
                                "thermodynamic provider failed for this candidate, \
                            continuing without its data: {err}"
                            ),
                            severity: WarningSeverity::Info,
                        }),
                    }

                    // Phase 13: context-only, same as reaction_energy above --
                    // never folded into score.rs's numeric scoring (AGENTS.md
                    // §4.3, ThermodynamicProvider::competing_phases's own doc
                    // comment).
                    //
                    // `competing_phases` is a target-only query (no reaction
                    // in its signature) -- a provider's honest answer can
                    // include this specific plan's own precursors/byproducts,
                    // since they're real phases in the same chemical system.
                    // But labeling a plan's own reaction participants as
                    // "competing" with it, on the evidence attached to that
                    // *same* plan, would be a false-confidence-shaped claim
                    // (AGENTS.md §21 audit) -- so anything exactly matching
                    // this reaction's own reactants/products is filtered out
                    // here, where the reaction is in scope, rather than in
                    // the provider (which reasonably has no reaction to
                    // compare against).
                    let this_reaction_species: Vec<_> = accepted
                        .reaction
                        .reactants
                        .iter()
                        .chain(&accepted.reaction.products)
                        .map(|s| s.composition.clone())
                        .collect();
                    match provider.competing_phases(composition) {
                        Ok(phases) => {
                            let phases: Vec<_> = phases
                                .into_iter()
                                .filter(|p| !this_reaction_species.contains(&p.composition))
                                .collect();
                            if !phases.is_empty() {
                                evidence.push(PlanningEvidence {
                                    kind: EvidenceKind::ThermodynamicData,
                                    source_id: None,
                                    statement: format!(
                                        "{} competing phase(s) with known formation energy \
                                        reported near this target composition by the \
                                        configured ThermodynamicProvider, excluding this \
                                        plan's own precursors and reaction products",
                                        phases.len()
                                    ),
                                    strength: EvidenceStrength::Weak,
                                    applicable_to: EvidenceScope::ExactTarget,
                                    limitations: vec![
                                        "competing-phase energetics do not account for \
                                        kinetics, particle size, or atmosphere, and are not \
                                        converted into a selectivity judgment (AGENTS.md §4.3)"
                                            .to_string(),
                                    ],
                                });
                            }
                        }
                        Err(err) => provider_warnings.push(PlanningWarning {
                            message: format!(
                                "thermodynamic provider's competing-phase lookup failed for \
                            this candidate, continuing without it: {err}"
                            ),
                            severity: WarningSeverity::Info,
                        }),
                    }
                }

                let precursors: Vec<PrecursorSelection> = accepted
                    .precursors
                    .iter()
                    .zip(&accepted.reaction.reactants)
                    .map(|(id, species)| PrecursorSelection {
                        precursor: id.clone(),
                        formula_units: species.coefficient,
                    })
                    .collect();

                if let Some(provider) = &self.process_evidence_provider {
                    match provider.precedents(target, &precursors) {
                        Ok(precedents) => {
                            for precedent in precedents {
                                // An empty description means this precedent has nothing
                                // prose-only to add (Phase 10's literature condition
                                // provider, for one) -- pushing a blank statement as
                                // evidence would be noise, not information.
                                if !precedent.description.is_empty() {
                                    evidence.push(PlanningEvidence {
                                        kind: EvidenceKind::UserProvidedPrecedent,
                                        source_id: None,
                                        statement: precedent.description,
                                        strength: EvidenceStrength::Weak,
                                        applicable_to: EvidenceScope::SimilarMaterial,
                                        limitations: vec![
                                            "this precedent's free-text description alone \
                                            carries no structured method/condition detail"
                                                .to_string(),
                                        ],
                                    });
                                }
                                // Phase 10: splice any structured, cited condition data
                                // into this template's still-unresolved Heat steps
                                // before scoring, rather than only ever adding
                                // free-text evidence that never changes what's
                                // actually planned.
                                evidence.extend(apply_condition_precedents(
                                    &mut template.steps,
                                    &precedent.conditions,
                                ));
                            }
                        }
                        Err(err) => provider_warnings.push(PlanningWarning {
                            message: format!(
                                "process evidence provider failed for this candidate, \
                            continuing without its data: {err}"
                            ),
                            severity: WarningSeverity::Info,
                        }),
                    }
                }

                let assessment = score_plan(
                    composition,
                    &applicability,
                    Some(&accepted.reaction),
                    &template.steps,
                    &evidence,
                    process_evidence_provider_consulted,
                    template.route_family,
                    &self.config.ranking_weights,
                );

                let mut plan_warnings = template.warnings;
                plan_warnings.extend(assessment.warnings);
                plan_warnings.extend(provider_warnings);

                plans.push(SynthesisPlan {
                    plan_id: derive_plan_id(
                        &accepted.precursors,
                        &accepted.reaction,
                        template.route_family,
                    ),
                    route_family: template.route_family,
                    precursors,
                    balanced_reaction: Some(accepted.reaction.clone()),
                    steps: template.steps,
                    score: assessment.score,
                    confidence: assessment.confidence,
                    applicability: assessment.applicability,
                    evidence,
                    warnings: plan_warnings,
                    assumptions: assessment.assumptions,
                    unresolved: assessment.unresolved,
                    manual_review_required: assessment.manual_review_required,
                });
            }
        }

        // Deterministic descending rank; ties break on plan_id so ordering
        // never depends on catalog/accepted-set iteration order (AGENTS.md
        // §21.4).
        plans.sort_by(|a, b| {
            b.score
                .total_ranking_score
                .value()
                .partial_cmp(&a.score.total_ranking_score.value())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.plan_id.0.cmp(&b.plan_id.0))
        });

        let max_plans = self.config.search_budget.max_plans_returned;
        let mut rejected_candidates = outcome.rejected;
        let overflow = plans.len().saturating_sub(max_plans);
        if overflow > 0 {
            rejected_candidates.push(RejectedCandidate {
                precursors: vec![],
                reason_codes: vec![RejectionCode::SearchBudgetExhausted],
                explanation: format!(
                    "{overflow} additional valid plan(s) were found but are not \
                    included: only the top {max_plans} by total_ranking_score are \
                    returned (SearchBudget::max_plans_returned)"
                ),
            });
        }
        plans.truncate(max_plans);

        Ok(SynthesisPlanningReport {
            schema_version: SCHEMA_VERSION,
            target: TargetSummary {
                composition: composition.clone(),
                structure_present: target.structure.is_some(),
                desired_phase: target.desired_phase.as_ref().map(|p| p.phase_name.clone()),
            },
            applicability,
            route_suitability,
            plans,
            rejected_candidates,
            unresolved: vec![],
            warnings,
            provenance,
        })
    }

    fn provenance(&self, execution_timestamp: &str) -> PlanningProvenance {
        PlanningProvenance {
            gugen_version: PlanningProvenance::gugen_version().to_string(),
            build_identifier: None,
            schema_version: SCHEMA_VERSION,
            chematic_crystal_version: None,
            mikiwame_version: None,
            precursor_catalog_version: None,
            thermodynamic_provider_version: None,
            process_template_version: None,
            ranking_config_digest: Some(ranking_weights_digest(&self.config.ranking_weights)),
            execution_timestamp: execution_timestamp.to_string(),
            deterministic_seed: self.config.deterministic_seed,
            enabled_features: enabled_features(),
        }
    }
}

fn enabled_features() -> Vec<String> {
    let mut features = Vec::new();
    if cfg!(feature = "serde") {
        features.push("serde".to_string());
    }
    if cfg!(feature = "clap") {
        features.push("clap".to_string());
    }
    if cfg!(feature = "mikiwame") {
        features.push("mikiwame".to_string());
    }
    if cfg!(feature = "materials_project") {
        features.push("materials_project".to_string());
    }
    features
}

/// Target elements that `constraints.forbidden_elements` also forbids --
/// self-contradictory input no plan can ever satisfy (AGENTS.md §26 Phase
/// 6 "invalid target handling"). Distinct from "no candidates cover the
/// target," which is a catalog-coverage outcome, not a domain judgment.
fn contradictory_elements(target: &TargetSpecification) -> Vec<Element> {
    target
        .composition
        .elements()
        .filter(|e| target.constraints.forbidden_elements.contains(e))
        .collect()
}

fn abstain(
    target: &TargetSpecification,
    contradictory: &[Element],
    provenance: PlanningProvenance,
) -> SynthesisPlanningReport {
    let symbols = contradictory
        .iter()
        .map(Element::symbol)
        .collect::<Vec<_>>()
        .join(", ");
    SynthesisPlanningReport {
        schema_version: SCHEMA_VERSION,
        target: TargetSummary {
            composition: target.composition.clone(),
            structure_present: target.structure.is_some(),
            desired_phase: target.desired_phase.as_ref().map(|p| p.phase_name.clone()),
        },
        applicability: ApplicabilityAssessment {
            level: ApplicabilityLevel::OutOfDomain,
            rationale: vec![format!(
                "target composition requires element(s) {symbols} that \
                PlanningConstraints.forbidden_elements also forbids -- no plan can \
                ever satisfy both"
            )],
        },
        // Abstained before any route family was ever considered -- no
        // suitability assessment to report, same reasoning as `plans: []`.
        route_suitability: vec![],
        plans: vec![],
        rejected_candidates: vec![],
        unresolved: vec![UnresolvedRequirement {
            description: "planning".to_string(),
            reason: format!(
                "target and constraints are self-contradictory over element(s) {symbols}"
            ),
        }],
        warnings: vec![],
        provenance,
    }
}

/// Content-derived, not position-derived (AGENTS.md §20: "plan IDを決定的
/// にする"): the same precursor set, reaction, and route family always get
/// the same id regardless of where it lands in ranked order or catalog
/// insertion order. `route_family` is part of the hash (Phase 12): since
/// Phase 12, the same accepted precursor set can produce a plan under more
/// than one route family, and those are different plans that must not
/// collide on `plan_id`.
fn derive_plan_id(
    precursors: &[PrecursorId],
    reaction: &BalancedReaction,
    route_family: RouteFamily,
) -> PlanId {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut ids: Vec<&str> = precursors.iter().map(|p| p.0.as_str()).collect();
    ids.sort_unstable();
    for id in &ids {
        id.hash(&mut hasher);
    }
    for species in reaction.reactants.iter().chain(&reaction.products) {
        for (element, amount) in species.composition.iter() {
            element.symbol().hash(&mut hasher);
            amount.to_bits().hash(&mut hasher);
        }
        species.coefficient.hash(&mut hasher);
    }
    format!("{route_family:?}").hash(&mut hasher);
    PlanId(format!("plan-{:016x}", hasher.finish()))
}

/// AGENTS.md §16 lists `InDomain` for "bulk inorganic solid-state" but
/// `OutOfDomain` for MOF/thin-film -- and gugen cannot currently tell those
/// apart. `TargetStructure { description: String }` is free text with no
/// classification; a structure gugen can't classify is not evidence of
/// being in-domain, so this stays `PartiallyInDomain` regardless of
/// whether structure is present. Only a real classifier (mikiwame, once
/// wired with actual structure data -- see the `mikiwame` adapter) or a
/// published `chematic-crystal` could justify `InDomain` here.
fn assess_applicability(target: &TargetSpecification) -> ApplicabilityAssessment {
    let rationale = if target.structure.is_some() {
        "structure provided, but gugen has no structural classifier wired in \
        to confirm it's in the validated bulk-inorganic domain (AGENTS.md §16 \
        lists both in-domain and out-of-domain examples with structure present)"
            .to_string()
    } else {
        "formula-only target, no structure provided (AGENTS.md §16's own \
        example for this level)"
            .to_string()
    };
    ApplicabilityAssessment {
        level: ApplicabilityLevel::PartiallyInDomain,
        rationale: vec![rationale],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composition::Composition;
    use crate::config::SearchBudget;
    use crate::error::ProviderError;
    use crate::precursor::{AvailabilityMetadata, InMemoryPrecursorCatalog, PrecursorCandidate};
    use crate::process::ProcessPrecedent;
    use crate::reaction::ReactionEnergy;
    use crate::target::PlanningConstraints;

    fn element(symbol: &str) -> Element {
        Element::new(symbol).unwrap()
    }

    fn composition(pairs: &[(&str, f64)]) -> Composition {
        Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
    }

    fn candidate(id: &str, pairs: &[(&str, f64)]) -> PrecursorCandidate {
        PrecursorCandidate {
            id: PrecursorId(id.to_string()),
            composition: composition(pairs),
            availability: None,
        }
    }

    fn barium_titanate_catalog() -> InMemoryPrecursorCatalog {
        InMemoryPrecursorCatalog::new(vec![
            candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            candidate("BaO", &[("Ba", 1.0), ("O", 1.0)]),
            candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
        ])
    }

    fn barium_titanate_target() -> TargetSpecification {
        TargetSpecification {
            composition: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
            structure: None,
            desired_phase: None,
            constraints: PlanningConstraints::default(),
        }
    }

    fn generous_config() -> PlanningConfig {
        PlanningConfig {
            search_budget: SearchBudget {
                max_precursor_sets: 10_000,
                max_precursors_per_plan: 3,
                max_plans_returned: 20,
            },
            ..PlanningConfig::default()
        }
    }

    #[test]
    fn offline_minimal_produces_ranked_plans_from_a_catalog_alone() {
        let planner = Planner::offline_minimal(barium_titanate_catalog(), generous_config());
        let report = planner
            .plan(&barium_titanate_target(), "2026-08-14T00:00:00Z")
            .unwrap();

        assert!(!report.plans.is_empty(), "expected at least one plan");
        assert!(
            report
                .plans
                .iter()
                .all(|p| p.balanced_reaction.is_some() && p.manual_review_required),
        );
        assert_eq!(
            report.provenance.execution_timestamp,
            "2026-08-14T00:00:00Z"
        );
        assert!(report.provenance.ranking_config_digest.is_some());
        // Descending order by total_ranking_score.
        for window in report.plans.windows(2) {
            assert!(
                window[0].score.total_ranking_score.value()
                    >= window[1].score.total_ranking_score.value()
            );
        }
        // plan_id must uniquely identify a plan within a report -- this is
        // the assertion that would have caught the search_precursor_sets
        // duplicate-acceptance bug automatically instead of by manually
        // inspecting `gugen plan` CLI output (see precursor.rs's
        // `a_redundant_larger_combination_is_rejected_as_a_duplicate_not_double_accepted`).
        let ids: std::collections::BTreeSet<&str> =
            report.plans.iter().map(|p| p.plan_id.0.as_str()).collect();
        assert_eq!(
            ids.len(),
            report.plans.len(),
            "plan_id must be unique across the report's plans: {:?}",
            report
                .plans
                .iter()
                .map(|p| &p.plan_id.0)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn self_contradictory_target_abstains_with_no_plans() {
        let mut target = barium_titanate_target();
        target.constraints.forbidden_elements.insert(element("Ba"));
        let planner = Planner::offline_minimal(barium_titanate_catalog(), generous_config());

        let report = planner.plan(&target, "2026-08-14T00:00:00Z").unwrap();

        assert!(report.plans.is_empty());
        assert_eq!(
            report.applicability.level,
            crate::report::ApplicabilityLevel::OutOfDomain
        );
    }

    #[test]
    fn empty_catalog_result_produces_a_warning_not_a_panic() {
        let empty = InMemoryPrecursorCatalog::new(vec![]);
        let planner = Planner::offline_minimal(empty, generous_config());

        let report = planner
            .plan(&barium_titanate_target(), "2026-08-14T00:00:00Z")
            .unwrap();

        assert!(report.plans.is_empty());
        assert!(
            report
                .warnings
                .iter()
                .any(|w| w.message.contains("no candidates"))
        );
    }

    struct FailingThermodynamicProvider;
    impl ThermodynamicProvider for FailingThermodynamicProvider {
        fn reaction_energy(
            &self,
            _reaction: &BalancedReaction,
            _conditions: &ThermodynamicConditions,
        ) -> std::result::Result<Option<ReactionEnergy>, ProviderError> {
            Err(ProviderError::Unavailable("simulated outage".to_string()))
        }
    }
    struct FailingProcessEvidenceProvider;
    impl ProcessEvidenceProvider for FailingProcessEvidenceProvider {
        fn precedents(
            &self,
            _target: &TargetSpecification,
            _precursors: &[PrecursorSelection],
        ) -> std::result::Result<Vec<ProcessPrecedent>, ProviderError> {
            Err(ProviderError::Unavailable("simulated outage".to_string()))
        }
    }

    /// AGENTS.md §21.5: one provider failing must not fail the whole plan.
    #[test]
    fn a_failing_optional_provider_degrades_to_a_warning_not_a_failure() {
        let planner = Planner::new(
            barium_titanate_catalog(),
            FailingProcessEvidenceProvider,
            FailingThermodynamicProvider,
            generous_config(),
        );

        let report = planner
            .plan(&barium_titanate_target(), "2026-08-14T00:00:00Z")
            .unwrap();

        assert!(!report.plans.is_empty());
        for plan in &report.plans {
            assert!(
                plan.warnings
                    .iter()
                    .filter(|w| w.message.contains("continuing without"))
                    .count()
                    >= 2,
                "expected both provider failures reflected as warnings: {:?}",
                plan.warnings
            );
        }
    }

    #[test]
    fn overflow_beyond_max_plans_returned_is_explained_not_silently_dropped() {
        let tight_config = PlanningConfig {
            search_budget: SearchBudget {
                max_plans_returned: 1,
                ..generous_config().search_budget
            },
            ..generous_config()
        };
        let planner = Planner::offline_minimal(barium_titanate_catalog(), tight_config);

        let report = planner
            .plan(&barium_titanate_target(), "2026-08-14T00:00:00Z")
            .unwrap();

        assert_eq!(report.plans.len(), 1);
        assert!(report.rejected_candidates.iter().any(|r| {
            r.reason_codes
                .contains(&RejectionCode::SearchBudgetExhausted)
                && r.explanation.contains("additional valid plan")
        }));
    }

    /// `plan_id` must be derived from a plan's own content, not its
    /// position: adding an unrelated candidate to the catalog (which
    /// changes generation order and ranked position for everything after
    /// it) must not change the id of a plan that doesn't use it.
    #[test]
    fn plan_id_is_stable_when_an_unrelated_candidate_is_added_to_the_catalog() {
        let target = barium_titanate_target();
        let baseline = Planner::offline_minimal(barium_titanate_catalog(), generous_config())
            .plan(&target, "2026-08-14T00:00:00Z")
            .unwrap();

        let mut with_extra = vec![
            candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            candidate("BaO", &[("Ba", 1.0), ("O", 1.0)]),
            candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
            // Shares no element with the target -- irrelevant to every
            // accepted plan, but changes catalog size/order.
            candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]),
        ];
        with_extra.reverse();
        let augmented =
            Planner::offline_minimal(InMemoryPrecursorCatalog::new(with_extra), generous_config())
                .plan(&target, "2026-08-14T00:00:00Z")
                .unwrap();

        // Since Phase 12, one precursor set can produce a plan under more
        // than one route family -- the key must include `route_family` too,
        // or two distinct plans (same precursors, different route family)
        // collide on this map's key and one is silently dropped, which
        // would make this assertion vacuous for whichever one survives.
        let plan_key = |plan: &SynthesisPlan| {
            let mut ids: Vec<String> = plan
                .precursors
                .iter()
                .map(|s| s.precursor.0.clone())
                .collect();
            ids.sort();
            (ids, plan.route_family)
        };
        let baseline_by_precursors: std::collections::BTreeMap<(Vec<String>, RouteFamily), &str> =
            baseline
                .plans
                .iter()
                .map(|p| (plan_key(p), p.plan_id.0.as_str()))
                .collect();
        assert_eq!(
            baseline_by_precursors.len(),
            baseline.plans.len(),
            "baseline plans must not collide on (precursor set, route family)"
        );

        for plan in &augmented.plans {
            if let Some(&expected_id) = baseline_by_precursors.get(&plan_key(plan)) {
                assert_eq!(
                    plan.plan_id.0.as_str(),
                    expected_id,
                    "plan_id for precursor set {:?} changed after an unrelated catalog addition",
                    plan_key(plan)
                );
            }
        }
    }

    #[test]
    fn missing_availability_metadata_still_flows_through_planning() {
        let with_metadata = InMemoryPrecursorCatalog::new(vec![PrecursorCandidate {
            id: PrecursorId("BaO".to_string()),
            composition: composition(&[("Ba", 1.0), ("O", 1.0)]),
            availability: Some(AvailabilityMetadata {
                source: "curated_fixture".to_string(),
            }),
        }]);
        let target = TargetSpecification {
            composition: composition(&[("Ba", 1.0), ("O", 1.0)]),
            structure: None,
            desired_phase: None,
            constraints: PlanningConstraints::default(),
        };
        let report = Planner::offline_minimal(with_metadata, generous_config())
            .plan(&target, "2026-08-14T00:00:00Z")
            .unwrap();
        assert!(!report.plans.is_empty());
    }
}
