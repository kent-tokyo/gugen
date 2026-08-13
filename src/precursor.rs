use crate::balance;
use crate::composition::{Composition, Element};
use crate::error::{ProviderError, Result};
use crate::provider::PrecursorCatalog;
use crate::rejection::{RejectedCandidate, RejectionCode};
use crate::target::PlanningConstraints;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PrecursorId(pub String);

impl std::fmt::Display for PrecursorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Minimal placeholder for provenance about where/whether a precursor is
/// obtainable. AGENTS.md §9 lists availability as a filter candidate but
/// doesn't specify a shape; kept intentionally small until a real provider
/// exists. Missing availability must not block a precursor from being
/// used -- it's a gap in metadata, not evidence the compound is
/// unavailable (AGENTS.md §21.2's "availability metadata欠損" test case).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AvailabilityMetadata {
    pub source: String,
}

/// Redox-compatibility, atmosphere-compatibility, and hazard/toxicity
/// metadata are out of Phase 3's scope (AGENTS.md §26 Phase 3's checklist
/// doesn't list them; they belong to later process/safety phases) and are
/// not modeled here yet.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PrecursorCandidate {
    pub id: PrecursorId,
    pub composition: Composition,
    pub availability: Option<AvailabilityMetadata>,
}

/// One precursor's role in a candidate plan.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PrecursorSelection {
    pub precursor: PrecursorId,
    /// Integer formula units, matching `BalancedReaction`'s coefficient
    /// convention (AGENTS.md §10).
    pub formula_units: u64,
}

/// In-memory `PrecursorCatalog` (AGENTS.md §8: in-memory/JSON/fixture
/// providers are the v0.1 priority, no network access). Candidates are
/// kept sorted by `PrecursorId` regardless of construction order, so
/// `candidates_for` results -- and everything built on them -- are
/// invariant to catalog insertion order (AGENTS.md §21.4).
#[derive(Debug, Clone)]
pub struct InMemoryPrecursorCatalog {
    candidates: Vec<PrecursorCandidate>,
}

impl InMemoryPrecursorCatalog {
    /// Deduplicates by `PrecursorId` (AGENTS.md §21.2's "duplicate
    /// elimination"), keeping the first entry encountered for a given id.
    /// A catalog with two different compositions under the same id is
    /// malformed input; silently keeping one is preferable to letting the
    /// id stop being a reliable identity key downstream.
    pub fn new(mut candidates: Vec<PrecursorCandidate>) -> Self {
        candidates.sort_by(|a, b| a.id.0.cmp(&b.id.0));
        candidates.dedup_by(|a, b| a.id == b.id);
        Self { candidates }
    }
}

impl PrecursorCatalog for InMemoryPrecursorCatalog {
    fn candidates_for(
        &self,
        target: &Composition,
        _constraints: &PlanningConstraints,
    ) -> std::result::Result<Vec<PrecursorCandidate>, ProviderError> {
        let target_elements: BTreeSet<Element> = target.elements().collect();
        Ok(self
            .candidates
            .iter()
            .filter(|c| {
                c.composition
                    .elements()
                    .any(|e| target_elements.contains(&e))
            })
            .cloned()
            .collect())
    }
}

/// One accepted candidate plan: which precursors, and the balanced
/// reaction that resulted (AGENTS.md §10's byproduct handling is folded
/// in here -- `reaction` may include a curated byproduct the search had to
/// introduce to make the elements balance).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct AcceptedPrecursorSet {
    pub precursors: Vec<PrecursorId>,
    pub reaction: crate::reaction::BalancedReaction,
}

/// Result of a bounded precursor search (AGENTS.md §9). `rejected` always
/// carries a reason for every candidate set the search actually
/// evaluated and turned down. If the search stopped early because
/// `SearchBudget::max_precursor_sets` was exhausted, `rejected` also
/// carries one sentinel entry with `RejectionCode::SearchBudgetExhausted`
/// and an empty `precursors` list -- distinguishing "we looked and found
/// nothing" from "we ran out of budget before looking everywhere"
/// (AGENTS.md §9: "budget不足を「候補なし」と混同してはいけません").
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PrecursorSearchOutcome {
    pub accepted: Vec<AcceptedPrecursorSet>,
    pub rejected: Vec<RejectedCandidate>,
}

