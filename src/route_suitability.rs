use crate::composition::{Composition, Element};
use crate::error::ProviderError;
use crate::evidence::{EvidenceScope, EvidenceStrength};
use crate::process::RouteFamily;
use crate::provider::RouteSuitabilityProvider;

/// AGENTS.md §4.3: thermodynamic favorability alone must not be read as
/// experimental likelihood. This type extends the same separation to
/// route-family choice -- a `Contradicts` verdict is one specific, sourced
/// reason to question a route, never a computed probability. `#[non_exhaustive]`
/// since this is a genuinely new, still-evolving vocabulary (Phase 15A) --
/// none of gugen's other public enums have this guard, but this one is
/// deliberately started with it rather than retrofitted later.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum SuitabilityVerdict {
    Supports,
    Contradicts,
    /// A specific, investigated question whose evidence is ambiguous --
    /// distinct from an assessment with zero `findings`, which means
    /// nothing was investigated at all (`insufficient_evidence`, not this).
    Unknown,
}

/// One independent piece of evidence about whether `route_family` suits a
/// target. Reuses `EvidenceStrength`/`EvidenceScope` from `evidence.rs`
/// rather than inventing parallel enums (AGENTS.md §7's closed-vocabulary
/// discipline applies here too).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SuitabilityFinding {
    pub verdict: SuitabilityVerdict,
    pub statement: String,
    pub source_id: Option<String>,
    pub strength: EvidenceStrength,
    pub applicable_to: EvidenceScope,
    pub limitations: Vec<String>,
}

/// One route family's suitability picture for a target -- deliberately a
/// `Vec`, never an aggregated single verdict, so contradictory findings are
/// never force-merged (the owner's explicit Phase 15A instruction). Empty
/// `findings` means `insufficient_evidence`, not "route rejected" -- absence
/// of evidence must never be read as evidence of unsuitability (AGENTS.md
/// §13's existing rule -- no evidence lowers confidence, it doesn't reject
/// -- applied to route suitability specifically). Nothing in `score.rs`
/// reads this type in Phase 15A: it carries no ranking weight yet.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct RouteSuitabilityAssessment {
    pub route_family: RouteFamily,
    pub findings: Vec<SuitabilityFinding>,
}

/// Discrete recommendation derived from a `RouteSuitabilityAssessment`
/// (Phase 15B) -- never a numeric score. `#[non_exhaustive]`, matching
/// `SuitabilityVerdict`'s precedent. `Recommended` carries no ranking
/// weight in this phase: nothing in `score.rs` reads it, and only
/// `NotRecommended` has a real behavioral effect (`Planner::plan` excludes
/// that plan from the recommended list -- see `derive_recommendation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum RouteRecommendation {
    Recommended,
    NotRecommended,
    InsufficientEvidence,
    ConflictingEvidence,
}

/// Pure function: no `Planner`/provider dependency, safe to unit-test
/// directly against a hand-built `RouteSuitabilityAssessment`. Order- and
/// duplicate-invariant by construction (`any`/`iter` over the whole
/// `findings` slice, not the first match).
///
/// Decision matrix (owner-specified, Phase 15B):
/// - any `Supports` **and** any `Contradicts` present, regardless of
///   strength/scope -> `ConflictingEvidence`. A weak `Contradicts` still
///   flags a real conflict rather than being silently outvoted by a
///   `Supports` finding.
/// - a "strong" `Contradicts` (see below) with **no** `Supports` finding
///   at all (not even a weak one) -> `NotRecommended`. Requiring zero
///   counter-evidence, not just weaker counter-evidence, is the
///   conservative reading of "反証となる支持証拠が存在しないか."
/// - `Supports` only (no `Contradicts`) -> `Recommended` -- an
///   informational label, not a ranking bonus.
/// - everything else (only `Unknown` findings, only sub-threshold
///   `Contradicts`, or no findings at all) -> `InsufficientEvidence`.
///
/// "Strong" `Contradicts` requires **both** `strength != Weak` and
/// `applicable_to == EvidenceScope::ExactTarget`. `ExactTarget` is an
/// allow-list, not a denylist of `SimilarMaterial`/`GeneralRule` --
/// deliberately so a future `EvidenceScope` variant can't silently start
/// counting as "strong enough" just because it isn't named in an
/// exclusion list. This is also the direct enforcement of the owner's
/// "SimilarMaterialは単独でハードなNotRecommendedを発生させない" rule.
pub fn derive_recommendation(assessment: &RouteSuitabilityAssessment) -> RouteRecommendation {
    let has_supports = assessment
        .findings
        .iter()
        .any(|f| f.verdict == SuitabilityVerdict::Supports);
    let has_contradicts = assessment
        .findings
        .iter()
        .any(|f| f.verdict == SuitabilityVerdict::Contradicts);
    let has_actionable_contradicts = assessment.findings.iter().any(|f| {
        f.verdict == SuitabilityVerdict::Contradicts
            && f.strength != EvidenceStrength::Weak
            && f.applicable_to == EvidenceScope::ExactTarget
    });

    if has_supports && has_contradicts {
        RouteRecommendation::ConflictingEvidence
    } else if has_actionable_contradicts {
        RouteRecommendation::NotRecommended
    } else if has_supports {
        RouteRecommendation::Recommended
    } else {
        RouteRecommendation::InsufficientEvidence
    }
}

