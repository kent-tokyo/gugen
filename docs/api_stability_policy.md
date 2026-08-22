# API Stability Policy (Phase 23D)

States what this crate's public API and report schema actually guarantee
today, and what they don't — written because both had grown organically
without a stated policy, and at least one of the gaps below (`SCHEMA_VERSION`)
had already caused real confusion once (see the v0.4.0 CHANGELOG entry it
quotes below).

## `SCHEMA_VERSION`

`SCHEMA_VERSION` (`src/report.rs`, currently `2`) is a plain `u32` embedded
in every `SynthesisPlanningReport`/`PlanningProvenance`. It is bumped only
when a change to `SynthesisPlanningReport`'s own shape is judged significant
enough to flag — it is **not** bumped for every report-shape change, and
this has already happened: it stayed `1` through v0.1.0, v0.2.0, and v0.3.0
even though the report gained fields in that span (e.g.
`route_suitability`/`not_recommended` in v0.3.0). It moved `1 -> 2` only in
v0.4.0, when `SynthesisPlan` gained `literature_evidence`. **`SCHEMA_VERSION`
does not cleanly delimit one report shape from another** — a consumer with
its own strict schema (a hand-written validator, or a `serde` struct with
`#[serde(deny_unknown_fields)]`) should not key strict-field-set validation
on this number. A consumer that already tolerates unknown fields needs no
special handling; one that doesn't needs to re-check its expected shape on
every gugen upgrade, not just when this number changes.

## `#[non_exhaustive]`

Applied, going forward, only to types whose own doc comment already states
a growth expectation — never added preemptively to a type with no stated
reason to grow. As of v0.5.0 (Phase 23D), that's: `SuitabilityVerdict`,
`RouteRecommendation` (`route_suitability.rs`, Phase 15A's original
application), and `RouteFamily`, `InertGas`, `MixingMethod`,
`GrindingMethod`, `FormingMethod` (`process.rs`, Phase 23D). One earlier
exception predates this stated policy: `CommercialCatalogError`
(`commercial_catalog`, Phase 22) is `#[non_exhaustive]` without its own
growth-rationale doc comment — left as-is rather than retroactively
un-marked (removing `#[non_exhaustive]` is itself a breaking change with
no real benefit), but not a model to repeat. For an enum, `#[non_exhaustive]`
means: code outside
this crate must include a wildcard arm (`_ => ...`) when matching on it, and
should not assume the variant list is complete. It does **not** block
constructing an existing named variant from outside this crate, and it says
nothing about structs — a `#[non_exhaustive]` struct still allows external
struct-literal construction unless its fields are also private (the
mechanism `BalancedReaction`/`ReactionSpecies` use as of v0.5.0, Phase 23A;
see the CHANGELOG). No struct in this crate is `#[non_exhaustive]` today.

No enum without a stated growth expectation in its own doc comment is
guaranteed to *stay* closed forever — `#[non_exhaustive]` is added to a type
only when there's a real reason to expect growth, following the same
"don't add speculatively" discipline `RouteFamily`'s own doc comment states
for its variants. A type not marked `#[non_exhaustive]` today may still gain
the attribute in a future breaking release if a real need to grow it
surfaces; this policy does not promise otherwise.

## `cargo semver-checks`

Run manually per release (`cargo semver-checks --all-features` against the
last published version) — not automated in CI (`ci.yml`/`publish.yml`).
Treat a clean `cargo semver-checks` run as necessary, not sufficient,
evidence of a non-breaking release. It has at least one confirmed blind
spot: a bare return-type shape change (e.g. `Option<T>` widening to
`Result<Option<T>>` while an existing `Ok`/`Some`/`None` caller keeps
compiling but a new `Err` arm is needed) was not flagged by any of its
lints when this happened in v0.4.0 (`balanced_reaction_delta_ev_per_atom`/
`decomposition_margin_ev_per_atom` gaining `Result`, see that CHANGELOG
entry). Every release still runs the full quality gate matrix (fmt,
clippy `-D warnings`, tests under both `--all-features` and
`--no-default-features`, doc build `-D warnings`, `cargo package --list`)
alongside `semver-checks`, and any breaking change found is disclosed by
hand in `CHANGELOG.md` regardless of whether the tool caught it.

## Constructor deprecation

`#[deprecated]` (first used in v0.5.0, Phase 23B, on `Planner`'s 5 named
constructors in favor of `Planner::builder(...)`) marks a function as
superseded without removing it. No removal timeline is promised by adding
`#[deprecated]` alone — a deprecated item stays available until a specific
future release states otherwise in its own `CHANGELOG.md` entry.
