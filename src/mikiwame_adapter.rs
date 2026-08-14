//! Maps `mikiwame`'s structural diagnostics onto gugen-native effects
//! (AGENTS.md §5, docs/integration.md). Feature-gated: with `mikiwame`
//! disabled, this module doesn't exist in the compiled crate.
//!
//! **Not called automatically by [`crate::Planner::plan`].** `mikiwame::
//! analyze` needs a `mikiwame::PeriodicStructureView` (a real lattice +
//! site list), and gugen's own `TargetStructure` is still free text --
//! there is no field on `TargetSpecification` to carry real geometry
//! through the planning pipeline (Phase 16 did not add one; see
//! `crate::target::TargetStructure`'s own doc comment). A caller that has
//! its own structure data (a `mikiwame::OwnedStructure`, its own
//! `PeriodicStructureView` impl, or -- with the `chematic_crystal` feature
//! enabled -- a `chematic_crystal::PeriodicStructure` converted via
//! [`crate::to_mikiwame_structure`]) can run `mikiwame::analyze`, pass the
//! resulting report to [`structural_effects`], and apply the result to a
//! [`crate::SynthesisPlan`] itself: check `abstain_reason` first (a
//! `Some` means mikiwame considers the structure invalid or a strong
//! anomaly -- discard the plan rather than score it), then fold `warnings`
//! into the plan's own warnings and use `confidence_penalty` to lower
//! `SynthesisPlan::confidence` (its fields are public; there is no crate
//! helper for this yet since no caller does it today).
//!
//! docs/integration.md's mapping:
//! - `InvalidInput` / a severe structural anomaly -> stop planning.
//! - Low applicability -> lower confidence, not a hard reject.
//! - Structural anomaly -> a `PlanningWarning`.
//! - Oxidation-state ambiguity -> propagate into plan branching. Mikiwame
//!   v0.1 has no `FindingCode` for this (see its `finding.rs`) -- this
//!   integration point is currently unreachable, not implemented as a
//!   no-op; revisit once mikiwame exposes it.

use crate::report::{PlanningWarning, WarningSeverity};
use crate::score::Score01;

/// Effects one `mikiwame::MaterialDiagnosticReport` should have on gugen
/// planning, decided by a caller that has structure data to analyze.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralDiagnosticEffects {
    /// `Some(reason)` if planning should stop for this target entirely
    /// (mikiwame `Verdict::InvalidInput` or `StrongAnomalyDetected`).
    pub abstain_reason: Option<String>,
    /// One entry per finding more severe than `Info`, plus one if
    /// `report.applicability.level` is an `ApplicabilityLevel` variant
    /// this adapter doesn't recognize yet (mikiwame may add one; that
    /// enum is `#[non_exhaustive]`).
    pub warnings: Vec<PlanningWarning>,
    /// How much this structural assessment should lower confidence,
    /// `0.0` meaning no penalty. Ordinal, not calibrated against real
    /// outcomes yet (no validation corpus exists -- same caveat as
    /// `RankingWeights::default()`, AGENTS.md §13). Meaningful even when
    /// `abstain_reason` is `Some`, but the abstention should take
    /// precedence over acting on this value.
    pub confidence_penalty: Score01,
}

