//! gugen playground WASM boundary. One exported function, string in,
//! string out -- avoids hand-writing wasm-bindgen struct bindings for
//! gugen's own types (unnecessary: `SynthesisPlanningReport` already
//! round-trips through `serde_json` cleanly).
//!
//! This crate is the real trust boundary for the playground: every safety
//! limit below is enforced here, on parsed input, before any call into
//! gugen -- not just in the JS UI, since anyone can call the exported wasm
//! function directly from devtools. No network access anywhere in this
//! crate or the web frontend that calls it.

use gugen::{
    Composition, Element, InMemoryPrecursorCatalog, Planner, PlanningConfig, PlanningConstraints,
    PrecursorCandidate, PrecursorId, SearchBudget, TargetSpecification,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use wasm_bindgen::prelude::*;

const MAX_INPUT_BYTES: usize = 256 * 1024;
const MAX_TARGET_ELEMENTS: usize = 12;
const MAX_CANDIDATES: usize = 60;
const MAX_PRECURSORS_PER_PLAN: usize = 6;
const MAX_PRECURSOR_SETS: usize = 50_000;
const MAX_PLANS_RETURNED: usize = 50;
const MAX_SYMBOL_LEN: usize = 40;

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

#[wasm_bindgen]
pub fn plan_synthesis(input_json: &str) -> String {
    match run(input_json) {
        Ok(report_json) => report_json,
        Err(message) => serde_json::json!({ "error": message }).to_string(),
    }
}

#[derive(Deserialize)]
struct PlanRequest {
    target_elements: BTreeMap<String, f64>,
    candidates: Vec<CandidateInput>,
    #[serde(default)]
    max_precursor_sets: Option<usize>,
    #[serde(default)]
    max_precursors_per_plan: Option<usize>,
    #[serde(default)]
    max_plans_returned: Option<usize>,
    execution_timestamp: String,
}

#[derive(Deserialize)]
struct CandidateInput {
    id: String,
    elements: BTreeMap<String, f64>,
}

fn run(input_json: &str) -> Result<String, String> {
    if input_json.len() > MAX_INPUT_BYTES {
        return Err(format!(
            "input is {} bytes, exceeding the {MAX_INPUT_BYTES}-byte limit",
            input_json.len()
        ));
    }

    let request: PlanRequest =
        serde_json::from_str(input_json).map_err(|e| format!("malformed request: {e}"))?;

    if request.target_elements.is_empty() {
        return Err("target_elements must not be empty".to_string());
    }
    if request.target_elements.len() > MAX_TARGET_ELEMENTS {
        return Err(format!(
            "target has {} elements, exceeding the {MAX_TARGET_ELEMENTS}-element limit",
            request.target_elements.len()
        ));
    }
    if request.candidates.len() > MAX_CANDIDATES {
        return Err(format!(
            "{} candidates supplied, exceeding the {MAX_CANDIDATES}-candidate limit",
            request.candidates.len()
        ));
    }
    check_symbol_lengths(request.target_elements.keys())?;
    for candidate in &request.candidates {
        if candidate.id.len() > MAX_SYMBOL_LEN {
            return Err(format!(
                "candidate id '{}' exceeds the {MAX_SYMBOL_LEN}-character limit",
                candidate.id
            ));
        }
        check_symbol_lengths(candidate.elements.keys())?;
    }

    let target_composition = composition_from_map(&request.target_elements)?;
    let candidates = request
        .candidates
        .iter()
        .map(|c| {
            Ok(PrecursorCandidate {
                id: PrecursorId(c.id.clone()),
                composition: composition_from_map(&c.elements)?,
                availability: None,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let search_budget = SearchBudget {
        max_precursor_sets: request
            .max_precursor_sets
            .unwrap_or(SearchBudget::default().max_precursor_sets)
            .min(MAX_PRECURSOR_SETS),
        max_precursors_per_plan: request
            .max_precursors_per_plan
            .unwrap_or(SearchBudget::default().max_precursors_per_plan)
            .min(MAX_PRECURSORS_PER_PLAN),
        max_plans_returned: request
            .max_plans_returned
            .unwrap_or(SearchBudget::default().max_plans_returned)
            .min(MAX_PLANS_RETURNED),
    };

    let target = TargetSpecification {
        composition: target_composition,
        structure: None,
        desired_phase: None,
        constraints: PlanningConstraints::default(),
    };
    let catalog = InMemoryPrecursorCatalog::new(candidates);
    let planner = Planner::builder(
        catalog,
        PlanningConfig {
            search_budget,
            ..PlanningConfig::default()
        },
    )
    .build();

    let report = planner
        .plan(&target, &request.execution_timestamp)
        .map_err(|e| e.to_string())?;

    serde_json::to_string(&report).map_err(|e| format!("failed to serialize report: {e}"))
}

fn check_symbol_lengths<'a>(symbols: impl Iterator<Item = &'a String>) -> Result<(), String> {
    for symbol in symbols {
        if symbol.len() > MAX_SYMBOL_LEN {
            return Err(format!(
                "element symbol '{symbol}' exceeds the {MAX_SYMBOL_LEN}-character limit"
            ));
        }
    }
    Ok(())
}

fn composition_from_map(elements: &BTreeMap<String, f64>) -> Result<Composition, String> {
    let pairs = elements
        .iter()
        .map(|(symbol, amount)| {
            let element = Element::new(symbol).map_err(|e| e.to_string())?;
            Ok((element, *amount))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Composition::new(pairs).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn batio3_request() -> String {
        serde_json::json!({
            "target_elements": {"Ba": 1.0, "Ti": 1.0, "O": 3.0},
            "candidates": [
                {"id": "BaCO3", "elements": {"Ba": 1.0, "C": 1.0, "O": 3.0}},
                {"id": "TiO2", "elements": {"Ti": 1.0, "O": 2.0}},
                {"id": "NaCl", "elements": {"Na": 1.0, "Cl": 1.0}},
            ],
            "execution_timestamp": "2026-08-25T00:00:00Z",
        })
        .to_string()
    }

    #[test]
    fn a_valid_batio3_request_recovers_the_cited_route() {
        let result = run(&batio3_request()).expect("must succeed");
        let report: serde_json::Value = serde_json::from_str(&result).unwrap();
        let plans = report["plans"].as_array().expect("plans array");
        assert!(
            plans.iter().any(|p| {
                let ids: Vec<&str> = p["precursors"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|s| s["precursor"].as_str().unwrap())
                    .collect();
                ids.contains(&"BaCO3") && ids.contains(&"TiO2") && ids.len() == 2
            }),
            "expected BaCO3 + TiO2 route in {plans:#?}"
        );
    }

    #[test]
    fn plan_synthesis_returns_a_structured_error_not_a_panic_on_malformed_json() {
        let result = plan_synthesis("not json");
        let value: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(value["error"].is_string());
    }

    #[test]
    fn oversized_target_element_count_is_rejected() {
        let mut target_elements = serde_json::Map::new();
        for symbol in [
            "H", "He", "Li", "Be", "B", "C", "N", "O", "F", "Ne", "Na", "Mg", "Al",
        ] {
            target_elements.insert(symbol.to_string(), serde_json::json!(1.0));
        }
        let request = serde_json::json!({
            "target_elements": target_elements,
            "candidates": [],
            "execution_timestamp": "2026-08-25T00:00:00Z",
        })
        .to_string();
        let err = run(&request).unwrap_err();
        assert!(err.contains("exceeding"), "unexpected error: {err}");
    }

    #[test]
    fn oversized_candidate_count_is_rejected() {
        let candidates: Vec<_> = (0..MAX_CANDIDATES + 1)
            .map(|i| serde_json::json!({"id": format!("X{i}"), "elements": {"Fe": 1.0}}))
            .collect();
        let request = serde_json::json!({
            "target_elements": {"Fe": 1.0},
            "candidates": candidates,
            "execution_timestamp": "2026-08-25T00:00:00Z",
        })
        .to_string();
        let err = run(&request).unwrap_err();
        assert!(err.contains("exceeding"), "unexpected error: {err}");
    }

    #[test]
    fn oversized_input_is_rejected_before_parsing() {
        let padded = format!("{{\"padding\": \"{}\"}}", "x".repeat(MAX_INPUT_BYTES + 1));
        let err = run(&padded).unwrap_err();
        assert!(err.contains("bytes"), "unexpected error: {err}");
    }

    #[test]
    fn search_budget_request_is_clamped_not_trusted() {
        let request = serde_json::json!({
            "target_elements": {"Ba": 1.0, "Ti": 1.0, "O": 3.0},
            "candidates": [
                {"id": "BaCO3", "elements": {"Ba": 1.0, "C": 1.0, "O": 3.0}},
                {"id": "TiO2", "elements": {"Ti": 1.0, "O": 2.0}},
            ],
            "max_precursor_sets": 999_999_999,
            "max_precursors_per_plan": 999,
            "max_plans_returned": 999,
            "execution_timestamp": "2026-08-25T00:00:00Z",
        })
        .to_string();
        // Must not hang or reject outright -- clamped internally, then a
        // normal (fast) search runs against the tiny candidate pool above.
        let result = run(&request).expect("clamped budget must still succeed");
        assert!(result.contains("BaCO3"));
    }

    #[test]
    fn an_invalid_element_symbol_is_a_structured_error_not_a_panic() {
        let request = serde_json::json!({
            "target_elements": {"Xx": 1.0},
            "candidates": [],
            "execution_timestamp": "2026-08-25T00:00:00Z",
        })
        .to_string();
        let err = run(&request).unwrap_err();
        assert!(!err.is_empty());
    }
}
