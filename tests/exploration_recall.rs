//! Phase 28 (Exploration Benchmark Lock): fast, dependency-free checks
//! for the recall-detection logic `examples/exploration_recall_baseline.rs`
//! uses against the full (gitignored, locally-regenerated)
//! `benchmarks/data/exploration_frozen_catalog_manifest.json`. Mirrors
//! `tests/large_scale_benchmark.rs`'s own split: this file is breadth-
//! and-logic sanity in `cargo test`, not the real metric computation
//! (that stays a manual `cargo run --example`, never part of CI, since
//! it needs the full, deliberately large, gitignored decoy catalog).
//!
//! No external fixture file is read here on purpose -- the real corpus
//! lives in gitignored, regenerable JSON (too large to commit, see
//! `benchmarks/exploration_build_frozen_decoy_catalog.py`'s own module
//! doc for the size rationale), so a "fast fixture" for this specific
//! file means small, synthetic, hand-built scenarios that pin down
//! exactly what "recovered" and "budget-exhausted" mean, not a slice of
//! real data.

use gugen::{
    AcceptedPrecursorSet, Composition, Element, PlanningConstraints, PrecursorCandidate,
    PrecursorId, RejectionCode, SearchBudget, search_precursor_sets,
};

/// Phase 28 gate criterion #3 (baseline immutability): the committed
/// v0.6.0 baseline result is the one artifact Phase 29's own +20%
/// recall gate is measured against, so a silent change to it (an
/// accidental re-run overwriting real pre-Phase-29 numbers with
/// post-Phase-29 ones, say) must fail loudly here rather than pass
/// unnoticed. `include_str!` (not a runtime read) is safe for this file
/// specifically -- unlike the frontier catalog, this one IS committed
/// (~285KB, within the kononova_sample.jsonl-style size precedent).
const BASELINE_JSON: &str = include_str!("../benchmarks/data/exploration_baseline_v0_6_0.json");

#[test]
fn committed_v0_6_0_baseline_matches_the_pinned_result() {
    let baseline: serde_json::Value = serde_json::from_str(BASELINE_JSON)
        .expect("benchmarks/data/exploration_baseline_v0_6_0.json must be valid JSON");
    assert_eq!(baseline["gugen_version"], "0.6.0");
    assert_eq!(baseline["total_rows"], 2798);
    assert_eq!(baseline["recovered_count"], 1191);
    assert_eq!(baseline["budget_exhausted_count"], 2772);
    // Pinned to 6 decimal places, matching the example's own {:.6}
    // formatting -- a change here means the baseline was regenerated,
    // which must be a deliberate, reviewed action (a new version-tagged
    // file), never a silent overwrite of this one.
    assert_eq!(baseline["recall"], 0.425661);
    assert_eq!(baseline["exhaustion_rate"], 0.990708);
}

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

/// The exact "was this known route recovered" comparison
/// `examples/exploration_recall_baseline.rs` uses against real data:
/// set-equality on precursor ids, ignoring order (a route is defined as
/// a *set* of precursor formulas throughout Phase 28's Python side --
/// `benchmarks/exploration_build_recall_manifest.py`'s own module doc).
fn route_recovered(accepted: &[AcceptedPrecursorSet], expected_route: &[&str]) -> bool {
    let mut expected: Vec<&str> = expected_route.to_vec();
    expected.sort_unstable();
    accepted.iter().any(|a| {
        let mut got: Vec<&str> = a.precursors.iter().map(|p| p.0.as_str()).collect();
        got.sort_unstable();
        got == expected
    })
}

fn budget_exhausted(outcome: &gugen::PrecursorSearchOutcome) -> bool {
    outcome
        .rejected
        .iter()
        .any(|r| r.reason_codes == vec![RejectionCode::SearchBudgetExhausted])
}

