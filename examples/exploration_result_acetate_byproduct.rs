//! Acetate byproduct support: re-runs the exact same exploration recall
//! measurement as `exploration_result_oxalate_byproduct.rs`, against the
//! same frozen catalog, with `curated_byproducts()`'s new acetone entry
//! (`src/balance.rs`) now also in place (on top of NO2/CO, already
//! merged). The only code difference from the oxalate result is the
//! acetone addition -- everything else (search algorithm, catalog,
//! budget) is unchanged, so a row-position diff against the committed
//! **post-oxalate** result isolates exactly what this one change
//! recovers. Deliberately diffs against
//! `exploration_result_oxalate_byproduct.json`, not the earlier nitrate
//! or v0.7.0 results -- those predate the oxalate fix and would
//! conflate gains.
//!
//! Run: `cargo run --release --example exploration_result_acetate_byproduct
//! --features serde` after regenerating the (gitignored) frozen catalog
//! locally (see `exploration_recall_baseline.rs`'s own doc comment for
//! the exact commands) -- release mode matters, not just for comfort.
//!
//! Writes `benchmarks/data/exploration_result_acetate_byproduct.json` --
//! a **separate** file from the committed, immutable
//! `exploration_baseline_v0_6_0.json` and
//! `exploration_result_oxalate_byproduct.json`, never overwriting
//! either. Chains back to the catalog and the v0.6.0 baseline via
//! sha256 checksums, same convention as
//! `exploration_result_oxalate_byproduct.rs`.

use gugen::{
    Composition, Element, PlanningConstraints, PrecursorCandidate, PrecursorId, RejectionCode,
    SearchBudget, search_precursor_sets,
};
use std::collections::BTreeMap;

const CATALOG_PATH: &str = "benchmarks/data/exploration_frozen_catalog_manifest.json";
const BASELINE_PATH: &str = "benchmarks/data/exploration_baseline_v0_6_0.json";
const PRIOR_RESULT_PATH: &str = "benchmarks/data/exploration_result_oxalate_byproduct.json";
const OUTPUT_PATH: &str = "benchmarks/data/exploration_result_acetate_byproduct.json";

/// Owner-confirmed 2026-08-23 (via `AskUserQuestion`, from Phase 28's
/// real baseline recall of 0.4257): relative +20% on overall recall.
/// See `ROADMAP.md`'s Phase 29 entry and this session's own memory
/// record for the full confirmation trail. Reused here as the same
/// reference point, not a gate this specific fix is expected to clear
/// alone -- no target expectation is pre-registered for this fix
/// (acetate's grounding is narrower than nitrate/oxalate's, and its
/// literal corpus footprint is small; report whatever the real number
/// is).
const BASELINE_RECALL: f64 = 0.425661;
const GATE_MULTIPLIER: f64 = 1.20;

#[derive(serde::Deserialize)]
struct CatalogCandidate {
    formula: String,
    elements: BTreeMap<String, f64>,
}

#[derive(serde::Deserialize)]
struct CatalogRow {
    target_formula: String,
    target_elements: BTreeMap<String, f64>,
    route: Vec<String>,
    candidates: Vec<CatalogCandidate>,
}

#[derive(serde::Deserialize)]
struct FrozenCatalog {
    rows: Vec<CatalogRow>,
}

/// Defensively re-validated, matching `examples/large_scale_benchmark.rs`'s
/// own `try_composition` precedent.
fn try_composition(elements: &BTreeMap<String, f64>) -> Option<Composition> {
    let pairs: Option<Vec<(Element, f64)>> = elements
        .iter()
        .map(|(sym, amt)| Element::new(sym).ok().map(|e| (e, *amt)))
        .collect();
    Composition::new(pairs?).ok()
}

struct RowResult {
    target_formula: String,
    route_arity: usize,
    recovered: bool,
    budget_exhausted: bool,
}

fn route_recovered(accepted: &[gugen::AcceptedPrecursorSet], expected_route: &[String]) -> bool {
    let mut expected: Vec<&str> = expected_route.iter().map(String::as_str).collect();
    expected.sort_unstable();
    accepted.iter().any(|a| {
        let mut got: Vec<&str> = a.precursors.iter().map(|p| p.0.as_str()).collect();
        got.sort_unstable();
        got == expected
    })
}

