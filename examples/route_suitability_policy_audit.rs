//! Generates `docs/route_suitability_corpus_audit.md` (Phase 17, AGENTS.md
//! §21.3/§27). Run with `cargo run --example route_suitability_policy_audit
//! --features serde` and copy the output verbatim, same discipline as
//! `examples/benchmark_report.rs`/`examples/large_scale_benchmark.rs`.
//!
//! **This is not a route-family prediction-accuracy benchmark, and must
//! never be read as one.** `InMemoryRouteSuitabilityProvider` (Phase 15A)
//! is a lookup over hand-verified literature evidence for specific
//! `(target, RouteFamily)` pairs, not a generalizing classifier -- a
//! holdout of genuinely unknown targets correctly returns
//! `InsufficientEvidence` almost everywhere, and reporting that as
//! "accuracy" would misrepresent what the system does. This report has two
//! parts instead:
//!
//! - **17A, corpus feasibility audit:** how much of the kind of evidence
//!   `RouteSuitabilityProvider` needs (which route family, whether it was
//!   compared against an alternative and found suitable/unsuitable, not
//!   merely "used") actually exists in a real, large literature corpus
//!   already available to this repo.
//! - **17B, decision-policy evaluation:** whether `derive_recommendation`
//!   (Phase 15B) behaves conservatively against the evidence 17A actually
//!   found -- exclusion precision and abstention behavior, not predictive
//!   accuracy.
//!
//! Data source for 17A: the existing `benchmarks/data/kononova_sample.jsonl`
//! (1500 rows, CC BY 4.0, fetched/filtered/attributed by Phase 11's
//! `benchmarks/fetch_kononova.py`) -- no new fetch. Its fields are `doi`,
//! `target_formula`, `target_elements`, `precursors`. **Route family,
//! success/failure, and comparative-route-rejection are not present in
//! this corpus and cannot be derived from it without reading each paper.**
//! Deliberately not attempted here: classifying a record's route family
//! from any keyword/operations heuristic would manufacture fabricated
//! ground truth (the exact failure mode Phase 15A's own literature
//! sourcing avoided), so this report never does it. That absence is 17A's
//! headline finding, not a gap papered over.
//!
//! Data source for 17B's holdout: one hand-verified literature record,
//! gathered fresh this phase via live CrossRef/Semantic Scholar lookups
//! (title/authors/venue/year/DOI confirmed; abstract read directly, open
//! access CC BY), never referenced during Phase 15A/15B's design of
//! `derive_recommendation`, and **never added to
//! `src/route_suitability.rs::curated_records()`** -- kept here only, so
//! it stays a genuine holdout for any future re-run. A bounded search (a
//! handful of targeted queries across two chemistry angles) found exactly
//! one record meeting the bar of "documents a route's own real difficulty
//! for a specific target," not merely "this route was used somewhere." A
//! single successful-use paper is `Supports` at most for the route it
//! reports and says nothing about any other route -- never treated as
//! `Contradicts` for an alternative. The low yield is itself a real
//! result: genuinely comparative or explicitly negative route evidence is
//! rare in the accessible literature, consistent with 17A's corpus
//! finding.
//!
//! Determinism (finding order does not change `derive_recommendation`'s
//! output for this real holdout record) is verified by a dedicated test
//! in `tests/route_suitability.rs`, not recomputed in this report.

use gugen::{
    Composition, Element, EvidenceScope, EvidenceStrength, InMemoryRouteSuitabilityProvider,
    RouteFamily, RouteRecommendation, RouteSuitabilityAssessment, RouteSuitabilityProvider,
    SuitabilityFinding, SuitabilityVerdict, derive_recommendation,
};
use std::collections::BTreeMap;

const CORPUS_JSONL: &str = include_str!("../benchmarks/data/kononova_sample.jsonl");

#[derive(serde::Deserialize)]
struct CorpusPrecursor {
    formula: String,
}

#[derive(serde::Deserialize)]
struct CorpusRow {
    doi: Option<String>,
    target_elements: BTreeMap<String, f64>,
    #[serde(default)]
    precursors: Vec<CorpusPrecursor>,
}

