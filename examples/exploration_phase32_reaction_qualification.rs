//! Phase 32, Section 4: real `balance()` attempts for every row
//! `benchmarks/build_phase32_qualification_input.py` marked
//! `"stage": "needs_balance"` (element sets already compatible: route
//! equals target exactly, or differs only by elements in the
//! conservative {C, H, O} allow-list). Rows the Python pre-pass
//! already terminally classified (formula-unsupported,
//! target/precursor element mismatch, dopant/host ambiguous) are
//! passed straight through, unattempted.
//!
//! For each row this tries: (a) `balance()` with the target alone as
//! the product side (matches Phase 21B's own corrected "as-declared"
//! methodology), and (b) for each byproduct candidate the Python pass
//! proposed (only ever CO2/H2O/O2, never gugen's full six-species
//! `curated_byproducts()`), `balance()` with `[target, candidate]` as
//! the product side. A result only counts as a genuine reaction if
//! `target` *and* every declared route precursor survives with a
//! strictly positive coefficient -- otherwise gugen would silently be
//! scoring a smaller sub-reaction than the one actually declared.
//!
//! This file makes no status decision (BalancedAsDeclared vs.
//! Unbalanceable vs. ...WithCompletion) -- it only reports the raw
//! facts (`as_declared_outcome`, `successful_byproduct_candidates`).
//! `benchmarks/analyze_phase32_qualification.py` applies the state
//! machine, per this project's established Rust-computes/
//! Python-analyzes split.
//!
//! Run: `cargo run --release --example exploration_phase32_reaction_qualification --features serde`
//! (after `python3 benchmarks/build_phase32_qualification_input.py`).

use gugen::{BalancedReaction, Composition, Element, balance};
use std::collections::BTreeMap;

const INPUT_PATH: &str = "benchmarks/data/phase32_qualification_input.json";
const OUTPUT_PATH: &str = "benchmarks/data/exploration_phase32_reaction_qualification_result.json";

#[derive(serde::Deserialize)]
struct InputRow {
    row_id: String,
    stage: String,
    target_formula: String,
    #[serde(default)]
    target_elements: Option<BTreeMap<String, f64>>,
    route_formulas: Vec<String>,
    #[serde(default)]
    route_elements: Vec<Option<BTreeMap<String, f64>>>,
    #[serde(default)]
    byproduct_candidates: Vec<String>,
}

#[derive(serde::Serialize)]
struct OutputRow {
    row_id: String,
    as_declared_outcome: &'static str,
    successful_byproduct_candidates: Vec<String>,
    // Human-readable rendering of the winning reaction, using the
    // row's own original formula strings -- for Section 7's manual
    // audit. Only set when a genuine (all-positive) balance was found,
    // either as-declared or via the unique successful candidate above.
    balanced_equation: Option<String>,
}

// Renders a winning reaction back into original formula strings (not
// re-derived element symbols) by matching each species' composition
// against the row's own (formula, composition) pairs -- exact
// composition equality, since two distinct declared precursors in one
// row sharing an identical composition would itself be a data problem
// worth surfacing separately, not silently disambiguated here.
fn render_equation(
    reaction: &BalancedReaction,
    reactant_formulas: &[(String, Composition)],
    target_formula: &str,
    target: &Composition,
    byproduct_name: Option<&str>,
    byproduct: Option<&Composition>,
) -> String {
    let label = |composition: &Composition| -> String {
        if composition == target {
            return target_formula.to_string();
        }
        if let Some((name, bp)) = byproduct_name.zip(byproduct) {
            if composition == bp {
                return name.to_string();
            }
        }
        reactant_formulas
            .iter()
            .find(|(_, c)| c == composition)
            .map(|(f, _)| f.clone())
            .unwrap_or_else(|| "<unmatched>".to_string())
    };
    let side = |species: &[gugen::ReactionSpecies]| -> String {
        species
            .iter()
            .map(|s| format!("{} {}", s.coefficient(), label(&s.composition)))
            .collect::<Vec<_>>()
            .join(" + ")
    };
    format!(
        "{} -> {}",
        side(reaction.reactants()),
        side(reaction.products())
    )
}

fn try_composition(elements: &BTreeMap<String, f64>) -> Option<Composition> {
    let pairs: Option<Vec<(Element, f64)>> = elements
        .iter()
        .map(|(sym, amt)| Element::new(sym).ok().map(|e| (e, *amt)))
        .collect();
    Composition::new(pairs?).ok()
}

