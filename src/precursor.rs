use crate::balance;
use crate::composition::{Composition, Element};
use crate::error::{ProviderError, Result};
use crate::provider::PrecursorCatalog;
use crate::rejection::{RejectedCandidate, RejectionCode};
use crate::target::PlanningConstraints;
use std::cmp::Ordering;
use std::collections::{BTreeSet, BinaryHeap};

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

/// One partial precursor selection under construction, explored by
/// `search_precursor_sets`'s guided best-first frontier (Phase 29).
/// `chosen` holds ascending indices into the search's own `candidates`
/// slice -- ascending order is this state's own canonical identity
/// (exactly one path builds a given `chosen`), so unlike a general graph
/// search, no separate visited-state set is needed to avoid revisiting
/// the same subset twice.
#[derive(Debug, Clone)]
struct SearchState {
    chosen: Vec<usize>,
    missing: BTreeSet<Element>,
    priority: SearchPriority,
}

/// Ordering signal only -- deliberately not `Score01`/`PlanScoreBreakdown`-
/// shaped, and never compared against `total_ranking_score`: that type
/// needs a `BalancedReaction`, which doesn't exist yet at this point in
/// the search, and (per `score.rs`'s own doc comment) most of its seven
/// dimensions are structurally pinned constants for the current
/// generator anyway, carrying no ordering information here. Every field
/// is derived purely from element coverage of the partial combination
/// itself -- never literature frequency or availability (Phase 30's
/// `FrequencyPriorGenerator`'s concern, deliberately out of scope here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SearchPriority {
    /// Target elements this state's chosen candidates don't cover yet.
    /// Primary signal: 0 means the combination is complete and ready for
    /// a stoichiometric-balance attempt.
    elements_missing: usize,
    /// Candidates chosen so far. Secondary tie-break: among states
    /// tied on `elements_missing`, prefer the smaller one -- more
    /// runway left before `max_precursors_per_plan`, and a simpler
    /// combination if it does turn out to balance.
    depth: usize,
}

impl PartialEq for SearchState {
    fn eq(&self, other: &Self) -> bool {
        self.chosen == other.chosen
    }
}
impl Eq for SearchState {}

impl Ord for SearchState {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max-heap, so "greater" must mean "explore
        // first": fewer missing elements, then fewer chosen candidates,
        // then a lexicographically smaller `chosen` as a final,
        // always-decisive, deterministic tie-break -- needs no extra
        // data, unlike e.g. commercial_catalog::search's PolicyEntry,
        // whose rows don't carry an equivalent natural index-vector
        // identity and so need a name-based tiebreak instead.
        other
            .priority
            .elements_missing
            .cmp(&self.priority.elements_missing)
            .then_with(|| other.priority.depth.cmp(&self.priority.depth))
            .then_with(|| other.chosen.cmp(&self.chosen))
    }
}
impl PartialOrd for SearchState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Builds the child state formed by adding candidate index `next` to
/// `parent_chosen`, or `None` if this specific addition is a monotonic
/// dead end (introduces a forbidden element, or an element neither the
/// target nor any curated byproduct can account for). Either violation
/// can never be undone by a later addition, so the whole subtree rooted
/// at this child is safe to prune here, without visiting any of its
/// descendants individually -- this is the real mechanism behind Phase
/// 29's efficiency gain over the previous exhaustive generator: a
/// pruned subtree never consumes a `SearchBudget::max_precursor_sets`
/// "combination considered" slot, so the same nominal budget lands on a
/// higher fraction of combinations that actually have a chance. Pushes
/// exactly one `RejectedCandidate` documenting the prune when it occurs.
#[allow(clippy::too_many_arguments)]
fn try_extend_state(
    parent_chosen: &[usize],
    next: usize,
    candidates: &[PrecursorCandidate],
    target_elements: &BTreeSet<Element>,
    byproduct_elements: &BTreeSet<Element>,
    forbidden_elements: &BTreeSet<Element>,
    rejected: &mut Vec<RejectedCandidate>,
) -> Option<SearchState> {
    let mut chosen = Vec::with_capacity(parent_chosen.len() + 1);
    chosen.extend_from_slice(parent_chosen);
    chosen.push(next);

    let combo_elements: BTreeSet<Element> = chosen
        .iter()
        .flat_map(|&i| candidates[i].composition.elements())
        .collect();

    if let Some(bad) = combo_elements
        .iter()
        .find(|e| forbidden_elements.contains(e))
    {
        rejected.push(RejectedCandidate {
            precursors: chosen.iter().map(|&i| candidates[i].id.clone()).collect(),
            reason_codes: vec![RejectionCode::ForbiddenElementPresent],
            explanation: format!(
                "precursor set contains forbidden element {bad} -- every larger \
                combination built on this one is pruned for the same reason"
            ),
        });
        return None;
    }

    let unremovable: Vec<Element> = combo_elements
        .difference(target_elements)
        .filter(|e| !byproduct_elements.contains(e))
        .copied()
        .collect();
    if !unremovable.is_empty() {
        rejected.push(RejectedCandidate {
            precursors: chosen.iter().map(|&i| candidates[i].id.clone()).collect(),
            reason_codes: vec![RejectionCode::UnsupportedByproductRequired],
            explanation: format!(
                "precursor set introduces element(s) with no curated byproduct to remove \
                them: {} -- every larger combination built on this one is pruned for the \
                same reason",
                join_symbols(&unremovable)
            ),
        });
        return None;
    }

    let missing: BTreeSet<Element> = target_elements
        .difference(&combo_elements)
        .copied()
        .collect();
    let priority = SearchPriority {
        elements_missing: missing.len(),
        depth: chosen.len(),
    };
    Some(SearchState {
        chosen,
        missing,
        priority,
    })
}

