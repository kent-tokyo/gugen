//! Phase 8 §21.3 "known-route recovery" and the false-confidence audit
//! §21/§28 ask for, built on curated, cited literature fixtures -- never
//! reactions written from memory (AGENTS.md §21.3: "記憶だけで作成しては
//! いけません").
//!
//! Sourcing: four of the five fixtures below are drawn from Kononova,
//! Huo, He, Rong, Botari, Sun, Tshitoyan, Ceder, "Text-mined dataset of
//! inorganic materials synthesis recipes," *Scientific Data* 6, 203
//! (2019) (docs/competitors.md). Its hosted data
//! (10.6084/m9.figshare.9722159) is licensed **CC BY 4.0** -- verified via
//! the figshare API (`license.name == "CC BY 4.0"`) on 2026-08-14, not
//! assumed; this resolves the "license not yet checked" item carried in
//! `tasks/todo.md` since Phase 0. Only a handful of individual routes are
//! cited here (with the paper DOIs that independently report them), not
//! the dataset itself -- the raw dataset is not bundled in this repo.
//!
//! The fifth (simple binary oxide) is **not** from Kononova: querying the
//! full 30,031-reaction dataset for any target matching a plain binary
//! oxide formula (NiO, Fe2O3, ZnO, CuO, CaO, ...) returns zero results --
//! an empirical finding, not an oversight. Commodity binary oxides are
//! precursors in that corpus's papers, never the reported synthesis
//! target, so a genuinely different, independently verified source is
//! used for that one fixture instead (see `simple_binary_oxide_cao`).

use gugen::{
    Composition, Element, InMemoryPrecursorCatalog, Planner, PlanningConfig, PlanningConstraints,
    PrecursorCandidate, PrecursorId, TargetSpecification,
};
use std::collections::BTreeSet;

fn element(symbol: &str) -> Element {
    Element::new(symbol).unwrap()
}

fn composition(pairs: &[(&str, f64)]) -> Composition {
    Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
}

fn candidate(id: &str, pairs: &[(&str, f64)]) -> PrecursorCandidate {
    PrecursorCandidate {
        id: PrecursorId(id.to_string()),
        composition: composition(pairs),
        availability: None,
    }
}

/// One curated, cited validation case. `literature_precursor_ids` is the
/// exact route the citation reports; `catalog` is the full set of
/// candidates offered to the planner (the literature route plus decoys),
/// so recovery is meaningful rather than a single-candidate pass-through.
struct LiteratureFixture {
    name: &'static str,
    category: &'static str,
    target: Composition,
    literature_precursor_ids: BTreeSet<&'static str>,
    catalog: Vec<PrecursorCandidate>,
    citation: &'static str,
}

