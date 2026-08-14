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
//! full 19,488-reaction dataset for any target matching a plain binary
//! oxide formula (NiO, Fe2O3, ZnO, CuO, CaO, ...) returns zero results --
//! an empirical finding, not an oversight. Commodity binary oxides are
//! precursors in that corpus's papers, never the reported synthesis
//! target, so a genuinely different, independently verified source is
//! used for that one fixture instead (see `simple_binary_oxide_cao`).
//!
//! **Phase 14 correction (2026-08-14)**: the DOI-attestation counts below
//! and the dataset size above were originally measured against a
//! wrong-provenance corpus (a differently-shaped, unlicensed GitHub
//! snapshot cached during an earlier session, not the officially-cited
//! figshare file) -- discovered during Phase 11, recorded as a pending,
//! deliberately-unfixed item, and corrected here by re-fetching the real,
//! CC BY 4.0-licensed figshare file (10.6084/m9.figshare.9722159, 19,488
//! reactions) live and recounting every route directly against it. Every
//! representative DOI below was independently re-verified this phase --
//! the two that turned out to be confirmed topic mismatches on direct
//! reading (found while sourcing Phase 10's condition data, see
//! `src/literature_conditions.rs`'s doc comment) have been replaced, not
//! left standing on a count correction alone. Full record:
//! `tasks/todo.md`'s Phase 14 section.

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
            citation: "0.5 La2O3 + 0.5 Al2O3 -> LaAlO3, cross-validated across 10 independent \
                paper DOIs in the Kononova et al. 2019 dataset (CC BY 4.0), recounted directly \
                against the correctly-licensed figshare file (Phase 14); representative entry: \
                DOI 10.1149/2.053405jes",
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
            citation: "1 Al2O3 + 1 MgO -> MgAl2O4, cross-validated across 16 independent paper \
                DOIs in the Kononova et al. 2019 dataset (CC BY 4.0), recounted directly \
                against the correctly-licensed figshare file (Phase 14); representative entry: \
                DOI 10.1007/s11663-014-0207-8 -- read directly (Phase 10), confirmed to report \
                both the route and its actual firing conditions.",
        },
        LiteratureFixture {
            // Phase 14 replacement for a former "Zn3(PO4)2" fixture (route:
            // 3 ZnO + 1 P2O5 -> Zn3(PO4)2). That route's representative DOI
            // (10.1016/j.jmmm.2015.06.001) was a confirmed topic mismatch
            // (a Sm-doped zinc-phosphate glass paper, melt-quenched, not
            // this reaction), and recounting directly against the correct,
            // correctly-licensed corpus found the ZnO + P2O5 route has
            // **zero** independent attestations there at all -- not a
            // count-correction case, a genuinely wrong fixture. Rather than
            // force-fit a different precursor combination onto the same
            // Zn3(PO4)2 target (Phase 10's own curated condition record for
            // Zn3(PO4)2 already does that, from ZnO + (NH4)2HPO4, and flags
            // its own source as `Weak` -- a short conference paper with an
            // internally inconsistent reported space group), this fixture
            // is a different, well-attested phosphate target found by
            // querying the correct corpus directly for phosphate routes
            // with multiple independent DOIs: LiFePO4, a globally
            // significant lithium-ion battery cathode material.
            name: "LiFePO4",
            category: "phosphate",
            target: composition(&[("Li", 1.0), ("Fe", 1.0), ("P", 1.0), ("O", 4.0)]),
            literature_precursor_ids: BTreeSet::from(["FePO4", "Li2CO3"]),
            catalog: vec![
                candidate("FePO4", &[("Fe", 1.0), ("P", 1.0), ("O", 4.0)]),
                candidate("Li2CO3", &[("Li", 2.0), ("C", 1.0), ("O", 3.0)]),
                candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]),
            ],
            citation: "Two separate claims, deliberately not fused into one (a fused version \
                of this citation was caught and corrected before this fixture was ever \
                committed -- see tasks/todo.md's Phase 14 section): (1) the Kononova et al. \
                2019 dataset (CC BY 4.0) attests that FePO4 + Li2CO3 is a real, independently \
                reported precursor set for a LiFePO4 target -- 6 distinct paper DOIs, recounted \
                by querying the correctly-licensed figshare file directly on 2026-08-14 (Phase \
                14); representative entry: DOI 10.1016/j.electacta.2009.03.063, Chang, Lv, \
                Tang, Li, Yuan, Wang, \"Synthesis and characterization of high-density \
                LiFePO4/C composites as cathode materials for lithium-ion batteries,\" \
                Electrochimica Acta (2009) -- title/authors/venue/year confirmed via CrossRef; \
                the paper itself is paywalled with no accessible full text found (confirmed via \
                Unpaywall), so its specific reported conditions were not independently read \
                (same disclosed-but-unread tier as this suite's existing LaAlO3 citation), only \
                the corpus's own attribution of this route to this DOI and the route's presence \
                among 6 independent attestations. (2) The exact reaction 4 FePO4 + 2 Li2CO3 -> \
                4 LiFePO4 + 2 CO2 + O2 is gugen's own balance() output, not a corpus claim -- \
                it balances exactly within gugen's existing curated byproduct allow-list (CO2 \
                and O2, no widening needed), verified by actually running balance() before \
                adopting this fixture. The corpus attests only the (target, precursor-set) \
                pair, not this specific coefficient/byproduct choice: at least 3 of the 6 \
                DOIs' own titles (electacta.2012.02.102, jallcom.2010.02.173, \
                jpowsour.2008.07.032 -- checked via CrossRef, not assumed) name carbothermal \
                reduction or carbon coating, i.e. carbon-mediated Fe3+ -> Fe2+ reduction \
                (LiFePO4/C composites), a different real-world mechanism than the O2-release \
                byproduct gugen's balancer independently arrived at. gugen does not model \
                carbothermal reduction; the O2-release route is a chemically valid balance of \
                the same target/precursor formulas, not a claim that any of these 6 papers used \
                that exact mechanism.",
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
                suite: 83 independent paper DOIs in the Kononova et al. 2019 dataset (CC BY \
                4.0) report it, recounted directly against the correctly-licensed figshare \
                file (Phase 14). The original representative entry (DOI \
                10.1111/j.1551-2916.2006.01172.x) was confirmed on direct reading to be a \
                NaNbO3-BaTiO3 solid-solution study, not plain BaTiO3 -- a topic mismatch found \
                while sourcing Phase 10's condition data, not a Phase 8 transcription error \
                (the same DOI also tags a separate, different-target NaNbO3 record in the same \
                corpus). Replaced with an independently verified, directly-read example: DOI \
                10.3390/cryst14040304, Qi et al., \"The Effect of Sputtering Target Density on \
                the Crystal and Electronic Structure of Epitaxial BaTiO3 Thin Films,\" Crystals \
                14(4), 304 (2024), open access (CC BY) -- read directly, confirms exactly this \
                route: \"TiO2 ... and BaCO3 ... powders were mixed in a molar ratio of 1:1 and \
                calcined.\" This DOI is not itself one of the 83 Kononova-corpus attestations \
                (that 2019 corpus predates this 2024 paper) -- it is a separate, independently \
                verified confirmation of the same route, a stronger evidentiary tier than \
                naming an unread corpus entry. Also a perovskite target (BaTiO3 satisfies both \
                categories in this list -- a real overlap, not a fixture-selection error) and \
                the same route used throughout this crate's own examples/tests since Phase 1.",
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
