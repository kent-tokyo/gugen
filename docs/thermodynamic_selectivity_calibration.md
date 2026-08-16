# Phase 21B: thermodynamic-selectivity calibration

**Status: in progress.** Phase 21A's GO for Phase 21B was conditioned on
four requirements. Conditions 2 and 3 completed first (§2, §3). Condition
1 was originally scoped against Materials Project, but the owner
explicitly redirected it to **OQMD** instead (unauthenticated REST API,
own data licensed CC BY 4.0 — verified, §6.1) to avoid blocking this
phase on a credential. As of this document's current revision, condition
1's live data pull is itself blocked by a **real, external OQMD service
outage** (confirmed independently, §6.2) — a different kind of blocker
than the original MP-key one, and not something this phase can design
around.

| Condition | Status |
|---|---|
| 1. Thermodynamic-coverage check against gugen's real data source | **Redirected to OQMD (owner instruction); pre-registered gate and polymorph policy fixed in §6 before any data was fetched. Live pull currently blocked by an OQMD service outage (confirmed, §6.2) — not yet measured.** |
| 2. Manual label audit (mirroring Phase 20D) | **Done — this document, §2.** |
| 3. Artifact filtering | **Done — this document, §3.** |
| 4. Carry forward leakage exclusion + DOI independence unit | **Done — both conditions 2 and 3 build directly on Phase 21A's leakage-excluded, DOI-tracked population.** |

**No calibration was run. No correlation was computed. No GO/NO-GO for
the calibration itself can be given yet** — that decision still needs
condition 1 to actually measure something, and OQMD's own outage
prevented that this session. This is the same discipline as Phase 21A's
own §7 ("not measured in this phase"): an unmet precondition is reported
as unmet, not quietly worked around, and a real infrastructure outage is
reported as exactly that, not silently substituted with a cached or
third-party copy of the data (§6.4). No `src/` change, no version bump.

## 1. Recap: what this builds on

Phase 21A found 385 targets (lenient label aggregation) / 371 (strict,
majority-vote) with ≥2 gas-free-computable routes and disagreeing
pure/impure outcomes, after excluding gugen's own 5 curated
validation-fixture targets. Full detail: `docs/thermodynamic_selectivity_dataset_feasibility.md`.

## 2. Condition 2: manual label audit (pilot wave)

**Method**, mirroring Phase 20D's exactly (`benchmarks/sample_literature_observation_audit.py`
precedent): a deterministic, seeded sample was drawn from the Phase 21A
"clean" population (`benchmarks/sample_thermodynamic_selectivity_audit.py`,
seed `20260815`, wave 0), with DOI as the independence unit (a paper
contributes at most one sampled item, since multiple records from the
same paper share one extraction run). Strata: `impure` (8 items — the
minority class, 23.2% of records, prioritized since a calibration study
depends on this class being real) and `pure` (7 items). Manifest:
`benchmarks/data/thermodynamic_selectivity_audit_manifest.json`.