/// Maps one `mikiwame::MaterialDiagnosticReport` to gugen-native effects.
pub fn structural_effects(
    report: &mikiwame::MaterialDiagnosticReport,
) -> StructuralDiagnosticEffects {
    let mut warnings: Vec<PlanningWarning> = report
        .findings
        .iter()
        .filter(|f| f.severity != mikiwame::Severity::Info)
        .map(|f| PlanningWarning {
            message: format!(
                "mikiwame finding {}: {} ({:?} severity)",
                f.code.as_str(),
                f.explanation,
                f.severity
            ),
            severity: map_severity(f.severity),
        })
        .collect();

    let (abstain_reason, verdict_penalty): (Option<String>, f64) = match report.overall.verdict {
        mikiwame::Verdict::StructurallyConsistent => (None, 0.0),
        mikiwame::Verdict::ReviewRecommended => (None, 0.3),
        mikiwame::Verdict::OutOfDomain => (None, 0.5),
        mikiwame::Verdict::StrongAnomalyDetected => (
            Some(
                "mikiwame reported a strong structural anomaly (Verdict::StrongAnomalyDetected)"
                    .to_string(),
            ),
            1.0,
        ),
        mikiwame::Verdict::InvalidInput => (
            Some(
                "mikiwame could not validate the input structure (Verdict::InvalidInput)"
                    .to_string(),
            ),
            1.0,
        ),
        // Unlike Severity/ApplicabilityLevel, mikiwame::Verdict is NOT
        // `#[non_exhaustive]` (checked in its source, model.rs) -- the
        // headline verdict is a deliberately closed set, so this match is
        // genuinely exhaustive with no wildcard needed.
    };

    let applicability_penalty = match report.applicability.level {
        mikiwame::ApplicabilityLevel::FullyApplicable => 0.0,
        mikiwame::ApplicabilityLevel::PartiallyApplicable => 0.3,
        mikiwame::ApplicabilityLevel::LimitedApplicability => 0.6,
        mikiwame::ApplicabilityLevel::NotApplicable => 1.0,
        _ => {
            warnings.push(PlanningWarning {
                message: format!(
                    "mikiwame returned an applicability level this adapter does not \
                    recognize ({:?}); treating it as limited applicability rather than \
                    assuming full applicability",
                    report.applicability.level
                ),
                severity: WarningSeverity::Caution,
            });
            0.6
        }
    };

    // Independent signals about the same structure -- take the worse of
    // the two rather than summing (summing would double-penalize when
    // both point at the same underlying problem) or averaging (which
    // would let a mild applicability level water down a severe verdict).
    let confidence_penalty = verdict_penalty.max(applicability_penalty);

    StructuralDiagnosticEffects {
        abstain_reason,
        warnings,
        confidence_penalty: Score01::new(confidence_penalty)
            .expect("verdict_penalty and applicability_penalty are hardcoded within [0, 1]"),
    }
}

