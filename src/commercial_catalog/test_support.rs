//! Shared test fixture builders for commercial_catalog's per-file test
//! modules. `pub(crate)` so sibling `#[cfg(test)] mod tests` blocks in
//! other files under this module can import via
//! `use crate::commercial_catalog::test_support::*;`.
#![cfg(test)]

use super::formula::parse_formula;
use super::model::{
    AvailabilityStatus, CommercialOfferId, CommercialPlanningConfig, CommercialPrecursorCatalog,
    CommercialPrecursorOffer, CommercialSourceType, CurrencyCode, Money, OfferProvenance,
    PackageMass, PurityFraction,
};
use crate::composition::{Composition, Element};
use crate::precursor::PrecursorId;
use std::collections::BTreeSet;

pub(crate) fn element(symbol: &str) -> Element {
    Element::new(symbol).unwrap()
}

pub(crate) fn composition(pairs: &[(&str, f64)]) -> Composition {
    Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
}

pub(crate) fn offer(id: &str, formula: &str) -> CommercialPrecursorOffer {
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

pub(crate) fn money(minor_units: u64, currency: &str) -> Money {
    Money::new(minor_units, CurrencyCode::new(currency).unwrap())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn priced_offer(
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

pub(crate) fn barium_titanate_plan() -> crate::report::SynthesisPlan {
    use crate::config::PlanningConfig;
    use crate::planner::Planner;
    use crate::precursor::{AvailabilityMetadata, InMemoryPrecursorCatalog, PrecursorCandidate};
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

pub(crate) fn baco3_tio2_catalog(
    offers: Vec<CommercialPrecursorOffer>,
) -> CommercialPrecursorCatalog {
    CommercialPrecursorCatalog::from_offers(offers).0
}

pub(crate) fn default_baco3_tio2_offers() -> Vec<CommercialPrecursorOffer> {
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

pub(crate) fn large_baco3_tio2_catalog(n: u64) -> CommercialPrecursorCatalog {
    let mut offers = Vec::new();
    for i in 0..n {
        offers.push(priced_offer(
            &format!("BACO3-{i}"),
            "BaCO3",
            "Example Materials Ltd.",
            Some(0.9),
            Some(100.0),
            Some((1000 + i, "USD")),
            Some(5),
            Some(AvailabilityStatus::InStock),
        ));
        offers.push(priced_offer(
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
    baco3_tio2_catalog(offers)
}

pub(crate) fn heuristic_tier_config() -> CommercialPlanningConfig {
    CommercialPlanningConfig {
        max_combinations_evaluated: 50,
        max_results_returned: 5,
        ..CommercialPlanningConfig::default()
    }
}
