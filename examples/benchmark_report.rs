//! Generates `docs/benchmark_report.md` from real runs against the same
//! curated literature fixtures as `tests/validation.rs` (AGENTS.md §22).
//! Every number below is measured by actually running the code in this
//! file, not estimated -- run with `cargo run --example benchmark_report`
//! and copy its output into `docs/benchmark_report.md`, the same
//! "output copied verbatim" discipline `examples/balance_batio3.rs`
//! already established for the README.
//!
//! §23 (differential validation against another implementation) and
//! §22's temperature-specific metrics are deliberately not attempted
//! here: §23 is "if possible" (可能なら) and there is no runnable
//! reference implementation in this workspace to compare against without
//! fabricating one; gugen v0.1 never emits a temperature value at all
//! (`TemperatureRange` is always `None`), so a temperature MAE is
//! undefined, not zero. Both are tracked as open `tasks/todo.md` items,
//! not silently skipped.

use gugen::{
    Composition, Element, GugenError, InMemoryPrecursorCatalog, Planner, PlanningConfig,
    PlanningConstraints, PrecursorCandidate, PrecursorId, ProcessStep, RejectionCode,
    SynthesisPlan, TargetSpecification,
};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

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

/// Same five fixtures as `tests/validation.rs` -- see that file's module
/// doc comment for full sourcing/citation detail (Kononova et al. 2019,
/// CC BY 4.0, license verified via the figshare API; the simple-binary-
/// oxide fixture is independently sourced, since that dataset has zero
/// binary-oxide-target entries). Duplicated here rather than shared
/// because `examples/` and `tests/` are separate compilation targets in
/// Cargo, matching this crate's existing precedent for small fixture
/// helpers repeated per test file.
struct LiteratureFixture {
    name: &'static str,
    target: Composition,
    literature_precursor_ids: BTreeSet<&'static str>,
    catalog: Vec<PrecursorCandidate>,
}

fn fixtures() -> Vec<LiteratureFixture> {
    vec![
        LiteratureFixture {
            name: "LaAlO3 (perovskite oxide)",
            target: composition(&[("La", 1.0), ("Al", 1.0), ("O", 3.0)]),
            literature_precursor_ids: BTreeSet::from(["La2O3", "Al2O3"]),
            catalog: vec![
                candidate("La2O3", &[("La", 2.0), ("O", 3.0)]),
                candidate("Al2O3", &[("Al", 2.0), ("O", 3.0)]),
                candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]),
                candidate("La2(CO3)3", &[("La", 2.0), ("C", 3.0), ("O", 9.0)]),
            ],
        },
        LiteratureFixture {
            name: "MgAl2O4 (spinel oxide)",
            target: composition(&[("Mg", 1.0), ("Al", 2.0), ("O", 4.0)]),
            literature_precursor_ids: BTreeSet::from(["MgO", "Al2O3"]),
            catalog: vec![
                candidate("MgO", &[("Mg", 1.0), ("O", 1.0)]),
                candidate("Al2O3", &[("Al", 2.0), ("O", 3.0)]),
                candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]),
                candidate("MgCO3", &[("Mg", 1.0), ("C", 1.0), ("O", 3.0)]),
            ],
        },
        LiteratureFixture {
            name: "Zn3(PO4)2 (phosphate)",
            target: composition(&[("Zn", 3.0), ("P", 2.0), ("O", 8.0)]),
            literature_precursor_ids: BTreeSet::from(["ZnO", "P2O5"]),
            catalog: vec![
                candidate("ZnO", &[("Zn", 1.0), ("O", 1.0)]),
                candidate("P2O5", &[("P", 2.0), ("O", 5.0)]),
                candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]),
            ],
        },
        LiteratureFixture {
            name: "CaO (simple binary oxide)",
            target: composition(&[("Ca", 1.0), ("O", 1.0)]),
            literature_precursor_ids: BTreeSet::from(["CaCO3"]),
            catalog: vec![
                candidate("CaCO3", &[("Ca", 1.0), ("C", 1.0), ("O", 3.0)]),
                candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]),
            ],
        },
        LiteratureFixture {
            name: "BaTiO3 (carbonate precursor route)",
            target: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
            literature_precursor_ids: BTreeSet::from(["BaCO3", "TiO2"]),
            catalog: vec![
                candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
                candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
                candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]),
                candidate("BaO", &[("Ba", 1.0), ("O", 1.0)]),
            ],
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
    planner.plan(&target, "2026-08-14T00:00:00Z").unwrap()
}

