//! Domain types: identifiers, validated scalars, offer/provenance/catalog
//! data, request/config, exclusions/warnings, and result types. See the
//! module root (`super`) doc comment for the whole feature's architecture.

use crate::composition::Composition;
use crate::precursor::PrecursorId;
use crate::report::{PlanId, WarningSeverity};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// The 5 CSV header names `load_csv` cannot proceed without. Single source
/// of truth shared by the required-column check in `loader.rs` and
/// `CommercialCatalogColumnMap`'s key validation below -- previously these
/// existed as two unsynced hardcoded copies.
pub(crate) const REQUIRED_CSV_COLUMNS: &[&str] = &[
    "offer_id",
    "manufacturer",
    "product_name",
    "formula",
    "source",
];

/// The 17 CSV header names `load_csv` reads if present. See
/// `REQUIRED_CSV_COLUMNS` above.
pub(crate) const OPTIONAL_CSV_COLUMNS: &[&str] = &[
    "purity_fraction",
    "package_mass_g",
    "price_minor_units",
    "currency",
    "availability",
    "lead_time_days",
    "particle_size_min_um",
    "particle_size_max_um",
    "cas_number",
    "tags",
    "catalog_number",
    "grade",
    "physical_form",
    "country_region",
    "product_url",
    "notes",
    "retrieved_at",
];

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
    min_um: f64,
    max_um: f64,
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

    pub fn min_um(&self) -> f64 {
        self.min_um
    }

    pub fn max_um(&self) -> f64 {
        self.max_um
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum AvailabilityStatus {
    InStock,
    LimitedStock,
    BackOrdered,
    MadeToOrder,
    Discontinued,
}

pub(crate) fn parse_availability(s: &str) -> Option<AvailabilityStatus> {
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

pub(crate) fn parse_source_type(s: &str) -> Option<CommercialSourceType> {
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
    #[error("invalid CSV column map: {reason}")]
    InvalidColumnMap { reason: String },
}

/// A declarative mapping from gugen's canonical CSV column names (e.g.
/// `formula`, `manufacturer`) to the header names an actual supplier's
/// export file uses (e.g. `Chemical Formula`, `Supplier`) -- lets
/// `CommercialPrecursorCatalog::load_csv_with_column_map` accept
/// non-standard headers without inventing per-manufacturer adapters. Only
/// the columns that differ need an entry; anything omitted is looked up
/// under its canonical name as usual.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommercialCatalogColumnMap(BTreeMap<String, String>);

impl CommercialCatalogColumnMap {
    /// `map` is canonical name -> external header name. Rejects an unknown
    /// canonical name (typo protection -- this is user-supplied config) and
    /// a non-injective map (two canonical names claiming the same external
    /// header is ambiguous).
    pub fn new(map: BTreeMap<String, String>) -> Result<Self, CommercialCatalogError> {
        let mut seen_external: BTreeSet<&str> = BTreeSet::new();
        for (canonical, external) in &map {
            if !REQUIRED_CSV_COLUMNS.contains(&canonical.as_str())
                && !OPTIONAL_CSV_COLUMNS.contains(&canonical.as_str())
            {
                return Err(CommercialCatalogError::InvalidColumnMap {
                    reason: format!("'{canonical}' is not a known canonical column name"),
                });
            }
            if !seen_external.insert(external.as_str()) {
                return Err(CommercialCatalogError::InvalidColumnMap {
                    reason: format!(
                        "external header '{external}' is claimed by more than one canonical column"
                    ),
                });
            }
        }
        Ok(Self(map))
    }

    /// External header name -> canonical column name, for `loader.rs`'s
    /// header-row remap step.
    pub(crate) fn canonical_by_external(&self) -> BTreeMap<&str, &str> {
        self.0
            .iter()
            .map(|(canonical, external)| (external.as_str(), canonical.as_str()))
            .collect()
    }
}

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

/// A loaded, deduplicated commercial-offer catalog. `offers` is private
/// (unlike `SynthesisPlan`'s all-`pub` convention): `offer_id` uniqueness is
/// a real invariant this type guarantees -- it is what makes the
/// combination search's final tie-break key total.
#[derive(Debug, Clone, PartialEq)]
pub struct CommercialPrecursorCatalog {
    offers: Vec<CommercialPrecursorOffer>,
}

// Split across three `impl` blocks, each in the file its logic actually
// belongs to (this keeps the dependency direction one-way: `loader.rs`
// and `matching.rs` both depend on `model.rs`'s type definitions, never
// the reverse). Legal Rust -- no orphan-rule restriction on inherent
// impls within one crate -- but grep for `impl CommercialPrecursorCatalog`
// will only find one third; an LSP "go to method" aggregates all three.
// - Construction/accessors: here.
// - `load_csv`/`load_json`: `loader.rs` (their implementation, `load_csv_impl`/
//   `load_json_impl`, already lives there -- keeping the thin `impl`
//   wrapper in the same file avoids model.rs needing to import loader.rs
//   at all).
// - `offers_matching`: `matching.rs` (its canonical-ratio logic belongs
//   there).
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

    pub fn offers(&self) -> &[CommercialPrecursorOffer] {
        &self.offers
    }

    pub fn get(&self, id: &CommercialOfferId) -> Option<&CommercialPrecursorOffer> {
        self.offers.iter().find(|o| &o.offer_id == id)
    }
}

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

/// Named procurement-combination ranking policies (Phase 24C), selected via
/// `assess_commercial_precursors_with_policy`/`assess_commercial_plans_with_policy`.
/// The plain `assess_commercial_precursors`/`assess_commercial_plans` always
/// use `Balanced`, unchanged from their pre-24C behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommercialRankingPolicy {
    /// Today's original ranking: fewest unresolved fields, then cheapest,
    /// then shortest lead time, then highest purity, then deterministic
    /// name tiebreaks.
    #[default]
    Balanced,
    /// Cheapest first; the remaining `Balanced` dimensions (unresolved
    /// count, lead time, purity, name tiebreaks) apply in their original
    /// order as tiebreakers.
    CostFirst,
    /// Shortest lead time first, then the remaining `Balanced` dimensions
    /// as tiebreakers.
    LeadTimeFirst,
    /// Highest purity first, then the remaining `Balanced` dimensions as
    /// tiebreakers.
    PurityFirst,
    /// Identical to `Balanced` -- unresolved-field count is already
    /// `Balanced`'s primary key, so there is no distinct metric to promote.
    /// Kept as its own named variant because ROADMAP names it explicitly;
    /// do not "fix" this into a fabricated distinct metric.
    MinimumUnresolvedData,
    /// The non-dominated set over (cost, lead time, purity, excess
    /// purchased mass) -- minimize cost/lead-time/excess-mass, maximize
    /// purity. A combination missing any of the 4 dimensions (including a
    /// cost that isn't currency-comparable) is excluded from the frontier
    /// and reported via a summary warning, never silently dropped.
    Pareto,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CommercialWarning {
    pub message: String,
    pub severity: WarningSeverity,
}

/// `Serialize` only, deliberately no `Deserialize`: `unresolved_fields`
/// is `Vec<&'static str>`, which cannot deserialize into a non-`'static`
/// borrow from an arbitrary input buffer (the general reason every
/// `&'static str`-bearing type below is Serialize-only, matching this
/// crate's existing precedent for output-only report types --
/// `reaction.rs`/`process.rs`/`thermodynamics.rs`/`score.rs`).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CommercialOfferSelection {
    pub precursor: PrecursorId,
    pub precursor_composition: Composition,
    pub reaction_coefficient: u64,
    pub offer_id: CommercialOfferId,
    /// Stoichiometric theoretical requirement -- purity-agnostic, always
    /// computable from the plan alone. Never a yield claim, never adjusted
    /// for process loss or weighing margin.
    pub theoretical_pure_mass_required_grams: f64,
    /// The selected offer's own purity, carried through so
    /// `CommercialCombination.min_purity` is traceable to the selection
    /// that produced it. `None` if the offer's purity is unknown.
    pub purity: Option<PurityFraction>,
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

/// `Serialize` only -- see `CommercialOfferSelection`'s doc comment
/// (`selections` transitively carries its `&'static str` fields).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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
    /// The lowest purity among this combination's selections. `None` if any
    /// selection's purity is unknown (same convention as
    /// `max_lead_time_days`).
    pub min_purity: Option<PurityFraction>,
    /// Sum of every selection's `excess_mass_grams`. `None` if any
    /// selection's excess mass is unknown.
    pub total_excess_mass_grams: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SearchBudgetSummary {
    pub combinations_evaluated: usize,
    pub combinations_omitted: u64,
    /// `false` if the evaluation budget was hit *or* any row was truncated
    /// by `max_offers_per_precursor` -- either one means the result set is
    /// not a complete accounting of every possible combination.
    pub is_exhaustive: bool,
}

/// `Serialize` only -- see `CommercialOfferSelection`'s doc comment
/// (`field` itself is `&'static str`).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct UnresolvedCommercialField {
    pub precursor: PrecursorId,
    pub offer_id: CommercialOfferId,
    pub field: &'static str,
}

