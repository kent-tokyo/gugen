# Synthesis Planning Report (schema v1)

**Target:** Ba:1, O:3, Ti:1

**Applicability:** PartiallyInDomain -- formula-only target, no structure provided (AGENTS.md §16's own example for this level)

## Plan plan-a702f5b0380d3716 (score 0.062)

- Target: Ba:1, O:3, Ti:1
- Route family: ConventionalSolidState
- Reaction: 1x(Ba:1, C:1, O:3) + 1x(O:2, Ti:1) -> 1x(Ba:1, O:3, Ti:1) + 1x(C:1, O:2)
- Manual review required: true
- Applicability: PartiallyInDomain -- formula-only target, no structure provided (AGENTS.md §16's own example for this level)

### Steps

- [Required] Weigh: BaCO3 x1, TiO2 x1
- [Required] Mix (DryMixing)
- [Required] Grind (MortarAndPestle), duration=unresolved
- [Optional] Form (UniaxialPressing), pressure=unresolved
- [Required] Heat (Calcination): temperature=unresolved, duration=unresolved, atmosphere=unresolved, ramp=unresolved
- [Recommended] Grind (MortarAndPestle), duration=unresolved
- [Required] Heat (Sintering): temperature=unresolved, duration=unresolved, atmosphere=unresolved, ramp=unresolved
- [Required] Cool (FurnaceCooling)
- [Recommended] Characterize (Xrd): verify target-phase formation

### Score breakdown

```
PlanScoreBreakdown {
    stoichiometric_validity: Score01(
        1.0,
    ),
    precursor_coverage: Score01(
        1.0,
    ),
    thermodynamic_support: None,
    process_simplicity: Score01(
        0.0,
    ),
    evidence_strength: Score01(
        0.25,
    ),
    safety_penalty: Score01(
        0.0,
    ),
    uncertainty_penalty: Score01(
        1.0,
    ),
    total_ranking_score: Score01(
        0.0625,
    ),
}
```

### Confidence

```
ConfidenceAssessment {
    overall: Score01(
        0.75,
    ),
    stoichiometry: Score01(
        1.0,
    ),
    precursor_selection: Score01(
        1.0,
    ),
    process_conditions: Score01(
        0.0,
    ),
    evidence_coverage: Score01(
        1.0,
    ),
}
```

### Evidence

- [Weak/ProcessTemplate] weigh/mix/grind/form are the fixed opening sequence of the v0.1 conventional solid-state template
- [Strong/StoichiometricBalance] balanced reaction releases a byproduct beyond the target, indicating a decomposition (calcination) step is needed before the final firing step
- [Weak/ProcessTemplate] AGENTS.md §11's template outline places a regrind between calcination and final firing

### Warnings

- [Caution] temperature, duration, ramp rate, and atmosphere are unresolved for every heating step: gugen has no thermodynamic or literature evidence provider wired in yet (AGENTS.md §4.1)
- [Severe] no hazard or safety data source is wired in yet: safety_penalty carries no real safety information, and this is not a safety clearance (AGENTS.md §15 "unknown hazardを安全と扱わない")

### Assumptions

- applicability is copied from the target-level assessment, not independently evaluated per route family: v0.1 has exactly one route family (ConventionalSolidState)

### Unresolved

- grinding duration: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)
- forming pressure: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)
- Calcination heating step temperature: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)
- Calcination heating step duration: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)
- Calcination heating step atmosphere: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)
- Calcination heating step ramp rate: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)
- grinding duration: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)
- Sintering heating step temperature: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)
- Sintering heating step duration: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)
- Sintering heating step atmosphere: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)
- Sintering heating step ramp rate: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)

## Rejected candidates

- ["BaCO3"] [MissingTargetElement]: precursor set does not cover target element(s): Ti
- ["TiO2"] [MissingTargetElement]: precursor set does not cover target element(s): Ba

_Generated 2026-08-14T00:00:00Z by gugen 0.1.0._
