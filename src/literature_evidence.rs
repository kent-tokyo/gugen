//! Cross-DOI literature evidence types -- always compiled, never gated
//! behind `literature_corpus`, so `Planner`/`SynthesisPlan` never require
//! that feature just to *describe* this kind of evidence. Only code that
//! can actually *produce* a real value (the corpus-backed computation in
//! `literature_observation_conflicts.rs`, `#[cfg(feature =
//! "literature_corpus")]`) needs the feature -- the same split
//! `provider.rs`'s `ThermodynamicProvider` trait already uses against
//! `materials_project_adapter.rs`'s feature-gated concrete
//! implementation. These types originated in Phase 20C
//! (`literature_observation_conflicts.rs`) and moved here for the v0.4.0
//! Integration phase specifically so `Planner`'s report schema doesn't
//! change shape depending on which crate features are enabled.
//!
//! **Reference-only, by construction, not by promise.** Nothing here is
//! ever passed to `score_plan` (`score.rs`) -- `LiteratureRouteEvidence`
//! is attached to `SynthesisPlan` as its own field, never folded into
//! `evidence: Vec<PlanningEvidence>`, `condition_conflicts`, or
//! `process_evidence_provider_consulted`, all three of which are direct
//! `score_plan` inputs. A caller cannot accidentally thread this into
//! scoring the way `PlanningEvidence` can, because the type never
//! appears in `score_plan`'s signature at all.

use crate::composition::Composition;
use crate::process::{Atmosphere, DurationRange, RouteFamily, TemperatureRange};
use std::collections::BTreeSet;

/// The grouping key *within* one route -- target/precursors/route_family
/// are already fixed by the enclosing [`RouteObservationAssessment`], so
/// only the operation shape and position remain. Two step groups with
/// the same `operation_index` but different `heating_operation_count`
/// are never the same key -- see `literature_observation_conflicts`'s
/// module doc comment for why that's the whole point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StepGroupKey {
    pub heating_operation_count: usize,
    pub operation_index: usize,
}

/// One distinct value among 2+ conflicting independent reports, with one
/// representative contributing DOI (the alphabetically-first DOI that
/// reported this exact value, for determinism) -- mirrors
/// `process.rs`'s own `FieldResolution::Conflict` shape, which also
/// keeps one source per distinct value rather than every contributor.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SourcedValue<T> {
    pub value: T,
    pub doi: String,
}

/// One field's cross-DOI comparison result within one [`StepGroupKey`].
/// See `literature_observation_conflicts`'s module doc comment for the
/// full rationale, especially why `InsufficientIndependentSources` and
/// `Unresolved` are distinct (exactly one DOI reported this field vs.
/// zero did) and why a lone value never resolves the field here, unlike
/// Phase 19's `apply_condition_precedents` -- that function answers "do
/// we have any data to fill this slot," this one answers "do
/// independent replications agree," so a single source is insufficient
/// by design, not an oversight.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum CrossDoiFieldStatus<T> {
    Agreement { value: T, source_dois: Vec<String> },
    Conflict { values: Vec<SourcedValue<T>> },
    InsufficientIndependentSources,
    Unresolved,
    SegmentationAmbiguous,
}

/// A positional comparison across independent DOIs, conditioned on one
/// operation shape -- never "the route's step N," always "among
/// independent DOIs whose heating was extracted with this many steps,
/// step N." `source_dois` is every distinct DOI that contributed *any*
/// field at this key (a superset of any one field's own contributors,
/// since not every DOI reports every field).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StepGroupAssessment {
    pub key: StepGroupKey,
    pub source_dois: Vec<String>,
    pub temperature: CrossDoiFieldStatus<TemperatureRange>,
    pub duration: CrossDoiFieldStatus<DurationRange>,
    pub atmosphere: CrossDoiFieldStatus<Atmosphere>,
}

