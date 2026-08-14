# Evidence Model (Phase 0 design)

Restates and elaborates AGENTS.md §7. This is the design gugen's implementer
must hold to when Phase 1 turns `EvidenceKind`/`PlanningEvidence` into real
types.

## Core rule

Every proposed number, precursor, or step must point at *why* it's there.
"Why" is one of a closed set of kinds (`EvidenceKind`, AGENTS.md §7):

`StoichiometricBalance`, `RuleBased`, `ThermodynamicData`,
`UserProvidedPrecedent`, `CuratedLiteratureRecord`, `SimilarComposition`,
`SimilarStructure`, `ProcessTemplate`, `SafetyConstraint`.

If nothing in that list applies, the evidence kind is `RuleBased`/heuristic
with `source_id: None` and this must be stated explicitly (`source: none`),
never omitted silently — a missing evidence field is not the same as
"no evidence," and the schema should not allow that ambiguity.

## No fabricated citations — applies to docs too

AGENTS.md §4.1/§7 forbid inventing DOIs, paper titles, patent numbers, or
URLs. This project's own Phase 0 docs are held to the same rule: everything
cited in `docs/competitors.md` was fetched from crates.io, the GitHub API,
or live web search on 2026-08-13, not recalled from training data. The same
discipline applies to `ProcessEvidenceProvider` implementations later —
a provider that can't verify a source must return no evidence, not a
plausible-sounding one.

## Confidence is decomposed, not a single number

A reaction can be stoichiometrically certain while its firing temperature
is completely unresolved. `ConfidenceAssessment` (AGENTS.md §16) keeps these
independent (`stoichiometry`, `precursor_selection`, `process_conditions`,
`evidence_coverage`, `overall`) specifically so a plan doesn't get an
undeserved low score on `overall` for being honest about one weak spot, nor
an undeserved high score by having its weak spot averaged away.

## Provenance is mandatory, not diagnostic-only

Every `SynthesisPlanningReport` carries a `PlanningProvenance` recording
gugen version, commit/build id, schema version, chematic-crystal version (if
used), mikiwame version (if enabled), precursor-catalog version,
thermodynamic-provider version, process-template version, ranking-config
digest, execution timestamp, deterministic seed, and enabled features
(AGENTS.md §7). This is what makes "why did this plan change between runs"
answerable without re-deriving it from logs.

## Rejected candidates are evidence too

A candidate that was *not* selected still needs a reason code from the
closed `RejectionCode` set (AGENTS.md §14). `THERMODYNAMIC_DATA_UNAVAILABLE`
specifically must not auto-reject — missing data is a confidence penalty or
warning, not proof of non-viability (AGENTS.md §13, §14). Reason codes are
themselves subject to the false-confidence audit in §22/§29: a planner that
always emits the same reason code regardless of actual cause is a bug, not
a feature.
