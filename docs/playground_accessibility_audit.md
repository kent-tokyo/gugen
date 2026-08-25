# gugen Playground — Accessibility Audit and Hardening

## Why this exists

The owner's own instruction after the Playground MVP shipped and deployed:
audit `playground/web` for accessibility, fix only what the audit finds,
no new features, no free-form input, no commercial catalog, no backend/
core changes. Explicit method: automated tools (axe/Lighthouse) as an
aid, keyboard operation, a screen-reader smoke test, narrow widths and
200% zoom, real Chrome recheck — with an explicit instruction not to
declare "accessible" from automated tool output alone.

## Method

1. Read `playground/web`'s actual current source (`index.html`,
   `main.js`, `render.js`, `styles.css`) — confirmed via `grep` that
   zero `outline`, `transition`, `animation`, `tabindex`, `aria-*`, or
   `role=` attributes existed anywhere before this audit.
2. Computed WCAG contrast ratios for every color pair in the palette
   directly (the WCAG relative-luminance formula, not eyeballed).
3. Injected `axe-core` (same-origin, matching the deployed CSP —
   `axe-core/cli`'s own bundled ChromeDriver didn't match the installed
   Chrome version, so it was injected directly into a real Chrome tab
   instead) and ran it against both the initial and results-shown page
   states.
4. Inspected the real accessibility tree (not just the DOM) as the
   closest available proxy for what a screen reader exposes.
5. Tested keyboard interaction by dispatching real `KeyboardEvent`s at
   focused elements and checking the resulting state (roving tabindex,
   `aria-selected`, panel visibility) — see "Tool limitations" below for
   what this could and couldn't cover.
6. Rendered the page inside a fixed-width, same-origin `<iframe>` (320px
   and 768px) to get genuine visual confirmation of narrow-viewport
   layout, since the browser automation's own window-resize tool did
   not propagate to the actual rendered viewport in this environment.

## Findings, classified before any fix (Blocker / Major / Minor / No issue)

**Blocker**: none. Every interactive control was already a semantic
`<button>`/`<a>`, structurally keyboard-operable by default; all content
was reachable; nothing trapped focus or hid content from everyone.

**Major**:
1. No WAI-ARIA tabs pattern on the 3-tab result view — plain buttons
   and `div`s with no `role="tablist"/"tab"/"tabpanel"`, `aria-selected`,
   `aria-controls`/`aria-labelledby`, or arrow-key navigation. Screen
   reader users got "button, button, button" with no indication these
   formed a tab set or which was selected.
2. Example-target selection was conveyed by a CSS class (color/border)
   only — no `aria-pressed` or equivalent, so assistive-technology users
   had no way to know which target was currently selected.
3. Status messages ("Planning…", errors) had no live region — nothing
   was announced to screen reader users, and a successful generation was
   indicated only by silently clearing the status text to nothing.
4. The Copy buttons gave zero feedback — visual or otherwise — on
   success or failure.
5. An example card's accessible name was its *entire* nested text
   content concatenated (category, description, "look for", "stays
   unresolved" — one long unstructured string), confirmed via the real
   accessibility tree, not assumed.
6. Card/button border contrast computed at **1.28–1.39:1** against both
   the page and panel backgrounds — well under the WCAG 1.4.11 (Non-text
   Contrast) 3:1 guideline for interactive-element boundaries. Text
   itself was never at risk (5.56–15.4:1, see below) — this was
   specifically about perceiving where a button/card starts and ends.
7. The process-step row's 3-column layout (requirement label / step
   name / fields) became cramped and hard to read at ~320–375px width —
   confirmed by actually rendering the page in a 320px-wide same-origin
   iframe, not inferred from CSS alone (an initial code-only read
   predicted a *different*, worse problem — real horizontal overflow on
   the tab buttons — that turned out not to happen once rendered:
   flexbox's default shrink + text wrap handled that case gracefully.
   The lesson held: automated/code-only inference both under- and
   over-predicted real issues; only rendering it settled which).

**Minor**:
8. `<section id="step3" hidden>` never had its `hidden` attribute
   removed by any code path — its "3. Compare" heading never appeared
   for anyone, sighted or not (confirmed via `getComputedStyle`:
   `display: none` even after generating a plan). Not an
   assistive-technology-specific bug, but a real content-completeness/
   heading-hierarchy gap.
