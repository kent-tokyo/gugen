//! Commercial offer matching: canonical, scale-invariant composition-ratio
//! bridging (`Fe2O3` matches `Fe4O6`) via exact rational arithmetic --
//! never floating-point ratio comparison, never a global `Composition::eq`
//! change. See the module root's doc comment for the full policy and its
//! single-element-allotrope exception.

use super::model::{CommercialPrecursorCatalog, CommercialPrecursorOffer};
use crate::composition::{Composition, Element};
use crate::frac::{Frac, gcd};

// The other half of this type's inherent impl (construction, loading,
// accessors) lives in `model.rs` -- split deliberately since this method's
// canonical-ratio logic belongs here. Legal Rust; see model.rs's own
// cross-reference note for why.
impl CommercialPrecursorCatalog {
    /// Matches on either literal `Composition` equality, or -- when both
    /// sides canonicalize successfully -- an equal canonical, scale-
    /// invariant element-ratio key (`Fe2O3` matches `Fe4O6`). See the
    /// module doc comment and `canonical_ratio_key`'s doc comment for what
    /// this does and does not bridge. The target's canonical key is
    /// computed once, not per offer.
    pub(crate) fn offers_matching<'a>(
        &'a self,
        composition: &'a Composition,
    ) -> impl Iterator<Item = &'a CommercialPrecursorOffer> + 'a {
        let target_canonical = canonical_ratio_key(composition);
        self.offers().iter().filter(move |o| {
            &o.composition == composition
                || (target_canonical.is_some()
                    && canonical_ratio_key(&o.composition) == target_canonical)
        })
    }
}

/// Reduces a composition's element-amount ratios to lowest integer terms
/// via exact rational (`Frac`) arithmetic -- never floating-point ratio
/// comparison. `Fe2O3` (Fe:2, O:3) and `Fe4O6` (Fe:4, O:6) both reduce to
/// `[(Fe, 2), (O, 3)]` and are therefore the same canonical key; `FeO`
/// (Fe:1, O:1) reduces to `[(Fe, 1), (O, 1)]`, a genuinely different key,
/// not merely a different scale of the same one. Iteration order matches
/// `Composition::elements()` (sorted by `Element`), so the result is
/// directly comparable with `==` and is deterministic regardless of the
/// order a caller originally supplied elements in.
///
/// Returns `None` in two cases, both making `offers_matching` fall back to
/// literal `Composition::eq` only:
///
/// - The composition has a single element. A single-element formula's atom
///   count is itself an allotrope identity (`O2` vs. `O3`, `S` vs. `S8`,
///   `P` vs. `P4` are chemically distinct substances), not a multi-element
///   compound's stoichiometric ratio, which genuinely can be rescaled
///   without changing what the formula means. `Composition` carries no
///   allotrope/structural information to tell these apart, so
///   canonicalization must not even attempt to bridge them -- doing so
///   would silently conflate exactly the kind of "different substance,
///   same reduced formula" case this policy exists to avoid (the
///   elemental analogue of the polymorph case documented above).
/// - The exact-integer reduction would overflow `i128` -- an extreme edge
///   case for any real multi-element formula (it would require many
///   elements with large, pairwise-near-coprime denominators) -- in which
///   case a canonical match can't be verified exactly, so none is claimed.
fn canonical_ratio_key(composition: &Composition) -> Option<Vec<(Element, i128)>> {
    if composition.len() <= 1 {
        return None;
    }

    let terms: Vec<(Element, Frac)> = composition
        .elements()
        .map(|element| {
            let amount = composition
                .amount_frac_of(element)
                .expect("every element yielded by Composition::elements() has an amount");
            (element, amount)
        })
        .collect();

    let mut lcm_den: i128 = 1;
    for (_, amount) in &terms {
        lcm_den = checked_lcm(lcm_den, amount.denominator())?;
    }

    let mut scaled: Vec<(Element, i128)> = Vec::with_capacity(terms.len());
    for (element, amount) in terms {
        let factor = lcm_den.checked_div(amount.denominator())?;
        let numerator = amount.numerator().checked_mul(factor)?;
        scaled.push((element, numerator));
    }

    let divisor = scaled
        .iter()
        .fold(0u128, |acc, (_, numerator)| {
            gcd(acc, numerator.unsigned_abs())
        })
        .max(1) as i128;

    Some(
        scaled
            .into_iter()
            .map(|(element, numerator)| (element, numerator / divisor))
            .collect(),
    )
}

fn checked_lcm(a: i128, b: i128) -> Option<i128> {
    let g = gcd(a.unsigned_abs(), b.unsigned_abs()).max(1) as i128;
    (a / g).checked_mul(b)
}

#[cfg(test)]
mod tests {
    use super::super::formula::parse_formula;
    use super::super::model::*;
    use super::*;
    use crate::commercial_catalog::test_support::*;

    #[test]
    fn composition_eq_itself_stays_literal_even_though_commercial_matching_does_not() {
        // Composition::eq is gugen's crate-wide equality (used by reaction
        // balancing and everywhere else) and is deliberately untouched by
        // Phase 22's canonical-ratio commercial matching policy -- it never
        // reduces. `offers_matching`'s own canonical-ratio behavior is
        // covered separately below (see the `canonical_ratio_key_*` and
        // `offers_matching_*` tests).
        let fe2o3 = parse_formula("Fe2O3").unwrap();
        let fe4o6 = parse_formula("Fe4O6").unwrap();
        assert_ne!(fe2o3, fe4o6);
    }

