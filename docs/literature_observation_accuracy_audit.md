Phase 20D: a manual extraction-accuracy audit of `LiteratureObservationCorpus`
against original source papers -- the question Phase 20B could not answer on
its own. Phase 20B (`docs/literature_observation_provider.md`) established
that the corpus's own text-mining pipeline (Kononova et al. 2019) sometimes
disagrees with itself: 1,063 of the 13,982 emitted observations have
`temperature: None` specifically because 2+ candidate readings disagreed.
Phase 20B correctly left those unresolved rather than guessing, but that
left an open question this document exists to answer: when the pipeline
reports *one* confident value, how often is it actually right, and when it
disagrees, is that a real experimental difference or a misextraction? This
regenerates on demand from the commands in "Reproduction" below; it is not
auto-updated.

**Scope boundary, same as Phase 20B's**: this audit does not build a
conflict resolver, a promotion policy, or any Planner connection. Its only
output is calibrated evidence about extraction accuracy, for Phase 20C's
design to be built on. See the owner's own framing, quoted in "Go/no-go" below.

## 1. Sample design and count

**Independence unit: the DOI, not the observation.** Two observations from
the same paper share one text-mining run over the same source text, so they
are not independent evidence about extraction accuracy. The sampler
(`benchmarks/sample_literature_observation_audit.py`) draws DOIs, and each
sampled DOI contributes exactly **one** observation to the manifest, chosen
by a fixed deterministic rule (lowest `(corpus_record_index,
operation_index)` among that DOI's qualifying observations) -- so every
judgment row is one independent Bernoulli trial per field, and no per-DOI
aggregation step is needed downstream.

**Strata** (disjoint by DOI, priority order below; a DOI qualifying for more
than one is assigned to the highest-priority one):

| stratum | population (DOIs) | why sampled |
|---|---|---|
| `temp_disagree` | 806 | the stratum this phase exists to measure -- observations with `temperature: None` from a genuine 2+-candidate disagreement |
| `atm_controlled` | 400 | atmosphere fell through to `Atmosphere::Controlled { description }` |
| `fully_resolved` | 2,190 | all three fields resolved -- checks for false confidence in clean-looking data |
| `baseline` | all 6,370 | unconditional simple random sample, independent of the above -- the control group that rules out cherry-picking edge cases |

A `sparse` stratum (all three fields `None`) was considered and dropped
before sampling: with abstract-only access -- the dominant access level, see
below -- there is usually no way to distinguish "the paper reports no firing
temperature" from "the abstract just doesn't mention the firing temperature
that's in Table 2." That budget went to `temp_disagree` instead.

**Two waves, seeded and reproducible** (seed `20260815` throughout;
`benchmarks/data/literature_observation_audit_manifest.json` records both
waves' exact sizes and the DOIs drawn):

- **Wave 0 (pilot, 10 DOIs, `temp_disagree` only)**: run first, specifically
  to measure real paper accessibility before sizing the rest, per the
  advisor-reviewed design (see "Discovered bugs" below for what the pilot
  caught).
- **Wave 1 (48 DOIs)**: `temp_disagree=20, atm_controlled=8,
  fully_resolved=8, baseline=12`, sized from the pilot's measured
  accessibility rate and the advisor's explicit priority ordering (cut
  `atm_controlled`/`fully_resolved` before `temp_disagree` if budget runs
  tight).

**Total: 58 DOIs checked** (10 + 48), each by one independent research
review (a fresh agent per DOI, real `WebSearch`/`WebFetch` against
legitimate sources only -- Crossref, Semantic Scholar, Unpaywall, OpenAlex,
publisher abstract pages, genuine open-access copies, author institutional
repositories; no paywall bypass, no sci-hub or mirror sites, ever). Plus a
**5-item inter-reviewer overlap** (see item 8) drawn from the accessible
first-pass items and given a second, blind, independent review.

## 2. Source accessibility

| access level | n | % |
|---|---|---|
| `full_text` | 5 | 8.6% |
| `abstract_only` | 17 | 29.3% |
| `source_inaccessible` | 36 | 62.1% |

Some usable text (full text or abstract) was reached for **22/58 (37.9%)**
of sampled DOIs. This is the single dominant constraint on this audit's
statistical power, and it is worth naming precisely: every `full_text` hit
in this sample came from a legitimate non-publisher mirror -- an author's
PhD thesis, an institutional repository, or PMC for an open-access journal
-- **never** from the publisher's own page. Zero of the 58 direct
publisher-page fetches (ScienceDirect, Wiley, ACS, IOPscience, Springer)
succeeded; all returned 403/bot-block or a login wall. This is expected for
a corpus built from mid-2000s-to-2010s Elsevier/Wiley/ACS synthesis
journals, but it means the true ceiling on this kind of audit, done this
way, is set by how much of the materials-synthesis literature has *any*
legitimate open-access footprint -- not by review effort.

Per stratum: `temp_disagree` 3 FT / 7 abs / 20 inacc (n=30, pilot+wave1
combined); `atm_controlled` 2 FT / 0 abs / 6 inacc (n=8); `fully_resolved` 0
FT / 5 abs / 3 inacc (n=8); `baseline` 0 FT / 5 abs / 7 inacc (n=12).

## 3. Identity accuracy

Whether the sampled record's target/precursors actually correspond to what
the paper describes -- computed two ways, both reported (see
`identity_accuracy()` in `audit_literature_observations.py` for why picking
only one framing would be a denominator choice worth catching):

- **Conservative** (unverifiable counts as a non-match): 41/58 = **70.7%**
  (95% CI [58.0%, 80.8%])
- **Excluding unverifiable** (only rows that reached a definite verdict):
  41/44 = **93.2%** (95% CI [81.8%, 97.7%])

**3 confirmed identity mismatches** (all found from title/abstract-level
evidence alone -- none required full text):

1. **`10.1016/j.physc.2013.04.028`** -- the corpus's target `BaCeO3` is
   attributed a "synthesis" (target CeO2+BaO2 → BaCeO3), but the actual
   paper is about melt-processed YBCO/Y211 bulk superconductors, where
   CeO2, BaCeO3, and BaO2 are three separate, parallel *additive* powders
   compared against each other -- not a sequential reaction. This looks
   like an NLP list-to-reaction mis-grouping, not a wrong number within a
   real route.
2. **`10.1016/j.jpowsour.2008.05.041`** -- the corpus's target is
   `Ce0.8Gd0.2O1.9` (GDC20) made from `Gd2O3 + Ce0.9Gd0.1O1.95` (GDC10). The
   actual paper studies CaO/SiO2-doped **GDC10** (`Ce0.9Gd0.1O1.95`) itself
   -- Gd2O3 never appears, GDC20 never appears, and the real dopant sources
   (a SiO2 sol and calcium acetate) are entirely absent from the extracted
   precursor list. This looks like a real raw material (GDC10 powder)
   mis-roled as a "precursor" for a target the paper never makes, with a
   Gd2O3 co-precursor apparently invented to balance the stoichiometry.
3. **`10.1007/s00339-016-9959-0`** -- the extracted record claims a
   `K2Ti2O5` synthesis; the DOI actually resolves to a XANES/micro-Raman
   spectroscopy paper on barium titanosilicate glass-ceramics, with no
   relation to potassium titanate at all. This is a DOI-to-record linkage
   error upstream in the corpus, not a within-record extraction error.

This is a materially different category of error than a wrong temperature:
a conflict resolver operating on these three records would be reconciling
conditions across records that were never the same experiment, or never a
real experiment at all. See "Go/no-go" for why this matters more than the
field-level percentages below.

## 4. Operation accuracy

Whether `operation_index` correctly identifies *which* heating step in the
paper's real procedure a given observation describes (as opposed to, say,
labeling a sintering step as the first calcination). Checkable only where
full text was reached (n=5): in every case, `operation_index` pointed at
the intended step and the extracted duration/atmosphere for that step
matched the correct stage, not a different one.

