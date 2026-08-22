//! Public assessment API: `assess_commercial_precursors`/
//! `assess_commercial_plans`. Never mutates `SynthesisPlan` -- see the
//! module root doc comment for the structural invariance argument.

use super::model::{
    CommercialCatalogError, CommercialCombination, CommercialExclusion, CommercialExclusionCode,
    CommercialOfferId, CommercialPlanAssessment, CommercialPlanningConfig,
    CommercialPlanningRequest, CommercialPrecursorCatalog, CommercialPrecursorOffer,
    CommercialWarning, SearchBudgetSummary, UnresolvedCommercialField,
};
use super::quantity::{compute_offer_quantity, molar_mass_g_per_mol, unresolved_fields_for};
use super::search::{
    OfferCandidate, build_combination, hard_constraint_violations, offer_rank_order,
    search_combinations,
};
use crate::composition::Composition;
use crate::precursor::PrecursorId;
use crate::reaction::BalancedReaction;
use crate::report::{SynthesisPlan, WarningSeverity};
use std::collections::BTreeSet;

fn validate_request(request: &CommercialPlanningRequest) -> Result<(), CommercialCatalogError> {
    if request.target_batch_mass_grams.is_some() && request.target_composition.is_none() {
        return Err(CommercialCatalogError::InconsistentRequest {
            reason: "target_batch_mass_grams was set without target_composition".to_string(),
        });
    }
    Ok(())
}

fn degraded_assessment(
    plan: &SynthesisPlan,
    message: String,
    severity: WarningSeverity,
) -> CommercialPlanAssessment {
    CommercialPlanAssessment {
        plan_id: plan.plan_id.clone(),
        every_precursor_has_a_match: false,
        combinations: Vec::new(),
        unmatched_precursors: Vec::new(),
        rejected_offers: Vec::new(),
        unresolved_commercial_fields: Vec::new(),
        warnings: vec![CommercialWarning { message, severity }],
        search_budget: SearchBudgetSummary {
            combinations_evaluated: 0,
            combinations_omitted: 0,
            is_exhaustive: true,
        },
    }
}

/// Resolves the target's stoichiometric scale factor from
/// `request.target_batch_mass_grams`/`target_composition`, if both are set
/// and the target composition is actually found among this specific plan's
/// reaction products. Falls back to `1.0` (the reaction's own minimal
/// integer scale) otherwise, with a warning explaining why -- this is a
/// per-plan condition, not a request-level error (a batch mass request
/// legitimately doesn't apply to every plan in a heterogeneous batch).
fn resolve_target_scale(
    request: &CommercialPlanningRequest,
    reaction: &BalancedReaction,
    warnings: &mut Vec<CommercialWarning>,
) -> f64 {
    let (Some(target_mass), Some(target_composition)) =
        (request.target_batch_mass_grams, &request.target_composition)
    else {
        return 1.0;
    };
    let Some(target_species) = reaction
        .products
        .iter()
        .find(|species| &species.composition == target_composition)
    else {
        warnings.push(CommercialWarning {
            message: "target_composition was not found among this plan's reaction products; \
                stoichiometric quantities use the reaction's own minimal integer scale instead \
                of the requested batch mass"
                .to_string(),
            severity: WarningSeverity::Caution,
        });
        return 1.0;
    };
    let target_basis_grams =
        target_species.coefficient as f64 * molar_mass_g_per_mol(&target_species.composition);
    if target_basis_grams <= 0.0 {
        return 1.0;
    }
    target_mass / target_basis_grams
}

