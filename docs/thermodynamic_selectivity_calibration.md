# Phase 21B: thermodynamic-selectivity calibration

**Status: partially executed. The calibration experiment itself has NOT
run.** Phase 21A's GO for Phase 21B (`docs/thermodynamic_selectivity_dataset_feasibility.md`)
was conditioned on four requirements. This document executes conditions
2 and 3, per an explicit owner decision to defer condition 1 (and
therefore the calibration behind it) until a Materials Project API key
is available:

| Condition | Status |
|---|---|
| 1. Thermodynamic-coverage check against gugen's real data source | **Not done — blocked.** `MaterialsProjectSnapshotProvider` structurally holds no API key and fetches nothing itself (`src/materials_project_adapter.rs` module doc, `docs/integration.md`); no key exists in this environment. Owner chose to run conditions 2/3 first rather than supply one now. |
| 2. Manual label audit (mirroring Phase 20D) | **Done — this document, §2.** |
| 3. Artifact filtering | **Done — this document, §3.** |
| 4. Carry forward leakage exclusion + DOI independence unit | **Done — both conditions 2 and 3 build directly on Phase 21A's leakage-excluded, DOI-tracked population.** |

**No calibration was run. No correlation was computed. No GO/NO-GO for
the calibration itself can be given yet** — that decision still needs
condition 1. This is the same discipline as Phase 21A's own §7
("not measured in this phase"): an unmet precondition is reported as
unmet, not quietly worked around or assumed favorable. No `src/` change,
no version bump.

## 1. Recap: what this builds on

Phase 21A found 385 targets (lenient label aggregation) / 371 (strict,
majority-vote) with ≥2 gas-free-computable routes and disagreeing
pure/impure outcomes, after excluding gugen's own 5 curated
validation-fixture targets. Full detail: `docs/thermodynamic_selectivity_dataset_feasibility.md`.

## 2. Condition 2: manual label audit (pilot wave)

**Method**, mirroring Phase 20D's exactly (`benchmarks/sample_literature_observation_audit.py`
precedent): a deterministic, seeded sample was drawn from the Phase 21A
"clean" population (`benchmarks/sample_thermodynamic_selectivity_audit.py`,
seed `20260815`, wave 0), with DOI as the independence unit (a paper
contributes at most one sampled item, since multiple records from the
same paper share one extraction run). Strata: `impure` (8 items — the
minority class, 23.2% of records, prioritized since a calibration study
depends on this class being real) and `pure` (7 items). Manifest:
`benchmarks/data/thermodynamic_selectivity_audit_manifest.json`.

