//! **Experimental, feature-gated (`experimental_grammar`, default off).**
//! Not part of gugen's stable public API: names, signatures, and the set
//! of shipped grammars may change in any 0.x release without a major
//! version bump, per this crate's pre-1.0 SemVer convention. Measured
//! against a real, DOI-grouped dev/eval split
//! (`docs/phase31_pr3_transformation_grammar_audit.md`): the grammar-only
//! and union-with-frequency-prior policies did not recover any
//! independent multi-step target beyond what frequency-prior candidates
//! (`FrequencyPriorGenerator`, stable) already reach -- union was
//! numerically identical to frequency-only on both splits. Not wired
//! into `Planner`. Kept because the module is self-contained and fully
//! tested, as a candidate-proposal primitive future work can build on --
//! not because it currently improves multi-step recall.
//!
//! Phase 31 PR 3: a small, conservative set of hand-written decomposition
//! grammars that propose intermediate candidate compositions for
//! `search_two_step_routes` (`src/multi_step.rs`), as an alternative
//! candidate source to sit alongside `FrequencyPriorGenerator`
//! (Phase 31 PR 2). **A grammar only proposes a candidate composition for
//! the existing search/balance pipeline to try -- it never asserts that a
//! reaction is real.** Every proposal must still survive
//! `search_precursor_sets`'s own `balance()` check before it counts as a
//! recovered route; nothing here bypasses that.
//!
//! This is deliberately *not* a general reaction-rule engine. Four
//! narrow, single-mechanism grammars, each restricted to a signature it
//! can identify exactly from element ratios alone (`Composition` has no
//! formula parser -- see `docs/phase31_pr3_transformation_grammar_audit.md`
//! for why every predicate below is ratio-based, not string-based):
//!
//! [`CarbonateToOxideGrammar`]: `MCO3 -> MO + CO2` per carbonate carbon.
//!
//! [`HydroxideToOxideGrammar`]: `M(OH)n -> MO(n/2) + (n/2) H2O`.
//!
//! [`NitrateToOxideGrammar`]: `M(NO3)n -> MO(n/2) + n NO2`. The byproduct
//! composition is deliberately not fixed -- only the oxide side is
//! proposed; `balance()`'s own `curated_byproducts()` already includes NO2
//! and will settle the byproduct side independently.
//!
//! [`AcidCarbonatePhosphateGrammar`]: `2 H3PO4 + M2CO3 -> 2 MH2PO4 + CO2 +
//! H2O`, restricted to monovalent-metal carbonates paired with phosphoric
//! acid specifically -- the one real case this grammar is modeled on (see
//! the module doc's DOI note). Does not attempt di-/tri-basic phosphate
//! salts or divalent/trivalent carbonate metals; ambiguous cases are
//! skipped, never guessed.
//!
//! Deliberately excluded from this PR (see the owner's own scope list):
//! arbitrary complex-oxide formation, solid solutions, redox grammars,
//! atmosphere-dependent reactions, volatile-element compensation,
//! empirical temperature-range rules, and ungrounded phase
//! transformations.

use crate::composition::{Composition, Element};
use crate::frac::Frac;
use std::collections::BTreeSet;

/// A grammar's stable identity, stamped onto every [`ProposedIntermediate`]
/// it produces. A string newtype, not an enum -- same rationale as
/// `GeneratorId` (`src/candidate_generator.rs`): only 4 of an
/// open-ended family are implemented here, and an enum with unbuilt
/// variants would force a premature `#[non_exhaustive]` decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct GrammarId(pub &'static str);

impl std::fmt::Display for GrammarId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// How certain a proposal's own arithmetic is, never a claim about
/// whether the reaction actually occurs. Declaration order is
/// significant: variants are ordered most- to least-certain, and
/// [`propose_all`] keeps the most-certain evidence class when the same
/// composition is proposed by more than one grammar. `#[non_exhaustive]`
/// because a third category (`Speculative`, for a future looser grammar)
/// is already anticipated but not needed for this PR's four grammars,
/// which are either exact charge/mass-balance derivations
/// (`Stoichiometric`) or a single empirically-motivated heuristic
/// (`CommonDecompositionHeuristic`).
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum GrammarEvidenceClass {
    /// The proposed composition follows exactly from mass or charge
    /// balance on the input(s) -- no product-selectivity assumption.
    Stoichiometric,
    /// The proposed composition is the most common real product of a
    /// known decomposition/substitution pattern, but a different product
    /// is chemically plausible (e.g. a different phosphate basicity).
    CommonDecompositionHeuristic,
}

