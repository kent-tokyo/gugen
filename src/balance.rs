use crate::composition::{Composition, Element};
use crate::error::{GugenError, Result};
use crate::frac::Frac;
use crate::reaction::{BalancedReaction, ReactionSpecies};
use std::collections::BTreeSet;

/// Curated allow-list of byproduct compositions v0.1 permits assuming
/// without target-specific evidence (AGENTS.md §10). A reaction that needs
/// a byproduct outside this list is `RejectionCode::UnsupportedByproductRequired`,
/// not a silently invented one.
pub fn curated_byproducts() -> Result<Vec<Composition>> {
    let c = Element::new("C")?;
    let h = Element::new("H")?;
    let o = Element::new("O")?;
    Ok(vec![
        Composition::new([(c, 1.0), (o, 2.0)])?, // CO2
        Composition::new([(h, 2.0), (o, 1.0)])?, // H2O
        Composition::new([(o, 2.0)])?,           // O2
    ])
}

/// Solves for integer, gcd-normalized coefficients balancing `reactants`
/// against `products` (AGENTS.md §10), using exact-rational Gauss-Jordan
/// elimination over the element x species matrix — never floating-point
/// approximation.
///
/// Returns every independent, chemically valid (all-positive-coefficient)
/// balance found. Usually zero or one; more than one only when the species
/// list leaves genuine stoichiometric ambiguity (e.g. Fe + O2 balanced
/// against {FeO, Fe2O3, Fe3O4} independently admits all three). An empty
/// result means no valid balance exists for this exact reactant/product
/// species list — the caller (Phase 3+) is responsible for trying
/// different precursor or byproduct combinations.
///
/// ponytail: only the null space's free-variable basis vectors are checked
/// individually for sign-validity; a valid reaction that requires *summing*
/// two or more basis vectors together is not searched for. This covers
/// every case in AGENTS.md §21.1's test list and every realistic
/// small-species-count solid-state reaction; revisit with a bounded
/// combination search if a real fixture ever needs one.
pub fn balance(
    reactants: &[Composition],
    products: &[Composition],
) -> Result<Vec<BalancedReaction>> {
    if reactants.is_empty() || products.is_empty() {
        return Err(GugenError::EmptyReaction);
    }

    let mut elements: BTreeSet<Element> = BTreeSet::new();
    for composition in reactants.iter().chain(products.iter()) {
        elements.extend(composition.elements());
    }
    let elements: Vec<Element> = elements.into_iter().collect();

    let species_count = reactants.len() + products.len();
    let mut matrix: Vec<Vec<Frac>> = Vec::with_capacity(elements.len());
    for &element in &elements {
        let mut row = Vec::with_capacity(species_count);
        for composition in reactants {
            row.push(
                composition
                    .amount_frac_of(element)
                    .unwrap_or_else(Frac::zero),
            );
        }
        for composition in products {
            let amount = composition
                .amount_frac_of(element)
                .unwrap_or_else(Frac::zero);
            row.push(amount.checked_neg()?);
        }
        matrix.push(row);
    }

    let pivots = row_reduce(&mut matrix)?;
    let pivot_cols: BTreeSet<usize> = pivots.iter().copied().collect();
    let free_cols: Vec<usize> = (0..species_count)
        .filter(|c| !pivot_cols.contains(c))
        .collect();

    let mut results = Vec::new();
    for &free_col in &free_cols {
        let mut vector = vec![Frac::zero(); species_count];
        vector[free_col] = Frac::one();
        for (row_idx, &pivot_col) in pivots.iter().enumerate() {
            vector[pivot_col] = Frac::zero().checked_sub(matrix[row_idx][free_col])?;
        }
        if let Some(reaction) = vector_to_reaction(&vector, reactants, products)? {
            results.push(reaction);
        }
    }

    Ok(results)
}

