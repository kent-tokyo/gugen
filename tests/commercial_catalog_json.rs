//! Feature-gated integration test for the JSON catalog loader against the
//! real fixture file. `required-features = ["commercial_catalog", "serde"]`
//! in Cargo.toml is the sole gating mechanism.

use gugen::{CommercialCatalogLoadMode, CommercialPrecursorCatalog};

const SAMPLE_JSON: &str = include_str!("fixtures/commercial_catalog_sample.json");

#[test]
fn the_synthetic_sample_catalog_loads_cleanly_from_json() {
    let (catalog, report) =
        CommercialPrecursorCatalog::load_json(SAMPLE_JSON, CommercialCatalogLoadMode::Strict)
            .unwrap();
    assert!(report.rejected.is_empty());
    assert_eq!(catalog.offers().len(), 2);
    let baco3 = catalog
        .offers()
        .iter()
        .find(|o| o.offer_id.0 == "EML-BACO3-99-JSON")
        .unwrap();
    assert_eq!(baco3.purity.map(|p| p.value()), Some(0.99));
    assert_eq!(baco3.tags.len(), 2);
}

#[test]
fn a_json_row_with_a_malformed_purity_type_is_rejected_in_lenient_mode() {
    let json = r#"{"offers": [
        {"offer_id": "BAD", "manufacturer": "Example Materials Ltd.", "product_name": "Broken",
         "formula": "TiO2", "source_type": "synthetic_fixture", "purity": "not-a-number"}
    ]}"#;
    let (catalog, report) =
        CommercialPrecursorCatalog::load_json(json, CommercialCatalogLoadMode::Lenient).unwrap();
    assert!(catalog.offers().is_empty());
    assert_eq!(report.rejected.len(), 1);
}
