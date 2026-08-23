//! Hard-constraint filtering, per-row candidate ranking, and the bounded
//! deterministic combination search (exact tier when the space fits the
//! budget, a lazy heuristic tier otherwise). See the module root doc
//! comment and `search_combinations`'s own doc comment for the two-tier
//! rationale.

use super::model::{
    AvailabilityStatus, CommercialCombination, CommercialExclusionCode, CommercialOfferSelection,
    CommercialPlanningConfig, CommercialPlanningRequest, CommercialPrecursorOffer,
    CommercialRankingPolicy, CurrencyCode, MissingCommercialDataPolicy, Money, PurityFraction,
};
use super::quantity::{OfferQuantity, cost_rank_key};
use crate::composition::Composition;
use crate::precursor::PrecursorId;

pub(crate) fn hard_constraint_violations(
    offer: &CommercialPrecursorOffer,
    request: &CommercialPlanningRequest,
) -> Vec<CommercialExclusionCode> {
    let mut codes = Vec::new();
    let missing_is_reject = request.missing_data_policy == MissingCommercialDataPolicy::Reject;

    if let Some(min_purity) = request.min_purity {
        match offer.purity {
            Some(p) if p.value() < min_purity.value() => {
                codes.push(CommercialExclusionCode::PurityBelowMinimum)
            }
            None if missing_is_reject => {
                codes.push(CommercialExclusionCode::MissingConstrainedField)
            }
            _ => {}
        }
    }

    if request
        .allowed_manufacturers
        .as_ref()
        .is_some_and(|allowed| !allowed.contains(&offer.manufacturer))
        || request.excluded_manufacturers.contains(&offer.manufacturer)
    {
        codes.push(CommercialExclusionCode::ManufacturerNotAllowed);
    }

    if let Some(max_lead_time) = request.max_lead_time_days {
        match offer.lead_time_days {
            Some(lt) if lt > max_lead_time => {
                codes.push(CommercialExclusionCode::LeadTimeExceedsMaximum)
            }
            None if missing_is_reject => {
                codes.push(CommercialExclusionCode::MissingConstrainedField)
            }
            _ => {}
        }
    }

    if let Some(allowed) = &request.allowed_availability_statuses {
        match offer.availability {
            Some(status) if !allowed.contains(&status) => {
                codes.push(CommercialExclusionCode::AvailabilityExcluded)
            }
            None if missing_is_reject => {
                codes.push(CommercialExclusionCode::MissingConstrainedField)
            }
            _ => {}
        }
    }

    if let Some(allowed) = &request.allowed_physical_forms {
        match &offer.physical_form {
            Some(form) if !allowed.contains(form) => {
                codes.push(CommercialExclusionCode::PhysicalFormNotAllowed)
            }
            None if missing_is_reject => {
                codes.push(CommercialExclusionCode::MissingConstrainedField)
            }
            _ => {}
        }
    }

    if request
        .required_tags
        .iter()
        .any(|tag| !offer.tags.contains(tag))
    {
        codes.push(CommercialExclusionCode::RequiredTagMissing);
    }
    if offer
        .tags
        .iter()
        .any(|tag| request.excluded_tags.contains(tag))
    {
        codes.push(CommercialExclusionCode::ExcludedTagPresent);
    }

    if let Some(allowed_currencies) = &request.allowed_currencies {
        match offer.unit_price {
            Some(price) if !allowed_currencies.contains(&price.currency()) => {
                codes.push(CommercialExclusionCode::CurrencyNotAllowed)
            }
            None if missing_is_reject => {
                codes.push(CommercialExclusionCode::MissingConstrainedField)
            }
            _ => {}
        }
    }

    if request.require_known_price && offer.unit_price.is_none() {
        codes.push(CommercialExclusionCode::PriceRequiredButUnknown);
    }
    if request.require_known_package_size && offer.package_mass.is_none() {
        codes.push(CommercialExclusionCode::PackageSizeRequiredButUnknown);
    }

    codes
}

// ---------------------------------------------------------------------
// Per-row candidate ranking
// ---------------------------------------------------------------------

pub(crate) struct OfferCandidate<'a> {
    pub(crate) offer: &'a CommercialPrecursorOffer,
    pub(crate) unresolved_fields: Vec<&'static str>,
    pub(crate) quantity: OfferQuantity,
}

/// Total order, ascending = better: fewer unresolved fields, then cheaper
/// (within a comparable cost bucket), then shorter lead time, then higher
/// purity, then manufacturer/catalog_number/offer_id as final deterministic
/// tiebreaks (offer_id is always unique, so this never returns `Equal` for
/// two distinct offers).
pub(crate) fn offer_rank_order(a: &OfferCandidate, b: &OfferCandidate) -> std::cmp::Ordering {
    a.unresolved_fields
        .len()
        .cmp(&b.unresolved_fields.len())
        .then_with(|| cost_rank_key(a.quantity.subtotal).cmp(&cost_rank_key(b.quantity.subtotal)))
        .then_with(|| {
            a.offer
                .lead_time_days
                .unwrap_or(u32::MAX)
                .cmp(&b.offer.lead_time_days.unwrap_or(u32::MAX))
        })
        .then_with(|| {
            let pa = a.offer.purity.map(|p| p.value()).unwrap_or(0.0);
            let pb = b.offer.purity.map(|p| p.value()).unwrap_or(0.0);
            pb.total_cmp(&pa) // descending: higher purity first
        })
        .then_with(|| a.offer.manufacturer.cmp(&b.offer.manufacturer))
        .then_with(|| a.offer.catalog_number.cmp(&b.offer.catalog_number))
        .then_with(|| a.offer.offer_id.0.cmp(&b.offer.offer_id.0))
}

