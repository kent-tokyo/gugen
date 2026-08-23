//! CSV/JSON catalog loading: row-level parsing and validation, funneling
//! into `CommercialPrecursorCatalog::from_offers` (`model.rs`) for the one
//! true construction path (sort, dedup, load report).

use super::formula::parse_formula;
use super::model::{
    CasNumber, CommercialCatalogColumnMap, CommercialCatalogError, CommercialCatalogLoadMode,
    CommercialCatalogLoadReport, CommercialOfferId, CommercialPrecursorCatalog,
    CommercialPrecursorOffer, CurrencyCode, Money, OPTIONAL_CSV_COLUMNS, OfferProvenance,
    PackageMass, ParticleSizeRangeUm, PurityFraction, REQUIRED_CSV_COLUMNS, RejectedOffer,
    parse_availability, parse_source_type,
};
use crate::error::ProviderError;
use std::collections::{BTreeMap, BTreeSet};

// Third of `CommercialPrecursorCatalog`'s three `impl` blocks -- see the
// cross-referencing comment on `model.rs`'s own `impl` block for why the
// type is split this way. Kept here specifically so `model.rs` never
// needs to import anything from `loader.rs`: the dependency runs one way
// (`loader.rs` depends on `model.rs`'s type definitions), not both.
impl CommercialPrecursorCatalog {
    pub fn load_csv(
        csv_text: &str,
        mode: CommercialCatalogLoadMode,
    ) -> std::result::Result<(CommercialPrecursorCatalog, CommercialCatalogLoadReport), ProviderError>
    {
        load_csv_impl(csv_text, mode, None)
    }

    /// Like `load_csv`, but first rewrites the header row per `column_map`
    /// (canonical name -> the header name this file actually uses) --
    /// lets a real-world export with non-standard headers (e.g. `Chemical
    /// Formula` instead of `formula`) load without a hand-edited file.
    /// Every row is parsed identically to `load_csv` afterward.
    pub fn load_csv_with_column_map(
        csv_text: &str,
        mode: CommercialCatalogLoadMode,
        column_map: &CommercialCatalogColumnMap,
    ) -> std::result::Result<(CommercialPrecursorCatalog, CommercialCatalogLoadReport), ProviderError>
    {
        load_csv_impl(csv_text, mode, Some(column_map))
    }

    #[cfg(feature = "serde")]
    pub fn load_json(
        json_text: &str,
        mode: CommercialCatalogLoadMode,
    ) -> std::result::Result<(CommercialPrecursorCatalog, CommercialCatalogLoadReport), ProviderError>
    {
        load_json_impl(json_text, mode)
    }
}

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

/// Rewrites `headers` per `column_map`, so every downstream lookup in
/// `parse_csv_offer_row` (and the required-column check above) sees
/// canonical names regardless of what the file's own header row says.
/// Only flags a collision that the map itself introduces -- two cells
/// resolving to the same canonical name where at least one was actually
/// rewritten -- never a collision between two cells the map left alone.
/// `load_csv` without a column map already tolerates arbitrary duplicate
/// headers (e.g. duplicate blank headers from trailing-comma exports, or
/// even a hand-duplicated non-required column); this must stay at least
/// that permissive, so a file that loads fine via `load_csv` must still
/// load fine via this function when the map happens not to touch the
/// colliding cells.
fn remap_headers(
    headers: &csv::StringRecord,
    column_map: &CommercialCatalogColumnMap,
) -> std::result::Result<csv::StringRecord, ProviderError> {
    let external_to_canonical = column_map.canonical_by_external();
    // (canonical-or-original name, was this cell rewritten by the map)
    let remapped: Vec<(String, bool)> = headers
        .iter()
        .map(|h| match external_to_canonical.get(h) {
            Some(canonical) => (canonical.to_string(), true),
            None => (h.to_string(), false),
        })
        .collect();

    let mut seen_canonical: BTreeMap<&str, bool> = BTreeMap::new();
    for (h, rewritten) in &remapped {
        let is_canonical = REQUIRED_CSV_COLUMNS.contains(&h.as_str())
            || OPTIONAL_CSV_COLUMNS.contains(&h.as_str());
        if !is_canonical {
            continue;
        }
        match seen_canonical.get(h.as_str()) {
            Some(&previously_rewritten) if *rewritten || previously_rewritten => {
                return Err(ProviderError::MalformedRecord(format!(
                    "column map: header '{h}' would appear more than once after applying the \
                    column map"
                )));
            }
            _ => {
                seen_canonical.insert(h.as_str(), *rewritten);
            }
        }
    }

    Ok(csv::StringRecord::from(
        remapped.into_iter().map(|(h, _)| h).collect::<Vec<_>>(),
    ))
}

