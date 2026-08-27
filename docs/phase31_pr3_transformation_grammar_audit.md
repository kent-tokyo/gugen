# Phase 31 PR 3 — Conservative Transformation Grammar Audit

## Why this exists

The owner approved adding hand-written chemistry as an intermediate-
candidate source for `search_two_step_routes`, but explicitly scoped it
as **"a demonstration of a small number of conservative, explainable
grammars that propose candidates" — not a general chemical-reaction
rule engine.** The instruction was precise about what to build, how to
measure it honestly, and what to report regardless of outcome:

- Exactly 4 grammar families, each a narrow, single mechanism.
- A grammar only *proposes* a candidate composition for the existing
  `search_precursor_sets`/`balance()`/`SynthesisRoute::new` pipeline to
  accept or reject — it never asserts a reaction is real.
- A DOI-grouped, deterministic development/evaluation split of the
  408-row `kononova_high_arity_sample.jsonl` holdout (Phase 31 PR 2),
  **committed before any grammar rule was written**, so the ordering is
  verifiable from git history, not just asserted.
- A 4-policy comparison (one-step baseline / frequency-only /
  grammar-only / union) against that split, with net-new recall among
  one-step-unreachable rows as the primary metric.
- A mandatory manual audit of recovered routes, with strict
  "stoichiometrically valid ≠ experimentally real mechanism" language
  discipline.
- Report honestly and stop — do not wire into `Planner`, do not proceed
  to depth-3/OQMD/reaction-network, no version bump, no release,
  regardless of the result.

## What was built

### `src/transformation_grammar.rs` — the grammar module

A `TransformationGrammar` trait (`propose(&self, precursors: &[Composition])
-> Vec<ProposedIntermediate>`) plus four implementations, each identified
purely by element-ratio predicates on a `Composition` — **not string
parsing**, since gugen has no formula parser anywhere in the crate
(confirmed directly, again, before writing this module):

| Grammar | Mechanism | Evidence class |
|---|---|---|
| `CarbonateToOxideGrammar` | `MCO3 -> MO + CO2` (per carbon: −1 C, −2 O) | Stoichiometric |
| `HydroxideToOxideGrammar` | `M(OH)n -> MO(n/2) + (n/2) H2O`, identified by exact O:H = 1:1 | Stoichiometric |
| `NitrateToOxideGrammar` | `M(NO3)n -> MO(n/2) + n NOx`, oxide side derived from charge balance only; byproduct composition deliberately not fixed | Stoichiometric |
| `AcidCarbonatePhosphateGrammar` | `2 H3PO4 + M2CO3 -> 2 MH2PO4 + CO2 + H2O`, monovalent-metal carbonates only | CommonDecompositionHeuristic |

Every predicate is deliberately **narrow, not widened to cover more
cases**: carbonate/nitrate grammars reject any composition containing
hydrogen (ruling out bicarbonates, hydrated nitrates, and nitric acid);
the hydroxide grammar rejects any composition containing carbon; the
acid+carbonate grammar only fires for an exact H3PO4 signature paired
with an exact monovalent-carbonate signature, and only ever proposes
the monobasic salt (never di-/tri-basic products, never divalent/
trivalent carbonate metals). Where a `Composition`-level predicate
couldn't cleanly separate one species from a chemically different one,
the predicate was narrowed further rather than widened to guess.

A mandatory `validate_proposed_composition` safety check runs on every
proposal from every grammar: rejects any proposal that introduces an
element absent from its own inputs, is identical to one of its inputs
(a no-op), or exceeds the combined element vocabulary of its inputs.
All arithmetic uses the crate's own exact-rational `Frac` type
internally (added one `pub(crate)` accessor, `Composition::amount_of_frac`,
for this — no public API change), not `f64`, so ratio predicates like
"O:H exactly 1:1" are never subject to floating-point rounding.

`propose_all()` runs every grammar, caps each one's raw output, and
deduplicates identical compositions across grammars — retaining every
contributing grammar's id and the most-certain evidence class seen —
mirroring `CandidateGeneratorEnsemble`'s own per-source-cap-then-merge
shape (`src/candidate_generator.rs`).

