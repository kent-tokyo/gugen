# Phase 21A: thermodynamic-selectivity dataset feasibility

**Question.** gugen's Phase 19P/19P.1 thermodynamic primitives
(`relative_solid_gibbs_ev_per_atom`, `balanced_reaction_delta_ev_per_atom`,
`decomposition_margin_ev_per_atom`; gas-free, solid-only, 300-1800 K) are
never connected to `score_plan` or ranking. Before any such connection is
even considered, does an independent dataset exist that could tell us
whether gugen's thermodynamic quantities actually correlate with which of
several real synthesis routes to the same target was more selective/
successful in practice?

This document reports only on that question — dataset feasibility. It
does not compute any calibration, does not propose or connect a `Score01`
mapping, and does not change any `src/` production code. **A negative
finding here would have been a formal, complete, valid outcome in its own
right** — it was not treated as a failure to work around.

**Result: GO for Phase 21B**, conditioned on four specific follow-up
requirements listed in [§7](#7-phase-21b-gonogo-decision). No version bump
accompanies this document; no public API changed.

## 1. Candidates investigated

| # | Candidate | Real experimental outcome, or computed proxy? | Distinct targets w/ ≥2 real-outcome-labeled routes | License | Verdict |
|---|---|---|---|---|---|
| 1 | McDermott, Dwaraknath, Persson et al., *ACS Cent. Sci.* (2023), DOI [10.1021/acscentsci.3c01051](https://doi.org/10.1021/acscentsci.3c01051) (`materialsproject/reaction-network`) | Mixed: 9 synchrotron-XRD-verified BaTiO3 reactions are real; the 3,520-reaction/82,985-candidate tables are computed selectivity rankings, not outcomes. | **1** (BaTiO3, 9 routes) | Code: modified BSD (`pyproject.toml`, verified). No bundled dataset license — SI is CC BY 4.0 (paper's own OA terms) but not machine-readable (embedded in PDF figures, not a structured file). | Rejected as a *label* source (n=1 target, far below sample gate). Retained as a candidate future *baseline-comparison* source for 21B — comparing gugen's ΔG against this paper's own computed selectivity metric is a legitimate methodology check, but is agreement-between-two-computed-quantities, not validation against reality. |
| 2 | Same-lineage follow-ups: Szymanski, Bartel, Ceder et al., *Sci. Adv.* (2024), DOI 10.1126/sciadv.adp3309 (~28 reactant pairs); Rom, Novick, McDermott et al., *JACS* (2024) (CaZrN2/CaHfN2, 2 targets) | Real (in-situ synchrotron XRD) | Well under 30 for either paper individually | CC BY 4.0 | Rejected individually (each below sample gate); not pursued further since candidate 6 already supersedes them in scale. |
| 3 | gugen's own literature corpus (`CorpusHeatingObservation`, Kononova-derived, Phase 20B) | Neither — **structurally has no outcome field at all.** Schema (`src/literature_observations.rs`): `target`, `precursors`, `route_family`, `heating_purpose` (always absent by construction), `operation_index`, `temperature`, `duration`, `atmosphere`, `doi`, `corpus_record_index`. No success/purity/yield/selectivity field. | 0 (not a label source in principle) | N/A | Rejected on a checkable schema fact, not a judgment call: every record is a route someone published — there is no negative class, so no field in this corpus can ever distinguish a better route from a worse one. |
| 4 | `MaterialsProjectSnapshotProvider` competing-phase energies (already in gugen, feature-gated) | Computed (DFT formation energy), not an experimental outcome | N/A | N/A | Rejected as a *label* source — this is a candidate **feature** source gugen already has, not ground truth for which route was actually more selective. Using it as its own label would be circular. |
| 5 | Open Reaction Database | Real, but organic/pharma reaction screens only (schema's own example: Fischer esterification) | 0 (no inorganic solid-state content) | N/A | Rejected — wrong domain. |
| 6 | **Lee, Cruse, Baibakova, Ceder, Jain, *Scientific Data* (2025)**, "Text-mined dataset of solid-state syntheses with impurity phases using Large Language Model." Figshare DOI [10.6084/m9.figshare.30423274](https://doi.org/10.6084/m9.figshare.30423274) | Real (LLM-text-mined from the primary literature, purity outcome explicitly extracted per synthesis attempt) | **385** (see §5) | **CC BY 4.0**, verified live | **Accepted.** The rest of this document characterizes this dataset. |

Search coverage for candidate 5 and for "any other independent dataset":
GitHub/arXiv/Semantic Scholar/Europe PMC API searches and citation-graph
traversal from the 36 papers citing candidate 1 (open-web search was
unavailable this session; see the research pass this document is based
on). NOMAD/OQMD/AFLOW/USPEX experimental cross-references were not
directly queried — flagged as unverified, not as confirmed-absent, should
a future pass want to check them.

## 2. Verification method (not taken from the paper's own description)

Per this project's established "verify, cite, nothing from memory"
discipline, candidate 6 was not accepted on the strength of its abstract.
The actual file was downloaded and independently parsed:

- Figshare article metadata fetched live: `license.name == "CC BY 4.0"`, confirmed.
- File `SS_rxns_80806.json.gz` (33,577,326 bytes) downloaded; MD5
  `2cca759682be4689e7d0d3d882d12909` verified against the figshare API's
  own `computed_md5` for that file — byte-identical to the real object.
- Parsed as JSON: a flat list of 80,806 records, each with `target`,
  `precursors`, `target_reaction` (a balanced equation), `impurity_phase`
  (list, possibly empty), `DOI`.
- All counts below come from
  `benchmarks/audit_thermodynamic_selectivity_dataset_feasibility.py`,
  reproducible by anyone by re-running that script (raw file gitignored,
  matching the "no bulk data bundled" precedent; a small machine-readable
  summary manifest — `benchmarks/data/thermodynamic_selectivity_dataset_feasibility_manifest.json`,
  8.6 KB — is committed).

This script touches no `src/` code and computes no gugen thermodynamic
quantity; it only characterizes the candidate label dataset itself.

## 3. Inclusion / exclusion rules

Applied in this order, each exclusion counted (never silently dropped):

1. **Null/unparseable target formula** (a text-mining extraction failure,
   `target[0].material_formula == null`): excluded. 9,914 of 80,806
   records (12.3%).
2. **Leakage against gugen's own curated validation fixtures**
   (`tests/validation.rs`, `src/literature_conditions.rs`): any record
   whose target is `BaTiO3`, `CaO`, `LaAlO3`, `LiFePO4`, or `MgAl2O4` is
   excluded outright — mirrors `benchmarks/fetch_kononova.py`'s identical
   target-level exclusion discipline, so that a future calibration study
   can never be "validated against" a target gugen's own route-generation
   code was already tuned or tested against. All 5 were present in this
   corpus; 1,104 records excluded.
3. **A "route" is defined as one target's distinct, deduplicated,
   alphabetically-sorted precursor-formula set** (not the balanced
   reaction, which can vary slightly in stoichiometry across reports of
   the same real route).
4. **A target has "selectivity signal"** only if it has ≥2 distinct
   routes *and* those routes' outcome verdicts disagree (at least one
   route pure, at least one impure). A target where every reported route
   was pure (or every one impure) carries no selectivity information —
   thermodynamics can't be tested against an outcome that never varies.
5. **A route is "gas-free-computable"** only if at least one of its
   extracted `target_reaction` balanced equations names no species from a
   fixed gas list (`O2`, `CO2`, `H2O`, `N2`, `H2`, `NH3`, `NO`, `NO2`,
   `N2O`, `SO2`, `SO3`, `CO`, `Cl2`, `HCl`, `H2S`, `CH4`, `F2`, `HF`,
   `Br2`, `I2`, `H2O2`, `N2O5`, `NO3`). This mirrors, without
   reimplementing, `balanced_reaction_delta_ev_per_atom`'s actual
   mechanism (`src/thermodynamics.rs`): it returns `Ok(None)` — a
   legitimate abstention, not an error — the moment any participating
   species has no matching `SolidThermodynamicEntry`, and a gas species
   (no crystal volume) never has one. A route naming a gas species is
   therefore, today, not a route gugen's existing function can produce a
   ΔG for at all.

## 4. Sample gate (fixed before any candidate's real numbers were seen)

Per this phase's own explicit pre-implementation advisor review: a
calibration dataset below **30 distinct targets, each with ≥2 comparable
(gas-free-computable) routes and a real, non-imputed outcome label**,
cannot support any correlation claim — a floor set with reference to
Phase 20D's own precedent (58 DOIs produced field-level n=6-8 with wide
confidence intervals it explicitly declined to call precise). This
threshold was written down *before* the dataset in §2 was downloaded, so
that a marginal result could not be talked into passing after the fact.

## 5. Sample counts

Of 80,806 raw records (9,914 null-target- and 1,104 leakage-excluded,
69,788 remaining across 33,001 distinct targets):

| Metric | Count | % of 69,788 |
|---|---|---|
| Outcome: pure (no impurity phase reported) | 53,582 | 76.8% |
| Outcome: impure (≥1 impurity phase reported) | 16,206 | 23.2% |
| No `target_reaction` extracted at all | 17,932 | 25.7% |
| `target_reaction` extracted, gas-free | 28,243 | 40.5% |
| `target_reaction` extracted, involves a gas species | 23,613 | 33.8% |

| Metric | Count |
|---|---|
| Distinct targets, any route/outcome data | 33,001 |
| Targets with ≥2 distinct precursor-set routes | 4,453 |
| **Targets with selectivity signal** (≥2 routes, differing pure/impure verdict) | **1,742** |
| ...of which span ≥2 distinct DOIs (not a single-paper artifact) | 1,710 (98.2%) |
| **...of which have ≥2 gas-free-computable routes that still disagree in outcome** | **385** |

**385 clears the 30-target sample gate by a margin of ~12.8x.** This was
not a marginal pass requiring any relaxation of the pre-stated criterion.

### Sensitivity to label-aggregation definition

The 385 figure uses a lenient aggregation: a route counts "pure" if *any*
reported attempt of it was pure, and "gas-free-computable" if *any* of its
extracted reactions was gas-free — optimistic for a route reported many
times (e.g. one pure result out of 40 attempts still labels the whole
route "pure"). A pre-commit review flagged this as undisclosed, so both
definitions are computed and reported, not just the lenient one:

| Aggregation | Targets with selectivity signal | Gas-free-computable, disagreeing outcome |
|---|---|---|
| Lenient (any-pure outcome, any-gas-free route) | 1,742 | **385** |
| Strict (majority-vote outcome, ties excluded; every extracted reaction of a route must be gas-free) | 1,690 | **371** |

The count is not sensitive to this choice in practice: 371 vs. 385 is a
3.6% difference, both comfortably above the 30-target floor (12.4x and
12.8x respectively). Separately, whether "any" or "all" extracted
reactions of a route must be gas-free turned out to make **no**
difference at all (385 and 371 hold under either gas-free rule) — checked
directly, not assumed: no route in this dataset had a mix of gas-free and
gas-releasing extractions across its reported attempts.

### Chemical-family distribution (of the 385 gas-free-computable targets)

| Family | Count | % |
|---|---|---|
| Oxide | 193 | 50.1% |
| Other/intermetallic | 148 | 38.4% |
| Halide | 21 | 5.5% |
| Sulfide/chalcogenide | 17 | 4.4% |
| Nitride | 6 | 1.6% |

(Classified by presence of O/N/S/halogen in the target formula; a coarse,
reproducible heuristic — see the script — not a materials-science
ontology.)

## 6. Gas-free applicability

This is the single largest reduction in this analysis: of 1,742 targets
with a genuine selectivity signal, only 385 (22%) survive the requirement
that ≥2 of their routes be individually computable by gugen's existing,
unmodified thermodynamic functions. Real solid-state synthesis
overwhelmingly proceeds through carbonate/nitrate/hydroxide decomposition
and O2 uptake or release — exactly the class of reaction Phase 19P's
gas-free scope was deliberately narrowed to exclude (see
`src/thermodynamics.rs`'s own module doc: "gas-free, closed solid-phase
systems only"). This is disclosed here as a real, load-bearing constraint
on any future calibration's scope, not smoothed over: **a Phase 21B study
would only ever be able to test the gas-free 22% of this dataset's
selectivity signal**, and any resulting finding (positive or negative)
would only generalize to gas-free routes, not to synthesis in general.

## 7. Thermodynamic coverage — not measured in this phase

**This document does not check whether the 385 targets' actual solid
species (BaCO3, TiO2, Fe2O3, Ni, Al, Nb2O5, ...) have real entries in a
thermodynamic dataset gugen can use** (e.g. via the feature-gated
`MaterialsProjectSnapshotProvider`). Doing so would require live
DFT-database queries and touches the same "does gugen already have this
data" question §1's feature/label distinction was built to keep separate
from label-hunting — and per this phase's own scope boundary ("does not
touch gugen's thermodynamic functions"), that check was not performed
here. Species observed are, by inspection, overwhelmingly common,
well-characterized inorganic compounds (oxides, carbonates, elements,
simple binaries) — but that is an unmeasured impression, not a
number this phase produced, and it is named as **Phase 21B's own
required first task** (§13, condition 1) rather than assumed adequate.

## 8. Leakage risk analysis

- **Target-level leakage against gugen's own fixtures**: mechanically
  excluded (§3, item 2) — `BaTiO3`, `CaO`, `LaAlO3`, `LiFePO4`,
  `MgAl2O4`, 1,104 records, 1.4% of the raw corpus. Removing them barely
  dents the sample (1,742 of 1,747 pre-exclusion selectivity-signal
  targets survive) — leakage avoidance does not collapse this dataset.
- **Independence unit**: DOI, matching Phase 20D's precedent. 98.2% of
  selectivity-signal targets have their differing-outcome routes reported
  across ≥2 distinct DOIs, meaning the signal is generally not a
  single-paper artifact.
- **Label/feature independence**: the *label* (pure/impure, text-mined
  from published papers) and the *feature* Phase 21B would compute
  (DFT-derived thermodynamic entries) come from structurally
  disconnected sources — this is the standard, accepted design for this
  class of study (the same pattern used in Ceder/Jain-group
  synthesizability work combining DFT features with text-mined outcome
  labels), not a leakage risk in itself.
- **Shared literature lineage, disclosed but not a leakage risk for this
  purpose**: this dataset and gugen's existing Kononova-derived corpus
  (Phase 20B) both draw on the broader published solid-state synthesis
  literature and share a general text-mining lineage (Ceder-group
  tooling). Some underlying papers may overlap between the two. This
  does not compromise a future calibration, since the two corpora serve
  different purposes (reference-only evidence display vs. a calibration
  label) and gugen's thermodynamic feature computation depends on
  neither.

## 9. Label-quality caveats (ground truth genuineness)

Per the pre-implementation advisor-review question "does ground truth
genuinely represent route/selectivity" — named honestly, not glossed over:

- **Pure/impure is binary and coarse**, not a continuous selectivity
  score. It reflects whether *any* impurity phase was reported for one
  specific synthesis attempt — conflating route thermodynamics with
  process-parameter execution (temperature/time/atmosphere choices),
  reaction kinetics, and each paper's own characterization detection
  limit and reporting practice. A route can show "impure" because it is
  thermodynamically disfavored, or because a specific paper under-fired
  it — this dataset cannot distinguish the two, and neither would a
  calibration built on it without further work.
- **Extraction is LLM-based and imperfect.** The source paper's own
  validation (98 hand-annotated reactions) reports target F1 0.78,
  precursor F1 0.96, impurity-phase F1 0.88 — not independently
  re-verified by gugen in this session, only cited from the paper.
- **Visible extraction artifacts**, observed directly while inspecting
  example routes for this document: duplicated/malformed formula tokens
  (e.g. `Ti3Ti`, `Si1Si`) and flux-mixture notations (e.g. `NaCl-KCl`)
  appearing as if they were single chemical species. These were not
  cleaned or filtered in this phase (out of scope — descriptive only)
  and would need explicit handling before any route containing them is
  used in a real calibration.
- **Paywall**: a future manual audit spot-check (mirroring Phase 20D's
  methodology) would still need to open individual, sometimes paywalled,
  source papers to verify a sampled label — a known cost, not a blocker,
  since Phase 20D already demonstrated a workable partial-audit approach
  at a comparable scale.

None of these caveats individually or together trip any of the phase's 9
explicit stop conditions (§10) — they are disclosed as real constraints
on what a future calibration could credibly claim, not reasons to reject
the dataset.

## 10. Explicit check against all 9 stop conditions

| Stop condition | Triggered? | Basis |
|---|---|---|
| Independent labels barely exist | **No** | 385 targets clear the sample gate by 12.8x |
| Mostly gas/liquid/aqueous, outside Phase 19P scope | No (but material) | 78% of selectivity-signal targets are excluded by gas-free filtering — disclosed in §6 as the largest real constraint, not a phase-killer |
| Competing assemblage needs arbitrary hindsight selection | No | Routes are real, independently reported alternatives, not hindsight-constructed |
| Thermodynamic data coverage too low | Unverified, not triggered | Not yet checked (§7); named as 21B's first required task rather than assumed either way |
| Leakage avoidance collapses the sample | No | Excluding all 5 known fixture targets removes 1.4% of records |
| Outcome definition incompatible across papers | No (but material) | Real LLM-extraction noise exists (§9), disclosed, not disqualifying at this scale with 98.2% multi-DOI corroboration |
| Paywall prevents verifying judgment basis | No (but a known future cost) | Dataset itself is openly structured; a manual audit (21B) will still hit some paywalls, as Phase 20D already did |
| Dataset license unknown | No | CC BY 4.0 verified live |
| Would need to cherry-pick success-only cases | No | 23.2% of all outcomes are impure; a real, non-trivial negative class exists |

## 11. Proposed label schema (sketch only — not connected to Planner/ranking)

- **Unit**: one `(target_composition, precursor_set)` pair = one route.
- **Label**: binary pure/impure per route. This document's headline count
  (§4, 385) uses the lenient "pure if any reported attempt of this exact
  route was pure" aggregation; a stricter majority-vote alternative was
  also computed (§5's sensitivity subsection: 371, a 3.6% reduction, not
  materially different) but is not automatically adopted — Phase 21B
  must decide this explicitly rather than silently default to whichever
  one this document happened to use as its headline.
- **Comparison unit**: within-target route pairs only, never cross-target
  — matches this whole phase's "which route to the same target" framing.
  A comparison is never made between routes to different targets.
- Explicitly **not** a continuous selectivity score, given the
  underlying data's own binary resolution limit.

## 12. Non-goals of this document (unchanged from the approved brief)

No `score_plan` connection, no `RankingWeights` change, no default
ranking change, no success-probability claim, no automatic temperature
selection, no gas-phase thermodynamics, no literature-condition
promotion, no version bump, no public API change of any kind.

## 13. Phase 21B go/no-go decision

**GO**, conditioned on Phase 21B's own scope explicitly including, as
required first steps rather than assumptions carried over from this
document:

1. **A real, live thermodynamic-coverage check** of the 385 targets'
   actual solid species against gugen's own thermodynamic data source
   (§7) — not assumed adequate.
2. **A manual audit spot-check** (mirroring Phase 20D's methodology,
   comparable sample size) of the pure/impure label against original
   source papers, to establish real label precision before any
   calibration claim relies on it.
3. **Explicit filtering** of malformed/artifact formula tokens (§9)
   before any affected route is used.
4. **Carrying forward** the leakage exclusion (5 fixture targets) and the
   DOI independence unit throughout.

A null/negative correlation finding in Phase 21B, if that is what these
steps lead to, is itself the valid, complete, reportable outcome named in
this phase's own brief — not a result to be quietly worked around.

Phase 21B is **not** auto-started by this document. It requires a
separate, new owner confirmation, per the owner's own explicit
instruction.