**Verification**: one independent research agent per ~5-item batch (3
batches, `general-purpose`, real WebSearch/WebFetch against legitimate
sources only — publisher pages, arXiv, PMC, Semantic Scholar, OpenAlex,
Unpaywall, CrossRef; **no paywall bypass attempted, no Sci-Hub, no pirate
mirrors**), asked to determine access level and whether the source paper
supports the claimed pure/impure verdict for that specific target+route.
Raw judgments (DOI/access-level/verdict/one-line reasoning only — no
paper text, matching Phase 20D's redistributable-data constraint):
`benchmarks/data/thermodynamic_selectivity_audit_judgments.json`.

### Results (n=15)

| Access level | Count | % |
|---|---|---|
| `full_text` | 1 | 6.7% |
| `abstract_only` | 5 | 33.3% |
| `source_inaccessible` | 9 | 60.0% |

| Verdict | Count | % |
|---|---|---|
| `match` | 2 | 13.3% |
| `mismatch` | 0 | 0.0% |
| `inconclusive` | 13 | 86.7% |

Among the 6 items with *any* access (full text or abstract): 2 match, 0
mismatch, 4 inconclusive (the accessible abstract didn't address purity
either way).

**Honest reading, not smoothed over**: this pilot found **zero
contradicting evidence** among everything it could check, but the
checkable fraction is far too small (2 confirmed matches, 0 mismatches)
to compute any meaningful precision estimate — a 2-of-15 (or 2-of-6)
sample cannot certify an accuracy rate, even though it is 2-of-2 (100%)
among the items that were conclusive. Most sampled DOIs are Elsevier
(ScienceDirect), whose platform returned HTTP 403 to automated fetches
regardless of open-access status, producing a 40% any-access rate (6.7%
full text). This is directly comparable to, not notably different from,
Phase 20D's own real access rate on a different (non-Elsevier-dominated)
DOI mix at n=58: 37.9% any access (8.6% full text), per
`benchmarks/data/literature_observation_audit_judgments.json`'s own
recorded `access_level` field — checked directly for this comparison,
not assumed. A further wave would likely face a similar access wall;
this is disclosed as a real, load-bearing limitation on how much
confidence any future calibration can place in this label source's
audited accuracy — not a problem this document solves, and not one to
quietly work around by treating title/metadata alone as evidence (the
agents were explicitly instructed not to, and followed that instruction:
e.g. item `10.1039/C3CE40473K`'s title reads as strongly suggestive of a
pure result, but was correctly marked inconclusive without abstract/
full-text confirmation).

### Real methodological caveats surfaced (beyond simple access limits)

Three distinct risk patterns were found by the verifying agents while
attempting real checks, worth carrying into any future calibration's own
design, not just this audit's numbers:

- **Complex/single-crystal processes flattened into a simple element
  route**: `10.1016/j.pnsc.2020.08.019` (Ni3Al) is a directionally-
  solidified single-crystal superalloy grown by a seed-crystal method,
  not a simple powder reaction — the dataset's 3-element route is an
  approximation of a substantially more complex real process.
- **Doped-compound routes possibly mislabeled under the host-lattice
  target formula**: `10.1063/1.5078773`'s route includes `MnCO3`, so the
  real synthesized product is Mn-doped ZnGa2O4, not the plain ZnGa2O4
  the target field names — a labeling ambiguity independent of access.
- **Flux-growth/single-crystal routes flattened into the same schema as
  solid-state powder synthesis**: `10.1016/j.materresbull.2013.10.047`'s
  precursor set (`Na2B4O7`, `PbO`, `V2O5`) reads as a flux-growth recipe
  for single crystals, not stoichiometric solid-state reactants — the
  source dataset may be applying one pure/impure schema across two
  physically different synthesis paradigms.
- A weaker, harder-to-quantify pattern also appeared: for several items,
  the cited DOI's paper topic was *adjacent to* rather than *centrally
  about* synthesizing the claimed target (a composite-mechanics paper, a
  ceramic-additive/dielectric paper, a high-pressure structural-stability
  paper) — consistent with, but not proof of, the text-mining pipeline
  sometimes picking up a compound name from a methods/introduction
  section rather than the paper's own synthesis result. Flagged as a
  hypothesis worth checking in a larger audit, not a confirmed finding at
  n=15.

None of this is treated as disqualifying gugen's own use of the dataset
(Phase 21A's finding stands on its schema-level facts, not on this
audit), but it materially narrows what condition 2 can honestly claim:
**this pilot did not, and at this scale could not, certify the pure/
impure label's accuracy.** A future, larger wave remains possible but
should be sized with the 60% inaccessibility rate found here as the
realistic expectation, not the more optimistic access rate Phase 20D
happened to get on a different (non-Elsevier-dominated) DOI mix.

## 3. Condition 3: artifact filtering

**Method**: `benchmarks/audit_thermodynamic_selectivity_dataset_feasibility.py`
(extended, same file Phase 21A committed) now also excludes, from the
385-target lenient headline, any route where the target or any precursor
formula matches one of two patterns found by direct inspection while
writing Phase 21A's report:

- **Duplicated-element artifact**: a bare element symbol immediately
  followed by its own repeat (e.g. `Ti3Ti`, `Si1Si`, `Al3Al`) — a clear
  LLM-extraction duplication bug, not a real compound.
- **Non-standard-separator formula**: any formula using a character
  outside letters/digits/parentheses/middle-dot/period/x — most commonly
  a `-` joining two formulas (e.g. `NaCl-KCl`, a genuine two-species flux
  mixture, or `(MgCO3)4-Mg(OH)2·5H2O`, a real mineral compound written
  with a non-standard hydrate separator). Both cases are, either way,
  not usable as a single parseable `Composition` without manual
  disambiguation this phase does not perform — so both are excluded
  together, honestly described as "not currently usable," not uniformly
  mislabeled as "wrong."

### Result

50 routes excluded for the duplicated-element pattern, 440 for the
non-standard-separator pattern (490 total, out of ~48,000 kept routes
across the full leakage-excluded population). After filtering:

| Metric | Before filtering | After filtering |
|---|---|---|
| Targets with selectivity signal | 1,742 | 1,715 |
| Targets with ≥2 gas-free-computable routes, disagreeing outcome | **385** | **381** |

**The sample gate (≥30 targets) still passes by a 12.7x margin after
artifact filtering — the finding barely moves.** Only 4 targets' entire
selectivity signal depended on an artifact-containing route. The
artifact-filtered, gas-free-computable population (1,692 individual
route rows across 381 targets) is written to
`benchmarks/data/thermodynamic_selectivity_clean_population.json` and is
the population any future calibration (and this document's own condition
2 sample) draws from — not the uncleaned 385.

## 4. What remains before a calibration can run

1. **Condition 1 (thermodynamic coverage)**: needs a Materials Project
   API key (or an equivalent formation-energy source) to construct
   `CompetingPhase` entries for the ~381 clean targets' actual solid
   species and confirm they resolve against
   `MaterialsProjectSnapshotProvider`. Not started.
2. **The calibration experiment itself**: computing
   `balanced_reaction_delta_ev_per_atom` for each clean route pair and
   testing correlation against the audited pure/impure label — blocked
   on condition 1.
3. Condition 2's own finding (§2) should shape the eventual calibration's
   scope: given the real accessibility and route-representation caveats
   found, any future calibration should treat the pure/impure label as
   noisy and unverified-at-scale, not as ground truth with an established
   accuracy rate, and should consider excluding or flagging the specific
   caveat patterns found here (doped-compound-under-host-formula routes,
   flux-growth routes) if they recur systematically in the larger clean
   population — this was not checked at scale in this document.

## 5. Non-goals (unchanged)

No `score_plan` connection, no `RankingWeights` change, no default
ranking change, no success-probability claim, no automatic temperature
selection, no gas-phase thermodynamics, no literature-condition
promotion, no version bump, no public API change of any kind, no
calibration result of any kind (none was computed).