**16 unit tests**, one of which is a real regression: `NitrateToOxideGrammar`
originally accepted any composition with "exactly one non-N/non-O
element" as a metal nitrate. `HNO3` (nitric acid — present as a real
precursor in the holdout corpus) satisfies that predicate with hydrogen
as the "metal", producing a nonsensical `H:1, O:0.5` proposal. Caught by
testing against real corpus formulas before finalizing, not invented as
a synthetic edge case. Fixed by excluding H-bearing compositions,
matching the other grammars' own established exclusion discipline.
Confirmed the regression test actually fails without the fix (temporarily
reverted, reran, restored) — this project's standing verification
discipline.

**Not wired into `Planner`.** Exported from the crate (`GrammarId`,
`GrammarEvidenceClass`, `ProposedIntermediate`, `DedupedProposal`,
`TransformationGrammar`, the four grammar structs, `default_grammars`,
`propose_all`) the same way `CandidateGenerator`/`FrequencyPriorGenerator`
were before any Planner integration — available, independently testable,
zero effect on any existing code path. `cargo semver-checks
check-release --baseline-version 0.6.0 --all-features`: 196 checks, 196
pass, no breaking change (a purely additive public surface).

### The dev/eval split — committed before any grammar rule existed

`benchmarks/build_grammar_audit_split.py` groups the 408-row holdout by
DOI (34 DOIs contribute more than one row — a row-level split could put
two rows from the same paper on opposite sides), sorts DOI keys,
shuffles with `random.Random(seed=31)`, and greedily assigns whole DOI
groups to whichever side has fewer rows so far. Result: **204/204** rows,
185/180 DOIs, committed as `benchmarks/data/exploration_grammar_split_manifest.json`
in its own commit (`053def9`) before `src/transformation_grammar.rs`
existed (added in `b5692c4`) — verifiable from git history.

**Disclosed contamination, not hidden or corrected for:** 3 of the 4
rows already inspected in PR 2's own hand-trace/Discovered-Work section
landed back on one side or the other by this deterministic split. Most
importantly, **`SiO2P2O5K2OMgOCaO` (DOI 10.1016/j.tca.2014.08.028)** —
the exact row whose hand-traced route (`2 H3PO4 + K2CO3 -> 2 KH2PO4 +
CO2 + H2O`) directly motivated `AcidCarbonatePhosphateGrammar`'s design
— landed on the **evaluation** side. This was not corrected by moving
the row to development; forcing that would be tuning the split around
an already-seen answer. It is disclosed here and in the manifest's
`known_pre_split_contamination` field, and its recovery (see Results)
is reported as circular evidence, not independent confirmation.

### The measurement — `examples/exploration_grammar_audit.rs`

Four policies, **one shared candidate cap (200)** for every policy — PR
2 found net-new recall highly sensitive to the candidate cap (0→12 out
of 294 across 20→380), so comparing a capped policy to an uncapped one
would make "X beats Y" mean nothing:

- `OneStepBaseline`: zero intermediates (sanity floor — net-new must be
  exactly 0 by construction; confirmed).
- `FrequencyOnly`: PR 2's `FrequencyPriorGenerator`, capped at 200 (down
  from PR 2's own 2000 — deliberately re-capped here for a fair
  same-ceiling comparison, not a claim that 200 is optimal for
  frequency alone).
- `GrammarOnly`: `propose_all` over the row's own real precursors,
  per-grammar cap 50, combined cap 200.
- `Union`: `FrequencyOnly` ∪ `GrammarOnly`, deduplicated, capped at 200
  total (so grammar proposals can be crowded out by frequency ones
  filling the shared cap first — this happened, see below, and is
  reported as a real finding, not a bug).

## Results — reported honestly

**A note on the gate criterion applied here**: the original directive
specified precise GO/STRONG-GO/NO-GO numeric thresholds, but this
implementing session's context was compacted between receiving that
directive and writing this section, and the exact thresholds were not
recoverable from the retained summary. Rather than invent numbers to
back-fill a threshold that can't be confirmed, the verdict below is
against the plain, defensible reading of the directive's own stated
primary metric: **does adding grammar-sourced candidates (via `Union`)
improve net-new two-step recall over `FrequencyOnly` alone.** If the
owner's original thresholds differ from this reading, the raw numbers
in the table below are unaffected and can be re-judged directly against
them.