/// One candidate intermediate composition, as proposed by exactly one
/// grammar from a set of real input precursor compositions. Mirrors
/// `GeneratedCandidate`'s single-source-provenance shape
/// (`src/candidate_generator.rs`) -- combining multiple grammars'
/// output happens one layer up, in [`propose_all`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ProposedIntermediate {
    pub composition: Composition,
    pub grammar: GrammarId,
    pub evidence_class: GrammarEvidenceClass,
    /// A short, fixed, human-readable explanation of the transformation
    /// applied -- never a claim that the source material's actual
    /// synthesis procedure used this step.
    pub rationale: &'static str,
}

/// A composition proposed by one or more grammars, after
/// [`propose_all`] deduplicates identical compositions across grammars
/// and retains every contributing grammar's id.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DedupedProposal {
    pub composition: Composition,
    pub grammars: Vec<GrammarId>,
    pub evidence_class: GrammarEvidenceClass,
    pub rationale: &'static str,
}

/// A transformation grammar: given the real precursor compositions
/// available for a synthesis, proposes candidate intermediate
/// compositions via one fixed, explainable mechanism. Implementations
/// must never assert that a proposal is a real reaction product --
/// only `search_precursor_sets`'s own `balance()` check, applied
/// downstream, decides that.
pub trait TransformationGrammar {
    fn id(&self) -> GrammarId;
    fn propose(&self, precursors: &[Composition]) -> Vec<ProposedIntermediate>;
}

/// Mandatory safety check applied to every proposal before it is
/// trusted, regardless of which grammar produced it: a proposal must
/// not introduce an element absent from every input (no grammar may
/// invent chemistry), must not be identical to one of its own inputs
/// (a no-op transformation adds nothing `search_two_step_routes`'s own
/// direct-search pass hasn't already tried), and must not carry more
/// distinct elements than the combined union of its inputs (it may only
/// ever be drawn *from* the inputs, never exceed their combined
/// vocabulary).
fn validate_proposed_composition(inputs: &[&Composition], proposed: &Composition) -> bool {
    let allowed: BTreeSet<Element> = inputs.iter().flat_map(|c| c.elements()).collect();
    if !proposed.elements().all(|e| allowed.contains(&e)) {
        return false;
    }
    if inputs.iter().any(|c| **c == *proposed) {
        return false;
    }
    proposed.len() <= allowed.len()
}

fn el(symbol: &'static str) -> Element {
    Element::new(symbol).unwrap_or_else(|_| panic!("{symbol} is a valid element symbol"))
}

fn frac(n: i128) -> Frac {
    Frac::new(n, 1).expect("small integer literal never overflows Frac")
}

/// `MCO3 -> MO + CO2` (per carbonate carbon: -1 C, -2 O, all other
/// elements unchanged). Narrowed to compositions with **no hydrogen**,
/// deliberately excluding bicarbonates and other mixed H+C+O species
/// rather than guessing which decomposition applies to them.
pub struct CarbonateToOxideGrammar;

impl TransformationGrammar for CarbonateToOxideGrammar {
    fn id(&self) -> GrammarId {
        GrammarId("carbonate-to-oxide")
    }

    fn propose(&self, precursors: &[Composition]) -> Vec<ProposedIntermediate> {
        let (c, o, h) = (el("C"), el("O"), el("H"));
        let mut out = Vec::new();
        for p in precursors {
            if p.len() < 3 || p.amount_of_frac(h).is_some() {
                continue;
            }
            let (Some(c_amt), Some(o_amt)) = (p.amount_of_frac(c), p.amount_of_frac(o)) else {
                continue;
            };
            let Ok(three_c) = c_amt.checked_mul(frac(3)) else {
                continue;
            };
            if o_amt < three_c {
                continue;
            }
            let Ok(two_c) = c_amt.checked_mul(frac(2)) else {
                continue;
            };
            let Ok(new_o) = o_amt.checked_sub(two_c) else {
                continue;
            };
            if new_o.is_zero() {
                continue;
            }
            let pairs: Vec<(Element, f64)> = p
                .iter()
                .filter(|(e, _)| *e != c)
                .map(|(e, amt)| {
                    if e == o {
                        (e, new_o.to_f64())
                    } else {
                        (e, amt)
                    }
                })
                .collect();
            let Ok(proposed) = Composition::new(pairs) else {
                continue;
            };
            if !validate_proposed_composition(&[p], &proposed) {
                continue;
            }
            out.push(ProposedIntermediate {
                composition: proposed,
                grammar: self.id(),
                evidence_class: GrammarEvidenceClass::Stoichiometric,
                rationale: "carbonate decomposition MCO3 -> MO + CO2: removed all C, removed 2xC \
                    of O, all other elements unchanged",
            });
        }
        out
    }
}