**Verification**: one independent research agent per ~5-item batch (3
batches, `general-purpose`, real WebSearch/WebFetch against legitimate
sources only — publisher pages, arXiv, PMC, Semantic Scholar, OpenAlex,
Unpaywall, CrossRef; **no paywall bypass attempted, no Sci-Hub, no pirate
mirrors**), asked to determine access level and whether the source paper
supports the claimed pure/impure verdict for that specific target+route.
Raw judgments (DOI/access-level/verdict/one-line reasoning only — no
paper text, matching Phase 20D's redistributable-data constraint):
`benchmarks/data/thermodynamic_selectivity_audit_judgments.json`.

### Results (n=15)

| Access level | Count | % |
|---|---|---|
| `full_text` | 1 | 6.7% |
| `abstract_only` | 5 | 33.3% |
| `source_inaccessible` | 9 | 60.0% |

| Verdict | Count | % |
|---|---|---|
| `match` | 2 | 13.3% |
| `mismatch` | 0 | 0.0% |
| `inconclusive` | 13 | 86.7% |

Among the 6 items with *any* access (full text or abstract): 2 match, 0
mismatch, 4 inconclusive (the accessible abstract didn't address purity
either way).

**Honest reading, not smoothed over**: this pilot found **zero
contradicting evidence** among everything it could check, but the
checkable fraction is far too small (2 confirmed matches, 0 mismatches)
to compute any meaningful precision estimate — a 2-of-15 (or 2-of-6)
sample cannot certify an accuracy rate, even though it is 2-of-2 (100%)
among the items that were conclusive. Most sampled DOIs are Elsevier
(ScienceDirect), whose platform returned HTTP 403 to automated fetches
regardless of open-access status, producing a 40% any-access rate (6.7%
full text). This is directly comparable to, not notably different from,
Phase 20D's own real access rate on a different (non-Elsevier-dominated)
DOI mix at n=58: 37.9% any access (8.6% full text), per
`benchmarks/data/literature_observation_audit_judgments.json`'s own
recorded `access_level` field — checked directly for this comparison,
not assumed. A further wave would likely face a similar access wall;
this is disclosed as a real, load-bearing limitation on how much
confidence any future calibration can place in this label source's
audited accuracy — not a problem this document solves, and not one to
quietly work around by treating title/metadata alone as evidence (the
agents were explicitly instructed not to, and followed that instruction:
e.g. item `10.1039/C3CE40473K`'s title reads as strongly suggestive of a
pure result, but was correctly marked inconclusive without abstract/
full-text confirmation).

### Real methodological caveats surfaced (beyond simple access limits)

Three distinct risk patterns were found by the verifying agents while
attempting real checks, worth carrying into any future calibration's own
design, not just this audit's numbers:

- **Complex/single-crystal processes flattened into a simple element
  route**: `10.1016/j.pnsc.2020.08.019` (Ni3Al) is a directionally-
  solidified single-crystal superalloy grown by a seed-crystal method,
  not a simple powder reaction — the dataset's 3-element route is an
  approximation of a substantially more complex real process.
- **Doped-compound routes possibly mislabeled under the host-lattice
  target formula**: `10.1063/1.5078773`'s route includes `MnCO3`, so the
  real synthesized product is Mn-doped ZnGa2O4, not the plain ZnGa2O4
  the target field names — a labeling ambiguity independent of access.
- **Flux-growth/single-crystal routes flattened into the same schema as
  solid-state powder synthesis**: `10.1016/j.materresbull.2013.10.047`'s
  precursor set (`Na2B4O7`, `PbO`, `V2O5`) reads as a flux-growth recipe
  for single crystals, not stoichiometric solid-state reactants — the
  source dataset may be applying one pure/impure schema across two
  physically different synthesis paradigms.
- A weaker, harder-to-quantify pattern also appeared: for several items,
  the cited DOI's paper topic was *adjacent to* rather than *centrally
  about* synthesizing the claimed target (a composite-mechanics paper, a
  ceramic-additive/dielectric paper, a high-pressure structural-stability
  paper) — consistent with, but not proof of, the text-mining pipeline
  sometimes picking up a compound name from a methods/introduction
  section rather than the paper's own synthesis result. Flagged as a
  hypothesis worth checking in a larger audit, not a confirmed finding at
  n=15.

None of this is treated as disqualifying gugen's own use of the dataset
(Phase 21A's finding stands on its schema-level facts, not on this
audit), but it materially narrows what condition 2 can honestly claim:
**this pilot did not, and at this scale could not, certify the pure/
impure label's accuracy.** A future, larger wave remains possible but
should be sized with the 60% inaccessibility rate found here as the
realistic expectation, not the more optimistic access rate Phase 20D
happened to get on a different (non-Elsevier-dominated) DOI mix.

## 3. Condition 3: artifact filtering

**Method**: `benchmarks/audit_thermodynamic_selectivity_dataset_feasibility.py`
(extended, same file Phase 21A committed) now also excludes, from the
385-target lenient headline, any route where the target or any precursor
formula matches one of two patterns found by direct inspection while
writing Phase 21A's report:

- **Duplicated-element artifact**: a bare element symbol immediately
  followed by its own repeat (e.g. `Ti3Ti`, `Si1Si`, `Al3Al`) — a clear
  LLM-extraction duplication bug, not a real compound.
- **Non-standard-separator formula**: any formula using a character
  outside letters/digits/parentheses/middle-dot/period/x — most commonly
  a `-` joining two formulas (e.g. `NaCl-KCl`, a genuine two-species flux
  mixture, or `(MgCO3)4-Mg(OH)2·5H2O`, a real mineral compound written
  with a non-standard hydrate separator). Both cases are, either way,
  not usable as a single parseable `Composition` without manual
  disambiguation this phase does not perform — so both are excluded
  together, honestly described as "not currently usable," not uniformly
  mislabeled as "wrong."

### Result

50 routes excluded for the duplicated-element pattern, 440 for the
non-standard-separator pattern (490 total, out of ~48,000 kept routes
across the full leakage-excluded population). After filtering:

| Metric | Before filtering | After filtering |
|---|---|---|
| Targets with selectivity signal | 1,742 | 1,715 |
| Targets with ≥2 gas-free-computable routes, disagreeing outcome | **385** | **381** |

**The sample gate (≥30 targets) still passes by a 12.7x margin after
artifact filtering — the finding barely moves.** Only 4 targets' entire
selectivity signal depended on an artifact-containing route. The
artifact-filtered, gas-free-computable population (1,692 individual
route rows across 381 targets) is written to
`benchmarks/data/thermodynamic_selectivity_clean_population.json` and is
the population any future calibration (and this document's own condition
2 sample) draws from — not the uncleaned 385.

## 4. What remains before a calibration can run

1. **Condition 1 (thermodynamic coverage)**: redirected to OQMD, see §6.
2. **The calibration experiment itself**: computing
   `balanced_reaction_delta_ev_per_atom` for each clean route pair and
   testing correlation against the audited pure/impure label — blocked
   on condition 1.
3. Condition 2's own finding (§2) should shape the eventual calibration's
   scope: given the real accessibility and route-representation caveats
   found, any future calibration should treat the pure/impure label as
   noisy and unverified-at-scale, not as ground truth with an established
   accuracy rate, and should consider excluding or flagging the specific
   caveat patterns found here (doped-compound-under-host-formula routes,
   flux-growth routes) if they recur systematically in the larger clean
   population — this was not checked at scale in this document.

## 6. Condition 1 via OQMD (owner-redirected, avoids the MP-key block)

The owner explicitly declined to wait for a Materials Project API key
and instead named OQMD (Open Quantum Materials Database) as condition
1's data source: its REST API needs no authentication, and its own data
is separately licensed (§6.1). This section fixes the source identity,
license, coverage gate, and polymorph policy **before** any data was
fetched, per this project's established discipline (Phase 21A's own
sample gate; the pre-implementation advisor-review item "coverage gate"
explicitly named for this phase).