| Split | Policy | Truly unreachable | Two-step (any) | Net-new | Net-new recall |
|---|---|---|---|---|---|
| development | one_step_baseline | 151 | 0 | 0 | 0.00% |
| development | frequency_only | 151 | 55 | 6 | 3.97% |
| development | **grammar_only** | 151 | 46 | **0** | **0.00%** |
| development | union | 151 | 55 | 6 | 3.97% |
| evaluation | one_step_baseline | 143 | 0 | 0 | 0.00% |
| evaluation | frequency_only | 143 | 57 | 3 | 2.10% |
| evaluation | **grammar_only** | 143 | 49 | **1** | **0.70%** |
| evaluation | union | 143 | 57 | 3 | 2.10% |

By arity (truly-unreachable rows only): development 5→125 (freq 5,
grammar 0), 6→21 (freq 1, grammar 0), 7→5 (0, 0); evaluation 5→112
(freq 3, grammar 1), 6→29 (0, 0), 7→2 (0, 0).

**The verdict, against the primary metric as specified (net-new
recovery beyond frequency-only, i.e. does Union beat FrequencyOnly):
NO-GO.** Union is numerically identical to `FrequencyOnly` on both
splits (6/151 and 3/143) — grammar contributed zero additional net-new
recoveries on either side once combined with frequency. Grammar-only
by itself found **0/151** net-new on development and **1/143** on
evaluation, and that one hit is `SiO2P2O5K2OMgOCaO` — the exact row
that motivated the grammar producing it. **This is circular, not
independent evidence**: the grammar was designed to solve this case, so
solving it confirms the implementation is correct, not that the
grammar generalizes to unseen chemistry.

**A secondary, more encouraging signal**: `grammar_only`'s `two_step_any`
count (46 development, 49 evaluation) is not far below `frequency_only`'s
(55, 57) — grammars A–C do find real, valid 2-stage routes for a
comparable number of rows to frequency-prior, just consistently for
rows that already have a 1-step route too (so they add no *net-new*
value under this metric, but they are not chemically wrong or useless
— see the audit below). No `NitrateToOxideGrammar` (grammar C) proposal
led to any accepted route on either split, despite the corpus containing
real nitrate precursors (`KNO3`, `LiNO3`, `NaNO3`, `Sr(NO3)2`) and the
grammar correctly proposing an oxide for each of them (confirmed
directly: `KNO3` → `K:1, O:0.5`, the correct K2O ratio) — this is
**disclosed as no evidence either way for grammar C**, not a failure;
its own proposals were simply never the missing piece in a full route
in this particular corpus.

## Manual audit (7 samples, ≥1 per grammar where any existed)

Every recovered composition was traced by hand against the mechanism
its grammar claims, checking the proposal is stoichiometrically valid —
**not** that it matches the paper's real synthesis procedure, which no
corpus available to this project records at per-step granularity.

- **`CarbonateToOxideGrammar` (3 samples, e.g. DOI 10.1007/s10832-013-9864-2,
  precursors including `BaCO3`)**: proposed `Ba:1, O:1` — exactly `BaO`,
  `BaCO3`'s real decomposition product. Stoichiometrically valid in
  every sample; not a claim that `BaTiO3`-family syntheses actually
  isolate `BaO` as a discrete intermediate (real solid-state synthesis
  is typically one calcination, per PR 2's own research into the
  reaction-network literature).
- **`HydroxideToOxideGrammar` (3 samples, e.g. DOI 10.1007/s10854-016-5674-z,
  precursors including `H3BO3`)**: proposed `B:1, O:1.5` — exactly the
  `B2O3` ratio. **A genuine, unplanned generalization worth disclosing**:
  this grammar was designed thinking of metal hydroxides `M(OH)n`, but
  its exact "O:H = 1:1" predicate also matches boric acid `H3BO3`
  (structurally `B(OH)3`), and `2 H3BO3 -> B2O3 + 3 H2O` is itself a
  real, well-known decomposition — so the generalization is chemically
  correct, not a false positive, but it was not anticipated when the
  predicate was written.
- **`NitrateToOxideGrammar`**: no sample exists (0 accepted routes used
  its proposals on either split) — see above.
