# Landscape Survey (Phase 0)

Verified 2026-08-13 via crates.io API, GitHub API, and web search. Every
source below was actually fetched; nothing is cited from memory. Where a
claim could not be verified, it is marked as such instead of filled in.

## 1. Name-collision check

| Name | crates.io | GitHub | Verdict |
|---|---|---|---|
| `gugen` | 0 matches (`GET /api/v1/crates?q=gugen`) | 26 repos matched `gugen`, none materials/chemistry related (POI-recommendation network, GitHub-profile READMEs, an event called "Gugen2019", "Gugenheim", an Ansible example) | No collision. Proceed with the name as instructed in AGENTS.md §1. |
| `chematic-crystal` | 0 exact match; `chematic` and 9 sibling crates (`chematic-mol`, `chematic-3d`, `chematic-fp`, …) exist, all organic/molecular cheminformatics (SMILES, SDF/MOL, fingerprints, force fields) by the `kent-tokyo` GitHub org | 0 repos named `chematic-crystal` | Not yet published anywhere. Treated as "not yet available" per AGENTS.md §5 — build against a minimal trait boundary, not a direct dependency. |
| `mikiwame` | 0 matches | 1 unrelated repo (`timatima0314/mikiwame`, no description, no `kent-tokyo` affiliation) | Not yet published by the expected ecosystem owner. Treated as optional/unavailable per AGENTS.md §5. The unrelated same-name GitHub repo is not a collision risk for a crates.io package (crates.io namespace is separate), but is worth re-checking before publish. |
| `renkin` | Exists: `renkin` v0.23.0, "Ultra-fast retrosynthesis engine for computer-aided synthesis planning (CASP) — pure Rust, WASM-ready, Python bindings via PyO3", `kent-tokyo` org | matches | Confirms AGENTS.md §1's reference point. gugen must not depend on it (AGENTS.md §1) and must not reuse its retrosynthesis algorithms/types (AGENTS.md line 76). |

### Same-author ecosystem context

