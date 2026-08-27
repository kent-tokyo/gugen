# Phase 21B calibration — pre-registration

**Committed before any reaction energy has been computed.** Per this
project's own standing discipline (Phase 21A's 30-target sample gate,
Phase 21B condition 1's coverage gate, Phase 31 PR 3's dev/eval split),
every design choice below is fixed here, in this commit, before seeing
a single result — so a later report can't be shaped by having already
seen the numbers. Triggered by the owner's explicit "phase21b"
instruction; per `docs/thermodynamic_selectivity_calibration.md` §6.5
Step 5 / §7, this is the "separate, later, explicitly-triggered phase"
that document has been waiting for since Phase 21B's four GO conditions
were all measured (2026-08-23).

## Hypothesis

Stated directionally, per Bartel et al. 2018's own physical picture:
**within a target, the route (precursor set) whose reaction to that
target has the more negative reaction energy (`balanced_reaction_delta_ev_per_atom`,
per atom) is more often the one the literature reports as producing a
phase-pure product.** Rationale: a more thermodynamically favorable
route leaves less driving force available to form a competing
byproduct phase. This is a hypothesis about a **noisy, unverified-at-scale
label** (§2 of the calibration doc found real accessibility and
route-representation caveats in the pure/impure label at n=15) — a
positive result is evidence about the label's relationship to reaction
energetics, not a validated claim that thermodynamics predicts real
experimental purity.

## Why a new parser was needed, and its exact scope

`balanced_reaction_delta_ev_per_atom` needs a `BalancedReaction`
(`Composition`s with real element-conserving coefficients), which
needs numeric element amounts — not the bare formula strings
(`"Ti3SiC2"`) this dataset stores. No formula parser exists anywhere
in gugen or this project's benchmark scripts (every other corpus used
so far ships pre-parsed per-element amounts). Rather than build a
general parser (nested parentheses with multipliers, hydrate dots), a
survey of the 273 Phase 21B condition-1 gate-passing targets' own
formulas (`benchmarks/data/oqmd_coverage_gate_result.json`'s
`passing_targets`, cross-referenced against
`thermodynamic_selectivity_clean_population.json`) found:

| Bucket | Distinct formulas (target+route, among passing-target rows) |
|---|---|
| Flat (element+optional decimal amount only) | 596 / 625 (95.4%) |
| — of which has a decimal subscript | 4 |
| Nested (contains parentheses, e.g. `(PbS)1.18(TiS2)2`) | 24 |
| Other (middle-dot hydrate separator, e.g. `DyCl3·6H2O`) | 5 |

`benchmarks/parse_flat_formula.py` implements **only the flat case**,
by design: recomputing the gate-passing-target count restricted to
targets whose own formula *and* every route-row formula parses flat
gives **269/273 targets (98.5%)** and **2351/2357 independent pairwise
comparisons (99.7%)** retained — the 24 nested + 5 hydrate-separator
exclusions cost 1.5% of targets. Building recursive nested-parenthesis
parsing for that gain was judged not worth the added correctness risk
in a brand-new capability this project has never needed before.
**Anything the parser can't parse exactly returns `None` and is
counted as an exclusion — never a best-effort guess** (same discipline
as `Frac::from_f64`, `src/frac.rs`).

**Parser correctness, checked before use, not assumed**: for every row
where both the target and every route formula parse flat (1286/1334
rows among passing-target rows), the route's combined element set must
be a superset of the target's element set (a gas-free/byproduct-free
or byproduct-explainable reaction can only add or retain elements from
its own reactants). Checked directly: **0 mismatches found** across
all 1286 parseable rows — no evidence of a parser bug or corpus
element-mismatch problem before proceeding.

## A discovered fact this pre-registration exists to handle honestly

Condition 1's own coverage gate (`analyze_oqmd_coverage_gate.py`)
measures only whether every species *individually* has an OQMD entry —
it does **not** check whether a route's precursors actually balance
into a valid reaction producing the target. Checked directly: among
the 1286 parseable rows, only **357 (27.8%)** have a route element set
*exactly equal* to the target's (a necessary condition for a
byproduct-free direct combination); the remaining **929 (72.2%)** have
at least one extra element in the route not present in the target —
consistent with either (a) a real byproduct the dataset's own
"gas-free" extraction failed to list (e.g. a carbonate/hydroxide/nitrate
precursor whose real decomposition byproduct is exactly one of
`curated_byproducts()`'s six species), or (b) a doped-compound-under-
host-formula labeling ambiguity (§2's own condition-2 finding, e.g. a
`MnCO3` dopant precursor for a target field naming only the undoped
host lattice) that is not a valid single reaction as stated at all.

**This pre-registration does not try to distinguish (a) from (b) by
inspection.** Instead: every row's reaction is attempted through
gugen's own existing, already-tested `balance()` (`src/balance.rs`,
unchanged), trying `[target]` plus every `curated_byproducts()` subset
as the candidate product side (identical to
`search_precursor_sets`'s own `evaluate_complete_state` pattern,
including its PR 78 fix requiring `target` to actually survive in the
returned products). A row that balances is chemically explicable via
gugen's own curated byproduct set (case a, or genuinely byproduct-free);
a row that doesn't is excluded, not guessed at (case b, or any other
unexplainable mismatch). **The real, final calibration sample size is
whatever this produces — not assumed here, measured and reported.**