fn fixtures() -> Vec<LiteratureFixture> {
    vec![
        LiteratureFixture {
            name: "LaAlO3",
            category: "perovskite oxide",
            target: composition(&[("La", 1.0), ("Al", 1.0), ("O", 3.0)]),
            literature_precursor_ids: BTreeSet::from(["La2O3", "Al2O3"]),
            catalog: vec![
                candidate("La2O3", &[("La", 2.0), ("O", 3.0)]),
                candidate("Al2O3", &[("Al", 2.0), ("O", 3.0)]),
                // Decoy: shares no element with the target.
                candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]),
                // Not actually a non-competing decoy: verified by running
                // it that La2(CO3)3 + Al2O3 -> LaAlO3 + CO2 balances too
                // (CO2 is a curated byproduct, and Al2O3 is right there in
                // the same catalog) -- a real, valid alternative route,
                // not a filtered-out candidate. Kept anyway: it's a third
                // genuine partial-precursor-match case (alongside MgAl2O4
                // and BaTiO3 below), and correcting this comment after
                // actually checking is more honest than the original
                // "cannot compete" claim this fixture shipped with.
                candidate("La2(CO3)3", &[("La", 2.0), ("C", 3.0), ("O", 9.0)]),
            ],
            citation: "0.5 La2O3 + 0.5 Al2O3 -> LaAlO3, cross-validated across 19 independent \
                paper DOIs in the Kononova et al. 2019 dataset (CC BY 4.0); representative \
                entry: DOI 10.1149/2.053405jes",
        },
        LiteratureFixture {
            name: "MgAl2O4",
            category: "spinel oxide",
            target: composition(&[("Mg", 1.0), ("Al", 2.0), ("O", 4.0)]),
            literature_precursor_ids: BTreeSet::from(["MgO", "Al2O3"]),
            catalog: vec![
                candidate("MgO", &[("Mg", 1.0), ("O", 1.0)]),
                candidate("Al2O3", &[("Al", 2.0), ("O", 3.0)]),
                candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]),
                // Decoy: an alternative, also-valid Mg source (carbonate
                // route) -- deliberately included so this fixture also
                // exercises a *partial* precursor match (same target
                // element, different real-world precursor than the cited
                // route) alongside the exact match.
                candidate("MgCO3", &[("Mg", 1.0), ("C", 1.0), ("O", 3.0)]),
            ],
            citation: "1 Al2O3 + 1 MgO -> MgAl2O4, cross-validated across 20 independent paper \
                DOIs in the Kononova et al. 2019 dataset (CC BY 4.0); representative entry: \
                DOI 10.1007/s11663-014-0207-8",
        },
        LiteratureFixture {
            name: "Zn3(PO4)2",
            category: "phosphate",
            target: composition(&[("Zn", 3.0), ("P", 2.0), ("O", 8.0)]),
            literature_precursor_ids: BTreeSet::from(["ZnO", "P2O5"]),
            catalog: vec![
                candidate("ZnO", &[("Zn", 1.0), ("O", 1.0)]),
                candidate("P2O5", &[("P", 2.0), ("O", 5.0)]),
                candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]),
            ],
            citation: "3 ZnO + 1 P2O5 -> Zn3(PO4)2, reported independently in 2 paper DOIs in \
                the Kononova et al. 2019 dataset (CC BY 4.0); representative entry: DOI \
                10.1016/j.jmmm.2015.06.001",
        },
        LiteratureFixture {
            name: "CaO",
            category: "simple binary oxide",
            target: composition(&[("Ca", 1.0), ("O", 1.0)]),
            literature_precursor_ids: BTreeSet::from(["CaCO3"]),
            catalog: vec![
                candidate("CaCO3", &[("Ca", 1.0), ("C", 1.0), ("O", 3.0)]),
                candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]),
            ],
            citation: "CaCO3 -> CaO + CO2 at 900 C. Seesanong, Seangarun, Boonchom, \
                Laohavisuti, Boonmee, Thompho, Rungrojchaipon, \"Low-Cost and Eco-Friendly \
                Calcium Oxide Prepared via Thermal Decompositions of Calcium Carbonate and \
                Calcium Acetate Precursors Derived from Waste Oyster Shells,\" Materials \
                17(15), 3875 (2024), DOI 10.3390/ma17153875. NOT from the Kononova dataset \
                (empirical finding: that corpus has zero simple-binary-oxide-target entries).",
        },
        LiteratureFixture {
            name: "BaTiO3",
            category: "carbonate precursor route",
            target: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
            literature_precursor_ids: BTreeSet::from(["BaCO3", "TiO2"]),
            catalog: vec![
                candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
                candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
                candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]),
                // Decoy: an alternative, also-valid Ba source -- see
                // MgAl2O4's fixture comment for why this is included.
                candidate("BaO", &[("Ba", 1.0), ("O", 1.0)]),
            ],
            citation: "1 BaCO3 + 1 TiO2 -> BaTiO3 + CO2, the strongest-attested route in this \
                suite: 88 independent paper DOIs in the Kononova et al. 2019 dataset (CC BY \
                4.0) report it; representative entry: DOI 10.1111/j.1551-2916.2006.01172.x. \
                Also a perovskite target (BaTiO3 satisfies both categories in this list -- a \
                real overlap, not a fixture-selection error) and the same route used \
                throughout this crate's own examples/tests since Phase 1, now independently \
                cross-checked against real literature rather than only internal convention.",
        },
    ]
}

fn plan_fixture(fixture: &LiteratureFixture) -> gugen::SynthesisPlanningReport {
    let catalog = InMemoryPrecursorCatalog::new(fixture.catalog.clone());
    let planner = Planner::offline_minimal(catalog, PlanningConfig::default());
    let target = TargetSpecification {
        composition: fixture.target.clone(),
        structure: None,
        desired_phase: None,
        constraints: PlanningConstraints::default(),
    };
    planner
        .plan(&target, "2026-08-14T00:00:00Z")
        .unwrap_or_else(|e| panic!("{} must plan without error: {e}", fixture.name))
}