// ---------------------------------------------------------------------
// Bounded combination search
// ---------------------------------------------------------------------

/// A materialized, pre-computed rank key for one complete combination (one
/// candidate index chosen per row) -- `Ord` compares only these precomputed
/// fields, never needing external context once constructed, which is what
/// lets it live directly inside a `BinaryHeap`.
struct HeapEntry {
    indices: Vec<usize>,
    unresolved_sum: usize,
    total_cost: Option<Money>,
    cost_key: (u8, Option<CurrencyCode>, u64),
    max_lead_time: u32,
    min_purity: f64,
    manufacturers: Vec<String>,
    catalog_numbers: Vec<String>,
    offer_ids: Vec<String>,
}

impl HeapEntry {
    fn new(indices: Vec<usize>, rows: &[Vec<OfferCandidate>]) -> Self {
        let selected: Vec<&OfferCandidate> =
            indices.iter().zip(rows).map(|(&i, row)| &row[i]).collect();
        let unresolved_sum = selected.iter().map(|c| c.unresolved_fields.len()).sum();
        let total_cost = combination_total_cost(&selected);
        let cost_key = cost_rank_key(total_cost);
        let max_lead_time = selected
            .iter()
            .map(|c| c.offer.lead_time_days.unwrap_or(u32::MAX))
            .max()
            .unwrap_or(0);
        let min_purity = selected
            .iter()
            .map(|c| c.offer.purity.map(|p| p.value()).unwrap_or(0.0))
            .fold(f64::INFINITY, f64::min);
        let mut manufacturers: Vec<String> = selected
            .iter()
            .map(|c| c.offer.manufacturer.clone())
            .collect();
        manufacturers.sort();
        let mut catalog_numbers: Vec<String> = selected
            .iter()
            .map(|c| c.offer.catalog_number.clone().unwrap_or_default())
            .collect();
        catalog_numbers.sort();
        let mut offer_ids: Vec<String> = selected
            .iter()
            .map(|c| c.offer.offer_id.0.clone())
            .collect();
        offer_ids.sort();
        Self {
            indices,
            unresolved_sum,
            total_cost,
            cost_key,
            max_lead_time,
            min_purity,
            manufacturers,
            catalog_numbers,
            offer_ids,
        }
    }
}

impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}
impl Eq for HeapEntry {}
impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for HeapEntry {
    /// Reversed vs. the "ascending = better" convention used elsewhere in
    /// this module, so that `BinaryHeap` (a max-heap) pops the *best*
    /// combination first -- `self` compares `Greater` exactly when `self`
    /// is better than `other`.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .unresolved_sum
            .cmp(&self.unresolved_sum)
            .then_with(|| other.cost_key.cmp(&self.cost_key))
            .then_with(|| other.max_lead_time.cmp(&self.max_lead_time))
            .then_with(|| self.min_purity.total_cmp(&other.min_purity))
            .then_with(|| other.manufacturers.cmp(&self.manufacturers))
            .then_with(|| other.catalog_numbers.cmp(&self.catalog_numbers))
            .then_with(|| other.offer_ids.cmp(&self.offer_ids))
    }
}

fn combination_total_cost(selected: &[&OfferCandidate]) -> Option<Money> {
    let mut iter = selected.iter();
    let mut total = iter.next()?.quantity.subtotal?;
    for candidate in iter {
        total = total.checked_add(&candidate.quantity.subtotal?)?;
    }
    Some(total)
}

/// Whether a combination's total cost satisfies `max_total_cost` (when set).
/// A total that isn't comparable (unknown, or a different currency than the
/// ceiling) can't be verified against the ceiling -- it passes rather than
/// being silently excluded or included; the caller attaches a warning for
/// that case. This must run *before* `max_results_returned` truncation --
/// applying it after truncation can return zero combinations even though a
/// lower-ranked, budget-satisfying combination exists.
fn passes_max_total_cost(total_cost: Option<Money>, max_total_cost: Option<Money>) -> bool {
    match (total_cost, max_total_cost) {
        (_, None) => true,
        (Some(cost), Some(max_cost)) if cost.currency() == max_cost.currency() => {
            cost.minor_units() <= max_cost.minor_units()
        }
        _ => true,
    }
}

