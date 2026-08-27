use crate::error::{GugenError, Result, require_finite};
use crate::frac::Frac;
use std::collections::BTreeMap;

/// Bounds for rationalizing a composition amount's `f64` into an exact
/// [`Frac`] (AGENTS.md §10: reaction balancing must be exact, not
/// float-approximate). `1e-9` is far tighter than any realistic formula
/// precision; `1_000_000` comfortably covers ordinary decimal subscripts
/// (e.g. `0.67`, `0.333`) without risking overflow in later arithmetic.
// `pub(crate)` so `error.rs`'s `AmountNotRational` message can interpolate
// the real values instead of hardcoding a second copy that could drift.
pub(crate) const MAX_RATIONAL_DENOMINATOR: i128 = 1_000_000;
pub(crate) const RATIONAL_TOLERANCE: f64 = 1e-9;

/// The 118 IUPAC element symbols, used to validate that a symbol supplied by
/// a caller is a real element rather than a typo. This is chemical-notation
/// fact, not a synthesis claim, so it is safe to hold as a static table
/// (contrast with AGENTS.md §4.1's rule against unsourced temperature/time
/// values, which does not apply here).
pub const ELEMENT_SYMBOLS: [&str; 118] = [
    "H", "He", "Li", "Be", "B", "C", "N", "O", "F", "Ne", "Na", "Mg", "Al", "Si", "P", "S", "Cl",
    "Ar", "K", "Ca", "Sc", "Ti", "V", "Cr", "Mn", "Fe", "Co", "Ni", "Cu", "Zn", "Ga", "Ge", "As",
    "Se", "Br", "Kr", "Rb", "Sr", "Y", "Zr", "Nb", "Mo", "Tc", "Ru", "Rh", "Pd", "Ag", "Cd", "In",
    "Sn", "Sb", "Te", "I", "Xe", "Cs", "Ba", "La", "Ce", "Pr", "Nd", "Pm", "Sm", "Eu", "Gd", "Tb",
    "Dy", "Ho", "Er", "Tm", "Yb", "Lu", "Hf", "Ta", "W", "Re", "Os", "Ir", "Pt", "Au", "Hg", "Tl",
    "Pb", "Bi", "Po", "At", "Rn", "Fr", "Ra", "Ac", "Th", "Pa", "U", "Np", "Pu", "Am", "Cm", "Bk",
    "Cf", "Es", "Fm", "Md", "No", "Lr", "Rf", "Db", "Sg", "Bh", "Hs", "Mt", "Ds", "Rg", "Cn", "Nh",
    "Fl", "Mc", "Lv", "Ts", "Og",
];

/// A validated element symbol. Construction is the only way to get one, so
/// every `Element` in the crate is guaranteed to be a real periodic-table
/// symbol. Matching is case-sensitive (`"Co"` cobalt vs `"CO"`, which is not
/// a valid symbol at all) — a catalog or fixture file using non-standard
/// casing will fail to parse rather than being silently reinterpreted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Element(&'static str);

impl Element {
    pub fn new(symbol: &str) -> Result<Self> {
        ELEMENT_SYMBOLS
            .iter()
            .find(|&&s| s == symbol)
            .map(|&s| Element(s))
            .ok_or_else(|| GugenError::InvalidElementSymbol(symbol.to_string()))
    }

    pub fn symbol(&self) -> &'static str {
        self.0
    }
}

impl std::fmt::Display for Element {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Element {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.0)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Element {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let symbol = String::deserialize(deserializer)?;
        Element::new(&symbol).map_err(serde::de::Error::custom)
    }
}

/// An elemental composition: element -> stoichiometric/formula amount.
/// Amounts must be finite and strictly positive; the composition must
/// contain at least one element. Iteration order is always by element
/// symbol (via `BTreeMap`), so results built from a `Composition` are
/// invariant to the order elements were supplied in (AGENTS.md §21.4).
///
/// Amounts are stored as exact rationals internally, rationalized once at
/// construction from the `f64` a caller supplies — not re-approximated on
/// every downstream use. `balance.rs` reads the exact form directly rather
/// than round-tripping back through floats (AGENTS.md §10).
/// `Eq`/`Ord` are derivable (unlike most numeric gugen types) because
/// `amounts` stores exact `Frac` values, not `f64` -- there is no
/// NaN-like non-total-order hazard here. This lets a `Composition` be used
/// as a `BTreeSet`/`BTreeMap` key directly (e.g.
/// `literature_observations.rs`'s order-invariant precursor-set identity),
/// without a caller needing a separate canonicalization step.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Composition {
    amounts: BTreeMap<Element, Frac>,
}