9. Tab panels had no `tabindex="0"`, so there was no single predictable
   keyboard stop to enter a panel's content, per the WAI-ARIA APG tabs
   pattern (minor: content remained fully reachable via a screen
   reader's own virtual-cursor navigation regardless).

**No issue — checked directly, not assumed**:
- `prefers-reduced-motion`: zero `transition`/`animation`/`@keyframes`
  exist anywhere in the stylesheet (confirmed via `grep`) — nothing to
  guard against.
- Focus indicator: no `outline: none` anywhere (confirmed via `grep`);
  visually confirmed a clear focus ring renders on a programmatically
  focused button. A `:focus-visible` rule was still added (see below)
  for cross-browser consistency, not because the default was broken.
- Heading hierarchy: h1 → h2 → h3 → h4, no skipped levels (aside from
  finding #8's dead section, now fixed).
- Landmarks: `<header>`/`<main>`/`<footer>` correctly exposed as
  `banner`/`main`/`contentinfo` in the real accessibility tree.
- Text color contrast: computed via the WCAG relative-luminance formula
  for every pair actually used — 5.56:1 to 15.4:1, comfortably passing
  AA (4.5:1) and mostly AAA (7:1).
- Tab-button wrapping at 320px: visually confirmed fine (see finding 7).
- `pre` (JSON/Markdown) overflow: already had `overflow-x: auto` from
  the original build — long lines scroll inside their own box, never
  break the page.
- 768px (tablet) width: visually confirmed clean via the same iframe
  technique.
- `axe-core`: 0 violations, 0 incomplete, both before and after fixes —
  reported here for completeness, explicitly **not** treated as proof
  of anything beyond what axe's ruleset actually checks (see "What this
  audit does not claim" below).
- CSP / zero external requests: unaffected by every fix below — no new
  dependency, no new network call anywhere.

## Tool limitations, disclosed rather than papered over

- **Literal Tab-key traversal could not be mechanically demonstrated.**
  Synthetic `Tab` keypresses dispatched through the browser-automation
  tool did not move `document.activeElement` in this CDP session (a
  tool limitation, not a page defect — confirmed separately that
  programmatic `.focus()` and dispatched `keydown` events on an already-
  focused element both worked correctly). Keyboard operability was
  instead verified structurally: every control is a native `<button>`/
  `<a>` (guaranteed Enter/Space activation), DOM order matches visual
  order, and the new tablist's arrow-key handling was confirmed by
  focusing a tab via JS and dispatching a real `ArrowRight` keydown,
  observing focus and `aria-selected`/panel visibility all move
  correctly together. A real manual Tab-key spot-check is still worth
  doing independently.
- **No true screen-reader (VoiceOver/NVDA) audio test was possible** in
  this environment. The real accessibility tree (via the browser tool's
  `read_page`) was used as the closest available proxy — it reflects
  the same role/name/state computation a screen reader consumes, but
  isn't a substitute for hearing actual announcement behavior,
  timing, or a specific screen reader's own quirks.
- **200%-browser-zoom specifically was not tested**; the 320px
  same-origin-iframe technique (a genuine narrow rendering, not a
  visual scale-up) was used instead to check reflow, which exercises
  the same CSS breakpoints WCAG 1.4.10 (Reflow) cares about.
- **`axe-core/cli`'s bundled ChromeDriver version didn't match the
  installed Chrome** (`This version of ChromeDriver only supports
  Chrome version 152... Current browser version is 151`) — worked
  around by injecting `axe-core` directly into a real Chrome tab
  instead of installing a version-matched driver, avoiding adding new
  system state for a one-off audit.
- **A real clipboard-permission hang was hit and is disclosed, not
  hidden**: calling `navigator.clipboard.writeText()` from this
  automated session hung indefinitely (45s timeout) rather than
  resolving or rejecting — almost certainly a permission-prompt with no
  user present to answer it, not a defect in the deployed page (a real
  user's own direct click on HTTPS does not typically prompt at all).
  Recovered by navigating away. This exact failure mode — a promise
  that never settles — was not something the original Copy button
  handled, so it became finding/fix material in its own right (see
  below), verified afterward with a mocked hanging clipboard rather
  than by risking triggering the same hang again.

## Fixes made (minimum necessary, `playground/web` only)

- `index.html`: real WAI-ARIA tabs markup (`role="tablist"` with
  `aria-label`, `role="tab"` + `aria-selected` + `aria-controls` on each
  tab button, `role="tabpanel"` + `aria-labelledby` + `tabindex="0"` on
  each panel); `role="status"` on the status paragraph; a small
  `aria-live="polite"` feedback span next to each Copy button; removed
  `#step3`'s `hidden` attribute so "3. Compare" actually appears,
  matching steps 1 and 2's own always-visible headers.
- `main.js`: `aria-pressed` + a concise `aria-label` on each example
  card; roving `tabindex` (`0` on the selected tab, `-1` on the rest)
  and Left/Right/Home/End arrow-key navigation on the tablist, per the
  WAI-ARIA APG automatic-activation tabs pattern; a real announced
  success message ("Plan generated: N accepted plan(s), M rejected
  candidate(s).") instead of silently clearing the status text; Copy
  button handlers rewritten with `try`/`catch` *and* a 3-second
  `Promise.race` timeout fallback (the hang disclosed above), so a
  clipboard write that never settles still produces a definite
  "Copy failed — select the text manually." message rather than
  silence, confirmed against both a mocked hanging clipboard and a
  mocked succeeding one.
- `styles.css`: `--border` strengthened from `#2a2e38` to `#606a80`
  (3.48:1 against the page background, 3.21:1 against card panels —
  both clear the 3:1 guideline; also finally gives the previously
  dead/unused `--ok` custom property a real use, in the new
  `.copy-feedback` success color); a `:focus-visible` rule for
  consistent, deliberate focus rings across browsers; `overflow-wrap:
  anywhere` on `body` as a blanket, zero-downside guard against any
  single long token breaking layout (only engages when a word actually
  has no natural break point — does not change normal text wrapping);
  a `@media (max-width: 480px)` rule stacking `.step`'s three columns
  vertically instead of squeezing them, confirmed by re-rendering the
  same 320px iframe test after the fix.

## Re-verification after fixes

- `axe-core`: 0 violations, 0 incomplete, 39 passes (up from 24/26 —
  more rules had ARIA state to actually check once the roles existed).
- Accessibility tree re-inspected: `tablist`/`tab`/`tabpanel` roles all
  present and correctly named; `status "Plan generated: ..."` present
  as a real status-role node; `aria-pressed`/`aria-selected`/roving
  `tabIndex` all confirmed via direct attribute reads, not just visual
  inspection.
- Arrow-key navigation re-confirmed: focusing the first tab and
  dispatching a real `ArrowRight` keydown moved focus to the second
  tab, set its `aria-selected="true"`, and unhid its panel — all three
  together, in one interaction.
- Copy feedback re-confirmed against both a mocked-hanging and a
  mocked-succeeding `navigator.clipboard`: "Copy failed — select the
  text manually." appears within ~3s in the hang case; "Copied!"
  appears immediately in the success case.
- 320px narrow-width re-render (same iframe technique): the process-
  step row now stacks cleanly (Required / Weigh / materials list, each
  on its own line) instead of the cramped three-column squeeze.
- Root quality gate: `cargo fmt --all -- --check`, `cargo test
  --workspace --all-features` both clean — this PR touches zero files
  under `src/` or `playground/wasm/`, so nothing there could have
  regressed; re-run anyway, matching this project's own "always
  re-verify" discipline.
- CSP and zero-external-request behavior: unaffected — no new
  dependency, no new network call in any fix.

## What this audit does not claim

Axe-core reporting 0 violations before *or after* this pass is not
treated as "the page is accessible" — several of the real findings
above (the missing tabs semantics, the verbose card names, the
non-text contrast gap, the narrow-width layout) were invisible to axe
entirely, both before they were fixed and structurally unreachable by
axe's own ruleset in general (it can't infer design intent like "this
button group is meant to behave like tabs," and it doesn't have a
reliable automated non-text-contrast check). This audit does not claim
WCAG conformance at any specific level, does not claim a real screen
reader was used, and does not claim every possible viewport/zoom
combination was exercised — see "Tool limitations" above for exactly
what could and couldn't be verified in this environment, and what a
follow-up manual pass (real Tab-key keyboard walkthrough, real
VoiceOver/NVDA session, real 200% browser zoom) would still add.

## Status

Implemented, tested, root quality gate green. Scope held: no new
features, no free-form input, no commercial-catalog work, zero changes
to `src/` or `playground/wasm/`.