/// `Serialize` only -- see `CommercialOfferSelection`'s doc comment
/// (`combinations`/`unresolved_commercial_fields` transitively carry
/// `&'static str` fields).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commercial_catalog::test_support::*;

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
    fn particle_size_range_rejects_non_finite_negative_and_inverted() {
        assert!(ParticleSizeRangeUm::new(f64::NAN, 10.0).is_err());
        assert!(ParticleSizeRangeUm::new(-1.0, 10.0).is_err());
        assert!(ParticleSizeRangeUm::new(10.0, 5.0).is_err());
        assert!(ParticleSizeRangeUm::new(1.0, 10.0).is_ok());
    }

    #[test]
    fn currency_code_requires_three_uppercase_ascii_letters() {
        assert!(CurrencyCode::new("USD").is_ok());
        assert!(CurrencyCode::new("us").is_err());
        assert!(CurrencyCode::new("usd").is_err());
        assert!(CurrencyCode::new("USDD").is_err());
    }

    #[test]
    fn column_map_rejects_an_unknown_canonical_name() {
        let map = BTreeMap::from([("formulla".to_string(), "Chemical Formula".to_string())]);
        assert!(matches!(
            CommercialCatalogColumnMap::new(map),
            Err(CommercialCatalogError::InvalidColumnMap { .. })
        ));
    }

    #[test]
    fn column_map_rejects_two_canonical_names_claiming_the_same_header() {
        let map = BTreeMap::from([
            ("formula".to_string(), "Product".to_string()),
            ("product_name".to_string(), "Product".to_string()),
        ]);
        assert!(matches!(
            CommercialCatalogColumnMap::new(map),
            Err(CommercialCatalogError::InvalidColumnMap { .. })
        ));
    }

    #[test]
    fn column_map_accepts_a_valid_partial_map() {
        let map = BTreeMap::from([
            ("formula".to_string(), "Chemical Formula".to_string()),
            ("manufacturer".to_string(), "Supplier".to_string()),
        ]);
        let column_map = CommercialCatalogColumnMap::new(map).unwrap();
        let by_external = column_map.canonical_by_external();
        assert_eq!(by_external.get("Chemical Formula"), Some(&"formula"));
        assert_eq!(by_external.get("Supplier"), Some(&"manufacturer"));
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
    fn money_checked_arithmetic_at_the_u64_max_boundary() {
        // Extends the overflow test above with boundary-adjacent cases,
        // not just far-past-overflow ones: values that land exactly on
        // u64::MAX must still succeed, and the smallest possible overflow
        // (one past the max) must still be None, not panic or wrap.
        let usd = CurrencyCode::new("USD").unwrap();
        assert_eq!(
            Money::new(u64::MAX, usd).checked_mul_quantity(1),
            Some(Money::new(u64::MAX, usd)),
            "multiplying by 1 at the exact max must succeed"
        );
        assert_eq!(
            Money::new(u64::MAX - 1, usd).checked_add(&Money::new(1, usd)),
            Some(Money::new(u64::MAX, usd)),
            "summing to exactly the max must succeed"
        );
        assert_eq!(
            Money::new(u64::MAX, usd).checked_add(&Money::new(1, usd)),
            None,
            "one past the max must overflow to None, not wrap"
        );
        let half_plus_one = u64::MAX / 2 + 1;
        assert_eq!(
            Money::new(half_plus_one, usd).checked_mul_quantity(2),
            None,
            "just over half the max, doubled, must overflow"
        );
    }

    #[test]
    fn cas_number_checksum_validates_water_and_rejects_a_bad_checksum() {
        assert!(CasNumber::new("7732-18-5").checksum_verified); // water
        assert!(!CasNumber::new("7732-18-4").checksum_verified);
        assert!(!CasNumber::new("not-a-cas-number").checksum_verified);
    }

    // -- catalog --

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
}