impl Composition {
    pub fn new(amounts: impl IntoIterator<Item = (Element, f64)>) -> Result<Self> {
        let mut map = BTreeMap::new();
        for (element, amount) in amounts {
            require_finite("composition amount", amount)?;
            if amount <= 0.0 {
                return Err(GugenError::NonPositiveAmount {
                    element: element.to_string(),
                    amount,
                });
            }
            let exact = Frac::from_f64(amount, MAX_RATIONAL_DENOMINATOR, RATIONAL_TOLERANCE)
                .ok_or_else(|| GugenError::AmountNotRational {
                    element: element.to_string(),
                    value: amount,
                })?;
            if map.insert(element, exact).is_some() {
                return Err(GugenError::DuplicateElement {
                    element: element.to_string(),
                });
            }
        }
        if map.is_empty() {
            return Err(GugenError::EmptyComposition);
        }
        Ok(Self { amounts: map })
    }

    pub fn amount_of(&self, element: Element) -> Option<f64> {
        self.amounts.get(&element).map(|f| f.to_f64())
    }

    /// Exact-rational amount, for internal callers (e.g.
    /// `transformation_grammar.rs`) that must subtract/compare element
    /// amounts without an `f64` rounding step -- `amount_of`'s `to_f64()`
    /// is fine for reporting but not for deciding whether a stoichiometric
    /// ratio predicate (e.g. "O:C exactly 3:1") holds exactly.
    #[cfg(feature = "experimental_grammar")]
    pub(crate) fn amount_of_frac(&self, element: Element) -> Option<Frac> {
        self.amounts.get(&element).copied()
    }

    pub fn elements(&self) -> impl Iterator<Item = Element> + '_ {
        self.amounts.keys().copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = (Element, f64)> + '_ {
        self.amounts.iter().map(|(&e, &a)| (e, a.to_f64()))
    }

    pub fn len(&self) -> usize {
        self.amounts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.amounts.is_empty()
    }

    /// Exact-rational accessor for `balance.rs`. Not public: callers outside
    /// the crate only ever see `f64` amounts (AGENTS.md §6 doesn't expose an
    /// exact-rational type in the public schema).
    pub(crate) fn amount_frac_of(&self, element: Element) -> Option<Frac> {
        self.amounts.get(&element).copied()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Composition {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.amounts.serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Composition {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Deserializing straight into a `BTreeMap<Element, f64>` would let
        // serde_json silently merge duplicate JSON object keys (last value
        // wins) before `Composition::new`'s duplicate check ever runs. A
        // manual visitor collects raw (possibly-duplicate) entries first so
        // duplicates are caught, not merged.
        struct CompositionVisitor;

        impl<'de> serde::de::Visitor<'de> for CompositionVisitor {
            type Value = Composition;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a map of element symbol to positive finite amount")
            }

            fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry::<Element, f64>()? {
                    entries.push(entry);
                }
                Composition::new(entries).map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_map(CompositionVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unknown_symbol() {
        assert!(Element::new("Xx").is_err());
    }

    #[test]
    fn ordinary_decimal_amounts_round_trip_exactly() {
        // 0.1 is not exactly representable in binary floating point, so
        // this exercises Frac::from_f64's continued-fraction rationalization
        // rather than a trivial power-of-two case.
        let la = Element::new("La").unwrap();
        let sr = Element::new("Sr").unwrap();
        let a = Composition::new([(la, 0.67), (sr, 0.1)]).unwrap();
        assert_eq!(a.amount_of(la), Some(0.67));
        assert_eq!(a.amount_of(sr), Some(0.1));
        // Reconstructing from the same f64 inputs must produce an equal
        // Composition -- exact-rational equality, not float comparison.
        let b = Composition::new([(sr, 0.1), (la, 0.67)]).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn symbol_matching_is_case_sensitive() {
        assert!(Element::new("Co").is_ok()); // cobalt
        assert!(Element::new("CO").is_err()); // not a symbol (would read as C + O)
        assert!(Element::new("co").is_err());
    }

    #[test]
    fn rejects_duplicate_element() {
        let ba = Element::new("Ba").unwrap();
        let err = Composition::new([(ba, 1.0), (ba, 2.0)]).unwrap_err();
        assert_eq!(
            err,
            GugenError::DuplicateElement {
                element: "Ba".to_string()
            }
        );
    }

    #[test]
    fn rejects_empty_and_non_positive_amounts() {
        assert!(Composition::new(std::iter::empty()).is_err());
        let ba = Element::new("Ba").unwrap();
        assert!(Composition::new([(ba, 0.0)]).is_err());
        assert!(Composition::new([(ba, -1.0)]).is_err());
        assert!(Composition::new([(ba, f64::NAN)]).is_err());
    }

    #[test]
    fn iteration_order_is_independent_of_insertion_order() {
        let o = Element::new("O").unwrap();
        let ba = Element::new("Ba").unwrap();
        let ti = Element::new("Ti").unwrap();
        let a = Composition::new([(ba, 1.0), (ti, 1.0), (o, 3.0)]).unwrap();
        let b = Composition::new([(o, 3.0), (ti, 1.0), (ba, 1.0)]).unwrap();
        let order_a: Vec<_> = a.elements().map(|e| e.symbol()).collect();
        let order_b: Vec<_> = b.elements().map(|e| e.symbol()).collect();
        assert_eq!(order_a, order_b);
        assert_eq!(order_a, vec!["Ba", "O", "Ti"]);
    }
}