/// Searches for precursor sets that can plausibly produce `target`,
/// drawn from `candidates` (typically the output of a `PrecursorCatalog`
/// query), respecting `constraints` and `budget`.
///
/// Deterministic bounded search (AGENTS.md §9): combinations of
/// `candidates`, sizes `1..=budget.max_precursors_per_plan`, generated in
/// a fixed (size-then-lexicographic-index) order, up to
/// `budget.max_precursor_sets` combinations evaluated. For each
/// combination that passes the coverage/forbidden-element/removability
/// filters, stoichiometric balance is attempted with the target alone
/// first, then with each curated byproduct subset added -- smallest
/// subset first, never the whole curated set at once (see the regression
/// test on `balance()` documenting why: extra byproduct columns can push
/// a valid answer into a *combination* of null-space basis vectors that
/// `balance()`'s single-basis-vector check won't find).
///
/// `budget.max_plans_returned` is deliberately **not** applied here: with
/// no ranking in this module, truncating `accepted` here would keep
/// whichever combinations happened to be generated first, not the best
/// ones. Callers that want a bounded final count (`Planner`, Phase 6) rank
/// `accepted` by score first and truncate after, explaining what didn't
/// make the cut.
///
/// `PRECURSOR_COUNT_EXCEEDED` and `DUPLICATE_PLAN` are unreachable by
/// construction in Phase 3: combinations never exceed
/// `max_precursors_per_plan` in the first place, and are generated as
/// unique index subsets so no duplicate combination is ever evaluated
/// twice. `ATMOSPHERE_CONFLICT`, `HAZARD_POLICY_BLOCKED`,
/// `THERMODYNAMIC_DATA_UNAVAILABLE`, and `USER_CONSTRAINT_VIOLATION`
/// belong to later phases (atmosphere/process modeling, safety, ranking,
/// and richer `PlanningConstraints` respectively) and are not emitted
/// here.
pub fn search_precursor_sets(
    target: &Composition,
    candidates: &[PrecursorCandidate],
    constraints: &PlanningConstraints,
    budget: &crate::config::SearchBudget,
) -> Result<PrecursorSearchOutcome> {
    let target_elements: BTreeSet<Element> = target.elements().collect();
    let byproducts = balance::curated_byproducts()?;
    let byproduct_elements: BTreeSet<Element> =
        byproducts.iter().flat_map(Composition::elements).collect();
    let byproduct_subsets = power_set(&byproducts);

    let (combos, budget_exhausted) = generate_combinations(
        candidates.len(),
        budget.max_precursors_per_plan,
        budget.max_precursor_sets,
    );

    let mut accepted = Vec::new();
    let mut rejected = Vec::new();

    for combo in &combos {
        let chosen: Vec<&PrecursorCandidate> = combo.iter().map(|&i| &candidates[i]).collect();
        let ids: Vec<PrecursorId> = chosen.iter().map(|c| c.id.clone()).collect();

        if let Some(bad) = chosen
            .iter()
            .flat_map(|c| c.composition.elements())
            .find(|e| constraints.forbidden_elements.contains(e))
        {
            rejected.push(RejectedCandidate {
                precursors: ids,
                reason_codes: vec![RejectionCode::ForbiddenElementPresent],
                explanation: format!("precursor set contains forbidden element {bad}"),
            });
            continue;
        }

        let combo_elements: BTreeSet<Element> = chosen
            .iter()
            .flat_map(|c| c.composition.elements())
            .collect();
        let missing: Vec<Element> = target_elements
            .difference(&combo_elements)
            .copied()
            .collect();
        if !missing.is_empty() {
            rejected.push(RejectedCandidate {
                precursors: ids,
                reason_codes: vec![RejectionCode::MissingTargetElement],
                explanation: format!(
                    "precursor set does not cover target element(s): {}",
                    join_symbols(&missing)
                ),
            });
            continue;
        }

        let unremovable: Vec<Element> = combo_elements
            .difference(&target_elements)
            .filter(|e| !byproduct_elements.contains(e))
            .copied()
            .collect();
        if !unremovable.is_empty() {
            rejected.push(RejectedCandidate {
                precursors: ids,
                reason_codes: vec![RejectionCode::UnsupportedByproductRequired],
                explanation: format!(
                    "precursor set introduces element(s) with no curated byproduct to remove them: {}",
                    join_symbols(&unremovable)
                ),
            });
            continue;
        }

        let reactant_compositions: Vec<Composition> =
            chosen.iter().map(|c| c.composition.clone()).collect();
        let mut found = Vec::new();
        for subset in &byproduct_subsets {
            let mut products = vec![target.clone()];
            products.extend(subset.iter().cloned());
            let results = balance::balance(&reactant_compositions, &products)?;
            if !results.is_empty() {
                found = results;
                break;
            }
        }

        if found.is_empty() {
            rejected.push(RejectedCandidate {
                precursors: ids,
                reason_codes: vec![RejectionCode::NoStoichiometricBalance],
                explanation:
                    "no integer balance exists for this precursor set against the target, \
                    with or without curated byproducts"
                        .to_string(),
            });
            continue;
        }

        for reaction in found {
            // `balance()` operates on bare `Composition`s and drops any
            // reactant whose solved coefficient is zero, so `reaction`'s
            // reactant list is not guaranteed to be `chosen` unfiltered --
            // re-derive the id list by matching composition rather than
            // assuming index alignment with `ids`.
            let matched_ids: Vec<PrecursorId> = reaction
                .reactants
                .iter()
                .map(|species| {
                    chosen
                        .iter()
                        .find(|c| c.composition == species.composition)
                        .map(|c| c.id.clone())
                        .expect(
                            "balance() only returns reactant species drawn from \
                            the compositions it was given",
                        )
                })
                .collect();
            accepted.push(AcceptedPrecursorSet {
                precursors: matched_ids,
                reaction,
            });
        }
    }

    if budget_exhausted {
        rejected.push(RejectedCandidate {
            precursors: vec![],
            reason_codes: vec![RejectionCode::SearchBudgetExhausted],
            explanation: format!(
                "stopped after evaluating {} precursor-set combination(s); more were possible",
                combos.len()
            ),
        });
    }

    Ok(PrecursorSearchOutcome { accepted, rejected })
}