## Unit of analysis and representative-pair selection

Per-target, single deterministic pair — not the full `n_pure ×
n_impure` cross product. Rationale: condition 2 already established
DOI as this dataset's independence unit; multiple rows sharing a
target (or a DOI) are not independent draws, and the full pairwise
count (2351, if computed naively) would overcount and is reported only
as a secondary, explicitly-non-independent descriptive number.

For each target with at least one row parseable, OQMD-covered by both
sides, **and successfully balanced** for each of the `pure` and
`impure` verdict classes: select exactly one representative row per
class by the **alphabetically smallest DOI** among that class's
balanceable rows (a row's `dois` list is reduced to its own minimum
string first; ties broken by the route tuple's own string
representation, for full determinism). **This selection never looks at
the computed reaction energy** — it is fixed before any energy exists,
so it cannot be (even unconsciously) chosen to favor the hypothesis.

A target qualifies for the calibration only if both a `pure` and an
`impure` representative exist under this rule.

## Primary metric, temperature, and statistical test

- **Temperature: 300 K** (the low end of gugen's own validated `[300,
  1800]` K range — the closest available point to a pure 0 K DFT
  comparison, chosen as the smallest-assumption default since no
  per-route synthesis temperature exists anywhere in this corpus to
  use instead). This is a **fixed convention, not a claim about any
  route's real synthesis temperature.**
- **Metric**: accuracy over qualifying targets = (targets where the
  `pure` representative's `balanced_reaction_delta_ev_per_atom` is
  strictly more negative than the `impure` representative's) / (all
  qualifying targets). An exact tie (extremely unlikely with real
  floating energies, but handled) counts toward neither the numerator
  nor the denominator, and the tie count is reported separately.
- **Test**: one-sided exact binomial test (`H0: accuracy = 0.5`, `H1:
  accuracy > 0.5`), computed by hand via `math.comb` (stdlib only, no
  new dependency) — the boring, correct choice for a directional
  paired-accuracy hypothesis.

## Pre-registered gate

**A necessary precondition, checked first**: if fewer than **30**
qualifying targets exist (this project's own recurring sample floor —
Phase 21A, Phase 21B condition 1), the calibration reports **NO-GO for
insufficient sample** and computes no accuracy or significance figure
at all — the same discipline as every prior phase's own floor.

If ≥30 qualifying targets exist, computed **before being told the
result**:

| Verdict | Criterion |
|---|---|
| **NO-GO** | one-sided binomial `p ≥ 0.05` (not statistically significant), regardless of accuracy |
| **GO** | `p < 0.05` and accuracy `< 0.70` |
| **STRONG GO** | `p < 0.01` and accuracy `≥ 0.70` |

A NO-GO does not by itself mean discard this direction of inquiry —
per this project's own standing instruction (restated after Phase 31
PR 3's own architecture-vs-cost framing), the result is reported
honestly either way, with the real sample size and its limitations, so
the owner can judge whether a larger population or a different
temperature convention is worth trying later.

## Secondary, descriptive-only metrics (never gating)

- Full un-deduplicated pairwise accuracy across every covered,
  parseable, balanceable `n_pure × n_impure` comparison per target —
  reported explicitly labeled as over-counting non-independent
  comparisons, not as a second gate.
- Sensitivity check at **1800 K** (gugen's upper validated bound): does
  the primary verdict change at the other end of the valid range.
- Residual cross-target DOI overlap: how many DOIs are selected as a
  representative row for more than one different target (a target-level
  design already avoids *within*-target DOI reuse; this checks the
  weaker *across*-target case and discloses it rather than assuming
  zero).
- The full funnel, every stage counted: 273 gate-passing targets → N
  after the flat-parser restriction (269, already measured) → N after
  requiring both verdict classes to have ≥1 successfully-balanced row
  → N qualifying targets in the final calibration.
- Informal chemical-family breakdown (reusing
  `analyze_oqmd_coverage_gate.py`'s own first-matching-anion heuristic,
  same non-taxonomy caveat).

## Non-goals (restated, unchanged from the calibration doc's own §7)

No `score_plan` connection, no `RankingWeights` change, no default
ranking change, no success-probability claim, no automatic temperature
selection, no gas-phase thermodynamics claim beyond the curated
byproduct set already used elsewhere in this crate, no literature-condition
promotion, no version bump, no public API change of any kind — **even
on a STRONG GO.** Condition 2's finding (the pure/impure label is noisy
and unverified at scale) caps what any result here can claim: evidence
about a label relationship, not a validated purity predictor.

## Status

Design and parser committed; the flat-parser element-superset
cross-check has been run (0 mismatches, reported above). No reaction
energy has been computed as of this commit. The `balance()`-success
funnel, the calibration computation itself, and its result are
reported in a follow-up commit.
