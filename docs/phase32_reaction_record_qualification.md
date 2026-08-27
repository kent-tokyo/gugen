# Phase 32: Reaction Record Qualification & Corpus Integrity

**Status: GO for the qualification layer itself. Neither Phase 21B nor
Phase 31 reopening condition is met by this phase's own measurements
-- both stay closed.**

## Why

Phase 31 PR 3 (transformation grammar) and the Phase 21B thermodynamic
calibration both closed as honest NO-GOs
(`docs/phase31_pr3_transformation_grammar_audit.md`,
`docs/phase21b_calibration_result.md`). The owner's diagnosis after
both: the common root cause was never insufficient algorithms -- it
was that the underlying reaction records (composition, byproducts,
dopant notation, reaction stages) were not in a machine-judgable form
to begin with. Phase 21B's own preregistration had already surfaced
one piece of this directly: condition 1's OQMD coverage gate (273
targets, GO) checked only per-species availability, never whether a
route's precursors actually balance into a valid reaction -- of 1285
OQMD-covered, flat-formula-parseable rows, only 347 (27%) did.

This phase does not build a new ranking, generator, or grammar. It
classifies every reaction record in the two corpora this project
already uses (Kononova, thermodynamic-selectivity) by whether it is in
a state Gugen's own search/balance/thermodynamic evaluation can
actually use, and quantifies exactly where and why data is lost.

## Scope

**In scope**: formula parsing/normalization audit; target/precursor
identity validation; as-declared reaction balanceability; missing-
byproduct diagnosis; dopant/host-formula ambiguity diagnosis; a
route-record qualification schema; corpus funnel measurement; a
manual audit; downgrading `oqmd-recovery-check.yml` to a coverage-only
monitor.

**Out of scope** (unconditional): new ranking; `Planner` wiring;
depth-3 multi-step search; new transformation grammars; OQMD
intermediate generation; reaction-network integration; re-running the
Phase 21B calibration; version bump; release.

## Qualification model

Implemented in Python (`benchmarks/`), not `src/` -- corpus auditing,
same home as every other corpus-analysis script this project has
built (`analyze_oqmd_coverage_gate.py`, `analyze_phase21b_calibration.py`).
`benchmarks/parse_flat_formula.py` is itself explicitly scoped as a
benchmark-only tool, not a general Gugen-library formula parser; this
phase keeps that same positioning rather than promoting either into
`src/`.

Seven statuses (`benchmarks/analyze_phase32_qualification.py`):

| Status | Confidence class | Meaning |
|---|---|---|
| `BalancedAsDeclared` | high | `balance()` finds an all-positive reaction using target + every declared precursor, no byproduct needed |
| `BalanceableWithConservativeByproductCompletion` | medium (inferred) | as-declared fails, but exactly one CO2/H2O/O2 candidate makes it balance |
| `FormulaUnsupported` | high | target or a precursor formula string doesn't parse |
| `TargetPrecursorElementMismatch` | high | target has a major element absent from every precursor -- no possible completion |
| `DopantHostAmbiguous` | medium (heuristic) | a minor-fraction target element is absent from the route, or the route carries an element outside the byproduct allow-list |
| `MissingOrZeroCoefficientPrecursor` | high | `balance()` finds a solution, but at least one declared precursor is not needed (coefficient zero) |
| `Unbalanceable` | high | no valid all-positive balance exists, with or without a byproduct candidate |

Every record retains its original formula strings and route list
unmodified; `declared_byproducts` is always `[]` (neither corpus
declares byproducts explicitly) and `inferred_byproduct` is a
**separate** field, never conflated with it. `provenance` names which
script produced the classification; `confidence_class` distinguishes
deterministic facts from heuristic/inferred judgments.

## Formula-shape taxonomy (measured before building anything)

Every distinct formula string in both corpora (Kononova: 1908 rows /
1923 distinct formulas across `kononova_sample.jsonl` +
`kononova_high_arity_sample.jsonl`; thermodynamic-selectivity: 1692
rows / 795 distinct formulas) was classified by shape
(`benchmarks/formula_shape_taxonomy.py`). Rows touched per bucket:

