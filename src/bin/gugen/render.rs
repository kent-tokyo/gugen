use gugen::{BalancedReaction, Composition, ProcessStep, ReactionSpecies, SynthesisPlan};
#[cfg(feature = "commercial_catalog")]
use gugen::{CommercialCatalogLoadReport, CommercialPlanAssessment};
use gugen::{SynthesisPlanningReport, TargetSummary};

// --- Rendering (JSON output goes through serde directly; the helpers below
// are only for `--format markdown` and `gugen explain`) ---

fn format_number(amount: f64) -> String {
    let rounded = amount.round();
    if (amount - rounded).abs() < 1e-9 {
        format!("{}", rounded as i64)
    } else {
        format!("{amount:.3}")
    }
}

/// `Composition` iterates by element symbol (`BTreeMap` order), which is
/// alphabetical, not conventional chemical-formula order (e.g. Ba/Ti/O, not
/// Ba/O/Ti) -- concatenating symbols+amounts directly would print something
/// that looks like a formula but isn't one (`BaO3Ti` for BaTiO3). Rendered
/// as explicit element:amount pairs instead, so it's honest about what it
/// is: gugen has no formula-notation orderer.
pub(crate) fn format_composition(c: &Composition) -> String {
    c.iter()
        .map(|(el, amt)| format!("{}:{}", el.symbol(), format_number(amt)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_species(species: &[ReactionSpecies]) -> String {
    species
        .iter()
        .map(|s| {
            format!(
                "{}x({})",
                s.coefficient(),
                format_composition(&s.composition)
            )
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn format_reaction(r: &BalancedReaction) -> String {
    format!(
        "{} -> {}",
        format_species(r.reactants()),
        format_species(r.products())
    )
}

/// `None` is rendered as "unresolved" rather than left blank: an unresolved
/// condition is a stated fact about this plan (AGENTS.md §4.1), not an
/// absence of output.
fn opt_debug<T: std::fmt::Debug>(value: &Option<T>) -> String {
    value
        .as_ref()
        .map_or_else(|| "unresolved".to_string(), |v| format!("{v:?}"))
}

fn format_step(step: &ProcessStep) -> String {
    use ProcessStep::*;
    match step {
        Weigh { materials } => {
            let parts: Vec<String> = materials
                .iter()
                .map(|m| format!("{} x{}", m.precursor, m.formula_units))
                .collect();
            format!("Weigh: {}", parts.join(", "))
        }
        Mix { method } => format!("Mix ({method:?})"),
        Grind { method, duration } => {
            format!("Grind ({method:?}), duration={}", opt_debug(duration))
        }
        Form { method, pressure } => {
            format!("Form ({method:?}), pressure={}", opt_debug(pressure))
        }
        Heat {
            purpose,
            temperature,
            duration,
            atmosphere,
            ramp,
        } => format!(
            "Heat ({purpose:?}): temperature={}, duration={}, atmosphere={}, ramp={}",
            opt_debug(temperature),
            opt_debug(duration),
            opt_debug(atmosphere),
            opt_debug(ramp)
        ),
        Cool { mode } => format!("Cool ({mode:?})"),
        IntermediateCharacterization { method, purpose } => {
            format!("Characterize ({method:?}): {purpose}")
        }
    }
}

pub(crate) fn render_plan_detail(target: &TargetSummary, plan: &SynthesisPlan) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "## Plan {} (score {:.3})\n\n",
        plan.plan_id,
        plan.score.total_ranking_score.value()
    ));
    out.push_str(&format!(
        "- Target: {}\n",
        format_composition(&target.composition)
    ));
    out.push_str(&format!("- Route family: {:?}\n", plan.route_family));
    if let Some(reaction) = &plan.balanced_reaction {
        out.push_str(&format!("- Reaction: {}\n", format_reaction(reaction)));
    }
    out.push_str(&format!(
        "- Manual review required: {}\n",
        plan.manual_review_required
    ));
    out.push_str(&format!(
        "- Applicability: {:?} -- {}\n\n",
        plan.applicability.level,
        plan.applicability.rationale.join("; ")
    ));

    out.push_str("### Steps\n\n");
    for planned in &plan.steps {
        out.push_str(&format!(
            "- [{:?}] {}\n",
            planned.requirement,
            format_step(&planned.step)
        ));
    }
    out.push('\n');

    out.push_str("### Score breakdown\n\n```\n");
    out.push_str(&format!("{:#?}\n", plan.score));
    out.push_str("```\n\n### Confidence\n\n```\n");
    out.push_str(&format!("{:#?}\n", plan.confidence));
    out.push_str("```\n\n");

    if !plan.evidence.is_empty() {
        out.push_str("### Evidence\n\n");
        for e in &plan.evidence {
            out.push_str(&format!(
                "- [{:?}/{:?}] {}\n",
                e.strength, e.kind, e.statement
            ));
        }
        out.push('\n');
    }

    if !plan.warnings.is_empty() {
        out.push_str("### Warnings\n\n");
        for w in &plan.warnings {
            out.push_str(&format!("- [{:?}] {}\n", w.severity, w.message));
        }
        out.push('\n');
    }

    if !plan.assumptions.is_empty() {
        out.push_str("### Assumptions\n\n");
        for a in &plan.assumptions {
            out.push_str(&format!("- {}\n", a.statement));
        }
        out.push('\n');
    }

    if !plan.unresolved.is_empty() {
        out.push_str("### Unresolved\n\n");
        for u in &plan.unresolved {
            out.push_str(&format!("- {}: {}\n", u.description, u.reason));
        }
        out.push('\n');
    }

    out
}

pub(crate) fn render_report_markdown(report: &SynthesisPlanningReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Synthesis Planning Report (schema v{})\n\n",
        report.schema_version
    ));
    out.push_str(&format!(
        "**Target:** {}\n\n",
        format_composition(&report.target.composition)
    ));
    out.push_str(&format!(
        "**Applicability:** {:?} -- {}\n\n",
        report.applicability.level,
        report.applicability.rationale.join("; ")
    ));

    if !report.warnings.is_empty() {
        out.push_str("**Report-level warnings:**\n\n");
        for w in &report.warnings {
            out.push_str(&format!("- [{:?}] {}\n", w.severity, w.message));
        }
        out.push('\n');
    }

    if report.plans.is_empty() {
        out.push_str("_No plans were produced for this target._\n\n");
    }
    for plan in &report.plans {
        out.push_str(&render_plan_detail(&report.target, plan));
    }

    if !report.rejected_candidates.is_empty() {
        out.push_str("## Rejected candidates\n\n");
        for r in &report.rejected_candidates {
            let ids: Vec<&str> = r.precursors.iter().map(|p| p.0.as_str()).collect();
            out.push_str(&format!(
                "- {ids:?} {:?}: {}\n",
                r.reason_codes, r.explanation
            ));
        }
        out.push('\n');
    }

    out.push_str(&format!(
        "_Generated {} by gugen {}._\n",
        report.provenance.execution_timestamp, report.provenance.gugen_version
    ));
    out
}

#[cfg(feature = "commercial_catalog")]
pub(crate) fn render_commercial_report_markdown(
    target: &TargetSummary,
    plans: &[SynthesisPlan],
    assessments: &[CommercialPlanAssessment],
    load_report: &CommercialCatalogLoadReport,
) -> String {
    let mut out = String::new();
    out.push_str("# Commercial Precursor Assessment\n\n");
    out.push_str(&format!(
        "**Target:** {}\n\n",
        format_composition(&target.composition)
    ));
    out.push_str(
        "_gugen does not certify commercial data: prices are estimates, availability may be \
        stale, and product suitability for a given synthesis is not certified. Verify vendor \
        documentation and SDS sheets separately._\n\n",
    );
    out.push_str(&format!(
        "**Commercial catalog load:** {} accepted, {} duplicate offer id(s) collapsed, {} \
        rejected.\n\n",
        load_report.accepted,
        load_report.duplicate_offer_ids_collapsed,
        load_report.rejected.len()
    ));

    for (plan, assessment) in plans.iter().zip(assessments.iter()) {
        out.push_str(&render_commercial_assessment_markdown(plan, assessment));
    }
    out
}

#[cfg(feature = "commercial_catalog")]
fn render_commercial_assessment_markdown(
    plan: &SynthesisPlan,
    assessment: &CommercialPlanAssessment,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "## Plan {} ({:?})\n\n",
        assessment.plan_id, plan.route_family
    ));
    out.push_str(&format!(
        "- Every precursor has a catalog match: {}\n\n",
        assessment.every_precursor_has_a_match
    ));

    if !assessment.unmatched_precursors.is_empty() {
        out.push_str("### Unmatched precursors\n\n");
        for (id, comp) in &assessment.unmatched_precursors {
            out.push_str(&format!("- {id} ({})\n", format_composition(comp)));
        }
        out.push('\n');
    }

    if assessment.combinations.is_empty() {
        out.push_str("_No procurement combinations were found._\n\n");
    } else {
        out.push_str("### Procurement combinations (ranked)\n\n");
        for (rank, combo) in assessment.combinations.iter().enumerate() {
            out.push_str(&format!("{}. `{}`\n", rank + 1, combo.combination_id));
            for sel in &combo.selections {
                out.push_str(&format!(
                    "   - {} <- offer {} ({:.3} g theoretical)\n",
                    sel.precursor, sel.offer_id, sel.theoretical_pure_mass_required_grams
                ));
            }
            match combo.total_cost {
                Some(cost) => out.push_str(&format!(
                    "   - Estimated subtotal: {} {}\n",
                    cost.minor_units(),
                    cost.currency()
                )),
                None => out.push_str(
                    "   - Total cost unknown (unresolved price/package size, or the selected \
                    offers span more than one currency -- gugen never converts currencies).\n",
                ),
            }
            match combo.min_purity {
                Some(p) => out.push_str(&format!("   - Min purity: {:.4}\n", p.value())),
                None => out.push_str(
                    "   - Min purity: unknown (at least one selection's purity is unknown)\n",
                ),
            }
            match combo.total_excess_mass_grams {
                Some(mass) => out.push_str(&format!("   - Total excess mass: {mass:.3} g\n")),
                None => out.push_str("   - Total excess mass: unknown\n"),
            }
        }
        out.push('\n');
    }

    if !assessment.rejected_offers.is_empty() {
        out.push_str("### Rejected offers\n\n");
        for r in &assessment.rejected_offers {
            out.push_str(&format!(
                "- {} {:?}: {}\n",
                r.precursor, r.reason_codes, r.explanation
            ));
        }
        out.push('\n');
    }

    if !assessment.unresolved_commercial_fields.is_empty() {
        out.push_str("### Unresolved fields\n\n");
        for u in &assessment.unresolved_commercial_fields {
            out.push_str(&format!(
                "- {} / offer {}: {}\n",
                u.precursor, u.offer_id, u.field
            ));
        }
        out.push('\n');
    }

    if !assessment.warnings.is_empty() {
        out.push_str("### Warnings\n\n");
        for w in &assessment.warnings {
            out.push_str(&format!("- [{:?}] {}\n", w.severity, w.message));
        }
        out.push('\n');
    }

    out.push_str("### Search budget\n\n```\n");
    out.push_str(&format!("{:#?}\n", assessment.search_budget));
    out.push_str("```\n\n");

    out
}