/// Attempts a stoichiometric balance for a state whose chosen candidates
/// already cover every target element, recording an accept or a
/// `NoStoichiometricBalance`/`DuplicatePlan` rejection. Unchanged
/// balance-then-dedup logic from the previous exhaustive search, just
/// invoked once per complete frontier state instead of once per
/// eagerly-generated combination. Returns the number of `balance::balance`
/// calls made, so the caller can report it alongside budget-exhaustion
/// diagnostics (this function makes gugen's own only repeated expensive
/// operation inside `search_precursor_sets` -- candidates themselves
/// arrive pre-materialized, so this search makes no provider calls of
/// its own to budget separately).
fn evaluate_complete_state(
    chosen: &[usize],
    candidates: &[PrecursorCandidate],
    target: &Composition,
    byproduct_subsets: &[Vec<Composition>],
    accepted: &mut Vec<AcceptedPrecursorSet>,
    rejected: &mut Vec<RejectedCandidate>,
) -> Result<usize> {
    let chosen_candidates: Vec<&PrecursorCandidate> =
        chosen.iter().map(|&i| &candidates[i]).collect();
    let ids: Vec<PrecursorId> = chosen_candidates.iter().map(|c| c.id.clone()).collect();
    let reactant_compositions: Vec<Composition> = chosen_candidates
        .iter()
        .map(|c| c.composition.clone())
        .collect();

    let mut found = Vec::new();
    let mut balance_calls = 0usize;
    for subset in byproduct_subsets {
        let mut products = vec![target.clone()];
        products.extend(subset.iter().cloned());
        balance_calls += 1;
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
            explanation: "no integer balance exists for this precursor set against the target, \
                with or without curated byproducts"
                .to_string(),
        });
        return Ok(balance_calls);
    }

    for reaction in found {
        // `balance()` operates on bare `Composition`s and drops any
        // reactant whose solved coefficient is zero, so `reaction`'s
        // reactant list is not guaranteed to be `chosen` unfiltered --
        // re-derive the id list by matching composition rather than
        // assuming index alignment with `ids`.
        let matched_ids: Vec<PrecursorId> = reaction
            .reactants()
            .iter()
            .map(|species| {
                chosen_candidates
                    .iter()
                    .find(|c| c.composition == species.composition)
                    .map(|c| c.id.clone())
                    .expect(
                        "balance() only returns reactant species drawn from \
                        the compositions it was given",
                    )
            })
            .collect();
        let candidate_set = AcceptedPrecursorSet {
            precursors: matched_ids,
            reaction,
        };

        // A larger combination can balance with one of its precursors'
        // coefficient solved to zero (`balance()` then drops it), which
        // collapses to the exact same reaction a smaller combination
        // already produced -- e.g. {BaCO3, BaO, TiO2} with BaCO3 zeroed
        // out equals {BaO, TiO2} outright. Two catalog entries sharing a
        // composition under different ids can also collapse to the same
        // reaction via two different combinations. Either way this is
        // one real route, not two: recording it twice would let it
        // silently double up in `Planner`'s ranked output and consume
        // two slots of `max_plans_returned` for what is really one plan.
        //
        // Dedup keys on `reaction` alone (not the full struct), and
        // keeps whichever colliding `precursors` id list sorts
        // lexicographically smallest, not whichever arrived first --
        // this makes the result independent of evaluation order, which
        // matters now that the frontier visits combinations in priority
        // order rather than always dictionary order (Phase 29). A
        // linear scan is O(n) per check (O(n^2) overall) -- accepted
        // counts stay small at this crate's catalog/budget scale, so
        // this is simpler than adding Hash/Ord to `BalancedReaction`
        // just to use a set.
        if let Some(existing_index) = accepted
            .iter()
            .position(|a| a.reaction == candidate_set.reaction)
        {
            if candidate_set.precursors < accepted[existing_index].precursors {
                let superseded = std::mem::replace(&mut accepted[existing_index], candidate_set);
                rejected.push(RejectedCandidate {
                    precursors: superseded.precursors,
                    reason_codes: vec![RejectionCode::DuplicatePlan],
                    explanation: "this precursor set and balanced reaction were already \
                        found via a different combination of candidates; a \
                        lexicographically-smaller equivalent precursor set was found \
                        and is kept instead, so the result does not depend on which \
                        combination was evaluated first"
                        .to_string(),
                });
            } else {
                rejected.push(RejectedCandidate {
                    precursors: candidate_set.precursors,
                    reason_codes: vec![RejectionCode::DuplicatePlan],
                    explanation: "this precursor set and balanced reaction were already \
                        found via a different combination of candidates (a larger \
                        combination's extra precursor solved to a zero coefficient, \
                        collapsing to the same effective reactants, or a different \
                        catalog entry shares this composition)"
                        .to_string(),
                });
            }
            continue;
        }
        accepted.push(candidate_set);
    }
    Ok(balance_calls)
}