| Bucket | Kononova rows | Thermo rows |
|---|---|---|
| flat | 1903 | 1692 |
| parentheses (single level) | 418 | 101 |
| nested parentheses | 28 | 0 |
| phase prefix/suffix (Greek polymorph, `(s)/(g)/(l)/(aq)`) | 53 | 0 |
| hydrate dot | 0 | 7 |
| malformed -- repeated element (would parse if summed) | 92 | 4 |
| malformed -- symbolic non-stoichiometry suffix (`-δ`, not recoverable) | 131 | 0 |
| malformed -- unrecognized shape (acronym/trade name/unknown symbol) | 96 | 14 |

**Decision: no parser extension was built.** Kononova ships pre-parsed
`elements`/`target_elements` for every row -- its formula *strings*
never gate balanceability at all, so every number in the Kononova
column above is purely descriptive, not something that would recover
additional usable rows if fixed. For the thermodynamic-selectivity
corpus, where string parsing *is* load-bearing, every candidate
extension's real recovery count was too small or too risky to justify
building: phase-affix stripping and nested-parenthesis support both
recover 0 rows there; repeated-element summing recovers 4/1692
(0.24%) and directly conflicts with this project's own established
anti-duplication-artifact discipline (`parse_flat_formula.py`'s own
`test_rejects_repeated_element_rather_than_summing`); hydrate-dot
recovers 7/1692 (0.4%), matching Phase 21B's own already-declined
precedent at similar magnitude; single-level parenthesis support would
recover 101/1692 (6%) but carries the same correctness risk Phase 21B
already weighed and declined for a comparable gain. `parse_flat_formula.py`
is unchanged.

## Corpus funnel

| Metric | Kononova | Thermodynamic-selectivity |
|---|---|---|
| Total rows | 1908 | 1692 |
| Parseable rows | 1908 | 1569 |
| Balanced as declared | 482 | 456 |
| Balanceable after conservative completion | 550 | 0 |
| Balanced or completable, total | 1032 (54.1%) | 456 (27.0%) |
| Formula-unsupported | 0 | 123 |
| Dopant/host-ambiguous | 361 | 1095 |
| Unbalanceable | 513 | 0 |
| Target missing | 0 | 0 |
| Precursor dropped to zero | 2 | 18 |
| OQMD-covered | N/A (never queried against OQMD) | 1327 |
| OQMD-covered and balanceable | N/A | 357 |
| Independent targets with a pure/impure pair | N/A (no verdict field) | 80 |

**Known Phase 21B baseline, reproduced exactly as a regression check**
(condition 1's 273 gate-passing targets, restricted to
flat-parseable + OQMD-covered, `benchmarks/analyze_phase32_qualification.py`'s
`reconstruct_phase21b_1285_row_ids`): 1285 rows, of which **347**
balance as-declared -- both numbers reproduced exactly against this
new, independently-built classifier. This is the GO gate's own
"existing 347 rows must not be lost" requirement, verified directly,
not assumed.

`TargetPrecursorElementMismatch` (a major target element absent from
every precursor, with no plausible dopant explanation) measured **0**
in both corpora -- every real element-set mismatch found turned out to
be either a minor-fraction dopant (2 cases) or a route-side extra
element outside the byproduct allow-list (the much more common
direction), never a target-side element with no explanation at all.

The thermo corpus's "80 independent pairable targets" (measured over
the full 1692-row population, no 273-target pre-filter) is higher than
Phase 21B's original 54 only because the restrictive gate was removed
-- not because any new data exists. It is still below the ≥100 floor
Phase 21B's own reopening condition requires (see below), so this
does not by itself change anything.

## Conservative byproduct completion

Allow-list: **CO2, H2O, O2 only** -- deliberately narrower than
`balance()`'s own six-species `curated_byproducts()` (no NO2, CO,
acetone). A candidate's non-oxygen elements must be present in the
route but absent from the target (a real carbonate/hydrate signal);
oxygen itself is not required to be "extra," since it legitimately
appears on both sides of nearly every solid-state oxide reaction and
an O2 release/uptake needs no foreign element at all (e.g. `2 Mn2O3 ->
4 MnO + O2`). A completion is only accepted when it is the **unique**
candidate that makes the reaction balance all-positive; if more than
one candidate succeeds, the row is kept `Unbalanceable` rather than
guessed at.

