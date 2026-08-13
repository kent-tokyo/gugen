# gugen (具現)

Explainable materials synthesis and process planning, in Rust.

Given a target inorganic composition (and optionally a target structure),
gugen returns candidate precursor sets, balanced reactions, and
solid-state process plans — each with its evidence, assumptions, and
unresolved conditions kept explicit and machine-readable. It does not
predict experimental success.

> **Status: early development, v0.1 in progress.** Phases 0-6 of 9 are
> done (architecture, foundation types, exact reaction balancing, bounded
> precursor-set search, solid-state process templates, plan scoring and
> confidence, and the end-to-end `Planner`). An optional `mikiwame`
> feature adapts structural diagnostics; the `chematic-crystal` adapter
> remains blocked on that crate's publication. The CLI doesn't exist yet.
> Not published, not merged to `main`, not ready for use. See
> [`tasks/todo.md`](tasks/todo.md) for exact phase-by-phase status and
> [the draft PR](https://github.com/kent-tokyo/gugen/pull/1) for what's
> under review.

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
1 Ba1O1 + 1 O2Ti1 -> 1 Ba1O3Ti1
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
gugen has no thermodynamic or literature evidence provider wired in yet.

### Plan scoring and confidence

`score_plan` computes a `PlanScoreBreakdown` and a `ConfidenceAssessment`
per plan — never a single collapsed number. Missing thermodynamic data is
excluded from the score rather than treated as failure; a plan with no
evidence scores lower than one with evidence. `total_ranking_score` is an
ordinal, explainable score for comparing candidates, never a success
probability — and in v0.1, with one route family and no thermodynamic
provider, it's honestly driven by only one real signal
(`process_simplicity`); see [`PlanScoreBreakdown`'s doc
comment](src/score.rs) for the full breakdown of what's currently constant
versus load-bearing. Every plan currently sets `manual_review_required:
true`, since gugen has no hazard/safety data source wired in yet.

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

Output:

```json
[
  {
    "reactants": [
      { "composition": { "Ba": 1.0, "O": 1.0 }, "coefficient": 1 },
      { "composition": { "O": 2.0, "Ti": 1.0 }, "coefficient": 1 }
    ],
    "products": [
      { "composition": { "Ba": 1.0, "O": 3.0, "Ti": 1.0 }, "coefficient": 1 }
    ]
  }
]
```

Build the CLI with `cargo build --features serde,clap --bin gugen`. Only
`gugen balance` exists so far; `plan`, `explain`, `validate-target`,
`doctor`, and `batch` are Phase 7 work.

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
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
cargo audit
```

Full spec: [`AGENTS.md`](AGENTS.md). Architecture and design decisions:
[`docs/`](docs/). Phase-by-phase progress: [`tasks/todo.md`](tasks/todo.md).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.

日本語版: [README_ja.md](README_ja.md)
