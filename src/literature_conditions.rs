use crate::composition::{Composition, Element};
use crate::error::ProviderError;
use crate::evidence::{EvidenceKind, EvidenceScope, EvidenceStrength};
use crate::precursor::{PrecursorId, PrecursorSelection};
use crate::process::{
    Atmosphere, ConditionPrecedent, DurationRange, HeatingPurpose, ProcessPrecedent,
    TemperatureRange,
};
use crate::provider::ProcessEvidenceProvider;
use crate::target::TargetSpecification;
use std::collections::BTreeSet;

/// One hand-verified literature condition record (AGENTS.md §21.3: never
/// authored from memory). `target`/`precursor_ids` identify which route
/// this applies to; `conditions` carries the actual per-step temperature/
/// duration/atmosphere data, each traceable to a real citation via its own
/// `ConditionPrecedent.source_id`.
#[derive(Debug, Clone)]
pub struct CuratedConditionRecord {
    pub target: Composition,
    pub precursor_ids: BTreeSet<PrecursorId>,
    pub conditions: Vec<ConditionPrecedent>,
}

/// `ProcessEvidenceProvider` backed by a small, hand-verified set of real,
/// cited firing conditions (Phase 10), expanded
/// from the same 5 literature-cited routes `tests/validation.rs` already
/// uses for precursor-set recovery. Not a general literature-mining
/// pipeline: that's a different trust tier (bulk statistical corpus vs.
/// individually verified citable evidence), left to a future large-scale
/// benchmark rather than mixed into this small, hand-checked set.
pub struct InMemoryLiteratureConditionProvider {
    records: Vec<CuratedConditionRecord>,
}

impl InMemoryLiteratureConditionProvider {
    /// Backed by gugen's own curated, hand-verified condition records.
    pub fn from_curated_records() -> Self {
        Self {
            records: curated_records(),
        }
    }
}

impl ProcessEvidenceProvider for InMemoryLiteratureConditionProvider {
    /// Matching is exact `Composition` equality (no epsilon/fuzzy
    /// matching) against a record's `target`. A record's conditions are
    /// marked `EvidenceScope::ExactTarget` when the queried precursor-ID
    /// set also equals the record's exactly, `SimilarMaterial` otherwise
    /// (the record's conditions still describe a real route to this
    /// target, but via a different precursor combination than the one the
    /// citation itself reports).
    fn precedents(
        &self,
        target: &TargetSpecification,
        precursors: &[PrecursorSelection],
    ) -> std::result::Result<Vec<ProcessPrecedent>, ProviderError> {
        let queried_ids: BTreeSet<PrecursorId> =
            precursors.iter().map(|p| p.precursor.clone()).collect();
        let mut out = Vec::new();
        for record in &self.records {
            if record.target != target.composition {
                continue;
            }
            let exact_route = record.precursor_ids == queried_ids;
            let conditions: Vec<ConditionPrecedent> = record
                .conditions
                .iter()
                .cloned()
                .map(|mut c| {
                    c.applicable_to = if exact_route {
                        EvidenceScope::ExactTarget
                    } else {
                        EvidenceScope::SimilarMaterial
                    };
                    c
                })
                .collect();
            out.push(ProcessPrecedent {
                description: String::new(),
                conditions,
            });
        }
        Ok(out)
    }
}

fn element(symbol: &str) -> Element {
    Element::new(symbol).expect("curated record uses a valid IUPAC element symbol")
}

fn composition(pairs: &[(&str, f64)]) -> Composition {
    Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt)))
        .expect("curated record uses a valid, finite composition")
}

fn precursor_ids(ids: &[&str]) -> BTreeSet<PrecursorId> {
    ids.iter().map(|id| PrecursorId(id.to_string())).collect()
}

