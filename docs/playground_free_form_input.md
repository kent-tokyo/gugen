# gugen Playground — Free-Form Target/Candidate Input

## Why this exists

The owner's own stated next-priority item for the Playground, after the
accessibility hardening pass: free-form input, ahead of a
commercial-catalog mini-demo or a custom domain. The 5 curated examples
remain the primary, default path — this adds a second, clearly
secondary way to try gugen against a target of the user's own choosing,
without touching anything about the curated-example flow.

## Scope

**In**: manual entry of a target composition (element/amount rows) and
one or more precursor candidates (id + element/amount rows), reusing
the exact same `plan_synthesis` request shape and safety limits the
curated-example flow already uses. Client-side validation mirroring
`playground/wasm/src/lib.rs`'s own limits, for immediate feedback —
the WASM wrapper remains the real, authoritative trust boundary
regardless.

**Out, deliberately**: a formula-string parser (typing "BaTiO3" and
having it expand to element/amount pairs). Checked directly before
designing anything: gugen has **no formula-string parser anywhere in
its public API** — `Composition::new` takes explicit `(Element, f64)`
pairs; `src/materials_project_adapter.rs`'s own doc comment says so
outright ("No formula parser exists in gugen"); `commercial_catalog.rs`
has one, but it's private and feature-gated behind `commercial_catalog`
(which this wrapper doesn't enable). Building one in JS for this PR
would mean inventing new chemistry-parsing logic outside gugen's own
core, risking disagreement with its semantics — real new scope, not a
UI change. Element/amount rows avoid that entirely and needed **zero
new WASM/Rust code**, matching every prior playground phase. Also out,
per the original MVP's own "Out" list, still deferred: CSV upload,
JSON paste, ranking-policy switches, a commercial-catalog demo.

## Design

New file `playground/web/src/custom.js`. A native `<details>`/
`<summary>` disclosure ("Or, build a custom target") right after the
curated-example citation, collapsed by default — no hand-built ARIA
disclosure widget needed; `<details>` already has correct keyboard
support (Enter/Space toggles) and screen-reader semantics (an implicit
expand/collapse state) built into the browser.

Inside: dynamic add/remove rows for target elements (`symbol` +
`amount` text/number inputs) and for precursor candidates (an `id`
input plus its own nested element rows), each row's inputs carrying an
explicit `aria-label` (`"Element symbol"`/`"Amount"`/`"Precursor id"`)
— placeholder text alone is never a substitute for an accessible name.
A "Generate from custom input" button, separate from the curated
examples' own "Generate synthesis plan" button, feeding the exact same
result tabs below.

`readCustomRequest()` collects the current row state into `{target_elements,
candidates}` — the identical shape `runPlan()` already builds from a
curated example — and validates: at least one target element, at most
12; at least one candidate, at most 60; symbol/id length ≤ 40 characters;
no duplicate element symbols within a target or a single candidate; no
duplicate candidate ids; every element needs a positive, finite amount.
Any validation failure throws with a plain-English message, caught and
shown through the exact same `role="status"` region the curated flow's
own errors already use — no new error-display mechanism.

`main.js`'s `runPlan()` was refactored to take `(target_elements,
candidates)` directly rather than always reading `state.selected`; two
thin callers (`runExamplePlan`, `runCustomPlan`) now feed it — the
curated-example code path is otherwise completely unchanged.

## Verification

- A real end-to-end run (Fe:2/O:3 target, a single `Fe2O3` candidate
  matching it exactly): produced 2 accepted plans, exactly as
  `plan_synthesis` would for the same request built any other way.
- Validation checked directly, not assumed: an empty target list
  produces "Enter at least one target element."; a duplicate element
  symbol produces "Element \"Ba\" was entered more than once."; both
  surface through the same status region and leave `#results` hidden.
- Remove-row buttons confirmed to actually remove their row from the
  DOM (count checked before/after via JS, not just visually).
- The curated-example flow re-run afterward, unchanged: BaTiO3's
  default generate still produces the identical 4 accepted / 1 rejected
  result this repo's own fixtures and prior playground phases already
  established.
- `axe-core` (same same-origin-injection technique as the accessibility
  hardening pass, run with the new `<details>` section open so its
  content is actually in the accessibility tree): 0 violations, 43
  passes (up from 39 — more interactive elements now exist to check).
- Every new input's accessible name checked directly via
  `getAttribute('aria-label')`, not eyeballed — all five inputs on a
  populated form carry the intended label.
- Narrow-width layout re-checked with the same fixed-width same-origin
  `<iframe>` technique the accessibility hardening pass used, with the
  `<details>` section forced open: rows wrap cleanly, nothing clipped
  or overflowing past the frame.
- Root quality gate: `cargo fmt --all -- --check`, `cargo test
  --workspace --all-features` clean — this change touches zero files
  under `src/` or `playground/wasm/`.

## What this does not claim

No claim about handling arbitrary or malicious formula text — there is
no formula text path at all, by design (see Scope). No claim about a
full manual keyboard walkthrough or a real screen-reader session for
the new rows specifically — same tool limitations as the accessibility
hardening pass apply here too (synthetic Tab-key traversal doesn't
register in this environment; the accessibility tree was used as the
closest available proxy).

## Status

Implemented, tested, root quality gate green. `playground/web` only.