/// `M(OH)n -> MO(n/2) + (n/2) H2O`, identified by an exact O:H = 1:1
/// ratio (every OH- group contributes exactly one O and one H).
/// Narrowed to compositions with **no carbon**, to avoid confusion with
/// hydrated carbonates or other mixed C+O+H species. Requires at least
/// 3 distinct elements (metal + O + H), matching
/// `CarbonateToOxideGrammar`'s own `len() < 3` guard -- without it, a
/// metal-free 2-element O:H=1:1 composition (e.g. H2O2) would pass this
/// grammar's own ratio check and produce a bare-oxygen "proposal" with
/// no metal for the claimed hydroxide-decomposition mechanism to apply
/// to (the same missing-guard bug class `NitrateToOxideGrammar`'s own
/// hydrogen exclusion was added to close, for HNO3).
pub struct HydroxideToOxideGrammar;

impl TransformationGrammar for HydroxideToOxideGrammar {
    fn id(&self) -> GrammarId {
        GrammarId("hydroxide-to-oxide")
    }

    fn propose(&self, precursors: &[Composition]) -> Vec<ProposedIntermediate> {
        let (o, h, c) = (el("O"), el("H"), el("C"));
        let mut out = Vec::new();
        for p in precursors {
            if p.len() < 3 || p.amount_of_frac(c).is_some() {
                continue;
            }
            let (Some(o_amt), Some(h_amt)) = (p.amount_of_frac(o), p.amount_of_frac(h)) else {
                continue;
            };
            if o_amt != h_amt {
                continue;
            }
            let Ok(half_h) = h_amt.checked_div(frac(2)) else {
                continue;
            };
            let Ok(new_o) = o_amt.checked_sub(half_h) else {
                continue;
            };
            if new_o.is_zero() {
                continue;
            }
            let pairs: Vec<(Element, f64)> = p
                .iter()
                .filter(|(e, _)| *e != h)
                .map(|(e, amt)| {
                    if e == o {
                        (e, new_o.to_f64())
                    } else {
                        (e, amt)
                    }
                })
                .collect();
            let Ok(proposed) = Composition::new(pairs) else {
                continue;
            };
            if !validate_proposed_composition(&[p], &proposed) {
                continue;
            }
            out.push(ProposedIntermediate {
                composition: proposed,
                grammar: self.id(),
                evidence_class: GrammarEvidenceClass::Stoichiometric,
                rationale: "hydroxide decomposition M(OH)n -> MO(n/2) + (n/2) H2O: removed all \
                    H, removed half of O, all other elements unchanged",
            });
        }
        out
    }
}

/// `M(NO3)n -> MO(n/2) + n "NOx"` -- the oxide side only, derived from
/// charge balance (`n` positive charges on M need `n/2` O2-), never
/// fixing which nitrogen oxide leaves (`balance()`'s own
/// `curated_byproducts()` already includes NO2 and settles that side
/// independently). Narrowed to compositions with **exactly one**
/// non-N/non-O/non-H element and **no hydrogen** -- excluding hydrogen
/// specifically rules out nitric acid (`HNO3`) and hydrated nitrates,
/// which are not metal nitrates and for which "the metal" would
/// otherwise wrongly resolve to hydrogen itself.
pub struct NitrateToOxideGrammar;

impl TransformationGrammar for NitrateToOxideGrammar {
    fn id(&self) -> GrammarId {
        GrammarId("nitrate-to-oxide")
    }

