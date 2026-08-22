use crate::composition::Element;
use crate::config::PlanningConfig;
use crate::error::{ProviderError, Result};
use crate::evidence::{EvidenceKind, EvidenceScope, EvidenceStrength, PlanningEvidence};
use crate::precursor::{PrecursorId, PrecursorSelection, search_precursor_sets};
use crate::process::{
    ConditionPrecedent, ProcessPrecedent, RouteFamily, applicable_route_family_templates,
    apply_condition_precedents,
};
use crate::provenance::PlanningProvenance;
use crate::provider::{
    LiteratureEvidenceProvider, PrecursorCatalog, ProcessEvidenceProvider,
    RouteSuitabilityProvider, ThermodynamicProvider,
};
use crate::reaction::{BalancedReaction, CompetingPhase, ReactionEnergy, ThermodynamicConditions};
use crate::rejection::{RejectedCandidate, RejectionCode};
use crate::report::{
    ApplicabilityAssessment, ApplicabilityLevel, NotRecommendedPlan, PlanId, PlanningWarning,
    SCHEMA_VERSION, SynthesisPlan, SynthesisPlanningReport, TargetSummary, UnresolvedRequirement,
    WarningSeverity,
};
use crate::route_suitability::{
    RouteRecommendation, RouteSuitabilityAssessment, SuitabilityVerdict, derive_recommendation,
};
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
    literature_evidence_provider: Option<Box<dyn LiteratureEvidenceProvider>>,
    config: PlanningConfig,
}

/// Builds a [`Planner`] with any combination of its 4 optional providers
/// (v0.5.0, Phase 23B) -- `catalog`/`config` are required up front (there is
/// nothing to plan from without a catalog), each provider is attached by
/// name in any order or combination, and `build()` is infallible (no
/// constructor, named or builder, performs any validation beyond field
/// assignment). The crate's first builder pattern; created because the 5
/// named constructors below only covered 3 of the real 2+-optional-provider
/// combinations.
pub struct PlannerBuilder {
    catalog: Box<dyn PrecursorCatalog>,
    config: PlanningConfig,
    thermodynamic_provider: Option<Box<dyn ThermodynamicProvider>>,
    process_evidence_provider: Option<Box<dyn ProcessEvidenceProvider>>,
    route_suitability_provider: Option<Box<dyn RouteSuitabilityProvider>>,
    literature_evidence_provider: Option<Box<dyn LiteratureEvidenceProvider>>,
}

impl PlannerBuilder {
    pub fn thermodynamic_provider(
        mut self,
        provider: impl ThermodynamicProvider + 'static,
    ) -> Self {
        self.thermodynamic_provider = Some(Box::new(provider));
        self
    }

    pub fn process_evidence_provider(
        mut self,
        provider: impl ProcessEvidenceProvider + 'static,
    ) -> Self {
        self.process_evidence_provider = Some(Box::new(provider));
        self
    }

    pub fn route_suitability_provider(
        mut self,
        provider: impl RouteSuitabilityProvider + 'static,
    ) -> Self {
        self.route_suitability_provider = Some(Box::new(provider));
        self
    }

    pub fn literature_evidence_provider(
        mut self,
        provider: impl LiteratureEvidenceProvider + 'static,
    ) -> Self {
        self.literature_evidence_provider = Some(Box::new(provider));
        self
    }

    pub fn build(self) -> Planner {
        Planner {
            catalog: self.catalog,
            thermodynamic_provider: self.thermodynamic_provider,
            process_evidence_provider: self.process_evidence_provider,
            route_suitability_provider: self.route_suitability_provider,
            literature_evidence_provider: self.literature_evidence_provider,
            config: self.config,
        }
    }
}

impl Planner {
    /// Starts a [`PlannerBuilder`] -- the general construction path,
    /// covering any combination of the 4 optional providers (v0.5.0,
    /// Phase 23B). Superseded the 5 named constructors below, none of
    /// which covered the real 2+-optional-provider combination space; kept
    /// as `#[deprecated]` wrappers around this builder for one release.
    pub fn builder(
        catalog: impl PrecursorCatalog + 'static,
        config: PlanningConfig,
    ) -> PlannerBuilder {
        PlannerBuilder {
            catalog: Box::new(catalog),
            config,
            thermodynamic_provider: None,
            process_evidence_provider: None,
            route_suitability_provider: None,
            literature_evidence_provider: None,
        }
    }