550 completions found (Kononova only -- the thermo corpus's few
non-balancing rows never happen to need exactly a CO2/H2O/O2
completion): 445 CO2 (carbonate decomposition), 12 H2O
(hydroxide/hydrate/boric-acid dehydration), 96 O2 (charge-compensating
redox, mostly aliovalent-doped perovskites/spinels), before the audit
exclusion below.

Every one of the 1488 balanced-or-completed rows (938 as-declared +
550 completions) was independently re-verified by an **element-sum
check written from scratch** (not reusing `balance()`'s own code path)
against the rendered equation: **zero mismatches**. Separately
verified: every completion's `inferred_byproduct` actually appears in
its own rendered equation (0 discrepancies), and no row has its target
formula also listed as one of its own declared precursors (0 identity-
reaction risks).

## Dopant/host ambiguity

1456 Kononova rows / 1095 thermo rows classified `DopantHostAmbiguous`,
split by which Section 5 pattern triggered it:

- **Route carries an element outside the byproduct allow-list, absent
  from the target** (the large majority): flux additives that never
  enter the product (e.g. `Bi4Ti3O12 <- B2O3, Bi2O3, TiO2`, B never in
  the target), doped-phosphor precursors under an undoped host target
  name (`Sr2Al2SiO7 <- ..., Eu2O3, CeO2, ...`), and nitrate/oxalate
  precursors whose real decomposition byproduct (NOx) is deliberately
  **not** on this phase's allow-list (`SrTiO3 <- TiO2, Sr(NO3)2`,
  N flagged, not force-completed -- Section 4's own instruction that a
  nitrogen candidate needs separate, later, data-justified
  consideration).
- **Target has a minor-fraction element absent from every route
  precursor** (rare, 2 cases found): multi-dopant phosphor targets
  where one trace dopant among several was dropped from the route
  list.

Neither pattern is force-balanced, matching Section 5's explicit
constraint.

## Manual audit (Section 7)

50-record deterministic sample (10 per populated status, sorted by row
id) plus targeted follow-up sampling on every `BalanceableWithConservativeByproductCompletion`
row (553 before exclusion) and representative `DopantHostAmbiguous`/
`Unbalanceable`/`MissingOrZeroCoefficientPrecursor` rows, hand-checked
for element conservation, consistency with source precursors, no
unnecessary added species, positive coefficients throughout, and
non-identity reactions.

**Finding, applied per Section 8's own rule** ("if even ONE false
completion is found for a family, downgrade that family to
diagnostic-only"): every one of the 3 single-precursor-route O2
completions found was suspicious on inspection --
`Li2MoO3 <- Li2MoO4` (a spontaneous Mo-reduction release with no
reducing agent declared), `Al2O3Ni <- Al2O3NiO` (a formula shaped like
a concatenation artifact from two separate precursors), and
`LiMn2O4 <- LiMn2O9/2` (a formula containing a literal fraction,
itself an unusual notation). None were multi-precursor cases like the
verified-good aliovalent-doping examples (e.g.
`Bi4Ti2.98Nb0.02O12 <- TiO2, Nb2O5, Bi2O3` + O2, a real Nb5+-for-Ti4+
charge-compensation case). **Single-precursor-route completions are
now excluded from auto-completion for every byproduct, not only O2**
(550 of the original 553 survive; the 3 excluded fall back to
`MissingOrZeroCoefficientPrecursor`/`Unbalanceable` per their
as-declared outcome). Every CO2 (445) and H2O (12) example sampled was
a textbook, well-documented decomposition (`BaCO3 + ZrO2 -> BaTiO3-style
carbonate routes`, `Al(OH)3`/`H3BO3`/`Ba(OH)2` dehydrations) -- zero
false completions found in either family.

A separate, real corpus-quality finding surfaced by
`MissingOrZeroCoefficientPrecursor`, not by construction: duplicate
precursor entries (`Mg2SiO4 <- MgO, SiO2, SiO2`) and excess-reagent
entries not incorporated into the product (`BaZrS3 <- BaS, S, ZrS2`,
elemental S correctly drops to zero) -- both genuine data-quality
signals this classifier surfaces rather than silently forcing a
3-precursor balance.