fn map_severity(severity: mikiwame::Severity) -> WarningSeverity {
    match severity {
        mikiwame::Severity::Info => WarningSeverity::Info,
        mikiwame::Severity::Low | mikiwame::Severity::Medium => WarningSeverity::Caution,
        mikiwame::Severity::High | mikiwame::Severity::Critical => WarningSeverity::Severe,
        // mikiwame::Severity is #[non_exhaustive]: default to the most
        // cautious mapping rather than guessing where a new variant sits.
        _ => WarningSeverity::Severe,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mikiwame::{AnalysisConfig, OwnedStructure, Site};

    fn cubic_lattice(a: f64) -> [[f64; 3]; 3] {
        [[a, 0.0, 0.0], [0.0, a, 0.0], [0.0, 0.0, a]]
    }

    fn site(element: &str, frac: [f64; 3]) -> Site {
        site_with_occupancy(element, frac, 1.0)
    }

    fn site_with_occupancy(element: &str, frac: [f64; 3], occupancy: f64) -> Site {
        Site {
            element: element.to_string(),
            fractional: frac,
            occupancy,
        }
    }

    /// A structurally unremarkable rock-salt-like arrangement: no findings
    /// mikiwame's v0.1 checks should flag.
    fn clean_structure() -> OwnedStructure {
        OwnedStructure::new(
            cubic_lattice(5.64),
            vec![site("Na", [0.0, 0.0, 0.0]), site("Cl", [0.5, 0.5, 0.5])],
        )
    }

    #[test]
    fn structurally_consistent_report_proceeds_with_no_penalty() {
        let report = mikiwame::analyze(&clean_structure(), &AnalysisConfig::default());
        let effects = structural_effects(&report);

        assert_eq!(effects.abstain_reason, None);
        assert_eq!(effects.confidence_penalty, Score01::new(0.0).unwrap());
    }

    /// AGENTS.md: "Severe site overlap -> stop planning" /
    /// "InvalidInput finding -> stop planning".
    #[test]
    fn invalid_input_structure_produces_an_abstain_reason() {
        let empty = OwnedStructure::new(cubic_lattice(5.0), vec![]);
        let report = mikiwame::analyze(&empty, &AnalysisConfig::default());

        assert_eq!(report.overall.verdict, mikiwame::Verdict::InvalidInput);
        let effects = structural_effects(&report);

        assert!(effects.abstain_reason.is_some());
        assert_eq!(effects.confidence_penalty, Score01::new(1.0).unwrap());
    }

    /// AGENTS.md: "Structural anomaly -> PlanningWarning". Two sites of
    /// the same element at the same position trigger SITE_DUPLICATE.
    #[test]
    fn a_structural_finding_becomes_a_planning_warning() {
        let duplicate_site = OwnedStructure::new(
            cubic_lattice(5.64),
            vec![
                site("Na", [0.0, 0.0, 0.0]),
                site("Na", [0.0, 0.0, 0.0]),
                site("Cl", [0.5, 0.5, 0.5]),
            ],
        );
        let report = mikiwame::analyze(&duplicate_site, &AnalysisConfig::default());
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == mikiwame::FindingCode::SiteDuplicate)
        );

        let effects = structural_effects(&report);
        assert!(
            effects
                .warnings
                .iter()
                .any(|w| w.message.contains("SITE_DUPLICATE"))
        );
    }

    /// A `High`-severity finding (occupancies summing past 1.0) must
    /// escalate the verdict enough to produce a `Severe` warning, mapped
    /// through `map_severity`.
    #[test]
    fn a_high_severity_finding_maps_to_a_severe_warning() {
        let overfull = OwnedStructure::new(
            cubic_lattice(5.64),
            vec![
                site_with_occupancy("Na", [0.25, 0.25, 0.25], 1.0),
                site_with_occupancy("K", [0.25, 0.25, 0.25], 1.0),
            ],
        );
        let report = mikiwame::analyze(&overfull, &AnalysisConfig::default());
        assert!(report.findings.iter().any(|f| {
            f.code == mikiwame::FindingCode::DisorderOccupancySumExceedsOne
                && f.severity == mikiwame::Severity::High
        }));

        let effects = structural_effects(&report);
        assert!(
            effects
                .warnings
                .iter()
                .any(|w| w.message.contains("DISORDER_OCCUPANCY_SUM_EXCEEDS_ONE")
                    && w.severity == WarningSeverity::Severe)
        );
    }

    #[test]
    fn info_severity_findings_alone_do_not_produce_warnings_or_abstention() {
        // Two different elements sharing a position, each at half
        // occupancy (occupancies sum to 1.0, so DISORDER_OCCUPANCY_SUM_
        // EXCEEDS_ONE does not also fire): DISORDER_PRESENT alone, Info
        // severity per mikiwame's own design (disorder is not itself an
        // anomaly).
        let disordered = OwnedStructure::new(
            cubic_lattice(5.64),
            vec![
                site_with_occupancy("Na", [0.25, 0.25, 0.25], 0.5),
                site_with_occupancy("K", [0.25, 0.25, 0.25], 0.5),
            ],
        );
        let report = mikiwame::analyze(&disordered, &AnalysisConfig::default());
        assert_eq!(
            report.overall.verdict,
            mikiwame::Verdict::StructurallyConsistent,
            "unexpected findings: {:?}",
            report.findings
        );

        let effects = structural_effects(&report);
        assert_eq!(effects.abstain_reason, None);
        assert!(effects.warnings.is_empty());
    }
}