/// One hand-verified route-suitability record (AGENTS.md §21.3: never
/// authored from memory). Keyed on `(target, route_family)`, not on a
/// precursor set -- unlike `CuratedConditionRecord`
/// (`literature_conditions.rs`), suitability doesn't depend on which
/// precursor combination was chosen.
#[derive(Debug, Clone)]
pub struct CuratedSuitabilityRecord {
    pub target: Composition,
    pub route_family: RouteFamily,
    pub findings: Vec<SuitabilityFinding>,
}

/// `RouteSuitabilityProvider` backed by a small, hand-verified set of real,
/// cited findings (Phase 15A). Deliberately minimal: this phase builds the
/// evidence *vessel*, not a comprehensive suitability database -- see this
/// module's `curated_records()` doc comment for what the two seed records
/// prove and what's intentionally deferred.
pub struct InMemoryRouteSuitabilityProvider {
    records: Vec<CuratedSuitabilityRecord>,
}

impl InMemoryRouteSuitabilityProvider {
    /// Backed by gugen's own curated, hand-verified suitability records.
    pub fn from_curated_records() -> Self {
        Self {
            records: curated_records(),
        }
    }
}

impl RouteSuitabilityProvider for InMemoryRouteSuitabilityProvider {
    /// Matching is exact `Composition` equality (no epsilon/fuzzy matching,
    /// same rule `InMemoryLiteratureConditionProvider` uses) against a
    /// record's `target`, plus an exact `route_family` match.
    fn assess(
        &self,
        target: &Composition,
        route_family: RouteFamily,
    ) -> std::result::Result<Vec<SuitabilityFinding>, ProviderError> {
        let mut out = Vec::new();
        for record in &self.records {
            if &record.target == target && record.route_family == route_family {
                out.extend(record.findings.iter().cloned());
            }
        }
        Ok(out)
    }
}

fn element(symbol: &str) -> Element {
    Element::new(symbol).expect("curated record uses a valid IUPAC element symbol")
}

fn composition(pairs: &[(&str, f64)]) -> Composition {
    Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt)))
        .expect("curated record uses a valid, finite composition")
}