fn load_corpus() -> Vec<CorpusRow> {
    CORPUS_JSONL
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str(l).expect(
                "benchmarks/data/kononova_sample.jsonl must be valid JSONL -- \
                regenerate with benchmarks/fetch_kononova.py",
            )
        })
        .collect()
}

fn try_composition(elements: &BTreeMap<String, f64>) -> Option<Composition> {
    let pairs: Option<Vec<(Element, f64)>> = elements
        .iter()
        .map(|(sym, amt)| Element::new(sym).ok().map(|e| (e, *amt)))
        .collect();
    Composition::new(pairs?).ok()
}

fn element(symbol: &str) -> Element {
    Element::new(symbol).expect("known-composition list uses a valid IUPAC element symbol")
}

fn composition(pairs: &[(&str, f64)]) -> Composition {
    Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt)))
        .expect("known-composition list uses a valid, finite composition")
}

/// Small, **explicitly non-exhaustive** set of well-established
/// polymorphic compositions (textbook crystallography -- each system's
/// polymorphism is common knowledge, not a specific literature claim
/// requiring its own DOI citation, unlike the numeric claims elsewhere in
/// this codebase). Fe2O3 is gugen's own documented trap case
/// (`route_suitability.rs`'s `no_curated_record_targets_bare_fe2o3_the_
/// documented_polymorph_trap` test). Used only to compute a **floor** --
/// see module doc and the report text below for why this is never
/// reported as a bare rate.
fn known_polymorphic_compositions() -> Vec<(&'static str, Composition)> {
    vec![
        (
            "Fe2O3 (hematite alpha-Fe2O3 / maghemite gamma-Fe2O3)",
            composition(&[("Fe", 2.0), ("O", 3.0)]),
        ),
        (
            "TiO2 (rutile / anatase / brookite)",
            composition(&[("Ti", 1.0), ("O", 2.0)]),
        ),
        (
            "ZrO2 (monoclinic / tetragonal / cubic)",
            composition(&[("Zr", 1.0), ("O", 2.0)]),
        ),
        (
            "Al2O3 (alpha / gamma)",
            composition(&[("Al", 2.0), ("O", 3.0)]),
        ),
        (
            "SiO2 (quartz / cristobalite / tridymite)",
            composition(&[("Si", 1.0), ("O", 2.0)]),
        ),
        (
            "CaCO3 (calcite / aragonite)",
            composition(&[("Ca", 1.0), ("C", 1.0), ("O", 3.0)]),
        ),
        (
            "ZnS (wurtzite / zinc blende)",
            composition(&[("Zn", 1.0), ("S", 1.0)]),
        ),
        (
            "Al(OH)3 (gibbsite / bayerite / nordstrandite)",
            composition(&[("Al", 1.0), ("O", 3.0), ("H", 3.0)]),
        ),
    ]
}

