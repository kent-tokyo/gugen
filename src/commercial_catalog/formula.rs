//! Chemical formula string parsing -- syntax only. Final validation
//! (element symbols, finiteness, positivity, exact rationalization) is
//! always delegated to `Element::new`/`Composition::new`; see the module
//! root's doc comment for the delegation boundary this deliberately keeps.

use super::model::CommercialCatalogError;
use crate::composition::{Composition, Element};

//
// Grammar, explicitly and only:
//   formula   := unit+
//   unit      := element number? | '(' formula ')' number?
//   number    := digit+ ('.' digit+)?
//   hydrate   := formula '\u{B7}' number? formula
// The hydrate separator is deliberately *only* the middle dot ('\u{B7}'),
// not the ASCII '.' -- '.' is already the decimal point in `number`, and a
// formula containing both decimal subscripts and an ASCII-dot hydrate
// separator is genuinely ambiguous at the character level (e.g. "SO4.2H2O"
// parses equally validly as "O subscript 4.2" or "O subscript 4, hydrate
// multiplier 2" -- no lookahead resolves that without guessing). The
// middle dot never collides with decimal notation, so it has no such
// ambiguity.
// Anything else (variable hydrates like "xH2O", '*' as a separator,
// unbalanced parens, unknown element symbols) is a hard parse error.

// Real chemical formulas never nest parenthesized groups more than a
// handful of levels deep (even complex coordination/organometallic
// formulas rarely exceed 3-4). This bound exists purely so unbounded
// recursion on adversarial/malformed catalog input (a CSV row with
// thousands of nested parens) becomes an ordinary parse error instead of
// a stack overflow -- which aborts the whole process and cannot be caught
// by `Result` at all, confirmed empirically: ~10,000 levels of nesting
// crashed the process before this guard existed.
const MAX_FORMULA_NESTING_DEPTH: usize = 64;

struct FormulaParser<'a> {
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    source: &'a str,
    // Only decremented on `parse_group`'s success path -- an early `Err`
    // return leaves it un-decremented, which is safe *only* because every
    // `FormulaParser` is single-use: `parse_fragment` and the hydrate-
    // multiplier parser in `parse_formula` each construct one fresh
    // instance per call, function-scoped, never reused across a retry or
    // stored anywhere else. If a caller is ever added that reuses one
    // `FormulaParser` across multiple `parse_group` calls, this field
    // needs restoring on the error path too.
    depth: usize,
}

impl<'a> FormulaParser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            chars: source.char_indices().peekable(),
            source,
            depth: 0,
        }
    }

    fn remaining(&mut self) -> &'a str {
        match self.chars.peek() {
            Some(&(i, _)) => &self.source[i..],
            None => "",
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), String> {
        match self.chars.next() {
            Some((_, c)) if c == expected => Ok(()),
            Some((_, c)) => Err(format!("expected '{expected}', found '{c}'")),
            None => Err(format!("expected '{expected}', found end of input")),
        }
    }

    fn parse_number(&mut self) -> Result<Option<f64>, String> {
        let start = match self.chars.peek() {
            Some(&(i, c)) if c.is_ascii_digit() => i,
            _ => return Ok(None),
        };
        let mut end = start;
        while let Some(&(i, c)) = self.chars.peek() {
            if c.is_ascii_digit() {
                end = i + c.len_utf8();
                self.chars.next();
            } else {
                break;
            }
        }
        if let Some(&(dot_i, '.')) = self.chars.peek() {
            let mut lookahead = self.chars.clone();
            lookahead.next();
            if matches!(lookahead.peek(), Some(&(_, c)) if c.is_ascii_digit()) {
                self.chars.next();
                end = dot_i + 1;
                while let Some(&(i, c)) = self.chars.peek() {
                    if c.is_ascii_digit() {
                        end = i + c.len_utf8();
                        self.chars.next();
                    } else {
                        break;
                    }
                }
            }
        }
        self.source[start..end]
            .parse::<f64>()
            .map(Some)
            .map_err(|_| format!("invalid number '{}'", &self.source[start..end]))
    }

    fn parse_element(&mut self) -> Result<Element, String> {
        let (start, first) = self
            .chars
            .next()
            .ok_or_else(|| "expected an element symbol".to_string())?;
        if !first.is_ascii_uppercase() {
            return Err(format!(
                "expected an uppercase element symbol start, found '{first}'"
            ));
        }
        if let Some(&(i2, second)) = self.chars.peek() {
            if second.is_ascii_lowercase() {
                let end = i2 + second.len_utf8();
                let candidate = &self.source[start..end];
                if let Ok(el) = Element::new(candidate) {
                    self.chars.next();
                    return Ok(el);
                }
            }
        }
        let end = start + first.len_utf8();
        let candidate = &self.source[start..end];
        Element::new(candidate)
            .map_err(|_| format!("'{candidate}' is not a recognized element symbol"))
    }

    /// Parses one `formula` (a run of units, terminated by `)` or end of
    /// input) into its own element-amount map -- kept separate from the
    /// caller's map so a parenthesized group's multiplier can be applied to
    /// the whole group at once.
    fn parse_group(&mut self) -> Result<std::collections::BTreeMap<Element, f64>, String> {
        let mut amounts = std::collections::BTreeMap::new();
        let mut saw_unit = false;
        loop {
            match self.chars.peek().copied() {
                None | Some((_, ')')) => break,
                Some((_, c)) if c.is_ascii_uppercase() => {
                    saw_unit = true;
                    let element = self.parse_element()?;
                    let amount = self.parse_number()?.unwrap_or(1.0);
                    *amounts.entry(element).or_insert(0.0) += amount;
                }
                Some((_, '(')) => {
                    saw_unit = true;
                    self.chars.next();
                    self.depth += 1;
                    if self.depth > MAX_FORMULA_NESTING_DEPTH {
                        return Err(format!(
                            "formula nesting exceeds the maximum supported depth \
                             ({MAX_FORMULA_NESTING_DEPTH})"
                        ));
                    }
                    let inner = self.parse_group()?;
                    self.depth -= 1;
                    self.expect(')')?;
                    let multiplier = self.parse_number()?.unwrap_or(1.0);
                    for (el, amt) in inner {
                        *amounts.entry(el).or_insert(0.0) += amt * multiplier;
                    }
                }
                Some((_, c)) => return Err(format!("unexpected character '{c}'")),
            }
        }
        if !saw_unit {
            return Err("empty formula group".to_string());
        }
        Ok(amounts)
    }
}