    #[test]
    fn canonical_ratio_key_reduces_fe2o3_and_fe4o6_to_the_same_key() {
        let fe2o3 = parse_formula("Fe2O3").unwrap();
        let fe4o6 = parse_formula("Fe4O6").unwrap();
        assert_eq!(
            canonical_ratio_key(&fe2o3).unwrap(),
            canonical_ratio_key(&fe4o6).unwrap()
        );
    }

    #[test]
    fn canonical_ratio_key_reduces_al2o3_and_al4o6_to_the_same_key() {
        let al2o3 = parse_formula("Al2O3").unwrap();
        let al4o6 = parse_formula("Al4O6").unwrap();
        assert_eq!(
            canonical_ratio_key(&al2o3).unwrap(),
            canonical_ratio_key(&al4o6).unwrap()
        );
    }

    #[test]
    fn canonical_ratio_key_matches_fractional_equivalent_compositions() {
        // La0.5Sr0.5MnO3 and La1Sr1Mn2O6 express the same element ratio at
        // different formula-unit scale, exactly like the integer Fe2O3 vs
        // Fe4O6 case -- canonicalization must handle non-integer subscripts
        // via exact rational arithmetic, not just whole-number ones.
        let half_scale = parse_formula("La0.5Sr0.5MnO3").unwrap();
        let double_scale = parse_formula("La1Sr1Mn2O6").unwrap();
        assert_eq!(
            canonical_ratio_key(&half_scale).unwrap(),
            canonical_ratio_key(&double_scale).unwrap()
        );
    }

    #[test]
    fn canonical_ratio_key_does_not_match_a_genuinely_different_ratio() {
        // FeO (1:1) vs Fe2O3 (2:3) is a different ratio, not just a
        // different scale of the same one -- canonicalization must not
        // conflate them.
        let feo = parse_formula("FeO").unwrap();
        let fe2o3 = parse_formula("Fe2O3").unwrap();
        assert_ne!(
            canonical_ratio_key(&feo).unwrap(),
            canonical_ratio_key(&fe2o3).unwrap()
        );
    }

    #[test]
    fn canonical_ratio_key_does_not_bridge_single_element_allotropes() {
        // O2 and O3 (dioxygen vs. ozone) are chemically distinct
        // substances, not the same substance at a different formula-unit
        // scale -- unlike Fe2O3/Fe4O6, a single-element atom count is an
        // allotrope identity, and Composition has no structural
        // information to distinguish allotropes any other way, so
        // canonicalization must not attempt to bridge them at all.
        let o2 = parse_formula("O2").unwrap();
        let o3 = parse_formula("O3").unwrap();
        assert!(canonical_ratio_key(&o2).is_none());
        assert!(canonical_ratio_key(&o3).is_none());

        let (catalog, _) = CommercialPrecursorCatalog::from_offers(vec![offer("A", "O3")]);
        assert!(
            catalog.offers_matching(&o2).next().is_none(),
            "O2 must not match an O3 offer via canonical-ratio bridging"
        );
    }

    #[test]
    fn canonical_ratio_key_does_not_match_hydrate_vs_anhydrous() {
        let anhydrous = parse_formula("CuSO4").unwrap();
        let hydrate = parse_formula("CuSO4\u{B7}5H2O").unwrap();
        assert_ne!(
            canonical_ratio_key(&anhydrous).unwrap(),
            canonical_ratio_key(&hydrate).unwrap()
        );
    }

    #[test]
    fn canonical_ratio_key_is_deterministic() {
        let fe2o3 = parse_formula("Fe2O3").unwrap();
        let key_a = canonical_ratio_key(&fe2o3).unwrap();
        let key_b = canonical_ratio_key(&fe2o3).unwrap();
        assert_eq!(key_a, key_b);
        // Also deterministic regardless of the order elements were supplied
        // in -- Composition::elements() already guarantees sorted
        // iteration, and canonical_ratio_key must preserve that.
        let reordered = Composition::new([(element("O"), 3.0), (element("Fe"), 2.0)]).unwrap();
        assert_eq!(canonical_ratio_key(&reordered).unwrap(), key_a);
    }

    // -- validated scalars --

    #[test]
    fn offers_matching_uses_canonical_ratio_equality() {
        // "B" is written at a different formula-unit scale (Fe4O6) than the
        // target (Fe2O3) -- same substance, same canonical ratio, so it
        // must match too, not just the literal-identical offer "A".
        let (catalog, _) = CommercialPrecursorCatalog::from_offers(vec![
            offer("A", "Fe2O3"),
            offer("B", "Fe4O6"),
            offer("C", "FeO"),
        ]);
        let target = parse_formula("Fe2O3").unwrap();
        let matches: Vec<&str> = catalog
            .offers_matching(&target)
            .map(|o| o.offer_id.0.as_str())
            .collect();
        assert_eq!(
            matches,
            vec!["A", "B"],
            "C (FeO) has a different ratio and must not match"
        );
    }

    #[test]
    fn offers_matching_preserves_the_original_formula_spelling_in_provenance() {
        // Canonical-ratio matching changes which offers are returned, not
        // what they say about themselves -- the offer's own `formula`
        // field (kept for display/diagnostics) must still read exactly as
        // the catalog supplied it, "Fe4O6", never silently rewritten to
        // match the target's "Fe2O3" spelling.
        let (catalog, _) = CommercialPrecursorCatalog::from_offers(vec![offer("B", "Fe4O6")]);
        let target = parse_formula("Fe2O3").unwrap();
        let matched = catalog.offers_matching(&target).next().unwrap();
        assert_eq!(matched.formula, "Fe4O6");
    }
}