/// Phase 17's single hand-verified holdout record. **Not** added to
/// `curated_records()` -- see module doc.
///
/// Corrado, Bellcase, Forrester, Dickey, Reaney, Jones, "Solid state
/// synthesis of BiFeO3 occurs through the intermediate Bi25FeO39
/// compound," Journal of the American Ceramic Society (2024), DOI
/// 10.1111/jace.19702, open access (CC BY) -- abstract read directly
/// (title/authors/venue/year confirmed via Semantic Scholar/CrossRef).
/// States: "Many studies have reported challenges in the synthesis of
/// BiFeO3 from starting oxides of Bi2O3 and Fe2O3, mainly associated with
/// the development of persistent secondary phases such as Bi25FeO39
/// (sillenite) and Bi2Fe4O9 (mullite)," and shows via in-situ high-
/// temperature XRD that these arise from a genuine two-step reaction
/// pathway (Bi2O3 + Fe2O3 -> Bi25FeO39 intermediate -> BiFeO3), not
/// merely incomplete mixing.
fn holdout_record() -> (Composition, RouteFamily, SuitabilityFinding) {
    (
        composition(&[("Bi", 1.0), ("Fe", 1.0), ("O", 3.0)]),
        RouteFamily::ConventionalSolidState,
        SuitabilityFinding {
            verdict: SuitabilityVerdict::Contradicts,
            statement: "Conventional solid-state synthesis of BiFeO3 from Bi2O3 + Fe2O3 \
                starting oxides is documented (via in-situ high-temperature XRD) to proceed \
                through an intermediate sillenite compound (Bi25FeO39), and persistent \
                secondary phases (Bi25FeO39, Bi2Fe4O9) are reported across many studies as a \
                recurring difficulty in reaching phase-pure BiFeO3 by this route -- not merely \
                incomplete mixing, but a genuine two-step reaction pathway"
                .to_string(),
            source_id: Some("10.1111/jace.19702".to_string()),
            strength: EvidenceStrength::Moderate,
            applicable_to: EvidenceScope::ExactTarget,
            limitations: vec![
                "single paper directly read (a 2024 mechanistic study); \"many studies\" \
                    reporting this difficulty is the paper's own characterization, not an \
                    independently-counted literature survey by gugen"
                    .to_string(),
                "documents conventional solid-state's own persistent difficulty for this \
                    target, not a head-to-head comparison against Mechanochemical for BiFeO3 in \
                    one paper -- no Mechanochemical BiFeO3 comparison was found in this phase's \
                    bounded search"
                    .to_string(),
                "does not claim phase-pure BiFeO3 via conventional solid-state is impossible \
                    -- only that persistent secondary phases are a well-documented recurring \
                    challenge; some studies report acceptably phase-pure product with careful \
                    stoichiometric and kinetic control"
                    .to_string(),
            ],
        },
    )
}

fn pct(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        100.0 * count as f64 / total as f64
    }
}

