# gugen — Phase Checklist

Source of truth for phase content is AGENTS.md §26. This file tracks status
only; do not restate the requirements here beyond what's needed to check
work off.

## Phase 0 — Landscape and Architecture — DONE (2026-08-13)

- [x] Existing inorganic synthesis planners surveyed → `docs/competitors.md`
- [x] Precursor-selection research surveyed → `docs/competitors.md`
- [x] Phase-diagram-planning boundary noted (reaction-network/thermodynamic-
      selectivity work treated as a future `ThermodynamicProvider` source,
      not something core reimplements) → `docs/competitors.md`,
      `docs/scientific_scope.md`
- [x] Solid-state synthesis dataset surveyed (Kononova et al. 2019
      text-mined dataset) → `docs/competitors.md`. **License not yet
      checked** — do not bundle or cite as ground truth until confirmed.
      Carried forward as an open item below.
- [x] chematic-crystal API checked → not published anywhere yet (verified
      via crates.io + GitHub API). Trait-boundary path adopted.
      → `docs/integration.md`
- [x] mikiwame API checked → not published anywhere yet. Optional-feature
      path adopted, diagnostic trait boundary deferred to Phase 6.
      → `docs/integration.md`
- [x] crates.io/GitHub name collision checked for `gugen`,
      `chematic-crystal`, `mikiwame` → no collisions found.
      → `docs/competitors.md`
- [x] Data licensing checked for the one dataset surveyed so far → open
      item, not blocking (see below).
- [x] Scientific scope confirmed against AGENTS.md §2–4 →
      `docs/scientific_scope.md`
- [x] Report schema referenced (AGENTS.md §6 taken as-is; no redesign
      needed) → `docs/architecture.md`
- [x] Provider boundary designed (AGENTS.md §8 traits taken as-is; isolation
      rules restated) → `docs/architecture.md`
- [x] Reaction balancing method decided: exact rational linear algebra /
      null-space over the element×species matrix, `i128` checked arithmetic,
      dependency choice deferred to Phase 2 → `docs/architecture.md`

**No stop-and-report condition was triggered.** Name checks came back
clean; chematic-crystal/mikiwame unavailability is the *expected* path
AGENTS.md §5 already designs for, not an obstacle.

**Open item carried forward (not blocking):** Kononova et al. 2019
text-mined dataset license must be checked before it's used as a fixture
source or bundled in any form (Phase 8/§22). If the license turns out to be
incompatible or unclear, that becomes a real stop-and-report at that point
(AGENTS.md §28 "使用候補datasetのライセンスが不明").

## Phase 1 — Foundation — NOT STARTED

- [ ] crate init (`Cargo.toml`, edition 2024, `#![forbid(unsafe_code)]`)
- [ ] `error.rs` typed errors
- [ ] `config.rs`
- [ ] validated numeric types (`TemperatureRange`, `DurationRange`, etc. —
      min≤max, finite, NaN-free per AGENTS.md §6)
- [ ] `composition.rs`
- [ ] `target.rs` incl. `TargetMaterialView` boundary from `docs/integration.md`
- [ ] report schema types (`report.rs`) — structure only, no ranking logic
- [ ] `provenance.rs`
- [ ] provider traits (`provider.rs`) — traits only, no implementations
- [ ] JSON round-trip test
- [ ] CI (fmt/clippy/test/doc gates from AGENTS.md §25)

Explicit reminder for this phase (AGENTS.md §26): do not grow diagnostic or
planning logic yet — foundation only.

## Phase 2–9

Not started. Will be filled in with the same DONE/NOT STARTED tracking
style as each phase begins; see AGENTS.md §26 for phase content and §29 for
the v0.1 completion checklist.