    fn propose(&self, precursors: &[Composition]) -> Vec<ProposedIntermediate> {
        let (n, o, h) = (el("N"), el("O"), el("H"));
        let mut out = Vec::new();
        for p in precursors {
            if p.amount_of_frac(h).is_some() {
                continue;
            }
            let others: Vec<Element> = p.elements().filter(|e| *e != n && *e != o).collect();
            if others.len() != 1 {
                continue;
            }
            let metal = others[0];
            let (Some(n_amt), Some(o_amt), Some(metal_amt)) = (
                p.amount_of_frac(n),
                p.amount_of_frac(o),
                p.amount_of_frac(metal),
            ) else {
                continue;
            };
            let Ok(three_n) = n_amt.checked_mul(frac(3)) else {
                continue;
            };
            if o_amt != three_n {
                continue; // not an exact nitrate (NO3) signature
            }
            let Ok(new_o) = n_amt.checked_div(frac(2)) else {
                continue;
            };
            let Ok(proposed) = Composition::new([(metal, metal_amt.to_f64()), (o, new_o.to_f64())])
            else {
                continue;
            };
            if !validate_proposed_composition(&[p], &proposed) {
                continue;
            }
            out.push(ProposedIntermediate {
                composition: proposed,
                grammar: self.id(),
                evidence_class: GrammarEvidenceClass::Stoichiometric,
                rationale: "nitrate decomposition M(NO3)n -> MO(n/2) + n NOx: oxide side derived \
                    from charge balance only; nitrogen byproduct composition not fixed",
            });
        }
        out
    }
}

/// `2 H3PO4 + M2CO3 -> 2 MH2PO4 + CO2 + H2O`, restricted to phosphoric
/// acid (exact H:P:O = 3:1:4 signature, no other elements) paired with a
/// monovalent-metal carbonate (exact M:C = 2:1, no hydrogen, and an
/// **exact** O:C = 3:1 -- deliberately stricter than
/// [`CarbonateToOxideGrammar`]'s own carbonate check, which accepts
/// O:C >= 3:1 to also correctly handle oxycarbonate-like compositions
/// with extra oxide oxygen beyond the carbonate group; this grammar
/// produces a fixed-formula guess rather than a pure mass-balance
/// derivation, so the tighter, exact signature is the safer choice
/// here). The proposed
/// composition is always the fixed monobasic-phosphate formula unit
/// `MH2PO4`, independent of the pair's actual relative amounts (matching
/// every other grammar here: a formula-unit-shaped candidate, not a
/// balanced-reaction claim). Does not attempt di-/tri-basic phosphate
/// salts, does not handle divalent/trivalent carbonate metals, and does
/// not generalize to other oxo-acids despite the "acid+carbonate"
/// pattern -- narrowed deliberately to the one real case this grammar is
/// modeled on (DOI 10.1016/j.tca.2014.08.028, see this phase's own doc).
pub struct AcidCarbonatePhosphateGrammar;

impl TransformationGrammar for AcidCarbonatePhosphateGrammar {
    fn id(&self) -> GrammarId {
        GrammarId("acid-carbonate-phosphate")
    }