#[cfg(feature = "commercial_catalog")]
pub(crate) fn render_commercial_report_csv(
    assessments: &[CommercialPlanAssessment],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut writer = csv::Writer::from_writer(vec![]);
    writer.write_record([
        "plan_id",
        "every_precursor_has_a_match",
        "combination_rank",
        "combination_id",
        "total_cost_minor_units",
        "currency",
        "all_costs_known",
        "max_lead_time_days",
        "min_purity",
        "total_excess_mass_grams",
        "all_availability_acceptable",
        "note",
    ])?;

    for assessment in assessments {
        if assessment.combinations.is_empty() {
            writer.write_record([
                assessment.plan_id.0.as_str(),
                &assessment.every_precursor_has_a_match.to_string(),
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "no procurement combination found",
            ])?;
            continue;
        }
        for (rank, combo) in assessment.combinations.iter().enumerate() {
            let (cost, currency) = match combo.total_cost {
                Some(cost) => (cost.minor_units().to_string(), cost.currency().to_string()),
                None => (String::new(), String::new()),
            };
            // `all_costs_known` is per-selection; a combination can still
            // have no `total_cost` if the selected offers span more than
            // one currency (gugen never converts currencies) -- state that
            // here so a blank cost next to `all_costs_known=true` doesn't
            // read as a bug in this writer.
            let note = if combo.total_cost.is_none() && combo.all_costs_known {
                "total unavailable: selected offers span more than one currency"
            } else {
                ""
            };
            writer.write_record([
                assessment.plan_id.0.as_str(),
                &assessment.every_precursor_has_a_match.to_string(),
                &(rank + 1).to_string(),
                &combo.combination_id,
                &cost,
                &currency,
                &combo.all_costs_known.to_string(),
                &combo
                    .max_lead_time_days
                    .map_or(String::new(), |d| d.to_string()),
                &combo
                    .min_purity
                    .map_or(String::new(), |p| p.value().to_string()),
                &combo
                    .total_excess_mass_grams
                    .map_or(String::new(), |m| m.to_string()),
                &combo.all_availability_acceptable.to_string(),
                note,
            ])?;
        }
    }

    Ok(String::from_utf8(writer.into_inner()?)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "commercial_catalog")]
    use crate::commands::tests::golden_commercial_assessment;
    use crate::commands::tests::golden_report;

    /// AGENTS.md §21.6: markdown output schema must not change
    /// unintentionally either. Same discipline as the JSON golden test.
    #[test]
    fn markdown_output_matches_the_golden_snapshot() {
        let rendered = render_report_markdown(&golden_report());
        let golden = include_str!("../../../tests/fixtures/batio3_report.md");
        assert_eq!(rendered.trim_end(), golden.trim_end());
    }

    /// AGENTS.md §21.6: markdown output schema must not change unintentionally.
    #[cfg(feature = "commercial_catalog")]
    #[test]
    fn commercial_plan_markdown_output_matches_the_golden_snapshot() {
        let (report, assessments, load_report) = golden_commercial_assessment();
        let rendered = render_commercial_report_markdown(
            &report.target,
            &report.plans,
            &assessments,
            &load_report,
        );
        let golden = include_str!("../../../tests/fixtures/commercial_plan_report.md");
        assert_eq!(rendered.trim_end(), golden.trim_end());
    }
}