    /// Full configuration: a catalog plus both optional providers.
    #[deprecated(
        since = "0.5.0",
        note = "use Planner::builder(catalog, config).process_evidence_provider(p).thermodynamic_provider(t).build() instead"
    )]
    pub fn new(
        catalog: impl PrecursorCatalog + 'static,
        process_evidence_provider: impl ProcessEvidenceProvider + 'static,
        thermodynamic_provider: impl ThermodynamicProvider + 'static,
        config: PlanningConfig,
    ) -> Self {
        Self::builder(catalog, config)
            .process_evidence_provider(process_evidence_provider)
            .thermodynamic_provider(thermodynamic_provider)
            .build()
    }

    /// Catalog only -- no thermodynamic or process-evidence provider.
    /// AGENTS.md §18: "providerがなくても最低限のstoichiometric planningを
    /// 実行できる構成" -- but conditions are still never fabricated to make
    /// up for the missing providers; they stay unresolved instead.
    #[deprecated(
        since = "0.5.0",
        note = "use Planner::builder(catalog, config).build() instead"
    )]
    pub fn offline_minimal(
        catalog: impl PrecursorCatalog + 'static,
        config: PlanningConfig,
    ) -> Self {
        Self::builder(catalog, config).build()
    }

    /// Catalog plus a process-evidence provider (e.g.
    /// `InMemoryLiteratureConditionProvider`, Phase 10) -- no thermodynamic
    /// provider. The one new two-provider combination Phase 10 needs; see
    /// `new`/`offline_minimal` for the other two.
    #[deprecated(
        since = "0.5.0",
        note = "use Planner::builder(catalog, config).process_evidence_provider(p).build() instead"
    )]
    pub fn with_process_evidence_provider(
        catalog: impl PrecursorCatalog + 'static,
        process_evidence_provider: impl ProcessEvidenceProvider + 'static,
        config: PlanningConfig,
    ) -> Self {
        Self::builder(catalog, config)
            .process_evidence_provider(process_evidence_provider)
            .build()
    }

    /// Catalog plus a route-suitability provider (e.g.
    /// `InMemoryRouteSuitabilityProvider`, Phase 15A) -- no thermodynamic or
    /// process-evidence provider. Mirrors `with_process_evidence_provider`'s
    /// shape; see `new`/`offline_minimal` for the other combinations.
    #[deprecated(
        since = "0.5.0",
        note = "use Planner::builder(catalog, config).route_suitability_provider(p).build() instead"
    )]
    pub fn with_route_suitability_provider(
        catalog: impl PrecursorCatalog + 'static,
        route_suitability_provider: impl RouteSuitabilityProvider + 'static,
        config: PlanningConfig,
    ) -> Self {
        Self::builder(catalog, config)
            .route_suitability_provider(route_suitability_provider)
            .build()
    }

    /// Catalog plus a literature-evidence provider (e.g.
    /// `LiteratureObservationCorpusProvider`, v0.4.0 Integration) -- no
    /// other optional provider. Mirrors `with_route_suitability_provider`'s
    /// shape; see `new`/`offline_minimal` for the other combinations. The
    /// resulting reports carry `SynthesisPlan.literature_evidence` but are
    /// otherwise identical to what `offline_minimal` alone would have
    /// produced -- score, confidence, ranking, and `steps` are unaffected
    /// by construction (`literature_evidence.rs`'s module doc comment).
    #[deprecated(
        since = "0.5.0",
        note = "use Planner::builder(catalog, config).literature_evidence_provider(p).build() instead"
    )]
    pub fn with_literature_evidence_provider(
        catalog: impl PrecursorCatalog + 'static,
        literature_evidence_provider: impl LiteratureEvidenceProvider + 'static,
        config: PlanningConfig,
    ) -> Self {
        Self::builder(catalog, config)
            .literature_evidence_provider(literature_evidence_provider)
            .build()
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

        // `competing_phases` is a target-only query -- its answer cannot
        // vary across `accepted`/route-family iterations below, since
        // `composition` is fixed for this whole `plan()` call. Computed
        // once here (previously: once per (accepted, route_family) pair,
        // a known, accepted inefficiency the ROADMAP recorded) and reused
        // by every iteration. Gated on a non-empty accepted set, matching
        // the pre-fix call site's own scope (it lived inside `for accepted
        // in &outcome.accepted`) -- an empty search result must still make
        // zero provider calls, not one, since nothing below will ever read
        // this value.
        let competing_phases_cache: Option<
            std::result::Result<Vec<CompetingPhase>, ProviderError>,
        > = if outcome.accepted.is_empty() {
            None
        } else {
            self.thermodynamic_provider
                .as_ref()
                .map(|provider| provider.competing_phases(composition))
        };

        let mut plans: Vec<SynthesisPlan> = Vec::with_capacity(outcome.accepted.len());
        for accepted in &outcome.accepted {
            // Phase 12: one accepted precursor set can now produce a plan
            // under more than one route family (e.g. ConventionalSolidState
            // and Mechanochemical both apply unconditionally). Each gets its
            // own full pass through provider lookups below.
            //
            // `reaction_energy` depends only on `accepted.reaction` (fixed
            // `ThermodynamicConditions::default()`), not on which route
            // family's template is being scored, so it's computed once per
            // `accepted` here and reused across every route family sharing
            // it -- closes the other half of the same known inefficiency
            // `competing_phases_cache` above closes; process-evidence
            // provider calls further below still run once per route family,
            // deliberately not touched by this change (out of the scope
            // ROADMAP recorded for this fix).
            let reaction_energy_cache: Option<
                std::result::Result<Option<ReactionEnergy>, ProviderError>,
            > = self.thermodynamic_provider.as_ref().map(|provider| {
                provider.reaction_energy(&accepted.reaction, &ThermodynamicConditions::default())
            });

            // `precursors` and `precedents` both depend only on `accepted`
            // (via `accepted.precursors`/`accepted.reaction.reactants()`),
            // not on which route family's template is being scored -- same
            // reasoning as `reaction_energy_cache` above, now extended
            // (v0.5.0, Phase 23C) to close the "process-evidence provider
            // calls still run once per route family" gap that same fix
            // deliberately left open at the time.
            let precursors: Vec<PrecursorSelection> = accepted
                .precursors
                .iter()
                .zip(accepted.reaction.reactants())
                .map(|(id, species)| PrecursorSelection {
                    precursor: id.clone(),
                    formula_units: species.coefficient(),
                })
                .collect();
            let precedents_cache: Option<
                std::result::Result<Vec<ProcessPrecedent>, ProviderError>,
            > = self
                .process_evidence_provider
                .as_ref()
                .map(|provider| provider.precedents(target, &precursors));

            for mut template in applicable_route_family_templates(composition, accepted) {
                let mut evidence = std::mem::take(&mut template.evidence);
                let mut provider_warnings = Vec::new();
                let mut condition_conflicts = Vec::new();
                let process_evidence_provider_consulted = self.process_evidence_provider.is_some();

                if let (Some(cached_energy), Some(cached_phases)) =
                    (&reaction_energy_cache, &competing_phases_cache)
                {
                    match cached_energy.clone() {
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
                        .reactants()
                        .iter()
                        .chain(accepted.reaction.products())
                        .map(|s| s.composition.clone())
                        .collect();
                    match cached_phases.clone() {
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

                if let Some(cached_precedents) = &precedents_cache {
                    match cached_precedents.clone() {
                        Ok(precedents) => {
                            let mut all_conditions: Vec<ConditionPrecedent> = Vec::new();
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
                                all_conditions.extend(precedent.conditions);
                            }
                            // Phase 10: splice any structured, cited condition data into
                            // this template's still-unresolved Heat steps before scoring,
                            // rather than only ever adding free-text evidence that never
                            // changes what's actually planned. Phase 19: every matching
                            // precedent across every returned ProcessPrecedent is
                            // collected first and applied in one order-independent call,
                            // rather than one ProcessPrecedent at a time -- calling this
                            // once per precedent let whichever one happened to run first
                            // silently win any field two precedents both supplied.
                            let (condition_evidence, conflicts) =
                                apply_condition_precedents(&mut template.steps, &all_conditions);
                            evidence.extend(condition_evidence);
                            condition_conflicts.extend(conflicts);
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
                    &condition_conflicts,
                    template.route_family,
                    &self.config.ranking_weights,
                );

                // v0.4.0 Integration: looked up *after* score_plan has
                // already run and returned, deliberately -- this evidence
                // is never a score_plan input (it isn't in that call's
                // argument list at all, unlike `evidence`/
                // `condition_conflicts`/`process_evidence_provider_consulted`
                // above), so nothing about its ordering here can affect
                // `assessment`. Restricted to ConventionalSolidState even
                // though `LiteratureObservationCorpusProvider` already
                // enforces the same restriction internally -- checked at
                // this call site too, so the "never applied to
                // Mechanochemical" claim doesn't rely on any one
                // implementation's internals alone.
                let mut literature_evidence = None;
                if template.route_family == RouteFamily::ConventionalSolidState {
                    if let Some(provider) = &self.literature_evidence_provider {
                        let precursor_compositions: Vec<_> = accepted
                            .reaction
                            .reactants()
                            .iter()
                            .map(|s| s.composition.clone())
                            .collect();
                        match provider.route_evidence(
                            composition,
                            template.route_family,
                            &precursor_compositions,
                        ) {
                            Ok(Some(route_evidence)) => {
                                let found = &route_evidence.assessment;
                                // Always disclosed, not just for the
                                // Conflict/shape-diversity cases -- a clean
                                // unanimous Agreement is exactly the result
                                // most likely to be misread as "the corpus
                                // endorses this temperature" if it were the
                                // one case left silent (pre-commit advisor
                                // review finding).
                                provider_warnings.push(PlanningWarning {
                                    message: format!(
                                        "literature evidence for this exact route: {} \
                                        independent DOI(s) found{}{} -- reference-only, \
                                        never applied to conditions or score",
                                        found.independent_doi_count(),
                                        if found.has_multiple_operation_shapes {
                                            ", with differing reported step counts across \
                                            DOIs"
                                        } else {
                                            ""
                                        },
                                        if found.has_any_conflict() {
                                            ", including a field-level disagreement among \
                                            independent DOIs"
                                        } else {
                                            ""
                                        },
                                    ),
                                    severity: WarningSeverity::Info,
                                });
                                literature_evidence = Some(route_evidence);
                            }
                            Ok(None) => {}
                            Err(err) => provider_warnings.push(PlanningWarning {
                                message: format!(
                                    "literature evidence provider failed for this candidate, \
                                    continuing without it: {err}"
                                ),
                                severity: WarningSeverity::Info,
                            }),
                        }
                    }
                }

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
                    precursors: precursors.clone(),
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
                    literature_evidence,
                });
            }
        }

        // Phase 15B: separate NotRecommended plans out *before* ranking so
        // SearchBudget::max_plans_returned's overflow message (below) only
        // ever counts recommendable plans -- it must never describe a plan
        // that was actually excluded for a stated reason as merely
        // "omitted by budget." Route families absent from `route_suitability`
        // (no provider configured, or that family's assess() call failed)
        // are treated as InsufficientEvidence by construction: `.find(..)`
        // returns `None`, so nothing is filtered -- identical to pre-15B
        // behavior whenever no provider is configured.
        let mut not_recommended = Vec::new();
        if !route_suitability.is_empty() {
            let mut kept = Vec::with_capacity(plans.len());
            for plan in plans {
                let assessment = route_suitability
                    .iter()
                    .find(|a| a.route_family == plan.route_family);
                match assessment {
                    Some(assessment)
                        if derive_recommendation(assessment)
                            == RouteRecommendation::NotRecommended =>
                    {
                        let contradicting_findings = assessment
                            .findings
                            .iter()
                            .filter(|f| f.verdict == SuitabilityVerdict::Contradicts)
                            .cloned()
                            .collect();
                        not_recommended.push(NotRecommendedPlan {
                            plan,
                            contradicting_findings,
                        });
                    }
                    _ => kept.push(plan),
                }
            }
            plans = kept;
        }

        // Explicit abstention (not an empty success) when every generated
        // plan was excluded above -- `applicability` is deliberately left
        // untouched (that's a claim about domain fit, not about whether
        // current evidence favors any specific route), so this uses the
        // same `unresolved` channel `abstain()` already uses for its own
        // abstention case, not a new signal.
        let mut unresolved = Vec::new();
        if plans.is_empty() && !not_recommended.is_empty() {
            unresolved.push(UnresolvedRequirement {
                description: "route selection".to_string(),
                reason: format!(
                    "every generated plan ({} total) was excluded as NotRecommended by \
                    route-suitability findings with strong, uncontested contradicting \
                    evidence -- see not_recommended for the specific plans and findings; \
                    an explicit abstention, not an absence of valid chemistry",
                    not_recommended.len()
                ),
            });
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
            not_recommended,
            rejected_candidates,
            unresolved,
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
    if cfg!(feature = "chematic_crystal") {
        features.push("chematic_crystal".to_string());
    }
    if cfg!(feature = "materials_project") {
        features.push("materials_project".to_string());
    }
    if cfg!(feature = "literature_corpus") {
        features.push("literature_corpus".to_string());
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
        not_recommended: vec![],
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
    for species in reaction.reactants().iter().chain(reaction.products()) {
        for (element, amount) in species.composition.iter() {
            element.symbol().hash(&mut hasher);
            amount.to_bits().hash(&mut hasher);
        }
        species.coefficient().hash(&mut hasher);
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
    use crate::literature_evidence::{
        CrossDoiFieldStatus, LiteratureRouteEvidence, RouteObservationAssessment, SourcedValue,
        StepGroupAssessment, StepGroupKey,
    };
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
        let planner = Planner::builder(barium_titanate_catalog(), generous_config()).build();
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
        let planner = Planner::builder(barium_titanate_catalog(), generous_config()).build();

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
        let planner = Planner::builder(empty, generous_config()).build();

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

    /// Counts real calls rather than answering from a canned per-call
    /// table, so the counters below directly measure how many times
    /// `plan()` actually invokes the provider -- the thing the caching fix
    /// (ROADMAP's "Known risks" duplicate-provider-call entry) changes.
    #[derive(Default)]
    struct CountingThermodynamicProvider {
        reaction_energy_calls: std::cell::Cell<usize>,
        competing_phases_calls: std::cell::Cell<usize>,
    }
    impl ThermodynamicProvider for CountingThermodynamicProvider {
        fn reaction_energy(
            &self,
            _reaction: &BalancedReaction,
            _conditions: &ThermodynamicConditions,
        ) -> std::result::Result<Option<ReactionEnergy>, ProviderError> {
            self.reaction_energy_calls
                .set(self.reaction_energy_calls.get() + 1);
            Ok(None)
        }

        fn competing_phases(
            &self,
            _target: &Composition,
        ) -> std::result::Result<Vec<CompetingPhase>, ProviderError> {
            self.competing_phases_calls
                .set(self.competing_phases_calls.get() + 1);
            Ok(Vec::new())
        }
    }
    // Planner::new takes ownership of its provider, but the test needs a
    // handle to read the counters afterward -- an Arc clone shares the same
    // Cells, so this impl just delegates to the wrapped provider.
    impl ThermodynamicProvider for std::rc::Rc<CountingThermodynamicProvider> {
        fn reaction_energy(
            &self,
            reaction: &BalancedReaction,
            conditions: &ThermodynamicConditions,
        ) -> std::result::Result<Option<ReactionEnergy>, ProviderError> {
            self.as_ref().reaction_energy(reaction, conditions)
        }

        fn competing_phases(
            &self,
            target: &Composition,
        ) -> std::result::Result<Vec<CompetingPhase>, ProviderError> {
            self.as_ref().competing_phases(target)
        }
    }
    struct NoopProcessEvidenceProvider;
    impl ProcessEvidenceProvider for NoopProcessEvidenceProvider {
        fn precedents(
            &self,
            _target: &TargetSpecification,
            _precursors: &[PrecursorSelection],
        ) -> std::result::Result<Vec<ProcessPrecedent>, ProviderError> {
            Ok(Vec::new())
        }
    }

    /// Same counting-not-canned-answer discipline as
    /// `CountingThermodynamicProvider` above, for `precedents` (v0.5.0,
    /// Phase 23C's dedup extension).
    #[derive(Default)]
    struct CountingProcessEvidenceProvider {
        precedents_calls: std::cell::Cell<usize>,
    }
    impl ProcessEvidenceProvider for CountingProcessEvidenceProvider {
        fn precedents(
            &self,
            _target: &TargetSpecification,
            _precursors: &[PrecursorSelection],
        ) -> std::result::Result<Vec<ProcessPrecedent>, ProviderError> {
            self.precedents_calls.set(self.precedents_calls.get() + 1);
            Ok(Vec::new())
        }
    }
    impl ProcessEvidenceProvider for std::rc::Rc<CountingProcessEvidenceProvider> {
        fn precedents(
            &self,
            target: &TargetSpecification,
            precursors: &[PrecursorSelection],
        ) -> std::result::Result<Vec<ProcessPrecedent>, ProviderError> {
            self.as_ref().precedents(target, precursors)
        }
    }

    /// Regression test for the ROADMAP "Known risks" entry: once Phase 12
    /// (multiple route families per accepted precursor set) and Phase 13
    /// (thermodynamic provider) are both configured, `reaction_energy`/
    /// `competing_phases` must not be called once per route family sharing
    /// the same accepted set or composition -- `reaction_energy` should run
    /// at most once per distinct accepted reaction, and `competing_phases`
    /// (a target-only query, invariant across the whole `plan()` call)
    /// should run exactly once regardless of how many accepted sets or
    /// route families exist.
    #[test]
    fn thermodynamic_provider_calls_are_not_duplicated_per_route_family() {
        let provider = std::rc::Rc::new(CountingThermodynamicProvider::default());
        let planner = Planner::builder(barium_titanate_catalog(), generous_config())
            .process_evidence_provider(NoopProcessEvidenceProvider)
            .thermodynamic_provider(provider.clone())
            .build();

        let report = planner
            .plan(&barium_titanate_target(), "2026-08-14T00:00:00Z")
            .unwrap();

        // Multiple route families (ConventionalSolidState, Mechanochemical)
        // apply unconditionally to every accepted set, so this fixture is
        // guaranteed to produce more plans than distinct accepted reactions
        // whenever the provider is actually configured -- otherwise this
        // test would trivially pass with 1 plan and prove nothing.
        assert!(
            report.plans.len() > 1,
            "fixture must produce multiple plans for this test to be meaningful, got {}",
            report.plans.len()
        );
        assert_eq!(
            provider.competing_phases_calls.get(),
            1,
            "competing_phases depends only on the target composition, which is fixed for \
            the whole plan() call -- it must be called exactly once, not once per plan"
        );
        assert!(
            provider.reaction_energy_calls.get() < report.plans.len(),
            "reaction_energy must be cached per accepted reaction, not called once per \
            route-family plan: {} calls for {} plans",
            provider.reaction_energy_calls.get(),
            report.plans.len()
        );
        assert!(
            provider.reaction_energy_calls.get() >= 1,
            "the provider must still actually be consulted at least once"
        );
    }

    /// The `competing_phases` cache is hoisted above the accepted-set loop
    /// (see `plan()`'s comment), so it must stay gated on a non-empty
    /// accepted set explicitly -- otherwise an empty search result would
    /// still make one provider call for a report that ends up with zero
    /// plans, unlike the pre-fix code (whose call site lived entirely
    /// inside `for accepted in &outcome.accepted`, so an empty accepted set
    /// made zero calls by construction).
    #[test]
    fn no_thermodynamic_provider_calls_when_nothing_is_accepted() {
        let provider = std::rc::Rc::new(CountingThermodynamicProvider::default());
        // A catalog that shares no element with the target: search_precursor_sets
        // accepts nothing, so this exercises the empty-accepted-set path
        // with a real (not offline_minimal) thermodynamic provider
        // configured.
        let unrelated_catalog =
            InMemoryPrecursorCatalog::new(vec![candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)])]);
        let planner = Planner::builder(unrelated_catalog, generous_config())
            .process_evidence_provider(NoopProcessEvidenceProvider)
            .thermodynamic_provider(provider.clone())
            .build();

        let report = planner
            .plan(&barium_titanate_target(), "2026-08-14T00:00:00Z")
            .unwrap();

        assert!(report.plans.is_empty());
        assert_eq!(provider.competing_phases_calls.get(), 0);
        assert_eq!(provider.reaction_energy_calls.get(), 0);
    }

    /// Regression test for Phase 23C's dedup extension: `precedents`
    /// depends only on `accepted` (via `accepted.precursors`/
    /// `accepted.reaction.reactants()`), not on which route family's
    /// template is being scored, so it must be called at most once per
    /// distinct accepted precursor set -- not once per route-family plan,
    /// mirroring `thermodynamic_provider_calls_are_not_duplicated_per_route_family`
    /// above for the sibling provider this same fix left un-deduplicated
    /// at the time (PR #37).
    #[test]
    fn process_evidence_provider_calls_are_not_duplicated_per_route_family() {
        let provider = std::rc::Rc::new(CountingProcessEvidenceProvider::default());
        let planner = Planner::builder(barium_titanate_catalog(), generous_config())
            .process_evidence_provider(provider.clone())
            .build();

        let report = planner
            .plan(&barium_titanate_target(), "2026-08-14T00:00:00Z")
            .unwrap();

        assert!(
            report.plans.len() > 1,
            "fixture must produce multiple plans for this test to be meaningful, got {}",
            report.plans.len()
        );
        assert!(
            provider.precedents_calls.get() < report.plans.len(),
            "precedents must be cached per accepted precursor set, not called once per \
            route-family plan: {} calls for {} plans",
            provider.precedents_calls.get(),
            report.plans.len()
        );
        assert!(
            provider.precedents_calls.get() >= 1,
            "the provider must still actually be consulted at least once"
        );
    }

    /// Same "empty accepted set makes zero provider calls" guard as
    /// `no_thermodynamic_provider_calls_when_nothing_is_accepted` above,
    /// for `precedents`.
    #[test]
    fn no_process_evidence_provider_calls_when_nothing_is_accepted() {
        let provider = std::rc::Rc::new(CountingProcessEvidenceProvider::default());
        let unrelated_catalog =
            InMemoryPrecursorCatalog::new(vec![candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)])]);
        let planner = Planner::builder(unrelated_catalog, generous_config())
            .process_evidence_provider(provider.clone())
            .build();

        let report = planner
            .plan(&barium_titanate_target(), "2026-08-14T00:00:00Z")
            .unwrap();

        assert!(report.plans.is_empty());
        assert_eq!(provider.precedents_calls.get(), 0);
    }

    /// AGENTS.md §21.5: one provider failing must not fail the whole plan.
    #[test]
    fn a_failing_optional_provider_degrades_to_a_warning_not_a_failure() {
        let planner = Planner::builder(barium_titanate_catalog(), generous_config())
            .process_evidence_provider(FailingProcessEvidenceProvider)
            .thermodynamic_provider(FailingThermodynamicProvider)
            .build();

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
        let planner = Planner::builder(barium_titanate_catalog(), tight_config).build();

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
        let baseline = Planner::builder(barium_titanate_catalog(), generous_config())
            .build()
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
            Planner::builder(InMemoryPrecursorCatalog::new(with_extra), generous_config())
                .build()
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
        let report = Planner::builder(with_metadata, generous_config())
            .build()
            .plan(&target, "2026-08-14T00:00:00Z")
            .unwrap();
        assert!(!report.plans.is_empty());
    }

    // v0.4.0 Integration: LiteratureEvidenceProvider wiring. Ungated (the
    // trait and its types are always compiled), so these run in the
    // default test suite regardless of the `literature_corpus` feature --
    // the score/ranking/steps non-interference guarantee is core enough
    // that it should not depend on that feature being enabled.

    fn conflicted_literature_evidence() -> LiteratureRouteEvidence {
        let assessment = RouteObservationAssessment {
            target: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
            precursors: [
                composition(&[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
                composition(&[("Ti", 1.0), ("O", 2.0)]),
            ]
            .into_iter()
            .collect(),
            route_family: RouteFamily::ConventionalSolidState,
            has_multiple_operation_shapes: true,
            observed_operation_counts: vec![1, 2],
            step_groups: vec![StepGroupAssessment {
                key: StepGroupKey {
                    heating_operation_count: 1,
                    operation_index: 0,
                },
                source_dois: vec!["10.1/a".to_string(), "10.1/b".to_string()],
                temperature: CrossDoiFieldStatus::Conflict {
                    values: vec![
                        SourcedValue {
                            value: crate::process::TemperatureRange::new(900.0, 900.0).unwrap(),
                            doi: "10.1/a".to_string(),
                        },
                        SourcedValue {
                            value: crate::process::TemperatureRange::new(950.0, 950.0).unwrap(),
                            doi: "10.1/b".to_string(),
                        },
                    ],
                },
                duration: CrossDoiFieldStatus::Unresolved,
                atmosphere: CrossDoiFieldStatus::InsufficientIndependentSources,
            }],
        };
        LiteratureRouteEvidence {
            limitations: crate::literature_evidence::literature_evidence_limitations(&assessment),
            assessment,
        }
    }

    /// Always returns the same conflict-laden evidence, regardless of
    /// query -- deliberately "bad news" (a real Conflict, real shape
    /// diversity), used to prove that even disagreement-carrying evidence
    /// never moves score/confidence/steps.
    struct StubLiteratureEvidenceProvider;
    impl LiteratureEvidenceProvider for StubLiteratureEvidenceProvider {
        fn route_evidence(
            &self,
            _target: &Composition,
            _route_family: RouteFamily,
            _precursors: &[Composition],
        ) -> std::result::Result<Option<LiteratureRouteEvidence>, ProviderError> {
            Ok(Some(conflicted_literature_evidence()))
        }
    }

    /// The opposite case from `conflicted_literature_evidence`: every
    /// field is a clean, unanimous `Agreement`, no shape diversity. This
    /// is the case most likely to be misread as "the corpus endorses this
    /// temperature" if it were the one left with no disclosure warning at
    /// all (pre-commit advisor review finding) -- so the warning must
    /// still fire here too.
    fn agreeing_literature_evidence() -> LiteratureRouteEvidence {
        let assessment = RouteObservationAssessment {
            target: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
            precursors: [
                composition(&[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
                composition(&[("Ti", 1.0), ("O", 2.0)]),
            ]
            .into_iter()
            .collect(),
            route_family: RouteFamily::ConventionalSolidState,
            has_multiple_operation_shapes: false,
            observed_operation_counts: vec![1],
            step_groups: vec![StepGroupAssessment {
                key: StepGroupKey {
                    heating_operation_count: 1,
                    operation_index: 0,
                },
                source_dois: vec!["10.1/a".to_string(), "10.1/b".to_string()],
                temperature: CrossDoiFieldStatus::Agreement {
                    value: crate::process::TemperatureRange::new(900.0, 900.0).unwrap(),
                    source_dois: vec!["10.1/a".to_string(), "10.1/b".to_string()],
                },
                duration: CrossDoiFieldStatus::Unresolved,
                atmosphere: CrossDoiFieldStatus::InsufficientIndependentSources,
            }],
        };
        LiteratureRouteEvidence {
            limitations: crate::literature_evidence::literature_evidence_limitations(&assessment),
            assessment,
        }
    }

    struct AgreeingLiteratureEvidenceProvider;
    impl LiteratureEvidenceProvider for AgreeingLiteratureEvidenceProvider {
        fn route_evidence(
            &self,
            _target: &Composition,
            _route_family: RouteFamily,
            _precursors: &[Composition],
        ) -> std::result::Result<Option<LiteratureRouteEvidence>, ProviderError> {
            Ok(Some(agreeing_literature_evidence()))
        }
    }

    struct FailingLiteratureEvidenceProvider;
    impl LiteratureEvidenceProvider for FailingLiteratureEvidenceProvider {
        fn route_evidence(
            &self,
            _target: &Composition,
            _route_family: RouteFamily,
            _precursors: &[Composition],
        ) -> std::result::Result<Option<LiteratureRouteEvidence>, ProviderError> {
            Err(ProviderError::Unavailable("simulated outage".to_string()))
        }
    }

    /// Records every `(route_family)` it was ever asked about -- a test
    /// that only checks `literature_evidence.is_none()` for a
    /// Mechanochemical plan would pass even if the call-site guard were
    /// deleted, since a real corpus-backed provider also returns nothing
    /// for that route family; this makes the guard itself the thing under
    /// test, not just its typical outcome. `Rc<RefCell<_>>`, not a bare
    /// `RefCell`, so the test can keep its own handle to read the log
    /// after the provider itself has been moved into the `Planner`.
    struct RecordingLiteratureEvidenceProvider {
        queried_route_families: std::rc::Rc<std::cell::RefCell<Vec<RouteFamily>>>,
    }
    impl LiteratureEvidenceProvider for RecordingLiteratureEvidenceProvider {
        fn route_evidence(
            &self,
            _target: &Composition,
            route_family: RouteFamily,
            _precursors: &[Composition],
        ) -> std::result::Result<Option<LiteratureRouteEvidence>, ProviderError> {
            self.queried_route_families.borrow_mut().push(route_family);
            Ok(None)
        }
    }

    #[test]
    fn no_provider_leaves_literature_evidence_none_on_every_plan() {
        let report = Planner::builder(barium_titanate_catalog(), generous_config())
            .build()
            .plan(&barium_titanate_target(), "2026-08-14T00:00:00Z")
            .unwrap();
        assert!(!report.plans.is_empty());
        assert!(report.plans.iter().all(|p| p.literature_evidence.is_none()));
    }

    #[test]
    fn literature_evidence_provider_attaches_evidence_without_changing_score_or_steps() {
        let target = barium_titanate_target();
        let baseline = Planner::builder(barium_titanate_catalog(), generous_config())
            .build()
            .plan(&target, "2026-08-14T00:00:00Z")
            .unwrap();
        let with_evidence = Planner::builder(barium_titanate_catalog(), generous_config())
            .literature_evidence_provider(StubLiteratureEvidenceProvider)
            .build()
            .plan(&target, "2026-08-14T00:00:00Z")
            .unwrap();

        assert_eq!(baseline.plans.len(), with_evidence.plans.len());
        let mut any_conventional_solid_state = false;
        for (before, after) in baseline.plans.iter().zip(with_evidence.plans.iter()) {
            assert_eq!(before.plan_id, after.plan_id);
            assert_eq!(
                before.score, after.score,
                "a configured LiteratureEvidenceProvider must never change score"
            );
            assert_eq!(
                before.confidence, after.confidence,
                "a configured LiteratureEvidenceProvider must never change confidence"
            );
            assert_eq!(
                before.steps, after.steps,
                "a configured LiteratureEvidenceProvider must never auto-fill ProcessStep fields"
            );
            assert!(before.literature_evidence.is_none());
            if after.route_family == RouteFamily::ConventionalSolidState {
                any_conventional_solid_state = true;
                assert!(
                    after.literature_evidence.is_some(),
                    "the stub provider always returns evidence for ConventionalSolidState"
                );
                assert!(
                    after
                        .warnings
                        .iter()
                        .any(|w| w.message.contains("literature evidence")
                            && w.message.contains("independent DOI")),
                    "a Conflict-carrying evidence must surface a disclosure warning: {:?}",
                    after.warnings
                );
            } else {
                assert!(
                    after.literature_evidence.is_none(),
                    "literature evidence must never be attached to a non-ConventionalSolidState plan"
                );
            }
        }
        assert!(
            any_conventional_solid_state,
            "test setup must actually exercise the ConventionalSolidState path"
        );

        // Ranking order itself (not just per-plan score) must also be
        // identical -- score equality alone wouldn't catch a change to
        // the *order* plans are placed in.
        let baseline_order: Vec<&str> = baseline
            .plans
            .iter()
            .map(|p| p.plan_id.0.as_str())
            .collect();
        let with_evidence_order: Vec<&str> = with_evidence
            .plans
            .iter()
            .map(|p| p.plan_id.0.as_str())
            .collect();
        assert_eq!(baseline_order, with_evidence_order);
    }

    #[test]
    fn clean_agreement_still_surfaces_a_disclosure_warning() {
        let report = Planner::builder(barium_titanate_catalog(), generous_config())
            .literature_evidence_provider(AgreeingLiteratureEvidenceProvider)
            .build()
            .plan(&barium_titanate_target(), "2026-08-14T00:00:00Z")
            .unwrap();

        let mut any_conventional_solid_state = false;
        for plan in &report.plans {
            if plan.route_family != RouteFamily::ConventionalSolidState {
                continue;
            }
            any_conventional_solid_state = true;
            assert!(plan.literature_evidence.is_some());
            assert!(
                plan.warnings
                    .iter()
                    .any(|w| w.message.contains("literature evidence")
                        && w.message.contains("independent DOI")),
                "a clean, unanimous Agreement must still surface a disclosure warning -- \
                otherwise it's the one case that silently reads as endorsement: {:?}",
                plan.warnings
            );
        }
        assert!(
            any_conventional_solid_state,
            "test setup must actually exercise the ConventionalSolidState path"
        );
    }

    #[test]
    fn literature_evidence_provider_failure_degrades_to_a_warning() {
        let report = Planner::builder(barium_titanate_catalog(), generous_config())
            .literature_evidence_provider(FailingLiteratureEvidenceProvider)
            .build()
            .plan(&barium_titanate_target(), "2026-08-14T00:00:00Z")
            .unwrap();

        assert!(!report.plans.is_empty());
        let mut any_conventional_solid_state = false;
        for plan in &report.plans {
            assert!(plan.literature_evidence.is_none());
            // Only ConventionalSolidState plans ever call the provider at
            // all (the Mechanochemical call-site guard) -- a Mechanochemical
            // plan correctly has no such warning, since it was never asked.
            if plan.route_family == RouteFamily::ConventionalSolidState {
                any_conventional_solid_state = true;
                assert!(
                    plan.warnings
                        .iter()
                        .any(|w| w.message.contains("literature evidence provider failed")),
                    "expected the provider failure reflected as a warning: {:?}",
                    plan.warnings
                );
            }
        }
        assert!(
            any_conventional_solid_state,
            "test setup must actually exercise the ConventionalSolidState path"
        );
    }

    #[test]
    fn literature_evidence_provider_is_never_asked_about_mechanochemical() {
        let log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let recorder = RecordingLiteratureEvidenceProvider {
            queried_route_families: log.clone(),
        };
        let report = Planner::builder(barium_titanate_catalog(), generous_config())
            .literature_evidence_provider(recorder)
            .build()
            .plan(&barium_titanate_target(), "2026-08-14T00:00:00Z")
            .unwrap();

        // Sanity: this target really does produce Mechanochemical plans
        // too (Phase 12's unconditional route-family applicability), so
        // the absence of a Mechanochemical query below is a real
        // guard-is-working signal, not a vacuous "nothing to ask about."
        assert!(
            report
                .plans
                .iter()
                .any(|p| p.route_family == RouteFamily::Mechanochemical)
        );
        let queried = log.borrow();
        assert!(
            !queried.is_empty(),
            "the recorder must have been called at least once (for ConventionalSolidState)"
        );
        assert!(
            queried
                .iter()
                .all(|&rf| rf == RouteFamily::ConventionalSolidState),
            "the literature evidence provider must never be asked about a route family other \
            than ConventionalSolidState: {queried:?}"
        );
    }
}
