# Synthesis Planning Report (schema v2)

**Target:** Ba:1, O:3, Ti:1

**Applicability:** PartiallyInDomain -- formula-only target, no structure provided (AGENTS.md §16's own example for this level)

## Plan plan-1677d44bfe4dbdc2 (score 0.062)

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

- applicability is copied from the target-level assessment, not independently evaluated per route family: no route-suitability precedent exists for this target under ConventionalSolidState specifically (every applicable route family is offered unconditionally, AGENTS.md §13)

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

## Plan plan-ee311be9350b7d8b (score 0.062)

- Target: Ba:1, O:3, Ti:1
- Route family: Mechanochemical
- Reaction: 1x(Ba:1, C:1, O:3) + 1x(O:2, Ti:1) -> 1x(Ba:1, O:3, Ti:1) + 1x(C:1, O:2)
- Manual review required: true
- Applicability: PartiallyInDomain -- formula-only target, no structure provided (AGENTS.md §16's own example for this level)

### Steps

- [Required] Weigh: BaCO3 x1, TiO2 x1
- [Required] Grind (BallMilling), duration=unresolved
- [Optional] Form (UniaxialPressing), pressure=unresolved
- [Required] Heat (Annealing): temperature=unresolved, duration=unresolved, atmosphere=unresolved, ramp=unresolved
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

- [Weak/ProcessTemplate] weigh, then a single high-energy ball-milling step (which performs mixing and grinding together, unlike the separate Mix/Grind steps of the conventional solid-state template) is the fixed opening sequence of the mechanochemical route template
- [Moderate/StoichiometricBalance] balanced reaction releases a byproduct beyond the target; ball milling alone is not reliably sufficient to complete such a reaction at room temperature, so a post-milling anneal is included -- the cited review reports specific byproduct-releasing compounds (e.g. gamma-Al2O3, ZrO2) that formed only after heating the as-milled powder

### Warnings

- [Caution] grinding duration, forming pressure, and (if present) heating temperature/duration/atmosphere/ramp are unresolved: gugen has no thermodynamic or literature evidence provider wired in yet (AGENTS.md §4.1)
- [Severe] no hazard or safety data source is wired in yet: safety_penalty carries no real safety information, and this is not a safety clearance (AGENTS.md §15 "unknown hazardを安全と扱わない")

### Assumptions

- applicability is copied from the target-level assessment, not independently evaluated per route family: no route-suitability precedent exists for this target under Mechanochemical specifically (every applicable route family is offered unconditionally, AGENTS.md §13)

### Unresolved

- grinding duration: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)
- forming pressure: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)
- Annealing heating step temperature: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)
- Annealing heating step duration: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)
- Annealing heating step atmosphere: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)
- Annealing heating step ramp rate: no thermodynamic or literature evidence provider is wired in yet (AGENTS.md §4.1)

## Rejected candidates

- ["BaCO3"] [MissingTargetElement]: precursor set does not cover target element(s): Ti
- ["TiO2"] [MissingTargetElement]: precursor set does not cover target element(s): Ba

_Generated 2026-08-14T00:00:00Z by gugen 0.4.2._
