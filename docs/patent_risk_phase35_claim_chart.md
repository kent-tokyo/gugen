# Preliminary patent-risk gate: Phase 35

Status: **implementation hold** (2026-09-06). This is a preliminary
screening record, not a legal opinion, FTO opinion, or claim-construction
conclusion. It records why the next roadmap phase is not being wired into
`Planner`/`SynthesisPlanningReport` yet.

## Scope

The intended product surface is a Rust library and CLI distributed through
crates.io and source repositories. That is broader than one jurisdiction.
This first chart covers the US documents found in the local screening pass;
JP, EP, and other jurisdiction-specific family members remain unreviewed.
No public release claim should be made from this document alone.

## Claim chart

| Reference | Relevant claim language, paraphrased | gugen overlap | Preliminary risk | Design-around / disposition |
|---|---|---|---|---|
| [US20240105285A1, claim 1](https://patents.google.com/patent/US20240105285A1/en) | Receive a user-specified molecule/reaction; query a reaction network containing molecule and reaction nodes; determine a synthesis route. | Existing `ReactionNetwork::search_routes` accepts caller-supplied precursors/target and traverses validated reaction hyperedges. It does not use molecule/reaction node objects, an online historical-data generator, or a performance-cost optimizer. | Medium preliminary similarity; claim scope and status require professional review. | Keep the core as an offline, caller-supplied hypergraph and bounded BFS. Do not add Planner-facing route selection, node-cost optimization, similarity thresholds, network updates, GUI, or historical-data ingestion in this phase. |
| [US20240105285A1, claims 18–19](https://patents.google.com/patent/US20240105285A1/en) | Method for receiving a route request, querying a reaction network, determining a route, generating the network from historical data, and assigning costs. | Phase 35 would add a target request and route output to the existing Planner, moving closer to this combination. | Medium-to-high preliminary similarity for the proposed Phase 35 shape. | Hold Phase 35. Preserve Phase 34 as validation/import tooling only; no route request-to-report wiring or cost model. |
| [US20210350881A1, claims 607–611](https://patents.google.com/patent/US20210350881A1/en) | Receive a recommended inorganic-material synthesis from a computer program, transfer reactants, and impose process conditions under computer control. | gugen emits plans and conditions but has no actuator, vessel, transfer, or computer-controlled process execution. | Low for current offline planning; high if autonomous-lab control is added. | Keep the explicit non-goal: no equipment/inventory-aware recommendation, actuator integration, or autonomous lab control. |
| [US11735292B2, claims 6, 11](https://patents.google.com/patent/US11735292B2/en) | Use graph search/ML to calculate a personalized preference score and display preferred pathways. | gugen has no personal-preference learning, feedback loop, equipment-aware score, or learned route preference. | Low for current code; high for preference-learning expansion. | Keep ranking deterministic and caller-supplied; defer Phase 37 and do not add feedback or personal preference state. |

## Decision

- Phase 35 implementation is **not cleared** by this screening.
- Phase 35 must remain unimplemented until the intended JP/US/EP scope is
  reviewed claim-by-claim by qualified counsel or a documented professional
  FTO review.
- The safe continuation is maintenance and non-routing work only: parser
  correctness, provenance reporting, deterministic validation, documentation,
  and tests for already-shipped boundaries.
- If a later review finds medium-or-higher risk in the current Phase 33
  public API, disable it behind a focused feature or remove it while keeping
  the lower-risk Phase 34 validation/import boundary.

## Phase 36 composition-retrieval screen

The first Phase 36 slice is deliberately narrower than neural or property-
aware similarity systems: caller-supplied compositions are normalized and
compared with a fixed L1 distance, with deterministic ID tie-breaking. It
does not infer structure, train a model, predict a property, recommend a
route, or build an approximate-nearest-neighbor index.

| Reference | Relevant concept | Similarity to the Phase 36 slice | Preliminary disposition |
|---|---|---|---|
| [US8918290B2](https://patents.google.com/patent/US8918290B2/en) | Compare composition data against a database and display equivalent materials ordered by a similarity factor. | Composition retrieval and ordered results are conceptually adjacent. | Keep the API as offline caller-supplied retrieval context with a fixed transparent distance; no classification, expert rules, property lookup, or equivalence claim. Counsel review remains required before broad distribution. |
| [EP4708304A1](https://patents.google.com/patent/EP4708304A1/en) | Fine-tuned neural embeddings and a vector database for chemical/material similarity search. | The current slice has no model, embeddings, training, vector database, or property information. | Do not add the neural/vector-database path in Phase 36. |
| [EP4708306A2](https://patents.google.com/patent/EP4708306A2/en) | AI-derived material-composition and structural feature similarity, including distance functions. | The current slice uses only normalized composition and a fixed L1 distance. | No AI feature learning, structure fingerprint, target-property relation, or adaptive weighting. Treat as not cleared for expansion. |

This is still a preliminary screen, not a clearance opinion. The Phase 36
retrieval fixture and leakage/Recall@k gate remain required before release
claims or a structure-aware extension.

## Phase 37 fixed-scorer screen

The first Phase 37 slice is a transparent boundary only: callers provide a
named feature vector and fixed weights, and gugen returns a deterministic
ordering while retaining the input features and scorer provenance. It does
not train, personalize, infer equipment preferences, consume execution
labels, or change Planner validity/safety gates.

| Reference | Relevant concept | Similarity to the fixed-scorer slice | Preliminary disposition |
|---|---|---|---|
| [US11735292B2, claims 6 and 11](https://patents.google.com/patent/US11735292B2/en) | Graph search or ML calculates a preference score, personalized to user skill/equipment preferences, and displays preferred pathways. | The slice returns a caller-weighted ordering, but has no graph search, training, user feedback, personalization, equipment/inventory data, or pathway generation. | Keep the scorer standalone and caller-supplied; do not add personalization, feedback, equipment-aware features, or Planner/report wiring. Counsel review remains required before broad distribution. |
| [US20240105285A1, claim 2](https://patents.google.com/patent/US20240105285A1/en) | Select a synthesis route by optimizing performance criteria such as steps, yield, or robustness. | The slice ranks arbitrary caller feature records and does not select a synthesis route or optimize a reaction network. | No route selection, network cost, yield/robustness prediction, or network-to-report integration. |

The Phase 37 boundary is therefore a low-risk engineering slice, not a
clearance determination. The fixed scorer now exposes only a margin-based
abstention/audit result; it still does not select reaction routes or learn
preferences. Calibration and any learned scorer remain gated on independent
data and jurisdiction-specific review.

## Phase 38 diversity-portfolio screen

The first Phase 38 slice is restricted to a generic, caller-supplied
portfolio boundary: fixed relevance values, fixed diversity features, a fixed
greedy marginal-diversity rule, and optional caller-supplied role labels. It
does not construct reaction routes, query a network, learn user preferences,
or connect to `Planner`.

| Reference | Relevant concept | Similarity to the Phase 38 slice | Preliminary disposition |
|---|---|---|---|
| [WO2020246908A1](https://patents.google.com/patent/WO2020246908A1/en) | Combine item relevance and diversity scores to produce a diversity-ranked set. | The generic relevance-plus-distance portfolio boundary is conceptually adjacent. | Keep a fixed, caller-supplied feature space and deterministic greedy rule; no user model, adaptive weighting, content presentation, or chemistry-specific route selection. |
| [US20210150453A1](https://patents.google.com/patent/US20210150453A1/en) | Select candidate groups using coverage, diversity, and connectivity objectives. | Optional role coverage is a narrower generic audit field, without opportunities, connectivity, or recommendation workflow. | Keep role labels caller-supplied and report uncovered roles; no graph connectivity, team/opportunity optimization, or Planner integration. |

This is a preliminary screen, not a clearance determination. Any
network-aware, learned, personalized, or recommendation-facing portfolio
feature remains gated on claim-by-claim review.

## Phase 38 Pareto-audit screen

The Pareto slice is narrower than portfolio optimization: it compares finite,
caller-supplied objective vectors and reports the non-dominated set. It does
not generate populations, evolve candidates, optimize allocations, or select
a single recommendation.

| Reference | Relevant concept | Similarity to the Pareto slice | Preliminary disposition |
|---|---|---|---|
| [US8219477B2](https://patents.google.com/patent/US8219477B2/en) | Multi-objective portfolio optimization using populations, dominance filtering, and an efficient frontier. | The non-dominance terminology is adjacent, but gugen has no portfolio allocations, evolutionary process, population archive, or investment decision. | Keep only a stateless caller-supplied frontier audit; no evolutionary search, allocation optimization, or recommendation output. |
| [US20140136460A1](https://patents.google.com/patent/US20140136460A1/en) | Multi-objective design selection and group selection from a Pareto frontier using thresholds. | The current API reports the frontier and does not apply thresholds or select a group. | Do not add threshold-based narrowing, domain-specific constraints, or Planner/network integration without review. |

This remains a preliminary screen, not a clearance determination. Pareto
frontier use in any route, equipment, user-preference, or commercial
recommendation workflow remains gated.

## Evidence boundary

The references above are public patent pages consulted on 2026-09-06. The
screening compares disclosed concepts at a high level; it does not determine
claim construction, prosecution history, expiration, enforceability,
territorial status, or infringement.
