//! Phase 21B calibration: computes `balanced_reaction_delta_ev_per_atom`
//! for every row `benchmarks/build_phase21b_calibration_input.py`
//! prepared (target/route formulas already parsed to element amounts,
//! OQMD thermodynamic data already resolved by formula) and writes a
//! per-row result file. Does **no** statistical analysis, no
//! representative-pair selection, no gate verdict -- that happens in
//! `benchmarks/analyze_phase21b_calibration.py`, matching this
//! project's established Rust-computes/Python-analyzes split
//! (`analyze_oqmd_coverage_gate.py`'s own precedent).
//!
//! Per `docs/phase21b_calibration_preregistration.md`'s corrected
//! methodology: `balance()` is called with the target alone as the
//! product side (no byproduct search -- a byproduct-needing reaction
//! would only ever abstain to `Ok(None)` downstream anyway, since this
//! pipeline never fabricates a solid-phase thermodynamic entry for a
//! gas/liquid byproduct). A row only counts as "balanced" if `target`
//! *and* every one of the row's own declared route precursors survives
//! with a strictly positive coefficient in the result (extending PR
//! 78's "target must survive" fix to every declared reactant) --
//! otherwise gugen would silently be scoring a smaller sub-reaction
//! than the one the literature actually reported.
//!
//! Run: `cargo run --release --example exploration_phase21b_calibration --features serde`
//! (after `python3 benchmarks/build_phase21b_calibration_input.py`).
//! Writes `benchmarks/data/exploration_phase21b_calibration_result.json`.

use gugen::{
    Composition, Element, Kelvin, SolidThermodynamicEntry, ThermodynamicDatasetIdentity, balance,
    balanced_reaction_delta_ev_per_atom,
};
use std::collections::BTreeMap;

const INPUT_PATH: &str = "benchmarks/data/phase21b_calibration_input.json";
const OUTPUT_PATH: &str = "benchmarks/data/exploration_phase21b_calibration_result.json";

#[derive(serde::Deserialize)]
struct InputPrecursor {
    formula: String,
    elements: BTreeMap<String, f64>,
}

#[derive(serde::Deserialize)]
struct InputRow {
    target_formula: String,
    target_elements: BTreeMap<String, f64>,
    route: Vec<InputPrecursor>,
    verdict: String,
    dois: Vec<String>,
}

#[derive(serde::Deserialize)]
struct ThermoRaw {
    delta_e_ev_per_atom: f64,
    volume_angstrom3_per_atom: f64,
}

#[derive(serde::Deserialize)]
struct InputFile {
    source_manifest_checksum: String,
    rows: Vec<InputRow>,
    thermodynamic_entries: BTreeMap<String, ThermoRaw>,
}

fn try_composition(elements: &BTreeMap<String, f64>) -> Option<Composition> {
    let pairs: Option<Vec<(Element, f64)>> = elements
        .iter()
        .map(|(sym, amt)| Element::new(sym).ok().map(|e| (e, *amt)))
        .collect();
    Composition::new(pairs?).ok()
}

fn thermo_entry(
    raw: &ThermoRaw,
    composition: Composition,
    formula: &str,
    dataset: &ThermodynamicDatasetIdentity,
) -> Option<SolidThermodynamicEntry> {
    SolidThermodynamicEntry::new(
        composition,
        Some(formula.to_string()),
        raw.delta_e_ev_per_atom,
        raw.volume_angstrom3_per_atom,
        dataset.clone(),
    )
    .ok()
}

