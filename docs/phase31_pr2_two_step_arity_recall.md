# Phase 31 PR 2 — Corpus-Grounded Intermediate Candidates + Two-Step Arity Recall

## Why this exists

The owner formally closed the gugen Playground track (5 shipped phases:
MVP, GitHub Pages deployment, accessibility hardening, free-form input,
commercial-catalog mini-demo) and redirected development back to gugen
core: Phase 31 (reaction hypergraph / multi-step routes). PR 1 built
`search_two_step_routes` (`src/multi_step.rs`) but left
`intermediate_candidates: &[Composition]` caller-supplied, verified
only against hand-built synthetic fixtures.

The owner's explicit instruction: decide the multi-step-edge **data
source** before writing more implementation, in priority order —
(1) a literature route corpus, (2) a small number of explicit
hand-written reaction grammars, (3) OQMD-style thermodynamic data,
(4) external reaction-network-derived data. Don't jump to large-scale
data integration; design a minimal step that correctly recovers known
multi-step routes using (1)+(2) first.

## Research: why tier 1 is a dead end, and what tier 2 actually means here

Checked directly, not assumed:

- `benchmarks/data/kononova_sample.jsonl` (1500 rows) and
  `benchmarks/data/thermodynamic_selectivity_clean_population.json`
  (Lee et al. 2025, 1692 rows) are both a single flat overall reaction
  per record — no intermediate/multi-step field in either schema.
- McDermott et al.'s reaction-network paper (ACS Cent. Sci. 2023,
  already on file in `docs/thermodynamic_selectivity_dataset_feasibility.md`
  as PDF-figure-only) was checked directly this session: blocked by
  the publisher's bot protection (HTTP 403). A web search of the
  paper's own abstract independently confirmed its baseline reaction is
  `BaCO3 + TiO2 -> BaTiO3 + CO2`, a **single-step** reaction — an
  earlier hypothesis in this session that BaTiO3 goes through an
  isolated BaO intermediate was wrong and was dropped before any code
  was written around it.
- **No fabricated "known route" is used anywhere in this PR.**

Tier 2 ("a small number of explicit hand-written reaction grammars")
turned out not to require inventing any new chemistry rule.
`search_two_step_routes`'s only requirement is a plain
`&[Composition]` of candidate intermediates, and `FrequencyPriorGenerator`
(`src/candidate_generator.rs`, already shipped in Phase 30 PR 1) already
does exactly this kind of proposal: element-overlap filtering against
a caller-supplied frequency table, general-purpose for any target. So
"tier 2" here means: build one **global** frequency table from every
precursor formula in the already-committed, real, licensed
`kononova_sample.jsonl` (1500 independent literature reactions), and
reuse `FrequencyPriorGenerator` unchanged as the intermediate source.

## The measurement: a real, checkable substitute for "recovers known routes"

No corpus has ground-truth multi-step labels, so "correctly recovers
known multi-step routes" can't be checked against a labeled answer
key. The honest substitute: `benchmarks/fetch_kononova.py` already
discards any real reaction needing more than
`SearchBudget::default().max_precursors_per_plan` (4) distinct
precursors. Those are real, literature-cited reactions gugen's
existing single-step search structurally cannot reach today. Measuring
how many of them `search_two_step_routes` (unchanged) now reaches,
given real precursor lists and the frequency-table intermediate
source, is a real, countable, zero-fabrication number.

**New data**: `benchmarks/fetch_kononova_high_arity.py` (imports
`fetch_kononova.py`'s own `parseable_composition`/`canonical_ratio`/
`route_key`/`EXCLUDED_ROUTES`/`fetch_dataset`, no logic duplicated;
does not modify `fetch_kononova.py` or its existing output) re-fetches
the same live-license-checked raw dataset and keeps reactions needing
more than 4 precursors instead of discarding them. Run for real (live
91MB download, not cached anywhere on this machine — checked directly
before running). Result: **408** real high-arity reactions (arity 5:
347, 6: 54, 7: 7), zero excluded for leakage against gugen's own
fixtures. This also resolved an open question:
`fetch_kononova.py`'s own combined `zero_or_too_many_precursors`
counter (408) turned out to be **100% arity>4**, 0% zero-precursor.

**New measurement**: `examples/exploration_two_step_arity_recall.rs`
builds the global frequency table from `kononova_sample.jsonl`
(disjoint from the holdout by construction), and for each holdout row
derives intermediate candidates via `FrequencyPriorGenerator::generate`,
filtered to compositions with fewer elements than the target and not
already a base precursor. `MAX_INTERMEDIATE_CANDIDATES` was calibrated
empirically (swept 20/50/100/200/380): net-new recall rose
monotonically at every step, never saturating below the frequency
table's own full size (380) — so it's set generously above that
(2000), not capped below the real data's own size for no reason.

## Result — reported honestly