pub fn assess_commercial_precursors(
    plan: &SynthesisPlan,
    catalog: &CommercialPrecursorCatalog,
    request: &CommercialPlanningRequest,
    config: &CommercialPlanningConfig,
) -> Result<CommercialPlanAssessment, CommercialCatalogError> {
    validate_request(request)?;

    let Some(reaction) = &plan.balanced_reaction else {
        return Ok(degraded_assessment(
            plan,
            "plan has no balanced reaction; nothing to match against the catalog".to_string(),
            WarningSeverity::Caution,
        ));
    };

    if plan.precursors.len() != reaction.reactants.len() {
        return Ok(degraded_assessment(
            plan,
            format!(
                "plan.precursors (len {}) and plan.balanced_reaction.reactants (len {}) are not \
                 the same length; cannot align precursor identities with reaction stoichiometry",
                plan.precursors.len(),
                reaction.reactants.len()
            ),
            WarningSeverity::Severe,
        ));
    }

    let mut warnings = Vec::new();
    let scale = resolve_target_scale(request, reaction, &mut warnings);

    let mut unmatched_precursors = Vec::new();
    let mut rejected_offers = Vec::new();
    let mut any_row_truncated = false;
    let mut rows: Vec<Vec<OfferCandidate>> = Vec::new();
    let mut row_meta: Vec<(PrecursorId, Composition, u64, f64)> = Vec::new();

    for (selection, species) in plan.precursors.iter().zip(&reaction.reactants) {
        let theoretical_pure_mass_required_grams =
            scale * species.coefficient as f64 * molar_mass_g_per_mol(&species.composition);
        row_meta.push((
            selection.precursor.clone(),
            species.composition.clone(),
            species.coefficient,
            theoretical_pure_mass_required_grams,
        ));

        let raw_candidates: Vec<&CommercialPrecursorOffer> =
            catalog.offers_matching(&species.composition).collect();
        if raw_candidates.is_empty() {
            unmatched_precursors.push((selection.precursor.clone(), species.composition.clone()));
            rows.push(Vec::new());
            continue;
        }

        let mut survivors: Vec<OfferCandidate> = Vec::new();
        for offer in raw_candidates {
            let quantity = compute_offer_quantity(offer, theoretical_pure_mass_required_grams);
            let mut codes = hard_constraint_violations(offer, request);
            if quantity.cost_overflowed {
                codes.push(CommercialExclusionCode::CostOverflow);
            }
            if codes.is_empty() {
                survivors.push(OfferCandidate {
                    offer,
                    unresolved_fields: unresolved_fields_for(offer),
                    quantity,
                });
            } else {
                rejected_offers.push(CommercialExclusion {
                    precursor: selection.precursor.clone(),
                    offer_id: Some(offer.offer_id.clone()),
                    reason_codes: codes,
                    explanation: format!(
                        "offer {} excluded from precursor {}",
                        offer.offer_id, selection.precursor
                    ),
                });
            }
        }

        survivors.sort_by(offer_rank_order);

        if survivors.len() > config.max_offers_per_precursor {
            any_row_truncated = true;
            for dropped in survivors.split_off(config.max_offers_per_precursor) {
                rejected_offers.push(CommercialExclusion {
                    precursor: selection.precursor.clone(),
                    offer_id: Some(dropped.offer.offer_id.clone()),
                    reason_codes: vec![CommercialExclusionCode::OfferCountCapExceeded],
                    explanation: format!(
                        "more than max_offers_per_precursor ({}) offers matched this precursor; \
                         lower-ranked offers were dropped",
                        config.max_offers_per_precursor
                    ),
                });
            }
            warnings.push(CommercialWarning {
                message: format!(
                    "precursor {} had more matching offers than max_offers_per_precursor; \
                     the result set is not exhaustive for this precursor",
                    selection.precursor
                ),
                severity: WarningSeverity::Info,
            });
        }

        if survivors.is_empty() {
            unmatched_precursors.push((selection.precursor.clone(), species.composition.clone()));
        }
        rows.push(survivors);
    }

    let every_precursor_has_a_match = unmatched_precursors.is_empty();
    let (index_vectors, evaluated, total_space) = if every_precursor_has_a_match {
        search_combinations(&rows, config, request.max_total_cost)
    } else {
        (Vec::new(), 0, 0)
    };

    let combinations_omitted = total_space.saturating_sub(evaluated as u64);
    let is_exhaustive =
        every_precursor_has_a_match && !any_row_truncated && combinations_omitted == 0;
    if every_precursor_has_a_match && !is_exhaustive {
        warnings.push(CommercialWarning {
            message: format!(
                "combination search is not exhaustive: {evaluated} combination(s) evaluated, \
                 {combinations_omitted} omitted"
            ),
            severity: WarningSeverity::Info,
        });
    }

    // max_total_cost was already applied as a hard filter *inside* the
    // search, before max_results_returned truncation -- see
    // `passes_max_total_cost`'s doc comment for why filtering here, after
    // truncation, would be wrong (it could return zero combinations even
    // when a lower-ranked, budget-satisfying one exists).
    let combinations: Vec<CommercialCombination> = index_vectors
        .iter()
        .map(|indices| build_combination(indices, &rows, &row_meta))
        .collect();

    let mut unresolved_commercial_fields: Vec<UnresolvedCommercialField> = Vec::new();
    let mut unresolved_seen: BTreeSet<(PrecursorId, CommercialOfferId, &'static str)> =
        BTreeSet::new();
    for combination in &combinations {
        for selection in &combination.selections {
            for &field in &selection.unresolved_fields {
                let key = (
                    selection.precursor.clone(),
                    selection.offer_id.clone(),
                    field,
                );
                if unresolved_seen.insert(key) {
                    unresolved_commercial_fields.push(UnresolvedCommercialField {
                        precursor: selection.precursor.clone(),
                        offer_id: selection.offer_id.clone(),
                        field,
                    });
                }
            }
        }
    }

    if request.max_total_cost.is_some()
        && every_precursor_has_a_match
        && evaluated > 0
        && combinations.is_empty()
    {
        // `evaluated > 0` rules out a zero-precursor plan (nothing was ever
        // searched, so there's nothing to blame on the ceiling). Phrased
        // over "the evaluated search space", not the whole combination
        // space -- the heuristic tier can exhaust its budget without
        // examining every combination, so claiming "all combinations
        // exceeded the ceiling" would overclaim on that path (the
        // is_exhaustive warning already flags that the search was
        // incomplete; this warning must not contradict it).
        warnings.push(CommercialWarning {
            message: "no combination in the evaluated search space satisfied max_total_cost"
                .to_string(),
            severity: WarningSeverity::Caution,
        });
    } else if let Some(max_total_cost) = request.max_total_cost {
        if combinations.iter().any(|c| {
            c.total_cost
                .is_none_or(|cost| cost.currency() != max_total_cost.currency())
        }) {
            warnings.push(CommercialWarning {
                message: "max_total_cost could not be verified for one or more combinations \
                    whose total cost is unknown or in a different currency"
                    .to_string(),
                severity: WarningSeverity::Caution,
            });
        }
    }

    Ok(CommercialPlanAssessment {
        plan_id: plan.plan_id.clone(),
        every_precursor_has_a_match,
        combinations,
        unmatched_precursors,
        rejected_offers,
        unresolved_commercial_fields,
        warnings,
        search_budget: SearchBudgetSummary {
            combinations_evaluated: evaluated,
            combinations_omitted,
            is_exhaustive,
        },
    })
}

/// Maps `assess_commercial_precursors` over each plan independently (fresh
/// `max_combinations_evaluated` budget per plan). `Err` is reserved for a
/// self-contradictory `request` -- checked once, up front, since it is
/// identical for every plan in the batch; a single malformed *plan* never
/// aborts the batch (see `assess_commercial_precursors`'s degraded-`Ok`
/// handling for plan-shape issues).
pub fn assess_commercial_plans(
    plans: &[SynthesisPlan],
    catalog: &CommercialPrecursorCatalog,
    request: &CommercialPlanningRequest,
    config: &CommercialPlanningConfig,
) -> Result<Vec<CommercialPlanAssessment>, CommercialCatalogError> {
    validate_request(request)?;
    plans
        .iter()
        .map(|plan| assess_commercial_precursors(plan, catalog, request, config))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::model::*;
    use super::*;
    use crate::commercial_catalog::test_support::*;

    #[test]
    fn assess_commercial_precursors_matches_and_ranks_offers() {
        let plan = barium_titanate_plan();
        let catalog = baco3_tio2_catalog(default_baco3_tio2_offers());
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();

        assert!(assessment.every_precursor_has_a_match);
        assert!(!assessment.combinations.is_empty());
        let best = &assessment.combinations[0];
        // The cheapest USD-priced offer for each row should win the top combination.
        let selected_ids: Vec<&str> = best
            .selections
            .iter()
            .map(|s| s.offer_id.0.as_str())
            .collect();
        assert!(selected_ids.contains(&"BACO3-CHEAP"));
        assert!(selected_ids.contains(&"TIO2-CHEAP"));
        assert_eq!(best.total_cost, Some(money(3600, "USD"))); // 1000*2 + 800*2, see quantity test below
    }

    #[test]
    fn assess_commercial_precursors_hand_checked_quantity_math() {
        let plan = barium_titanate_plan();
        let catalog = baco3_tio2_catalog(default_baco3_tio2_offers());
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        let best = &assessment.combinations[0];
        let baco3 = best
            .selections
            .iter()
            .find(|s| s.offer_id.0 == "BACO3-CHEAP")
            .unwrap();
        // BaCO3 molar mass = 137.327 + 12.011 + 3*15.999 = 197.335 g/mol,
        // coefficient 1, scale 1.0 -> theoretical requirement 197.335 g.
        assert!((baco3.theoretical_pure_mass_required_grams - 197.335).abs() < 1e-6);
        // purity-adjusted: 197.335 / 0.99 = 199.328...
        let adjusted = baco3.purity_adjusted_purchase_mass_grams.unwrap();
        assert!((adjusted - 197.335 / 0.99).abs() < 1e-6);
        // package_mass 100g -> ceil(199.33.../100) = 2 packages
        assert_eq!(baco3.package_count, Some(2));
        assert_eq!(baco3.purchased_mass_grams, Some(200.0));
        assert!(baco3.excess_mass_grams.unwrap() > 0.0);
        assert_eq!(baco3.subtotal, Some(money(2000, "USD")));
        assert!(
            !baco3.assumptions.is_empty(),
            "a purity adjustment was applied, so the caveat must be present"
        );
    }

    #[test]
    fn assess_commercial_precursors_no_balanced_reaction_is_a_degraded_ok_not_an_error() {
        let mut plan = barium_titanate_plan();
        plan.balanced_reaction = None;
        let catalog = baco3_tio2_catalog(default_baco3_tio2_offers());
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        assert!(!assessment.every_precursor_has_a_match);
        assert!(assessment.combinations.is_empty());
        assert!(!assessment.warnings.is_empty());
    }

    #[test]
    fn assess_commercial_precursors_precursor_reactant_length_mismatch_is_a_degraded_ok() {
        let mut plan = barium_titanate_plan();
        plan.precursors.push(plan.precursors[0].clone());
        let catalog = baco3_tio2_catalog(default_baco3_tio2_offers());
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        assert!(!assessment.every_precursor_has_a_match);
        assert!(assessment.combinations.is_empty());
    }

    #[test]
    fn assess_commercial_precursors_zero_precursor_plan_does_not_warn_about_cost_ceiling() {
        // A plan with nothing to buy (rows empty) is a degenerate but
        // valid case per finding 2's "don't assume plan shape" guard.
        // every_precursor_has_a_match is vacuously true here (zero
        // unmatched precursors), so without the `evaluated > 0` guard the
        // max_total_cost-excluded-everything warning would incorrectly
        // fire for a plan where nothing was ever searched.
        let mut plan = barium_titanate_plan();
        plan.precursors.clear();
        if let Some(reaction) = plan.balanced_reaction.as_mut() {
            reaction.reactants.clear();
        }
        let catalog = baco3_tio2_catalog(default_baco3_tio2_offers());
        let request = CommercialPlanningRequest {
            max_total_cost: Some(money(1, "USD")),
            ..Default::default()
        };
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &request,
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        assert!(assessment.combinations.is_empty());
        assert!(
            !assessment
                .warnings
                .iter()
                .any(|w| w.message.contains("max_total_cost")),
            "a plan with nothing to buy must not claim the cost ceiling excluded \
             anything: {:?}",
            assessment.warnings
        );
    }

    #[test]
    fn assess_commercial_precursors_unmatched_precursor_is_reported_not_silently_dropped() {
        let plan = barium_titanate_plan();
        // Only BaCO3 offers -- TiO2 has nothing in the catalog.
        let catalog = baco3_tio2_catalog(vec![priced_offer(
            "BACO3-ONLY",
            "BaCO3",
            "Example Materials Ltd.",
            Some(0.99),
            Some(100.0),
            Some((1000, "USD")),
            Some(5),
            Some(AvailabilityStatus::InStock),
        )]);
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        assert!(!assessment.every_precursor_has_a_match);
        assert_eq!(assessment.unmatched_precursors.len(), 1);
        assert!(assessment.combinations.is_empty());
    }

    #[test]
    fn assess_commercial_precursors_minimum_purity_filtering() {
        let plan = barium_titanate_plan();
        let mut offers = default_baco3_tio2_offers();
        // A high-purity TiO2 offer so the 0.995 threshold below isolates
        // the BaCO3-side filtering this test actually targets, rather than
        // also starving the TiO2 row (both default TiO2 offers are < 0.995).
        offers.push(priced_offer(
            "TIO2-HIGHPURITY",
            "TiO2",
            "Example Materials Ltd.",
            Some(0.999),
            Some(50.0),
            Some((900, "USD")),
            Some(5),
            Some(AvailabilityStatus::InStock),
        ));
        let catalog = baco3_tio2_catalog(offers);
        let request = CommercialPlanningRequest {
            min_purity: Some(PurityFraction::new(0.995).unwrap()),
            ..Default::default()
        };
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &request,
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        let best = &assessment.combinations[0];
        // Only BACO3-PREMIUM (0.999) clears the 0.995 bar for BaCO3.
        assert!(
            best.selections
                .iter()
                .any(|s| s.offer_id.0 == "BACO3-PREMIUM")
        );
        assert!(assessment.rejected_offers.iter().any(|r| {
            r.offer_id.as_ref().map(|o| o.0.as_str()) == Some("BACO3-CHEAP")
                && r.reason_codes
                    .contains(&CommercialExclusionCode::PurityBelowMinimum)
        }));
    }

    #[test]
    fn assess_commercial_precursors_max_lead_time_filtering() {
        let plan = barium_titanate_plan();
        let catalog = baco3_tio2_catalog(default_baco3_tio2_offers());
        let request = CommercialPlanningRequest {
            max_lead_time_days: Some(10),
            ..Default::default()
        };
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &request,
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        assert!(assessment.rejected_offers.iter().any(|r| {
            r.offer_id.as_ref().map(|o| o.0.as_str()) == Some("BACO3-PREMIUM")
                && r.reason_codes
                    .contains(&CommercialExclusionCode::LeadTimeExceedsMaximum)
        }));
    }

    #[test]
    fn assess_commercial_precursors_availability_filtering() {
        let plan = barium_titanate_plan();
        let mut offers = default_baco3_tio2_offers();
        offers.push(priced_offer(
            "BACO3-DISCONTINUED",
            "BaCO3",
            "Example Materials Ltd.",
            Some(0.9999),
            Some(100.0),
            Some((1, "USD")),
            Some(1),
            Some(AvailabilityStatus::Discontinued),
        ));
        let catalog = baco3_tio2_catalog(offers);
        let request = CommercialPlanningRequest {
            allowed_availability_statuses: Some(
                [
                    AvailabilityStatus::InStock,
                    AvailabilityStatus::LimitedStock,
                ]
                .into_iter()
                .collect(),
            ),
            ..Default::default()
        };
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &request,
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        assert!(assessment.rejected_offers.iter().any(|r| {
            r.offer_id.as_ref().map(|o| o.0.as_str()) == Some("BACO3-DISCONTINUED")
                && r.reason_codes
                    .contains(&CommercialExclusionCode::AvailabilityExcluded)
        }));
    }

    #[test]
    fn assess_commercial_precursors_missing_price_reject_excludes_offer() {
        let plan = barium_titanate_plan();
        let catalog = baco3_tio2_catalog(default_baco3_tio2_offers());
        let request = CommercialPlanningRequest {
            require_known_price: true,
            ..Default::default()
        };
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &request,
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        assert!(assessment.rejected_offers.iter().any(|r| {
            r.offer_id.as_ref().map(|o| o.0.as_str()) == Some("BACO3-NOPRICE")
                && r.reason_codes
                    .contains(&CommercialExclusionCode::PriceRequiredButUnknown)
        }));
    }

    #[test]
    fn assess_commercial_precursors_missing_price_keep_with_warning_stays_selectable() {
        let plan = barium_titanate_plan();
        // Only the no-price BaCO3 offer, so it must be selected (or reported
        // unresolved), never simply dropped, when the policy keeps it.
        let catalog = baco3_tio2_catalog(vec![
            priced_offer(
                "BACO3-NOPRICE",
                "BaCO3",
                "Example Materials Ltd.",
                Some(0.98),
                Some(100.0),
                None,
                Some(3),
                Some(AvailabilityStatus::InStock),
            ),
            priced_offer(
                "TIO2-CHEAP",
                "TiO2",
                "Example Materials Ltd.",
                Some(0.99),
                Some(50.0),
                Some((800, "USD")),
                Some(5),
                Some(AvailabilityStatus::InStock),
            ),
        ]);
        let request = CommercialPlanningRequest::default(); // require_known_price: false
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &request,
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        assert!(assessment.every_precursor_has_a_match);
        let best = &assessment.combinations[0];
        assert!(
            best.selections
                .iter()
                .any(|s| s.offer_id.0 == "BACO3-NOPRICE")
        );
        assert_eq!(
            best.total_cost, None,
            "one selection's price is unknown, so no total cost"
        );
        assert!(
            assessment
                .unresolved_commercial_fields
                .iter()
                .any(|f| f.offer_id.0 == "BACO3-NOPRICE" && f.field == "unit_price")
        );
    }

    #[test]
    fn assess_commercial_precursors_mixed_currency_total_is_none_with_a_warning() {
        let plan = barium_titanate_plan();
        // Force selection of the EUR TiO2 offer by removing the USD one.
        let catalog = baco3_tio2_catalog(vec![
            priced_offer(
                "BACO3-CHEAP",
                "BaCO3",
                "Example Materials Ltd.",
                Some(0.99),
                Some(100.0),
                Some((1000, "USD")),
                Some(5),
                Some(AvailabilityStatus::InStock),
            ),
            priced_offer(
                "TIO2-EUR",
                "TiO2",
                "Osaka Demo Reagents",
                Some(0.97),
                Some(50.0),
                Some((700, "EUR")),
                Some(10),
                Some(AvailabilityStatus::InStock),
            ),
        ]);
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        let best = &assessment.combinations[0];
        assert_eq!(
            best.total_cost, None,
            "mixed currency must never be silently summed"
        );
        assert!(!best.all_costs_known || best.total_cost.is_none());
    }

    #[test]
    fn assess_commercial_precursors_max_total_cost_filters_before_truncation_not_after() {
        // Regression test for a bug where max_total_cost was applied as a
        // post-hoc filter on the already-truncated top max_results_returned
        // list: if every top-ranked combination exceeded the ceiling but a
        // lower-ranked one satisfied it, the caller got zero combinations
        // even though a satisfying one existed. The premium offer below
        // outranks the cheap offer on unresolved-field count (its lead time
        // is known, the cheap offer's is not) despite costing far more --
        // so with max_results_returned: 1, a post-truncation filter would
        // keep only the premium combination and then reject it, while the
        // fix filters before truncating and returns the cheap one instead.
        let plan = barium_titanate_plan();
        let catalog = baco3_tio2_catalog(vec![
            priced_offer(
                "BACO3-PREMIUM",
                "BaCO3",
                "Example Materials Ltd.",
                Some(1.0),
                Some(250.0),
                Some((1_000_000, "USD")),
                Some(5),
                Some(AvailabilityStatus::InStock),
            ),
            priced_offer(
                "BACO3-CHEAP-UNKNOWN-LEADTIME",
                "BaCO3",
                "Demo Chemical Supply Co.",
                Some(1.0),
                Some(250.0),
                Some((5_000, "USD")),
                None,
                Some(AvailabilityStatus::InStock),
            ),
            priced_offer(
                "TIO2-ONLY",
                "TiO2",
                "Example Materials Ltd.",
                Some(1.0),
                Some(100.0),
                Some((50_000, "USD")),
                Some(5),
                Some(AvailabilityStatus::InStock),
            ),
        ]);
        let request = CommercialPlanningRequest {
            max_total_cost: Some(money(200_000, "USD")),
            ..Default::default()
        };
        let config = CommercialPlanningConfig {
            max_results_returned: 1,
            ..Default::default()
        };
        let assessment = assess_commercial_precursors(&plan, &catalog, &request, &config).unwrap();
        assert_eq!(
            assessment.combinations.len(),
            1,
            "a budget-satisfying combination exists and must be returned, not dropped"
        );
        let best = &assessment.combinations[0];
        assert!(
            best.selections
                .iter()
                .any(|s| s.offer_id.0 == "BACO3-CHEAP-UNKNOWN-LEADTIME")
        );
        assert_eq!(best.total_cost, Some(money(55_000, "USD")));
    }

    #[test]
    fn assess_commercial_precursors_max_total_cost_excluding_everything_is_reported_not_silent() {
        // Every precursor matches and the search space is non-empty, but
        // max_total_cost is set below any achievable total -- must produce
        // a warning explaining why, not read as "matching succeeded,
        // nothing to buy". Both offers below have a known price in a single
        // shared currency, so their combination's cost is always verifiable
        // against the ceiling -- an unknown-price offer would trivially
        // pass (not comparable), which would defeat this test.
        let plan = barium_titanate_plan();
        let catalog = baco3_tio2_catalog(vec![
            priced_offer(
                "BACO3-PRICED",
                "BaCO3",
                "Example Materials Ltd.",
                Some(0.99),
                Some(250.0),
                Some((1000, "USD")),
                Some(5),
                Some(AvailabilityStatus::InStock),
            ),
            priced_offer(
                "TIO2-PRICED",
                "TiO2",
                "Example Materials Ltd.",
                Some(0.99),
                Some(100.0),
                Some((800, "USD")),
                Some(5),
                Some(AvailabilityStatus::InStock),
            ),
        ]);
        let request = CommercialPlanningRequest {
            max_total_cost: Some(money(1, "USD")),
            ..Default::default()
        };
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &request,
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        assert!(assessment.combinations.is_empty());
        assert!(assessment.every_precursor_has_a_match);
        assert!(
            assessment.search_budget.is_exhaustive,
            "this test's 2x1 space must fit the default budget -- pins which \
             search tier (exhaustive, not heuristic) the warning wording below \
             is verified against"
        );
        assert!(
            assessment
                .warnings
                .iter()
                .any(|w| w.message.contains("max_total_cost")),
            "an empty result caused by the cost ceiling must be explained, not silent: {:?}",
            assessment.warnings
        );
    }

    #[test]
    fn assess_commercial_precursors_unreported_availability_counts_as_acceptable() {
        // precursor.rs's existing convention: missing availability metadata
        // is a gap, not evidence the compound is unavailable. A combination
        // built from offers that simply never reported availability must
        // not read as "unacceptable".
        let plan = barium_titanate_plan();
        let catalog = baco3_tio2_catalog(vec![
            priced_offer(
                "BACO3-NOAVAIL",
                "BaCO3",
                "Example Materials Ltd.",
                Some(0.99),
                Some(250.0),
                Some((1000, "USD")),
                Some(5),
                None,
            ),
            priced_offer(
                "TIO2-NOAVAIL",
                "TiO2",
                "Example Materials Ltd.",
                Some(0.99),
                Some(100.0),
                Some((800, "USD")),
                Some(5),
                None,
            ),
        ]);
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        let best = &assessment.combinations[0];
        assert!(
            best.all_availability_acceptable,
            "unreported availability must count as acceptable-but-unknown, not unacceptable"
        );
    }

    #[test]
    fn assess_commercial_precursors_discontinued_offer_makes_availability_unacceptable() {
        // The default request doesn't restrict allowed_availability_statuses
        // (so Discontinued offers aren't hard-excluded), which makes this
        // branch reachable: an explicitly Discontinued selection must still
        // be flagged via all_availability_acceptable.
        let plan = barium_titanate_plan();
        let catalog = baco3_tio2_catalog(vec![
            priced_offer(
                "BACO3-DISCONTINUED",
                "BaCO3",
                "Example Materials Ltd.",
                Some(0.99),
                Some(250.0),
                Some((1000, "USD")),
                Some(5),
                Some(AvailabilityStatus::Discontinued),
            ),
            priced_offer(
                "TIO2-INSTOCK",
                "TiO2",
                "Example Materials Ltd.",
                Some(0.99),
                Some(100.0),
                Some((800, "USD")),
                Some(5),
                Some(AvailabilityStatus::InStock),
            ),
        ]);
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        let best = &assessment.combinations[0];
        assert!(
            !best.all_availability_acceptable,
            "a Discontinued selection must make the combination availability-unacceptable"
        );
    }

    #[test]
    fn assess_commercial_precursors_cost_overflow_excludes_the_offer_not_panics() {
        let plan = barium_titanate_plan();
        let mut offers = default_baco3_tio2_offers();
        // An astronomically large unit price combined with a tiny package
        // size drives package_count * price past u64::MAX.
        offers.push(priced_offer(
            "BACO3-OVERFLOW",
            "BaCO3",
            "Example Materials Ltd.",
            Some(0.5),
            Some(0.0000001),
            Some((u64::MAX, "USD")),
            Some(1),
            Some(AvailabilityStatus::InStock),
        ));
        let catalog = baco3_tio2_catalog(offers);
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        assert!(assessment.rejected_offers.iter().any(|r| {
            r.offer_id.as_ref().map(|o| o.0.as_str()) == Some("BACO3-OVERFLOW")
                && r.reason_codes
                    .contains(&CommercialExclusionCode::CostOverflow)
        }));
    }

    #[test]
    fn assess_commercial_precursors_max_offers_per_precursor_truncates_and_warns() {
        let plan = barium_titanate_plan();
        let mut offers = default_baco3_tio2_offers();
        for i in 0..10 {
            offers.push(priced_offer(
                &format!("BACO3-EXTRA-{i}"),
                "BaCO3",
                "Example Materials Ltd.",
                Some(0.9),
                Some(100.0),
                Some((9999, "USD")),
                Some(30),
                Some(AvailabilityStatus::InStock),
            ));
        }
        let catalog = baco3_tio2_catalog(offers);
        let config = CommercialPlanningConfig {
            max_offers_per_precursor: 2,
            ..CommercialPlanningConfig::default()
        };
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &config,
        )
        .unwrap();
        assert!(!assessment.search_budget.is_exhaustive);
        assert!(assessment.rejected_offers.iter().any(|r| {
            r.reason_codes
                .contains(&CommercialExclusionCode::OfferCountCapExceeded)
        }));
    }

    #[test]
    fn assess_commercial_precursors_max_combinations_evaluated_is_reported_not_silent() {
        let plan = barium_titanate_plan();
        let mut baco3_offers = Vec::new();
        let mut tio2_offers = Vec::new();
        for i in 0..5 {
            baco3_offers.push(priced_offer(
                &format!("BACO3-{i}"),
                "BaCO3",
                "Example Materials Ltd.",
                Some(0.9),
                Some(100.0),
                Some((1000 + i, "USD")),
                Some(5),
                Some(AvailabilityStatus::InStock),
            ));
            tio2_offers.push(priced_offer(
                &format!("TIO2-{i}"),
                "TiO2",
                "Example Materials Ltd.",
                Some(0.9),
                Some(50.0),
                Some((800 + i, "USD")),
                Some(5),
                Some(AvailabilityStatus::InStock),
            ));
        }
        let mut offers = baco3_offers;
        offers.extend(tio2_offers);
        let catalog = baco3_tio2_catalog(offers);
        let config = CommercialPlanningConfig {
            max_combinations_evaluated: 2,
            max_results_returned: 100,
            ..CommercialPlanningConfig::default()
        };
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &config,
        )
        .unwrap();
        assert_eq!(assessment.search_budget.combinations_evaluated, 2);
        assert!(!assessment.search_budget.is_exhaustive);
        assert!(assessment.search_budget.combinations_omitted > 0);
    }

    /// A catalog with `n` BaCO3 offers and `n` TiO2 offers, each offer
    /// individually priced (never tied) so ranking has something real to
    /// discriminate on. Paired with a small `max_combinations_evaluated`
    /// (`n * n` comfortably exceeds any reasonable budget for `n >= 5`),
    /// this forces `search_combinations` into the heuristic tier -- used
    /// by the tests below, which check that the *heuristic* tier (not
    /// just the exact tier, already covered by the brute-force oracle
    /// test) is itself deterministic, input-order-independent, and never
    /// emits a duplicate combination.

    #[test]
    fn assess_commercial_precursors_is_deterministic_across_repeated_calls() {
        let plan = barium_titanate_plan();
        let catalog = baco3_tio2_catalog(default_baco3_tio2_offers());
        let a = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        let b = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn assess_commercial_precursors_ordering_is_independent_of_catalog_input_order() {
        let plan = barium_titanate_plan();
        let mut offers = default_baco3_tio2_offers();
        let catalog_forward = baco3_tio2_catalog(offers.clone());
        offers.reverse();
        let catalog_reversed = baco3_tio2_catalog(offers);

        let a = assess_commercial_precursors(
            &plan,
            &catalog_forward,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        let b = assess_commercial_precursors(
            &plan,
            &catalog_reversed,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn assess_commercial_precursors_deterministic_combination_id_is_row_ordered_not_sorted() {
        let plan = barium_titanate_plan();
        let catalog = baco3_tio2_catalog(default_baco3_tio2_offers());
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        let best = &assessment.combinations[0];
        let expected_id = best
            .selections
            .iter()
            .map(|s| s.offer_id.0.as_str())
            .collect::<Vec<_>>()
            .join("|");
        assert_eq!(best.combination_id, expected_id);
    }

    #[test]
    fn assess_commercial_precursors_target_batch_mass_scales_quantities() {
        let plan = barium_titanate_plan();
        let catalog = baco3_tio2_catalog(default_baco3_tio2_offers());
        let target_composition = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let request = CommercialPlanningRequest {
            target_composition: Some(target_composition),
            // BaTiO3 molar mass ~= 233.192 g/mol; ask for 10x that in grams
            // so the scale factor should come out to ~10.
            target_batch_mass_grams: Some(2331.92),
            ..Default::default()
        };
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &request,
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        let best = &assessment.combinations[0];
        let baco3 = best
            .selections
            .iter()
            .find(|s| s.offer_id.0 == "BACO3-CHEAP")
            .unwrap();
        // Without scaling this would be ~197.335g; with ~10x batch mass it
        // should be roughly 10x that.
        assert!(baco3.theoretical_pure_mass_required_grams > 1900.0);
    }

    #[test]
    fn assess_commercial_precursors_target_not_found_among_products_warns_and_falls_back() {
        let plan = barium_titanate_plan();
        let catalog = baco3_tio2_catalog(default_baco3_tio2_offers());
        let request = CommercialPlanningRequest {
            target_composition: Some(composition(&[("Na", 1.0), ("Cl", 1.0)])),
            target_batch_mass_grams: Some(100.0),
            ..Default::default()
        };
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &request,
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        assert!(assessment.every_precursor_has_a_match);
        assert!(assessment.warnings.iter().any(|w| {
            w.message
                .contains("was not found among this plan's reaction products")
        }));
    }

    #[test]
    fn assess_commercial_precursors_inconsistent_request_is_an_error() {
        let plan = barium_titanate_plan();
        let catalog = baco3_tio2_catalog(default_baco3_tio2_offers());
        let request = CommercialPlanningRequest {
            target_batch_mass_grams: Some(100.0),
            target_composition: None,
            ..Default::default()
        };
        let result = assess_commercial_precursors(
            &plan,
            &catalog,
            &request,
            &CommercialPlanningConfig::default(),
        );
        assert!(matches!(
            result,
            Err(CommercialCatalogError::InconsistentRequest { .. })
        ));
    }

    #[test]
    fn assess_commercial_plans_one_malformed_plan_does_not_abort_the_batch() {
        let good_plan = barium_titanate_plan();
        let mut bad_plan = good_plan.clone();
        bad_plan.balanced_reaction = None;
        let catalog = baco3_tio2_catalog(default_baco3_tio2_offers());
        let results = assess_commercial_plans(
            &[bad_plan, good_plan],
            &catalog,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        assert_eq!(results.len(), 2);
        assert!(!results[0].every_precursor_has_a_match);
        assert!(results[1].every_precursor_has_a_match);
    }

    #[test]
    fn assess_commercial_plans_rejects_an_inconsistent_request_before_touching_any_plan() {
        let plan = barium_titanate_plan();
        let catalog = baco3_tio2_catalog(default_baco3_tio2_offers());
        let request = CommercialPlanningRequest {
            target_batch_mass_grams: Some(100.0),
            target_composition: None,
            ..Default::default()
        };
        let result = assess_commercial_plans(
            &[plan],
            &catalog,
            &request,
            &CommercialPlanningConfig::default(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn assess_commercial_precursors_empty_catalog_reports_everything_unmatched() {
        let plan = barium_titanate_plan();
        let (catalog, _) = CommercialPrecursorCatalog::from_offers(vec![]);
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        assert!(!assessment.every_precursor_has_a_match);
        assert_eq!(assessment.unmatched_precursors.len(), 2);
    }

    // -- brute-force oracle for the bounded combination search --
}