    fn propose(&self, precursors: &[Composition]) -> Vec<ProposedIntermediate> {
        let (h, p_el, o, c) = (el("H"), el("P"), el("O"), el("C"));
        let is_phosphoric_acid = |comp: &Composition| -> bool {
            if comp.len() != 3 {
                return false;
            }
            let (Some(h_amt), Some(p_amt), Some(o_amt)) = (
                comp.amount_of_frac(h),
                comp.amount_of_frac(p_el),
                comp.amount_of_frac(o),
            ) else {
                return false;
            };
            h_amt == p_amt.checked_mul(frac(3)).unwrap_or(frac(0))
                && o_amt == p_amt.checked_mul(frac(4)).unwrap_or(frac(0))
        };
        let monovalent_carbonate_metal = |comp: &Composition| -> Option<(Element, Frac)> {
            if comp.len() != 3 || comp.amount_of_frac(h).is_some() {
                return None;
            }
            let others: Vec<Element> = comp.elements().filter(|e| *e != c && *e != o).collect();
            if others.len() != 1 {
                return None;
            }
            let metal = others[0];
            let (Some(c_amt), Some(o_amt), Some(metal_amt)) = (
                comp.amount_of_frac(c),
                comp.amount_of_frac(o),
                comp.amount_of_frac(metal),
            ) else {
                return None;
            };
            if o_amt != c_amt.checked_mul(frac(3)).ok()? {
                return None; // not an exact carbonate signature
            }
            if metal_amt != c_amt.checked_mul(frac(2)).ok()? {
                return None; // not monovalent (M:C must be exactly 2:1)
            }
            Some((metal, metal_amt))
        };

        let mut out = Vec::new();
        for (i, a) in precursors.iter().enumerate() {
            for (j, b) in precursors.iter().enumerate() {
                if i == j {
                    continue;
                }
                if !is_phosphoric_acid(a) {
                    continue;
                }
                let Some((metal, _)) = monovalent_carbonate_metal(b) else {
                    continue;
                };
                let Ok(proposed) =
                    Composition::new([(metal, 1.0), (h, 2.0), (p_el, 1.0), (o, 4.0)])
                else {
                    continue;
                };
                if !validate_proposed_composition(&[a, b], &proposed) {
                    continue;
                }
                out.push(ProposedIntermediate {
                    composition: proposed,
                    grammar: self.id(),
                    evidence_class: GrammarEvidenceClass::CommonDecompositionHeuristic,
                    rationale: "acid+carbonate monobasic phosphate salt formation: \
                        2 H3PO4 + M2CO3 -> 2 MH2PO4 + CO2 + H2O; other phosphate basicities \
                        (M2HPO4, M3PO4) are chemically plausible but not proposed",
                });
            }
        }
        out
    }
}

/// Every grammar shipped in this PR, in a fixed order (used by
/// `propose_all`'s callers, e.g. the PR 3 benchmark harness, so "grammar
/// -only" runs are reproducible without hand-listing grammars at each
/// call site).
pub fn default_grammars() -> Vec<Box<dyn TransformationGrammar>> {
    vec![
        Box::new(CarbonateToOxideGrammar),
        Box::new(HydroxideToOxideGrammar),
        Box::new(NitrateToOxideGrammar),
        Box::new(AcidCarbonatePhosphateGrammar),
    ]
}

