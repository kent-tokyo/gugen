# Scientific Scope (v0.1)

This restates AGENTS.md §2–§4 as concrete design constraints. AGENTS.md is
the source of truth; this document exists so scope decisions during
implementation can be checked against a short list instead of re-reading the
full spec.

## The question gugen answers

> Given a target inorganic material (composition, optionally structure),
> which precursors, reactions, process steps, and condition ranges are
> plausible candidates — and what is the evidence and uncertainty behind
> each?

## In scope for v0.1

- Inorganic crystalline bulk materials, explicit target composition,
  optional target structure.
- Two route families (v0.2.0, Phase 12 added the second):
  - Conventional solid-state: weigh → mix → grind → (optional form) →
    calcine → regrind → sinter/anneal → cool → (optional) intermediate
    characterization.
  - Mechanochemical (structural route only -- see below): weigh → ball
    milling (combined mix+grind) → (optional form) → (conditional
    post-milling anneal) → (conditional cool) → (optional) intermediate
    characterization. Both route families are offered unconditionally for
    every accepted precursor set (AGENTS.md §13); gugen has no
    route-suitability classifier to prefer one over the other for a given
    target.
- Abstract atmosphere categories (air / inert / oxidizing / reducing /
  vacuum / controlled-described), roughly ambient pressure.
- Multiple alternative candidate plans, ranked by an explainable, rule-based
  multi-criteria score.
- Known/curated precursor candidates only (no literature scraping in core).

## Explicitly out of scope for v0.1 (AGENTS.md §3)

Organic synthesis, retrosynthesis, MOF/COF planning, polymer synthesis,
thin-film/CVD/PVD/ALD, electrodeposition, high-pressure synthesis,
hydrothermal/solvothermal, molten-salt synthesis, direct control of
automated lab equipment, DFT, molecular dynamics, kinetic-rate-constant
prediction, yield prediction, success-probability prediction, automated
literature scraping, ungrounded LLM-generated recipes, patent search,
market analysis. *Detailed* mechanochemical conditions (milling duration,
ball-to-powder ratio, RPM, etc.) stay out of scope even though the
mechanochemical route's structure is now in scope (Phase 12) -- same
"no unsourced numeric conditions" discipline the conventional route's
firing conditions are already held to.

These are candidate future *route-family plugins*, not v0.1 features. A
design that quietly starts depending on solving one of these (e.g. a ranking
formula that implicitly needs a yield estimate) is a scope violation even if
no code named "yield" appears.

## Four non-negotiable guardrails (AGENTS.md §4)

Every module that touches ranking, conditions, or evidence must be checked
against these before merge:

1. **No unsourced numeric conditions.** A temperature or duration value may
   only appear in output if it traces to a `PlanningEvidence` entry (route
   template prior, decomposition constraint, user precedent, curated
   literature record). Otherwise the field is `unresolved`, never a
   plausible-looking default. Phase 10's `InMemoryLiteratureConditionProvider`
   is the first real satisfier of the "curated literature record" case —
   every value it supplies traces to a real, hand-verified citation
   (`src/literature_conditions.rs`), and it still leaves a field `None`
   rather than guess when it has no matching record for a given target.
2. **`RankingScore` ≠ success probability.** v0.1 score is ordinal, for
   sorting candidates against each other. It is never rendered, documented,
   or reasoned about as "N% likely to succeed."
3. **Thermodynamic favorability ≠ experimental likelihood.** A reaction can
   be energetically downhill and still fail from kinetic barriers,
   competing phases, precursor passivation, gas transport, particle size,
   diffusion distance, atmosphere mismatch, volatilization, crucible
   reaction, or metastability. `PlanScoreBreakdown.thermodynamic_support` is
   one input among several, never a stand-in for overall confidence.
4. **Novelty ≠ feasibility.** An OOD composition/structure is not assumed
   synthesizable just because it's novel, nor assumed unsynthesizable.
   Novelty may be tracked as auxiliary metadata but does not feed the v0.1
   planning score.

## What gugen does not guarantee (AGENTS.md §2)

Experimental success, target-phase formation, single-phase product,
reaction completion at a stated temperature, high yield, safe
executability, patentability, industrial scalability. Output is a set of
candidate plans, not a validated SOP.

## Applicability boundary (used by `ApplicabilityAssessment`, AGENTS.md §16)

| Input | Applicability |
|---|---|
| Bulk inorganic, solid-state-plausible | `InDomain` |
| Formula only, no structure | `PartiallyInDomain` |
| Severe structural disorder (via mikiwame, if enabled) | `PartiallyInDomain` |
| MOF/COF, thin film, or any §3 out-of-scope target | `OutOfDomain` |

`OutOfDomain` targets should be abstained on (rejected with a clear reason),
not force-fit into a solid-state plan. This is the mechanism behind the
v0.1 completion criterion "out-of-domain inputを棄却できる" (AGENTS.md §29)
and is measured later as "out-of-domain abstention rate" (AGENTS.md §22).