fn main() {
    let raw = std::fs::read_to_string(INPUT_PATH)
        .unwrap_or_else(|e| panic!("failed to read {INPUT_PATH}: {e}"));
    let input: InputFile =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{INPUT_PATH} must be valid: {e}"));

    let dataset = ThermodynamicDatasetIdentity {
        source: "OQMD".to_string(),
        release: "gugen Phase 21B condition 1 manifest, 2026-08-23".to_string(),
        compatibility_scheme: "OQMD delta_e (eV/atom, 0K DFT formation energy) used directly as \
            formation_enthalpy_ev_per_atom, per docs/thermodynamic_selectivity_calibration.md §6.3"
            .to_string(),
        snapshot_checksum: input.source_manifest_checksum.clone(),
    };

    let temperatures = [
        (
            "t300",
            Kelvin::new(300.0).expect("300K is within gugen's valid range"),
        ),
        (
            "t1800",
            Kelvin::new(1800.0).expect("1800K is within gugen's valid range"),
        ),
    ];

    let mut rows_out: Vec<String> = Vec::with_capacity(input.rows.len());
    let mut balanced_count = 0usize;
    let mut unbalanced_count = 0usize;
    let mut unparseable_count = 0usize;
    let mut abstained_at_300k = 0usize;

    for row in &input.rows {
        let Some(target) = try_composition(&row.target_elements) else {
            unparseable_count += 1;
            continue;
        };
        let mut reactants = Vec::with_capacity(row.route.len());
        let mut ok = true;
        for p in &row.route {
            let Some(c) = try_composition(&p.elements) else {
                ok = false;
                break;
            };
            reactants.push(c);
        }
        if !ok {
            unparseable_count += 1;
            continue;
        }

        let results = match balance(&reactants, std::slice::from_ref(&target)) {
            Ok(r) => r,
            Err(_) => {
                unbalanced_count += 1;
                rows_out.push(row_json(row, false, None, None));
                continue;
            }
        };

        let genuine = results.into_iter().find(|reaction| {
            reaction.products().iter().any(|s| s.composition == target)
                && reaction.reactants().len() == reactants.len()
        });

        let Some(reaction) = genuine else {
            unbalanced_count += 1;
            rows_out.push(row_json(row, false, None, None));
            continue;
        };
        balanced_count += 1;

        // Build entries for every species this row's own formulas name --
        // balanced_reaction_delta_ev_per_atom looks these up by composition
        // and abstains (Ok(None)) for anything not present, so it's safe
        // to hand it every formula this row has thermodynamic data for.
        let mut entries = Vec::new();
        if let Some(raw_target) = input.thermodynamic_entries.get(&row.target_formula) {
            if let Some(e) = thermo_entry(raw_target, target.clone(), &row.target_formula, &dataset)
            {
                entries.push(e);
            }
        }
        for p in &row.route {
            if let Some(raw_p) = input.thermodynamic_entries.get(&p.formula) {
                let Some(c) = try_composition(&p.elements) else {
                    continue;
                };
                if let Some(e) = thermo_entry(raw_p, c, &p.formula, &dataset) {
                    entries.push(e);
                }
            }
        }

        let mut deltas: BTreeMap<&'static str, Option<f64>> = BTreeMap::new();
        for (label, t) in &temperatures {
            let delta =
                balanced_reaction_delta_ev_per_atom(&reaction, &entries, *t).unwrap_or(None);
            deltas.insert(label, delta);
        }
        if deltas.get("t300").copied().flatten().is_none() {
            abstained_at_300k += 1;
        }

        rows_out.push(row_json(
            row,
            true,
            deltas.get("t300").copied().flatten(),
            deltas.get("t1800").copied().flatten(),
        ));
    }

    println!("Phase 21B calibration: per-row balance() + energy computation");
    println!(
        "total rows: {}; unparseable (should be 0, input already pre-filtered): {}",
        input.rows.len(),
        unparseable_count
    );
    println!("balanced (target + every declared precursor survives): {balanced_count}");
    println!("not balanced (excluded, not guessed at): {unbalanced_count}");
    println!(
        "balanced but abstained at 300K (a species lacks thermodynamic data): {abstained_at_300k}"
    );

    let out = format!(
        "{{\n  \"description\": \"Phase 21B calibration -- per-row balance()+energy result. \
        balanced=false rows were excluded from balance()/energy computation entirely, not \
        assigned a guessed energy. delta_ev_per_atom_t300/t1800 are null when balanced but a \
        species lacks OQMD thermodynamic data (a legitimate Ok(None) abstention, not a \
        failure).\",\n  \"source_manifest_checksum\": {:?},\n  \"total_rows\": {},\n  \
        \"unparseable\": {unparseable_count},\n  \"balanced\": {balanced_count},\n  \
        \"not_balanced\": {unbalanced_count},\n  \"abstained_at_300k\": {abstained_at_300k},\n  \
        \"rows\": [\n{}\n  ]\n}}\n",
        input.source_manifest_checksum,
        input.rows.len(),
        rows_out.join(",\n")
    );
    std::fs::write(OUTPUT_PATH, &out).expect("failed to write result");
    println!("wrote {OUTPUT_PATH}");
}

fn row_json(
    row: &InputRow,
    balanced: bool,
    delta_300: Option<f64>,
    delta_1800: Option<f64>,
) -> String {
    let route_formulas: Vec<String> = row
        .route
        .iter()
        .map(|p| format!("{:?}", p.formula))
        .collect();
    format!(
        "    {{\"target_formula\": {:?}, \"route\": [{}], \"verdict\": {:?}, \"dois\": {:?}, \
        \"balanced\": {balanced}, \"delta_ev_per_atom_t300\": {}, \"delta_ev_per_atom_t1800\": {}}}",
        row.target_formula,
        route_formulas.join(", "),
        row.verdict,
        row.dois,
        delta_300
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string()),
        delta_1800
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string()),
    )
}