/// Enumerates complete combinations (one offer per row), best-first, up to
/// `config.max_results_returned`, evaluating at most
/// `config.max_combinations_evaluated`. Returns
/// `(combinations, combinations_evaluated, total_combination_space)` -- the
/// caller combines this with whether any row was truncated by
/// `max_offers_per_precursor` to determine `is_exhaustive`.
///
/// Two-tier, not a single "k smallest combinations from k sorted lists"
/// frontier search throughout: that lazy-heap technique is only provably
/// correct when every row's pre-sort order is monotonic with respect to
/// *every* combination-level aggregate it's used for -- true for
/// unresolved-field-count (sum), lead time (max), and purity (min), but
/// **false** for total-cost comparability. "Same currency as every other
/// selected offer" is a joint property across rows, not a per-row-local
/// one, so no fixed per-row order can make it monotonic (verified with a
/// concrete failure caught by this module's own test suite during
/// implementation: a two-currency catalog where the lazy search emitted a
/// mixed-currency, cost-unknown combination as its first/best result, when
/// a same-currency, cost-known combination existed and correctly outranks
/// it under `HeapEntry::Ord` once actually compared).
///
/// So: whenever the *entire* combination space fits within
/// `max_combinations_evaluated`, enumerate it exactly and rank by the real
/// `HeapEntry::Ord` -- no monotonicity assumption needed, provably correct,
/// and this is the common case for realistic per-precursor offer counts.
/// Only when the space is too large to enumerate does this fall back to
/// the lazy frontier search as a bounded, honest best-effort heuristic --
/// `is_exhaustive: false` already tells the caller the result may not be
/// the true global best in that case, which is now an accurate, not just
/// a budget-exhaustion, caveat.
pub(crate) fn search_combinations(
    rows: &[Vec<OfferCandidate>],
    config: &CommercialPlanningConfig,
    max_total_cost: Option<Money>,
) -> (Vec<Vec<usize>>, usize, u64) {
    if rows.is_empty() || rows.iter().any(|row| row.is_empty()) {
        return (Vec::new(), 0, 0);
    }

    let total_space: u64 = rows
        .iter()
        .fold(1u64, |acc, row| acc.saturating_mul(row.len() as u64));

    if total_space <= config.max_combinations_evaluated as u64 {
        exhaustive_search(rows, config, total_space, max_total_cost)
    } else {
        heuristic_search(rows, config, total_space, max_total_cost)
    }
}

/// Decodes every `combo_index` in `0..total_space` as a mixed-radix
/// (per-row base) index vector, scores each exactly, and returns the top
/// `max_results_returned` by the real `HeapEntry::Ord` -- correct by
/// direct enumeration, no monotonicity assumption. `total_space` is
/// guaranteed `<= config.max_combinations_evaluated` by the caller, so this
/// never allocates more than the caller's own configured budget.
/// Decodes `combo_index` (in `0..total_space`) into a per-row candidate
/// index vector -- shared by `exhaustive_search`, `ranked_search_by_policy`,
/// and `pareto_search`, all of which enumerate the same mixed-radix space.
fn decode_combo_index(combo_index: u64, rows: &[Vec<OfferCandidate>]) -> Vec<usize> {
    let mut remainder = combo_index;
    let mut indices = vec![0usize; rows.len()];
    for (row_i, row) in rows.iter().enumerate() {
        let len = row.len() as u64;
        indices[row_i] = (remainder % len) as usize;
        remainder /= len;
    }
    indices
}

fn exhaustive_search(
    rows: &[Vec<OfferCandidate>],
    config: &CommercialPlanningConfig,
    total_space: u64,
    max_total_cost: Option<Money>,
) -> (Vec<Vec<usize>>, usize, u64) {
    let mut all: Vec<HeapEntry> = Vec::with_capacity(total_space as usize);
    for combo_index in 0..total_space {
        all.push(HeapEntry::new(decode_combo_index(combo_index, rows), rows));
    }
    all.sort_by(|a, b| b.cmp(a)); // descending: best (greatest) first
    // Filter by max_total_cost *before* truncating -- otherwise a
    // budget-satisfying combination ranked below the top max_results_returned
    // entries would be silently dropped, leaving zero results even though a
    // qualifying combination exists.
    let results = all
        .into_iter()
        .filter(|e| passes_max_total_cost(e.total_cost, max_total_cost))
        .take(config.max_results_returned)
        .map(|e| e.indices)
        .collect();
    (results, total_space as usize, total_space)
}

/// Lazy frontier search (the "k smallest combinations from k sorted lists"
/// technique, the same family as Dijkstra/A* optimality), used only when
/// `total_space` exceeds the evaluation budget. Bounded (never visits more
/// than `max_combinations_evaluated` states, never materializes the full
/// product), and correct with respect to the aggregates that genuinely are
/// per-row-monotonic (unresolved-field count, lead time, purity) -- but see
/// `search_combinations`'s doc comment for why it is a best-effort
/// heuristic, not a proof of global optimality, specifically for total-cost
/// ranking across a catalog spanning more than one currency.
fn heuristic_search(
    rows: &[Vec<OfferCandidate>],
    config: &CommercialPlanningConfig,
    total_space: u64,
    max_total_cost: Option<Money>,
) -> (Vec<Vec<usize>>, usize, u64) {
    use std::collections::{BTreeSet, BinaryHeap};

    let mut heap = BinaryHeap::new();
    let mut visited: BTreeSet<Vec<usize>> = BTreeSet::new();
    let start = vec![0usize; rows.len()];
    visited.insert(start.clone());
    heap.push(HeapEntry::new(start, rows));

    let mut results = Vec::new();
    let mut evaluated = 0usize;

    while let Some(entry) = heap.pop() {
        if evaluated >= config.max_combinations_evaluated {
            break;
        }
        evaluated += 1;
        // Keep expanding neighbors even when this entry fails the cost
        // ceiling -- it's still a valid frontier node, and a
        // budget-satisfying combination may only be reachable through it.
        if passes_max_total_cost(entry.total_cost, max_total_cost) {
            results.push(entry.indices.clone());
            if results.len() >= config.max_results_returned {
                break;
            }
        }
        for row_i in 0..rows.len() {
            let mut neighbor = entry.indices.clone();
            neighbor[row_i] += 1;
            if neighbor[row_i] >= rows[row_i].len() {
                continue;
            }
            if visited.insert(neighbor.clone()) {
                heap.push(HeapEntry::new(neighbor, rows));
            }
        }
    }

    (results, evaluated, total_space)
}

