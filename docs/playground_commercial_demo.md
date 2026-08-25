# gugen Playground — Commercial Catalog Mini-Demo

## Why this exists

The owner's own stated next-priority item for the Playground after
free-form input, described in the original playground proposal: show
that gugen's commercial-catalog matching (Phase 22) is a strictly
separate post-processing stage over an already-produced plan — price,
lead time, and supplier data never feed back into the scientific plan
ranking. The owner's own spec: fictional product data, compare
purity/package size/price/lead time/required purchase count/surplus
mass, and make the science/commerce separation visible on screen, not
just in documentation.

## Scope

**In**: a 4th result tab, "Commercial (demo)", populated automatically
after every plan generation (curated example or custom input) by
matching the generated plan's precursors against a small, fixed,
fictional catalog. Reuses gugen's own `assess_commercial_plans` and
`CommercialPrecursorCatalog::load_json` — zero new matching or ranking
logic written for the playground.

**Out, deliberately**: real supplier data or any live pricing source
(the owner's own spec calls for fictional data only); a catalog editor
or upload UI (the demo catalog is fixed, not user-supplied); ranking
policy switches (`CommercialRankingPolicy` stays at its `Balanced`
default); a custom domain (already deprioritized by the owner,
unrelated to this feature).

## Design

**Fixture reuse, not new data.** The demo catalog is
`tests/fixtures/commercial_catalog_sample.json`, transcribed verbatim
into `playground/web/src/catalog.js` (not imported — `tests/` isn't
part of the published crate or reachable from JS, same constraint
`examples.js` already documents for its own curated targets). Every
offer's own `notes` field already says "Fictional fixture data only.";
the two offers (`BaCO3`, `TiO2`) only fully match the BaTiO3 example's
cited route — other targets correctly show "no match", which is
rendered, not hidden or suppressed.

**One new WASM export, same shape as the existing one.**
`playground/wasm/Cargo.toml` now enables gugen's `commercial_catalog`
feature (confirmed to compile clean on `wasm32-unknown-unknown` before
writing any code against it — the feature pulls in the `csv` crate
unconditionally via `commercial_catalog = ["dep:csv"]`, even though
this playground only exercises the JSON-loading path, so the wasm32
build of `csv` itself was the real risk to check first). `lib.rs` adds
`assess_commercial(plans_json: &str, catalog_json: &str) -> String` —
string in, string out, same convention as `plan_synthesis`, same
`{"error": "..."}` shape on failure instead of a panic. Internally:
parse `plans_json` into `Vec<SynthesisPlan>` (already
`Serialize`/`Deserialize` via the `serde` feature this wrapper already
enables), `CommercialPrecursorCatalog::load_json` the catalog text,
enforce a new `MAX_CATALOG_OFFERS` (200) limit alongside the existing
`MAX_INPUT_BYTES` size check on both input strings, then call
`assess_commercial_plans` with `CommercialPlanningRequest::default()`
and `CommercialPlanningConfig::default()` (unrestricted matching — the
right default for a demo with only 2 fictional offers). This crate
remains the trust boundary, same as every other exported function.

**Rendering, `render.js`.** A new `renderCommercial(assessments,
container)` follows the file's existing `el()`/`textContent`-only
convention (no `innerHTML`). Every plan's assessment renders: a fixed
disclosure paragraph (fictional catalog, commercial ranking never
feeds back into scientific ranking — rendered in the panel itself, not
only in this doc); one card per matched combination showing total
price, lead time, minimum purity, and per-precursor purity/required
package count/surplus mass/price — directly the owner's own requested
comparison fields; and an explicit "No demo catalog offer for: ..."
line for any precursor the fixture catalog can't match, instead of
silently omitting it.

**Wiring, `main.js`.** `renderCommercialTab(plans)` is called from
`runPlan()` right after the existing accepted/rejected/JSON-Markdown
rendering, using the same `#commercial-panel` tab already added to
`index.html`'s existing `role="tablist"` — no changes needed to the
existing `showTab`/`wireTabs` keyboard logic, since it already
operates generically over every `.tab-button`/`.tab-panel` pair.

## Verification

- `cargo check --target wasm32-unknown-unknown --features
  "serde,commercial_catalog"` confirmed clean *before* writing any
  code against the feature — the real risk (does `csv` build for
  wasm32) was checked first, not assumed.
- `playground/wasm`: `cargo fmt -- --check`, `cargo clippy
  --all-targets -- -D warnings`, `cargo test` — 11 tests pass,
  including a real BaTiO3-plan-vs-sample-catalog match (confirms
  `every_precursor_has_a_match` is `true` for the cited route),
  malformed-plans and malformed-catalog structured-error cases (not
  panics), and an empty-plan-list case.
- `wasm-pack build --target web` succeeds; generated JS bindings
  confirmed to export both `plan_synthesis` and `assess_commercial`.
- Real browser run over a local static server: BaTiO3 generated,
  Commercial tab opened — the BaCO3+TiO2 route shows a fully-matched
  combination (107.50 USD total, 5-day lead time, 99.0% minimum
  purity, both precursors needing exactly 1 package with a computed
  surplus mass); the BaO-route plans correctly show "No demo catalog
  offer for: BaO" instead of a false match.
- Zero non-page-asset network requests (checked via the browser's own
  network log after the commercial call ran) — `assess_commercial`
  runs synchronously in-tab, no fetch anywhere.
- `axe-core` (same same-origin-injection technique as the prior
  accessibility pass), run with the Commercial tab open and populated:
  0 violations, 38 passes.
- Arrow-key tab navigation re-checked directly (`ArrowRight` from the
  JSON/Markdown tab moves focus to and selects the new Commercial tab)
  — the existing roving-tabindex code needed no changes to cover the
  4th tab correctly.
- `cargo package --list` from the repo root: `playground/` still does
  not appear (the existing `Cargo.toml` exclude rule is unaffected by
  this change).
- Root quality gate not re-run in full: this change touches zero files
  under `src/`, and `playground/wasm` has no `[workspace]` membership
  (invisible to root `cargo test --workspace`), so the prior
  `cargo fmt --all -- --check` / `cargo test --workspace` state is
  unaffected.

## What this does not claim

No claim about real supplier pricing, availability, or lead times —
every figure shown comes from a fixture file whose own `notes` field
says so. No claim that every example target has a commercial match —
only BaTiO3's cited route does today, by design (a bigger fictional
catalog is future scope, not required for the demo's own point). No
claim of a full manual screen-reader session — same tooling
limitation already disclosed in the accessibility-hardening and
free-form-input passes; the accessibility tree remains the closest
available proxy in this environment.

## Status

Implemented, tested, merged (`main@35e1a36`, PR #75), deployed, and
re-verified on the live production site
(`https://kent-tokyo.github.io/gugen/`) — a real BaTiO3 generation on
the deployed page shows the fully-matched BaCO3+TiO2 combination with
correct price/lead-time/purity/package-count numbers, console clean,
zero non-page-asset network requests. `playground/wasm` (new export +
`commercial_catalog` feature) and `playground/web` only — no changes
to `src/`.