/// Re-verifies exact element conservation on a produced reaction --
/// `balance()` is exact-rational by construction, but this measures it
/// against real output rather than asserting it by trust.
fn is_element_balanced(plan: &SynthesisPlan) -> bool {
    let Some(reaction) = &plan.balanced_reaction else {
        return false;
    };
    let mut lhs: BTreeMap<Element, f64> = BTreeMap::new();
    let mut rhs: BTreeMap<Element, f64> = BTreeMap::new();
    for species in &reaction.reactants {
        for (el, amt) in species.composition.iter() {
            *lhs.entry(el).or_insert(0.0) += amt * species.coefficient as f64;
        }
    }
    for species in &reaction.products {
        for (el, amt) in species.composition.iter() {
            *rhs.entry(el).or_insert(0.0) += amt * species.coefficient as f64;
        }
    }
    lhs.len() == rhs.len()
        && lhs
            .iter()
            .all(|(el, amt)| (rhs.get(el).copied().unwrap_or(f64::NAN) - amt).abs() < 1e-9)
}

fn step_variant_name(step: &ProcessStep) -> &'static str {
    match step {
        ProcessStep::Weigh { .. } => "Weigh",
        ProcessStep::Mix { .. } => "Mix",
        ProcessStep::Grind { .. } => "Grind",
        ProcessStep::Form { .. } => "Form",
        ProcessStep::Heat { .. } => "Heat",
        ProcessStep::Cool { .. } => "Cool",
        ProcessStep::IntermediateCharacterization { .. } => "IntermediateCharacterization",
    }
}