#[test]
fn a_findable_route_is_reported_recovered() {
    let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
    let candidates = vec![
        candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
        candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
    ];
    let outcome = search_precursor_sets(
        &target,
        &candidates,
        &PlanningConstraints::default(),
        &SearchBudget::default(),
    )
    .unwrap();
    assert!(route_recovered(&outcome.accepted, &["BaCO3", "TiO2"]));
    assert!(!budget_exhausted(&outcome));
}

#[test]
fn a_route_never_offered_in_the_catalog_is_reported_not_recovered() {
    let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
    // Only TiO2 is offered -- BaCO3 (the other half of the known route)
    // is missing from this catalog entirely, so the target can't even
    // be covered, let alone the specific route recovered.
    let candidates = vec![candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)])];
    let outcome = search_precursor_sets(
        &target,
        &candidates,
        &PlanningConstraints::default(),
        &SearchBudget::default(),
    )
    .unwrap();
    assert!(!route_recovered(&outcome.accepted, &["BaCO3", "TiO2"]));
}

/// Pins the exact mechanism Phase 28's own "headroom" gate criterion
/// depends on: today's search truncates from the high-arity end (see
/// src/precursor.rs's own `generate_combinations`), so a catalog large
/// enough to blow the budget on 1-3-precursor combinations *alone*
/// means a genuinely findable 4-precursor route (arity exactly
/// `max_precursors_per_plan`, so in principle reachable) is *never even
/// generated* -- not merely out-ranked or deprioritized. This is real
/// budget pressure, not a hypothetical --
/// `exploration_build_frozen_decoy_catalog.py` is sized (28
/// candidates/row) specifically to reproduce this at scale.
#[test]
fn a_high_arity_route_can_be_starved_out_by_budget_before_ever_being_generated() {
    let target = composition(&[("Fe", 1.0), ("Cu", 1.0), ("Zn", 1.0), ("Ni", 1.0)]);
    // 40 single-element decoys (real elements, none of which are the
    // target's own 4) plus the 4 real sources the only known route
    // needs -- 44 candidates total. C(44,1)+C(44,2)+C(44,3) =
    // 44+946+13244 = 14,234, already above the default budget (10,000)
    // *without counting any size-4 combination at all* -- so the real
    // answer (size 4, exactly at max_precursors_per_plan) is never
    // reached, regardless of how good any candidate's chemistry is.
    const DECOY_ELEMENTS: [&str; 40] = [
        "Li", "Na", "K", "Rb", "Cs", "Be", "Mg", "Ca", "Sr", "Ba", "Sc", "Y", "Ti", "Zr", "Hf",
        "V", "Nb", "Ta", "Cr", "Mo", "W", "Mn", "Tc", "Re", "Co", "Rh", "Ir", "Pd", "Pt", "Ag",
        "Au", "Cd", "Hg", "Al", "Ga", "In", "Tl", "Sn", "Pb", "Bi",
    ];
    let mut candidates: Vec<PrecursorCandidate> = DECOY_ELEMENTS
        .iter()
        .map(|symbol| candidate(&format!("Decoy_{symbol}"), &[(symbol, 1.0)]))
        .collect();
    candidates.push(candidate("A_src", &[("Fe", 1.0)]));
    candidates.push(candidate("B_src", &[("Cu", 1.0)]));
    candidates.push(candidate("C_src", &[("Zn", 1.0)]));
    candidates.push(candidate("D_src", &[("Ni", 1.0)]));

    let outcome = search_precursor_sets(
        &target,
        &candidates,
        &PlanningConstraints::default(),
        &SearchBudget::default(),
    )
    .unwrap();

    assert!(
        budget_exhausted(&outcome),
        "this catalog must exhaust the default budget for this test to mean anything"
    );
    assert!(
        !route_recovered(&outcome.accepted, &["A_src", "B_src", "C_src", "D_src"]),
        "the only real route is arity 4 (findable in principle, exactly at \
        max_precursors_per_plan) but must be starved out by budget \
        exhaustion during size-1..3 generation alone -- this is the exact \
        mechanism Phase 29 exists to fix"
    );
}