The deeper issue found is not misindexing but **granularity**: three of the
five full-text cases show a single sentence in the paper describing a
*sequential multi-stage* heat treatment (e.g. "calcined at 900°C then
1000°C" or "sintered at 900°C/4h then 1250°C/4h") that the pipeline captured
as one `HeatingOperation` with two conflicting temperature candidates,
rather than as two separate operations. See item 6 -- this is the single
most common confirmed mechanism behind `temp_disagree` observations in this
sample.

## 5. Field-level accuracy

Match rate among rows that reached accessible source AND had a value to
check (`accessible_but_unstated` rows -- source was read but doesn't state
the field -- are reported alongside but excluded from the CI itself, since
they carry no evidence either way):

| field | matches / trials | rate (95% CI) | `accessible_but_unstated` (excluded) |
|---|---|---|---|
| temperature | 5/8 | 62.5% [30.6%, 86.3%] | 8 |
| duration | 5/6 | 83.3% [43.6%, 97.0%] | 10 |
| atmosphere | 8/8 | 100.0% [67.6%, 100.0%] | 8 |

These intervals are wide -- 8, 6, and 8 trials respectively -- and should be
read as "consistent with fairly high accuracy on the fields we could check,
with too little data to rule out a materially lower true rate," not as a
precise measurement. Per-stratum breakdowns (also wide, n=1-4 each) are in
`benchmarks/data/literature_observation_audit_summary.json`.

*One of the three `temperature` mismatches
(`10.1016/j.jeurceramsoc.2005.03.032`) is itself contested -- a second,
independent reviewer reached `match` on the same source (item 8). Counting
it as `match` instead moves `temperature` to 6/8 = **75.0%** (95% CI
[40.9%, 92.9%]). This table reports the first-pass verdict as its primary
figure, but that row is flagged unresolved rather than left for a reader to
take 62.5% as more settled than it is -- see the bullet below.*

Two of the three temperature mismatches deserve individual mention rather
than being absorbed into the rate:

- **`10.1016/j.jeurceramsoc.2005.03.032`** -- gugen extracted a single,
  confident 1360°C (not a disagreement). The source document itself is
  internally inconsistent: 1360°C appears in both the actual published
  article's freely-visible "section snippets" preview *and* the
  introduction of the first author's own PhD thesis, while the thesis's
  detailed Chapter 4 methods section states 1630°C for the same step. Two
  independent reviewers reached different conclusions about which value is
  authoritative -- the first argued 1630°C on physical grounds (SrTiO3's
  eutectic temperature requires exceeding 1430°C for the described
  liquid-phase grain growth, which 1360°C would not do), the second
  favored 1360°C because it is what the article's own text says, twice,
  independently of the thesis's internally-conflicting chapter. This audit
  does not adjudicate between them -- it is reported as an unresolved,
  disclosed case, and stands as a concrete illustration of why "check
  against the original paper" is not always a clean yes/no even with full
  text in hand.
- **`10.1016/j.ssi.2017.03.028`** (pilot, wave 0) -- gugen's raw candidates
  were `[500°C, 950°C]`. The accessible source (author's PhD thesis) shows
  a first calcination at 500°C/12h to preform a volatile phase, then a
  separate 950°C/24h anneal -- both real values in the source, but the
  reviewer judged the atmosphere/duration fields as `mismatch` for the
  fused operation rather than classifying a `multi_entry_cause`, since the
  exact causal mechanism linking the two source stages to the two raw
  candidates wasn't fully nailed down. Kept as reported, not upgraded.

## 6. Multi-entry disagreement causes

Classified **only** over `temp_disagree` rows that reached `full_text`
access (n=3) -- this classification requires seeing the actual sentence(s),
so abstract-only or inaccessible rows are excluded from this denominator
entirely, reported separately from every other metric per the pre-commit
review requirement:

- **2/3 classified as `genuine_multi_condition`** (`10.1016/j.jpowsour.2007.08.077`,
  `10.1016/j.ssc.2009.01.002`) -- both independently reconfirmed by a second,
  blind reviewer for the first DOI (identical verdict down to every field).
- **1/3 left `insufficient_evidence`** (`10.1016/j.ssi.2017.03.028`, see item 5) --
  the reviewer had real evidence of a problem but not enough to commit to a
  specific cause.

**The dominant confirmed mechanism, named precisely**: in every full-text
case where the cause could be pinned down (this sample's 2, plus a third
case in the `atm_controlled` stratum for an atmosphere disagreement,
`10.1038/srep04350` -- see item 7), the root cause was the same shape: the
source paper describes a genuine **sequential or parallel multi-stage
treatment in one sentence** ("calcined at 900°C then 1000°C"; "heated
either in air or in a reducing atmosphere"), and the extraction pipeline
captured it as a single `HeatingOperation` with 2+ internally conflicting
candidate values, rather than as two separate operations. This is not "the
paper is ambiguous" -- it is a specific, nameable **step-segmentation**
failure mode upstream of gugen (in the Kononova pipeline), consistent
across every full-text-confirmed case in this sample. It is directly
actionable for Phase 20C: these are not two conflicting reports about *one*
step, they are one paper's *two* steps merged into one record, and a
promotion policy that tries to pick "the" temperature for such a record is
solving the wrong problem.

No `nlp_misextraction`, `unit_confusion`, or `other` cause was confirmed in
this sample at the `full_text` bar -- but n=3 is far too small to say those
mechanisms are rare, only that this sample didn't confirm one. Two
mechanically-computable, corpus-wide (not manually audited) signals are
worth noting as a cheap supplementary check, not as classified causes: among
all 1,063 `temp_disagree` observations, several sampled items had raw
candidate pairs with one value under 5°C alongside a plausible
three-or-four-digit firing temperature (e.g. `[1.0, 940.0]`,
`[2.0, 1350.0]`) -- physically implausible as a second real firing
temperature for ceramic synthesis, and a plausible (not confirmed) signature
of contamination from an unrelated number rather than two genuine
conditions. This was not classified as `multi_entry_cause` for any specific
record without full-text confirmation, per the rubric's own conservatism.

## 7. Atmosphere-mapping results

Checkable at `full_text` for 2 of 8 sampled `atm_controlled` DOIs, both
confirming the mapping was correct rather than a misextraction:

- **`10.1038/srep04350`** -- gugen's `Controlled { "air, argon, hydrogen" }`
  (three gas tokens for one operation) is explained exactly: the paper's
  Methods state samples were heat-treated "either in air or in a reducing
  atmosphere (10% H2 in Ar)" -- two parallel sample branches at identical
  temperature/duration, correctly merged into one gas-token set by the
  extraction, just not split back into two branch-specific records.
  Classified `genuine_multi_condition`.
- **`10.1016/j.jeurceramsoc.2005.03.032`** -- gugen's `Controlled {
  "argon, hydrogen" }` matched the source's stated 5% H2/95% Ar sintering
  atmosphere exactly.

Both `atm_controlled` cases checked were correct. n=2 -- not enough to
generalize, but zero misses is itself informative alongside the field-level
100% (n=8) figure in item 5.

## 8. Inter-reviewer agreement

A 5-item overlap subset was drawn from the *accessible* (full_text or
abstract_only) first-pass items -- deliberately not from
`source_inaccessible` items, where a second review would trivially "agree"
on `not_checked` and measure nothing. Each second-pass review was
independent and blind to the first-pass verdict.

| field | agreement |
|---|---|
| `access_level` | 2/5 = 40% |
| `identity_match` | 4/5 = 80% |
| `temperature_verdict` | 2/5 = 40% |
| `duration_verdict` | 3/5 = 60% |
| `atmosphere_verdict` | 3/5 = 60% |

**The dominant disagreement is `access_level` itself, not judgment quality.**
3 of 5 items disagreed on whether the source was reachable at all --
ScienceDirect's free "section snippets" preview and certain repository
mirrors are bot-detection-gated in a way that is not deterministic across
independent attempts, so the same DOI can yield `full_text` on one pass and
`source_inaccessible` on another. Every downstream field-verdict
disagreement on those 3 items is a direct consequence of this
access-level non-determinism, not a difference in how carefully the two
reviewers read the same text.

Where **both** reviewers reached `full_text` independently (1 of the 5
overlap items, `10.1016/j.jpowsour.2007.08.077`), agreement was
**exact on every field**, including the `genuine_multi_condition`
classification. The one substantive judgment disagreement in this small
overlap is `10.1016/j.jeurceramsoc.2005.03.032` (item 5's mismatch/match
split on an internally-inconsistent source) -- disclosed there, not
resolved here.

**Read this honestly, not favorably**: n=5 is too small to generalize a
reliability coefficient from, and the access-level non-determinism it
surfaced is itself a real limitation of doing this kind of audit via
automated web research (see item 12) -- a re-run of this exact audit would
not necessarily reach the same DOIs at the same access levels.

## 9. Automatically-applicable fraction

**Best estimate given current information: effectively 0%, and this is a
finding, not a gap.** This audit measures *population-level* base rates
(e.g. "atmosphere fields that could be checked were correct 8/8 times in
this small sample") -- it does not, and cannot, certify any *individual*
observation as correct. Nothing in `LiteratureObservationCorpus`'s schema
carries a per-observation confidence signal, and this audit did not build
one (out of scope, see the "must not add" list in the phase charter). A
future promotion policy that auto-applies observations based only on which
stratum they fall in would be applying a population average to individual
cases it has no way to distinguish -- exactly the risk the owner's original
message about Phase 20C warned against.

## 10. Reference-only fraction

Correspondingly: **100% of the corpus remains reference-only** after this
phase, unchanged from Phase 20B's stance. What this audit adds is not a
promotion mechanism but calibrated priors Phase 20C's design can build on --
e.g. that `temp_disagree` observations are not uniformly untrustworthy (2/3
full-text-confirmed cases were genuine multi-stage conditions, not
misextractions, and the failure mode is a *specific, nameable* segmentation
issue, not random noise), that identity-level corpus errors exist and are
categorically worse than value-level ones, and that any future
per-observation confidence mechanism cannot rely on stratum membership
alone.

## 11. Discovered bugs

Two real issues were caught during this phase's own implementation, both
fixed **before** any judgment was collected against the final rubric (per
the phase's own rule against mixing a fix into the same evaluation set as
results):

1. **Sampler manifest missing raw disagreement candidates.** The wave-0
   pilot's manifest rows told a reviewer *that* 2+ candidate temperatures
   disagreed but not *what they were* -- reviewers could not check "does
   the source confirm A, B, both, or neither" against a specific number.
   Fixed by adding `raw_temperature_candidates_celsius` to
   `sample_literature_observation_audit.py`'s manifest rows before wave 1
   was drawn.

   This gap was checked against wave 0's actual collected judgments, not
   assumed harmless: in all 3 full-text `temp_disagree` pilot cases, the
   reviewer located the real candidate values by reading the source
   directly (e.g. finding "900°C then 1000°C" in the text itself), not by
   cross-checking against a pre-supplied list -- so the missing field did
   not corrupt the 2 `genuine_multi_condition` classifications in item 6.
   The one case where the gap plausibly mattered,
   `10.1016/j.ssi.2017.03.028`, is exactly the one that landed in
   `insufficient_evidence`/a hedged field verdict rather than a confident
   wrong one (item 5) -- the missing information produced appropriate
   caution, not a miscount. Given this, wave 0's judgments are included in
   every metric on equal footing with wave 1's; `field_accuracy()` and
   `multi_entry_cause_tally()` in `audit_literature_observations.py` do
   not filter by wave, and this paragraph is the record of why that's
   safe rather than a silently-applied gap.
2. **`(min_value, max_value)` defaulting bug in the raw-candidate
   extractor.** `raw_temperature_candidates()`'s first draft used
   `entry.get("min_value", entry.get("max_value"))` -- a no-op when the key
   is present but `None` (the common case for a single-sided reading),
   raising `TypeError: '<' not supported between instances of 'float' and
   'NoneType'` on the first real run. Fixed to explicit `None`-coalescing
   (`mn if mn is not None else mx`), matching the same pattern
   `build_literature_observation_snapshot.py`'s `resolved_range` already
   used -- caught immediately by running the sampler, not discovered
   downstream.

No corpus-level (Rust) loader/parser bug was found -- both discovered
issues were in this phase's own new Python tooling, not in
`LiteratureObservationCorpus` or the Phase 20B snapshot builder.

## 12. Limitations

- **Small n throughout.** 58 DOIs sampled, only 22 (37.9%) reached any
  usable text, only 5 (8.6%) reached full text. Every confidence interval
  in this document is correspondingly wide. Treat every percentage here as
  "consistent with," never as a precise measurement -- and see item 9 for
  why per-observation certification isn't available at any sample size
  without a different kind of mechanism.
- **Access-level non-determinism** (item 8): the same DOI can resolve to
  different access levels across independent attempts, because publisher
  bot-detection and repository-mirror availability are not deterministic.
  A re-run of this audit with the same seed would draw the same DOIs but
  might not reach the same access levels for all of them.
- **`full_text` in this sample never meant the publisher's own copy.**
  Every full-text hit came from an author's thesis, an institutional
  repository, or PMC -- never ScienceDirect/Wiley/ACS/IOPscience directly.
  Two of those cases (`10.1016/j.ssc.2009.01.002`,
  `10.1016/j.jeurceramsoc.2005.03.032`) relied specifically on a
  same-author thesis chapter as a proxy for the published article's actual
  text -- a reasonable substitute (the author is the same person describing
  the same experiment) but not literally the peer-reviewed, published
  version of record.
- **`multi_entry_cause`'s n=3 is too small to characterize the 1,063-strong
  `temp_disagree` population.** The step-segmentation mechanism named in
  item 6 is confirmed real, not confirmed dominant at corpus scale.
- **Identity accuracy's "excl. unverifiable" framing (93.2%) should not be
  quoted alone** -- 14 of 58 items could not be confirmed as the right
  record at all beyond a plausible-sounding title, and the conservative
  framing (70.7%) treats that honestly as a real gap in what this audit
  established, not noise to discard.
- **No copyrighted paper text is stored anywhere in this phase's
  deliverables** -- `benchmarks/data/literature_observation_audit_judgments.json`
  and `..._second_pass.json` contain only DOIs, wave/stratum tags, and
  judgment enums, matching the redistributable-data constraint given for
  this phase. Evidence for each verdict lived only in each review agent's
  working context and this document's own short paraphrases (no quote
  longer than the rubric's own ~8-word limit).

## 13. Go/no-go decision for Phase 20C

**No invented numeric threshold** -- consistent with this project's
anti-fabrication discipline, and because a single number cannot honestly
summarize confidence intervals this wide. The go/no-go question this audit
actually answers is the owner's own framing: *"間違った観測をきれいに決定論的に統合しても、
それは精密に間違う装置です"* (cleanly, deterministically integrating wrong
observations just produces a device that is precisely wrong) -- so the
real question is whether this audit found evidence that Phase 20C's
conflict-classification work has something real to classify, and whether it
now knows more about what to look for than "take the max" guesswork would.

**Go**, on both counts, with the audit's findings directly shaping scope:

1. **Genuine multi-condition disagreements are real and have a specific,
   nameable mechanism** (item 6): sequential/parallel multi-stage
   treatments merged into one operation. Phase 20C's classification work
   should look for this pattern specifically (e.g. "do the 2+ candidates
   differ by an amount and direction consistent with a calcine-then-sinter
   schedule?"), not treat every disagreement as unstructured noise to
   average away.
2. **Identity-level corpus errors exist and are categorically different
   from value-level disagreements** (item 3): 3 confirmed cases where the
   record itself doesn't describe the claimed reaction. Phase 20C's design
   must treat "is this even a real, self-consistent route" as a
   precondition checked *before* any conflict resolution across records --
   a conflict resolver has no business reconciling conditions between
   records that were never the same experiment.
3. **No per-observation confidence signal exists or was built here**
   (items 9-10) -- Phase 20C cannot assume it can auto-promote a subset of
   "trustworthy-looking" observations; it must either design one, or accept
   that promotion stays gated on something other than stratum membership.
4. **Sample sizes here are too small to parameterize a policy directly** --
   Phase 20C should treat this audit as evidence that its problem is real
   and structured, not as a source of calibrated thresholds to plug in.

## 14. Quality gates

This phase's deliverables are Python tooling and documentation only --
`sample_literature_observation_audit.py` and
`audit_literature_observations.py` are new offline scripts, not part of the
Rust crate's build. No `src/` changes, no `Cargo.toml` changes, no
`cargo` surface touched.

- `python3 benchmarks/sample_literature_observation_audit.py --wave 0 ...`
  and `--wave 1 ...`: both ran clean, manifest reproducible (re-running
  wave 0 with the same seed after the raw-candidates fix reproduced the
  identical 10 DOIs -- see item 11).
- `python3 benchmarks/audit_literature_observations.py`: runs clean,
  writes `benchmarks/data/literature_observation_audit_summary.json`,
  cross-checked by hand against the manifest/judgments row counts (58/58
  matched keys, no orphans either direction).
- No Rust source changed for this phase (`git status --short` and
  `git diff --stat -- src Cargo.toml` both empty before commit), so
  `cargo clippy`/`cargo test` were not re-run. `cargo fmt --all -- --check`
  was run and is clean (trivially, since no `.rs` file changed).
  `cargo package --list --allow-dirty` was run and confirmed the new
  `.py` scripts and this `.md` doc appear in the packaged crate while
  `benchmarks/data/*.json` do not -- verifying Phase 20B's `Cargo.toml`
  `exclude` glob also covers this phase's new data files, which is the
  check that actually backs the redistributable-data promise in item 12.

## 15. PR/commit

Branch `phase20d/literature-observation-accuracy-audit`. See `tasks/todo.md`
for the full internal work-report record; PR body carries the same summary.

**Deliverables**: `benchmarks/sample_literature_observation_audit.py`
(deterministic, seeded sampler); `benchmarks/audit_literature_observations.py`
(aggregator: accessibility, identity accuracy, field accuracy with Wilson
CIs, multi-entry-cause tally, inter-reviewer agreement); `benchmarks/data/literature_observation_audit_manifest.json`
(58-row sampling manifest, both waves, seeds recorded); `benchmarks/data/literature_observation_audit_judgments.json`
(58 first-pass judgments, IDs/enums only); `benchmarks/data/literature_observation_audit_second_pass.json`
(5-item blind overlap review); `benchmarks/data/literature_observation_audit_summary.json`
(computed metrics); this document.

## Reproduction

```
python3 benchmarks/sample_literature_observation_audit.py \
    --local <cached-raw-corpus.json> --wave 0 --seed 20260815 --sizes temp_disagree=10
python3 benchmarks/sample_literature_observation_audit.py \
    --local <cached-raw-corpus.json> --wave 1 --seed 20260815 \
    --sizes temp_disagree=20,atm_controlled=8,fully_resolved=8,baseline=12
# (manual/agent-assisted review against each manifest row's DOI, per the
# rubric described above, produces the judgments file)
python3 benchmarks/audit_literature_observations.py
```

The manifest and judgments files are committed (IDs/judgments only, no
corpus or paper text); the raw Kononova corpus itself is not (see
`benchmarks/build_literature_observation_snapshot.py`'s own docstring for
why -- same live-fetch-and-license-check discipline applies here).