// ---------------------------------------------------------------------
// Named policies (Phase 24C) -- `Balanced` keeps using `search_combinations`
// above, completely unchanged. Every other policy runs one of the two
// functions below instead: a capped direct enumeration of the same
// mixed-radix space, deliberately simpler and always-correct-for-what-it-
// evaluates rather than reusing the heuristic tier, which is only proven
// monotonic for `Balanced`'s own aggregates (see `search_combinations`'s
// doc comment above).
// ---------------------------------------------------------------------

/// Per-combination metrics for the named-policy and `Pareto` search paths
/// below -- deliberately separate from `HeapEntry` (used only by
/// `Balanced`'s own search, left untouched) so this new code can never
/// perturb `Balanced`'s behavior. Every optional dimension stays a genuine
/// `Option`, unlike `HeapEntry`'s internal unknown-value sentinels, since
/// `Pareto` specifically needs to tell "worst known value" apart from
/// "unknown".
struct PolicyEntry {
    indices: Vec<usize>,
    unresolved_sum: usize,
    total_cost: Option<Money>,
    cost_key: (u8, Option<CurrencyCode>, u64),
    max_lead_time_days: Option<u32>,
    min_purity: Option<f64>,
    total_excess_mass_grams: Option<f64>,
    manufacturers: Vec<String>,
    catalog_numbers: Vec<String>,
    offer_ids: Vec<String>,
}

impl PolicyEntry {
    fn new(indices: Vec<usize>, rows: &[Vec<OfferCandidate>]) -> Self {
        let selected: Vec<&OfferCandidate> =
            indices.iter().zip(rows).map(|(&i, row)| &row[i]).collect();
        let unresolved_sum = selected.iter().map(|c| c.unresolved_fields.len()).sum();
        let total_cost = combination_total_cost(&selected);
        let cost_key = cost_rank_key(total_cost);
        // `None` if any selection's lead time/purity is unknown --
        // `try_fold` short-circuits to `None` the moment one selection's
        // value is `None`, so this never falls back to a sentinel.
        let max_lead_time_days = selected
            .iter()
            .try_fold(0u32, |acc, c| c.offer.lead_time_days.map(|lt| acc.max(lt)));
        let min_purity = selected.iter().try_fold(f64::INFINITY, |acc, c| {
            c.offer.purity.map(|p| acc.min(p.value()))
        });
        let total_excess_mass_grams: Option<f64> =
            selected.iter().map(|c| c.quantity.excess_mass_grams).sum();
        let mut manufacturers: Vec<String> = selected
            .iter()
            .map(|c| c.offer.manufacturer.clone())
            .collect();
        manufacturers.sort();
        let mut catalog_numbers: Vec<String> = selected
            .iter()
            .map(|c| c.offer.catalog_number.clone().unwrap_or_default())
            .collect();
        catalog_numbers.sort();
        let mut offer_ids: Vec<String> = selected
            .iter()
            .map(|c| c.offer.offer_id.0.clone())
            .collect();
        offer_ids.sort();
        Self {
            indices,
            unresolved_sum,
            total_cost,
            cost_key,
            max_lead_time_days,
            min_purity,
            total_excess_mass_grams,
            manufacturers,
            catalog_numbers,
            offer_ids,
        }
    }

    /// Deterministic name-based tiebreak -- same fields/order as
    /// `HeapEntry::Ord`'s own tail.
    fn name_tiebreak(&self, other: &Self) -> std::cmp::Ordering {
        self.manufacturers
            .cmp(&other.manufacturers)
            .then_with(|| self.catalog_numbers.cmp(&other.catalog_numbers))
            .then_with(|| self.offer_ids.cmp(&other.offer_ids))
    }
}

