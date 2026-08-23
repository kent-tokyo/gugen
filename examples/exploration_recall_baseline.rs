//! Phase 28 (Exploration Benchmark Lock): computes today's
//! (pre-Phase-29) precursor-set recall against the frozen, deliberately-
//! pressured decoy catalog, and writes the immutable baseline result
//! Phase 29's own gate is measured against.
//!
//! Run: `cargo run --example exploration_recall_baseline --features serde`
//! after regenerating the (gitignored, too large to commit -- see
//! `benchmarks/exploration_build_frozen_decoy_catalog.py`'s own module
//! doc) input locally:
//!   python3 benchmarks/exploration_build_recall_manifest.py
//!   python3 benchmarks/exploration_build_frozen_decoy_catalog.py
//!
//! Reads `benchmarks/data/exploration_frozen_catalog_manifest.json` at
//! *runtime* (`std::fs::read_to_string`, deliberately not `include_str!`
//! -- that file is gitignored and regenerable, so a fresh checkout must
//! still compile cleanly without it existing; only running this example
//! needs it present).
//!
//! For each row, calls `search_precursor_sets` directly (not the full
//! `Planner::plan` -- this benchmark is about the search algorithm
//! itself, not process templates or scoring) with
//! `SearchBudget::default()`, exactly as `Planner::plan` does in
//! production. "Recovered" means the row's own known route (a set of
//! precursor formulas) is present, as a set, among `outcome.accepted`
//! -- the identical comparison `tests/exploration_recall.rs` pins down
//! with small synthetic fixtures.
//!
//! Writes `benchmarks/data/exploration_baseline_v0_6_0.json`: per-row
//! results plus the headroom diagnostics Phase 28's own gate criterion
//! #4 needs (recall R0, budget-exhaustion rate, and recall/exhaustion
//! broken out by route arity -- today's search truncates from the
//! high-arity end, so arity-4 routes are where headroom should
//! concentrate). This file's own filename embeds the version it was
//! computed against (`v0_6_0`) and is never overwritten by a later
//! Phase 29 run, which writes a differently-named result file instead
//! (`exploration_result_v0_7_0.json`) -- immutability by construction,
//! not by convention alone.

use gugen::{
    Composition, Element, PlanningConstraints, PrecursorCandidate, PrecursorId, RejectionCode,
    SearchBudget, search_precursor_sets,
};
use std::collections::BTreeMap;
use std::path::Path;

const CATALOG_PATH: &str = "benchmarks/data/exploration_frozen_catalog_manifest.json";
const OUTPUT_PATH: &str = "benchmarks/data/exploration_baseline_v0_6_0.json";

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
/// own `try_composition` precedent -- never trust an external JSON
/// fixture's own filtering blindly, even one this same repo's Python
/// side produced (AGENTS.md §25 forbids panicking on ordinary input).
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

    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"gugen_version\": \"0.6.0\",\n");
    out.push_str(&format!("  \"catalog_path\": {CATALOG_PATH:?},\n"));
    out.push_str(&format!("  \"catalog_sha256\": {:?},\n", sha256_hex(&raw)));
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

    std::fs::write(OUTPUT_PATH, &out).expect("failed to write baseline result");

    println!("gugen 0.6.0 exploration recall baseline");
    println!("total rows: {total} ({skipped_unrepresentable} skipped, unrepresentable)");
    println!("recall (R0): {recovered_count}/{total} = {recall:.4}");
    println!("budget-exhaustion rate: {exhausted_count}/{total} = {exhaustion_rate:.4}");
    println!("by route arity:");
    for (arity, (n, recovered, exhausted)) in &by_arity {
        println!(
            "  arity {arity}: {n} routes, recall {:.4}, exhaustion rate {:.4}",
            *recovered as f64 / *n.max(&1) as f64,
            *exhausted as f64 / *n.max(&1) as f64,
        );
    }
    println!("wrote {OUTPUT_PATH}");
    let _ = Path::new(OUTPUT_PATH);
}

/// Minimal, dependency-free SHA-256 (this crate has no `sha2`/hashing
/// dependency anywhere; adding one purely for a baseline-file checksum
/// used only by this benchmark script isn't worth a new dependency --
/// this is a direct, unremarkable implementation of the standard
/// algorithm, not a cryptographic-security-sensitive use).
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