fn parse_fragment(s: &str) -> Result<std::collections::BTreeMap<Element, f64>, String> {
    let mut parser = FormulaParser::new(s);
    let amounts = parser.parse_group()?;
    if let Some((_, c)) = parser.chars.peek().copied() {
        return Err(format!("unexpected trailing character '{c}'"));
    }
    Ok(amounts)
}

pub(crate) fn parse_formula(formula: &str) -> Result<Composition, CommercialCatalogError> {
    let wrap = |reason: String| CommercialCatalogError::FormulaParseError {
        formula: formula.to_string(),
        reason,
    };

    let trimmed = formula.trim();
    if trimmed.is_empty() {
        return Err(wrap("formula is empty".to_string()));
    }

    let sep_positions: Vec<usize> = trimmed
        .char_indices()
        .filter(|&(_, c)| c == '\u{B7}')
        .map(|(i, _)| i)
        .collect();
    if sep_positions.len() > 1 {
        return Err(wrap("more than one hydrate separator found".to_string()));
    }

    let (main_part, hydrate_part) = match sep_positions.first() {
        Some(&pos) => {
            let sep_char = trimmed[pos..].chars().next().unwrap();
            (&trimmed[..pos], Some(&trimmed[pos + sep_char.len_utf8()..]))
        }
        None => (trimmed, None),
    };

    let mut amounts = parse_fragment(main_part).map_err(wrap)?;

    if let Some(hydrate) = hydrate_part {
        let mut multiplier_parser = FormulaParser::new(hydrate);
        let multiplier = multiplier_parser
            .parse_number()
            .map_err(wrap)?
            .unwrap_or(1.0);
        let remainder = multiplier_parser.remaining();
        let hydrate_amounts = parse_fragment(remainder)
            .map_err(|reason| wrap(format!("hydrate fragment: {reason}")))?;
        for (el, amt) in hydrate_amounts {
            *amounts.entry(el).or_insert(0.0) += amt * multiplier;
        }
    }

    let pairs: Vec<(Element, f64)> = amounts.into_iter().collect();
    Composition::new(pairs).map_err(|e| wrap(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commercial_catalog::test_support::*;
    use proptest::prelude::*;

    #[test]
    fn parses_a_simple_formula() {
        assert_eq!(
            parse_formula("Fe2O3").unwrap(),
            composition(&[("Fe", 2.0), ("O", 3.0)])
        );
    }

    #[test]
    fn parses_a_parenthesized_group() {
        assert_eq!(
            parse_formula("Ca(OH)2").unwrap(),
            composition(&[("Ca", 1.0), ("O", 2.0), ("H", 2.0)])
        );
    }

    #[test]
    fn parses_decimal_subscripts() {
        assert_eq!(
            parse_formula("La0.67Sr0.33MnO3").unwrap(),
            composition(&[("La", 0.67), ("Sr", 0.33), ("Mn", 1.0), ("O", 3.0)])
        );
    }

    #[test]
    fn parses_a_hydrate_with_middle_dot_separator() {
        // CuSO4 \u{B7} 5H2O -> Cu:1, S:1, O:4+5=9, H:10
        assert_eq!(
            parse_formula("CuSO4\u{B7}5H2O").unwrap(),
            composition(&[("Cu", 1.0), ("S", 1.0), ("O", 9.0), ("H", 10.0)])
        );
    }

    #[test]
    fn a_hydrate_multiplier_of_one_may_be_omitted() {
        assert_eq!(
            parse_formula("CaSO4\u{B7}H2O").unwrap(),
            composition(&[("Ca", 1.0), ("S", 1.0), ("O", 5.0), ("H", 2.0)])
        );
    }

    #[test]
    fn anhydrous_and_hydrate_forms_are_different_compositions() {
        let anhydrous = parse_formula("CaSO4").unwrap();
        let hydrate = parse_formula("CaSO4\u{B7}2H2O").unwrap();
        assert_ne!(anhydrous, hydrate);
    }

    #[test]
    fn rejects_a_variable_hydrate() {
        assert!(parse_formula("CuSO4\u{B7}xH2O").is_err());
    }

    #[test]
    fn rejects_an_asterisk_hydrate_separator() {
        assert!(parse_formula("CuSO4*5H2O").is_err());
    }

    #[test]
    fn ascii_dot_is_consumed_as_a_decimal_point_not_a_hydrate_separator() {
        // '.' is only ever the decimal point in `number` -- "CaSO4.2H2O" is
        // parsed as O's subscript extending to "4.2", not as a hydrate
        // separator (see the module's grammar comment). This is a real,
        // syntactically valid formula, just not the hydrate the ASCII dot
        // might suggest -- it does not equal the middle-dot hydrate form.
        let ascii_dot = parse_formula("CaSO4.2H2O").unwrap();
        let hydrate = parse_formula("CaSO4\u{B7}2H2O").unwrap();
        assert_ne!(ascii_dot, hydrate);
        assert_eq!(
            ascii_dot,
            composition(&[("Ca", 1.0), ("S", 1.0), ("O", 5.2), ("H", 2.0)])
        );
    }

    #[test]
    fn rejects_an_out_of_grammar_string() {
        assert!(parse_formula("not a formula!").is_err());
    }

    #[test]
    fn rejects_an_unbalanced_paren() {
        assert!(parse_formula("Ca(OH2").is_err());
        assert!(parse_formula("CaOH)2").is_err());
    }

    #[test]
    fn rejects_an_unknown_element_symbol() {
        assert!(parse_formula("Xx2O3").is_err());
    }

    #[test]
    fn parses_nested_parenthesized_groups() {
        // A reasonable, real-world nesting depth (a coordination-compound
        // formula might genuinely need two levels) must parse correctly,
        // with the multiplier of an outer group applying to everything
        // the inner group already expanded.
        assert_eq!(
            parse_formula("Ca((OH)2)3").unwrap(),
            composition(&[("Ca", 1.0), ("O", 6.0), ("H", 6.0)])
        );
    }

    #[test]
    fn rejects_formula_nesting_beyond_the_supported_depth_without_crashing() {
        // Regression test: unbounded recursion on deeply nested parens
        // used to stack-overflow the whole process (confirmed empirically
        // at ~10,000 levels during implementation) -- a crash that cannot
        // be caught by `Result` at all, which matters because this input
        // can come from an untrusted CSV file. It must now be an ordinary
        // parse error.
        let formula = format!("{}Fe{}", "(".repeat(10_000), ")".repeat(10_000));
        assert!(parse_formula(&formula).is_err());
    }

    #[test]
    fn rejects_a_zero_subscript() {
        // A zero-amount element is nonsensical in a formula and is
        // rejected by Composition::new's positive-amount check, not
        // silently dropped or treated as "element absent".
        assert!(parse_formula("Fe0O3").is_err());
    }

    #[test]
    fn rejects_a_negative_or_signed_subscript() {
        // The grammar has no sign production -- a `-` or `+` where a
        // subscript or element symbol is expected is simply an
        // unrecognized character, not a parsed negative amount.
        assert!(parse_formula("Fe-2O3").is_err());
        assert!(parse_formula("Fe+2O3").is_err());
    }

    #[test]
    fn rejects_trailing_garbage_after_an_otherwise_valid_formula() {
        assert!(parse_formula("Fe2O3$").is_err());
        assert!(parse_formula("Fe2O3 extra text").is_err());
    }

    #[test]
    fn sums_an_element_appearing_both_inside_and_outside_a_group() {
        // Fe appears once as a bare unit and again inside a parenthesized
        // group -- correct formula parsing sums the total atom count
        // across the whole formula; this is fragment-merging the parser
        // must do to produce the flat map Composition::new expects, not a
        // caller-contract "accidental duplicate key" error (see the
        // module doc comment on the formula-parser/Composition::new
        // responsibility boundary).
        assert_eq!(
            parse_formula("Fe(Fe)2O3").unwrap(),
            composition(&[("Fe", 3.0), ("O", 3.0)])
        );
    }

    #[test]
    fn leading_and_trailing_whitespace_is_trimmed_but_internal_whitespace_is_rejected() {
        assert_eq!(
            parse_formula("  Fe2O3  ").unwrap(),
            parse_formula("Fe2O3").unwrap()
        );
        assert!(parse_formula("Fe2 O3").is_err());
    }

    #[test]
    fn rejects_a_cyrillic_lookalike_element_symbol() {
        // Cyrillic "е" (U+0435) looks identical to Latin "e" at a glance,
        // but Element::new matches against ELEMENT_SYMBOLS by exact ASCII
        // string equality, so a formula using it is rejected as an
        // unrecognized character, not silently reinterpreted as Latin.
        assert!(parse_formula("F\u{0435}2O3").is_err());
    }

    #[test]
    fn rejects_a_unicode_lookalike_hydrate_separator() {
        // U+2022 BULLET and U+00B7 MIDDLE DOT look similar but are
        // different code points -- only the exact U+00B7 is recognized as
        // a hydrate separator; a bullet is just an unrecognized character.
        assert!(parse_formula("CuSO4\u{2022}5H2O").is_err());
    }

    const ELEMENT_POOL: &[&str] = &[
        "H", "He", "Li", "C", "N", "O", "F", "Na", "Mg", "Al", "Si", "S", "Cl", "K", "Ca", "Fe",
        "Cu", "Zn", "Ba", "Ti", "La", "Sr", "Mn",
    ];

    proptest! {
        /// Rendering a randomly-generated Composition to a formula string
        /// (element+amount concatenation, no parens/hydrate needed to
        /// exercise this property) and parsing it back must reconstruct an
        /// equal Composition. Amounts are computed as `n / 100.0` for
        /// integer `n` -- Rust's f64 Display/FromStr round-trip exactly
        /// (shortest-string-that-reparses-identically), so the rendered
        /// string reparses to the *same f64 bit pattern* fed into the
        /// original, making both sides call `Frac::from_f64` on identical
        /// input -- this property is robust by construction, not just
        /// "usually passes" (mirrors composition.rs's own
        /// `ordinary_decimal_amounts_round_trip_exactly` mechanism).
        #[test]
        fn round_trips_through_render_and_parse(
            pairs in prop::collection::hash_map(
                prop::sample::select(ELEMENT_POOL),
                1u32..=9999u32,
                1..=6,
            )
        ) {
            let composition_pairs: Vec<(&str, f64)> = pairs
                .iter()
                .map(|(&sym, &n)| (sym, n as f64 / 100.0))
                .collect();
            let original = composition(&composition_pairs);
            let rendered: String = composition_pairs
                .iter()
                .map(|(sym, amt)| format!("{sym}{amt}"))
                .collect();
            let parsed = parse_formula(&rendered).unwrap();
            prop_assert_eq!(parsed, original);
        }

        /// The parser must never panic on arbitrary input -- it always
        /// returns a `Result`. This module already found one real
        /// stack-overflow bug by hand (unbounded paren-nesting recursion);
        /// a broad fuzz-style sweep over arbitrary Unicode strings covers
        /// digit-run, malformed-fragment, and character-mix variations the
        /// hand-written edge-case tests don't enumerate.
        #[test]
        fn never_panics_on_arbitrary_input(chars in prop::collection::vec(any::<char>(), 0..200)) {
            let s: String = chars.into_iter().collect();
            let _ = parse_formula(&s);
        }

        /// Same no-panic property, but biased specifically toward deep
        /// paren nesting at randomized depths -- the hand-written
        /// regression test only pins one fixed depth (10,000); this
        /// exercises the MAX_FORMULA_NESTING_DEPTH guard's correctness
        /// across the whole range around and beyond the boundary.
        #[test]
        fn never_panics_on_randomized_nesting_depth(depth in 0usize..5_000) {
            let formula = format!("{}Fe{}", "(".repeat(depth), ")".repeat(depth));
            let _ = parse_formula(&formula);
        }
    }
}
