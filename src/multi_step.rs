//! Phase 31 PR 1: two-step (precursors -> intermediate -> target) synthesis
//! route search. Pure orchestration over the existing single-step
//! `search_precursor_sets` (`src/precursor.rs`) -- `balance()`/
//! `BalancedReaction` already have no concept of "target" baked in
//! (`balance()` just solves reactants vs. products for any composition
//! lists), so a multi-step route is simply a sequence of already-generic
//! `BalancedReaction`s, chained by treating an earlier stage's product as
//! a later stage's available reactant. No change to `search_precursor_sets`,
//! `balance()`, `BalancedReaction`, or `Planner`.
//!
//! Deliberately not wired into `Planner`/`SynthesisPlan` in this PR, and
//! deliberately not measured against reaction-network's own published
//! BaTiO3 result or any real corpus -- see `docs/phase31_pr1_two_step_route_search.md`
//! for the two Step-0 data gaps this defers (McDermott et al.'s 9 routes
//! are not machine-readable; no multi-step-labeled corpus exists in this
//! repo yet). Verified here by hand-built synthetic fixtures only.

use crate::composition::Composition;
use crate::config::SearchBudget;
use crate::error::GugenError;
use crate::precursor::{PrecursorCandidate, PrecursorId, search_precursor_sets};
use crate::reaction::BalancedReaction;
use crate::target::PlanningConstraints;
use std::collections::BTreeSet;
use thiserror::Error;

/// Failure modes specific to assembling/validating a [`SynthesisRoute`].
/// Deliberately a separate enum from [`GugenError`], not a new variant on
/// it -- `GugenError` is a public, non-`#[non_exhaustive]` enum, so adding
/// a variant there would be a breaking change for every downstream
/// exhaustive `match`. `ProviderError` already establishes the "a distinct
/// concern gets its own error enum" precedent in this crate
/// (`src/error.rs`); route assembly is similarly distinct from core
/// numeric/compositional validation.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum RouteError {
    #[error("a synthesis route needs at least one stage")]
    EmptyRoute,
    #[error("the final stage's products do not include the target composition")]
    FinalStageMissingTarget,
    #[error(
        "stage {stage_index}'s reactant {composition:?} is not explained by any base precursor or an earlier stage's product"
    )]
    UnexplainedReactant {
        composition: Composition,
        stage_index: usize,
    },
    #[error("underlying single-step search failed: {0}")]
    Search(#[from] GugenError),
}

/// An ordered sequence of [`BalancedReaction`] stages, where each stage
/// after the first consumes only compositions already "available" -- a
/// base precursor, or a product of an earlier stage. A `BalancedReaction`
/// is already a valid hyperedge (many reactants -> many products); this
/// type's only job is enforcing that a sequence of them forms a genuinely
/// connected route to `target`, not an arbitrary bag of unrelated
/// reactions.
///
/// Smart-constructor validated, matching this crate's existing convention
/// (`Composition::new`, `ReactionSpecies::new`, `BalancedReaction::new`,
/// `CompetingPhase::new`): never exposes an invalid `SynthesisRoute`.
#[derive(Debug, Clone, PartialEq)]
pub struct SynthesisRoute {
    stages: Vec<BalancedReaction>,
}

impl SynthesisRoute {
    /// `base_precursors` is every composition considered available before
    /// the route starts (not necessarily used by every stage). Checks, in
    /// order: `stages` is non-empty; the final stage's products include
    /// `target`; every reactant of every stage is either in
    /// `base_precursors` or a product of a strictly earlier stage --
    /// element conservation *within* each stage is already guaranteed by
    /// `BalancedReaction::new` itself, so this constructor only adds the
    /// cross-stage connectivity check.
    pub fn new(
        stages: Vec<BalancedReaction>,
        base_precursors: &[Composition],
        target: &Composition,
    ) -> Result<Self, RouteError> {
        let Some(last) = stages.last() else {
            return Err(RouteError::EmptyRoute);
        };
        if !last.products().iter().any(|s| &s.composition == target) {
            return Err(RouteError::FinalStageMissingTarget);
        }

        let mut known: BTreeSet<Composition> = base_precursors.iter().cloned().collect();
        for (stage_index, stage) in stages.iter().enumerate() {
            for reactant in stage.reactants() {
                if !known.contains(&reactant.composition) {
                    return Err(RouteError::UnexplainedReactant {
                        composition: reactant.composition.clone(),
                        stage_index,
                    });
                }
            }
            known.extend(stage.products().iter().map(|s| s.composition.clone()));
        }

        Ok(Self { stages })
    }