**A real, surprising finding reshaped the metric itself**: of the 408
"high-arity" holdout rows, **111 were already reachable in one step**
from a smaller real subset of the listed precursors — the corpus's own
"needs more than 4 precursors" listing does not mean gugen's
single-step search actually fails on all of them. Counting these
toward two-step "recall" would be dishonest (they didn't need two-step
help). The correct denominator is the **294** rows confirmed genuinely
unreachable in one step.

Of those **294** rows, two-step search — using only the corpus-grounded
frequency-prior intermediate source, zero hand-written chemistry rules
— recovered **12 net-new** (a route not already found in one step):

- **12/294 = 4.08%** (relative to the raw 408-row holdout: 12/408 = 2.94%)
- By arity: 5 → 11/237 (4.64%), 6 → 1/50 (2.00%), 7 → 0/7 (0%)

This is modest but real, non-fabricated, and honestly reported —
matching this project's own "report the number even when it's small"
precedent (Phase 30's ensemble ablation). **No formal gate is claimed
passed or failed here** — this PR establishes the measurement and a
first real number, not a pre-declared pass/fail threshold.

**Hand-verified, not just trusted from the aggregate count**: the
`SiO2P2O5K2OMgOCaO` row (DOI 10.1016/j.tca.2014.08.028, 5 real
precursors: MgO, CaCO3, H3PO4, SiO2, K2CO3) was traced by hand.
Stage 0: `2 H3PO4 + K2CO3 -> 2 KH2PO4 + CO2 + H2O` (verified balanced
on every element by hand) produces KH2PO4 — monopotassium phosphate, a
real, well-known compound, not an artifact. Stage 1:
`MgO + CaCO3 + SiO2 + 2 KH2PO4 -> target + CO2 + 2 H2O` (also verified
balanced by hand) reaches the exact real target composition. The
staging is chemically sensible (acid–carbonate neutralization, then
oxide/phosphate combination) even though it isn't a claim that this
matches the original paper's own actual procedure (see "What this does
not claim").

## Discovered Work — two real, pre-existing defects, not fixed here

Running this measurement against real data (not just synthetic
fixtures, PR 1's own limitation) surfaced two genuine correctness gaps,
affecting **3 of 408** rows:

1. **`search_precursor_sets_core` can accept a spurious identity
   reaction.** Reproduced with a minimal standalone case: a candidate
   whose composition exactly equals one of `curated_byproducts()`
   (e.g. elemental O2) can be "accepted" as a trivial `O2 -> O2`
   no-op, completely unrelated to the actual target. This lives in
   `src/precursor.rs`'s core search, not in `src/multi_step.rs` —
   it would affect the real `Planner` too, not just this benchmark.
2. **`search_two_step_routes`'s own route-construction loop is not
   robust to an unexpected accepted entry.** It converts every
   `direct.accepted` entry via `SynthesisRoute::new(...)?` inside a
   `for` loop — one malformed or unexpected entry (from finding #1
   above, or from a separate `UnexplainedReactant` case seen once in
   this run) aborts the **entire row's** route construction, silently
   discarding otherwise-valid one-step and two-step routes for that
   row, not just the one bad entry.

Both are genuine, reproducible, **not fixed in this PR** — deliberately
out of scope (this PR's own approved plan explicitly excluded any
change to `src/multi_step.rs` or `search_precursor_sets`, and both
defects need a dedicated investigation + regression pass, not a
same-PR patch). The 3 affected rows
(`NdSiAlON`/`YbSiAlON` from DOI 10.1016/j.jmatprotec.2006.10.015, and
`Na0.5Bi0.5Cu3Ti4O12` from DOI 10.1016/j.materresbull.2014.01.009) are
excluded from every recall figure above and listed by name in
`benchmarks/data/exploration_two_step_arity_recall_result.json`'s
`search_errors`, not silently dropped. **Recommended as a high-priority
follow-up**, given finding #1 affects real `Planner` usage, not only
this benchmark.

## What this does not claim

No claim about real supplier/procedure data — this measures gugen's
own two-step *search capability* against real, high-arity literature
targets, using a real, corpus-grounded (not fabricated) candidate
source. It is **not** a claim that any recovered intermediate (e.g.
KH2PO4 above) matches the documented real synthesis procedure for that
paper — no per-step ground truth exists in any corpus available to
this project to check that against. No claim that 4.08% is a ceiling —
only the frequency-prior/element-overlap/simpler-than-target design
explored here; a hand-written decomposition grammar (tier 2's other
half) or an OQMD-based intermediate source (tier 3) remain open,
undecided next steps. No claim that the two discovered defects are
fixed — see Discovered Work.

## Status

Implemented and measured. `src/` untouched (per the approved plan's
explicit scope boundary); new files: `benchmarks/fetch_kononova_high_arity.py`,
`benchmarks/data/kononova_high_arity_sample.jsonl` (committed, 408
real rows), `examples/exploration_two_step_arity_recall.rs`,
`benchmarks/data/exploration_two_step_arity_recall_result.json`. Root
quality gate (`cargo fmt --all -- --check`, `cargo clippy --workspace
--all-targets --all-features -- -D warnings`, `cargo test --workspace
--all-features` / `--no-default-features`) green.