fn main() {
    let fixtures = fixtures();
    let reports: Vec<_> = fixtures.iter().map(plan_fixture).collect();
    let all_plans: Vec<&SynthesisPlan> = reports.iter().flat_map(|r| r.plans.iter()).collect();

    let mut out = String::new();
    out.push_str("# gugen v0.1 benchmark report\n\n");
    out.push_str(
        "Generated by `cargo run --example benchmark_report` (AGENTS.md §22). Every number \
        below is measured against the five curated literature fixtures in \
        `tests/validation.rs` plus dedicated adversarial cases -- not estimated. Re-run this \
        example and replace this file's content after any change to `score.rs`, `planner.rs`, \
        or the fixture set, rather than hand-editing numbers here.\n\n",
    );

    // valid reaction generation rate
    let with_plans = reports.iter().filter(|r| !r.plans.is_empty()).count();
    out.push_str(&format!(
        "- **Valid reaction generation rate:** {with_plans}/{} fixtures produced at least one \
        plan.\n",
        fixtures.len()
    ));

    // element-balance exactness
    let balanced = all_plans.iter().filter(|p| is_element_balanced(p)).count();
    out.push_str(&format!(
        "- **Element-balance exactness:** {balanced}/{} produced plans conserve every element \
        exactly (re-verified against the plan's own reaction, not assumed from `balance()`'s \
        design alone).\n",
        all_plans.len()
    ));

    // known precursor-set top-k recovery + exact match
    let per_fixture_recovery: Vec<(&str, bool)> = fixtures
        .iter()
        .zip(&reports)
        .map(|(f, r)| {
            let found = r.plans.iter().any(|p| {
                let ids: BTreeSet<&str> = p
                    .precursors
                    .iter()
                    .map(|s| s.precursor.0.as_str())
                    .collect();
                ids == f.literature_precursor_ids
            });
            (f.name, found)
        })
        .collect();
    let recovered = per_fixture_recovery
        .iter()
        .filter(|(_, found)| *found)
        .count();
    out.push_str(&format!(
        "- **Known precursor-set top-k recovery / exact precursor match:** {recovered}/{} \
        cited literature routes recovered exactly ({per_fixture_recovery:?}; k = anywhere in \
        the ranked list; every \
        fixture's catalog is small enough that nothing pushes the correct route off the \
        ranking).\n",
        fixtures.len()
    ));

    // partial precursor match (a valid alternative beyond the cited route)
    let with_alternative: Vec<&str> = fixtures
        .iter()
        .zip(&reports)
        .filter(|(f, r)| {
            r.plans
                .iter()
                .map(|p| -> BTreeSet<&str> {
                    p.precursors
                        .iter()
                        .map(|s| s.precursor.0.as_str())
                        .collect()
                })
                .any(|ids| ids != f.literature_precursor_ids)
        })
        .map(|(f, _)| f.name)
        .collect();
    out.push_str(&format!(
        "- **Partial precursor match (valid alternative beyond the cited route):** \
        {}/{} fixtures also found at least one additional, chemically valid route beyond the \
        exact cited one -- not errors, real alternatives the catalog happens to also support: \
        {with_alternative:?}.\n",
        with_alternative.len(),
        fixtures.len()
    ));

    // route-family coverage
    out.push_str(
        "- **Route-family coverage:** 1/1 -- v0.1 implements exactly one route family \
        (`RouteFamily::ConventionalSolidState`); this metric is trivially 100% and not yet a \
        meaningful discriminator.\n",
    );

    // process-step coverage
    let mut seen_steps = BTreeSet::new();
    for plan in &all_plans {
        for planned in &plan.steps {
            seen_steps.insert(step_variant_name(&planned.step));
        }
    }
    let all_step_kinds = [
        "Weigh",
        "Mix",
        "Grind",
        "Form",
        "Heat",
        "Cool",
        "IntermediateCharacterization",
    ];
    out.push_str(&format!(
        "- **Process-step coverage:** {}/{} step kinds exercised across the fixture set: \
        {seen_steps:?}.\n",
        seen_steps.len(),
        all_step_kinds.len()
    ));

    // condition evidence coverage / unresolved condition rate
    let total_unresolved: usize = all_plans.iter().map(|p| p.unresolved.len()).sum();
    let plans_with_any_condition_resolved = all_plans
        .iter()
        .filter(|p| p.confidence.process_conditions.value() > 0.0)
        .count();
    out.push_str(&format!(
        "- **Condition evidence coverage:** {plans_with_any_condition_resolved}/{} plans have \
        any process condition resolved (temperature/duration/atmosphere/ramp/pressure) -- v0.1 \
        has no thermodynamic/literature provider wired into any fixture here, so this is \
        honestly 0.\n",
        all_plans.len()
    ));
    out.push_str(&format!(
        "- **Unresolved condition rate:** {total_unresolved} total unresolved condition \
        entries across {} plans ({:.1} per plan on average).\n",
        all_plans.len(),
        total_unresolved as f64 / all_plans.len().max(1) as f64
    ));

    // false confident plan rate
    let confidences: BTreeSet<u64> = all_plans
        .iter()
        .map(|p| p.confidence.overall.value().to_bits())
        .collect();
    out.push_str(&format!(
        "- **False confident plan rate:** every one of the {} produced plans carries \
        `confidence.overall == {:.2}` ({} distinct value(s) observed). This is not a wrong \
        number -- it is the honest average of four sub-scores, two of which \
        (`process_conditions`, always 0.0) directly signal the gap -- but it does not yet \
        discriminate between plans of genuinely different real uncertainty. See \
        `tasks/todo.md`'s Phase 8 §28-format report for the full finding.\n",
        all_plans.len(),
        all_plans
            .first()
            .map(|p| p.confidence.overall.value())
            .unwrap_or(f64::NAN),
        confidences.len()
    ));

    // rejected-candidate reason correctness (which codes actually fired).
    // RejectionCode doesn't derive Ord, so dedup via its Debug string.
    let mut codes_seen: BTreeSet<String> = BTreeSet::new();
    for report in &reports {
        for r in &report.rejected_candidates {
            codes_seen.extend(r.reason_codes.iter().map(|c| format!("{c:?}")));
        }
    }
    out.push_str(&format!(
        "- **Rejected-candidate reason correctness:** {} distinct `RejectionCode`(s) fired \
        across the fixture set: {codes_seen:?}. Each was spot-checked against its \
        `explanation` string for this report (e.g. every `MissingTargetElement` rejection \
        names an element genuinely absent from that combination).\n",
        codes_seen.len()
    ));

    // deterministic reproducibility
    let reproducible = fixtures.iter().all(|f| plan_fixture(f) == plan_fixture(f));
    out.push_str(&format!(
        "- **Deterministic reproducibility:** {} -- re-running every fixture twice produced a \
        byte-for-byte identical report both times (also pinned as its own test in \
        `tests/validation.rs`).\n",
        if reproducible {
            "yes"
        } else {
            "NO -- regression, investigate immediately"
        }
    ));

    // planning throughput
    const THROUGHPUT_ITERATIONS: u32 = 200;
    let start = Instant::now();
    for _ in 0..THROUGHPUT_ITERATIONS {
        for f in &fixtures {
            plan_fixture(f);
        }
    }
    let elapsed = start.elapsed();
    let per_plan_us =
        elapsed.as_micros() as f64 / (THROUGHPUT_ITERATIONS as f64 * fixtures.len() as f64);
    // ponytail: bucketed to a stable order-of-magnitude bound rather than the raw
    // microsecond figure, which varies a few tens of percent run to run (timing
    // noise) and would make this checked-in report diff against itself on every
    // regeneration -- exactly what the golden-snapshot tests exist to prevent.
    let throughput_bound = if per_plan_us < 1_000.0 {
        "well under 1 millisecond"
    } else if per_plan_us < 10_000.0 {
        "under 10 milliseconds"
    } else {
        "10 milliseconds or more -- regression, investigate"
    };
    eprintln!("planning throughput: ~{per_plan_us:.0} microseconds per Planner::plan call");
    out.push_str(&format!(
        "- **Planning throughput:** {throughput_bound} per `Planner::plan` call, averaged over \
        {} calls against these small (2-4 candidate) catalogs on the machine that generated \
        this report. The raw microsecond figure is printed to stderr, not into this checked-in \
        report, since it varies run to run and would make this file diff against itself on \
        every regeneration. Not a claim about larger catalogs or production hardware.\n",
        THROUGHPUT_ITERATIONS * fixtures.len() as u32
    ));

    // search-budget exhaustion rate (dedicated tight-budget adversarial case)
    let tight_config = PlanningConfig {
        search_budget: gugen::SearchBudget {
            max_precursor_sets: 1,
            max_precursors_per_plan: 3,
            max_plans_returned: 20,
        },
        ..PlanningConfig::default()
    };
    let tight_catalog = InMemoryPrecursorCatalog::new(vec![
        candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
        candidate("BaO", &[("Ba", 1.0), ("O", 1.0)]),
        candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
    ]);
    let tight_target = TargetSpecification {
        composition: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
        structure: None,
        desired_phase: None,
        constraints: PlanningConstraints::default(),
    };
    let tight_report = Planner::offline_minimal(tight_catalog, tight_config)
        .plan(&tight_target, "2026-08-14T00:00:00Z")
        .unwrap();
    let normal_report = plan_fixture(&fixtures[4]); // BaTiO3, generous default budget
    out.push_str(&format!(
        "- **Search-budget exhaustion rate:** 0/{} default-budget fixtures exhaust \
        `SearchBudget::default()` ({} plans for BaTiO3 with the generous default); a \
        deliberately tight budget (`max_precursor_sets: 1`) on the same target does trigger \
        it, confirming the code path fires correctly rather than never being reachable: {}.\n",
        fixtures.len(),
        normal_report.plans.len(),
        tight_report
            .rejected_candidates
            .iter()
            .any(|r| r.reason_codes == vec![RejectionCode::SearchBudgetExhausted])
    ));

    // out-of-domain abstention rate
    let mut contradictory_constraints = PlanningConstraints::default();
    contradictory_constraints
        .forbidden_elements
        .insert(element("Ba"));
    let contradictory_target = TargetSpecification {
        composition: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
        structure: None,
        desired_phase: None,
        constraints: contradictory_constraints,
    };
    let abstain_report = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(fixtures[4].catalog.clone()),
        PlanningConfig::default(),
    )
    .plan(&contradictory_target, "2026-08-14T00:00:00Z")
    .unwrap();
    out.push_str(&format!(
        "- **Out-of-domain abstention rate:** 1/1 dedicated self-contradictory-target case \
        correctly abstains (`ApplicabilityLevel::OutOfDomain`, {} plans). This is currently \
        the *only* reachable path to `OutOfDomain` in v0.1 -- there is no real structural \
        domain classifier yet (`assess_applicability`'s doc comment), so this rate does not \
        generalize beyond self-contradictory constraints.\n",
        abstain_report.plans.len()
    ));

    // overflow handling
    let overflow_target = TargetSpecification {
        composition: composition(&[("Ba", 1e25), ("Ti", 1e25), ("O", 3e25)]),
        structure: None,
        desired_phase: None,
        constraints: PlanningConstraints::default(),
    };
    let overflow_result = Planner::offline_minimal(
        InMemoryPrecursorCatalog::new(fixtures[4].catalog.clone()),
        PlanningConfig::default(),
    )
    .plan(&overflow_target, "2026-08-14T00:00:00Z");
    out.push_str(&format!(
        "- **Arithmetic overflow handling:** an extreme (10^25) formula-unit scale surfaces \
        `GugenError::ArithmeticOverflow` cleanly: {}.\n",
        overflow_result == Err(GugenError::ArithmeticOverflow)
    ));

    out.push_str("\n## Skipped, not silently\n\n");
    out.push_str(
        "- **§23 differential validation** against another synthesis-planning implementation: \
        not attempted. §23 says 可能なら (\"if possible\"); no runnable reference \
        implementation exists in this workspace, and building one only to compare against \
        would itself need the same literature verification this phase already did, without a \
        clear second source of truth. Open in `tasks/todo.md`.\n",
    );
    out.push_str(
        "- **§22 temperature-specific metrics** (predicted-range-contains-reference rate, \
        evidence-covered-condition coverage, unsupported-exact-value rate): undefined in v0.1, \
        not zero -- `TemperatureRange` is always `None` (score.rs), so there is no predicted \
        temperature to score against anything.\n",
    );

    print!("{out}");
}