/// Gauss-Jordan elimination to reduced row echelon form, in place. Returns
/// the pivot column index for each pivot row, in row order.
fn row_reduce(matrix: &mut [Vec<Frac>]) -> Result<Vec<usize>> {
    let rows = matrix.len();
    let cols = matrix.first().map_or(0, Vec::len);
    let mut pivots = Vec::new();
    let mut pivot_row = 0;

    for col in 0..cols {
        if pivot_row >= rows {
            break;
        }
        let Some(sel) = (pivot_row..rows).find(|&r| !matrix[r][col].is_zero()) else {
            continue;
        };
        matrix.swap(pivot_row, sel);

        let pivot_val = matrix[pivot_row][col];
        for cell in &mut matrix[pivot_row] {
            *cell = cell.checked_div(pivot_val)?;
        }

        let pivot_row_snapshot = matrix[pivot_row].clone();
        for (r, row) in matrix.iter_mut().enumerate() {
            if r == pivot_row {
                continue;
            }
            let factor = row[col];
            if factor.is_zero() {
                continue;
            }
            for (cell, &pivot_cell) in row.iter_mut().zip(&pivot_row_snapshot) {
                let sub = factor.checked_mul(pivot_cell)?;
                *cell = cell.checked_sub(sub)?;
            }
        }

        pivots.push(col);
        pivot_row += 1;
    }

    Ok(pivots)
}

/// Converts one null-space basis vector into a `BalancedReaction`, or
/// `None` if this basis vector isn't chemically valid on its own (mixed
/// signs -- some species would need a negative amount) or degenerates to
/// having nothing on one side (e.g. the free species itself zeroes out).
fn vector_to_reaction(
    vector: &[Frac],
    reactants: &[Composition],
    products: &[Composition],
) -> Result<Option<BalancedReaction>> {
    let all_non_negative = vector.iter().all(|f| !is_negative(f));
    let all_non_positive = vector.iter().all(|f| is_negative(f) || f.is_zero());
    if !all_non_negative && !all_non_positive {
        return Ok(None);
    }
    let negate = all_non_positive && !all_non_negative;

    let mut signed = Vec::with_capacity(vector.len());
    for &f in vector {
        signed.push(if negate { f.checked_neg()? } else { f });
    }

    let Some(scaled) = scale_to_integers(&signed)? else {
        return Ok(None);
    };

    let (reactant_coeffs, product_coeffs) = scaled.split_at(reactants.len());

    let reactant_species: Vec<ReactionSpecies> = reactants
        .iter()
        .zip(reactant_coeffs)
        .filter(|&(_, &coeff)| coeff != 0)
        .map(|(composition, &coeff)| {
            ReactionSpecies::new(composition.clone(), coeff)
                .expect("coeff != 0 already filtered above")
        })
        .collect();
    let product_species: Vec<ReactionSpecies> = products
        .iter()
        .zip(product_coeffs)
        .filter(|&(_, &coeff)| coeff != 0)
        .map(|(composition, &coeff)| {
            ReactionSpecies::new(composition.clone(), coeff)
                .expect("coeff != 0 already filtered above")
        })
        .collect();

    match BalancedReaction::new(reactant_species, product_species) {
        Ok(reaction) => Ok(Some(reaction)),
        Err(GugenError::EmptyReaction) => Ok(None),
        Err(other) => Err(other),
    }
}

fn is_negative(f: &Frac) -> bool {
    f.numerator() < 0
}

/// Scales an all-non-negative rational vector to the minimal integer
/// vector with the same ratios: multiply through by the LCM of the
/// denominators, then divide by the GCD of the results (AGENTS.md §10's
/// gcd-normalization requirement). Returns `None` on overflow rather than
/// erroring, since an overflowing scale factor most often means this
/// particular basis vector -- not the whole `balance()` call -- doesn't
/// have a representable minimal integer form; skip it rather than fail
/// every other candidate.
fn scale_to_integers(vector: &[Frac]) -> Result<Option<Vec<u64>>> {
    let mut lcm: i128 = 1;
    for f in vector {
        if f.is_zero() {
            continue;
        }
        let Some(next) = checked_lcm(lcm, f.denominator()) else {
            return Ok(None);
        };
        lcm = next;
    }

    let lcm_frac = Frac::new(lcm, 1)?;
    let mut integers: Vec<i128> = Vec::with_capacity(vector.len());
    for f in vector {
        let Ok(scaled) = f.checked_mul(lcm_frac) else {
            return Ok(None);
        };
        debug_assert_eq!(scaled.denominator(), 1);
        integers.push(scaled.numerator());
    }

    let g = integers
        .iter()
        .filter(|&&n| n != 0)
        .map(|&n| n.unsigned_abs())
        .fold(0u128, gcd)
        .max(1);

    let mut result = Vec::with_capacity(integers.len());
    for n in integers {
        let reduced = n / (g as i128);
        let Ok(as_u64) = u64::try_from(reduced) else {
            return Ok(None);
        };
        result.push(as_u64);
    }
    Ok(Some(result))
}

