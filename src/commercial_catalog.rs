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
//! **Exact composition match, no ratio normalization.** Two formulas
//! written at different formula-unit scale (e.g. `Fe2O3` vs `Fe4O6`) do
//! *not* match -- `Composition`'s own equality is literal, not
//! GCD-reduced, and this module makes no attempt to bridge that. Hydrates
//! are naturally distinct from their anhydrous form because the formula
//! parser folds hydrate water into the flat element-amount map, giving
//! `CaSO4` and `CaSO4.2H2O` different atom counts -- no special-case logic
//! needed, just correct parsing.
//!
//! **gugen does not certify commercial data.** Catalog values are supplied
//! data; prices are estimates; availability may be stale; product
//! suitability for a given synthesis is not certified. Vendor documentation
//! and SDS sheets must be checked separately.

use crate::composition::{Composition, Element};
use crate::error::ProviderError;
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

struct FormulaParser<'a> {
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    source: &'a str,
}

impl<'a> FormulaParser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            chars: source.char_indices().peekable(),
            source,
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
                    let inner = self.parse_group()?;
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
    fn same_ratio_different_scale_formulas_do_not_match() {
        // Fe2O3 and Fe4O6 are the same substance at different formula-unit
        // scale -- Phase 22's exact-match policy is deliberately literal,
        // not ratio-normalized (Composition::eq itself never reduces).
        let fe2o3 = parse_formula("Fe2O3").unwrap();
        let fe4o6 = parse_formula("Fe4O6").unwrap();
        assert_ne!(fe2o3, fe4o6);
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
}
