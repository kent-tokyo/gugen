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
success-probability number, free-form giant candidate catalogs, CSV
drag-and-drop, a commercial-catalog mini-demo. (Manual free-form target/
candidate entry, still without a formula-string parser or CSV/JSON
upload, was added in a later pass — see
`docs/playground_free_form_input.md`.)

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

## Deployment (GitHub Pages, owner's explicit "gugen PlaygroundをGitHub
Pagesへ公開してください" instruction)

- **Production URL**: `https://kent-tokyo.github.io/gugen/` (a GitHub
  Pages *project* site — no custom domain, no CNAME). The workflow's
  own `page_url` output is the actual source of truth if it ever
  differs from this expected URL.
- **Build/deploy workflow**: `.github/workflows/playground-pages.yml`.
  Triggers on push to `main` (paths: `playground/**`, `src/**`,
  `Cargo.toml`, `Cargo.lock`, the workflow file itself),
  `workflow_dispatch`, and `pull_request` (same paths) — PR runs only
  execute the `build` job (wasm32 check, wrapper fmt/clippy/test,
  `wasm-pack build`, generated-asset presence check); the `deploy` job
  (`actions/deploy-pages`) only runs on `push`/`workflow_dispatch`,
  never on a PR. Official GitHub Pages artifact-deployment actions only
  (`actions/configure-pages`, `actions/upload-pages-artifact`,
  `actions/deploy-pages`) — no `gh-pages` branch, no committed `pkg/`
  build output anywhere in git history.
- **`wasm-pack` pinned at `0.15.0`** (`cargo install wasm-pack
  --version 0.15.0 --locked` in CI) — not auto-fetched latest; the
  exact version this MVP was built and manually verified against
  (`wasm-pack build` locally warned "newer version available: 0.15.0,
  you are using: 0.13.1" during this session's own manual build, so
  0.15.0 was confirmed to exist and work before pinning it in CI).
- **Artifact source**: `playground/web` only (the static site directory
  — HTML/CSS/JS plus the freshly-built `pkg/`), never the whole repo.
  `pkg/` is not a source of truth: it's regenerated from the Rust/JS
  source under `playground/` on every deploy, and stays gitignored.
- **Subpath-safe by construction**: every asset reference in
  `index.html`/`main.js` is relative (`src/styles.css`, `src/main.js`,
  `../pkg/gugen_playground_wasm.js`, `./examples.js`, `./render.js`) —
  no absolute `/src/...`/`/pkg/...` paths, no `<base href>` — so the
  same files work unmodified whether served from `/` (local dev) or
  `/gugen/` (the Pages project path). Confirmed by construction, not
  just assumed.
- **Content-Security-Policy**: a `<meta http-equiv="Content-Security-
  Policy">` tag in `index.html` (`default-src 'self'; script-src 'self'
  'wasm-unsafe-eval'; style-src 'self'; img-src 'self' data:;
  connect-src 'self'; font-src 'self'; object-src 'none'; base-uri
  'none'; frame-ancestors 'none'; form-action 'none'`) — `'wasm-unsafe-
  eval'` is the modern, narrower directive for WASM instantiation
  (not the broader `'unsafe-eval'`). Verified empirically before
  shipping, not assumed: served locally under this exact CSP, ran a
  full generate-a-plan cycle in a real Chrome tab, confirmed zero CSP
  violation messages in the console and a correct result rendered.
- **No analytics, no telemetry, no cookies, no external fonts/CDN-hosted
  JS or CSS, no runtime `fetch` beyond the page's own same-origin
  assets** — deliberately, since adding any of these would weaken the
  "no network calls, nothing sent anywhere" claim this playground's
  whole design rests on. Ordinary outbound links a user clicks
  themselves (the GitHub repo link, DOI citation links) are fine and
  unrelated to this.
- **Rollback**: revert the offending commit on `main`; the next push
  re-runs the same workflow, which rebuilds `playground/web` fresh from
  source and redeploys via the same official Pages artifact flow — no
  manual artifact surgery, no separate branch to reset.

## Status

**Live**: `https://kent-tokyo.github.io/gugen/`. PR #71 (workflow +
CSP) merged `main@bbb09d9`. The first push-triggered run's `build` job
passed cleanly but `Configure Pages` failed ("Get Pages site failed" —
Pages had never been enabled on the repo, confirmed independently via
`gh api repos/.../pages` returning 404). This was a real, anticipated
one-time owner action, not a workflow bug: the owner set Settings →
Pages → Build and deployment → Source → GitHub Actions; the same run
was then re-run (`gh run rerun`, no code changes) and both `build` and
`deploy` completed successfully.

Verified live, not just deployed: `curl -I` confirmed HTTP 200 and
correct MIME types (`application/wasm` for the `.wasm` binary,
`application/javascript` for JS); a real Chrome session at the live
URL ran the full BaTiO3 flow (accepted plans, rejected candidates —
the same real `DuplicatePlan` case as the local run, JSON view, copy
button) with zero console errors and zero network requests beyond the
page's own `kent-tokyo.github.io/gugen/*` assets (checked via devtools
Network tab, same as the local verification). A deliberately-broken
path request confirmed real 404 handling, not a catch-all 200.

Root quality gate green throughout, `playground/` confirmed excluded
from the published package. README/README_ja link added in a separate
PR after live verification, per the owner's own sequencing.