fn byproduct_composition(name: &str) -> Composition {
    let c = Element::new("C").expect("C is a valid element");
    let h = Element::new("H").expect("H is a valid element");
    let o = Element::new("O").expect("O is a valid element");
    match name {
        "CO2" => Composition::new([(c, 1.0), (o, 2.0)]).expect("CO2 is a valid composition"),
        "H2O" => Composition::new([(h, 2.0), (o, 1.0)]).expect("H2O is a valid composition"),
        "O2" => Composition::new([(o, 2.0)]).expect("O2 is a valid composition"),
        other => panic!("unknown byproduct candidate {other:?} -- allow-list is CO2/H2O/O2 only"),
    }
}

enum Outcome {
    AllPositive(BalancedReaction),
    SolutionNotAllPositive,
    NoSolution,
}

fn attempt(target: &Composition, reactants: &[Composition], products: &[Composition]) -> Outcome {
    match balance(reactants, products) {
        Ok(results) if !results.is_empty() => {
            let genuine = results.into_iter().find(|r| {
                r.products().iter().any(|s| s.composition == *target)
                    && r.reactants().len() == reactants.len()
            });
            match genuine {
                Some(reaction) => Outcome::AllPositive(reaction),
                None => Outcome::SolutionNotAllPositive,
            }
        }
        _ => Outcome::NoSolution,
    }
}

fn main() {
    let raw = std::fs::read_to_string(INPUT_PATH)
        .unwrap_or_else(|e| panic!("failed to read {INPUT_PATH}: {e}"));
    let rows: Vec<InputRow> =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{INPUT_PATH} must be valid: {e}"));

    let mut out = Vec::new();
    let mut attempted = 0usize;
    let mut all_positive_as_declared = 0usize;

    for row in &rows {
        if row.stage != "needs_balance" {
            continue;
        }
        attempted += 1;

        let Some(target) = row.target_elements.as_ref().and_then(try_composition) else {
            panic!(
                "row {} marked needs_balance but target didn't parse",
                row.row_id
            );
        };
        let reactants: Vec<Composition> = row
            .route_elements
            .iter()
            .map(|e| {
                e.as_ref().and_then(try_composition).unwrap_or_else(|| {
                    panic!(
                        "row {} marked needs_balance but a precursor didn't parse",
                        row.row_id
                    )
                })
            })
            .collect();
        let reactant_formulas: Vec<(String, Composition)> = row
            .route_formulas
            .iter()
            .cloned()
            .zip(reactants.iter().cloned())
            .collect();

        let as_declared = attempt(&target, &reactants, std::slice::from_ref(&target));
        let as_declared_outcome = match &as_declared {
            Outcome::AllPositive(_) => "all_positive",
            Outcome::SolutionNotAllPositive => "solution_not_all_positive",
            Outcome::NoSolution => "no_solution",
        };
        let mut equation = match &as_declared {
            Outcome::AllPositive(reaction) => Some(render_equation(
                reaction,
                &reactant_formulas,
                &row.target_formula,
                &target,
                None,
                None,
            )),
            _ => None,
        };
        if matches!(as_declared, Outcome::AllPositive(_)) {
            all_positive_as_declared += 1;
        }

        let mut successful = Vec::new();
        let mut successful_reactions = Vec::new();
        if !matches!(as_declared, Outcome::AllPositive(_)) {
            for name in &row.byproduct_candidates {
                let candidate = byproduct_composition(name);
                let products = [target.clone(), candidate.clone()];
                if let Outcome::AllPositive(reaction) = attempt(&target, &reactants, &products) {
                    successful.push(name.clone());
                    successful_reactions.push((name.clone(), candidate, reaction));
                }
            }
        }
        if successful.len() == 1 {
            let (name, candidate, reaction) = &successful_reactions[0];
            equation = Some(render_equation(
                reaction,
                &reactant_formulas,
                &row.target_formula,
                &target,
                Some(name.as_str()),
                Some(candidate),
            ));
        }

        out.push(OutputRow {
            row_id: row.row_id.clone(),
            as_declared_outcome,
            successful_byproduct_candidates: successful,
            balanced_equation: equation,
        });
    }

    println!("Phase 32: real balance() attempts");
    println!("rows attempted (stage == needs_balance): {attempted}");
    println!("all-positive as-declared (no byproduct needed): {all_positive_as_declared}");

    let json = serde_json::to_string_pretty(&out).expect("output is always serializable");
    std::fs::write(OUTPUT_PATH, json).expect("failed to write result");
    println!("wrote {OUTPUT_PATH}");
}