/// Runs every grammar in `grammars` against `precursors`, caps each
/// grammar's own raw output at `per_grammar_cap`, then deduplicates
/// identical compositions across grammars -- retaining every
/// contributing grammar's id and the most-certain evidence class seen --
/// sorted most-certain/most-corroborated first and capped combined at
/// `combined_cap`. Mirrors `CandidateGeneratorEnsemble`'s own
/// per-source-cap-then-merge shape (`src/candidate_generator.rs`).
pub fn propose_all(
    grammars: &[Box<dyn TransformationGrammar>],
    precursors: &[Composition],
    per_grammar_cap: usize,
    combined_cap: usize,
) -> Vec<DedupedProposal> {
    use std::collections::BTreeMap;

    let mut by_composition: BTreeMap<
        Composition,
        (BTreeSet<GrammarId>, GrammarEvidenceClass, &'static str),
    > = BTreeMap::new();
    for grammar in grammars {
        let mut proposals = grammar.propose(precursors);
        proposals.truncate(per_grammar_cap);
        for proposal in proposals {
            by_composition
                .entry(proposal.composition.clone())
                .and_modify(|(ids, evidence_class, _)| {
                    ids.insert(proposal.grammar);
                    if proposal.evidence_class < *evidence_class {
                        *evidence_class = proposal.evidence_class;
                    }
                })
                .or_insert_with(|| {
                    let mut ids = BTreeSet::new();
                    ids.insert(proposal.grammar);
                    (ids, proposal.evidence_class, proposal.rationale)
                });
        }
    }

    let mut out: Vec<DedupedProposal> = by_composition
        .into_iter()
        .map(
            |(composition, (ids, evidence_class, rationale))| DedupedProposal {
                composition,
                grammars: ids.into_iter().collect(),
                evidence_class,
                rationale,
            },
        )
        .collect();
    out.sort_by(|a, b| {
        a.evidence_class
            .cmp(&b.evidence_class)
            .then_with(|| b.grammars.len().cmp(&a.grammars.len()))
            .then_with(|| a.composition.cmp(&b.composition))
    });
    out.truncate(combined_cap);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comp(pairs: &[(&'static str, f64)]) -> Composition {
        Composition::new(pairs.iter().map(|(s, a)| (el(s), *a))).unwrap()
    }

    #[test]
    fn carbonate_to_oxide_strips_carbon_and_two_thirds_of_oxygen() {
        let caco3 = comp(&[("Ca", 1.0), ("C", 1.0), ("O", 3.0)]);
        let out = CarbonateToOxideGrammar.propose(std::slice::from_ref(&caco3));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].composition, comp(&[("Ca", 1.0), ("O", 1.0)]));
        assert_eq!(out[0].evidence_class, GrammarEvidenceClass::Stoichiometric);
    }

    #[test]
    fn carbonate_to_oxide_handles_a_non_unit_metal_ratio() {
        // La2(CO3)3 -> La2O3 + 3 CO2
        let la2co33 = comp(&[("La", 2.0), ("C", 3.0), ("O", 9.0)]);
        let out = CarbonateToOxideGrammar.propose(std::slice::from_ref(&la2co33));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].composition, comp(&[("La", 2.0), ("O", 3.0)]));
    }

    #[test]
    fn carbonate_to_oxide_skips_bicarbonate_like_hydrogen_bearing_compositions() {
        let bicarbonate = comp(&[("K", 1.0), ("H", 1.0), ("C", 1.0), ("O", 3.0)]);
        assert!(
            CarbonateToOxideGrammar
                .propose(std::slice::from_ref(&bicarbonate))
                .is_empty()
        );
    }

    #[test]
    fn carbonate_to_oxide_skips_a_composition_with_too_little_oxygen() {
        // O:C < 3, not a carbonate signature
        let not_carbonate = comp(&[("Ca", 1.0), ("C", 1.0), ("O", 2.0)]);
        assert!(
            CarbonateToOxideGrammar
                .propose(std::slice::from_ref(&not_carbonate))
                .is_empty()
        );
    }

    #[test]
    fn hydroxide_to_oxide_strips_hydrogen_and_half_of_oxygen() {
        let caoh2 = comp(&[("Ca", 1.0), ("O", 2.0), ("H", 2.0)]);
        let out = HydroxideToOxideGrammar.propose(std::slice::from_ref(&caoh2));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].composition, comp(&[("Ca", 1.0), ("O", 1.0)]));
    }

    #[test]
    fn hydroxide_to_oxide_skips_a_composition_without_exact_one_to_one_o_h() {
        let not_hydroxide = comp(&[("Na", 1.0), ("O", 1.0), ("H", 3.0)]);
        assert!(
            HydroxideToOxideGrammar
                .propose(std::slice::from_ref(&not_hydroxide))
                .is_empty()
        );
    }

    #[test]
    fn hydroxide_to_oxide_skips_a_metal_free_composition_instead_of_treating_it_as_a_hydroxide() {
        // H2O2 has an exact O:H = 1:1 ratio and no carbon -- it would
        // otherwise pass this grammar's own ratio check with no metal
        // for the claimed hydroxide-decomposition mechanism to apply
        // to, producing a nonsensical bare-oxygen "proposal".
        let h2o2 = comp(&[("O", 2.0), ("H", 2.0)]);
        assert!(
            HydroxideToOxideGrammar
                .propose(std::slice::from_ref(&h2o2))
                .is_empty(),
            "a metal-free O:H=1:1 composition must not be treated as a metal hydroxide"
        );
    }

    #[test]
    fn nitrate_to_oxide_derives_the_oxide_from_charge_balance() {
        let ca_no3_2 = comp(&[("Ca", 1.0), ("N", 2.0), ("O", 6.0)]);
        let out = NitrateToOxideGrammar.propose(std::slice::from_ref(&ca_no3_2));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].composition, comp(&[("Ca", 1.0), ("O", 1.0)]));
    }

    #[test]
    fn nitrate_to_oxide_handles_a_trivalent_metal() {
        let fe_no3_3 = comp(&[("Fe", 1.0), ("N", 3.0), ("O", 9.0)]);
        let out = NitrateToOxideGrammar.propose(std::slice::from_ref(&fe_no3_3));
        assert_eq!(out.len(), 1);
        // Fe:O = 1:1.5, i.e. Fe2O3's ratio
        assert_eq!(out[0].composition, comp(&[("Fe", 1.0), ("O", 1.5)]));
    }

    #[test]
    fn nitrate_to_oxide_skips_multi_metal_compositions() {
        let ambiguous = comp(&[("Ca", 1.0), ("K", 1.0), ("N", 3.0), ("O", 9.0)]);
        assert!(
            NitrateToOxideGrammar
                .propose(std::slice::from_ref(&ambiguous))
                .is_empty()
        );
    }

    #[test]
    fn nitrate_to_oxide_skips_nitric_acid_instead_of_treating_hydrogen_as_the_metal() {
        // HNO3 has H:1, N:1, O:3 -- exactly one non-N/non-O element (H) and
        // an exact O=3N nitrate signature, so a naive "exactly one other
        // element" predicate would wrongly resolve "the metal" to hydrogen
        // and propose a nonsensical H:1,O:0.5 composition. Real corpus data
        // (kononova_high_arity_sample.jsonl) contains HNO3 as a precursor,
        // which is how this was caught.
        let hno3 = comp(&[("H", 1.0), ("N", 1.0), ("O", 3.0)]);
        assert!(
            NitrateToOxideGrammar
                .propose(std::slice::from_ref(&hno3))
                .is_empty()
        );
    }

    #[test]
    fn acid_carbonate_phosphate_proposes_the_monobasic_salt() {
        let h3po4 = comp(&[("H", 3.0), ("P", 1.0), ("O", 4.0)]);
        let k2co3 = comp(&[("K", 2.0), ("C", 1.0), ("O", 3.0)]);
        let out = AcidCarbonatePhosphateGrammar.propose(&[h3po4, k2co3]);
        assert_eq!(out.len(), 1);
        assert_eq!(
            out[0].composition,
            comp(&[("K", 1.0), ("H", 2.0), ("P", 1.0), ("O", 4.0)])
        );
        assert_eq!(
            out[0].evidence_class,
            GrammarEvidenceClass::CommonDecompositionHeuristic
        );
    }

    #[test]
    fn acid_carbonate_phosphate_skips_a_divalent_carbonate_metal() {
        let h3po4 = comp(&[("H", 3.0), ("P", 1.0), ("O", 4.0)]);
        let caco3 = comp(&[("Ca", 1.0), ("C", 1.0), ("O", 3.0)]);
        assert!(
            AcidCarbonatePhosphateGrammar
                .propose(&[h3po4, caco3])
                .is_empty()
        );
    }

    #[test]
    fn validate_rejects_a_proposal_that_invents_an_element() {
        let input = comp(&[("Ca", 1.0), ("C", 1.0), ("O", 3.0)]);
        let invented = comp(&[("Ca", 1.0), ("N", 1.0)]);
        assert!(!validate_proposed_composition(&[&input], &invented));
    }

    #[test]
    fn validate_rejects_a_no_op_identical_to_its_own_input() {
        let input = comp(&[("Ca", 1.0), ("O", 1.0)]);
        assert!(!validate_proposed_composition(&[&input], &input.clone()));
    }

    #[test]
    fn propose_all_dedups_across_grammars_and_keeps_every_contributor() {
        // A composition producible by two different single-input grammars
        // from two different real precursors should appear once, with
        // both grammar ids retained.
        let caco3 = comp(&[("Ca", 1.0), ("C", 1.0), ("O", 3.0)]);
        let caoh2 = comp(&[("Ca", 1.0), ("O", 2.0), ("H", 2.0)]);
        let grammars = default_grammars();
        let out = propose_all(&grammars, &[caco3, caoh2], 50, 200);
        let cao = comp(&[("Ca", 1.0), ("O", 1.0)]);
        let hit = out.iter().find(|p| p.composition == cao).unwrap();
        assert_eq!(hit.grammars.len(), 2);
        assert_eq!(hit.evidence_class, GrammarEvidenceClass::Stoichiometric);
    }

    #[test]
    fn propose_all_respects_the_combined_cap() {
        let mut precursors = Vec::new();
        for i in 1..=10 {
            precursors.push(comp(&[
                ("Ca", i as f64),
                ("C", i as f64),
                ("O", 3.0 * i as f64),
            ]));
        }
        let grammars: Vec<Box<dyn TransformationGrammar>> = vec![Box::new(CarbonateToOxideGrammar)];
        let out = propose_all(&grammars, &precursors, 50, 3);
        assert_eq!(out.len(), 3);
    }
}