/// AGENTS.md §21.3/§22: every curated, citable literature route must
/// actually be found by the search -- not just "some plan," the exact
/// cited precursor set. Measures §22's "known precursor-set top-k
/// recovery" and "exact precursor match" for this suite (k = "anywhere in
/// the ranked list," since these fixtures are small enough that nothing
/// pushes the correct route out of a generous `SearchBudget`).
#[test]
fn every_literature_route_is_recovered_exactly() {
    let mut missing = Vec::new();
    for fixture in fixtures() {
        let report = plan_fixture(&fixture);
        let found = report.plans.iter().any(|p| {
            let ids: BTreeSet<&str> = p
                .precursors
                .iter()
                .map(|s| s.precursor.0.as_str())
                .collect();
            ids == fixture.literature_precursor_ids
        });
        if !found {
            missing.push(format!(
                "{} ({}): cited route {:?} not recovered; accepted sets were {:?} -- {}",
                fixture.name,
                fixture.category,
                fixture.literature_precursor_ids,
                report
                    .plans
                    .iter()
                    .map(|p| p
                        .precursors
                        .iter()
                        .map(|s| s.precursor.0.clone())
                        .collect::<Vec<_>>())
                    .collect::<Vec<_>>(),
                fixture.citation
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "known-route recovery failed for: {missing:#?}"
    );
}

/// MgAl2O4 and BaTiO3 deliberately include an alternative, also-valid
/// precursor for one target element (MgCO3 alongside MgO; BaO alongside
/// BaCO3). A correct search must accept *both* the cited exact route and
/// the valid alternative as separate plans -- neither hides the other,
/// and the alternative is not treated as an error (AGENTS.md's
/// "alternatives" concept from §13, exercised end to end here).
#[test]
fn a_valid_alternative_precursor_is_accepted_alongside_the_cited_route() {
    let batio3 = fixtures().into_iter().find(|f| f.name == "BaTiO3").unwrap();
    let report = plan_fixture(&batio3);
    let sets: Vec<BTreeSet<&str>> = report
        .plans
        .iter()
        .map(|p| {
            p.precursors
                .iter()
                .map(|s| s.precursor.0.as_str())
                .collect()
        })
        .collect();
    assert!(
        sets.contains(&BTreeSet::from(["BaCO3", "TiO2"])),
        "cited route missing: {sets:?}"
    );
    assert!(
        sets.contains(&BTreeSet::from(["BaO", "TiO2"])),
        "valid alternative (BaO instead of BaCO3) missing -- a partial precursor match \
        that should still be a real, present plan: {sets:?}"
    );
}

/// AGENTS.md §21/§28 false-confidence audit: sweep every plan this suite
/// produces and check nothing overstates what v0.1 actually knows.
/// `manual_review_required` must always be `true` (no hazard data source
/// exists -- score.rs), and every plan must carry the mandatory `Severe`
/// safety warning alongside it, regardless of how "clean" the underlying
/// chemistry looks.
#[test]
fn every_recovered_plan_still_requires_manual_review_with_an_explicit_warning() {
    for fixture in fixtures() {
        let report = plan_fixture(&fixture);
        for plan in &report.plans {
            assert!(
                plan.manual_review_required,
                "{}: manual_review_required must always be true in v0.1",
                fixture.name
            );
            assert!(
                plan.warnings
                    .iter()
                    .any(|w| w.severity == gugen::WarningSeverity::Severe),
                "{}: every plan must carry an explicit Severe safety warning: {:?}",
                fixture.name,
                plan.warnings
            );
        }
    }
}

/// AGENTS.md §28's literal trigger is "validation corpusでfalse confident
/// plansが多い" -- measured, not asserted away. `confidence.overall` is
/// the average of four Score01 dimensions (score.rs), and in v0.1
/// `process_conditions` is *always* 0.0 (no condition is ever resolved),
/// so for any plan with a balanced reaction and non-empty evidence the
/// average is structurally `(1 + 1 + 0 + 1) / 4 == 0.75`, regardless of
/// how different two plans' real uncertainty is. This test measures that
/// finding directly rather than asserting a specific number by
/// assumption -- see tasks/todo.md's Phase 8 section for the full
/// §28-format report on what this does and doesn't mean, and why it's
/// not treated as a defect to silently "fix" with an unsourced
/// recalibration.
#[test]
fn confidence_overall_is_measured_not_assumed_to_be_constant() {
    let mut distinct = BTreeSet::new();
    let mut total = 0;
    for fixture in fixtures() {
        let report = plan_fixture(&fixture);
        for plan in &report.plans {
            total += 1;
            distinct.insert(plan.confidence.overall.value().to_bits());
            assert_eq!(
                plan.confidence.process_conditions.value(),
                0.0,
                "{}: process_conditions must be 0.0 in v0.1 (no condition is ever resolved)",
                fixture.name
            );
        }
    }
    assert!(total > 0, "fixture suite must produce plans to measure");
    assert_eq!(
        distinct,
        BTreeSet::from([0.75_f64.to_bits()]),
        "confidence.overall was NOT constant at 0.75 across the fixture suite -- the \
        false-confidence finding documented in tasks/todo.md no longer matches reality; \
        re-derive the report before trusting it"
    );
}

/// AGENTS.md §21/§22 reproducibility: the same target, catalog, and
/// execution_timestamp must produce a byte-for-byte identical report
/// every time (AGENTS.md §25's determinism requirement, exercised
/// end-to-end through `Planner`, not just at the balancing/search layer
/// individual unit tests already cover).
#[test]
fn planning_is_reproducible_across_repeated_runs() {
    for fixture in fixtures() {
        let a = plan_fixture(&fixture);
        let b = plan_fixture(&fixture);
        assert_eq!(
            a, b,
            "{}: two runs of the same input diverged",
            fixture.name
        );
    }
}