/// One route (target + precursor set + route family) with at least one
/// [`StepGroupAssessment`] backed by 2+ independent DOIs. Routes with no
/// cross-DOI replication anywhere are never emitted -- see
/// `LiteratureObservationCorpus::cross_doi_comparisons`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RouteObservationAssessment {
    pub target: Composition,
    pub precursors: BTreeSet<Composition>,
    pub route_family: RouteFamily,
    /// True iff 2+ distinct *independent DOIs* for this route report
    /// different `heating_operation_count` values -- computed from
    /// *every* independent DOI for this route, not just ones that
    /// cleared the 2-DOI bar for a listed `step_groups` entry, so even a
    /// single differently-shaped outlier DOI is reflected here. Each DOI
    /// is canonicalized to one shape first (its lowest
    /// `corpus_record_index` entry for this route, same tie-break as the
    /// step groups) before comparing, so a single DOI covering two
    /// records with different shapes for the same route can never set
    /// this on its own -- that would be a within-paper artifact, not
    /// independent DOIs disagreeing, exactly the class of thing the
    /// DOI-as-independence-unit rule exists to exclude. An independent,
    /// route-level warning -- never overrides any `step_groups` entry's
    /// field status.
    pub has_multiple_operation_shapes: bool,
    /// Every distinct `heating_operation_count` seen among this route's
    /// independent DOIs (one canonicalized shape per DOI, see
    /// `has_multiple_operation_shapes`), sorted ascending.
    pub observed_operation_counts: Vec<usize>,
    pub step_groups: Vec<StepGroupAssessment>,
}

impl RouteObservationAssessment {
    /// Every distinct DOI contributing to *any* step group in this
    /// assessment -- a single step group's own `source_dois` only covers
    /// that one operation shape; this is the union across all of them.
    pub fn independent_doi_count(&self) -> usize {
        self.step_groups
            .iter()
            .flat_map(|g| &g.source_dois)
            .collect::<BTreeSet<_>>()
            .len()
    }

    /// True iff any field in any step group is a `Conflict` -- used to
    /// decide whether a caller (e.g. `Planner`) should surface a
    /// disagreement warning alongside this evidence.
    pub fn has_any_conflict(&self) -> bool {
        self.step_groups.iter().any(|g| {
            matches!(g.temperature, CrossDoiFieldStatus::Conflict { .. })
                || matches!(g.duration, CrossDoiFieldStatus::Conflict { .. })
                || matches!(g.atmosphere, CrossDoiFieldStatus::Conflict { .. })
        })
    }
}

/// A [`RouteObservationAssessment`] plus the disclosures a consumer needs
/// to not overread it -- what `Planner` attaches to a
/// `SynthesisPlan` when a `LiteratureEvidenceProvider` is configured and
/// finds matching evidence for that plan's exact route. Reference-only:
/// never auto-applied to `ProcessStep` conditions, never fed to
/// `score_plan`, never converted to a `ConditionPrecedent`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LiteratureRouteEvidence {
    pub assessment: RouteObservationAssessment,
    pub limitations: Vec<String>,
}

/// A fixed baseline of disclosures every [`LiteratureRouteEvidence`]
/// carries (Phase 20D's own findings about what this corpus can and
/// cannot certify), plus conditional additions when `assessment` itself
/// shows step-count diversity or a field-level conflict. Deliberately
/// never phrases "more evidence surfaced" as "conditions are more
/// accurate" -- this reports what was found, not a confidence claim.
pub fn literature_evidence_limitations(assessment: &RouteObservationAssessment) -> Vec<String> {
    let mut limitations = vec![
        "population-level base rates only -- Phase 20D's manual audit (58 DOIs) found this \
         corpus cannot certify any individual observation as correct; this is reference-only \
         evidence, never auto-applied to process conditions or score"
            .to_string(),
        "identity-audit status is unknown for the vast majority of DOIs in this corpus -- only \
         58 of 6,370+ DOIs were manually verified against original papers, and 3 confirmed \
         identity-level errors were found among them"
            .to_string(),
        "atmosphere agreement/conflict, when present, excludes free-text values -- most raw \
         atmosphere data in this corpus is unstructured and never contributes to a verdict"
            .to_string(),
    ];
    if assessment.has_multiple_operation_shapes {
        limitations.push(
            "independent literature reports for this exact route describe different numbers \
             of heating steps -- any single operation shape's agreement/conflict reflects only \
             one of several reported process structures for this route, not a single settled \
             procedure"
                .to_string(),
        );
    }
    if assessment.has_any_conflict() {
        limitations.push(
            "at least one field shows independent DOIs reporting different values at the same \
             route and operation position -- disclosed as a real disagreement, never resolved \
             or averaged"
                .to_string(),
        );
    }
    limitations
}
