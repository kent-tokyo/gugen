//! Phase 22: connects gugen's chemical planning output to real purchasable
//! products, without ever feeding commercial data back into scientific
//! planning. Two strictly separate stages: chemical planning (existing
//! `Planner`/`score_plan`) produces a `SynthesisPlan`; this module then
//! matches that plan's precursor compositions against a caller-supplied
//! catalog of commercial offers (price, purity, package size, supplier).
//!
//! **Never mutates `SynthesisPlan`.** `assess_commercial_precursors`/
//! `assess_commercial_plans` take `&SynthesisPlan` and return a wholly
//! separate `CommercialPlanAssessment` -- score, confidence, the balanced
//! reaction, and process steps are structurally unreachable from this
//! module (mirrors `literature_evidence.rs`'s "reference-only, by
//! construction" boundary: this is a separate return type from a separate
//! function, never threaded into `score_plan`'s signature or as a new
//! `SynthesisPlan` field).
//!
//! **No network access anywhere in this module.** Product URLs are stored
//! strings, never fetched. Catalogs are loaded from caller-supplied CSV/JSON
//! text only.
//!
//! **Commercial offer matching uses a canonical, scale-invariant
//! composition ratio -- not literal `Composition::eq`.** Two formulas
//! written at different formula-unit scale (`Fe2O3` vs `Fe4O6`) are the
//! same substance and match here, via exact-rational GCD reduction of the
//! whole element-ratio vector (never floating-point ratio comparison --
//! see `canonical_ratio_key`). This is scale-invariant canonicalization of
//! *one supplied formula*, not alias or substitute-precursor inference:
//! gugen's own `Composition::eq` (used everywhere else in the crate, e.g.
//! reaction balancing) is untouched and stays strictly literal. Formulas
//! with a genuinely different ratio (`FeO` vs `Fe2O3`), a different
//! element set (different oxides, carbonates, doped vs. undoped hosts), or
//! a hydrate vs. its anhydrous form (`CaSO4` vs `CaSO4·2H2O` -- the
//! formula parser folds hydrate water into the flat element-amount map,
//! giving them different, non-proportional atom counts) remain distinct:
//! canonicalization only reduces a shared ratio to lowest terms, it never
//! bridges formulas that aren't proportional to begin with.
//!
//! **gugen does not certify commercial data.** Catalog values are supplied
//! data; prices are estimates; availability may be stale; product
//! suitability for a given synthesis is not certified. Vendor documentation
//! and SDS sheets must be checked separately.

use crate::composition::{Composition, Element};
use crate::error::ProviderError;
use crate::frac::{Frac, gcd};
use std::collections::BTreeSet;

// ---------------------------------------------------------------------
// Identifiers and validated scalars
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CommercialOfferId(pub String);

impl std::fmt::Display for CommercialOfferId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A validated purity fraction, `0 < x <= 1`. Not `Score01`: `Score01`
/// allows `0.0` (a meaningful "no support" score elsewhere in the crate),
/// while a `0.0` purity is meaningless/rejectable here, and nothing else in
/// the crate reuses `Score01` outside `score_plan`'s own domain.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PurityFraction(f64);

impl PurityFraction {
    pub fn new(value: f64) -> Result<Self, CommercialCatalogError> {
        if !value.is_finite() || value <= 0.0 || value > 1.0 {
            return Err(CommercialCatalogError::InvalidPurity { value });
        }
        Ok(Self(value))
    }

    pub fn value(&self) -> f64 {
        self.0
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for PurityFraction {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        PurityFraction::new(value).map_err(serde::de::Error::custom)
    }
}

/// A package size, canonically stored in grams. `from_milligrams`/
/// `from_kilograms` are convenience constructors for spec's minimum "mg/g/kg"
/// requirement -- volume packaging is out of scope for Phase 22.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct PackageMass(f64);

impl PackageMass {
    pub fn new(grams: f64) -> Result<Self, CommercialCatalogError> {
        if !grams.is_finite() || grams <= 0.0 {
            return Err(CommercialCatalogError::InvalidPackageMass { value: grams });
        }
        Ok(Self(grams))
    }

    pub fn from_milligrams(mg: f64) -> Result<Self, CommercialCatalogError> {
        Self::new(mg / 1_000.0)
    }

    pub fn from_kilograms(kg: f64) -> Result<Self, CommercialCatalogError> {
        Self::new(kg * 1_000.0)
    }

