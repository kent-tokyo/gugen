# gugen (具現)

[![Crates.io](https://img.shields.io/crates/v/gugen.svg)](https://crates.io/crates/gugen)
[![docs.rs](https://img.shields.io/docsrs/gugen)](https://docs.rs/gugen)
[![CI](https://github.com/kent-tokyo/gugen/actions/workflows/ci.yml/badge.svg)](https://github.com/kent-tokyo/gugen/actions/workflows/ci.yml)
[![License](https://img.shields.io/crates/l/gugen.svg)](#license)

**English** | [日本語](README_ja.md)

Explainable materials synthesis and process planning, in Rust.

Given a target inorganic composition (and optionally a target structure),
gugen returns candidate precursor sets, balanced reactions, and
solid-state process plans — each with its evidence, assumptions, and
unresolved conditions kept explicit and machine-readable. It does not
predict experimental success.

> **Status: v0.4.2 published.**
> [crates.io](https://crates.io/crates/gugen) /
> [docs.rs](https://docs.rs/gugen) / [v0.4.2 release](https://github.com/kent-tokyo/gugen/releases/tag/v0.4.2).
> v0.4.2 adds an optional, off-by-default Commercial Precursor Catalog
> feature (`commercial_catalog`): matches an existing synthesis plan's
> precursors against a caller-supplied catalog of commercial offers, as a
> post-planning stage that never affects the plan's score, confidence,
> reaction, or process steps -- see "Commercial Precursor Catalog" below
> and [`CHANGELOG.md`](CHANGELOG.md) for the full list and known
> limitations.

## What gugen does and doesn't guarantee

gugen's output is a set of candidate plans, not a validated SOP. It does
not guarantee: experimental success, target-phase formation, a single
phase product, reaction completion at a stated temperature, high yield,
safe executability, patentability, or industrial scalability. A ranking
score is an ordinal, explainable measure for sorting candidates against
each other — never a success probability. See
[`docs/scientific_scope.md`](docs/scientific_scope.md) for the full list
of what's in and out of scope for v0.1, and
[`docs/evidence_model.md`](docs/evidence_model.md) for how evidence,
assumptions, and unresolved conditions are kept separate.

## What works today

### Reaction balancing

Exact-rational Gauss-Jordan elimination over the element × species
matrix — never floating-point approximation (see
[`docs/architecture.md`](docs/architecture.md)). The full runnable source
for this example is [`examples/balance_batio3.rs`](examples/balance_batio3.rs).

```rust
use gugen::{balance, Composition, Element};

let ba = Element::new("Ba")?;
let ti = Element::new("Ti")?;
let o = Element::new("O")?;

let bao = Composition::new([(ba, 1.0), (o, 1.0)])?;
let tio2 = Composition::new([(ti, 1.0), (o, 2.0)])?;
let batio3 = Composition::new([(ba, 1.0), (ti, 1.0), (o, 3.0)])?;

let reactions = balance(&[bao, tio2], &[batio3])?;
```

Output (`cargo run --example balance_batio3`):

```
1 Ba:1, O:1 + 1 O:2, Ti:1 -> 1 Ba:1, O:3, Ti:1
```

### Bounded precursor-set search

`search_precursor_sets` runs a deterministic, budget-bounded search over
a precursor catalog, returning both accepted precursor sets (each with
its balanced reaction) and every rejected candidate with a reason code —
never just the winners. See [`src/precursor.rs`](src/precursor.rs)'s
tests for worked examples.

### Solid-state process templates

`conventional_solid_state_template` turns an accepted precursor set into a
weigh/mix/grind/form/heat/cool/characterize step sequence, each step marked
`Required`/`Recommended`/`Optional`/`Unresolved`. It does not apply the same
template to every material: a route that releases a byproduct (e.g. a
carbonate route releasing CO₂) gets an extra calcination step that an
oxide-only route to the same target does not. Temperature, duration, ramp
rate, and atmosphere are left unresolved (`None`) rather than guessed —
this worked example uses `Planner::offline_minimal`, which wires in no
provider at all. A caller can configure a `ProcessEvidenceProvider` (e.g.
`InMemoryLiteratureConditionProvider`) to resolve some of these fields from
cited literature; see `docs/integration.md`.

### Plan scoring and confidence

`score_plan` computes a `PlanScoreBreakdown` and a `ConfidenceAssessment`
per plan — never a single collapsed number. Missing thermodynamic data is
excluded from the score rather than treated as failure; a plan with no
evidence scores lower than one with evidence. `total_ranking_score` is an
ordinal, explainable score for comparing candidates, never a success
probability — `thermodynamic_support` stays `None` regardless of whether a
`ThermodynamicProvider` is configured (a resolved reaction energy becomes
evidence, never a numeric score — AGENTS.md §4.3), so `total_ranking_score`
is honestly driven by only one real signal (`process_simplicity`, now
computed per-route-family since Phase 12 added a second route family — see
below); see
[`PlanScoreBreakdown`'s doc comment](src/score.rs) for the full breakdown
of what's currently constant versus load-bearing. Every plan currently sets `manual_review_required:
true`, since gugen has no hazard/safety data source wired in yet.

### Commercial Precursor Catalog

Optional, off-by-default `commercial_catalog` feature: matches an existing
`SynthesisPlan`'s precursors against a caller-supplied catalog of
commercial offers (price, purity, package size, supplier), as a
post-planning stage that never touches the plan's score, confidence,
reaction, or process steps. Matches on a canonical, scale-invariant
composition ratio via exact rational arithmetic (`Fe2O3` matches `Fe4O6`;
hydrate vs. anhydrous and different compounds stay distinct; no alias or
substitute-precursor inference); CSV/JSON catalog import with a
structured accepted/rejected load report; stoichiometric quantity
calculation, purity-adjusted purchase mass, package-count rounding,
currency-safe checked-arithmetic cost totals; a bounded search over
complete purchasing combinations. See
[`docs/commercial_precursor_catalog.md`](docs/commercial_precursor_catalog.md)
for the full CSV/JSON schema, exact-match policy, and quantity-calculation
rules, and `examples/commercial_catalog_assessment.rs` for a runnable
worked example (`cargo run --example commercial_catalog_assessment
--features commercial_catalog`). No real catalog data ships with gugen;
gugen does not certify commercial data (prices/availability are supplied
estimates, not guarantees).

### CLI

```
$ gugen balance reaction.json
```

`reaction.json`:

```json
{
  "reactants": [
    {"Ba": 1.0, "O": 1.0},
    {"Ti": 1.0, "O": 2.0}
  ],
  "products": [
    {"Ba": 1.0, "Ti": 1.0, "O": 3.0}
  ]
}
```

Output (`serde_json::to_string_pretty`, one field per line):

```json
[
  {
    "reactants": [
      {
        "composition": {
          "Ba": 1.0,
          "O": 1.0
        },
        "coefficient": 1
      },
      {
        "composition": {
          "O": 2.0,
          "Ti": 1.0
        },
        "coefficient": 1
      }
    ],
    "products": [
      {
        "composition": {
          "Ba": 1.0,
          "O": 3.0,
          "Ti": 1.0
        },
        "coefficient": 1
      }
    ]
  }
]
```

Build the CLI with `cargo build --features serde,clap --bin gugen`.
Subcommands (AGENTS.md §19):

```
gugen balance reaction.json
gugen plan target.json --catalog precursors.json [--output report.json] [--format json|markdown]
gugen explain report.json --plan plan-001
gugen validate-target target.json
gugen doctor
gugen batch input.json --catalog precursors.json [--output out.json]
```

`target.json`/`precursors.json`/`input.json` reuse gugen's own public JSON
shapes (`TargetSpecification`, a JSON array of `PrecursorCandidate`, and a
JSON array of `TargetSpecification` respectively) rather than a separate
CLI-specific format. `gugen batch` plans every target independently — one
target's failure doesn't abort the rest.

### Worked example: a full synthesis plan

```
$ gugen plan target.json --catalog precursors.json --format markdown
```

`target.json` (BaTiO3) and `precursors.json` (the standard BaCO3 + TiO2
solid-state route to it):

```json
{
  "composition": {"Ba": 1.0, "Ti": 1.0, "O": 3.0},
  "structure": null,
  "desired_phase": null,
  "constraints": {"forbidden_elements": []}
}
```

```json
[
  {"id": "BaCO3", "composition": {"Ba": 1.0, "C": 1.0, "O": 3.0}, "availability": null},
  {"id": "TiO2", "composition": {"Ti": 1.0, "O": 2.0}, "availability": null}
]
```

Output (real, unedited `gugen plan` output; this is also
`tests/fixtures/batio3_report.md`'s golden snapshot, minus its score-
breakdown/confidence/assumptions/unresolved-conditions sections and
rejected-candidate section for length — all are in the full file):

```markdown
# Synthesis Planning Report (schema v1)

**Target:** Ba:1, O:3, Ti:1

**Applicability:** PartiallyInDomain -- formula-only target, no structure provided (AGENTS.md §16's own example for this level)

## Plan plan-1677d44bfe4dbdc2 (score 0.062)

- Target: Ba:1, O:3, Ti:1
- Route family: ConventionalSolidState
- Reaction: 1x(Ba:1, C:1, O:3) + 1x(O:2, Ti:1) -> 1x(Ba:1, O:3, Ti:1) + 1x(C:1, O:2)
- Manual review required: true
- Applicability: PartiallyInDomain -- formula-only target, no structure provided (AGENTS.md §16's own example for this level)

### Steps

- [Required] Weigh: BaCO3 x1, TiO2 x1
- [Required] Mix (DryMixing)
- [Required] Grind (MortarAndPestle), duration=unresolved
- [Optional] Form (UniaxialPressing), pressure=unresolved
- [Required] Heat (Calcination): temperature=unresolved, duration=unresolved, atmosphere=unresolved, ramp=unresolved
- [Recommended] Grind (MortarAndPestle), duration=unresolved
- [Required] Heat (Sintering): temperature=unresolved, duration=unresolved, atmosphere=unresolved, ramp=unresolved
- [Required] Cool (FurnaceCooling)
- [Recommended] Characterize (Xrd): verify target-phase formation

### Evidence

- [Weak/ProcessTemplate] weigh/mix/grind/form are the fixed opening sequence of the v0.1 conventional solid-state template
- [Strong/StoichiometricBalance] balanced reaction releases a byproduct beyond the target, indicating a decomposition (calcination) step is needed before the final firing step
- [Weak/ProcessTemplate] AGENTS.md §11's template outline places a regrind between calcination and final firing

### Warnings

- [Caution] temperature, duration, ramp rate, and atmosphere are unresolved for every heating step: gugen has no thermodynamic or literature evidence provider wired in yet (AGENTS.md §4.1)
- [Severe] no hazard or safety data source is wired in yet: safety_penalty carries no real safety information, and this is not a safety clearance (AGENTS.md §15 "unknown hazardを安全と扱わない")

## Plan plan-ee311be9350b7d8b (score 0.062)

- Target: Ba:1, O:3, Ti:1
- Route family: Mechanochemical
- Reaction: 1x(Ba:1, C:1, O:3) + 1x(O:2, Ti:1) -> 1x(Ba:1, O:3, Ti:1) + 1x(C:1, O:2)
- Manual review required: true
- Applicability: PartiallyInDomain -- formula-only target, no structure provided (AGENTS.md §16's own example for this level)

### Steps

- [Required] Weigh: BaCO3 x1, TiO2 x1
- [Required] Grind (BallMilling), duration=unresolved
- [Optional] Form (UniaxialPressing), pressure=unresolved
- [Required] Heat (Annealing): temperature=unresolved, duration=unresolved, atmosphere=unresolved, ramp=unresolved
- [Required] Cool (FurnaceCooling)
- [Recommended] Characterize (Xrd): verify target-phase formation

### Evidence

- [Weak/ProcessTemplate] weigh, then a single high-energy ball-milling step (which performs mixing and grinding together, unlike the separate Mix/Grind steps of the conventional solid-state template) is the fixed opening sequence of the mechanochemical route template
- [Moderate/StoichiometricBalance] balanced reaction releases a byproduct beyond the target; ball milling alone is not reliably sufficient to complete such a reaction at room temperature, so a post-milling anneal is included -- the cited review reports specific byproduct-releasing compounds (e.g. gamma-Al2O3, ZrO2) that formed only after heating the as-milled powder

### Warnings

- [Caution] grinding duration, forming pressure, and (if present) heating temperature/duration/atmosphere/ramp are unresolved: gugen has no thermodynamic or literature evidence provider wired in yet (AGENTS.md §4.1)
- [Severe] no hazard or safety data source is wired in yet: safety_penalty carries no real safety information, and this is not a safety clearance (AGENTS.md §15 "unknown hazardを安全と扱わない")
```

Note the two plans: since Phase 12, every accepted precursor set is offered
under *every* applicable route family (currently 2), not just one — gugen
has no route-suitability classifier to prefer one for a given target, so
both are always shown, ranked independently (AGENTS.md §13). Note also the
calcination/regrind step in the first plan: it's there because the balanced
reaction releases CO2 (see the Evidence entry), not because every plan gets
the same template — a carbonate-free route to the same target wouldn't have
it, and the mechanochemical plan's own post-milling anneal is conditioned
the same way. The full report also carries a per-plan score breakdown,
confidence assessment (five independent sub-scores, not one blended
number), assumptions list, and every rejected single-precursor candidate
with its reason code.

## Ecosystem

```
                       chematic-crystal
               periodic structure foundation
                             │
                ┌────────────┴────────────┐
                │                         │
             mikiwame                  gugen
     explainable diagnostics    synthesis/process planning
```

gugen depends on `chematic-crystal` for periodic structure types once
that crate is published (not yet, as of 2026-08-14 — see
[`docs/integration.md`](docs/integration.md)); until then it builds
against a minimal trait boundary it owns itself. `mikiwame` is published
and integrated as an optional, off-by-default `mikiwame` feature
(`cargo build --features mikiwame`) that maps its structural diagnostics
onto gugen's own warnings/confidence — not yet wired into `Planner::plan`,
since that still needs `chematic-crystal`-shaped structure data gugen
doesn't have. gugen never depends on `renkin` (molecular retrosynthesis)
and does not reuse its algorithms — gugen is a materials-domain sibling,
not a port.

## Development

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo test --workspace --no-default-features
cargo test --no-default-features --features mikiwame
cargo test --no-default-features --features commercial_catalog
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
cargo build --features serde,clap --bin gugen
cargo check --target wasm32-unknown-unknown
cargo check --target wasm32-unknown-unknown --features mikiwame
cargo audit
```

Architecture and design decisions: [`docs/`](docs/).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

日本語版: [README_ja.md](README_ja.md)
