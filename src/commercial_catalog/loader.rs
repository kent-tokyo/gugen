//! CSV/JSON catalog loading: row-level parsing and validation, funneling
//! into `CommercialPrecursorCatalog::from_offers` (`model.rs`) for the one
//! true construction path (sort, dedup, load report).

use super::formula::parse_formula;
use super::model::{
    CasNumber, CommercialCatalogError, CommercialCatalogLoadMode, CommercialCatalogLoadReport,
    CommercialOfferId, CommercialPrecursorCatalog, CommercialPrecursorOffer, CurrencyCode, Money,
    OfferProvenance, PackageMass, ParticleSizeRangeUm, PurityFraction, RejectedOffer,
    parse_availability, parse_source_type,
};
use crate::error::ProviderError;
use std::collections::BTreeSet;

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

pub(crate) fn load_csv_impl(
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