    pub fn grams(&self) -> f64 {
        self.0
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for PackageMass {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let grams = f64::deserialize(deserializer)?;
        PackageMass::new(grams).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ParticleSizeRangeUm {
    pub min_um: f64,
    pub max_um: f64,
}

impl ParticleSizeRangeUm {
    pub fn new(min_um: f64, max_um: f64) -> Result<Self, CommercialCatalogError> {
        if !min_um.is_finite() || !max_um.is_finite() {
            return Err(CommercialCatalogError::InvalidParticleSizeRange {
                reason: "min_um and max_um must both be finite".to_string(),
            });
        }
        if min_um < 0.0 {
            return Err(CommercialCatalogError::InvalidParticleSizeRange {
                reason: format!("min_um must be >= 0, got {min_um}"),
            });
        }
        if min_um > max_um {
            return Err(CommercialCatalogError::InvalidParticleSizeRange {
                reason: format!("min_um ({min_um}) must be <= max_um ({max_um})"),
            });
        }
        Ok(Self { min_um, max_um })
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ParticleSizeRangeUm {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Raw {
            min_um: f64,
            max_um: f64,
        }
        let raw = Raw::deserialize(deserializer)?;
        ParticleSizeRangeUm::new(raw.min_um, raw.max_um).map_err(serde::de::Error::custom)
    }
}

/// A 3-letter uppercase ASCII currency code. Format-validated only, *not* a
/// full ISO 4217 whitelist: unlike `Element` (which must reject typos
/// against a closed table because the crate needs to refuse invalid symbols
/// outright), the only thing this module's arithmetic needs from a currency
/// is "is this the same currency as that one" -- format validation plus
/// preventing cross-currency summing satisfies that without a table that
/// would go stale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CurrencyCode([u8; 3]);

impl CurrencyCode {
    pub fn new(code: &str) -> Result<Self, CommercialCatalogError> {
        let bytes = code.as_bytes();
        if bytes.len() != 3 || !bytes.iter().all(u8::is_ascii_uppercase) {
            return Err(CommercialCatalogError::InvalidCurrencyCode {
                code: code.to_string(),
            });
        }
        Ok(Self([bytes[0], bytes[1], bytes[2]]))
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("CurrencyCode bytes are always valid ASCII")
    }
}

impl std::fmt::Display for CurrencyCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for CurrencyCode {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for CurrencyCode {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        CurrencyCode::new(&s).map_err(serde::de::Error::custom)
    }
}

/// Money as integer minor units -- never `f64`, which has no `checked_*`
/// arithmetic. Any `(minor_units, currency)` pair is valid once `currency`
/// itself validated, so construction is infallible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Money {
    minor_units: u64,
    currency: CurrencyCode,
}

impl Money {
    pub fn new(minor_units: u64, currency: CurrencyCode) -> Self {
        Self {
            minor_units,
            currency,
        }
    }

    pub fn minor_units(&self) -> u64 {
        self.minor_units
    }

    pub fn currency(&self) -> CurrencyCode {
        self.currency
    }

    /// `None` on overflow -- never panics.
    pub fn checked_mul_quantity(&self, count: u64) -> Option<Money> {
        self.minor_units
            .checked_mul(count)
            .map(|minor_units| Money {
                minor_units,
                currency: self.currency,
            })
    }

    /// `None` on overflow *or* currency mismatch -- currencies are never
    /// summed across each other.
    pub fn checked_add(&self, other: &Money) -> Option<Money> {
        if self.currency != other.currency {
            return None;
        }
        self.minor_units
            .checked_add(other.minor_units)
            .map(|minor_units| Money {
                minor_units,
                currency: self.currency,
            })
    }
}

/// A CAS Registry Number as supplied by a catalog row. `checksum_verified`
/// distinguishes "checksum verified" from "checksum failed or the string
/// isn't CAS-shaped at all" -- Phase 22 never uses CAS as a basis for
/// chemical-identity matching (composition is), so a malformed CAS is
/// recorded, not rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CasNumber {
    pub raw: String,
    pub checksum_verified: bool,
}

impl CasNumber {
    pub fn new(raw: &str) -> Self {
        Self {
            raw: raw.to_string(),
            checksum_verified: cas_checksum_valid(raw),
        }
    }
}

/// CAS check-digit algorithm: for `NNNNNNN-NN-C`, reverse the digits before
/// the checksum, multiply each by its 1-based position, sum, mod 10.
fn cas_checksum_valid(raw: &str) -> bool {
    let mut groups = raw.split('-');
    let (Some(g1), Some(g2), Some(g3)) = (groups.next(), groups.next(), groups.next()) else {
        return false;
    };
    if groups.next().is_some() {
        return false;
    }
    if !(2..=7).contains(&g1.len()) || !g1.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if g2.len() != 2 || !g2.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    if g3.len() != 1 || !g3.bytes().all(|b| b.is_ascii_digit()) {
        return false;
    }
    let checksum_digit = (g3.as_bytes()[0] - b'0') as u32;
    let sum: u32 = g1
        .bytes()
        .chain(g2.bytes())
        .rev()
        .enumerate()
        .map(|(i, b)| (i as u32 + 1) * (b - b'0') as u32)
        .sum();
    sum % 10 == checksum_digit
}

// ---------------------------------------------------------------------
// Offer, provenance, catalog
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AvailabilityStatus {
    InStock,
    LimitedStock,
    BackOrdered,
    MadeToOrder,
    Discontinued,
}

fn parse_availability(s: &str) -> Option<AvailabilityStatus> {
    match s.trim().to_ascii_lowercase().as_str() {
        "in_stock" => Some(AvailabilityStatus::InStock),
        "limited_stock" => Some(AvailabilityStatus::LimitedStock),
        "back_ordered" => Some(AvailabilityStatus::BackOrdered),
        "made_to_order" => Some(AvailabilityStatus::MadeToOrder),
        "discontinued" => Some(AvailabilityStatus::Discontinued),
        _ => None,
    }
}

/// How an offer's data entered gugen -- describes the ingestion mechanism,
/// not anything about the product itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CommercialSourceType {
    UserSuppliedCsv,
    VendorExport,
    DistributorExport,
    ManuallyTranscribed,
    SyntheticFixture,
}

fn parse_source_type(s: &str) -> Option<CommercialSourceType> {
    match s.trim().to_ascii_lowercase().as_str() {
        "user_supplied_csv" => Some(CommercialSourceType::UserSuppliedCsv),
        "vendor_export" => Some(CommercialSourceType::VendorExport),
        "distributor_export" => Some(CommercialSourceType::DistributorExport),
        "manually_transcribed" => Some(CommercialSourceType::ManuallyTranscribed),
        "synthetic_fixture" => Some(CommercialSourceType::SyntheticFixture),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OfferProvenance {
    pub source_type: CommercialSourceType,
    pub source_identifier: String,
    /// Caller-supplied string; gugen never reads the system clock.
    pub retrieved_at: Option<String>,
    pub supplied_by: Option<String>,
    pub license_or_terms: Option<String>,
    pub checksum: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CommercialPrecursorOffer {
    // required
    pub offer_id: CommercialOfferId,
    pub manufacturer: String,
    pub product_name: String,
    pub composition: Composition,
    pub provenance: OfferProvenance,
    // optional
    pub formula: String,
    pub catalog_number: Option<String>,
    pub cas_number: Option<CasNumber>,
    pub grade: Option<String>,
    pub purity: Option<PurityFraction>,
    pub package_mass: Option<PackageMass>,
    pub unit_price: Option<Money>,
    pub availability: Option<AvailabilityStatus>,
    pub lead_time_days: Option<u32>,
    pub physical_form: Option<String>,
    pub particle_size_range_um: Option<ParticleSizeRangeUm>,
    pub country_region: Option<String>,
    /// Stored, never fetched -- no network access anywhere in this module.
    pub product_url: Option<String>,
    pub tags: BTreeSet<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Error, Clone, PartialEq)]
#[non_exhaustive]
pub enum CommercialCatalogError {
    #[error("purity fraction must be > 0 and <= 1, got {value}")]
    InvalidPurity { value: f64 },
    #[error("package mass must be finite and > 0 grams, got {value}")]
    InvalidPackageMass { value: f64 },
    #[error("particle size range invalid: {reason}")]
    InvalidParticleSizeRange { reason: String },
    #[error("{code:?} is not a 3-letter uppercase ASCII currency code")]
    InvalidCurrencyCode { code: String },
    #[error("could not parse formula {formula:?}: {reason}")]
    FormulaParseError { formula: String, reason: String },
    #[error("inconsistent commercial planning request: {reason}")]
    InconsistentRequest { reason: String },
}

use thiserror::Error;

// ---------------------------------------------------------------------
// Formula parser
// ---------------------------------------------------------------------
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

// ---------------------------------------------------------------------
// Load report
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommercialCatalogLoadMode {
    Strict,
    Lenient,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RejectedOffer {
    pub row: usize,
    pub offer_id: String,
    pub field: String,
    pub reason: String,
    pub original_value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommercialCatalogLoadReport {
    pub accepted: usize,
    pub duplicate_offer_ids_collapsed: usize,
    pub rejected: Vec<RejectedOffer>,
}

// ---------------------------------------------------------------------
// Catalog
// ---------------------------------------------------------------------

/// A loaded, deduplicated commercial-offer catalog. `offers` is private
/// (unlike `SynthesisPlan`'s all-`pub` convention): `offer_id` uniqueness is
/// a real invariant this type guarantees -- it is what makes the
/// combination search's final tie-break key total.
#[derive(Debug, Clone, PartialEq)]
pub struct CommercialPrecursorCatalog {
    offers: Vec<CommercialPrecursorOffer>,
}

impl CommercialPrecursorCatalog {
    /// The single true constructor -- both `load_csv` and `load_json` funnel
    /// through this after per-row parsing. Sorted by `offer_id`, so results
    /// built from a catalog are invariant to construction order. Infallible:
    /// a `Vec` of already-valid offers can only produce duplicate-id
    /// rejections, tracked as `duplicate_offer_ids_collapsed` (the first
    /// offer for a given id, by sorted order, is kept).
    pub fn from_offers(
        mut offers: Vec<CommercialPrecursorOffer>,
    ) -> (Self, CommercialCatalogLoadReport) {
        offers.sort_by(|a, b| a.offer_id.0.cmp(&b.offer_id.0));
        let mut duplicate_offer_ids_collapsed = 0;
        let mut deduped: Vec<CommercialPrecursorOffer> = Vec::with_capacity(offers.len());
        for offer in offers {
            if deduped
                .last()
                .is_some_and(|last: &CommercialPrecursorOffer| last.offer_id == offer.offer_id)
            {
                duplicate_offer_ids_collapsed += 1;
                continue;
            }
            deduped.push(offer);
        }
        let report = CommercialCatalogLoadReport {
            accepted: deduped.len(),
            duplicate_offer_ids_collapsed,
            rejected: Vec::new(),
        };
        (Self { offers: deduped }, report)
    }

    pub fn load_csv(
        csv_text: &str,
        mode: CommercialCatalogLoadMode,
    ) -> std::result::Result<(Self, CommercialCatalogLoadReport), ProviderError> {
        load_csv_impl(csv_text, mode)
    }

    #[cfg(feature = "serde")]
    pub fn load_json(
        json_text: &str,
        mode: CommercialCatalogLoadMode,
    ) -> std::result::Result<(Self, CommercialCatalogLoadReport), ProviderError> {
        load_json_impl(json_text, mode)
    }

    pub fn offers(&self) -> &[CommercialPrecursorOffer] {
        &self.offers
    }

    pub fn get(&self, id: &CommercialOfferId) -> Option<&CommercialPrecursorOffer> {
        self.offers.iter().find(|o| &o.offer_id == id)
    }

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
        self.offers.iter().filter(move |o| {
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

/// Both-or-neither: price and currency must be present together (spec: a
/// row with exactly one of them is malformed, not "price unknown").
fn validate_price(
    price_minor_units: Option<u64>,
    currency: Option<CurrencyCode>,
) -> Result<Option<Money>, String> {
    match (price_minor_units, currency) {
        (None, None) => Ok(None),
        (Some(price), Some(currency)) => Ok(Some(Money::new(price, currency))),
        (Some(_), None) => Err("price_minor_units present without currency".to_string()),
        (None, Some(_)) => Err("currency present without price_minor_units".to_string()),
    }
}

fn validate_particle_size(
    min_um: Option<f64>,
    max_um: Option<f64>,
) -> Result<Option<ParticleSizeRangeUm>, CommercialCatalogError> {
    match (min_um, max_um) {
        (None, None) => Ok(None),
        (Some(min_um), Some(max_um)) => ParticleSizeRangeUm::new(min_um, max_um).map(Some),
        (Some(_), None) => Err(CommercialCatalogError::InvalidParticleSizeRange {
            reason: "particle_size_min_um present without particle_size_max_um".to_string(),
        }),
        (None, Some(_)) => Err(CommercialCatalogError::InvalidParticleSizeRange {
            reason: "particle_size_max_um present without particle_size_min_um".to_string(),
        }),
    }
}

fn load_csv_impl(
    csv_text: &str,
    mode: CommercialCatalogLoadMode,
) -> std::result::Result<(CommercialPrecursorCatalog, CommercialCatalogLoadReport), ProviderError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(csv_text.as_bytes());
    let headers = reader
        .headers()
        .map_err(|e| ProviderError::MalformedRecord(format!("CSV header: {e}")))?
        .clone();

    for required_column in [
        "offer_id",
        "manufacturer",
        "product_name",
        "formula",
        "source",
    ] {
        if !headers.iter().any(|h| h == required_column) {
            return Err(ProviderError::MalformedRecord(format!(
                "CSV header is missing required column '{required_column}'"
            )));
        }
    }

    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    let mut seen_ids: BTreeSet<String> = BTreeSet::new();

    for (row, result) in reader.records().enumerate() {
        let record = match result {
            Ok(r) => r,
            Err(e) => {
                if mode == CommercialCatalogLoadMode::Strict {
                    return Err(ProviderError::MalformedRecord(format!("row {row}: {e}")));
                }
                rejected.push(RejectedOffer {
                    row,
                    offer_id: String::new(),
                    field: "row".to_string(),
                    reason: e.to_string(),
                    original_value: String::new(),
                });
                continue;
            }
        };

        match parse_csv_offer_row(&record, &headers, row) {
            Ok(offer) => {
                if !seen_ids.insert(offer.offer_id.0.clone()) {
                    // Duplicate offer_id is always a soft rejection, even in
                    // Strict mode: it's data noise in one row, not evidence
                    // the whole file is corrupt.
                    rejected.push(RejectedOffer {
                        row,
                        offer_id: offer.offer_id.0.clone(),
                        field: "offer_id".to_string(),
                        reason: "duplicate offer_id within this load".to_string(),
                        original_value: offer.offer_id.0.clone(),
                    });
                    continue;
                }
                accepted.push(offer);
            }
            Err(rejection) => {
                if mode == CommercialCatalogLoadMode::Strict {
                    return Err(ProviderError::MalformedRecord(format!(
                        "row {}: field '{}': {}",
                        rejection.row, rejection.field, rejection.reason
                    )));
                }
                rejected.push(rejection);
            }
        }
    }

    let (catalog, from_offers_report) = CommercialPrecursorCatalog::from_offers(accepted);
    Ok((
        catalog,
        CommercialCatalogLoadReport {
            accepted: from_offers_report.accepted,
            duplicate_offer_ids_collapsed: from_offers_report.duplicate_offer_ids_collapsed,
            rejected,
        },
    ))
}

fn parse_csv_offer_row(
    record: &csv::StringRecord,
    headers: &csv::StringRecord,
    row: usize,
) -> Result<CommercialPrecursorOffer, RejectedOffer> {
    let index_of = |name: &str| headers.iter().position(|h| h == name);
    let field = |name: &str| -> Option<String> {
        index_of(name)
            .and_then(|i| record.get(i))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let reject = |field_name: &str, reason: String, original_value: &str| RejectedOffer {
        row,
        offer_id: field("offer_id").unwrap_or_default(),
        field: field_name.to_string(),
        reason,
        original_value: original_value.to_string(),
    };
    let required = |name: &str| -> Result<String, RejectedOffer> {
        field(name).ok_or_else(|| reject(name, format!("missing required field '{name}'"), ""))
    };
    let parse_opt_f64 = |name: &str| -> Result<Option<f64>, RejectedOffer> {
        match field(name) {
            None => Ok(None),
            Some(s) => s
                .parse::<f64>()
                .map(Some)
                .map_err(|e| reject(name, e.to_string(), &s)),
        }
    };
    let parse_opt_u32 = |name: &str| -> Result<Option<u32>, RejectedOffer> {
        match field(name) {
            None => Ok(None),
            Some(s) => s
                .parse::<u32>()
                .map(Some)
                .map_err(|e| reject(name, e.to_string(), &s)),
        }
    };
    let parse_opt_u64 = |name: &str| -> Result<Option<u64>, RejectedOffer> {
        match field(name) {
            None => Ok(None),
            Some(s) => s
                .parse::<u64>()
                .map(Some)
                .map_err(|e| reject(name, e.to_string(), &s)),
        }
    };

    let offer_id = required("offer_id")?;
    let manufacturer = required("manufacturer")?;
    let product_name = required("product_name")?;
    let formula = required("formula")?;
    let source = required("source")?;

    let composition =
        parse_formula(&formula).map_err(|e| reject("formula", e.to_string(), &formula))?;
    let source_type = parse_source_type(&source).ok_or_else(|| {
        reject(
            "source",
            format!("'{source}' is not a recognized source type"),
            &source,
        )
    })?;

    let purity = match field("purity_fraction") {
        None => None,
        Some(s) => Some(
            s.parse::<f64>()
                .map_err(|e| reject("purity_fraction", e.to_string(), &s))
                .and_then(|v| {
                    PurityFraction::new(v).map_err(|e| reject("purity_fraction", e.to_string(), &s))
                })?,
        ),
    };
    let package_mass = match parse_opt_f64("package_mass_g")? {
        None => None,
        Some(v) => Some(
            PackageMass::new(v)
                .map_err(|e| reject("package_mass_g", e.to_string(), &v.to_string()))?,
        ),
    };
    let price_minor_units = parse_opt_u64("price_minor_units")?;
    let currency = match field("currency") {
        None => None,
        Some(s) => Some(CurrencyCode::new(&s).map_err(|e| reject("currency", e.to_string(), &s))?),
    };
    let unit_price = validate_price(price_minor_units, currency)
        .map_err(|reason| reject("price_minor_units", reason, ""))?;
    let availability = match field("availability") {
        None => None,
        Some(s) => Some(parse_availability(&s).ok_or_else(|| {
            reject(
                "availability",
                format!("'{s}' is not a recognized availability status"),
                &s,
            )
        })?),
    };
    let lead_time_days = parse_opt_u32("lead_time_days")?;
    let particle_size_min_um = parse_opt_f64("particle_size_min_um")?;
    let particle_size_max_um = parse_opt_f64("particle_size_max_um")?;
    let particle_size_range_um = validate_particle_size(particle_size_min_um, particle_size_max_um)
        .map_err(|e| reject("particle_size_min_um", e.to_string(), ""))?;
    let cas_number = field("cas_number").map(|s| CasNumber::new(&s));
    let tags: BTreeSet<String> = field("tags")
        .map(|s| {
            s.split(';')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    Ok(CommercialPrecursorOffer {
        offer_id: CommercialOfferId(offer_id.clone()),
        manufacturer,
        product_name,
        composition,
        provenance: OfferProvenance {
            source_type,
            source_identifier: offer_id,
            retrieved_at: field("retrieved_at"),
            supplied_by: None,
            license_or_terms: None,
            checksum: None,
        },
        formula,
        catalog_number: field("catalog_number"),
        cas_number,
        grade: field("grade"),
        purity,
        package_mass,
        unit_price,
        availability,
        lead_time_days,
        physical_form: field("physical_form"),
        particle_size_range_um,
        country_region: field("country_region"),
        product_url: field("product_url"),
        tags,
        notes: field("notes"),
    })
}

#[cfg(feature = "serde")]
fn load_json_impl(
    json_text: &str,
    mode: CommercialCatalogLoadMode,
) -> std::result::Result<(CommercialPrecursorCatalog, CommercialCatalogLoadReport), ProviderError> {
    #[derive(serde::Deserialize)]
    struct CatalogFile {
        offers: Vec<serde_json::Value>,
    }
    #[derive(serde::Deserialize)]
    struct RawOffer {
        offer_id: String,
        manufacturer: String,
        product_name: String,
        formula: String,
        source_type: String,
        source_identifier: Option<String>,
        catalog_number: Option<String>,
        cas_number: Option<String>,
        grade: Option<String>,
        purity: Option<PurityFraction>,
        package_mass_g: Option<f64>,
        price_minor_units: Option<u64>,
        currency: Option<String>,
        availability: Option<String>,
        lead_time_days: Option<u32>,
        physical_form: Option<String>,
        particle_size_min_um: Option<f64>,
        particle_size_max_um: Option<f64>,
        country_region: Option<String>,
        product_url: Option<String>,
        retrieved_at: Option<String>,
        tags: Option<Vec<String>>,
        notes: Option<String>,
    }

    let file: CatalogFile = serde_json::from_str(json_text)
        .map_err(|e| ProviderError::MalformedRecord(format!("catalog file: {e}")))?;

    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    let mut seen_ids: BTreeSet<String> = BTreeSet::new();

    for (row, value) in file.offers.into_iter().enumerate() {
        let convert = || -> Result<CommercialPrecursorOffer, RejectedOffer> {
            let raw: RawOffer =
                serde_json::from_value(value.clone()).map_err(|e| RejectedOffer {
                    row,
                    offer_id: value
                        .get("offer_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    field: "row".to_string(),
                    reason: e.to_string(),
                    original_value: value.to_string(),
                })?;
            let reject = |field_name: &str, reason: String, original_value: &str| RejectedOffer {
                row,
                offer_id: raw.offer_id.clone(),
                field: field_name.to_string(),
                reason,
                original_value: original_value.to_string(),
            };

            let composition = parse_formula(&raw.formula)
                .map_err(|e| reject("formula", e.to_string(), &raw.formula))?;
            let source_type = parse_source_type(&raw.source_type).ok_or_else(|| {
                reject(
                    "source_type",
                    format!("'{}' is not a recognized source type", raw.source_type),
                    &raw.source_type,
                )
            })?;
            let currency = match &raw.currency {
                None => None,
                Some(s) => {
                    Some(CurrencyCode::new(s).map_err(|e| reject("currency", e.to_string(), s))?)
                }
            };
            let unit_price = validate_price(raw.price_minor_units, currency)
                .map_err(|reason| reject("price_minor_units", reason, ""))?;
            let availability = match &raw.availability {
                None => None,
                Some(s) => Some(parse_availability(s).ok_or_else(|| {
                    reject(
                        "availability",
                        format!("'{s}' is not a recognized availability status"),
                        s,
                    )
                })?),
            };
            let particle_size_range_um =
                validate_particle_size(raw.particle_size_min_um, raw.particle_size_max_um)
                    .map_err(|e| reject("particle_size_min_um", e.to_string(), ""))?;

            Ok(CommercialPrecursorOffer {
                offer_id: CommercialOfferId(raw.offer_id.clone()),
                manufacturer: raw.manufacturer,
                product_name: raw.product_name,
                composition,
                provenance: OfferProvenance {
                    source_type,
                    source_identifier: raw
                        .source_identifier
                        .unwrap_or_else(|| raw.offer_id.clone()),
                    retrieved_at: raw.retrieved_at,
                    supplied_by: None,
                    license_or_terms: None,
                    checksum: None,
                },
                formula: raw.formula,
                catalog_number: raw.catalog_number,
                cas_number: raw.cas_number.as_deref().map(CasNumber::new),
                grade: raw.grade,
                purity: raw.purity,
                package_mass: raw
                    .package_mass_g
                    .map(PackageMass::new)
                    .transpose()
                    .map_err(|e| {
                        reject(
                            "package_mass_g",
                            e.to_string(),
                            &raw.package_mass_g.unwrap_or_default().to_string(),
                        )
                    })?,
                unit_price,
                availability,
                lead_time_days: raw.lead_time_days,
                physical_form: raw.physical_form,
                particle_size_range_um,
                country_region: raw.country_region,
                product_url: raw.product_url,
                tags: raw.tags.unwrap_or_default().into_iter().collect(),
                notes: raw.notes,
            })
        };

        match convert() {
            Ok(offer) => {
                if !seen_ids.insert(offer.offer_id.0.clone()) {
                    rejected.push(RejectedOffer {
                        row,
                        offer_id: offer.offer_id.0.clone(),
                        field: "offer_id".to_string(),
                        reason: "duplicate offer_id within this load".to_string(),
                        original_value: offer.offer_id.0.clone(),
                    });
                    continue;
                }
                accepted.push(offer);
            }
            Err(rejection) => {
                if mode == CommercialCatalogLoadMode::Strict {
                    return Err(ProviderError::MalformedRecord(format!(
                        "row {}: field '{}': {}",
                        rejection.row, rejection.field, rejection.reason
                    )));
                }
                rejected.push(rejection);
            }
        }
    }

    let (catalog, from_offers_report) = CommercialPrecursorCatalog::from_offers(accepted);
    Ok((
        catalog,
        CommercialCatalogLoadReport {
            accepted: from_offers_report.accepted,
            duplicate_offer_ids_collapsed: from_offers_report.duplicate_offer_ids_collapsed,
            rejected,
        },
    ))
}

// =======================================================================
// Plan assessment (Phase 22B/22C): matches a SynthesisPlan's precursors
// against a catalog, applies commercial constraints, computes purchase
// quantities and costs, and searches a bounded space of complete
// combinations. Never touches SynthesisPlan/BalancedReaction/ProcessStep --
// see the module doc comment.
// =======================================================================

use crate::precursor::PrecursorId;
use crate::reaction::BalancedReaction;
use crate::report::{PlanId, SynthesisPlan, WarningSeverity};

// ---------------------------------------------------------------------
// Request / config
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MissingCommercialDataPolicy {
    /// A hard-constrained field the offer doesn't report excludes the
    /// offer. Conservative default: a request constraint that can't be
    /// verified is treated as unsatisfied, not as satisfied-by-omission.
    #[default]
    Reject,
    /// A hard-constrained field the offer doesn't report is treated as
    /// "does not violate" -- only fields the offer actually reports are
    /// checked against the constraint.
    KeepWithWarning,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct CommercialPlanningRequest {
    /// Required only if `target_batch_mass_grams` is `Some` -- identifies
    /// which entry in the plan's `balanced_reaction.products` is the
    /// target, since `products` also holds curated byproducts.
    pub target_composition: Option<Composition>,
    pub target_batch_mass_grams: Option<f64>,
    pub allowed_manufacturers: Option<BTreeSet<String>>,
    pub excluded_manufacturers: BTreeSet<String>,
    pub min_purity: Option<PurityFraction>,
    pub max_lead_time_days: Option<u32>,
    /// `None` = unrestricted (including `Discontinued`).
    pub allowed_availability_statuses: Option<BTreeSet<AvailabilityStatus>>,
    pub allowed_physical_forms: Option<BTreeSet<String>>,
    pub required_tags: BTreeSet<String>,
    pub excluded_tags: BTreeSet<String>,
    /// Its own currency further constrains matching: a combination whose
    /// total isn't comparable to this (different or unknown currency) can't
    /// be checked against it, and is kept with a warning rather than
    /// silently passed or failed.
    pub max_total_cost: Option<Money>,
    pub allowed_currencies: Option<BTreeSet<CurrencyCode>>,
    pub require_known_price: bool,
    pub require_known_package_size: bool,
    pub missing_data_policy: MissingCommercialDataPolicy,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommercialPlanningConfig {
    pub max_offers_per_precursor: usize,
    pub max_combinations_evaluated: usize,
    pub max_results_returned: usize,
}

impl Default for CommercialPlanningConfig {
    fn default() -> Self {
        // ponytail: arbitrary-but-documented starting bounds; revisit once
        // measured against a real catalog.
        Self {
            max_offers_per_precursor: 50,
            max_combinations_evaluated: 10_000,
            max_results_returned: 5,
        }
    }
}

// ---------------------------------------------------------------------
// Exclusions, warnings
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CommercialExclusionCode {
    PurityBelowMinimum,
    ManufacturerNotAllowed,
    LeadTimeExceedsMaximum,
    /// `missing_data_policy: Reject`, on a hard-constrained optional field
    /// (purity/lead time/availability/physical form/currency).
    MissingConstrainedField,
    /// `request.require_known_price` and the offer's `unit_price` is `None`.
    PriceRequiredButUnknown,
    /// `request.require_known_package_size` and the offer's `package_mass`
    /// is `None`.
    PackageSizeRequiredButUnknown,
    AvailabilityExcluded,
    RequiredTagMissing,
    ExcludedTagPresent,
    CurrencyNotAllowed,
    PhysicalFormNotAllowed,
    OfferCountCapExceeded,
    EvaluationBudgetExhausted,
    /// `Money::checked_mul_quantity`/`checked_add` returned `None` -- the
    /// offer is excluded, never treated as costing 0 and never panicked on.
    CostOverflow,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommercialExclusion {
    pub precursor: PrecursorId,
    /// `None` for a whole-row exclusion (zero surviving offers), not a
    /// specific offer.
    pub offer_id: Option<CommercialOfferId>,
    pub reason_codes: Vec<CommercialExclusionCode>,
    pub explanation: String,
}

/// Its own type (not a reuse of `PlanningWarning`), deliberately -- keeps
/// this module's return type structurally separate from anything
/// `score_plan` could ever consume, reinforcing at the type level that
/// commercial data can never leak into scientific scoring.
#[derive(Debug, Clone, PartialEq)]
pub struct CommercialWarning {
    pub message: String,
    pub severity: WarningSeverity,
}

// ---------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct CommercialOfferSelection {
    pub precursor: PrecursorId,
    pub precursor_composition: Composition,
    pub reaction_coefficient: u64,
    pub offer_id: CommercialOfferId,
    /// Stoichiometric theoretical requirement -- purity-agnostic, always
    /// computable from the plan alone. Never a yield claim, never adjusted
    /// for process loss or weighing margin.
    pub theoretical_pure_mass_required_grams: f64,
    /// `None` if the offer's purity is unknown.
    pub purity_adjusted_purchase_mass_grams: Option<f64>,
    /// `None` if the offer's package mass is unknown.
    pub package_count: Option<u64>,
    pub purchased_mass_grams: Option<f64>,
    pub excess_mass_grams: Option<f64>,
    /// `None` if price or package size is unknown, or the multiplication
    /// overflowed -- never 0.
    pub subtotal: Option<Money>,
    pub unresolved_fields: Vec<&'static str>,
    pub assumptions: Vec<String>,
    pub warnings: Vec<CommercialWarning>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommercialCombination {
    /// Offer ids joined in precursor-row order with `|` -- plain string
    /// concatenation, not a hash: hash algorithms like `DefaultHasher`
    /// aren't guaranteed stable across Rust versions, which would silently
    /// break determinism tests later. String concatenation is trivially,
    /// permanently deterministic.
    pub combination_id: String,
    pub selections: Vec<CommercialOfferSelection>,
    /// `Some` only if every selection's subtotal is known and they all
    /// share one currency.
    pub total_cost: Option<Money>,
    pub all_costs_known: bool,
    /// `None` if any selection's lead time is unknown.
    pub max_lead_time_days: Option<u32>,
    /// `true` unless a selected offer is explicitly `Discontinued`.
    /// Unreported availability (no offer field set) counts as
    /// acceptable-but-unknown, not unacceptable -- missing metadata is not
    /// evidence the compound can't be procured.
    pub all_availability_acceptable: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchBudgetSummary {
    pub combinations_evaluated: usize,
    pub combinations_omitted: u64,
    /// `false` if the evaluation budget was hit *or* any row was truncated
    /// by `max_offers_per_precursor` -- either one means the result set is
    /// not a complete accounting of every possible combination.
    pub is_exhaustive: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnresolvedCommercialField {
    pub precursor: PrecursorId,
    pub offer_id: CommercialOfferId,
    pub field: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommercialPlanAssessment {
    pub plan_id: PlanId,
    pub every_precursor_has_a_match: bool,
    /// Ranked best-first, up to `max_results_returned`.
    pub combinations: Vec<CommercialCombination>,
    pub unmatched_precursors: Vec<(PrecursorId, Composition)>,
    pub rejected_offers: Vec<CommercialExclusion>,
    /// Deduplicated across the offers actually selected in `combinations`
    /// (not every surviving catalog candidate) -- bounded by
    /// `max_results_returned`, not by catalog size.
    pub unresolved_commercial_fields: Vec<UnresolvedCommercialField>,
    pub warnings: Vec<CommercialWarning>,
    pub search_budget: SearchBudgetSummary,
}

// ---------------------------------------------------------------------
// Quantity / cost math (all checked, never panics)
// ---------------------------------------------------------------------

/// Sum of atomic weights over a `Composition`'s amounts -- the one thing
/// this module needs that nothing else in the crate exposes publicly (the
/// IUPAC atomic-weight table is `pub(crate)` specifically for this reuse).
pub(crate) fn molar_mass_g_per_mol(composition: &Composition) -> f64 {
    composition
        .iter()
        .map(|(element, amount)| crate::thermodynamics::atomic_weight_amu(element) * amount)
        .sum()
}

fn unresolved_fields_for(offer: &CommercialPrecursorOffer) -> Vec<&'static str> {
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
struct OfferQuantity {
    purity_adjusted_purchase_mass_grams: Option<f64>,
    package_count: Option<u64>,
    purchased_mass_grams: Option<f64>,
    excess_mass_grams: Option<f64>,
    subtotal: Option<Money>,
    cost_overflowed: bool,
}

fn compute_offer_quantity(
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
fn cost_rank_key(subtotal: Option<Money>) -> (u8, Option<CurrencyCode>, u64) {
    match subtotal {
        Some(money) => (0, Some(money.currency()), money.minor_units()),
        None => (1, None, 0),
    }
}

// ---------------------------------------------------------------------
// Hard constraints
// ---------------------------------------------------------------------

fn hard_constraint_violations(
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

struct OfferCandidate<'a> {
    offer: &'a CommercialPrecursorOffer,
    unresolved_fields: Vec<&'static str>,
    quantity: OfferQuantity,
}

/// Total order, ascending = better: fewer unresolved fields, then cheaper
/// (within a comparable cost bucket), then shorter lead time, then higher
/// purity, then manufacturer/catalog_number/offer_id as final deterministic
/// tiebreaks (offer_id is always unique, so this never returns `Equal` for
/// two distinct offers).
fn offer_rank_order(a: &OfferCandidate, b: &OfferCandidate) -> std::cmp::Ordering {
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
fn search_combinations(
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
fn exhaustive_search(
    rows: &[Vec<OfferCandidate>],
    config: &CommercialPlanningConfig,
    total_space: u64,
    max_total_cost: Option<Money>,
) -> (Vec<Vec<usize>>, usize, u64) {
    let mut all: Vec<HeapEntry> = Vec::with_capacity(total_space as usize);
    for combo_index in 0..total_space {
        let mut remainder = combo_index;
        let mut indices = vec![0usize; rows.len()];
        for (row_i, row) in rows.iter().enumerate() {
            let len = row.len() as u64;
            indices[row_i] = (remainder % len) as usize;
            remainder /= len;
        }
        all.push(HeapEntry::new(indices, rows));
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

fn build_combination(
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
    }
}

// ---------------------------------------------------------------------
// Public assessment API
// ---------------------------------------------------------------------

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
    use super::*;

    fn element(symbol: &str) -> Element {
        Element::new(symbol).unwrap()
    }

    fn composition(pairs: &[(&str, f64)]) -> Composition {
        Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
    }

    // -- formula parser --

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
    fn purity_fraction_rejects_zero_and_above_one() {
        assert!(PurityFraction::new(0.0).is_err());
        assert!(PurityFraction::new(1.0000001).is_err());
        assert!(PurityFraction::new(f64::NAN).is_err());
        assert!(PurityFraction::new(1.0).is_ok());
        assert!(PurityFraction::new(0.001).is_ok());
    }

    #[test]
    fn package_mass_rejects_non_positive_and_converts_units() {
        assert!(PackageMass::new(0.0).is_err());
        assert!(PackageMass::new(-1.0).is_err());
        assert_eq!(PackageMass::from_kilograms(1.0).unwrap().grams(), 1000.0);
        assert_eq!(PackageMass::from_milligrams(500.0).unwrap().grams(), 0.5);
    }

    #[test]
    fn currency_code_requires_three_uppercase_ascii_letters() {
        assert!(CurrencyCode::new("USD").is_ok());
        assert!(CurrencyCode::new("us").is_err());
        assert!(CurrencyCode::new("usd").is_err());
        assert!(CurrencyCode::new("USDD").is_err());
    }

    #[test]
    fn money_checked_arithmetic_never_panics_on_overflow() {
        let usd = CurrencyCode::new("USD").unwrap();
        let eur = CurrencyCode::new("EUR").unwrap();
        let big = Money::new(u64::MAX, usd);
        assert_eq!(big.checked_mul_quantity(2), None);
        assert_eq!(big.checked_add(&Money::new(1, usd)), None);
        assert_eq!(
            Money::new(1, usd).checked_add(&Money::new(1, eur)),
            None,
            "currencies never sum"
        );
        assert_eq!(
            Money::new(100, usd).checked_add(&Money::new(50, usd)),
            Some(Money::new(150, usd))
        );
    }

    #[test]
    fn cas_number_checksum_validates_water_and_rejects_a_bad_checksum() {
        assert!(CasNumber::new("7732-18-5").checksum_verified); // water
        assert!(!CasNumber::new("7732-18-4").checksum_verified);
        assert!(!CasNumber::new("not-a-cas-number").checksum_verified);
    }

    // -- catalog --

    fn offer(id: &str, formula: &str) -> CommercialPrecursorOffer {
        CommercialPrecursorOffer {
            offer_id: CommercialOfferId(id.to_string()),
            manufacturer: "Example Materials Ltd.".to_string(),
            product_name: "Demo Oxide Grade A".to_string(),
            composition: parse_formula(formula).unwrap(),
            provenance: OfferProvenance {
                source_type: CommercialSourceType::SyntheticFixture,
                source_identifier: id.to_string(),
                retrieved_at: None,
                supplied_by: None,
                license_or_terms: None,
                checksum: None,
            },
            formula: formula.to_string(),
            catalog_number: None,
            cas_number: None,
            grade: None,
            purity: None,
            package_mass: None,
            unit_price: None,
            availability: None,
            lead_time_days: None,
            physical_form: None,
            particle_size_range_um: None,
            country_region: None,
            product_url: None,
            tags: BTreeSet::new(),
            notes: None,
        }
    }

    #[test]
    fn from_offers_collapses_duplicate_ids_and_sorts_by_id() {
        let (catalog, report) = CommercialPrecursorCatalog::from_offers(vec![
            offer("B", "TiO2"),
            offer("A", "Fe2O3"),
            offer("A", "Fe2O3"),
        ]);
        assert_eq!(report.accepted, 2);
        assert_eq!(report.duplicate_offer_ids_collapsed, 1);
        let ids: Vec<&str> = catalog
            .offers()
            .iter()
            .map(|o| o.offer_id.0.as_str())
            .collect();
        assert_eq!(ids, vec!["A", "B"]);
    }

    #[test]
    fn empty_catalog_is_not_an_error() {
        let (catalog, report) = CommercialPrecursorCatalog::from_offers(vec![]);
        assert!(catalog.offers().is_empty());
        assert_eq!(report.accepted, 0);
    }

    // -- CSV loading --

    const SAMPLE_CSV: &str = "offer_id,manufacturer,product_name,formula,source,purity_fraction,package_mass_g,price_minor_units,currency\n\
        EML-1,Example Materials Ltd.,Demo Oxide Grade A,Fe2O3,synthetic_fixture,0.99,500,4500,USD\n\
        EML-2,Example Materials Ltd.,Demo Oxide Grade B,TiO2,synthetic_fixture,,,,\n";

    #[test]
    fn load_csv_accepts_valid_rows() {
        let (catalog, report) =
            CommercialPrecursorCatalog::load_csv(SAMPLE_CSV, CommercialCatalogLoadMode::Strict)
                .unwrap();
        assert_eq!(report.accepted, 2);
        assert!(report.rejected.is_empty());
        assert_eq!(catalog.offers().len(), 2);
    }

    #[test]
    fn load_csv_rejects_an_unparseable_formula_in_lenient_mode() {
        let csv = "offer_id,manufacturer,product_name,formula,source\nBAD,Example Materials Ltd.,Broken,not-a-formula!,synthetic_fixture\n";
        let (catalog, report) =
            CommercialPrecursorCatalog::load_csv(csv, CommercialCatalogLoadMode::Lenient).unwrap();
        assert!(catalog.offers().is_empty());
        assert_eq!(report.rejected.len(), 1);
        assert_eq!(report.rejected[0].field, "formula");
    }

    #[test]
    fn load_csv_strict_mode_fails_the_whole_load_on_the_first_bad_row() {
        let csv = "offer_id,manufacturer,product_name,formula,source\nBAD,Example Materials Ltd.,Broken,not-a-formula!,synthetic_fixture\n";
        let result = CommercialPrecursorCatalog::load_csv(csv, CommercialCatalogLoadMode::Strict);
        assert!(result.is_err());
    }

    #[test]
    fn load_csv_duplicate_offer_id_is_a_soft_rejection_even_in_strict_mode() {
        let csv = "offer_id,manufacturer,product_name,formula,source\n\
            A,Example Materials Ltd.,Demo Oxide Grade A,Fe2O3,synthetic_fixture\n\
            A,Example Materials Ltd.,Demo Oxide Grade A (dup),TiO2,synthetic_fixture\n";
        let (catalog, report) =
            CommercialPrecursorCatalog::load_csv(csv, CommercialCatalogLoadMode::Strict).unwrap();
        assert_eq!(catalog.offers().len(), 1);
        assert_eq!(report.rejected.len(), 1);
        assert_eq!(report.rejected[0].field, "offer_id");
    }

    #[test]
    fn load_csv_rejects_a_row_with_price_but_no_currency() {
        let csv = "offer_id,manufacturer,product_name,formula,source,price_minor_units\nA,Example Materials Ltd.,Demo Oxide,Fe2O3,synthetic_fixture,100\n";
        let (_, report) =
            CommercialPrecursorCatalog::load_csv(csv, CommercialCatalogLoadMode::Lenient).unwrap();
        assert_eq!(report.rejected.len(), 1);
    }

    #[test]
    fn load_csv_missing_header_column_is_a_hard_failure() {
        let csv = "manufacturer,product_name,formula,source\nExample Materials Ltd.,Demo Oxide,Fe2O3,synthetic_fixture\n";
        let result = CommercialPrecursorCatalog::load_csv(csv, CommercialCatalogLoadMode::Lenient);
        assert!(result.is_err());
    }

    #[test]
    fn load_csv_empty_file_produces_an_empty_catalog() {
        let csv = "offer_id,manufacturer,product_name,formula,source\n";
        let (catalog, report) =
            CommercialPrecursorCatalog::load_csv(csv, CommercialCatalogLoadMode::Strict).unwrap();
        assert!(catalog.offers().is_empty());
        assert_eq!(report.accepted, 0);
    }

    // -- JSON loading --

    #[cfg(feature = "serde")]
    #[test]
    fn load_json_accepts_valid_offers() {
        let json = r#"{"offers": [
            {"offer_id": "A", "manufacturer": "Example Materials Ltd.", "product_name": "Demo Oxide Grade A",
             "formula": "Fe2O3", "source_type": "synthetic_fixture", "purity": 0.99}
        ]}"#;
        let (catalog, report) =
            CommercialPrecursorCatalog::load_json(json, CommercialCatalogLoadMode::Strict).unwrap();
        assert_eq!(report.accepted, 1);
        assert_eq!(catalog.offers()[0].purity.map(|p| p.value()), Some(0.99));
    }

    #[cfg(feature = "serde")]
    #[test]
    fn load_json_rejects_a_malformed_field_type_in_lenient_mode() {
        let json = r#"{"offers": [
            {"offer_id": "A", "manufacturer": "Example Materials Ltd.", "product_name": "Demo Oxide Grade A",
             "formula": "Fe2O3", "source_type": "synthetic_fixture", "purity": "not-a-number"}
        ]}"#;
        let (catalog, report) =
            CommercialPrecursorCatalog::load_json(json, CommercialCatalogLoadMode::Lenient)
                .unwrap();
        assert!(catalog.offers().is_empty());
        assert_eq!(report.rejected.len(), 1);
    }

    // ===================================================================
    // Assessment: composition matching, hard constraints, quantity/cost, search
    // ===================================================================

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

    fn money(minor_units: u64, currency: &str) -> Money {
        Money::new(minor_units, CurrencyCode::new(currency).unwrap())
    }

    #[allow(clippy::too_many_arguments)]
    fn priced_offer(
        id: &str,
        formula: &str,
        manufacturer: &str,
        purity: Option<f64>,
        package_mass_g: Option<f64>,
        price: Option<(u64, &str)>,
        lead_time_days: Option<u32>,
        availability: Option<AvailabilityStatus>,
    ) -> CommercialPrecursorOffer {
        let mut o = offer(id, formula);
        o.manufacturer = manufacturer.to_string();
        o.purity = purity.map(|p| PurityFraction::new(p).unwrap());
        o.package_mass = package_mass_g.map(|m| PackageMass::new(m).unwrap());
        o.unit_price = price.map(|(units, cur)| money(units, cur));
        o.lead_time_days = lead_time_days;
        o.availability = availability;
        o
    }

    fn barium_titanate_plan() -> crate::report::SynthesisPlan {
        use crate::config::PlanningConfig;
        use crate::planner::Planner;
        use crate::precursor::{
            AvailabilityMetadata, InMemoryPrecursorCatalog, PrecursorCandidate,
        };
        use crate::target::{PlanningConstraints, TargetSpecification};

        let planner = Planner::offline_minimal(
            InMemoryPrecursorCatalog::new(vec![
                PrecursorCandidate {
                    id: PrecursorId("BaCO3".to_string()),
                    composition: composition(&[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
                    availability: Some(AvailabilityMetadata {
                        source: "curated_fixture".to_string(),
                    }),
                },
                PrecursorCandidate {
                    id: PrecursorId("TiO2".to_string()),
                    composition: composition(&[("Ti", 1.0), ("O", 2.0)]),
                    availability: Some(AvailabilityMetadata {
                        source: "curated_fixture".to_string(),
                    }),
                },
            ]),
            PlanningConfig::default(),
        );
        let target_spec = TargetSpecification {
            composition: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
            structure: None,
            desired_phase: None,
            constraints: PlanningConstraints::default(),
        };
        let report = planner.plan(&target_spec, "2026-08-22T00:00:00Z").unwrap();
        report
            .plans
            .into_iter()
            .next()
            .expect("BaCO3 + TiO2 -> BaTiO3 must produce at least one plan")
    }

    fn baco3_tio2_catalog(offers: Vec<CommercialPrecursorOffer>) -> CommercialPrecursorCatalog {
        CommercialPrecursorCatalog::from_offers(offers).0
    }

    fn default_baco3_tio2_offers() -> Vec<CommercialPrecursorOffer> {
        vec![
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
                "BACO3-PREMIUM",
                "BaCO3",
                "Demo Chemical Supply Co.",
                Some(0.999),
                Some(100.0),
                Some((5000, "USD")),
                Some(20),
                Some(AvailabilityStatus::InStock),
            ),
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
        ]
    }

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