## Decision gates

**Qualification layer itself: GO.** Deterministic (no randomness
anywhere in the pipeline); never modifies original formula/route
data (`declared_byproducts` stays empty, `inferred_byproduct` is a
separate field); every inference carries a `provenance` and
`reason_codes` field; the manual audit found and excluded one false-
completion-prone family (single-precursor-route O2) before shipping,
per Section 8's own rule, and found zero false completions in every
other family sampled; the funnel is reproducible from committed source
data via two scripts + one Rust example; the existing 347 balanced
rows are reproduced exactly, not lost.

**Phase 21B reopening: conditions not met.** The reopening bar is
≥100 new independent qualified target pairs, dopant/host-ambiguous
rows excluded, from a genuinely new qualified corpus -- not a re-count
of the same data. This phase's own measurement (80 pairable targets,
over the *same* 1692-row population Phase 21B already drew from, no
new independent literature data) is both below the floor and not the
right kind of number even if it were higher. **Phase 21B stays
closed.**

**Phase 31 reopening: conditions not met.** The reopening bar is ≥30
real ground-truth multi-step/intermediate routes. This phase produced
none (out of scope -- it classifies single-reaction records, not
multi-step routes). **Phase 31 stays closed**; no depth-3, no OQMD
intermediate generation, no reaction-network integration.

## OQMD workflow (Section 9)

`.github/workflows/oqmd-recovery-check.yml` renamed to "OQMD data
availability monitor." Reworded throughout (workflow comments, issue
title/body, `.github/scripts/check_oqmd_recovery.py`'s docstring) to
state explicitly: this is a coverage-only signal, not a balanceability
check, not a selectivity-accuracy check, and not a phase-resumption
trigger -- citing this phase's own 1327-covered/357-balanced numbers
and Phase 21B's reopening bar directly in the issue body a recovery
event would open. Unchanged: it still never blocks a merge, never
starts calibration automatically, and keeps its daily/manual-dispatch
schedule (not raised in frequency).

## What this does not claim

No claim that the 550 conservative completions or 482+456 as-declared
balances represent literally the reaction each paper reported --
`balance()` finds *a* valid stoichiometric combination, and multiple
valid combinations can exist for the same species list. No claim that
`DopantHostAmbiguous` rows are unusable -- they are unresolved by this
phase's conservative rules, not proven wrong. No claim about
`Unbalanceable` rows beyond "no all-positive solution exists under
this allow-list" -- most likely need a byproduct outside CO2/H2O/O2
(nitrate/oxalate byproducts, chloride byproducts -- both known,
separately-declined gaps) or are corpus data-quality issues, not
evidence that `balance()` itself is wrong. No new capability wired
into `Planner`, `score_plan`, `RankingWeights`, or any generator/
grammar -- this phase is a measurement and classification layer only.

## Status

Implemented and measured. New files: `benchmarks/formula_shape_taxonomy.py`
(+ `benchmarks/data/phase32_formula_shape_taxonomy.json`, committed),
`benchmarks/build_phase32_qualification_input.py`,
`examples/exploration_phase32_reaction_qualification.rs`,
`benchmarks/analyze_phase32_qualification.py`
(+ `benchmarks/data/phase32_qualification_result.json`, committed,
~5KB), `benchmarks/test_phase32_qualification.py` (22 tests).
Intermediate files (`phase32_qualification_input.json`,
`exploration_phase32_reaction_qualification_result.json`,
`phase32_qualified_records.json`) gitignored as regenerable-in-seconds,
matching Phase 21B's own precedent. `.github/workflows/oqmd-recovery-check.yml`
and `.github/scripts/check_oqmd_recovery.py` reworded, not restructured.
No `src/` change, no version bump. Root quality gate (`cargo fmt --all
-- --check`, `cargo clippy --workspace --all-targets --all-features --
-D warnings`, `cargo test --workspace --all-features` /
`--no-default-features`, `RUSTDOCFLAGS="-D warnings" cargo doc
--all-features --no-deps`) green; every Python test suite in
`benchmarks/` green. **Stopping here per Section 10's own stop
condition -- no automatic progression to new model development or to
reopening either closed phase.**