/// Hand-verified against each paper's own text (AGENTS.md §21.3), not
/// authored from memory or "typical" values, and each fetched/read live
/// this phase, not recalled from training data. For MgAl2O4 and CaO, the
/// DOI is the same representative citation `tests/validation.rs`'s
/// corresponding `LiteratureFixture` already uses for precursor-set
/// recovery. For LaAlO3, that representative DOI turned out to be
/// inaccessible (fully paywalled, no copy found anywhere), so a
/// different, freely-accessible, independently verified paper is cited
/// for condition data specifically here, while `tests/validation.rs`'s
/// own citation keeps its original DOI (still what the Kononova dataset
/// attributes the precursor-recovery route to -- an access problem, not
/// a content mismatch).
///
/// For Zn3(PO4)2 and BaTiO3, the DOI `tests/validation.rs` originally
/// cited turned out on inspection to be a **confirmed topic mismatch** --
/// a different material/process than the fixture's citation text
/// described (Zn3(PO4)2's DOI is a glass paper made by melt-quenching;
/// BaTiO3's DOI is a NaNbO3-BaTiO3 solid-solution study). This finding
/// (made while sourcing condition data for this phase) directly informed
/// Phase 14's later fix to `tests/validation.rs` itself: BaTiO3's
/// representative DOI there now cites this same replacement paper
/// (10.3390/cryst14040304), and the Zn3(PO4)2 fixture was replaced
/// entirely with a different, better-attested phosphate target (LiFePO4)
/// once recounting against the correctly-licensed corpus found the
/// ZnO + P2O5 route has zero independent attestations there. This curated
/// record below still keeps its own Zn3(PO4)2 entry (a real, if `Weak`,
/// condition source for that target) even though `tests/validation.rs`
/// no longer has a same-named fixture -- the two files serve different
/// purposes and are not required to name the same targets. Zn3(PO4)2's
/// condition source here reports a different precursor route (ZnO +
/// (NH4)2HPO4, not ZnO + P2O5) -- recorded as what the paper actually
/// used, not force-fit to match any fixture;
/// `InMemoryLiteratureConditionProvider` correctly downgrades this to
/// `EvidenceScope::SimilarMaterial` rather than `ExactTarget` when queried
/// against a different precursor combination for the same target. A field
/// left `None` means the source paper genuinely doesn't state it (e.g.
/// atmosphere, some durations), never a filled-in "presumably air" guess
/// -- AGENTS.md §4's guardrail 1 applies here exactly as it does to the
/// generator itself.
fn curated_records() -> Vec<CuratedConditionRecord> {
    vec![
        // MgAl2O4 spinel, MgO + Al2O3 -> MgAl2O4 (no byproduct, so
        // gugen's template for this route has exactly one Heat step:
        // Sintering, no Calcination).
        //
        // Abdeyazdan, Dogan, Rhamdhani, Chapman, Monaghan, "Dynamic
        // Wetting of CaO-Al2O3-SiO2-MgO Liquid Oxide on MgAl2O4 Spinel,"
        // Metall. Mater. Trans. B 46(1), 208-219 (2015), DOI
        // 10.1007/s11663-014-0207-8 -- the same representative DOI
        // tests/validation.rs cites for this fixture's precursor route.
        // Section II (Experimental), open-access author copy via
        // University of Wollongong Research Online: the spinel substrate
        // was made by reaction sintering, "mixed and then pressed into
        // disks and sintered at 1873 K (1600 C) for 24 hours," then
        // "crushed to a fine powder ... and re-sintered at 1998 K
        // (1725 C) for 6 hours." Two real stages are reported; this
        // record represents the second (final, reported as the
        // completing) stage as `Sintering`, since gugen's template has
        // only one Heat step for a byproduct-free oxide route and the
        // second stage is the one that finishes densification -- the
        // first stage's 1600 C/24 h is named here, not silently dropped,
        // so a reader can judge that choice. Atmosphere is not stated for
        // either sintering stage in that paragraph (the paper does state
        // Ar for a separate, later wetting-test furnace -- not this
        // synthesis step, and not conflated with it here).
        CuratedConditionRecord {
            target: composition(&[("Mg", 1.0), ("Al", 2.0), ("O", 4.0)]),
            precursor_ids: precursor_ids(&["MgO", "Al2O3"]),
            conditions: vec![ConditionPrecedent {
                purpose: HeatingPurpose::Sintering,
                temperature: Some(TemperatureRange::new(1725.0, 1725.0).unwrap()),
                duration: Some(DurationRange::new(6.0, 6.0).unwrap()),
                atmosphere: None,
                ramp: None,
                evidence_kind: EvidenceKind::CuratedLiteratureRecord,
                source_id: Some("10.1007/s11663-014-0207-8".to_string()),
                statement: "MgAl2O4 spinel reaction-sintered from MgO + Al2O3: pressed \
                    and sintered at 1600 C for 24 h, crushed, then re-sintered at \
                    1725 C for 6 h (this record uses the second, completing stage; \
                    the first stage's 1600 C/24 h is not separately represented \
                    since this route's template has only one Heat step)"
                    .to_string(),
                strength: EvidenceStrength::Moderate,
                applicable_to: EvidenceScope::ExactTarget,
            }],
        },
        // CaO, CaCO3 -> CaO + CO2 (releases CO2, so gugen's template adds
        // a Calcination step -- and this decomposition IS a calcination
        // in the literal sense, a clean match).
        //
        // Seesanong, Seangarun, Boonchom, Laohavisuti, Boonmee, Thompho,
        // Rungrojchaipon, "Low-Cost and Eco-Friendly Calcium Oxide
        // Prepared via Thermal Decompositions of Calcium Carbonate and
        // Calcium Acetate Precursors Derived from Waste Oyster Shells,"
        // Materials 17(15), 3875 (2024), DOI 10.3390/ma17153875 -- the
        // same DOI tests/validation.rs's CaO fixture already cites.
        // Materials and Methods (full text, PMC11313493, open access):
        // "20 g of a raw agent in the crucible was calcined by a furnace
        // for 1 h," with the reaction scheme itself labeled "900 C:
        // CaCO3(s) -> CaO(s) + CO2(g)." Atmosphere is not stated for this
        // bulk calcination (the paper's TGA reference run separately used
        // flowing N2, but that's a small-scale analytical measurement, not
        // this synthesis step -- not conflated here).
        CuratedConditionRecord {
            target: composition(&[("Ca", 1.0), ("O", 1.0)]),
            precursor_ids: precursor_ids(&["CaCO3"]),
            conditions: vec![ConditionPrecedent {
                purpose: HeatingPurpose::Calcination,
                temperature: Some(TemperatureRange::new(900.0, 900.0).unwrap()),
                duration: Some(DurationRange::new(1.0, 1.0).unwrap()),
                atmosphere: None,
                ramp: None,
                evidence_kind: EvidenceKind::CuratedLiteratureRecord,
                source_id: Some("10.3390/ma17153875".to_string()),
                statement: "CaCO3 calcined to CaO + CO2 at 900 C for 1 h".to_string(),
                strength: EvidenceStrength::Strong,
                applicable_to: EvidenceScope::ExactTarget,
            }],
        },
        // LaAlO3, La2O3 + Al2O3 -> LaAlO3 (no byproduct, one Heat step:
        // Sintering). tests/validation.rs's representative DOI
        // (10.1149/2.053405jes) is the right paper but fully paywalled,
        // no accessible copy found anywhere -- this uses a different,
        // freely-accessible paper instead, verified independently.
        //
        // Jakka, Silva, Soares, Pavani, "Exploring the potential of Eu3+
        // and Mn4+ activated LaAlO3 phosphors as red and far-red emitters
        // for horticulture lighting," RSC Advances 13(45), 31314-31320
        // (2023), DOI 10.1039/d3ra03241h, open access (PMC10600514). The
        // undoped (x=0, y=0) sample is La2O3 + Al2O3 only: "sintered at
        // 1500 C for 5 h in a muffle furnace." Single-step firing, no
        // separate calcination reported. Atmosphere not stated.
        CuratedConditionRecord {
            target: composition(&[("La", 1.0), ("Al", 1.0), ("O", 3.0)]),
            precursor_ids: precursor_ids(&["La2O3", "Al2O3"]),
            conditions: vec![ConditionPrecedent {
                purpose: HeatingPurpose::Sintering,
                temperature: Some(TemperatureRange::new(1500.0, 1500.0).unwrap()),
                duration: Some(DurationRange::new(5.0, 5.0).unwrap()),
                atmosphere: None,
                ramp: None,
                evidence_kind: EvidenceKind::CuratedLiteratureRecord,
                source_id: Some("10.1039/d3ra03241h".to_string()),
                statement: "LaAlO3 solid-state reaction from La2O3 + Al2O3 (undoped x=0,y=0 \
                    sample), sintered at 1500 C for 5 h in a muffle furnace"
                    .to_string(),
                strength: EvidenceStrength::Moderate,
                applicable_to: EvidenceScope::ExactTarget,
            }],
        },
        // Zn3(PO4)2, ZnO + (NH4)2HPO4 -> Zn3(PO4)2 (releases NH3/H2O, so
        // both Calcination and Sintering are reported). Note: this is a
        // DIFFERENT precursor combination than tests/validation.rs's
        // fixture (ZnO + P2O5) -- the P2O5 route's representative DOI
        // (10.1016/j.jmmm.2015.06.001) turned out to be a confirmed topic
        // mismatch (a Sm-doped zinc phosphate glass paper, melt-quenched,
        // not this reaction at all). Recorded as the real route this
        // source actually used, not force-fit to ZnO + P2O5 --
        // `InMemoryLiteratureConditionProvider` correctly marks this
        // `SimilarMaterial`, not `ExactTarget`, when queried against a
        // ZnO + P2O5 plan.
        //
        // El azizi, Salhi, Elouafi, Tizliouine, Ezairi, "Photoluminescence
        // and Refractive Index Dispersion Properties of Zn3-xMx(PO4)2
        // (M=Co, Ni; x=1) Nanoparticles," Engineering Proceedings 67(1),
        // 18 (2024), DOI 10.3390/engproc2024067018, open access (CC BY).
        // The undoped host (x=0) is ZnO + (NH4)2HPO4 only: "fired at
        // 500 C for 3 h in air. After being reground, they were sintered
        // at 950 C in air for 5 h." A short (~3 page) conference-
        // proceedings-tier paper, not a full journal article, and its own
        // reported space group (Pnma, the hydrate hopeite family) is
        // inconsistent with known anhydrous Zn3(PO4)2 polymorphs
        // (monoclinic C2/c or P21/c) -- flagged here and reflected in a
        // `Weak` strength rather than `Moderate`, not silently treated as
        // equally reliable as the other four records.
        CuratedConditionRecord {
            target: composition(&[("Zn", 3.0), ("P", 2.0), ("O", 8.0)]),
            precursor_ids: precursor_ids(&["ZnO", "(NH4)2HPO4"]),
            conditions: vec![
                ConditionPrecedent {
                    purpose: HeatingPurpose::Calcination,
                    temperature: Some(TemperatureRange::new(500.0, 500.0).unwrap()),
                    duration: Some(DurationRange::new(3.0, 3.0).unwrap()),
                    atmosphere: Some(Atmosphere::Air),
                    ramp: None,
                    evidence_kind: EvidenceKind::CuratedLiteratureRecord,
                    source_id: Some("10.3390/engproc2024067018".to_string()),
                    statement: "Zn3(PO4)2 from ZnO + (NH4)2HPO4 (undoped x=0 host), fired \
                        at 500 C for 3 h in air; source is a short conference-proceedings \
                        paper with an internally inconsistent reported space group -- \
                        weighed accordingly"
                        .to_string(),
                    strength: EvidenceStrength::Weak,
                    applicable_to: EvidenceScope::ExactTarget,
                },
                ConditionPrecedent {
                    purpose: HeatingPurpose::Sintering,
                    temperature: Some(TemperatureRange::new(950.0, 950.0).unwrap()),
                    duration: Some(DurationRange::new(5.0, 5.0).unwrap()),
                    atmosphere: Some(Atmosphere::Air),
                    ramp: None,
                    evidence_kind: EvidenceKind::CuratedLiteratureRecord,
                    source_id: Some("10.3390/engproc2024067018".to_string()),
                    statement: "Zn3(PO4)2 from ZnO + (NH4)2HPO4 (undoped x=0 host), \
                        reground and sintered at 950 C for 5 h in air; source is a short \
                        conference-proceedings paper with an internally inconsistent \
                        reported space group -- weighed accordingly"
                        .to_string(),
                    strength: EvidenceStrength::Weak,
                    applicable_to: EvidenceScope::ExactTarget,
                },
            ],
        },
        // BaTiO3, BaCO3 + TiO2 -> BaTiO3 + CO2 (releases CO2, both
        // Calcination and Sintering are reported). Originally cited here
        // because tests/validation.rs's own representative DOI at the time
        // (10.1111/j.1551-2916.2006.01172.x) turned out to be a confirmed
        // topic mismatch (a NaNbO3-BaTiO3 solid-solution study, not plain
        // BaTiO3) -- Phase 14 later adopted this same replacement DOI as
        // tests/validation.rs's own representative entry too, so the two
        // files now agree, not just coincidentally match.
        //
        // Qi, Peng, Bi, Zhang, Su, Li, Zhang, Zhang, Zhou, Zhang, Cao,
        // "The Effect of Sputtering Target Density on the Crystal and
        // Electronic Structure of Epitaxial BaTiO3 Thin Films," Crystals
        // 14(4), 304 (2024), DOI 10.3390/cryst14040304, open access (CC
        // BY). "TiO2 ... and BaCO3 ... powders were mixed in a molar
        // ratio of 1:1 and calcined in a muffle furnace at 1000 C for
        // 2 h... the two targets were sintered at temperatures of 1200 C
        // and 1350 C" (two parallel samples for comparison, not one
        // recommended value -- recorded as the reported span, not a
        // single invented point). Sintering duration and atmosphere for
        // either step are not stated in the source.
        CuratedConditionRecord {
            target: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
            precursor_ids: precursor_ids(&["BaCO3", "TiO2"]),
            conditions: vec![
                ConditionPrecedent {
                    purpose: HeatingPurpose::Calcination,
                    temperature: Some(TemperatureRange::new(1000.0, 1000.0).unwrap()),
                    duration: Some(DurationRange::new(2.0, 2.0).unwrap()),
                    atmosphere: None,
                    ramp: None,
                    evidence_kind: EvidenceKind::CuratedLiteratureRecord,
                    source_id: Some("10.3390/cryst14040304".to_string()),
                    statement: "BaCO3 + TiO2 (1:1 molar) calcined at 1000 C for 2 h in a \
                        muffle furnace"
                        .to_string(),
                    strength: EvidenceStrength::Moderate,
                    applicable_to: EvidenceScope::ExactTarget,
                },
                ConditionPrecedent {
                    purpose: HeatingPurpose::Sintering,
                    temperature: Some(TemperatureRange::new(1200.0, 1350.0).unwrap()),
                    duration: None,
                    atmosphere: None,
                    ramp: None,
                    evidence_kind: EvidenceKind::CuratedLiteratureRecord,
                    source_id: Some("10.3390/cryst14040304".to_string()),
                    statement: "reaction product pressed into ceramic sputtering targets \
                        and sintered at 1200-1350 C (two parallel samples of different \
                        target density; duration not stated in source)"
                        .to_string(),
                    strength: EvidenceStrength::Moderate,
                    applicable_to: EvidenceScope::ExactTarget,
                },
            ],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AGENTS.md §21.4 (determinism/order-invariance): resolution must not
    /// depend on which of two overlapping records happens to run first.
    /// The cheapest way to guarantee that is to make ambiguity impossible
    /// by construction -- no two records may claim the same
    /// (target, precursor_ids, purpose) triple.
    #[test]
    fn curated_records_have_no_duplicate_target_precursor_purpose_keys() {
        let mut seen: BTreeSet<(String, Vec<String>, String)> = BTreeSet::new();
        for record in curated_records() {
            let target_key = format!("{:?}", record.target);
            let precursor_key: Vec<String> =
                record.precursor_ids.iter().map(|p| p.0.clone()).collect();
            for condition in &record.conditions {
                let key = (
                    target_key.clone(),
                    precursor_key.clone(),
                    format!("{:?}", condition.purpose),
                );
                assert!(
                    seen.insert(key.clone()),
                    "duplicate curated condition record for {key:?} -- two records \
                    claiming the same target/precursor-set/purpose makes resolution \
                    order-dependent"
                );
            }
        }
    }
}