/// `None` sorts last (worst) -- shorter is better, and an unknown lead time
/// must never be preferred over a known one just because it's unmeasured.
fn cmp_lead_time_best_first(a: Option<u32>, b: Option<u32>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// `None` sorts last (worst) -- higher purity is better, same
/// unknown-is-never-preferred rule as `cmp_lead_time_best_first`.
fn cmp_purity_best_first(a: Option<f64>, b: Option<f64>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => b.total_cmp(&a), // descending: higher purity first
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Best-first comparator for `CostFirst`/`LeadTimeFirst`/`PurityFirst`:
/// promotes the named dimension to primary, keeps `Balanced`'s remaining 3
/// dimensions (of its original 5-key chain, `unresolved_sum` excepted --
/// see below) as tiebreakers in their original relative order, then the
/// same deterministic name tiebreak `HeapEntry::Ord` uses. `unresolved_sum`
/// is always the first tiebreaker after the promoted primary, matching
/// `Balanced`'s own primary key -- a policy choosing *what's optimized*
/// should not also have to give up the "prefer more complete data" tiebreak
/// `Balanced` already establishes as the module's baseline expectation.
fn policy_compare(
    a: &PolicyEntry,
    b: &PolicyEntry,
    policy: CommercialRankingPolicy,
) -> std::cmp::Ordering {
    let unresolved = || a.unresolved_sum.cmp(&b.unresolved_sum);
    let cost = || a.cost_key.cmp(&b.cost_key);
    let lead_time = || cmp_lead_time_best_first(a.max_lead_time_days, b.max_lead_time_days);
    let purity = || cmp_purity_best_first(a.min_purity, b.min_purity);
    let names = || a.name_tiebreak(b);
    match policy {
        CommercialRankingPolicy::CostFirst => cost()
            .then_with(unresolved)
            .then_with(lead_time)
            .then_with(purity)
            .then_with(names),
        CommercialRankingPolicy::LeadTimeFirst => lead_time()
            .then_with(unresolved)
            .then_with(cost)
            .then_with(purity)
            .then_with(names),
        CommercialRankingPolicy::PurityFirst => purity()
            .then_with(unresolved)
            .then_with(cost)
            .then_with(lead_time)
            .then_with(names),
        CommercialRankingPolicy::Balanced
        | CommercialRankingPolicy::MinimumUnresolvedData
        | CommercialRankingPolicy::Pareto => {
            // `Balanced`/`MinimumUnresolvedData` are handled entirely by
            // `search_combinations` above; `Pareto` uses `dominates`, not a
            // total order. This function is never called for them.
            unreachable!("policy_compare is only called for CostFirst/LeadTimeFirst/PurityFirst")
        }
    }
}

/// `ranked_search_by_policy`/`pareto_search`'s shared enumeration step:
/// every combination in `0..total_space.min(config.max_combinations_evaluated)`,
/// decoded and scored. Capping here (rather than enumerating everything and
/// truncating after) keeps this bounded by the caller's own configured
/// budget, matching `exhaustive_search`'s guarantee.
fn evaluate_all_by_policy(
    rows: &[Vec<OfferCandidate>],
    config: &CommercialPlanningConfig,
    total_space: u64,
) -> Vec<PolicyEntry> {
    let evaluated = total_space.min(config.max_combinations_evaluated as u64);
    (0..evaluated)
        .map(|combo_index| PolicyEntry::new(decode_combo_index(combo_index, rows), rows))
        .collect()
}

/// Used for `CostFirst`/`LeadTimeFirst`/`PurityFirst`/`MinimumUnresolvedData`
/// (the last of which reuses `search_combinations` instead -- see
/// `CommercialRankingPolicy::MinimumUnresolvedData`'s doc comment; this
/// function is only reached for the first three). Ranks the capped
/// enumeration by `policy`'s comparator, filters `max_total_cost` *before*
/// truncating to `max_results_returned` -- mirroring `exhaustive_search`'s
/// own ordering, for the same reason (see `passes_max_total_cost`'s doc
/// comment).
pub(crate) fn ranked_search_by_policy(
    rows: &[Vec<OfferCandidate>],
    config: &CommercialPlanningConfig,
    max_total_cost: Option<Money>,
    policy: CommercialRankingPolicy,
) -> (Vec<Vec<usize>>, usize, u64) {
    if rows.is_empty() || rows.iter().any(|row| row.is_empty()) {
        return (Vec::new(), 0, 0);
    }
    let total_space: u64 = rows
        .iter()
        .fold(1u64, |acc, row| acc.saturating_mul(row.len() as u64));
    let mut all = evaluate_all_by_policy(rows, config, total_space);
    let evaluated = all.len();
    all.sort_by(|a, b| policy_compare(a, b, policy));
    let results = all
        .into_iter()
        .filter(|e| passes_max_total_cost(e.total_cost, max_total_cost))
        .take(config.max_results_returned)
        .map(|e| e.indices)
        .collect();
    (results, evaluated, total_space)
}

/// Pareto dominance: `a` dominates `b` iff `a` is at least as good as `b`
/// on every dimension and strictly better on at least one. Minimizes
/// cost/lead-time/excess-mass, maximizes purity. Callers must only pass
/// entries that already passed `pareto_search`'s "all 4 dimensions known,
/// same currency" filter -- every `unwrap()` here relies on that
/// precondition, and the currency check specifically is why this compares
/// raw `minor_units()` rather than the full `cost_key` (which would
/// otherwise compare mismatched currencies lexicographically instead of by
/// magnitude, silently wrong).
fn dominates(a: &PolicyEntry, b: &PolicyEntry) -> bool {
    let a_cost = a.total_cost.unwrap().minor_units();
    let b_cost = b.total_cost.unwrap().minor_units();
    let a_lead = a.max_lead_time_days.unwrap();
    let b_lead = b.max_lead_time_days.unwrap();
    let a_purity = a.min_purity.unwrap();
    let b_purity = b.min_purity.unwrap();
    let a_excess = a.total_excess_mass_grams.unwrap();
    let b_excess = b.total_excess_mass_grams.unwrap();

    let at_least_as_good =
        a_cost <= b_cost && a_lead <= b_lead && a_purity >= b_purity && a_excess <= b_excess;
    let strictly_better =
        a_cost < b_cost || a_lead < b_lead || a_purity > b_purity || a_excess < b_excess;
    at_least_as_good && strictly_better
}

/// `CommercialRankingPolicy::Pareto`. Enumerates the same capped
/// mixed-radix space as `ranked_search_by_policy`, excludes any
/// combination missing cost, lead time, purity, or excess mass, and
/// separately excludes a combination whose cost is known but priced in a
/// different currency than the rest of the evaluated set (this crate never
/// converts currencies -- comparing costs across currencies would be
/// silently wrong, so the reference currency is simply the first one seen
/// in enumeration order, which is already deterministic; anything else is
/// excluded the same way missing data is, not partially compared). The
/// non-dominated set is ordered deterministically (cheapest first, via
/// `CostFirst`'s own comparator) and truncated to `max_results_returned`.
/// Returns `(combination_indices, evaluated, total_space,
/// excluded_for_missing_data)`. `O(n^2)` in the number of evaluated
/// combinations for the dominance pass -- fine at
/// `CommercialPlanningConfig::default()`'s 10,000-combination budget;
/// revisit only if a real workload measures this as a bottleneck.
pub(crate) fn pareto_search(
    rows: &[Vec<OfferCandidate>],
    config: &CommercialPlanningConfig,
    max_total_cost: Option<Money>,
) -> (Vec<Vec<usize>>, usize, u64, usize) {
    if rows.is_empty() || rows.iter().any(|row| row.is_empty()) {
        return (Vec::new(), 0, 0, 0);
    }
    let total_space: u64 = rows
        .iter()
        .fold(1u64, |acc, row| acc.saturating_mul(row.len() as u64));
    let all = evaluate_all_by_policy(rows, config, total_space);
    let evaluated = all.len();

    let (individually_known, mut excluded): (Vec<PolicyEntry>, Vec<PolicyEntry>) =
        all.into_iter().partition(|e| {
            e.total_cost.is_some()
                && e.max_lead_time_days.is_some()
                && e.min_purity.is_some()
                && e.total_excess_mass_grams.is_some()
        });

    let reference_currency = individually_known
        .first()
        .and_then(|e| e.total_cost)
        .map(|cost| cost.currency());
    let (comparable, mut cross_currency): (Vec<PolicyEntry>, Vec<PolicyEntry>) = individually_known
        .into_iter()
        .partition(|e| e.total_cost.map(|c| c.currency()) == reference_currency);
    excluded.append(&mut cross_currency);

    let frontier: Vec<&PolicyEntry> = comparable
        .iter()
        .filter(|candidate| !comparable.iter().any(|other| dominates(other, candidate)))
        .collect();
    let mut frontier_sorted = frontier;
    frontier_sorted.sort_by(|a, b| policy_compare(a, b, CommercialRankingPolicy::CostFirst));

    let results = frontier_sorted
        .into_iter()
        .filter(|e| passes_max_total_cost(e.total_cost, max_total_cost))
        .take(config.max_results_returned)
        .map(|e| e.indices.clone())
        .collect();

    (results, evaluated, total_space, excluded.len())
}

pub(crate) fn build_combination(
    indices: &[usize],
    rows: &[Vec<OfferCandidate>],
    row_meta: &[(PrecursorId, Composition, u64, f64)],
) -> CommercialCombination {
    let selected: Vec<&OfferCandidate> = indices
        .iter()
        .enumerate()
        .map(|(row_i, &idx)| &rows[row_i][idx])
        .collect();

    let selections: Vec<CommercialOfferSelection> = indices
        .iter()
        .enumerate()
        .map(|(row_i, &idx)| {
            let candidate = &rows[row_i][idx];
            let (precursor, composition, coefficient, theoretical_mass) = &row_meta[row_i];
            let mut assumptions = Vec::new();
            if candidate
                .quantity
                .purity_adjusted_purchase_mass_grams
                .is_some()
            {
                assumptions.push(
                    "Purchase mass was adjusted using the catalog purity value. This does not \
                     establish that the unspecified impurities are inert or acceptable for the \
                     synthesis."
                        .to_string(),
                );
            }
            CommercialOfferSelection {
                precursor: precursor.clone(),
                precursor_composition: composition.clone(),
                reaction_coefficient: *coefficient,
                offer_id: candidate.offer.offer_id.clone(),
                theoretical_pure_mass_required_grams: *theoretical_mass,
                purity: candidate.offer.purity,
                purity_adjusted_purchase_mass_grams: candidate
                    .quantity
                    .purity_adjusted_purchase_mass_grams,
                package_count: candidate.quantity.package_count,
                purchased_mass_grams: candidate.quantity.purchased_mass_grams,
                excess_mass_grams: candidate.quantity.excess_mass_grams,
                subtotal: candidate.quantity.subtotal,
                unresolved_fields: candidate.unresolved_fields.clone(),
                assumptions,
                warnings: Vec::new(),
            }
        })
        .collect();

    let total_cost = combination_total_cost(&selected);
    let all_costs_known = selections.iter().all(|s| s.subtotal.is_some());
    let (mut max_lead_time, mut lead_time_known) = (0u32, true);
    for candidate in &selected {
        match candidate.offer.lead_time_days {
            Some(lt) => max_lead_time = max_lead_time.max(lt),
            None => lead_time_known = false,
        }
    }
    // `None` if any selection's purity/excess mass is unknown -- matching
    // `max_lead_time_days`'s existing convention above.
    let min_purity = selections
        .iter()
        .try_fold(f64::INFINITY, |acc, s| s.purity.map(|p| acc.min(p.value())))
        .and_then(|value| PurityFraction::new(value).ok());
    let total_excess_mass_grams: Option<f64> = selections.iter().map(|s| s.excess_mass_grams).sum();
    // "Acceptable" here is a fixed, documented judgment independent of the
    // request's own availability filter (which may not have restricted
    // availability at all): not explicitly Discontinued. Unreported
    // availability (`None`) counts as acceptable-but-unknown, matching
    // precursor.rs's existing convention that missing availability metadata
    // is a gap, not evidence of unavailability -- it must not read as
    // "unacceptable" just because a supplier didn't report a status.
    let all_availability_acceptable = selected
        .iter()
        .all(|c| c.offer.availability != Some(AvailabilityStatus::Discontinued));
    let combination_id = selected
        .iter()
        .map(|c| c.offer.offer_id.0.as_str())
        .collect::<Vec<_>>()
        .join("|");

    CommercialCombination {
        combination_id,
        selections,
        total_cost,
        all_costs_known,
        max_lead_time_days: lead_time_known.then_some(max_lead_time),
        all_availability_acceptable,
        min_purity,
        total_excess_mass_grams,
    }
}

#[cfg(test)]
mod tests {
    use super::super::assessment::assess_commercial_precursors;
    use super::super::model::*;
    use super::super::quantity::{compute_offer_quantity, unresolved_fields_for};
    use super::*;
    use crate::commercial_catalog::test_support::*;
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    /// Builds `row_lens.len()` rows, each with `row_lens[i]` offers, priced
    /// and dated so ranking has something real to discriminate on (no two
    /// offers within a row tie on every criterion). Shared by the two
    /// randomized tests below.
    fn candidate_rows_of_shape(row_lens: &[usize]) -> Vec<Vec<CommercialPrecursorOffer>> {
        row_lens
            .iter()
            .enumerate()
            .map(|(row_i, &offer_count)| {
                (0..offer_count)
                    .map(|i| {
                        let price = (row_i as u64 + 1) * 977 + (i as u64) * 131 + 1;
                        priced_offer(
                            &format!("R{row_i}-{i}"),
                            "Fe2O3",
                            "Example Materials Ltd.",
                            Some(0.9 + (i as f64) * 0.01),
                            Some(100.0),
                            Some((price, "USD")),
                            Some(5 + i as u32),
                            Some(AvailabilityStatus::InStock),
                        )
                    })
                    .collect()
            })
            .collect()
    }

    fn candidates_from(offers: &[CommercialPrecursorOffer]) -> Vec<OfferCandidate<'_>> {
        let mut candidates: Vec<OfferCandidate<'_>> = offers
            .iter()
            .map(|offer| OfferCandidate {
                offer,
                unresolved_fields: unresolved_fields_for(offer),
                quantity: compute_offer_quantity(offer, 197.335),
            })
            .collect();
        candidates.sort_by(offer_rank_order);
        candidates
    }

    proptest! {
        /// Generalizes `combination_search_matches_a_brute_force_enumeration`
        /// (below, a fixed 3x3 case) across many small randomly-shaped
        /// catalogs -- broader coverage of the same already-proven-correct
        /// exact-tier algorithm, catching indexing/off-by-one bugs a single
        /// fixed case could miss. `max_combinations_evaluated` is set
        /// comfortably above any generated total space (at most 4^4=256)
        /// so this always exercises `exhaustive_search`, never the
        /// heuristic tier.
        #[test]
        fn exhaustive_search_matches_a_brute_force_enumeration_across_random_shapes(
            row_lens in prop::collection::vec(1usize..=4, 1..=4),
        ) {
            let offer_rows = candidate_rows_of_shape(&row_lens);
            let rows: Vec<Vec<OfferCandidate>> =
                offer_rows.iter().map(|offers| candidates_from(offers)).collect();
            let total_space: u64 = rows.iter().fold(1u64, |acc, r| acc * r.len() as u64);

            let config = CommercialPlanningConfig {
                max_offers_per_precursor: 50,
                max_combinations_evaluated: 1000,
                max_results_returned: total_space as usize,
            };
            let (heap_results, evaluated, reported_total_space) =
                search_combinations(&rows, &config, None);
            prop_assert_eq!(evaluated as u64, total_space);
            prop_assert_eq!(reported_total_space, total_space);

            let mut all_combos: Vec<HeapEntry> = Vec::new();
            for combo_index in 0..total_space {
                let mut remainder = combo_index;
                let mut indices = vec![0usize; rows.len()];
                for (row_i, row) in rows.iter().enumerate() {
                    let len = row.len() as u64;
                    indices[row_i] = (remainder % len) as usize;
                    remainder /= len;
                }
                all_combos.push(HeapEntry::new(indices, &rows));
            }
            all_combos.sort_by(|a, b| b.cmp(a));
            let oracle_order: Vec<Vec<usize>> = all_combos.into_iter().map(|e| e.indices).collect();

            prop_assert_eq!(heap_results, oracle_order);
        }

        /// Structural invariant, not a specific-value assertion: for
        /// randomized *oversized* catalogs (forced into the heuristic
        /// tier), the search must never evaluate more combinations than
        /// its own configured budget, and it must never return more
        /// results than `max_results_returned` -- both bounds the whole
        /// point of the heuristic tier existing.
        #[test]
        fn heuristic_search_never_exceeds_its_evaluation_budget(
            row_lens in prop::collection::vec(5usize..=8, 2..=4),
            max_combinations_evaluated in 5usize..=50,
        ) {
            let offer_rows = candidate_rows_of_shape(&row_lens);
            let rows: Vec<Vec<OfferCandidate>> =
                offer_rows.iter().map(|offers| candidates_from(offers)).collect();
            let total_space: u64 = rows.iter().fold(1u64, |acc, r| acc * r.len() as u64);
            prop_assume!(total_space > max_combinations_evaluated as u64);

            let config = CommercialPlanningConfig {
                max_offers_per_precursor: 50,
                max_combinations_evaluated,
                max_results_returned: 5,
            };
            let (results, evaluated, reported_total_space) =
                search_combinations(&rows, &config, None);
            prop_assert_eq!(reported_total_space, total_space);
            prop_assert!(
                evaluated <= max_combinations_evaluated,
                "evaluated {evaluated} must never exceed the configured budget {max_combinations_evaluated}"
            );
            prop_assert!(results.len() <= config.max_results_returned);
        }
    }

    #[test]
    fn heuristic_search_tier_is_actually_what_this_test_exercises() {
        // A precondition check for the three tests below: if this ever
        // stops being true (e.g. someone lowers the offer count or raises
        // the default budget), those tests would silently start
        // exercising the exact tier instead and no longer cover what
        // their names say they cover.
        let plan = barium_titanate_plan();
        let catalog = large_baco3_tio2_catalog(20);
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &heuristic_tier_config(),
        )
        .unwrap();
        assert!(
            !assessment.search_budget.is_exhaustive,
            "fixture must be large enough (20x20=400) to exceed the 50-combination budget"
        );
    }

    #[test]
    fn heuristic_search_is_deterministic_across_repeated_calls() {
        let plan = barium_titanate_plan();
        let catalog = large_baco3_tio2_catalog(20);
        let config = heuristic_tier_config();
        let a = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &config,
        )
        .unwrap();
        let b = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &config,
        )
        .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn heuristic_search_ordering_is_independent_of_catalog_input_order() {
        let plan = barium_titanate_plan();
        let catalog_forward = large_baco3_tio2_catalog(20);
        let mut reversed_offers: Vec<CommercialPrecursorOffer> = catalog_forward.offers().to_vec();
        reversed_offers.reverse();
        let (catalog_reversed, _) = CommercialPrecursorCatalog::from_offers(reversed_offers);
        let config = heuristic_tier_config();

        let a = assess_commercial_precursors(
            &plan,
            &catalog_forward,
            &CommercialPlanningRequest::default(),
            &config,
        )
        .unwrap();
        let b = assess_commercial_precursors(
            &plan,
            &catalog_reversed,
            &CommercialPlanningRequest::default(),
            &config,
        )
        .unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn heuristic_search_never_returns_a_duplicate_combination() {
        let plan = barium_titanate_plan();
        let catalog = large_baco3_tio2_catalog(20);
        let assessment = assess_commercial_precursors(
            &plan,
            &catalog,
            &CommercialPlanningRequest::default(),
            &heuristic_tier_config(),
        )
        .unwrap();
        let ids: Vec<&str> = assessment
            .combinations
            .iter()
            .map(|c| c.combination_id.as_str())
            .collect();
        let unique: BTreeSet<&str> = ids.iter().copied().collect();
        assert_eq!(
            ids.len(),
            unique.len(),
            "the frontier search's visited-set dedup must prevent the same combination \
             from being emitted twice: {ids:?}"
        );
    }

    #[test]
    fn combination_search_matches_a_brute_force_enumeration() {
        // 3 rows x 3 offers = 27 combinations, small enough to enumerate
        // directly and compare against the heap search's ranking.
        fn row(prefix: &str, prices: [u64; 3]) -> Vec<CommercialPrecursorOffer> {
            (0..3)
                .map(|i| {
                    priced_offer(
                        &format!("{prefix}-{i}"),
                        "Fe2O3",
                        "Example Materials Ltd.",
                        Some(0.9 + i as f64 * 0.01),
                        Some(100.0),
                        Some((prices[i], "USD")),
                        Some(5 + i as u32),
                        Some(AvailabilityStatus::InStock),
                    )
                })
                .collect()
        }
        let row_a: Vec<CommercialPrecursorOffer> = row("A", [300, 100, 200]);
        let row_b: Vec<CommercialPrecursorOffer> = row("B", [50, 250, 150]);
        let row_c: Vec<CommercialPrecursorOffer> = row("C", [400, 350, 10]);

        fn candidates_for(offers: &[CommercialPrecursorOffer]) -> Vec<OfferCandidate<'_>> {
            let mut candidates: Vec<OfferCandidate<'_>> = offers
                .iter()
                .map(|offer| OfferCandidate {
                    offer,
                    unresolved_fields: unresolved_fields_for(offer),
                    quantity: compute_offer_quantity(offer, 197.335),
                })
                .collect();
            candidates.sort_by(offer_rank_order);
            candidates
        }
        let rows = vec![
            candidates_for(&row_a),
            candidates_for(&row_b),
            candidates_for(&row_c),
        ];

        let config = CommercialPlanningConfig {
            max_offers_per_precursor: 50,
            max_combinations_evaluated: 27,
            max_results_returned: 27,
        };
        let (heap_results, evaluated, total_space) = search_combinations(&rows, &config, None);
        assert_eq!(evaluated, 27);
        assert_eq!(total_space, 27);

        // Brute force: enumerate every (i, j, k) triple, build the same
        // HeapEntry rank key, and sort best-first with the same Ord.
        let mut all_combos: Vec<HeapEntry> = Vec::new();
        for i in 0..3 {
            for j in 0..3 {
                for k in 0..3 {
                    all_combos.push(HeapEntry::new(vec![i, j, k], &rows));
                }
            }
        }
        all_combos.sort_by(|a, b| b.cmp(a)); // descending: best (greatest) first
        let oracle_order: Vec<Vec<usize>> = all_combos.into_iter().map(|e| e.indices).collect();

        assert_eq!(
            heap_results, oracle_order,
            "the bounded heap search must visit combinations in exactly the same best-first order as a full brute-force enumeration"
        );
    }
}