fn join_symbols(elements: &[Element]) -> String {
    elements
        .iter()
        .map(Element::symbol)
        .collect::<Vec<_>>()
        .join(", ")
}

/// All subsets of `items`, ordered smallest-size-first (empty set first),
/// then by ascending index within each size.
fn power_set<T: Clone>(items: &[T]) -> Vec<Vec<T>> {
    let mut subsets = Vec::with_capacity(1 << items.len());
    for size in 0..=items.len() {
        for combo in index_combinations(items.len(), size) {
            subsets.push(combo.iter().map(|&i| items[i].clone()).collect());
        }
    }
    subsets
}

/// Every combination of `size` indices from `0..n`, in lexicographic
/// order. Eager (collects into a `Vec`) rather than a lazy iterator --
/// simplest correct approach for the small catalog sizes this crate
/// targets; revisit only if a real catalog makes eager generation
/// measurably expensive.
fn index_combinations(n: usize, size: usize) -> Vec<Vec<usize>> {
    fn recurse(
        start: usize,
        n: usize,
        size: usize,
        current: &mut Vec<usize>,
        out: &mut Vec<Vec<usize>>,
    ) {
        if current.len() == size {
            out.push(current.clone());
            return;
        }
        for i in start..n {
            current.push(i);
            recurse(i + 1, n, size, current, out);
            current.pop();
        }
    }
    let mut out = Vec::new();
    recurse(0, n, size, &mut Vec::new(), &mut out);
    out
}

