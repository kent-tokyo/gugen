//! Feature-gated integration test for the CSV catalog loader against the
//! real fixture files (mirrors `tests/materials_project_adapter.rs`'s
//! convention: feature-gating is done entirely via `Cargo.toml`'s
//! `required-features`, not `#[cfg(feature = ...)]` inside this file).

use gugen::{CommercialCatalogLoadMode, CommercialPrecursorCatalog};

const SAMPLE_CSV: &str = include_str!("fixtures/commercial_catalog_sample.csv");
const BROKEN_CSV: &str = include_str!("fixtures/commercial_catalog_sample_broken.csv");

#[test]
fn the_synthetic_sample_catalog_loads_cleanly_in_strict_mode() {
    let (catalog, report) =
        CommercialPrecursorCatalog::load_csv(SAMPLE_CSV, CommercialCatalogLoadMode::Strict)
            .unwrap();
    assert!(report.rejected.is_empty());
    assert_eq!(report.accepted, 7);
    assert_eq!(catalog.offers().len(), 7);
}

#[test]
fn the_broken_sibling_fixture_fails_strict_mode_on_the_first_bad_row() {
    let result =
        CommercialPrecursorCatalog::load_csv(BROKEN_CSV, CommercialCatalogLoadMode::Strict);
    assert!(result.is_err());
}

#[test]
fn the_broken_sibling_fixture_reports_every_bad_row_in_lenient_mode() {
    let (catalog, report) =
        CommercialPrecursorCatalog::load_csv(BROKEN_CSV, CommercialCatalogLoadMode::Lenient)
            .unwrap();
    assert_eq!(catalog.offers().len(), 1, "only EML-OK should survive");
    assert_eq!(report.rejected.len(), 3);
    let fields: Vec<&str> = report.rejected.iter().map(|r| r.field.as_str()).collect();
    assert!(fields.contains(&"formula"));
    assert!(fields.contains(&"purity_fraction"));
    assert!(fields.contains(&"price_minor_units"));
}
