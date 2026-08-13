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
  target, precursors, operations/conditions, and balanced equations. A
  candidate source of `CuratedLiteratureRecord` evidence and of validation
  fixtures for §21.3/§22 — **but its license has not yet been checked** and
  must not be bundled or treated as curated ground truth until it is
  (tracked in `tasks/todo.md`; this is a stop-and-report item per AGENTS.md
  §28 if it blocks Phase 8, not Phase 0).

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