/// Searches for precursor sets that can plausibly produce `target`,
/// drawn from `candidates` (typically the output of a `PrecursorCatalog`
/// query), respecting `constraints` and `budget`.
///
/// Deterministic guided best-first search (Phase 29, AGENTS.md §9):
/// starting from each single candidate, a frontier of partial
/// combinations (`SearchState`) is explored in order of fewest missing
/// target elements first, popping and expanding one state at a time
/// (`budget.max_precursor_sets` counts states actually popped, i.e.
/// combinations genuinely considered -- see `try_extend_state`'s own
/// doc comment for why a pruned subtree never consumes this budget at
/// all). A state whose chosen candidates already cover every target
/// element attempts a stoichiometric balance immediately (target alone
/// first, then with each curated byproduct subset added -- smallest
/// subset first, never the whole curated set at once; see the
/// regression test on `balance()` documenting why: extra byproduct
/// columns can push a valid answer into a *combination* of null-space
/// basis vectors that `balance()`'s single-basis-vector check won't
/// find) and is still expanded further afterward (a larger superset may
/// balance too, or may collapse to the same route -- see
/// `evaluate_complete_state`'s duplicate handling), up to
/// `budget.max_precursors_per_plan` candidates. With an unlimited
/// budget this visits the exact same combinations, and produces the
/// exact same `accepted` set, as the crate's own brute-force generator
/// (pinned by `search_matches_brute_force_enumeration_under_an_unlimited_budget`)
/// -- only the *order* of exploration, and hence which combinations
/// survive a *limited* budget, differs from the dictionary-order
/// generator this replaced.
///
/// `budget.max_plans_returned` is deliberately **not** applied here: with
/// no ranking in this module, truncating `accepted` here would keep
/// whichever combinations happened to be explored first, not the best
/// ones. Callers that want a bounded final count (`Planner`, Phase 6) rank
/// `accepted` by score first and truncate after, explaining what didn't
/// make the cut.
///
/// `DUPLICATE_PLAN` is reachable, as above. `PRECURSOR_COUNT_EXCEEDED` is
/// now reachable too (unlike before Phase 29): a state that reaches
/// `max_precursors_per_plan` candidates while still missing target
/// element(s) is a genuine dead end -- it cannot grow further and never
/// covered the target, so it is rejected explicitly rather than merely
/// stopping silently. `ATMOSPHERE_CONFLICT`, `HAZARD_POLICY_BLOCKED`,
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

    let mut accepted: Vec<AcceptedPrecursorSet> = Vec::new();
    let mut rejected = Vec::new();
    let mut frontier: BinaryHeap<SearchState> = BinaryHeap::new();

    for next in 0..candidates.len() {
        if let Some(state) = try_extend_state(
            &[],
            next,
            candidates,
            &target_elements,
            &byproduct_elements,
            &constraints.forbidden_elements,
            &mut rejected,
        ) {
            frontier.push(state);
        }
    }

    let mut considered = 0usize;
    let mut balance_calls = 0usize;
    let mut budget_exhausted = false;

    while let Some(state) = frontier.pop() {
        if considered >= budget.max_precursor_sets {
            budget_exhausted = true;
            break;
        }
        considered += 1;

        if state.missing.is_empty() {
            balance_calls += evaluate_complete_state(
                &state.chosen,
                candidates,
                target,
                &byproduct_subsets,
                &mut accepted,
                &mut rejected,
            )?;
        }

        if state.chosen.len() >= budget.max_precursors_per_plan {
            if !state.missing.is_empty() {
                rejected.push(RejectedCandidate {
                    precursors: state
                        .chosen
                        .iter()
                        .map(|&i| candidates[i].id.clone())
                        .collect(),
                    reason_codes: vec![RejectionCode::PrecursorCountExceeded],
                    explanation: format!(
                        "reached the {}-precursor limit (SearchBudget::max_precursors_per_plan) \
                        while still missing target element(s): {}",
                        budget.max_precursors_per_plan,
                        join_symbols(&state.missing.iter().copied().collect::<Vec<_>>())
                    ),
                });
            }
            continue;
        }

        let start = state.chosen.last().map_or(0, |&i| i + 1);
        for next in start..candidates.len() {
            if let Some(child) = try_extend_state(
                &state.chosen,
                next,
                candidates,
                &target_elements,
                &byproduct_elements,
                &constraints.forbidden_elements,
                &mut rejected,
            ) {
                frontier.push(child);
            }
        }
    }

    if budget_exhausted {
        rejected.push(RejectedCandidate {
            precursors: vec![],
            reason_codes: vec![RejectionCode::SearchBudgetExhausted],
            explanation: format!(
                "stopped after considering {considered} precursor-set combination(s) in \
                priority order ({balance_calls} balance() call(s) attempted); more were \
                possible"
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
///
/// Production code no longer calls this (Phase 29 replaced dictionary-
/// order generation with `search_precursor_sets`'s own guided best-first
/// frontier) -- kept, test-only, as the brute-force reference oracle for
/// `search_matches_brute_force_enumeration_under_an_unlimited_budget`.
#[cfg(test)]
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

        // Sr is unremovable the moment SrCO3 is chosen at all -- a
        // monotonic violation no later addition can undo (Phase 29's
        // frontier search prunes the whole subtree here, at the
        // one-candidate root, rather than re-discovering the same
        // violation separately for every larger combination built on
        // top of it -- see `try_extend_state`'s own doc comment). So the
        // rejection is recorded once, for `{SrCO3}` alone, and covers
        // `{SrCO3, TiO2}`/`{SrCO3, BaO}`/`{SrCO3, TiO2, BaO}` implicitly,
        // rather than once per combination as the pre-Phase-29 exhaustive
        // generator would have recorded it.
        let bad_root = outcome.rejected.iter().find(|r| {
            let ids: BTreeSet<&str> = r.precursors.iter().map(|p| p.0.as_str()).collect();
            ids == BTreeSet::from(["SrCO3"])
        });
        assert_eq!(
            bad_root.map(|r| r.reason_codes.clone()),
            Some(vec![RejectionCode::UnsupportedByproductRequired])
        );
        assert!(
            outcome
                .accepted
                .iter()
                .all(|a| !a.precursors.iter().any(|p| p.0 == "SrCO3")),
            "no accepted set may use SrCO3, directly or via any larger combination: {:?}",
            outcome.accepted
        );
    }

    /// `curated_byproducts()` now includes NO2 (metal-nitrate thermal
    /// decomposition, `2 Ba(NO3)2 -> 2 BaO + 4 NO2 + O2`) -- a nitrate
    /// precursor that previously introduced an uncoverable N atom (like
    /// SrCO3's uncoverable Sr in `rejects_sets_with_unremovable_extra_elements`)
    /// must now be accepted.
    #[test]
    fn accepts_a_nitrate_precursor_via_the_curated_no2_byproduct() {
        let target = composition(&[("Ba", 1.0), ("O", 1.0)]);
        let catalog = vec![candidate(
            "Ba(NO3)2",
            &[("Ba", 1.0), ("N", 2.0), ("O", 6.0)],
        )];
        let outcome = search_precursor_sets(
            &target,
            &catalog,
            &PlanningConstraints::default(),
            &generous_budget(),
        )
        .unwrap();

        let nitrate_route = outcome.accepted.iter().find(|a| {
            let ids: BTreeSet<&str> = a.precursors.iter().map(|p| p.0.as_str()).collect();
            ids == BTreeSet::from(["Ba(NO3)2"])
        });
        assert!(
            nitrate_route.is_some(),
            "Ba(NO3)2 -> BaO + NO2 + O2 must now be accepted: {:?}",
            outcome
        );
    }

    /// `curated_byproducts()` now includes CO (metal-oxalate thermal
    /// decomposition, `FeC2O4 -> FeO + CO2 + CO`) -- an oxalate
    /// precursor that previously introduced an uncoverable second
    /// carbon-and-oxygen sink must now be accepted.
    #[test]
    fn accepts_an_oxalate_precursor_via_the_curated_co_byproduct() {
        let target = composition(&[("Fe", 1.0), ("O", 1.0)]);
        let catalog = vec![candidate("FeC2O4", &[("Fe", 1.0), ("C", 2.0), ("O", 4.0)])];
        let outcome = search_precursor_sets(
            &target,
            &catalog,
            &PlanningConstraints::default(),
            &generous_budget(),
        )
        .unwrap();

        let oxalate_route = outcome.accepted.iter().find(|a| {
            let ids: BTreeSet<&str> = a.precursors.iter().map(|p| p.0.as_str()).collect();
            ids == BTreeSet::from(["FeC2O4"])
        });
        assert!(
            oxalate_route.is_some(),
            "FeC2O4 -> FeO + CO2 + CO must now be accepted: {:?}",
            outcome
        );
    }

    /// `src/balance.rs`'s
    /// `offering_every_curated_byproduct_at_once_can_introduce_real_ambiguity_co_does_here`
    /// found that raw `balance()`, given every curated byproduct at
    /// once, now returns *two* independently valid solutions for
    /// BaCO3 + TiO2 -> BaTiO3 (CO's presence alongside CO2/O2 opens a
    /// second basis vector). This confirms, at the real search level,
    /// that `search_precursor_sets` is unaffected: `power_set`'s
    /// strictly smallest-cardinality-first subset order means the
    /// size-1 `{CO2}` subset alone balances this combination and the
    /// loop breaks before ever trying a CO-inclusive subset, so exactly
    /// one BaTiO3 route is accepted, not two.
    #[test]
    fn search_finds_exactly_one_batio3_route_even_though_the_full_curated_set_is_ambiguous() {
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let catalog = vec![
            candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
        ];
        let outcome = search_precursor_sets(
            &target,
            &catalog,
            &PlanningConstraints::default(),
            &generous_budget(),
        )
        .unwrap();

        let ba_ti_routes: Vec<_> = outcome
            .accepted
            .iter()
            .filter(|a| {
                let ids: BTreeSet<&str> = a.precursors.iter().map(|p| p.0.as_str()).collect();
                ids == BTreeSet::from(["BaCO3", "TiO2"])
            })
            .collect();
        assert_eq!(
            ba_ti_routes.len(),
            1,
            "exactly one BaCO3+TiO2 route must be accepted, not the CO-inclusive ambiguous \
            second solution: {:?}",
            outcome.accepted
        );
        assert_eq!(
            ba_ti_routes[0].reaction.products().len(),
            2,
            "the accepted route's byproduct must be plain CO2, not the CO+O2 split"
        );
    }

    /// `curated_byproducts()` now includes acetone (metal-acetate
    /// ketonic decarboxylation, `Ba(CH3COO)2 -> BaO + (CH3)2CO + CO2`)
    /// -- an acetate precursor that previously introduced an
    /// uncoverable hydrogen-and-extra-carbon sink must now be accepted.
    #[test]
    fn accepts_an_acetate_precursor_via_the_curated_acetone_byproduct() {
        let target = composition(&[("Ba", 1.0), ("O", 1.0)]);
        let catalog = vec![candidate(
            "Ba(CH3COO)2",
            &[("Ba", 1.0), ("C", 4.0), ("H", 6.0), ("O", 4.0)],
        )];
        let outcome = search_precursor_sets(
            &target,
            &catalog,
            &PlanningConstraints::default(),
            &generous_budget(),
        )
        .unwrap();

        let acetate_route = outcome.accepted.iter().find(|a| {
            let ids: BTreeSet<&str> = a.precursors.iter().map(|p| p.0.as_str()).collect();
            ids == BTreeSet::from(["Ba(CH3COO)2"])
        });
        assert!(
            acetate_route.is_some(),
            "Ba(CH3COO)2 -> BaO + (CH3)2CO + CO2 must now be accepted: {:?}",
            outcome
        );
    }

    /// `src/balance.rs`'s
    /// `offering_every_curated_byproduct_at_once_can_introduce_more_ambiguity_once_hydrogen_and_carbon_coexist_acetone_does_there`
    /// found that raw `balance()`, given every curated byproduct at
    /// once, now returns *four* independently valid solutions for
    /// `Ba(OH)2 + BaCO3 + TiO2 -> Ba2TiO4` (acetone's presence, once
    /// both hydrogen and carbon are already present, opens a fourth
    /// basis vector). This confirms, at the real search level, that
    /// `search_precursor_sets` is unaffected: the size-1 `{CO2}`
    /// subset alone already balances this 3-candidate combination
    /// (with `Ba(OH)2`'s coefficient solved to zero), so the loop
    /// breaks before ever trying an acetone-inclusive subset.
    #[test]
    fn search_finds_exactly_one_ba2tio4_route_even_though_the_full_curated_set_is_ambiguous() {
        let target = composition(&[("Ba", 2.0), ("Ti", 1.0), ("O", 4.0)]);
        let catalog = vec![
            candidate("Ba(OH)2", &[("Ba", 1.0), ("O", 2.0), ("H", 2.0)]),
            candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
        ];
        let outcome = search_precursor_sets(
            &target,
            &catalog,
            &PlanningConstraints::default(),
            &generous_budget(),
        )
        .unwrap();

        let carbonate_routes: Vec<_> = outcome
            .accepted
            .iter()
            .filter(|a| {
                let ids: BTreeSet<&str> = a.precursors.iter().map(|p| p.0.as_str()).collect();
                ids == BTreeSet::from(["BaCO3", "TiO2"])
            })
            .collect();
        assert_eq!(
            carbonate_routes.len(),
            1,
            "exactly one BaCO3+TiO2 route must be accepted, not the acetone-inclusive \
            ambiguous alternative: {:?}",
            outcome.accepted
        );
        assert!(
            outcome.accepted.iter().all(|a| {
                let ids: BTreeSet<&str> = a.precursors.iter().map(|p| p.0.as_str()).collect();
                ids != BTreeSet::from(["Ba(OH)2", "BaCO3", "TiO2"])
            }),
            "the 3-candidate acetone-splitting route must never be accepted: {:?}",
            outcome.accepted
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
                accepted.reaction.reactants().len(),
                "precursors and reactants must be the same length: {accepted:?}"
            );
            for (id, species) in accepted
                .precursors
                .iter()
                .zip(accepted.reaction.reactants())
            {
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

    /// A redundant Ba source (BaCO3 and BaO both supply Ba) means the
    /// 3-candidate combination {BaCO3, BaO, TiO2} can balance with BaCO3's
    /// coefficient solved to zero -- collapsing to the exact same
    /// precursors and reaction the 2-candidate combination {BaO, TiO2}
    /// already produced. That must surface once in `accepted` and once
    /// (not silently) as a `DuplicatePlan` rejection, never twice in
    /// `accepted` -- a real bug caught by `gugen plan`'s CLI output on this
    /// exact fixture, not a hypothetical case.
    #[test]
    fn a_redundant_larger_combination_is_rejected_as_a_duplicate_not_double_accepted() {
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let catalog = vec![
            candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
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

        let expected_ids = BTreeSet::from([
            PrecursorId("BaO".to_string()),
            PrecursorId("TiO2".to_string()),
        ]);
        let occurrences = outcome
            .accepted
            .iter()
            .filter(|a| a.precursors.iter().cloned().collect::<BTreeSet<_>>() == expected_ids)
            .count();
        assert_eq!(
            occurrences, 1,
            "the {{BaO, TiO2}} precursor set must be accepted exactly once: {:?}",
            outcome.accepted
        );

        assert!(
            outcome.rejected.iter().any(|r| {
                r.reason_codes == vec![RejectionCode::DuplicatePlan]
                    && r.precursors.contains(&PrecursorId("BaO".to_string()))
                    && r.precursors.contains(&PrecursorId("TiO2".to_string()))
            }),
            "the redundant 3-candidate collapse must be explained as DuplicatePlan, not \
            silently dropped or silently double-accepted: {:?}",
            outcome.rejected
        );
    }

    /// Two catalog entries can share a composition under different ids
    /// (e.g. two vendors of the same compound) and independently balance to
    /// the exact same reaction. Whichever is evaluated first must not
    /// decide which id list is kept -- the canonical (lexicographically
    /// smallest) one always wins, so a caller free to visit candidates in
    /// any order (Phase 29's guided search, not just today's dictionary
    /// order) gets the same `accepted` output regardless of traversal
    /// order.
    #[test]
    fn duplicate_collapse_keeps_the_lexicographically_smallest_precursor_set_regardless_of_arrival_order()
     {
        let target = composition(&[("Ba", 1.0), ("O", 1.0)]);
        let vendor_a = candidate("BaO-vendorA", &[("Ba", 1.0), ("O", 1.0)]);
        let vendor_b = candidate("BaO-vendorB", &[("Ba", 1.0), ("O", 1.0)]);

        let forward = search_precursor_sets(
            &target,
            &[vendor_a.clone(), vendor_b.clone()],
            &PlanningConstraints::default(),
            &generous_budget(),
        )
        .unwrap();
        let reversed = search_precursor_sets(
            &target,
            &[vendor_b, vendor_a],
            &PlanningConstraints::default(),
            &generous_budget(),
        )
        .unwrap();

        let expected = vec![PrecursorId("BaO-vendorA".to_string())];
        assert_eq!(
            forward
                .accepted
                .iter()
                .map(|a| a.precursors.clone())
                .collect::<Vec<_>>(),
            vec![expected.clone()],
            "forward order must keep the lexicographically-smaller id: {:?}",
            forward.accepted
        );
        assert_eq!(
            reversed
                .accepted
                .iter()
                .map(|a| a.precursors.clone())
                .collect::<Vec<_>>(),
            vec![expected],
            "reversed order must keep the same id, not whichever arrived first: {:?}",
            reversed.accepted
        );
        assert!(
            reversed
                .rejected
                .iter()
                .any(|r| r.reason_codes == vec![RejectionCode::DuplicatePlan]),
            "the superseded vendorB-first entry must be recorded as DuplicatePlan, not \
            silently dropped: {:?}",
            reversed.rejected
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

    /// Brute-force reference oracle for
    /// `search_matches_brute_force_enumeration_under_an_unlimited_budget`
    /// below -- deliberately independent of `search_precursor_sets`'s own
    /// frontier logic (not the same code path dressed up as a second
    /// implementation): generates every combination via
    /// `generate_combinations` (dictionary order, this crate's
    /// pre-Phase-29 exhaustive algorithm) and evaluates each one with its
    /// own copy of the cheap forbidden/coverage/removability checks,
    /// reusing only `evaluate_complete_state` (the balance-then-dedup
    /// step both algorithms must agree on to produce the same
    /// `accepted` set at all).
    fn brute_force_accepted(
        target: &Composition,
        candidates: &[PrecursorCandidate],
        constraints: &PlanningConstraints,
        max_precursors_per_plan: usize,
    ) -> Vec<AcceptedPrecursorSet> {
        let target_elements: BTreeSet<Element> = target.elements().collect();
        let byproducts = balance::curated_byproducts().unwrap();
        let byproduct_elements: BTreeSet<Element> =
            byproducts.iter().flat_map(Composition::elements).collect();
        let byproduct_subsets = power_set(&byproducts);

        let (combos, _exhausted) =
            generate_combinations(candidates.len(), max_precursors_per_plan, usize::MAX);

        let mut accepted = Vec::new();
        let mut rejected = Vec::new();
        for combo in &combos {
            let combo_elements: BTreeSet<Element> = combo
                .iter()
                .flat_map(|&i| candidates[i].composition.elements())
                .collect();
            if combo_elements
                .iter()
                .any(|e| constraints.forbidden_elements.contains(e))
            {
                continue;
            }
            if !target_elements.is_subset(&combo_elements) {
                continue;
            }
            let unremovable = combo_elements
                .difference(&target_elements)
                .any(|e| !byproduct_elements.contains(e));
            if unremovable {
                continue;
            }
            evaluate_complete_state(
                combo,
                candidates,
                target,
                &byproduct_subsets,
                &mut accepted,
                &mut rejected,
            )
            .unwrap();
        }
        accepted
    }

    fn canonical_sort(accepted: Vec<AcceptedPrecursorSet>) -> Vec<Vec<String>> {
        let mut sets: Vec<Vec<String>> = accepted
            .into_iter()
            .map(|a| {
                let mut ids: Vec<String> = a.precursors.iter().map(|p| p.0.clone()).collect();
                ids.sort();
                ids
            })
            .collect();
        sets.sort();
        sets
    }

    /// Phase 29's core correctness requirement: with an unlimited budget,
    /// the guided frontier search must visit every combination the old
    /// exhaustive dictionary-order generator would have, and therefore
    /// produce the exact same `accepted` set -- only the *order* of
    /// exploration (and hence what a *limited* budget leaves out)
    /// differs. Covers a plain case, a redundant-precursor collapse case
    /// (the exact fixture `duplicate_collapse_keeps_the_lexicographically_smallest_precursor_set_regardless_of_arrival_order`
    /// and `a_redundant_larger_combination_is_rejected_as_a_duplicate_not_double_accepted`
    /// already pin individually), and a case with both a forbidden-
    /// element-shaped decoy and an irrelevant decoy present together.
    #[test]
    fn search_matches_brute_force_enumeration_under_an_unlimited_budget() {
        let scenarios: Vec<(Composition, Vec<PrecursorCandidate>)> = vec![
            (
                composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
                barium_titanate_catalog(),
            ),
            (
                composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
                vec![
                    candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
                    candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
                    candidate("BaO", &[("Ba", 1.0), ("O", 1.0)]),
                    candidate("SrCO3", &[("Sr", 1.0), ("C", 1.0), ("O", 3.0)]),
                    candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]),
                ],
            ),
            (
                composition(&[("Fe", 2.0), ("O", 3.0)]),
                vec![
                    candidate("Fe2O3", &[("Fe", 2.0), ("O", 3.0)]),
                    candidate("Fe", &[("Fe", 1.0)]),
                    candidate("O2", &[("O", 2.0)]),
                    candidate("FeCO3", &[("Fe", 1.0), ("C", 1.0), ("O", 3.0)]),
                ],
            ),
        ];

        for (target, catalog) in scenarios {
            let outcome = search_precursor_sets(
                &target,
                &catalog,
                &PlanningConstraints::default(),
                &SearchBudget {
                    max_precursor_sets: usize::MAX,
                    ..SearchBudget::default()
                },
            )
            .unwrap();
            let reference = brute_force_accepted(
                &target,
                &catalog,
                &PlanningConstraints::default(),
                SearchBudget::default().max_precursors_per_plan,
            );

            assert_eq!(
                canonical_sort(outcome.accepted),
                canonical_sort(reference),
                "frontier search and brute-force enumeration must agree under an \
                unlimited budget for target {target:?}"
            );
            assert!(
                !outcome
                    .rejected
                    .iter()
                    .any(|r| r.reason_codes == vec![RejectionCode::SearchBudgetExhausted]),
                "an unlimited budget must never report SearchBudgetExhausted"
            );
        }
    }
}
