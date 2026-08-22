//! Stoichiometric quantity and cost math -- all checked arithmetic, never
//! panics. `theoretical_pure_mass_required_grams` is always the
//! stoichiometric theoretical requirement, computed from the plan alone;
//! see the module root doc comment for why this must never be called a
//! yield.

use super::model::{CommercialPrecursorOffer, CurrencyCode, Money};
use crate::composition::Composition;

/// Sum of atomic weights over a `Composition`'s amounts -- the one thing
/// this module needs that nothing else in the crate exposes publicly (the
/// IUPAC atomic-weight table is `pub(crate)` specifically for this reuse).
pub(crate) fn molar_mass_g_per_mol(composition: &Composition) -> f64 {
    composition
        .iter()
        .map(|(element, amount)| crate::thermodynamics::atomic_weight_amu(element) * amount)
        .sum()
}

pub(crate) fn unresolved_fields_for(offer: &CommercialPrecursorOffer) -> Vec<&'static str> {
    let mut fields = Vec::new();
    if offer.purity.is_none() {
        fields.push("purity");
    }
    if offer.package_mass.is_none() {
        fields.push("package_mass");
    }
    if offer.unit_price.is_none() {
        fields.push("unit_price");
    }
    if offer.lead_time_days.is_none() {
        fields.push("lead_time_days");
    }
    fields
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OfferQuantity {
    pub(crate) purity_adjusted_purchase_mass_grams: Option<f64>,
    pub(crate) package_count: Option<u64>,
    pub(crate) purchased_mass_grams: Option<f64>,
    pub(crate) excess_mass_grams: Option<f64>,
    pub(crate) subtotal: Option<Money>,
    pub(crate) cost_overflowed: bool,
}

pub(crate) fn compute_offer_quantity(
    offer: &CommercialPrecursorOffer,
    theoretical_pure_mass_required_grams: f64,
) -> OfferQuantity {
    let purity_adjusted_purchase_mass_grams = offer
        .purity
        .map(|p| theoretical_pure_mass_required_grams / p.value());
    let package_count = match (purity_adjusted_purchase_mass_grams, offer.package_mass) {
        (Some(mass), Some(pkg)) => Some((mass / pkg.grams()).ceil().max(0.0) as u64),
        _ => None,
    };
    let purchased_mass_grams = package_count
        .zip(offer.package_mass)
        .map(|(count, pkg)| count as f64 * pkg.grams());
    let excess_mass_grams = purchased_mass_grams
        .zip(purity_adjusted_purchase_mass_grams)
        .map(|(purchased, required)| purchased - required);
    let mut cost_overflowed = false;
    let subtotal = match (offer.unit_price, package_count) {
        (Some(price), Some(count)) => match price.checked_mul_quantity(count) {
            Some(money) => Some(money),
            None => {
                cost_overflowed = true;
                None
            }
        },
        _ => None,
    };
    OfferQuantity {
        purity_adjusted_purchase_mass_grams,
        package_count,
        purchased_mass_grams,
        excess_mass_grams,
        subtotal,
        cost_overflowed,
    }
}

/// A lexicographic, totally-ordered cost key: comparable offers/combinations
/// (known price, and -- for combinations -- one shared currency) always sort
/// before incomparable ones, then by currency code, then by amount.
///
/// This is *not* the naive "compare cost only when comparable, else Equal"
/// reading of "全価格が既知かつ同一通貨ならtotal costが低い" -- that reading
/// is not transitive (verified with a concrete counterexample during
/// implementation: A priced $200, B price-unknown, C priced $100, all
/// otherwise tied. `Equal`-on-incomparable plus a later offer_id tiebreak
/// gives A < B < C by id, but a direct A-vs-C cost comparison gives C < A --
/// contradictory, and `sort_by` cannot resolve it deterministically). A
/// fixed, total lexicographic ordering over the key tuple has no such
/// bridge-collapse cases, by construction.
pub(crate) fn cost_rank_key(subtotal: Option<Money>) -> (u8, Option<CurrencyCode>, u64) {
    match subtotal {
        Some(money) => (0, Some(money.currency()), money.minor_units()),
        None => (1, None, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commercial_catalog::test_support::*;

    #[test]
    fn package_count_does_not_round_up_when_exactly_at_a_package_boundary() {
        // purity 1.0 keeps the first division exact; 500.0/100.0 is exact
        // in IEEE754 (both operands exactly representable, evenly
        // divisible) -- this pins the expected behavior explicitly rather
        // than relying on it working by luck, and would catch a naive-but-
        // wrong implementation that pads every package count by one as a
        // safety margin.
        let offer = priced_offer(
            "A",
            "BaCO3",
            "Example Materials Ltd.",
            Some(1.0),
            Some(100.0),
            Some((1000, "USD")),
            Some(5),
            None,
        );
        let quantity = compute_offer_quantity(&offer, 500.0);
        assert_eq!(
            quantity.package_count,
            Some(5),
            "exact divisibility must not round up an extra package"
        );
    }

    #[test]
    fn package_count_rounds_up_when_just_over_a_package_boundary() {
        let offer = priced_offer(
            "A",
            "BaCO3",
            "Example Materials Ltd.",
            Some(1.0),
            Some(100.0),
            Some((1000, "USD")),
            Some(5),
            None,
        );
        let quantity = compute_offer_quantity(&offer, 500.01);
        assert_eq!(quantity.package_count, Some(6));
    }
}