### 6.1 Source, license, endpoint (verified, not assumed)

- **API**: `GET https://oqmd.org/oqmdapi/formationenergy` (also
  `/oqmdapi/formationenergy/<id>` for a single entry), unauthenticated —
  confirmed against the `qmpy_rester` reference client
  (`github.com/mohanliu/qmpy_rester`) and OQMD's own `restful.html`
  documentation, and against `mohanliu/oqmddoc`'s README ("No API key
  required").
- **Data license**: **CC BY 4.0**, stated by OQMD itself ("The data in
  OQMD is licensed under CC-BY 4.0") — this is the data license, not to
  be confused with the separate MIT license covering the `qmpy` software
  only. `oqmd.org` itself is down as this is written (§6.2), so this was
  verified via a Wayback Machine capture, timestamp `20260803134040`
  (~12 days before this outage was found) — the same sentence and license
  link appears unchanged in captures from 2021, 2024, and 2025, i.e. a
  stable, long-standing policy, not a stale or one-off snapshot.
  Attribution requirement: cite Saal, Kirklin, Aykol, Meredig, Wolverton,
  *JOM* 65, 1501 (2013), doi:10.1007/s11837-013-0755-4, and Kirklin, Saal,
  Meredig, Thompson, Doak, Aykol, Rühl, Wolverton, *npj Computational
  Materials* 1, 15010 (2015), doi:10.1038/npjcompumats.2015.10.
- **Scope caveat**: OQMD does not redistribute original ICSD-sourced
  input structures (an ICSD licensing restriction) — it provides ICSD
  collection codes instead. This does not block using OQMD's own
  computed formation energies/structures, which are what condition 1
  needs.
- **The owner's stated "v1.8, Feb 2026" dataset version is unverified**
  and is **not** written into any manifest on the strength of the
  prompt alone. The real snapshot identity this phase records is: the
  API response's own `meta.api_version` and `meta.time_stamp` fields,
  this fetcher's own retrieval datetime, the exact query used, and a
  checksum of the downloaded JSON — the same discipline as every other
  dataset this project has ever fetched (`fetch_kononova.py`,
  `audit_thermodynamic_selectivity_dataset_feasibility.py`).
- **Fields actually requested** (via the API's own `fields` param, so
  the raw snapshot never carries unrelated per-entry data like `sites`/
  `unit_cell`), matching `benchmarks/fetch_oqmd_coverage.py`'s
  `QUERY_FIELDS` exactly: `name` (formula), `entry_id`, `natoms`,
  `volume` (**unit-cell** volume, for `natoms` atoms — not per-atom),
  `delta_e` (formation energy, eV/atom, 0 K DFT), `spacegroup`,
  `stability` (hull distance, eV/atom, captured for context — not part
  of the coverage decision), `duplicate_entry_id`. Confirmed against the
  reference client and OQMD's own docs — treated as *probable*, not
  certain, since the documentation page is copyright-dated 2019 citing
  an older `qmpy` version; the fetcher asserts the fields the coverage
  decision itself needs (`name`, `entry_id`, `natoms`, `volume`,
  `delta_e`) are present in the first real response rather than silently
  coercing a missing one.

### 6.2 Live service status: down as of 2026-08-15, recovered 2026-08-16

`https://oqmd.org/` and `https://oqmd.org/oqmdapi/formationenergy`
returned `HTTP 502` on every attempt made while writing this section on
2026-08-15 — confirmed independently by two separate fetch paths
(direct `curl` and an agent's own `WebFetch`), consistently, not a
one-off transient blip in a single tool. No status page or open GitHub
issue documenting this specific outage could be found. **This was a
real, external infrastructure blocker, not a design gap in this
phase** — condition 1's live measurement could not be completed on
2026-08-15 for this reason.

**Update, 2026-08-16: the service is back.** While building the
automated recovery check below, a real (not mocked) request to
`https://oqmd.org/oqmdapi/formationenergy?composition=TiO2` returned
`HTTP 200` with a well-formed response (`_oqmd_version: 1.0`, 3
returned entries, e.g. `entry_id: 2475`, `delta_e: -3.21575255518126`
eV/atom, `stability: 0.0269`) — a genuine, usable formation-energy
result, not an empty or malformed body. This does **not** by itself
restart condition 1 — that still needs its own fresh, explicit owner
trigger, same as every prior update in this document required. It only
means the external blocker that stopped condition 1 on 2026-08-15 is,
as of this check, no longer present.

### 6.2.1 Automated recovery detection (added 2026-08-16)

Because the outage above already recurred once, a daily, low-load
*notification* mechanism was added so any future recurrence — or a
fresh outage after this one — doesn't depend on someone remembering to
check manually: `.github/workflows/oqmd-recovery-check.yml` runs
`.github/scripts/check_oqmd_recovery.py` once a day. "Recovered" is
defined as *OQMD returning usable data* (HTTP 200, valid JSON, the
expected fields present, at least one entry with a non-null `delta_e`
for a small fixed test composition), not merely "the site responds" —
a 200 with an empty or malformed body would still count as down. On
recovery it opens exactly one GitHub Issue and stops (a search for an
already-open issue with the same title prevents a duplicate on
subsequent daily runs); while down, it produces no issue and no
notification at all, so consecutive "still down" days don't train
anyone to ignore the one day that matters. **Opening that issue does
not, by itself, restart condition 1, run
`benchmarks/fetch_oqmd_coverage.py` for real, or start calibration** —
those still require the owner's own separate, explicit instruction,
same as the rest of this document already requires. See the script's
own module doc for the exact healthy/unhealthy criteria and retry
policy. **Recovery detection is necessary but not sufficient for a full
fetch to succeed**: §6.2.2 below shows the service can flap within
hours of a detected recovery, so a full 795-formula fetch needs a
*sustained* window, not just a healthy instant — this is exactly why
the fetcher's resume cache (§6.2.2) matters, since it lets a short
window make partial, retainable progress instead of an all-or-nothing
attempt.

### 6.2.2 Condition 1 fetch attempt, 2026-08-16: a second outage before completion

With OQMD confirmed live (§6.2), the owner authorized running condition
1's real coverage fetch
(`python3 benchmarks/fetch_oqmd_coverage.py`, 795 distinct formulas).
Observed uptime profile across roughly two hours:

- Health check (§6.2): up. PR #36 smoke test: up.
- Real fetch, attempt 1: `HTTP 429` at 50/795 — aborted, wrote nothing,
  per §6.4's discipline (no partial deliverable file).
- Real fetch, attempt 2: a network read-timeout at 50/795, a different
  symptom at the same request count — indicated general post-outage
  flakiness rather than one fixed rate-limit threshold.
- **Fix applied**: `query_composition` gained retry-with-backoff (4
  attempts, exponential) on HTTP 429/5xx/timeout only — non-transient
  failures (malformed JSON, missing fields) still raise immediately,
  no retry, unchanged from §6.4's original design. A resumable
  per-formula JSONL cache was also added, since a full run spans
  multiple execution windows and one run was killed by the environment
  partway through (~400/795) with no code-level failure at all.
- With both in place, resumed fetch runs reached 714/795 (89.8%)
  formulas cached, successfully riding out further transient errors —
  then failed persistently on `VNb9O25` with `HTTP 502` across all 4
  retry attempts.
- **Verified this was a real, service-wide outage, not one bad
  formula**: a direct `curl` on `VNb9O25` reproduced the 502 with
  OQMD's own error-page body ("temporary error... try again in 30
  seconds"); direct `curl` on two unrelated, simpler formulas
  (`Nb2O5`, `VO2`) also returned 502; `check_oqmd_recovery.py` (§6.2.1)
  independently reported `healthy=False: all 3 attempts failed: HTTP
  502`. A further resume attempt after a wait failed identically at
  the same point.
- **Result: condition 1 remains not measured**, per §6.4's own
  pre-committed rule for exactly this situation. No
  `oqmd_coverage_manifest.json` exists, so there is no coverage number
  and no gate verdict (§6.3) to report. The 714/795 raw fetch results
  live only in the gitignored, uncommitted resume cache
  (`benchmarks/data/.oqmd_fetch_cache.jsonl`) — **this cache is not a
  partial coverage result and must never be scored as one**; its only
  legitimate use is letting the next fetch attempt skip formulas
  already confirmed, needing only the remaining ~81.
- This is OQMD's second distinct outage within roughly 24 hours of the
  first one (§6.2) ending — evidence the service is currently flapping,
  not durably recovered. The daily recovery-check workflow (§6.2.1)
  will still flag the *next* healthy instant it observes, but per the
  note added there, that instant is not by itself grounds to expect a
  full fetch will complete; only a sustained window will.

### 6.3 Pre-registered coverage gate and polymorph policy

Fixed here, before any successful fetch, so a later result can't be
talked into passing by adjusting the criterion after seeing it (same
discipline as Phase 21A's 30-target sample gate):

- **Field mapping**: OQMD's `delta_e` (eV/atom, 0 K DFT formation
  energy) maps directly to `SolidThermodynamicEntry::formation_enthalpy_ev_per_atom`
  — this is the same "0 K DFT energy stands in for 0 K formation
  enthalpy" convention gugen's own type doc already establishes for any
  caller-supplied dataset (`src/thermodynamics.rs`: "A caller-supplied
  0 K formation enthalpy... gugen never fetches this data itself"), the
  same convention `MaterialsProjectSnapshotProvider` already uses for MP
  data. OQMD's `volume` is **unit-cell** volume; `SolidThermodynamicEntry`
  needs **per-atom** volume, so the fetcher computes `volume / natoms` —
  stated explicitly here so the conversion is never silently assumed.
- **Polymorph policy**: OQMD returns multiple entries per composition
  (distinct `spacegroup`/`entry_id`). Policy: **take the lowest
  `delta_e` among matches** — mirrors `MaterialsProjectSnapshotProvider::energy_for`'s
  existing "most stable known phase" convention exactly
  (`src/materials_project_adapter.rs`), order-independent by
  construction. This is a **modeling convention, never an experimental
  phase identification** — stated here so it cannot later be described
  as "the phase this route actually formed." `duplicate_entry_id` rows
  (OQMD's own explicit duplicate-calculation marker) are excluded before
  this selection, not counted as independent entries.
- **Dataset-mixing prevention**: OQMD entries are tagged with their own
  `ThermodynamicDatasetIdentity` (`source: "OQMD"`), distinct from any
  Materials Project identity. `balanced_reaction_delta_ev_per_atom`'s
  existing `InconsistentThermodynamicDataset` check (`src/thermodynamics.rs`)
  already refuses to compute across two different dataset identities —
  this is gugen's own existing, structural mechanism for the owner's
  "don't mix MP and OQMD entries" requirement, not a new check this
  phase has to build.
- **Coverage gate**: `balanced_reaction_delta_ev_per_atom` returns
  `Ok(None)` — an abstention — the moment *any single species* in a
  reaction lacks an entry, so coverage is all-or-nothing per route, and
  a route *pair* (needed for a within-target comparison) is only usable
  if **both** routes are fully covered. Per-species coverage can look
  high while very few full route pairs survive — both numbers are
  computed and reported (§ coverage report, once data is available), but
  **the gate is on the route-pair number**: condition 1 passes only if
  **≥30 targets retain ≥2 fully-OQMD-covered, gas-free, outcome-
  disagreeing routes** — the same 30-target floor as Phase 21A's sample
  gate, applied one stage later in the pipeline.

### 6.4 What was not done to route around the outage

No cached, mirrored, or third-party copy of OQMD's data was substituted
for a live pull. The Wayback Machine was used only to read a *license
statement*, not to source data, and the sharper reason that's a
legitimate distinction (not just "text vs. bulk data"): a license
statement is a **claim about terms**, whose staleness this document
independently verified by checking it was unchanged across five years
of snapshots; a dataset is a **measurement**, whose staleness cannot be
verified at all once the live source is unreachable — an archived
formation-energy value could reflect a since-corrected calculation and
there is no way to know. This is the same principle that ruled out using
the Lee et al. dataset's `mp_eabovehull` field as a coverage substitute
in an earlier draft of this document; it generalizes beyond that one
case. If the live service is still down when this phase is next picked
up, condition 1 remains reported as **not measured, for a documented
external reason** — not silently worked around. `oqmd.org`,
`www.oqmd.org`, `api.oqmd.org`, and `oqmd.northwestern.edu` were all
checked (only the separately-hosted `static.oqmd.org` docs mirror
responded) before concluding this, not on a single failed request.

**Committability of the raw coverage snapshot**: not yet decided, since
its real size is unknown until a live fetch succeeds — the fetcher
already requests only the fields the coverage decision needs (no
`sites`/`unit_cell`), and the same size test used throughout this
project applies once real data exists (commit if comparable to
`kononova_sample.jsonl`'s ~500KB, gitignore and keep only a manifest
otherwise).

## 7. Non-goals (unchanged)

No `score_plan` connection, no `RankingWeights` change, no default
ranking change, no success-probability claim, no automatic temperature
selection, no gas-phase thermodynamics, no literature-condition
promotion, no version bump, no public API change of any kind, no
calibration result of any kind (none was computed).