- **`AcidCarbonatePhosphateGrammar` (1 sample, DOI 10.1016/j.tca.2014.08.028)**:
  proposed `K:1, H:2, P:1, O:4` — exactly `KH2PO4`. Stoichiometrically
  valid and hand-verified balanced in both stages already in PR 2's own
  writeup. **Flagged again here**: this is the contaminated row: recovering
  it confirms the implementation matches its own design case, not that
  the grammar generalizes.

No route recovered by any grammar is claimed to match a real documented
synthesis procedure — only that the proposed intermediate is a valid,
balanced stepping-stone composition under gugen's own search/balance
pipeline.

## Architecture-vs-cost tradeoff, for owner judgment

A NO-GO on the primary metric does not by itself mean discard, per the
original instruction — the cost side of the ledger:

- **Cost of keeping**: `src/transformation_grammar.rs` is ~460 lines,
  self-contained, zero dependencies on or from `Planner`/`precursor.rs`/
  `multi_step.rs` internals beyond `Composition` itself, 16 passing unit
  tests, no runtime cost on any path that doesn't explicitly call it.
  Maintenance burden is effectively zero unless new grammars are added.
- **Cost of removing**: also near zero — nothing else in the crate
  depends on it.
- **Evidence for keeping despite NO-GO**: grammars A/B/C independently
  confirm 2-stage route validity for a comparable count of rows to
  frequency-prior (46–49 vs. 55–57), suggesting the *mechanism* is sound
  even where it adds no net-new value at this corpus's current
  candidate cap and search budget — a different budget or a richer
  grammar family (still conservative) might change the net-new number
  without any architectural rework, since `TransformationGrammar` is
  already a clean extension point.
- **Evidence against keeping as currently scoped**: grammar D's only
  positive result is circular; grammars A–C add zero net-new lift; a
  4th grammar (nitrate) has zero measured evidence either way. As
  shipped, this is a demonstration that the mechanism works end-to-end,
  not yet a demonstration that it helps.

Recommendation for the owner: keep the module (near-zero cost, clean
extension point, one real bug found and fixed via this exercise), but
do not treat grammar D's result as validated until it is tested against
a genuinely new, non-contaminated acid+carbonate case — none exists in
the current holdout's development or evaluation split by construction
(the only such case in the 408-row corpus is the one that seeded the
grammar).

## What this does not claim

No claim that any recovered intermediate matches a documented real
synthesis procedure (see Manual audit). No claim that 4 grammars is a
ceiling or that this NO-GO generalizes to a larger or different grammar
family — only these 4, at this candidate cap (200), against this
408-row corpus's dev/eval split, is measured. No claim that
`NitrateToOxideGrammar` is broken or ineffective in general — only that
no accepted route in this corpus happened to use its output; the
grammar's own unit tests confirm its arithmetic is correct. Not a claim
that grammar-only's 46–49 "any route" count represents wasted work —
those are real, valid 2-stage routes, just not *additional* recall
beyond what one-step search already provides for those specific rows.

## Status

Implemented and measured. Stopping here per the original scope: **not**
wired into `Planner`, **not** proceeding to search-depth 3, OQMD-based,
or reaction-network-based intermediate sources, **no** version bump, **no**
release. Quality gate green: `cargo fmt --all -- --check`, `cargo
clippy --workspace --all-targets --all-features -- -D warnings`, `cargo
test --workspace --all-features` / `--no-default-features` (16 new unit
tests, 0 failures), `RUSTDOCFLAGS="-D warnings" cargo doc --all-features
--no-deps`, `cargo semver-checks check-release --baseline-version 0.6.0
--all-features` ("no semver update required" — purely additive).

New files: `benchmarks/build_grammar_audit_split.py`,
`benchmarks/data/exploration_grammar_split_manifest.json`,
`src/transformation_grammar.rs`, `examples/exploration_grammar_audit.rs`,
`benchmarks/data/exploration_grammar_audit_result.json`. One `pub(crate)`
addition to `src/composition.rs` (`amount_of_frac`, no public API
change).

**Update (v0.7.0 release, 2026-08-27)**: this module now ships behind a
new `experimental_grammar` Cargo feature, default off, precisely because
of this audit's own NO-GO result -- not stable, unconditional public API
that promises long-term compatibility. `default_grammars()`/`propose_all()`
and every grammar type require `--features experimental_grammar` to
use. No change to the measured result or any of the numbers above.
