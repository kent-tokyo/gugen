# gugen Playground — Browser WASM Demo (MVP)

## Why this exists

The owner's own diagnosis: gugen is a "the flow is the pitch" tool — the
value (explainable candidates, real rejection reasons, unresolved
conditions instead of a success-probability guess) doesn't come across
from a README or an API list, only from actually watching a target go
through pick → propose → reject → unresolved. A browser playground
makes that flow tryable with zero setup, without turning gugen into a
different kind of product: the science stays exactly as gugen produces
it, this only changes how it's shown.

## Scope

**In (MVP)**: a static, fully client-side page. 5 curated, cited
example targets (no free-form input yet). Renders `accepted` plans,
`rejected_candidates` with real reasons, process steps with resolved/
unresolved conditions, evidence/assumptions, and a JSON/Markdown view
with copy. Zero network calls after the page loads.

**Out (owner's explicit list, all deferred to a later stage)**: OQMD or
any external API, real supplier catalogs, execution-record writes, user
accounts, a server, an LLM, Phase 30's diagnostic tie-break machinery, a
success-probability number, free-form giant candidate catalogs,
free-form target/catalog input, CSV drag-and-drop, a commercial-catalog
mini-demo.

## Research findings that shaped this build

1. **wasm32 already worked, unmodified.** `README.md`'s own
   "Development" checklist already lists `cargo check --target
   wasm32-unknown-unknown` — confirmed empirically before writing any
   code: `cargo check --target wasm32-unknown-unknown --features serde`
   compiled gugen core clean, no changes needed anywhere under `src/`.
   (Not CI-enforced — no `wasm` job exists in `.github/workflows/` —
   manual/local only; noted, not a blocker.)
2. **The data model already carries everything the UI needed.**
   `SynthesisPlanningReport` (`src/report.rs`) already has `plans`,
   `not_recommended`, `rejected_candidates: Vec<RejectedCandidate>`
   (`{precursors, reason_codes, explanation}`), `unresolved`,
   `warnings`; every `SynthesisPlan` already carries `evidence`,
   `assumptions`, `unresolved`, `manual_review_required`, `steps`,
   `score`, `confidence` — all already serde-serializable. The
   playground needed **zero new backend logic**.
3. **Real, citable examples already existed.** `tests/validation.rs`'s
   `fixtures()` (5 curated, DOI-cited literature routes) mapped
   directly onto the owner's own 4 requested example categories:
   BaTiO3 (headline, multi-route), CaO (simplest), LiFePO4 (byproduct
   route), MgAl2O4 (second multi-route case). Transcribed into
   `playground/web/src/examples.js` rather than reinvented.
4. **A gap the owner's sketch didn't mention**: `Cargo.toml` has no
   `include` allowlist, so `cargo package`/`cargo publish` sweeps in
   every git-tracked file by default. `playground/` (a second Cargo
   crate, JS source, build output) had to be added to the existing
   `exclude` list (same precedent as `benchmarks/data/*`) or it would
   have shipped inside the published `gugen` crate for no reason.
   Verified via `cargo package --list` before and after the fix.

## What was built

```
playground/
├── wasm/
│   ├── Cargo.toml       # publish = false, crate-type = ["cdylib"]
│   └── src/lib.rs       # the entire Rust/JS boundary -- one exported fn
└── web/
    ├── index.html
    ├── src/{main.js, examples.js, render.js, styles.css}
    └── pkg/             # wasm-pack build output, gitignored
```

**`playground/wasm/src/lib.rs`**: one exported function,
`plan_synthesis(input_json: &str) -> String` — JSON string in, JSON
string out, always (never a panic across the boundary:
`console_error_panic_hook` is a defense-in-depth backstop, not a
substitute for the explicit validation below). Internally: enforces
every safety limit on parsed input (this crate is the real trust
boundary, not the JS UI — anyone can call the exported wasm function
directly from devtools), builds `Composition`/`PrecursorCandidate`s via
gugen's own existing constructors, clamps the caller's requested
`SearchBudget` to a fixed ceiling (never trusts a caller-larger value),
then `InMemoryPrecursorCatalog::new` + `Planner::builder(catalog,
config).build().plan(&target, timestamp)` — zero optional providers,
matching the offline-only MVP scope.

**Safety limits enforced in Rust**, checked before any call into
gugen: target element count ≤ 12, candidate count ≤ 60, formula/id
string length ≤ 40 chars, input JSON size ≤ 256 KB,
`max_precursors_per_plan` clamped to ≤ 6, `max_precursor_sets` clamped
to ≤ 50,000, `max_plans_returned` clamped to ≤ 50. No network access
anywhere in `playground/wasm` or `playground/web` — no `fetch`, no
auto-loaded URLs, nothing sent anywhere; verified directly (browser
devtools Network tab showed only the page's own static assets, plus
unrelated browser-extension requests, after a full example run).

**New, separate `RouteError`-style error type was *not* needed here**
(unlike Phase 31 PR 1's `RouteError`) — this wrapper returns plain
`Result<String, String>` internally and always serializes to
`{"error": "..."}` on failure, since nothing here needs to cross back
into gugen's own `Result<T, GugenError>` machinery.

**`playground/web/`**: no bundler/framework — `wasm-pack build --target
web --out-dir ../web/pkg` emits a plain ES module that `index.html`
imports directly via `<script type="module">`. 3-step guided flow (pick
a curated example → generate → compare via 3 tabs: accepted plans,
rejected candidates, JSON/Markdown). Every process step renders every
field explicitly, with an unresolved value shown as an explicit
"unresolved" label — never blank — matching gugen's own
abstention-not-a-guess convention. No score is rendered large or alone;
score/confidence sit inline in the same card as evidence/assumptions/
unresolved.

## Verification

1. `cargo check --target wasm32-unknown-unknown --features serde` on
   root gugen: clean (confirmed before writing any playground code).
2. `cd playground/wasm && cargo fmt -- --check && cargo clippy
   --all-targets -- -D warnings && cargo test`: 7 unit tests, all
   passing natively (wasm-bindgen's `&str`/`String`-only boundary
   compiles and runs fine on the host target, no wasm32 cross-compile
   needed for the test suite itself) — a valid BaTiO3 request recovers
   the cited route; malformed JSON, an invalid element symbol, and
   every individual safety-limit violation all return a structured
   `{"error": ...}`, never panic; an oversized requested `SearchBudget`
   is silently clamped, not rejected or trusted.
3. `wasm-pack build --target web --out-dir ../web/pkg`: succeeded,
   produced a working ES module + `.wasm` binary.
4. Served `playground/web/` locally and drove it in a real Chrome tab
   (this repo's own "verify in-browser, not just it compiled"
   convention): BaTiO3 generated a real plan (Mechanochemical route via
   the BaO alternative precursor), the process-step table rendered
   every field including explicit "unresolved" labels for temperature/
   duration/atmosphere, the Rejected Candidates tab showed a real
   `DuplicatePlan` rejection (the search found the BaO+TiO2 combination
   twice via different paths, correctly deduped one — Phase 30.6's own
   dedup fix from earlier this session, visible live), the JSON view
   rendered the full raw report. Switched to CaO and regenerated
   successfully. Console showed zero errors attributable to the page's
   own code (3 unrelated `chrome-extension://` messages present, not
   from this site). Network tab showed only `localhost:8787`'s own
   static assets (HTML/CSS/JS/wasm) plus an unrelated browser
   extension's own requests — nothing external, confirming the
   "nothing sent anywhere" claim directly rather than just asserting it.
5. `cargo package --list` from the repo root confirmed **no** files
   under `playground/` are swept into the published crate, after the
   `Cargo.toml` `exclude` addition.
6. Root quality gate unaffected: `cargo fmt --all -- --check`, `cargo
   clippy --workspace --all-targets --all-features -- -D warnings`,
   `cargo test --workspace --all-features` / `--no-default-features`
   (zero regressions), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace
   --all-features --no-deps`, `cargo semver-checks check-release
   --baseline-version 0.6.0 --all-features` (no update required — this
   build touches zero files under `src/`).

## What this does not claim

No claim about search/ranking quality — this is a presentation layer
over data gugen's existing `Planner`/`search_precursor_sets` already
produce, with zero changes to either. No claim about production
readiness of a public deployment (GitHub Pages hosting, custom domain,
CDN caching, etc.) — that's a deploy-time decision, not part of this
build. No claim that the 5 examples are exhaustive coverage of gugen's
capabilities — they're the same 5 curated literature fixtures this
crate already uses for its own `every_literature_route_is_recovered_exactly`
test, chosen because they're already verified and cited, not because
they're the only interesting cases.

## Status

Implemented, tested (native unit tests + a real in-browser run), root
quality gate green, `playground/` confirmed excluded from the published
package. README/README_ja not touched — the owner asked for the link
only once this is live, as a separate, later step.
