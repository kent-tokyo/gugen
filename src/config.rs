/// Bounds on the deterministic precursor-set search (AGENTS.md §9).
/// Exhausting the budget must be reported, never silently treated as "no
/// candidates" (AGENTS.md §9: "budget不足を「候補なし」と混同してはいけません").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SearchBudget {
    pub max_precursor_sets: usize,
    pub max_precursors_per_plan: usize,
    pub max_plans_returned: usize,
}

impl Default for SearchBudget {
    fn default() -> Self {
        // ponytail: arbitrary-but-documented starting bounds, not a
        // scientific claim. Revisit once Phase 3's search is implemented
        // and can be measured against real catalogs.
        Self {
            max_precursor_sets: 10_000,
            max_precursors_per_plan: 4,
            max_plans_returned: 20,
        }
    }
}

/// Top-level planner configuration (AGENTS.md §18). Grows in later phases
/// (e.g. `RankingWeights` is added once Phase 5 lands ranking).
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlanningConfig {
    pub search_budget: SearchBudget,
    pub deterministic_seed: u64,
}