/// Two hand-verified seed records (AGENTS.md §21.3: fetched/read live this
/// phase, not recalled from training data), chosen to prove the
/// `Supports`/`Contradicts` distinction works end to end -- not a
/// comprehensive suitability database. Phase 15B added `derive_recommendation`
/// (the logic that actually acts on these findings) but deliberately did
/// **not** expand this curated set -- the Mg(OH)2 record below is already
/// `ExactTarget`/`Moderate`, strong enough to exercise `NotRecommended`
/// end to end, which was enough to prove the filtering wiring honestly.
/// A broader curated database covering more negative-filtering cases
/// (volatile precursor + long high-temperature firing, air-sensitive
/// target + atmospheric processing, etc.) is left to future work.
///
/// **Supports**: Kozma, Lipták, Deák, Rónavári, Kukovecz, Kónya,
/// "Conversion Study on the Formation of Mechanochemically Synthesized
/// BaTiO3," Chemistry 4(2), 606-616 (2022), DOI 10.3390/chemistry4020042,
/// open access (CC BY) -- abstract read directly (title/authors/venue/year
/// confirmed via CrossRef). States the aim was "the one-step production of
/// BaTiO3 from BaO and TiO2 starting materials," developing "the
/// preparation of BaTiO3 with a perovskite structure even without
/// subsequent heat treatment." A different precursor pair (BaO, not BaCO3)
/// than gugen's own conventional-solid-state BaTiO3 fixture -- irrelevant
/// here, since suitability is keyed on target + route family only, not a
/// specific precursor combination.
///
/// **Contradicts**: Hou, Li, Xudong, Xie, An, "Development and
/// Characterization on the Isothermal Kinetics of Mg(OH)2-sol Synthesized
/// by Chemical Method," Journal of Asian Ceramic Societies (2021), DOI
/// 10.1080/21870764.2021.2019376, open access (CC BY) -- abstract read
/// directly (title/authors/venue confirmed via CrossRef). Reports Mg(OH)2
/// "completely transformed" into cubic MgO at sintering temperatures
/// "higher than 350 C." Cross-referenced (not re-cited fresh) against
/// gugen's own already-curated MgAl2O4 record just above in
/// `literature_conditions.rs`, which cites 1725 K (~1452 C) for
/// conventional solid-state synthesis of a comparable magnesium-oxide-
/// family ceramic -- roughly 1100 C above where Mg(OH)2 has already fully
/// decomposed, so a conventional high-temperature firing route cannot
/// reach Mg(OH)2 as a target phase at all, not merely produce it
/// inefficiently. Not the same failure mode as maghemite (gamma-Fe2O3)
/// vs. hematite (alpha-Fe2O3), a case deliberately rejected while sourcing
/// this record: both are the same `Composition` (Fe2O3) but different
/// polymorphs, which gugen's `Composition` type cannot distinguish --
/// using it here would have wrongly contradicted the common, legitimate
/// hematite-via-conventional-solid-state case too. Mg(OH)2 has no such
/// ambiguity: it is a genuinely different `Composition` from MgO (1:2:2
/// vs. 1:1), not a same-formula polymorph.
fn curated_records() -> Vec<CuratedSuitabilityRecord> {
    vec![
        CuratedSuitabilityRecord {
            target: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
            route_family: RouteFamily::Mechanochemical,
            findings: vec![SuitabilityFinding {
                verdict: SuitabilityVerdict::Supports,
                statement: "Ball milling of BaO + TiO2 was developed to produce BaTiO3 \
                    with a perovskite structure via one-step mechanochemical processing, \
                    even without subsequent heat treatment"
                    .to_string(),
                source_id: Some("10.3390/chemistry4020042".to_string()),
                strength: EvidenceStrength::Moderate,
                applicable_to: EvidenceScope::ExactTarget,
                limitations: vec![
                    "reports a different precursor pair (BaO + TiO2) than gugen's own \
                        conventional-solid-state BaTiO3 fixture (BaCO3 + TiO2); this is not \
                        scope-narrowing (suitability findings are precursor-set independent by \
                        design -- this finding is about BaTiO3 the target, and the paper is \
                        about BaTiO3), only a difference worth disclosing"
                        .to_string(),
                    "single source; not independently cross-validated against a second \
                        paper the way the Contradicts record below is"
                        .to_string(),
                ],
            }],
        },
        CuratedSuitabilityRecord {
            target: composition(&[("Mg", 1.0), ("O", 2.0), ("H", 2.0)]),
            route_family: RouteFamily::ConventionalSolidState,
            findings: vec![SuitabilityFinding {
                verdict: SuitabilityVerdict::Contradicts,
                statement: "Mg(OH)2 fully transforms into cubic MgO at sintering \
                    temperatures above 350 C (sourced). Real-world conventional \
                    solid-state synthesis of comparable magnesium-oxide-family ceramics is \
                    reported in the literature at much higher temperatures -- e.g. ~1452 C \
                    for MgAl2O4 (gugen's own already-curated record, itself sourced) -- so a \
                    conventional high-temperature firing route, as actually practiced, cannot \
                    reach Mg(OH)2 as a target phase at all: the target decomposes long before \
                    reaching temperatures that route family typically requires. This is a \
                    claim about real-world conventional solid-state synthesis, not about \
                    gugen's own ConventionalSolidState template specifically -- that template \
                    leaves Heat step temperature unresolved (None) unless a process-evidence \
                    provider fills it, so it carries no temperature of its own to contradict"
                    .to_string(),
                source_id: Some("10.1080/21870764.2021.2019376".to_string()),
                strength: EvidenceStrength::Moderate,
                applicable_to: EvidenceScope::ExactTarget,
                limitations: vec![
                    "the ~1452 C comparison point is a single data point (gugen's own \
                        already-curated MgAl2O4 record, literature_conditions.rs) for a \
                        different target, not a fresh source measuring Mg(OH)2 and a \
                        solid-state route side by side in one paper, and not a guarantee that \
                        every conventional solid-state route runs that hot"
                        .to_string(),
                    "does not itself state that low-temperature routes (precipitation, \
                        hydrothermal) work for Mg(OH)2 -- only that high-temperature \
                        firing cannot reach it; the positive claim is not sourced here"
                        .to_string(),
                ],
            }],
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AGENTS.md §21.4 (determinism/order-invariance): resolution must not
    /// depend on which of two overlapping records happens to run first --
    /// guaranteed here by making ambiguity impossible by construction, same
    /// approach `literature_conditions.rs` uses.
    #[test]
    fn curated_records_have_no_duplicate_target_route_family_keys() {
        let mut seen: std::collections::BTreeSet<(String, RouteFamily)> =
            std::collections::BTreeSet::new();
        for record in curated_records() {
            let key = (format!("{:?}", record.target), record.route_family);
            assert!(
                seen.insert(key.clone()),
                "duplicate curated suitability record for {key:?} -- two records \
                claiming the same target/route-family makes resolution order-dependent"
            );
        }
    }

    /// The maghemite (gamma-Fe2O3) vs. hematite (alpha-Fe2O3) lesson from
    /// this module's own doc comment, made into a permanent, checkable
    /// regression guard rather than just prose: bare Fe2O3 is a known,
    /// documented trap where two real polymorphs share one `Composition`
    /// gugen cannot distinguish, so a Contradicts finding keyed on it alone
    /// would be misapplied to whichever polymorph the finding doesn't
    /// actually describe. This is a single documented case, not a general
    /// phase-safety checker -- real phase-awareness needs structural data
    /// (Phase 16, `chematic-crystal`), not just a denylist here.
    #[test]
    fn no_curated_record_targets_bare_fe2o3_the_documented_polymorph_trap() {
        let fe2o3 = composition(&[("Fe", 2.0), ("O", 3.0)]);
        for record in curated_records() {
            assert_ne!(
                record.target, fe2o3,
                "bare Fe2O3 is ambiguous between maghemite and hematite -- see this \
                module's doc comment for why a suitability finding must not be keyed \
                on it alone"
            );
        }
    }

    fn finding(
        verdict: SuitabilityVerdict,
        strength: EvidenceStrength,
        applicable_to: EvidenceScope,
    ) -> SuitabilityFinding {
        SuitabilityFinding {
            verdict,
            statement: "test finding".to_string(),
            source_id: None,
            strength,
            applicable_to,
            limitations: vec![],
        }
    }

    fn assessment(findings: Vec<SuitabilityFinding>) -> RouteSuitabilityAssessment {
        RouteSuitabilityAssessment {
            route_family: RouteFamily::ConventionalSolidState,
            findings,
        }
    }

    #[test]
    fn no_findings_at_all_is_insufficient_evidence() {
        assert_eq!(
            derive_recommendation(&assessment(vec![])),
            RouteRecommendation::InsufficientEvidence
        );
    }

    #[test]
    fn only_unknown_findings_is_insufficient_evidence() {
        let a = assessment(vec![finding(
            SuitabilityVerdict::Unknown,
            EvidenceStrength::Strong,
            EvidenceScope::ExactTarget,
        )]);
        assert_eq!(
            derive_recommendation(&a),
            RouteRecommendation::InsufficientEvidence
        );
    }

    #[test]
    fn a_single_strong_exact_target_contradicts_is_not_recommended() {
        let a = assessment(vec![finding(
            SuitabilityVerdict::Contradicts,
            EvidenceStrength::Moderate,
            EvidenceScope::ExactTarget,
        )]);
        assert_eq!(
            derive_recommendation(&a),
            RouteRecommendation::NotRecommended
        );
    }

    #[test]
    fn supports_and_contradicts_coexisting_is_conflicting_evidence_not_excluded() {
        let a = assessment(vec![
            finding(
                SuitabilityVerdict::Supports,
                EvidenceStrength::Weak,
                EvidenceScope::ExactTarget,
            ),
            finding(
                SuitabilityVerdict::Contradicts,
                EvidenceStrength::Strong,
                EvidenceScope::ExactTarget,
            ),
        ]);
        assert_eq!(
            derive_recommendation(&a),
            RouteRecommendation::ConflictingEvidence,
            "a Contradicts finding must not silently win over a coexisting Supports \
            finding -- conflict must be surfaced, not resolved by fiat"
        );
    }

    #[test]
    fn a_weak_contradicts_alongside_supports_still_flags_conflict_not_recommended() {
        // Even a Weak-strength Contradicts must not be silently outvoted by
        // a Supports finding -- ConflictingEvidence, not Recommended.
        let a = assessment(vec![
            finding(
                SuitabilityVerdict::Supports,
                EvidenceStrength::Strong,
                EvidenceScope::ExactTarget,
            ),
            finding(
                SuitabilityVerdict::Contradicts,
                EvidenceStrength::Weak,
                EvidenceScope::SimilarMaterial,
            ),
        ]);
        assert_eq!(
            derive_recommendation(&a),
            RouteRecommendation::ConflictingEvidence
        );
    }

    #[test]
    fn supports_only_is_recommended_as_a_label_only() {
        let a = assessment(vec![finding(
            SuitabilityVerdict::Supports,
            EvidenceStrength::Moderate,
            EvidenceScope::ExactTarget,
        )]);
        assert_eq!(derive_recommendation(&a), RouteRecommendation::Recommended);
    }

    /// The owner's explicit safety condition: `SimilarMaterial` alone must
    /// never produce a hard `NotRecommended`, even at `Strong` strength --
    /// only `ExactTarget` clears the bar. Constructed by hand here, not
    /// added to shipped `curated_records()` (keeping the seed data at two
    /// real worked examples).
    #[test]
    fn a_similar_material_contradicts_alone_is_not_actionable() {
        let a = assessment(vec![finding(
            SuitabilityVerdict::Contradicts,
            EvidenceStrength::Strong,
            EvidenceScope::SimilarMaterial,
        )]);
        assert_eq!(
            derive_recommendation(&a),
            RouteRecommendation::InsufficientEvidence,
            "SimilarMaterial-scoped evidence, however strong, must not alone exclude a \
            route -- only ExactTarget does"
        );
    }

    #[test]
    fn a_weak_exact_target_contradicts_alone_is_not_actionable() {
        let a = assessment(vec![finding(
            SuitabilityVerdict::Contradicts,
            EvidenceStrength::Weak,
            EvidenceScope::ExactTarget,
        )]);
        assert_eq!(
            derive_recommendation(&a),
            RouteRecommendation::InsufficientEvidence
        );
    }

    #[test]
    fn finding_order_does_not_affect_the_derived_recommendation() {
        let forward = assessment(vec![
            finding(
                SuitabilityVerdict::Unknown,
                EvidenceStrength::Weak,
                EvidenceScope::GeneralRule,
            ),
            finding(
                SuitabilityVerdict::Contradicts,
                EvidenceStrength::Strong,
                EvidenceScope::ExactTarget,
            ),
        ]);
        let reversed = assessment(forward.findings.iter().cloned().rev().collect());
        assert_eq!(
            derive_recommendation(&forward),
            derive_recommendation(&reversed)
        );
    }

    #[test]
    fn duplicated_findings_do_not_change_the_derived_recommendation() {
        let single = assessment(vec![finding(
            SuitabilityVerdict::Contradicts,
            EvidenceStrength::Strong,
            EvidenceScope::ExactTarget,
        )]);
        let mut duplicated = single.findings.clone();
        duplicated.extend(single.findings.clone());
        let duplicated = assessment(duplicated);
        assert_eq!(
            derive_recommendation(&single),
            derive_recommendation(&duplicated)
        );
    }

    #[test]
    fn matched_target_and_route_family_returns_real_findings() {
        let provider = InMemoryRouteSuitabilityProvider::from_curated_records();
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let findings = provider
            .assess(&target, RouteFamily::Mechanochemical)
            .unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].verdict, SuitabilityVerdict::Supports);
    }

    #[test]
    fn unmatched_target_returns_empty_findings_not_an_error() {
        let provider = InMemoryRouteSuitabilityProvider::from_curated_records();
        let target = composition(&[("Zn", 1.0), ("O", 1.0)]);
        let findings = provider
            .assess(&target, RouteFamily::Mechanochemical)
            .unwrap();
        assert!(findings.is_empty());
    }

    #[test]
    fn matched_target_wrong_route_family_returns_empty_findings() {
        let provider = InMemoryRouteSuitabilityProvider::from_curated_records();
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let findings = provider
            .assess(&target, RouteFamily::ConventionalSolidState)
            .unwrap();
        assert!(findings.is_empty());
    }
}