fn main() {
    let raw_rows = load_corpus();
    let corpus_len = raw_rows.len();
    let targets: Vec<Composition> = raw_rows
        .iter()
        .filter_map(|r| try_composition(&r.target_elements))
        .collect();
    let doi_present = raw_rows
        .iter()
        .filter(|r| r.doi.as_deref().is_some_and(|d| !d.trim().is_empty()))
        .count();

    let provider = InMemoryRouteSuitabilityProvider::from_curated_records();
    let route_families = [
        RouteFamily::ConventionalSolidState,
        RouteFamily::Mechanochemical,
    ];
    let covered_by_shipped_provider = targets
        .iter()
        .filter(|t| {
            route_families
                .iter()
                .any(|&rf| !provider.assess(t, rf).unwrap().is_empty())
        })
        .count();

    let known_polymorphs = known_polymorphic_compositions();
    let polymorph_floor = targets
        .iter()
        .filter(|t| known_polymorphs.iter().any(|(_, kp)| *t == kp))
        .count();
    let polymorph_hits: BTreeMap<&str, usize> = known_polymorphs
        .iter()
        .map(|(name, kp)| (*name, targets.iter().filter(|t| *t == kp).count()))
        .collect();
    let precursor_hits = |formula: &str| {
        raw_rows
            .iter()
            .filter(|r| r.precursors.iter().any(|p| p.formula == formula))
            .count()
    };
    let fe2o3_precursor_hits = precursor_hits("Fe2O3");
    let tio2_precursor_hits = precursor_hits("TiO2");

    let (holdout_target, holdout_route_family, holdout_finding) = holdout_record();
    let fe2o3 = composition(&[("Fe", 2.0), ("O", 3.0)]);
    assert_ne!(
        holdout_target, fe2o3,
        "holdout target must not be the documented Fe2O3 polymorph trap -- see \
        route_suitability.rs's own regression test for why"
    );
    assert!(
        provider
            .assess(&holdout_target, holdout_route_family)
            .unwrap()
            .is_empty(),
        "holdout target/route_family must not already be covered by the shipped \
        curated provider -- otherwise it is not a genuine holdout"
    );
    let holdout_assessment = RouteSuitabilityAssessment {
        route_family: holdout_route_family,
        findings: vec![holdout_finding],
    };
    let holdout_recommendation = derive_recommendation(&holdout_assessment);
    // This phase's single holdout record documents a real, sourced
    // difficulty with no counter-evidence found -- NotRecommended is the
    // literature-consistent outcome, so it counts as a correct exclusion.
    // Not a general labeling scheme: with exactly one holdout record,
    // "correct" here means "matches what the sourced literature actually
    // reports," checked by hand for this one case, not automated.
    let holdout_is_correct_exclusion =
        holdout_recommendation == RouteRecommendation::NotRecommended;

    let ba_ti_o3 = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
    let mg_oh2 = composition(&[("Mg", 1.0), ("O", 2.0), ("H", 2.0)]);
    let ca_oh2 = composition(&[("Ca", 1.0), ("O", 2.0), ("H", 2.0)]);
    let ga_fe_o3 = composition(&[("Ga", 1.0), ("Fe", 1.0), ("O", 3.0)]);
    let dev_batio3 = derive_recommendation(&RouteSuitabilityAssessment {
        route_family: RouteFamily::Mechanochemical,
        findings: provider
            .assess(&ba_ti_o3, RouteFamily::Mechanochemical)
            .unwrap(),
    });
    let dev_mgoh2 = derive_recommendation(&RouteSuitabilityAssessment {
        route_family: RouteFamily::ConventionalSolidState,
        findings: provider
            .assess(&mg_oh2, RouteFamily::ConventionalSolidState)
            .unwrap(),
    });
    let dev_caoh2 = derive_recommendation(&RouteSuitabilityAssessment {
        route_family: RouteFamily::ConventionalSolidState,
        findings: provider
            .assess(&ca_oh2, RouteFamily::ConventionalSolidState)
            .unwrap(),
    });
    let dev_gafeo3 = derive_recommendation(&RouteSuitabilityAssessment {
        route_family: RouteFamily::Mechanochemical,
        findings: provider
            .assess(&ga_fe_o3, RouteFamily::Mechanochemical)
            .unwrap(),
    });

    let mut out = String::new();
    out.push_str(
        "# gugen route-suitability corpus audit + decision-policy evaluation (Phase 17)\n\n",
    );
    out.push_str(
        "Generated by `cargo run --example route_suitability_policy_audit --features serde`. \
        **Not a route-family prediction-accuracy benchmark** -- see this file's own module doc \
        comment for why that framing would misrepresent `InMemoryRouteSuitabilityProvider` \
        (a hand-verified literature-evidence lookup, not a generalizing classifier). \
        Re-run and replace this file's content after any change to `route_suitability.rs` or \
        `benchmarks/fetch_kononova.py`'s filter criteria, rather than hand-editing numbers here.\n\n",
    );

    out.push_str("## 17A: Literature Corpus Feasibility Audit\n\n");
    out.push_str(&format!(
        "- **Corpus:** {corpus_len} rows loaded from `benchmarks/data/kononova_sample.jsonl` \
        (Kononova et al. 2019, CC BY 4.0; see `benchmarks/data/ATTRIBUTION.md`). \
        {targets_parsed} targets representable by gugen's own `Composition` type. \
        {doi_present}/{corpus_len} rows carry a non-empty `doi` field.\n",
        targets_parsed = targets.len(),
    ));
    out.push_str(
        "- **Route family, success/failure, and comparative-route-rejection: not present in \
        this corpus.** Its only fields are `doi`, `target_formula`, `target_elements`, \
        `precursors` -- confirmed directly from `examples/large_scale_benchmark.rs`'s own \
        `CorpusRow` and re-checked here. This is 17A's headline finding, not a limitation \
        worked around: the largest text-mined synthesis corpus already available to this repo, \
        in its readily-available form, cannot by itself support evaluating which route family \
        suits a target, only which precursors were used to reach it. Per-paper reading, not \
        bulk classification, is what `route_suitability.rs`'s curated records already do -- this \
        audit does not attempt to scale that via any keyword or operations-derived heuristic, \
        which would fabricate ground truth rather than measure it.\n",
    );
    out.push_str(&format!(
        "- **Evidence coverage against the currently shipped provider:** \
        {covered_by_shipped_provider}/{} corpus targets get at least one finding from \
        `InMemoryRouteSuitabilityProvider::from_curated_records()` (4 curated records total: \
        2 from Phase 15A, 2 more added since -- see `route_suitability.rs`'s own \
        `curated_records()` doc comment). Expected near-zero and measured, not assumed -- \
        `fetch_kononova.py`'s own \
        leakage filter already excludes exact route matches to gugen's curated fixtures.\n",
        targets.len()
    ));
    out.push_str(&format!(
        "- **Polymorph-ambiguity floor:** {polymorph_floor}/{} corpus targets exactly match one \
        of {} explicitly hand-listed, well-established polymorphic compositions (textbook \
        crystallography, not individually DOI-cited): {polymorph_hits:?}. Measured as zero, and \
        this measurement is itself a finding, not an absence of one: none of these eight \
        binary-oxide/carbonate/hydroxide systems appears as a `target_formula` in this sample, \
        while they \
        appear heavily as *precursor* formulas ({fe2o3_precursor_hits} rows list Fe2O3 among \
        precursors, {tio2_precursor_hits} list TiO2) -- consistent with `fetch_kononova.py`'s \
        source corpus treating common binary oxides mostly as starting materials rather than \
        synthesis targets, not with polymorph ambiguity being rare in materials science \
        generally. This remains a **floor**, not a rate: the list is non-exhaustive, exact-\
        `Composition` matching misses any polymorph reported at a different formula-unit scale, \
        and this corpus's target/precursor skew means the floor says more about \
        `kononova_sample.jsonl`'s composition than about polymorph ambiguity at large.\n",
        targets.len(),
        known_polymorphs.len(),
    ));

    out.push_str("\n## 17B: Decision-Policy Evaluation\n\n");
    out.push_str(
        "**Two real categories, not three.** `derive_recommendation`'s decision matrix \
        (Phase 15B) was designed from stated principles and never fit against real data -- \
        there is no genuine \"threshold-reference\" set in gugen's actual history, so this \
        report does not invent one. **dev** = the 4 existing `curated_records()` entries \
        (already informed the original design, not blind -- the 2 added after Phase 15B did \
        not change `derive_recommendation` itself, only exercised it against new real cases). \
        **holdout** = the record gathered \
        fresh this phase (see module doc), never referenced during Phase 15A/15B's design, \
        evaluated once. `derive_recommendation` is not adjusted based on what follows, \
        regardless of outcome.\n\n",
    );
    out.push_str(&format!(
        "- **dev sanity check (already-known behavior, not a discovery):** BaTiO3 + \
        Mechanochemical -> {dev_batio3:?}; Mg(OH)2 + ConventionalSolidState -> {dev_mgoh2:?}; \
        Ca(OH)2 + ConventionalSolidState -> {dev_caoh2:?}; GaFeO3 + Mechanochemical -> \
        {dev_gafeo3:?}.\n"
    ));
    assert_eq!(
        holdout_recommendation,
        RouteRecommendation::NotRecommended,
        "holdout record's derive_recommendation output changed -- update this report's \
        hard_exclusion_precision/false_exclusion_rate text to match"
    );
    let (hard_exclusion_precision_num, false_exclusion_num) = if holdout_is_correct_exclusion {
        (1, 0)
    } else {
        (0, 1)
    };
    out.push_str(&format!(
        "- **Holdout (N=1):** BiFeO3 + ConventionalSolidState -> {holdout_recommendation:?}. \
        Hand-checked against the sourced literature (not an automated correctness scheme, \
        `N`=1): this is the literature-consistent outcome, so \
        **hard_exclusion_precision = {hard_exclusion_precision_num}/1, false_exclusion_rate = \
        {false_exclusion_num}/1** for this single record. Caveat this single case actually turns \
        on: the cited paper's own framing is that conventional solid-state synthesis *reaches* \
        BiFeO3 (via an intermediate), with secondary phases as a persistent complication -- not \
        that the route categorically fails. Whether \"recurring difficulty\" should count as a \
        correct hard exclusion, versus a softer warning, is a judgment call this one record \
        cannot settle; it is not a case where the cited literature reports the route simply does \
        not work. **conflict_detection_rate: N/A** -- no conflicting-evidence holdout case was \
        found in this phase's bounded search (would need both a Supports and a Contradicts \
        finding for the same real (target, route_family) pair). A single-record holdout does not \
        support a general precision claim; these numbers describe this one case, not a validated \
        rate.\n",
    ));
    let abstained = targets.len() - covered_by_shipped_provider;
    out.push_str(&format!(
        "- **abstention_rate / evidence_coverage at scale:** {abstained}/{} corpus targets \
        ({:.1}%) get zero findings from the shipped provider -- the same measurement as \
        \"evidence coverage\" above, restated as its complement. This is the one genuinely \
        large-N number in this report; every other 17B metric above is small-sample by \
        construction, because a hand-verified literature-evidence lookup does not scale to \
        thousands of targets without per-paper reading gugen has not done for them.\n",
        targets.len(),
        pct(abstained, targets.len()),
    ));
    out.push_str(&format!(
        "- **phase_ambiguity_rate:** restates the 17A floor above in the decision-policy \
        context -- {polymorph_floor}/{} targets exactly match a hand-listed polymorphic \
        composition and so could not have a `Contradicts`/`Supports` finding correctly applied \
        via `Composition` alone even if literature evidence existed, because gugen's \
        `Composition` type cannot distinguish the polymorph the evidence would actually describe \
        (the Fe2O3/hematite/maghemite trap, generalized to a small hand-listed set of other \
        known systems). As above, this measures this corpus's target/precursor composition, not \
        an inherent rarity of polymorph ambiguity.\n",
        targets.len()
    ));
    out.push_str(
        "- **determinism:** verified by a dedicated test in `tests/route_suitability.rs` \
        (shuffles this holdout record's findings, asserts `derive_recommendation`'s output is \
        unchanged), not recomputed in this report.\n",
    );

    out.push_str("\n## Completion framing\n\n");
    out.push_str(
        "This phase is not gated on strong numbers (the owner's own explicit instruction). The \
        real finding: the largest literature corpus already available to this repo cannot \
        support route-suitability evaluation without per-paper reading, genuinely comparative \
        or negative route evidence is rare even under a real, honest search effort, and the \
        current decision policy therefore abstains on almost everything outside its 4 \
        hand-verified curated records plus this phase's 1 hand-verified holdout record. \
        **Update (2026-08-28):** 2 more curated records (Ca(OH)2/ConventionalSolidState, \
        GaFeO3/Mechanochemical, both `Contradicts`) were added since this phase's own \
        completion -- the first real expansion of this set, and the first `Contradicts` \
        finding for `Mechanochemical` specifically. Still small; gugen still needs a \
        substantially larger hand-verified negative-evidence corpus, not a change to \
        `derive_recommendation`.\n",
    );

    out.push_str("\n## Skipped, not silently\n\n");
    out.push_str(
        "- **Automated route-family classification of the corpus** (e.g. from an `operations` \
        field, if the raw Kononova dataset carries one): not attempted. This report only uses \
        the already-committed, license-verified `kononova_sample.jsonl`, which does not carry \
        this field; deriving route-family labels from any keyword heuristic would fabricate \
        ground truth rather than measure it.\n",
    );
    out.push_str(
        "- **Expanding the holdout set to a fixed target count:** not attempted. The bounded \
        search this phase ran found one record meeting the bar; padding the holdout with \
        weaker \"route X was used\" records (which are `Supports` at most, never usable for the \
        exclusion-precision metrics) was explicitly avoided per the owner's own warning.\n",
    );
    out.push_str(
        "- **Structural (chematic-crystal/mikiwame) route-suitability findings:** not attempted \
        -- Phase 16's own explicit non-goal, unchanged here; still no literature-backed \
        connection between structural diagnostics and route-family choice.\n",
    );

    print!("{out}");
}