The `kent-tokyo` GitHub org (source: `GET /users/kent-tokyo/repos`) also owns
`risksieve` and `veridict` — both named in AGENTS.md §1's ecosystem diagram
as future related tools — plus `yomitoki` ("Fast, explainable, route-free
molecular synthesizability diagnostics"), which is the molecular-side analog
of `mikiwame`'s materials-side diagnostics role. This corroborates that
AGENTS.md describes a real, coherent, in-progress ecosystem rather than an
aspirational one, and that `chematic-crystal` / `mikiwame` are plausibly
in-progress but simply not pushed/published yet.

### `renkin`'s public API shape (checked so gugen does not reinvent or copy it)

Fetched from docs.rs/renkin: modules `search` (core retrosynthesis search),
`scorer`/`score`, `reranker` (LightGBM-based), `evidence`/`evidence_match`,
`synthesizability`, `validation`, `candidate`, `ring_context`. Error handling
via `anyhow`. This is a template/route-graph search over molecular reaction
templates — a different problem shape from gugen's bounded precursor-set
search over inorganic compounds (AGENTS.md §9–§10). The *naming vocabulary*
(evidence, confidence/validation, candidate, score) is shared ecosystem
convention and already mirrored in AGENTS.md §6's type design — no further
action needed, just confirmation gugen isn't drifting from it by accident.

## 2. Prior art in inorganic synthesis planning

Found via web search, 2026-08-13. Listed with what each is, and what gugen
does differently.

- **Precursor recommendation via literature-mined material similarity.**
  He, Huo, et al., *Science Advances* 9, eadg8180 (2023),
  [arXiv:2302.02303](https://arxiv.org/abs/2302.02303),
  [science.org/doi/10.1126/sciadv.adg8180](https://www.science.org/doi/10.1126/sciadv.adg8180).
  Learns precursor sets from a knowledge base of ~29,900 text-mined recipes
  by analogy to similar known targets; reports 82%+ top-5 success on 2,654
  held-out targets. This is the closest prior art to §9's precursor
  candidate generation, but it is a statistical recommender trained on a
  literature corpus, not a rule-based/deterministic bounded search with
  explicit rejection reasons. gugen's differentiator (AGENTS.md §24) is
  determinism, explicit evidence/assumption separation, and returning
  *why candidates were rejected*, none of which this line of work targets.

- **LLM-based synthesis planning.** "Language Models Enable Data-Augmented
  Synthesis Planning for Inorganic Materials," *ACS Applied Materials &
  Interfaces* 17(51) (2025),
  [pubs.acs.org/doi/10.1021/acsami.5c11229](https://pubs.acs.org/doi/full/10.1021/acsami.5c11229).
  Off-the-shelf LLMs (GPT-4.1, Gemini 2.0 Flash, Llama 4 Maverick) predict
  precursors (up to 53.8% top-1 / 66.8% top-5) and calcination/sintering
  temperatures (MAE <126 °C). This is precisely the category AGENTS.md §3
  and §21 forbid gugen from becoming ("LLMによる根拠なしのrecipe生成"): a
  temperature MAE of >100 °C from an ungrounded LLM is not a suggested range
  with evidence, it is a point estimate with no traceable source. gugen must
  not converge toward this pattern.

- **Ranking-based precursor set prediction.** "Retro-Rank-In: A
  Ranking-Based Approach for Inorganic Materials Synthesis Planning" (2025),
  [arXiv:2502.04289](https://arxiv.org/pdf/2502.04289). Targets the
  limitation that prior ML models can only recombine precursors seen in
  training. Relevant as a reference point for §13's ranking design, but
  gugen's v0.1 ranking is explicitly rule-based/multi-criteria
  (`PlanScoreBreakdown`), not learned.

- **Text-mined synthesis recipe dataset.** Kononova, Huo, He, Rong, Botari,
  Sun, Tshitoyan, Ceder, "Text-mined dataset of inorganic materials
  synthesis recipes," *Scientific Data* 6, 203 (2019),
  [nature.com/articles/s41597-019-0224-1](https://www.nature.com/articles/s41597-019-0224-1),
  code at
  [github.com/CederGroupHub/text-mined-synthesis_public](https://github.com/CederGroupHub/text-mined-synthesis_public).
  19,488 synthesis entries auto-extracted from 53,538 paragraphs, with
  target, precursors, operations/conditions, and balanced equations. Used
  in Phase 8 as a source of curated, cited validation fixtures
  (`tests/validation.rs`) — its hosted data
  (`10.6084/m9.figshare.9722159`) is licensed **CC BY 4.0**, verified via
  the figshare API on 2026-08-14 (`license.name == "CC BY 4.0"`), not the
  GitHub code repo (which carries no license). Only a handful of
  individual cited routes are used, with attribution; the raw dataset is
  not bundled in this repo. Querying the full dataset also produced a
  real finding: it contains zero reactions whose target is a plain binary
  oxide (NiO, Fe2O3, ZnO, ...) — see `tests/validation.rs`'s module doc
  comment.

- **Reaction-network / thermodynamic-selectivity approaches.** McDermott,
  Dwaraknath, Persson, "A graph-based network for predicting chemical
  reaction pathways in solid-state materials synthesis," *Nat. Commun.*
  (2021); McDermott et al., "Assessing Thermodynamic Selectivity of
  Solid-State Reactions for the Predictive Synthesis of Inorganic
  Materials," *ACS Central Science* (2023),
  [pubs.acs.org/doi/10.1021/acscentsci.3c01051](https://pubs.acs.org/doi/10.1021/acscentsci.3c01051).
  Builds a reaction network from computed phase-diagram thermodynamics and
  scores reaction selectivity (primary/secondary competition metrics) over
  3,520 literature reactions. This is thermodynamics-driven route
  discovery — closer to a `ThermodynamicProvider` implementation than to
  gugen's core planner. AGENTS.md §4.3 explicitly requires separating
  "thermodynamically favorable" from "experimentally likely to succeed";
  this line of work is a potential future provider, not something gugen's
  core should reimplement.

- **Phase-evolution simulation.** "A Cellular Automaton Simulation for
  Predicting Phase Evolution in Solid-State Reactions" (ReactCA),
  *Chem. Mater.* (2025), [arXiv:2407.19124](https://arxiv.org/pdf/2407.19124).
  Simulates time-dependent intermediate/product phase evolution as a
  function of precursor choice, atmosphere, and heating profile. Out of
  scope for gugen v0.1 (AGENTS.md §3 excludes kinetic simulation).

## 3. Positioning (AGENTS.md §24, restated as a design commitment)

None of the surveyed prior art combines: (a) a deterministic, rule-based
core with documented default weights, (b) explicit separation of evidence,
assumption, confidence, and applicability, (c) rejected-candidate reasons as
a first-class output, and (d) an explicit refusal to state ranking scores as
success probabilities. That combination — not a novel ML model — is gugen's
differentiation, per AGENTS.md §24.

## 4. v0.4.0 competitive positioning correction (verified 2026-08-15)

The owner ran an external competitive-scoring analysis of gugen 0.4.0
against four named prior-art systems (reaction-network, Retro-Rank-In,
SyntMTE, SynthesisSimilarity), scoring gugen 86/100 (up from 83/100 at
v0.3.0, itself the subject of an earlier such analysis that triggered
Phase 18). Before using that analysis to plan future work, every claim
in it was independently re-verified — gugen-side numbers against this
repo's own currently-committed files, competitor-side claims via live
GitHub/arXiv/Semantic Scholar API fetches on 2026-08-15 (not from
memory, matching this document's own §1-§2 discipline). Two real
corrections surfaced; recorded here so a future comparison doesn't
repeat them.

### 4.1 SyntMTE is not a new competitor — it is this document's own
### "LLM-based synthesis planning" entry (§2), under-described

The paper behind the report's "SyntMTE" entry, "Language Models Enable
Data-Augmented Synthesis Planning for Inorganic Materials"
([arXiv:2506.12557](https://arxiv.org/abs/2506.12557), posted
2025-06-14; *ACS Applied Materials & Interfaces* 17(51) (2025),
self-reported "under review" on the repo, unverified beyond the
authors' own text), is the **same paper** §2 already cites for its
off-the-shelf-LLM baseline numbers (MAE <126 °C). "SyntMTE" is that
paper's own specialized fine-tuned model — a better-performing variant
within the same work, not a separate line of research. Verified
directly against the code repo
([github.com/Thorben010/SyntMTE](https://github.com/Thorben010/SyntMTE)):
sintering-temperature MAE 73 °C and calcination-temperature MAE 98 °C
(confirmed verbatim against the arXiv abstract), last push 2025-09-07,
no `LICENSE` file (`license: null` via the GitHub API). All four
specific numeric/date/license claims about SyntMTE check out — the
error was filing it as a fifth, independent competitor rather than a
missed detail on an entry §2 already had.

### 4.2 reaction-network's cited BaTiO3 statistic is the same source
### §2 already cites, not a second, independent one

The report's supporting figure for reaction-network (3,520 literature
reactions analyzed; 82,985 candidate BaTiO3 reactions; 9 selected for
experimental testing) traces to McDermott, Dwaraknath, Persson,
"Assessing Thermodynamic Selectivity of Solid-State Reactions for the
Predictive Synthesis of Inorganic Materials," *ACS Central Science*
(2023), DOI [10.1021/acscentsci.3c01051](https://pubs.acs.org/doi/10.1021/acscentsci.3c01051)
(arXiv:2308.11816) — the exact paper §2 already cites for
reaction-network, not a separate study. The report frames it as
"別研究" (a separate study), which double-counts one citation as if it
corroborated the entry from two independent directions. This doesn't
make reaction-network's real capability smaller — the repo is
genuinely active
([github.com/materialsproject/reaction-network](https://github.com/materialsproject/reaction-network),
~1,057 commits on the default branch, confirming "1000+"; README
confirms reaction enumeration, reaction-network construction, and
pathfinding) and the one cited study is a genuine experimental
validation — but the report's own stated rationale for reaction-
network's score leans on one independent source, not the two its
"別研究" framing implied. New finding, not in the original report:
reaction-network does carry a real license (`pyproject.toml` declares
`license = "modified BSD"` with an OSI BSD classifier; GitHub's
auto-detector shows `NOASSERTION` only because the LICENSE file text is
a custom LBNL variant, not because no license exists) — corrected here
since license status matters for any future dependency or comparison
decision.

### 4.3 Two of the report's four "competitors" are one research
### group's related output, not independent lines of work

SyntMTE and Retro-Rank-In ([arXiv:2502.04289](https://arxiv.org/abs/2502.04289),
v1→v2 both February 2025) share 5 of the same authors (Prein, Pan,
Jehkul, Olivetti, Rupp) — the same TUM-affiliated group behind the
LLM-based synthesis planning paper in §4.1. Retro-Rank-In's paper is
real, but **no public code repository could be found** for it (GitHub
search and the lead author's own repository list both turn up nothing)
— stated as unverifiable, not as a confirmed absence, since "not found
by search" is a weaker claim than "confirmed not to exist."

Combined with §4.1, the report's apparent 5-way ranked field (gugen
plus 4 named competitors) is really **3 independent prior-art
lineages**, not 4:

| Lineage | Named item(s) | Repo status | Independent of the other lineages? |
|---|---|---|---|
| McDermott/Persson group (LBNL) | reaction-network | Active, ~1,057 commits, real BSD-style license | Yes |
| Prein/Rupp group (TUM) | LLM-based synthesis planning / SyntMTE (specialized model) and Retro-Rank-In | SyntMTE: active, last push 2025-09, no license. Retro-Rank-In: no public repo found | SyntMTE and Retro-Rank-In are not independent of *each other* (5 shared authors); both are independent of the other two rows |
| CederGroupHub (He/Huo et al.) | SynthesisSimilarity | Inactive since 2023-11-16, no license | Yes |

SynthesisSimilarity itself needed no correction: repo name confirmed
([github.com/CederGroupHub/SynthesisSimilarity](https://github.com/CederGroupHub/SynthesisSimilarity)),
last push 2023-11-16 (confirms "no updates since 2023"), no license
file (confirms "none set").

### 4.4 What this correction does and does not change

**Does not change**: gugen's own 6-axis score (86/100) is carried
forward exactly as reported. This pass verified the *facts* underneath
that score (every cited gugen-side number traces to a real,
currently-committed file in this repo) — it did not, and could not,
independently re-derive the score's own 6-axis *weighting*, which is
an inherent judgment call with no fetchable ground truth to check it
against. The score stays unchanged for two distinct reasons that
should not be conflated: nothing about gugen's own capability, evidence,
or limitations changed during this pass; and a smaller, better-
understood competitor field is if anything a *weaker* basis for a
competitive-ranking claim (fewer independent reference points), never
a reason to score gugen higher. Fixing a citation-counting error in the
competitive landscape is not evidence of anything about gugen itself,
and is not treated as such here.

**Does change**: how the competitive field should be described. 3
independent prior-art lineages, not 5 nominally-independent named
items; reaction-network's top score rests on one independent
experimental-validation source, not two; SyntMTE is best understood as
an update to §2's existing LLM-based-synthesis-planning entry, not a
new catalog item; Retro-Rank-In's code availability is unverifiable
rather than confirmed either way.