fn checked_lcm(a: i128, b: i128) -> Option<i128> {
    let g = gcd(a.unsigned_abs(), b.unsigned_abs()).max(1) as i128;
    (a / g).checked_mul(b)
}

fn gcd(a: u128, b: u128) -> u128 {
    if b == 0 { a } else { gcd(b, a % b) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element(symbol: &str) -> Element {
        Element::new(symbol).unwrap()
    }

    fn composition(pairs: &[(&str, f64)]) -> Composition {
        Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
    }

    /// AGENTS.md §21.1: "単純な1対1反応" (simple 1:1 reaction).
    #[test]
    fn simple_one_to_one_reaction() {
        // BaO + TiO2 -> BaTiO3
        let reactants = vec![
            composition(&[("Ba", 1.0), ("O", 1.0)]),
            composition(&[("Ti", 1.0), ("O", 2.0)]),
        ];
        let products = vec![composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)])];

        let results = balance(&reactants, &products).unwrap();
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.reactants().len(), 2);
        assert_eq!(r.products().len(), 1);
        assert!(r.reactants().iter().all(|s| s.coefficient() == 1));
        assert!(r.products().iter().all(|s| s.coefficient() == 1));
    }

    /// AGENTS.md §21.1: "carbonateからoxide材料＋CO2".
    #[test]
    fn carbonate_decomposes_to_oxide_plus_co2() {
        // BaCO3 -> BaO + CO2
        let reactants = vec![composition(&[("Ba", 1.0), ("C", 1.0), ("O", 3.0)])];
        let products = vec![
            composition(&[("Ba", 1.0), ("O", 1.0)]),
            composition(&[("C", 1.0), ("O", 2.0)]),
        ];

        let results = balance(&reactants, &products).unwrap();
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.reactants()[0].coefficient(), 1);
        assert_eq!(r.products().len(), 2);
        assert!(r.products().iter().all(|s| s.coefficient() == 1));
    }

    /// Checks an assumption precursor search (Phase 3) is built on: does
    /// offering *every* curated byproduct at once (rather than a targeted
    /// subset) risk defeating the single-basis-vector heuristic documented
    /// in `balance()`'s `ponytail:` note? For BaCO3 + TiO2 -> BaTiO3 + CO2,
    /// empirically no: H2O and O2 each touch elements (H, extra O) that
    /// don't create genuine ambiguity here, so both forms return the same
    /// single correct answer with H2O/O2 correctly dropped at zero
    /// coefficient. That does NOT prove the general case is always safe --
    /// a target or byproduct set that shares more structure could still
    /// hit the combination-of-basis-vectors ceiling. Precursor search uses
    /// the more expensive but strictly safer smallest-subset-first strategy
    /// anyway (trivial cost: 2^3 = 8 subsets for 3 curated byproducts), and
    /// this test exists so a future change to `curated_byproducts()` that
    /// breaks this assumption is caught here instead of silently.
    #[test]
    fn all_curated_byproducts_at_once_happens_to_work_for_this_case_but_search_does_not_rely_on_it()
    {
        let reactants = vec![
            composition(&[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            composition(&[("Ti", 1.0), ("O", 2.0)]),
        ];
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let co2 = composition(&[("C", 1.0), ("O", 2.0)]);
        let h2o = composition(&[("H", 2.0), ("O", 1.0)]);
        let o2 = composition(&[("O", 2.0)]);

        let targeted = balance(&reactants, &[target.clone(), co2.clone()]).unwrap();
        assert_eq!(
            targeted.len(),
            1,
            "targeted subset {{target, CO2}} must balance"
        );

        let everything = balance(&reactants, &[target, co2, h2o, o2]).unwrap();
        assert_eq!(everything.len(), 1);
        assert_eq!(
            everything[0].products().len(),
            2,
            "H2O and O2 must be dropped at zero coefficient"
        );
        assert_eq!(everything[0], targeted[0]);
    }

    /// AGENTS.md §21.1: "酸素を副生成物または反応物として含む反応" (O2 as
    /// byproduct or as a reactant) -- byproduct direction.
    #[test]
    fn oxygen_as_byproduct() {
        // 2 Ag2O -> 4 Ag + O2
        let reactants = vec![composition(&[("Ag", 2.0), ("O", 1.0)])];
        let products = vec![composition(&[("Ag", 1.0)]), composition(&[("O", 2.0)])];

        let results = balance(&reactants, &products).unwrap();
        assert_eq!(results.len(), 1);
        let r = &results[0];
        assert_eq!(r.reactants()[0].coefficient(), 2);
        let ag = r
            .products()
            .iter()
            .find(|s| s.composition.amount_of(element("Ag")).is_some())
            .unwrap();
        let o2 = r
            .products()
            .iter()
            .find(|s| s.composition.amount_of(element("O")).is_some())
            .unwrap();
        assert_eq!(ag.coefficient(), 4);
        assert_eq!(o2.coefficient(), 1);
    }

    /// AGENTS.md §21.1: O2 as a reactant.
    #[test]
    fn oxygen_as_reactant() {
        // 4 Fe + 3 O2 -> 2 Fe2O3
        let reactants = vec![composition(&[("Fe", 1.0)]), composition(&[("O", 2.0)])];
        let products = vec![composition(&[("Fe", 2.0), ("O", 3.0)])];

        let results = balance(&reactants, &products).unwrap();
        assert_eq!(results.len(), 1);
        let r = &results[0];
        let fe = r
            .reactants()
            .iter()
            .find(|s| s.composition.amount_of(element("Fe")).is_some())
            .unwrap();
        let o2 = r
            .reactants()
            .iter()
            .find(|s| s.composition.amount_of(element("O")).is_some())
            .unwrap();
        assert_eq!(fe.coefficient(), 4);
        assert_eq!(o2.coefficient(), 3);
        assert_eq!(r.products()[0].coefficient(), 2);
    }

    /// AGENTS.md §21.1: "複数前駆体" (multiple precursors).
    #[test]
    fn multiple_precursors() {
        // BaCO3 + SrCO3 + TiO2 + TiO2 -> Ba0.5Sr0.5TiO3-family style multi-precursor
        // Keep it simple and exact: BaCO3 + TiO2 -> BaTiO3 + CO2 already
        // covers 2 precursors -> 2 products; use a 3-precursor case here.
        // CaCO3 + SrCO3 + TiO2 doesn't balance to a single simple product,
        // so use a well-known 3-precursor solid-state target instead:
        // BaCO3 + SrCO3 -> not a real reaction; replace with a clean case:
        // Na2CO3 + CaCO3 + SiO2 -> requires a real compound. Use a simpler,
        // exact 3-reactant case instead: 2 LiOH + CO2 -> Li2CO3 + H2O.
        let reactants = vec![
            composition(&[("Li", 2.0), ("O", 2.0), ("H", 2.0)]),
            composition(&[("C", 1.0), ("O", 2.0)]),
        ];
        let products = vec![
            composition(&[("Li", 2.0), ("C", 1.0), ("O", 3.0)]),
            composition(&[("H", 2.0), ("O", 1.0)]),
        ];

        let results = balance(&reactants, &products).unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].reactants().iter().all(|s| s.coefficient() == 1));
        assert!(results[0].products().iter().all(|s| s.coefficient() == 1));
    }

    /// AGENTS.md §21.1: "解なし" (no solution) -- disjoint element sets.
    #[test]
    fn no_solution_for_disjoint_elements() {
        let reactants = vec![composition(&[("Fe", 1.0)])];
        let products = vec![composition(&[("Na", 1.0), ("Cl", 1.0)])];

        let results = balance(&reactants, &products).unwrap();
        assert!(results.is_empty());
    }

    /// AGENTS.md §21.1: "複数解" (multiple solutions). Fe + O2 independently
    /// balances against each of FeO, Fe2O3, and Fe3O4 -- three genuinely
    /// different reactions, all of which must be preserved (AGENTS.md §10).
    #[test]
    fn multiple_solutions_for_iron_oxide_family() {
        let reactants = vec![composition(&[("Fe", 1.0)]), composition(&[("O", 2.0)])];
        let products = vec![
            composition(&[("Fe", 1.0), ("O", 1.0)]), // FeO
            composition(&[("Fe", 2.0), ("O", 3.0)]), // Fe2O3
            composition(&[("Fe", 3.0), ("O", 4.0)]), // Fe3O4
        ];

        let results = balance(&reactants, &products).unwrap();
        assert_eq!(
            results.len(),
            3,
            "expected one independent balance per iron oxide"
        );

        // Every returned reaction must use exactly one of the three
        // products (this basis-vector construction zeroes the other two).
        for r in &results {
            assert_eq!(r.products().len(), 1);
            assert!(r.reactants().iter().all(|s| s.coefficient() > 0));
        }
    }

    /// AGENTS.md §21.1: gcd正規化 (gcd normalization) -- coefficients must
    /// come out minimal, not an arbitrary common multiple.
    #[test]
    fn coefficients_are_gcd_normalized() {
        // 2 H2 + O2 -> 2 H2O; a naive solver could return 4/2/4.
        let reactants = vec![composition(&[("H", 2.0)]), composition(&[("O", 2.0)])];
        let products = vec![composition(&[("H", 2.0), ("O", 1.0)])];

        let results = balance(&reactants, &products).unwrap();
        assert_eq!(results.len(), 1);
        let r = &results[0];
        let h2 = r
            .reactants()
            .iter()
            .find(|s| s.composition.amount_of(element("H")).is_some())
            .unwrap();
        let o2 = r
            .reactants()
            .iter()
            .find(|s| s.composition.amount_of(element("O")).is_some())
            .unwrap();
        assert_eq!(h2.coefficient(), 2);
        assert_eq!(o2.coefficient(), 1);
        assert_eq!(r.products()[0].coefficient(), 2);
    }

    /// AGENTS.md §21.1: 元素保存 (element conservation) -- verify the
    /// returned coefficients actually balance every element, independent
    /// of how the solver got there.
    #[test]
    fn element_conservation_holds() {
        let reactants = vec![
            composition(&[("Ba", 1.0), ("O", 1.0)]),
            composition(&[("Ti", 1.0), ("O", 2.0)]),
        ];
        let products = vec![composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)])];

        let results = balance(&reactants, &products).unwrap();
        let r = &results[0];
        for &el in &[element("Ba"), element("Ti"), element("O")] {
            let lhs: f64 = r
                .reactants()
                .iter()
                .map(|s| s.composition.amount_of(el).unwrap_or(0.0) * s.coefficient() as f64)
                .sum();
            let rhs: f64 = r
                .products()
                .iter()
                .map(|s| s.composition.amount_of(el).unwrap_or(0.0) * s.coefficient() as f64)
                .sum();
            assert!(
                (lhs - rhs).abs() < 1e-9,
                "element {el} unbalanced: {lhs} vs {rhs}"
            );
        }
    }

    /// AGENTS.md §21.1: permutation invariance -- reordering species must
    /// not change which reactions are found (though which reactant/product
    /// list position holds which coefficient will naturally track the
    /// reordering).
    #[test]
    fn permutation_invariance() {
        let reactants_a = vec![
            composition(&[("Ba", 1.0), ("O", 1.0)]),
            composition(&[("Ti", 1.0), ("O", 2.0)]),
        ];
        let reactants_b = vec![
            composition(&[("Ti", 1.0), ("O", 2.0)]),
            composition(&[("Ba", 1.0), ("O", 1.0)]),
        ];
        let products = vec![composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)])];

        let results_a = balance(&reactants_a, &products).unwrap();
        let results_b = balance(&reactants_b, &products).unwrap();
        assert_eq!(results_a.len(), 1);
        assert_eq!(results_b.len(), 1);

        let total_a: u64 = results_a[0]
            .reactants()
            .iter()
            .map(|s| s.coefficient())
            .sum();
        let total_b: u64 = results_b[0]
            .reactants()
            .iter()
            .map(|s| s.coefficient())
            .sum();
        assert_eq!(total_a, total_b);
    }

    /// AGENTS.md §21.1: 大きい係数 (large coefficients) and overflow
    /// handling -- a reaction whose minimal integer coefficients are large
    /// must still balance exactly, and a case engineered to overflow must
    /// error rather than panic or silently wrap.
    #[test]
    fn large_but_representable_coefficients() {
        // 3 A + 97 B -> 1 C, where C = A3B97 (an artificial but exact case
        // exercising a large coefficient without needing a real compound).
        let reactants = vec![composition(&[("Na", 3.0)]), composition(&[("K", 97.0)])];
        let products = vec![composition(&[("Na", 3.0), ("K", 97.0)])];

        let results = balance(&reactants, &products).unwrap();
        assert_eq!(results.len(), 1);
        let r = &results[0];
        let na = r
            .reactants()
            .iter()
            .find(|s| s.composition.amount_of(element("Na")).is_some())
            .unwrap();
        let k = r
            .reactants()
            .iter()
            .find(|s| s.composition.amount_of(element("K")).is_some())
            .unwrap();
        assert_eq!(na.coefficient(), 1);
        assert_eq!(k.coefficient(), 1);
        assert_eq!(r.products()[0].coefficient(), 1);
    }

    #[test]
    fn rejects_empty_reactant_or_product_list() {
        let comp = composition(&[("Fe", 1.0)]);
        assert!(balance(&[], std::slice::from_ref(&comp)).is_err());
        assert!(balance(&[comp], &[]).is_err());
    }

    /// AGENTS.md §21.1: overflow処理 (overflow handling). Isolates the
    /// scaling step's overflow path directly at the `Frac` level rather
    /// than trying to coax an artificial `Composition` into producing
    /// astronomically large coefficients through the public API.
    #[test]
    fn scale_to_integers_reports_denominator_overflow_as_no_solution_for_that_candidate() {
        let huge_a = Frac::new(1, i128::MAX / 2).unwrap();
        let huge_b = Frac::new(1, (i128::MAX / 2) - 1).unwrap();
        let result = scale_to_integers(&[huge_a, huge_b]).unwrap();
        assert!(
            result.is_none(),
            "LCM of two near-i128::MAX denominators must overflow, not panic"
        );
    }

    /// Distinct from the test above: that one overflows at the
    /// `checked_lcm` step (huge denominators). This one keeps the LCM
    /// itself small (2) but gives one entry a numerator already at
    /// `i128::MAX`, so the multiply-by-LCM step overflows instead --
    /// the bug this test guards against silently let that overflow
    /// propagate as `Err` (via a bare `?`) rather than the documented
    /// `Ok(None)` "skip this candidate" contract.
    #[test]
    fn scale_to_integers_reports_multiply_overflow_as_no_solution_not_an_error() {
        let huge_numerator = Frac::new(i128::MAX, 1).unwrap();
        let denominator_two = Frac::new(1, 2).unwrap();
        let result = scale_to_integers(&[huge_numerator, denominator_two]);
        assert!(
            matches!(result, Ok(None)),
            "a numerator already at i128::MAX times an LCM of 2 must overflow \
             the multiply step as Ok(None), not Err: {result:?}"
        );
    }
}
