// A tiny, fictional commercial catalog -- mirrors
// tests/fixtures/commercial_catalog_sample.json exactly (transcribed, not
// imported: tests/ isn't part of the published crate or reachable from
// JS). Every offer's own "notes" field already says "Fictional fixture
// data only." -- this is not real supplier pricing or availability, and
// only covers the BaTiO3 example's precursors (BaCO3, TiO2). Passed as
// raw JSON text to `assess_commercial`, which is gugen's own
// commercial_catalog feature parsing it -- no catalog-parsing logic is
// reimplemented here.

export const CATALOG_JSON = JSON.stringify({
  offers: [
    {
      offer_id: "EML-BACO3-99-JSON",
      manufacturer: "Example Materials Ltd.",
      product_name: "Demo Oxide Grade A Barium Carbonate",
      formula: "BaCO3",
      source_type: "synthetic_fixture",
      source_identifier: "EML-BACO3-99-JSON",
      catalog_number: "TC-BACO3-99",
      cas_number: "513-77-9",
      grade: "ACS",
      purity: 0.99,
      package_mass_g: 500.0,
      price_minor_units: 4550,
      currency: "USD",
      availability: "in_stock",
      lead_time_days: 5,
      physical_form: "powder",
      particle_size_min_um: 1.0,
      particle_size_max_um: 10.0,
      product_url: "https://example.invalid/products/baco3-99",
      retrieved_at: "2026-08-01",
      tags: ["oxide", "ceramic-precursor"],
      notes: "Fictional fixture data only.",
    },
    {
      offer_id: "EML-TIO2-995-JSON",
      manufacturer: "Example Materials Ltd.",
      product_name: "Demo Oxide Grade A Titanium Dioxide Anatase",
      formula: "TiO2",
      source_type: "synthetic_fixture",
      source_identifier: "EML-TIO2-995-JSON",
      catalog_number: "TC-TIO2-995",
      cas_number: "13463-67-7",
      grade: "ACS",
      purity: 0.995,
      package_mass_g: 500.0,
      price_minor_units: 6200,
      currency: "USD",
      availability: "in_stock",
      lead_time_days: 3,
      physical_form: "powder",
      tags: ["oxide", "ceramic-precursor"],
      notes: "Fictional fixture data only.",
    },
  ],
});