    pub fn stages(&self) -> &[BalancedReaction] {
        &self.stages
    }

    /// The stage whose products include `target` -- always `stages`'s
    /// last element, guaranteed to exist by `new`'s own validation.
    pub fn final_reaction(&self) -> &BalancedReaction {
        self.stages
            .last()
            .expect("SynthesisRoute::new guarantees at least one stage")
    }
}

/// Searches for routes to `target` of depth 1 (today's existing
/// `search_precursor_sets`, unchanged) and depth 2 (via each composition
/// in `intermediate_candidates`). `intermediate_candidates` is
/// caller-supplied, never computed or fetched by this function -- matching
/// `FrequencyPriorGenerator`'s own "caller-supplied, never computed by the
/// crate" convention (`src/candidate_generator.rs`); a caller with a
/// `ThermodynamicProvider` can source it from `competing_phases(target)`,
/// but this function stays provider-agnostic and trivially testable with
/// plain fixtures.
///
/// For each intermediate `I`: first checks whether `I` itself is
/// reachable from `base_candidates` (a depth-1 search targeting `I`
/// instead of `target`); if so, splices a synthetic
/// produced-not-purchased `PrecursorCandidate` for `I` into an expanded
/// pool and searches that pool against the real `target`. Any accepted
/// set that actually consumes the synthetic candidate becomes a 2-stage
/// `SynthesisRoute` -- one that doesn't (already covered by the depth-1
/// pass) is skipped, so a route already reachable in one step is never
/// duplicated as a spurious two-step one.
///
/// Bounded to `1 + intermediate_candidates.len() + 1` calls to
/// `search_precursor_sets` total -- not combinatorial, since
/// `intermediate_candidates` is caller-bounded (real `competing_phases`
/// data is on the order of tens of entries per target).
///
/// `// ponytail: when an intermediate is reachable via more than one
/// distinct stage-1 combination, only the first (search_precursor_sets's
/// own deterministic ordering) is used to build a route -- this returns
/// at least one valid route per reachable intermediate, not every
/// combination of stage-1 x stage-2 routes. Revisit with exhaustive
/// enumeration only if a real fixture needs the alternates.`
pub fn search_two_step_routes(
    target: &Composition,
    base_candidates: &[PrecursorCandidate],
    intermediate_candidates: &[Composition],
    constraints: &PlanningConstraints,
    budget: &SearchBudget,
) -> Result<Vec<SynthesisRoute>, RouteError> {
    let base_compositions: Vec<Composition> = base_candidates
        .iter()
        .map(|c| c.composition.clone())
        .collect();

    let mut routes = Vec::new();

    let direct = search_precursor_sets(target, base_candidates, constraints, budget)?;
    for accepted in &direct.accepted {
        // A malformed accepted set here reflects a defect in the
        // underlying `search_precursor_sets_core` search itself (e.g. a
        // candidate matching a `curated_byproducts()` composition
        // exactly can be spuriously "accepted" as a no-op identity
        // reaction unrelated to `target` -- discovered via real-corpus
        // testing, tracked as a separate fix, see
        // docs/phase31_pr2_two_step_arity_recall.md), not a problem
        // with this caller's own inputs. Skipping just that one
        // candidate set preserves every other legitimate route for this
        // call instead of discarding all of them over one bad entry.
        if let Ok(route) =
            SynthesisRoute::new(vec![accepted.reaction.clone()], &base_compositions, target)
        {
            routes.push(route);
        }
    }

    for (index, intermediate) in intermediate_candidates.iter().enumerate() {
        if intermediate == target {
            continue;
        }
        let stage_one = search_precursor_sets(intermediate, base_candidates, constraints, budget)?;
        let Some(first_stage) = stage_one.accepted.first() else {
            continue;
        };

        let synthetic_id = PrecursorId(format!("__gugen_multi_step_intermediate_{index}"));
        let mut expanded_pool: Vec<PrecursorCandidate> = base_candidates.to_vec();
        expanded_pool.push(PrecursorCandidate {
            id: synthetic_id.clone(),
            composition: intermediate.clone(),
            availability: None,
        });

        let stage_two = search_precursor_sets(target, &expanded_pool, constraints, budget)?;
        for accepted in &stage_two.accepted {
            if !accepted.precursors.contains(&synthetic_id) {
                continue;
            }
            // Same defensive skip as the direct-route loop above, and
            // for the same underlying reason: a malformed accepted set
            // here (e.g. `first_stage.reaction`'s products not
            // literally containing `intermediate`, so the synthetic
            // candidate's own composition is "unexplained") traces back
            // to the same upstream search defect, not a problem with
            // this specific two-step combination's siblings.
            if let Ok(route) = SynthesisRoute::new(
                vec![first_stage.reaction.clone(), accepted.reaction.clone()],
                &base_compositions,
                target,
            ) {
                routes.push(route);
            }
        }
    }

    Ok(routes)
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

    fn candidate(id: &str, pairs: &[(&str, f64)]) -> PrecursorCandidate {
        PrecursorCandidate {
            id: PrecursorId(id.to_string()),
            composition: composition(pairs),
            availability: None,
        }
    }

    fn tight_budget() -> SearchBudget {
        SearchBudget {
            max_precursor_sets: 10_000,
            max_precursors_per_plan: 4,
            max_plans_returned: 20,
        }
    }

    /// Target needs all five of {Fe, Li, Na, K, O2} at once -- arity 5,
    /// exceeding `max_precursors_per_plan = 4` -- so a direct one-step
    /// search structurally cannot find it (`PrecursorCountExceeded`
    /// territory, not "no valid balance"). Via the intermediate
    /// `I = FeLiNa` (arity 3 to make, then arity 3 with I+K+O2), each
    /// stage stays within budget.
    fn five_element_fixture() -> (Composition, Vec<PrecursorCandidate>, Composition) {
        let target = composition(&[
            ("Fe", 1.0),
            ("Li", 1.0),
            ("Na", 1.0),
            ("K", 1.0),
            ("O", 1.0),
        ]);
        let base = vec![
            candidate("Fe", &[("Fe", 1.0)]),
            candidate("Li", &[("Li", 1.0)]),
            candidate("Na", &[("Na", 1.0)]),
            candidate("K", &[("K", 1.0)]),
            candidate("O2", &[("O", 2.0)]),
        ];
        let intermediate = composition(&[("Fe", 1.0), ("Li", 1.0), ("Na", 1.0)]);
        (target, base, intermediate)
    }

    #[test]
    fn direct_search_cannot_reach_a_five_way_target_under_a_tight_budget() {
        let (target, base, _intermediate) = five_element_fixture();
        let outcome = search_precursor_sets(
            &target,
            &base,
            &PlanningConstraints::default(),
            &tight_budget(),
        )
        .unwrap();
        assert!(
            outcome.accepted.is_empty(),
            "arity-5 target should be unreachable at max_precursors_per_plan=4"
        );
    }

    #[test]
    fn two_step_search_recovers_the_route_the_direct_search_cannot_reach() {
        let (target, base, intermediate) = five_element_fixture();
        let routes = search_two_step_routes(
            &target,
            &base,
            &[intermediate],
            &PlanningConstraints::default(),
            &tight_budget(),
        )
        .unwrap();

        assert_eq!(routes.len(), 1, "expected exactly one two-step route");
        let route = &routes[0];
        assert_eq!(route.stages().len(), 2);
        assert!(
            route
                .final_reaction()
                .products()
                .iter()
                .any(|s| s.composition == target)
        );
    }

    #[test]
    fn a_directly_reachable_target_is_not_duplicated_as_a_spurious_two_step_route() {
        let base = vec![
            candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
        ];
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        // An intermediate from a totally unrelated chemical system --
        // must not interfere with or duplicate the direct route.
        let unrelated_intermediate = composition(&[("Fe", 1.0), ("Li", 1.0)]);

        let generous = SearchBudget::default();
        let routes = search_two_step_routes(
            &target,
            &base,
            &[unrelated_intermediate],
            &PlanningConstraints::default(),
            &generous,
        )
        .unwrap();

        assert_eq!(
            routes.len(),
            1,
            "the direct route must appear exactly once, not duplicated"
        );
        assert_eq!(routes[0].stages().len(), 1);
    }

    #[test]
    fn an_unreachable_target_returns_no_routes_without_panicking() {
        let base = vec![candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)])];
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let intermediate = composition(&[("Na", 1.0), ("Cl", 1.0)]);

        let routes = search_two_step_routes(
            &target,
            &base,
            &[intermediate],
            &PlanningConstraints::default(),
            &SearchBudget::default(),
        )
        .unwrap();

        assert!(routes.is_empty());
    }

    /// Regression for a real defect found via real-corpus testing
    /// (`docs/phase31_pr2_two_step_arity_recall.md`'s Discovered Work):
    /// `search_precursor_sets_core` can accept a spurious identity
    /// reaction when a candidate's composition exactly equals one of
    /// `curated_byproducts()` (O2 here) -- `O2 -> O2`, unrelated to
    /// `target`. Before the fix, converting that malformed accepted
    /// entry via `SynthesisRoute::new(...)?` aborted the *entire*
    /// function with `FinalStageMissingTarget`, discarding the other,
    /// perfectly valid direct routes below alongside it. This exact
    /// 5-element oxynitride-style target/candidate shape (elemental O2
    /// present, target spanning O and N) reproduces it.
    #[test]
    fn a_spurious_identity_accepted_set_does_not_poison_the_whole_search() {
        let target = composition(&[
            ("Al", 1.0),
            ("N", 1.0),
            ("Nd", 1.0),
            ("O", 1.0),
            ("Si", 1.0),
        ]);
        let base = vec![
            candidate("AlN", &[("Al", 1.0), ("N", 1.0)]),
            candidate("Al2O3", &[("Al", 2.0), ("O", 3.0)]),
            candidate("Nd2O3", &[("Nd", 2.0), ("O", 3.0)]),
            candidate("Si3N4", &[("Si", 3.0), ("N", 4.0)]),
            candidate("O2", &[("O", 2.0)]),
        ];

        let routes = search_two_step_routes(
            &target,
            &base,
            &[],
            &PlanningConstraints::default(),
            &SearchBudget::default(),
        )
        .expect("a spurious O2->O2 accepted entry must not abort the whole search");

        assert!(
            !routes.is_empty(),
            "the legitimate direct routes must survive the spurious entry"
        );
        assert!(
            routes.iter().all(|r| r
                .final_reaction()
                .products()
                .iter()
                .any(|s| s.composition == target)),
            "every surviving route must actually produce the real target, not the spurious O2 no-op"
        );
    }

    fn simple_reaction(
        reactant: (&str, &[(&str, f64)]),
        product: (&str, &[(&str, f64)]),
    ) -> BalancedReaction {
        use crate::reaction::ReactionSpecies;
        BalancedReaction::new(
            vec![ReactionSpecies::new(composition(reactant.1), 1).unwrap()],
            vec![ReactionSpecies::new(composition(product.1), 1).unwrap()],
        )
        .unwrap_or_else(|e| panic!("{reactant:?} -> {product:?} should conserve: {e}"))
    }

    #[test]
    fn synthesis_route_rejects_empty_stages() {
        let target = composition(&[("Fe", 1.0)]);
        let err = SynthesisRoute::new(Vec::new(), &[], &target).unwrap_err();
        assert_eq!(err, RouteError::EmptyRoute);
    }

    #[test]
    fn synthesis_route_rejects_a_final_stage_that_does_not_produce_the_target() {
        let stage = simple_reaction(("Fe", &[("Fe", 1.0)]), ("Fe", &[("Fe", 1.0)]));
        let target = composition(&[("Li", 1.0)]);
        let err =
            SynthesisRoute::new(vec![stage], &[composition(&[("Fe", 1.0)])], &target).unwrap_err();
        assert_eq!(err, RouteError::FinalStageMissingTarget);
    }

    #[test]
    fn synthesis_route_rejects_a_stage_with_an_unexplained_reactant() {
        // stage_one: Fe -> Fe (a base precursor is available). stage_two:
        // Li -> Li, but "Li" was never a base precursor nor stage_one's
        // product -- an unexplained input into stage two.
        let stage_one = simple_reaction(("Fe", &[("Fe", 1.0)]), ("Fe", &[("Fe", 1.0)]));
        let stage_two = simple_reaction(("Li", &[("Li", 1.0)]), ("Li", &[("Li", 1.0)]));
        let target = composition(&[("Li", 1.0)]);

        let err = SynthesisRoute::new(
            vec![stage_one, stage_two],
            &[composition(&[("Fe", 1.0)])],
            &target,
        )
        .unwrap_err();

        assert_eq!(
            err,
            RouteError::UnexplainedReactant {
                composition: composition(&[("Li", 1.0)]),
                stage_index: 1,
            }
        );
    }
}