/// Generates combinations of indices `0..n`, sizes `1..=max_size`, in
/// (size, then lexicographic) order, stopping once `budget` combinations
/// have been produced. Returns `(combinations, true)` if generation was
/// cut short by the budget, `(combinations, false)` if every combination
/// was generated.
fn generate_combinations(n: usize, max_size: usize, budget: usize) -> (Vec<Vec<usize>>, bool) {
    let mut result = Vec::new();
    let mut exhausted = false;
    'sizes: for size in 1..=max_size.min(n) {
        for combo in index_combinations(n, size) {
            if result.len() >= budget {
                exhausted = true;
                break 'sizes;
            }
            result.push(combo);
        }
    }
    (result, exhausted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SearchBudget;

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

    fn barium_titanate_catalog() -> Vec<PrecursorCandidate> {
        vec![
            candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            candidate("BaO", &[("Ba", 1.0), ("O", 1.0)]),
            candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
            // Shares zero elements with Ba-Ti-O -- genuinely irrelevant,
            // unlike e.g. SrCO3 which shares O with almost any oxide
            // target and so is *not* a useful "irrelevant" fixture.
            candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]),
        ]
    }

    fn generous_budget() -> SearchBudget {
        SearchBudget {
            max_precursor_sets: 10_000,
            max_precursors_per_plan: 3,
            max_plans_returned: 100,
        }
    }

    /// AGENTS.md §21.2: target元素をすべて被覆.
    #[test]
    fn accepts_a_set_that_covers_every_target_element() {
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let catalog = barium_titanate_catalog();
        let outcome = search_precursor_sets(
            &target,
            &catalog,
            &PlanningConstraints::default(),
            &generous_budget(),
        )
        .unwrap();

        let ba_ti = outcome.accepted.iter().find(|a| {
            let ids: BTreeSet<&str> = a.precursors.iter().map(|p| p.0.as_str()).collect();
            ids == BTreeSet::from(["BaCO3", "TiO2"])
        });
        assert!(
            ba_ti.is_some(),
            "BaCO3 + TiO2 must be accepted: {:?}",
            outcome.accepted
        );
    }

    /// AGENTS.md §21.2: 不要元素を含む候補の除外 (candidates introducing
    /// an element with no curated byproduct to remove it are excluded).
    #[test]
    fn rejects_sets_with_unremovable_extra_elements() {
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        // SrCO3 alone doesn't cover Ba/Ti, so pair it with TiO2 to isolate
        // the "extra element Sr has no curated byproduct" rejection from
        // plain coverage failure.
        let catalog = vec![
            candidate("SrCO3", &[("Sr", 1.0), ("C", 1.0), ("O", 3.0)]),
            candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
            candidate("BaO", &[("Ba", 1.0), ("O", 1.0)]),
        ];
        let outcome = search_precursor_sets(
            &target,
            &catalog,
            &PlanningConstraints::default(),
            &generous_budget(),
        )
        .unwrap();

        let bad_combo = outcome.rejected.iter().find(|r| {
            let ids: BTreeSet<&str> = r.precursors.iter().map(|p| p.0.as_str()).collect();
            ids == BTreeSet::from(["SrCO3", "TiO2", "BaO"])
        });
        assert_eq!(
            bad_combo.map(|r| r.reason_codes.clone()),
            Some(vec![RejectionCode::UnsupportedByproductRequired])
        );
    }

    /// AGENTS.md §21.2: 最大前駆体数 (max precursor count is respected).
    #[test]
    fn never_generates_a_combination_larger_than_the_configured_maximum() {
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let catalog = barium_titanate_catalog();
        let budget = SearchBudget {
            max_precursors_per_plan: 2,
            ..generous_budget()
        };
        let outcome =
            search_precursor_sets(&target, &catalog, &PlanningConstraints::default(), &budget)
                .unwrap();

        for a in &outcome.accepted {
            assert!(a.precursors.len() <= 2);
        }
        for r in &outcome.rejected {
            assert!(r.precursors.len() <= 2);
        }
    }

    /// AGENTS.md §21.2: forbidden precursor.
    #[test]
    fn rejects_combinations_containing_a_forbidden_element() {
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let catalog = barium_titanate_catalog();
        let mut constraints = PlanningConstraints::default();
        constraints.forbidden_elements.insert(element("C"));

        let outcome =
            search_precursor_sets(&target, &catalog, &constraints, &generous_budget()).unwrap();

        assert!(
            outcome
                .accepted
                .iter()
                .all(|a| !a.precursors.iter().any(|p| p.0 == "BaCO3")),
            "no accepted set may use BaCO3 once C is forbidden"
        );
        let forbidden_rejection = outcome
            .rejected
            .iter()
            .find(|r| r.precursors.iter().any(|p| p.0 == "BaCO3"))
            .expect("BaCO3-containing combinations must be rejected, not silently dropped");
        assert_eq!(
            forbidden_rejection.reason_codes,
            vec![RejectionCode::ForbiddenElementPresent]
        );
    }

    /// AGENTS.md §21.2: duplicate elimination -- a catalog with a
    /// duplicated entry must not produce duplicate accepted results.
    /// Dedup is `InMemoryPrecursorCatalog`'s job (by `PrecursorId`), not
    /// `search_precursor_sets`'s -- so this goes through the catalog, not
    /// a raw candidate slice.
    #[test]
    fn duplicate_catalog_entries_do_not_duplicate_results() {
        let target = composition(&[("Ba", 1.0), ("O", 1.0)]);
        let raw = vec![
            candidate("BaO", &[("Ba", 1.0), ("O", 1.0)]),
            candidate("BaO", &[("Ba", 1.0), ("O", 1.0)]),
        ];
        let catalog = InMemoryPrecursorCatalog::new(raw);
        let candidates = catalog
            .candidates_for(&target, &PlanningConstraints::default())
            .unwrap();
        assert_eq!(
            candidates.len(),
            1,
            "duplicate PrecursorId entries must collapse to one"
        );

        let outcome = search_precursor_sets(
            &target,
            &candidates,
            &PlanningConstraints::default(),
            &generous_budget(),
        )
        .unwrap();

        let single_bao_accepts = outcome
            .accepted
            .iter()
            .filter(|a| a.precursors == vec![PrecursorId("BaO".to_string())])
            .count();
        assert_eq!(single_bao_accepts, 1);
    }

    /// `AcceptedPrecursorSet.precursors` must stay index-aligned with
    /// `AcceptedPrecursorSet.reaction.reactants` even when `balance()`
    /// drops a chosen precursor because its solved coefficient came out
    /// zero -- a redundant Ba source (BaCO3 and BaO both supply Ba) is a
    /// real case where that happens.
    #[test]
    fn accepted_precursor_ids_stay_aligned_with_reaction_reactants() {
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let catalog = vec![
            candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            candidate("BaO", &[("Ba", 1.0), ("O", 1.0)]),
            candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
        ];
        let outcome = search_precursor_sets(
            &target,
            &catalog,
            &PlanningConstraints::default(),
            &generous_budget(),
        )
        .unwrap();

        for accepted in &outcome.accepted {
            assert_eq!(
                accepted.precursors.len(),
                accepted.reaction.reactants.len(),
                "precursors and reactants must be the same length: {accepted:?}"
            );
            for (id, species) in accepted.precursors.iter().zip(&accepted.reaction.reactants) {
                let candidate = catalog.iter().find(|c| &c.id == id).unwrap();
                assert_eq!(
                    candidate.composition, species.composition,
                    "precursor id {id} must match its reactant composition"
                );
            }
        }
        assert!(
            !outcome.accepted.is_empty(),
            "fixture must actually exercise the search, not vacuously pass"
        );
    }

    /// AGENTS.md §21.2/§21.4: deterministic catalog-order invariance.
    #[test]
    fn result_is_independent_of_catalog_insertion_order() {
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let mut shuffled = barium_titanate_catalog();
        shuffled.reverse();

        let a = search_precursor_sets(
            &target,
            &InMemoryPrecursorCatalog::new(barium_titanate_catalog())
                .candidates_for(&target, &PlanningConstraints::default())
                .unwrap(),
            &PlanningConstraints::default(),
            &generous_budget(),
        )
        .unwrap();
        let b = search_precursor_sets(
            &target,
            &InMemoryPrecursorCatalog::new(shuffled)
                .candidates_for(&target, &PlanningConstraints::default())
                .unwrap(),
            &PlanningConstraints::default(),
            &generous_budget(),
        )
        .unwrap();

        let ids_a: BTreeSet<Vec<String>> = a
            .accepted
            .iter()
            .map(|s| s.precursors.iter().map(|p| p.0.clone()).collect())
            .collect();
        let ids_b: BTreeSet<Vec<String>> = b
            .accepted
            .iter()
            .map(|s| s.precursors.iter().map(|p| p.0.clone()).collect())
            .collect();
        assert_eq!(ids_a, ids_b);
    }

    /// AGENTS.md §9: search budget exhaustion must be visible, not
    /// conflated with "no candidates."
    #[test]
    fn budget_exhaustion_is_reported_distinctly_from_no_candidates() {
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let catalog = barium_titanate_catalog();
        let tiny_budget = SearchBudget {
            max_precursor_sets: 1,
            max_precursors_per_plan: 3,
            max_plans_returned: 100,
        };

        let outcome = search_precursor_sets(
            &target,
            &catalog,
            &PlanningConstraints::default(),
            &tiny_budget,
        )
        .unwrap();

        let exhaustion = outcome
            .rejected
            .iter()
            .find(|r| r.reason_codes == vec![RejectionCode::SearchBudgetExhausted]);
        assert!(
            exhaustion.is_some(),
            "must report budget exhaustion: {:?}",
            outcome.rejected
        );
        assert!(exhaustion.unwrap().precursors.is_empty());
    }

    /// AGENTS.md §21.2: availability metadata欠損 -- missing availability
    /// must not block acceptance; a candidate with metadata and one
    /// without must be treated identically.
    #[test]
    fn missing_availability_metadata_does_not_block_acceptance() {
        let target = composition(&[("Ba", 1.0), ("O", 1.0)]);
        let with_metadata = vec![PrecursorCandidate {
            id: PrecursorId("BaO".to_string()),
            composition: composition(&[("Ba", 1.0), ("O", 1.0)]),
            availability: Some(AvailabilityMetadata {
                source: "curated_fixture".to_string(),
            }),
        }];
        let without_metadata = vec![candidate("BaO", &[("Ba", 1.0), ("O", 1.0)])];

        let a = search_precursor_sets(
            &target,
            &with_metadata,
            &PlanningConstraints::default(),
            &generous_budget(),
        )
        .unwrap();
        let b = search_precursor_sets(
            &target,
            &without_metadata,
            &PlanningConstraints::default(),
            &generous_budget(),
        )
        .unwrap();

        assert_eq!(a.accepted.len(), 1);
        assert_eq!(b.accepted.len(), 1);
        assert_eq!(a.accepted[0].reaction, b.accepted[0].reaction);
    }

    #[test]
    fn in_memory_catalog_scopes_to_target_relevant_candidates_and_ignores_insertion_order() {
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let catalog = InMemoryPrecursorCatalog::new(barium_titanate_catalog());
        let result = catalog
            .candidates_for(&target, &PlanningConstraints::default())
            .unwrap();

        let ids: Vec<&str> = result.iter().map(|c| c.id.0.as_str()).collect();
        assert!(
            !ids.contains(&"NaCl"),
            "NaCl shares no element with Ba-Ti-O and must be scoped out"
        );
        assert!(ids.contains(&"BaCO3") && ids.contains(&"BaO") && ids.contains(&"TiO2"));
        // sorted by PrecursorId regardless of the constructor's input order
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }
}