pub(crate) fn load_csv_impl(
    csv_text: &str,
    mode: CommercialCatalogLoadMode,
    column_map: Option<&CommercialCatalogColumnMap>,
) -> std::result::Result<(CommercialPrecursorCatalog, CommercialCatalogLoadReport), ProviderError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(csv_text.as_bytes());
    let mut headers = reader
        .headers()
        .map_err(|e| ProviderError::MalformedRecord(format!("CSV header: {e}")))?
        .clone();

    if let Some(column_map) = column_map {
        headers = remap_headers(&headers, column_map)?;
    }

    for required_column in REQUIRED_CSV_COLUMNS {
        if !headers.iter().any(|h| h == *required_column) {
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
pub(crate) fn load_json_impl(
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

    // -- column-mapped CSV loading --

    #[test]
    fn load_csv_with_column_map_accepts_a_partially_renamed_header_row() {
        // Only `formula`/`manufacturer` are renamed; everything else keeps
        // its canonical name -- proves partial mapping, not all-or-nothing.
        let csv = "offer_id,Supplier,product_name,Chemical Formula,source\n\
            EML-1,Example Materials Ltd.,Demo Oxide,Fe2O3,synthetic_fixture\n";
        let column_map = CommercialCatalogColumnMap::new(BTreeMap::from([
            ("manufacturer".to_string(), "Supplier".to_string()),
            ("formula".to_string(), "Chemical Formula".to_string()),
        ]))
        .unwrap();
        let (catalog, report) = CommercialPrecursorCatalog::load_csv_with_column_map(
            csv,
            CommercialCatalogLoadMode::Strict,
            &column_map,
        )
        .unwrap();
        assert_eq!(report.accepted, 1);
        assert_eq!(catalog.offers()[0].manufacturer, "Example Materials Ltd.");
    }

    #[test]
    fn load_csv_with_column_map_still_hard_fails_on_a_missing_required_column() {
        let csv = "offer_id,Supplier,product_name,source\nEML-1,Example Materials Ltd.,Demo Oxide,synthetic_fixture\n";
        let column_map = CommercialCatalogColumnMap::new(BTreeMap::from([(
            "manufacturer".to_string(),
            "Supplier".to_string(),
        )]))
        .unwrap();
        let result = CommercialPrecursorCatalog::load_csv_with_column_map(
            csv,
            CommercialCatalogLoadMode::Strict,
            &column_map,
        );
        assert!(result.is_err());
    }

    #[test]
    fn load_csv_with_column_map_rejects_a_collision_after_remapping() {
        // The file already has a literal `formula` column, and the map
        // separately renames the `chem` column to `formula` too -- both
        // resolve to `formula`, an ambiguous first-match bind (the
        // collision involves a cell the map actually rewrote).
        let csv = "offer_id,manufacturer,product_name,formula,source,chem\n\
            EML-1,Example Materials Ltd.,Demo Oxide,Fe2O3,synthetic_fixture,TiO2\n";
        let column_map = CommercialCatalogColumnMap::new(BTreeMap::from([(
            "formula".to_string(),
            "chem".to_string(),
        )]))
        .unwrap();
        let result = CommercialPrecursorCatalog::load_csv_with_column_map(
            csv,
            CommercialCatalogLoadMode::Strict,
            &column_map,
        );
        assert!(result.is_err());
    }

    #[test]
    fn load_csv_with_column_map_does_not_error_on_a_pre_existing_duplicate_the_map_never_touches() {
        // `load_csv` (no map) has no duplicate-header check at all -- two
        // `notes` columns just means `position()`'s first match wins. The
        // column map must not become stricter than that for cells it never
        // rewrites; only a map-introduced collision (see the test above)
        // is an error.
        let csv = "offer_id,Supplier,product_name,formula,source,notes,notes\n\
            EML-1,Example Materials Ltd.,Demo Oxide,Fe2O3,synthetic_fixture,a,b\n";
        let column_map = CommercialCatalogColumnMap::new(BTreeMap::from([(
            "manufacturer".to_string(),
            "Supplier".to_string(),
        )]))
        .unwrap();
        let result = CommercialPrecursorCatalog::load_csv_with_column_map(
            csv,
            CommercialCatalogLoadMode::Strict,
            &column_map,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn load_csv_with_column_map_round_trips_every_canonical_column() {
        // Maps all 22 canonical names to distinct non-standard headers and
        // populates every field in one row -- catches drift between
        // `REQUIRED_CSV_COLUMNS`/`OPTIONAL_CSV_COLUMNS` and this file's own
        // `field(...)` call sites in `parse_csv_offer_row`.
        let all_canonical: Vec<&str> = REQUIRED_CSV_COLUMNS
            .iter()
            .chain(OPTIONAL_CSV_COLUMNS.iter())
            .copied()
            .collect();
        let external_of = |canonical: &str| format!("Col {canonical}");
        let column_map = CommercialCatalogColumnMap::new(
            all_canonical
                .iter()
                .map(|c| (c.to_string(), external_of(c)))
                .collect(),
        )
        .unwrap();

        let values: BTreeMap<&str, &str> = [
            ("offer_id", "EML-1"),
            ("manufacturer", "Example Materials Ltd."),
            ("product_name", "Demo Oxide"),
            ("formula", "Fe2O3"),
            ("source", "synthetic_fixture"),
            ("purity_fraction", "0.99"),
            ("package_mass_g", "500"),
            ("price_minor_units", "4500"),
            ("currency", "USD"),
            ("availability", "in_stock"),
            ("lead_time_days", "7"),
            ("particle_size_min_um", "1.0"),
            ("particle_size_max_um", "2.0"),
            ("cas_number", "7732-18-5"),
            ("tags", "a;b"),
            ("catalog_number", "CAT-1"),
            ("grade", "ACS"),
            ("physical_form", "powder"),
            ("country_region", "US"),
            ("product_url", "https://example.test/p"),
            ("notes", "note"),
            ("retrieved_at", "2026-08-14"),
        ]
        .into_iter()
        .collect();
        assert_eq!(values.len(), all_canonical.len());

        let header_row = all_canonical
            .iter()
            .map(|c| external_of(c))
            .collect::<Vec<_>>()
            .join(",");
        let value_row = all_canonical
            .iter()
            .map(|c| values[c])
            .collect::<Vec<_>>()
            .join(",");
        let csv = format!("{header_row}\n{value_row}\n");

        let (catalog, report) = CommercialPrecursorCatalog::load_csv_with_column_map(
            &csv,
            CommercialCatalogLoadMode::Strict,
            &column_map,
        )
        .unwrap();
        assert_eq!(report.accepted, 1);
        let offer = &catalog.offers()[0];
        assert_eq!(offer.offer_id.0, "EML-1");
        assert_eq!(offer.manufacturer, "Example Materials Ltd.");
        assert_eq!(offer.product_name, "Demo Oxide");
        assert_eq!(offer.formula, "Fe2O3");
        assert!(offer.purity.is_some());
        assert!(offer.package_mass.is_some());
        assert!(offer.unit_price.is_some());
        assert!(offer.availability.is_some());
        assert_eq!(offer.lead_time_days, Some(7));
        assert!(offer.particle_size_range_um.is_some());
        assert!(offer.cas_number.is_some());
        assert_eq!(offer.tags.len(), 2);
        assert!(offer.catalog_number.is_some());
        assert!(offer.grade.is_some());
        assert!(offer.physical_form.is_some());
        assert!(offer.country_region.is_some());
        assert!(offer.product_url.is_some());
        assert!(offer.notes.is_some());
        assert!(offer.provenance.retrieved_at.is_some());
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
}