fn main() {
    let raw = std::fs::read_to_string(CATALOG_PATH).unwrap_or_else(|e| {
        panic!(
            "could not read {CATALOG_PATH}: {e}\n\
            regenerate it first:\n\
            \x20 python3 benchmarks/exploration_build_recall_manifest.py\n\
            \x20 python3 benchmarks/exploration_build_frozen_decoy_catalog.py"
        )
    });
    let catalog: FrozenCatalog = serde_json::from_str(&raw).expect(
        "benchmarks/data/exploration_frozen_catalog_manifest.json must be valid JSON -- \
        regenerate with benchmarks/exploration_build_frozen_decoy_catalog.py",
    );
    let baseline_raw = std::fs::read_to_string(BASELINE_PATH)
        .unwrap_or_else(|e| panic!("could not read committed baseline {BASELINE_PATH}: {e}"));

    let mut results = Vec::with_capacity(catalog.rows.len());
    let mut skipped_unrepresentable = 0usize;

    for row in &catalog.rows {
        let Some(target) = try_composition(&row.target_elements) else {
            skipped_unrepresentable += 1;
            continue;
        };
        let candidates: Option<Vec<PrecursorCandidate>> = row
            .candidates
            .iter()
            .map(|c| {
                try_composition(&c.elements).map(|composition| PrecursorCandidate {
                    id: PrecursorId(c.formula.clone()),
                    composition,
                    availability: None,
                })
            })
            .collect();
        let Some(candidates) = candidates else {
            skipped_unrepresentable += 1;
            continue;
        };

        let outcome = search_precursor_sets(
            &target,
            &candidates,
            &PlanningConstraints::default(),
            &SearchBudget::default(),
        )
        .expect("search_precursor_sets must not error on a well-formed synthetic catalog");

        let budget_exhausted = outcome
            .rejected
            .iter()
            .any(|r| r.reason_codes == vec![RejectionCode::SearchBudgetExhausted]);

        results.push(RowResult {
            target_formula: row.target_formula.clone(),
            route_arity: row.route.len(),
            recovered: route_recovered(&outcome.accepted, &row.route),
            budget_exhausted,
        });
    }

    let total = results.len();
    let recovered_count = results.iter().filter(|r| r.recovered).count();
    let exhausted_count = results.iter().filter(|r| r.budget_exhausted).count();

    let mut by_arity: BTreeMap<usize, (usize, usize, usize)> = BTreeMap::new();
    for r in &results {
        let entry = by_arity.entry(r.route_arity).or_insert((0, 0, 0));
        entry.0 += 1;
        if r.recovered {
            entry.1 += 1;
        }
        if r.budget_exhausted {
            entry.2 += 1;
        }
    }

    let recall = recovered_count as f64 / total.max(1) as f64;
    let exhaustion_rate = exhausted_count as f64 / total.max(1) as f64;
    let required_recall = BASELINE_RECALL * GATE_MULTIPLIER;
    let gate_pass = recall >= required_recall;

    println!(
        "gugen (acetate byproduct fix, on top of Phase 29 + nitrate + oxalate fixes) \
        exploration recall result"
    );
    println!("total rows: {total} ({skipped_unrepresentable} skipped, unrepresentable)");
    println!("recall (R1): {recovered_count}/{total} = {recall:.4}");
    println!("budget-exhaustion rate: {exhausted_count}/{total} = {exhaustion_rate:.4}");
    println!(
        "baseline (R0, v0.6.0): {BASELINE_RECALL:.4} -- Phase 29's own (unrelated) +20% gate \
        reference point: {required_recall:.4} ({})",
        if gate_pass { "PASS" } else { "FAIL" }
    );
    println!(
        "this fix is not gated on that threshold -- see the diff vs. the oxalate-fix result \
        below for its real, isolated effect"
    );
    println!("by route arity:");
    for (arity, (n, recovered, exhausted)) in &by_arity {
        println!(
            "  arity {arity}: {n} routes, recall {:.4}, exhaustion rate {:.4}",
            *recovered as f64 / *n.max(&1) as f64,
            *exhausted as f64 / *n.max(&1) as f64,
        );
    }

    // Stratify by whether each row was budget-exhausted under the
    // committed v0.6.0 baseline (per-row detail parsed from that file
    // directly, not re-derived) -- per this phase's own verification
    // plan, the gain should concentrate almost entirely in that bucket;
    // if it doesn't, the budget-semantics reinterpretation needs
    // auditing before this result is trusted at face value.
    let baseline_json: serde_json::Value =
        serde_json::from_str(&baseline_raw).expect("committed baseline must be valid JSON");
    let mut baseline_exhausted_by_target: BTreeMap<String, bool> = BTreeMap::new();
    if let Some(rows) = baseline_json["rows"].as_array() {
        for row in rows {
            if let (Some(t), Some(e)) = (
                row["target_formula"].as_str(),
                row["budget_exhausted"].as_bool(),
            ) {
                baseline_exhausted_by_target.insert(t.to_string(), e);
            }
        }
    }
    let (mut was_exhausted_n, mut was_exhausted_recovered) = (0usize, 0usize);
    let (mut was_not_exhausted_n, mut was_not_exhausted_recovered) = (0usize, 0usize);
    for r in &results {
        match baseline_exhausted_by_target.get(&r.target_formula) {
            Some(true) => {
                was_exhausted_n += 1;
                if r.recovered {
                    was_exhausted_recovered += 1;
                }
            }
            Some(false) => {
                was_not_exhausted_n += 1;
                if r.recovered {
                    was_not_exhausted_recovered += 1;
                }
            }
            None => {}
        }
    }
    println!("stratified by v0.6.0 budget-exhaustion status:");
    println!(
        "  was exhausted under v0.6.0: {was_exhausted_recovered}/{was_exhausted_n} recovered \
        under Phase 29 ({:.4})",
        was_exhausted_recovered as f64 / was_exhausted_n.max(1) as f64
    );
    println!(
        "  was NOT exhausted under v0.6.0: {was_not_exhausted_recovered}/{was_not_exhausted_n} \
        recovered under Phase 29 ({:.4})",
        was_not_exhausted_recovered as f64 / was_not_exhausted_n.max(1) as f64
    );

    // Direct row-by-row diff against the committed oxalate-fix result
    // (same catalog, same search algorithm, the only code difference is
    // curated_byproducts()'s new acetone entry) -- this isolates
    // exactly what this fix recovers, more precisely than any estimate
    // made ahead of running it.
    //
    // Deliberately indexed by row POSITION, not `target_formula`: 1866
    // of this catalog's 2798 rows share a `target_formula` with at
    // least one other row (up to 66 times -- distinct literature routes
    // to the same compound, e.g. four different BaTiO3 rows). An
    // earlier version of this diff keyed a `BTreeMap<String, bool>` by
    // `target_formula` alone, which silently collapsed those rows and
    // reported hundreds of bogus "newly lost" targets that were really
    // just a different row's prior status leaking through the
    // collision -- confirmed spurious by direct reproduction (every
    // BaTiO3 row's real recovered/not-recovered status was unchanged).
    // Row position is safe here because both runs read the byte-
    // identical (same sha256) catalog file in the same iteration order
    // with the same target/candidate-parseability skip predicate
    // (confirmed: identical `skipped_unrepresentable_rows` count and
    // zero `target_formula` mismatches at any shared index) -- asserted
    // below rather than assumed.
    let prior_raw = std::fs::read_to_string(PRIOR_RESULT_PATH).unwrap_or_else(|e| {
        panic!("could not read committed prior result {PRIOR_RESULT_PATH}: {e}")
    });
    let prior_json: serde_json::Value =
        serde_json::from_str(&prior_raw).expect("committed oxalate-fix result must be valid JSON");
    let prior_rows: &Vec<serde_json::Value> = prior_json["rows"]
        .as_array()
        .expect("committed oxalate-fix result must have a rows array");
    assert_eq!(
        prior_rows.len(),
        results.len(),
        "prior result and this run must have the same row count (same catalog, same skip \
        predicate) for a row-by-row diff to be meaningful"
    );
    let mut newly_recovered: Vec<&str> = Vec::new();
    let mut newly_lost: Vec<&str> = Vec::new();
    for (prior_row, r) in prior_rows.iter().zip(results.iter()) {
        let prior_target = prior_row["target_formula"]
            .as_str()
            .expect("prior row must have a target_formula");
        assert_eq!(
            prior_target, r.target_formula,
            "row order drift between the prior result and this run -- a positional diff is \
            unsafe here"
        );
        let prior_recovered = prior_row["recovered"]
            .as_bool()
            .expect("prior row must have a recovered bool");
        match (prior_recovered, r.recovered) {
            (false, true) => newly_recovered.push(r.target_formula.as_str()),
            (true, false) => newly_lost.push(r.target_formula.as_str()),
            _ => {}
        }
    }
    println!(
        "diff vs. committed oxalate-fix result (isolates this fix's effect): \
        {} newly recovered, {} newly lost",
        newly_recovered.len(),
        newly_lost.len()
    );
    println!("  newly recovered targets: {newly_recovered:?}");
    println!("  newly lost targets: {newly_lost:?}");

    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(
        "  \"description\": \"acetate-byproduct-fix exploration recall result -- not a gated \
        change, this file reports the real measured effect of adding acetone to \
        curated_byproducts() on top of Phase 29's frontier search and the merged nitrate/ \
        oxalate fixes\",\n",
    );
    out.push_str(&format!("  \"catalog_path\": {CATALOG_PATH:?},\n"));
    out.push_str(&format!("  \"catalog_sha256\": {:?},\n", sha256_hex(&raw)));
    out.push_str(&format!(
        "  \"baseline_path\": {BASELINE_PATH:?},\n  \"baseline_sha256\": {:?},\n",
        sha256_hex(&baseline_raw)
    ));
    out.push_str(&format!(
        "  \"prior_result_path\": {PRIOR_RESULT_PATH:?},\n  \"prior_result_sha256\": {:?},\n",
        sha256_hex(&prior_raw)
    ));
    out.push_str(&format!("  \"baseline_recall\": {BASELINE_RECALL:.6},\n"));
    out.push_str(&format!(
        "  \"gate_multiplier_reference_only\": {GATE_MULTIPLIER:.2},\n"
    ));
    out.push_str(&format!(
        "  \"required_recall_reference_only\": {required_recall:.6},\n"
    ));
    out.push_str(&format!("  \"total_rows\": {total},\n"));
    out.push_str(&format!(
        "  \"skipped_unrepresentable_rows\": {skipped_unrepresentable},\n"
    ));
    out.push_str(&format!("  \"recovered_count\": {recovered_count},\n"));
    out.push_str(&format!("  \"recall\": {recall:.6},\n"));
    out.push_str(&format!(
        "  \"budget_exhausted_count\": {exhausted_count},\n"
    ));
    out.push_str(&format!("  \"exhaustion_rate\": {exhaustion_rate:.6},\n"));
    out.push_str(&format!(
        "  \"gate_result_reference_only\": {:?},\n",
        if gate_pass { "PASS" } else { "FAIL" }
    ));
    out.push_str(&format!(
        "  \"newly_recovered_vs_oxalate_fix_count\": {},\n",
        newly_recovered.len()
    ));
    out.push_str(&format!(
        "  \"newly_lost_vs_oxalate_fix_count\": {},\n",
        newly_lost.len()
    ));
    out.push_str(&format!(
        "  \"newly_recovered_vs_oxalate_fix_targets\": {},\n",
        serde_json::to_string(&newly_recovered).unwrap()
    ));
    out.push_str(&format!(
        "  \"newly_lost_vs_oxalate_fix_targets\": {},\n",
        serde_json::to_string(&newly_lost).unwrap()
    ));
    out.push_str("  \"stratified_by_v0_6_0_exhaustion_status\": {\n");
    out.push_str(&format!(
        "    \"was_exhausted\": {{\"total\": {was_exhausted_n}, \"recovered\": \
        {was_exhausted_recovered}, \"recall\": {:.6}}},\n",
        was_exhausted_recovered as f64 / was_exhausted_n.max(1) as f64
    ));
    out.push_str(&format!(
        "    \"was_not_exhausted\": {{\"total\": {was_not_exhausted_n}, \"recovered\": \
        {was_not_exhausted_recovered}, \"recall\": {:.6}}}\n",
        was_not_exhausted_recovered as f64 / was_not_exhausted_n.max(1) as f64
    ));
    out.push_str("  },\n");
    out.push_str("  \"by_route_arity\": {\n");
    let arity_entries: Vec<String> = by_arity
        .iter()
        .map(|(arity, (n, recovered, exhausted))| {
            format!(
                "    \"{arity}\": {{\"total\": {n}, \"recovered\": {recovered}, \
                \"budget_exhausted\": {exhausted}, \"recall\": {:.6}, \
                \"exhaustion_rate\": {:.6}}}",
                *recovered as f64 / *n.max(&1) as f64,
                *exhausted as f64 / *n.max(&1) as f64,
            )
        })
        .collect();
    out.push_str(&arity_entries.join(",\n"));
    out.push_str("\n  },\n");
    out.push_str("  \"rows\": [\n");
    let row_entries: Vec<String> = results
        .iter()
        .map(|r| {
            format!(
                "    {{\"target_formula\": {:?}, \"route_arity\": {}, \"recovered\": {}, \
                \"budget_exhausted\": {}}}",
                r.target_formula, r.route_arity, r.recovered, r.budget_exhausted
            )
        })
        .collect();
    out.push_str(&row_entries.join(",\n"));
    out.push_str("\n  ]\n");
    out.push_str("}\n");

    std::fs::write(OUTPUT_PATH, &out).expect("failed to write result");
    println!("wrote {OUTPUT_PATH}");
}

/// Minimal, dependency-free SHA-256 -- identical implementation to
/// `exploration_recall_baseline.rs`'s own (kept as an independent copy;
/// each `examples/*.rs` binary compiles standalone in this crate's
/// existing convention, see that file's own doc comment for why adding
/// a `sha2` dependency isn't worth it for one checksum use).
fn sha256_hex(input: &str) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = input.as_bytes().to_vec();
    let bit_len = (msg.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let (mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh) =
            (h[0], h[1], h[2], h[3], h[4], h[5], h[6], h[7]);
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    h.iter().map(|word| format!("{word:08x}")).collect()
}
