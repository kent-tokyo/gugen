//! Multi-source candidate generation (Phase 30). `CandidateGenerator`
//! (`src/provider.rs`) is the per-source contract; this module holds every
//! type built on top of it: the provenance-carrying output shape
//! (`GeneratedCandidate`), two real generators (`CatalogExactGenerator`,
//! `FrequencyPriorGenerator`), and the ensemble that combines them
//! (`CandidateGeneratorEnsemble`). PR 1 ships exactly these two generators
//! -- `thermodynamic`, `prior-experiment`, and `literature-analog` are
//! each their own future PR; `chemical-substitution` has no backing data
//! anywhere in this crate and is deferred indefinitely pending a separate
//! owner decision on caller-supplied similarity data.

use crate::composition::{Composition, Element};
use crate::error::ProviderError;
use crate::precursor::{InMemoryPrecursorCatalog, PrecursorCandidate, PrecursorId};
use crate::provider::{CandidateGenerator, PrecursorCatalog};
use crate::target::PlanningConstraints;
use std::collections::{BTreeMap, BTreeSet};

/// A generator's stable identity, stamped onto every [`GeneratedCandidate`]
/// it produces and used to label a failed `generate()` call in
/// [`EnsembleOutput::generator_errors`]. A string newtype rather than an
/// enum: PR 1 only populates 2 of the eventual 6 named generators, and an
/// enum with unbuilt variants would force a premature `#[non_exhaustive]`
/// decision the crate's own API stability policy reserves for types whose
/// doc comment already states a growth expectation. Adding generator #3
/// later never forces a semver decision this way.
///
/// `Serialize` only, deliberately no `Deserialize`: the inner `&'static
/// str` cannot deserialize into a non-`'static` borrow from an arbitrary
/// input buffer -- matches this crate's existing precedent for
/// `&'static str`-bearing output-only types (`CommercialOfferSelection`,
/// `src/commercial_catalog/model.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct GeneratorId(pub &'static str);

impl std::fmt::Display for GeneratorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// One precursor candidate as proposed by exactly one generator. This is
/// where "full provenance" actually lives -- deliberately a wrapper type,
/// not a field added to `PrecursorCandidate` (not `#[non_exhaustive]`,
/// adding a field there would be a breaking change). `rank` is a plain
/// ordinal (0 = the generator's own top pick), never a float/confidence:
/// a generator's internal priority can never be read as a success
/// probability, because the type has no score field to misuse (mirrors
/// `SearchPriority`'s own score/priority separation, `src/precursor.rs`).
///
/// `Serialize` only -- see [`GeneratorId`]'s doc comment (`generator`
/// transitively carries its `&'static str` field).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct GeneratedCandidate {
    pub candidate: PrecursorCandidate,
    pub generator: GeneratorId,
    pub rank: usize,
}

/// Wraps an existing [`InMemoryPrecursorCatalog`] as a `CandidateGenerator`
/// -- "exact" means literally whatever the catalog returns, in its own
/// element-overlap-filtered order, stamped with rank = output position.
/// Near-zero new logic: reuses `InMemoryPrecursorCatalog`'s existing
/// filter/sort/dedup verbatim rather than reimplementing it.
pub struct CatalogExactGenerator {
    catalog: InMemoryPrecursorCatalog,
}

impl CatalogExactGenerator {
    pub fn new(catalog: InMemoryPrecursorCatalog) -> Self {
        Self { catalog }
    }
}

impl CandidateGenerator for CatalogExactGenerator {
    fn id(&self) -> GeneratorId {
        GeneratorId("catalog-exact")
    }

    fn generate(
        &self,
        target: &Composition,
        constraints: &PlanningConstraints,
    ) -> std::result::Result<Vec<GeneratedCandidate>, ProviderError> {
        let candidates = self.catalog.candidates_for(target, constraints)?;
        Ok(candidates
            .into_iter()
            .enumerate()
            .map(|(rank, candidate)| GeneratedCandidate {
                candidate,
                generator: self.id(),
                rank,
            })
            .collect())
    }
}

/// Proposes precursors ranked by a caller-supplied frequency table --
/// never computed or bundled by this crate itself, matching the
/// established "caller supplies the data, core never fetches/bundles it"
/// convention (`ThermodynamicProvider`/`MaterialsProjectSnapshotProvider`'s
/// own precedent). A caller can build the table from anything: their own
/// literature database, `LiteratureObservationCorpus`, or a benchmark's
/// own precursor-formula counts.
pub struct FrequencyPriorGenerator {
    /// Sorted once at construction (descending frequency, ascending
    /// `PrecursorId` as a deterministic tie-break) -- mirrors
    /// `InMemoryPrecursorCatalog::new`'s own "sort once, not per-query"
    /// convention.
    entries: Vec<(PrecursorCandidate, u64)>,
}

impl FrequencyPriorGenerator {
    pub fn new(mut entries: Vec<(PrecursorCandidate, u64)>) -> Self {
        entries.sort_by(|(a, a_freq), (b, b_freq)| {
            b_freq.cmp(a_freq).then_with(|| a.id.0.cmp(&b.id.0))
        });
        Self { entries }
    }
}

impl CandidateGenerator for FrequencyPriorGenerator {
    fn id(&self) -> GeneratorId {
        GeneratorId("frequency-prior")
    }

    fn generate(
        &self,
        target: &Composition,
        _constraints: &PlanningConstraints,
    ) -> std::result::Result<Vec<GeneratedCandidate>, ProviderError> {
        let target_elements: BTreeSet<Element> = target.elements().collect();
        Ok(self
            .entries
            .iter()
            .filter(|(candidate, _frequency)| {
                candidate
                    .composition
                    .elements()
                    .any(|e| target_elements.contains(&e))
            })
            .enumerate()
            .map(|(rank, (candidate, _frequency))| GeneratedCandidate {
                candidate: candidate.clone(),
                generator: self.id(),
                rank,
            })
            .collect())
    }
}

/// Combined output of every generator in a [`CandidateGeneratorEnsemble`]
/// run, per design principle 5 (every branch/provider outcome stays
/// distinguishable, never silently dropped): `candidates` is what a
/// `PrecursorCatalog` caller (e.g. `search_precursor_sets`) actually
/// consumes, `provenance` keeps every generator that proposed each id, and
/// `generator_errors` keeps every generator that failed outright, labeled
/// by which one.
#[derive(Debug, Clone, PartialEq)]
pub struct EnsembleOutput {
    pub candidates: Vec<PrecursorCandidate>,
    pub provenance: BTreeMap<PrecursorId, Vec<GeneratedCandidate>>,
    pub generator_errors: Vec<(GeneratorId, ProviderError)>,
}

/// Combines multiple [`CandidateGenerator`]s into one candidate list via
/// min-rank fusion, and implements [`PrecursorCatalog`] itself -- so an
/// ensemble is a drop-in `Planner::builder` catalog argument, requiring
/// zero changes to `Planner`/`PlannerBuilder`. PR 1 does not wire this
/// into `Planner`; every measurement in this PR calls the ensemble
/// directly, the same way every existing exploration-recall benchmark
/// already bypasses `Planner` and calls `search_precursor_sets` directly.
pub struct CandidateGeneratorEnsemble {
    generators: Vec<Box<dyn CandidateGenerator>>,
}

impl CandidateGeneratorEnsemble {
    pub fn new(generators: Vec<Box<dyn CandidateGenerator>>) -> Self {
        Self { generators }
    }

    /// Runs every generator, catching each one's failure individually
    /// (mirrors the existing `route_suitability_provider` catch-and-
    /// continue loop already in `Planner::plan`, `src/planner.rs`) so one
    /// generator's failure never prevents the others' candidates from
    /// being used.
    ///
    /// **Combination rule -- min-rank fusion**: a candidate's ensemble
    /// rank is the smallest rank any generator gave it, ties broken
    /// alphabetically by `PrecursorId` (matches this crate's existing
    /// determinism discipline, e.g. `search_precursor_sets`'s own
    /// tiebreaks). **Duplicate-id conflicts** (two generators proposing
    /// the same id with different composition/availability data):
    /// first-generator-in-list wins for the payload, matching
    /// `InMemoryPrecursorCatalog::new`'s own stated precedent -- but
    /// every generator that proposed it still appears in `provenance`,
    /// so nothing is silently collapsed.
    pub fn generate_with_provenance(
        &self,
        target: &Composition,
        constraints: &PlanningConstraints,
    ) -> EnsembleOutput {
        let mut best: BTreeMap<PrecursorId, (usize, PrecursorCandidate)> = BTreeMap::new();
        let mut provenance: BTreeMap<PrecursorId, Vec<GeneratedCandidate>> = BTreeMap::new();
        let mut generator_errors = Vec::new();

        for generator in &self.generators {
            match generator.generate(target, constraints) {
                Ok(generated) => {
                    for gc in generated {
                        let id = gc.candidate.id.clone();
                        best.entry(id.clone())
                            .and_modify(|(rank, _payload)| {
                                if gc.rank < *rank {
                                    *rank = gc.rank;
                                }
                            })
                            .or_insert_with(|| (gc.rank, gc.candidate.clone()));
                        provenance.entry(id).or_default().push(gc);
                    }
                }
                Err(err) => generator_errors.push((generator.id(), err)),
            }
        }

        let mut fused: Vec<(usize, PrecursorCandidate)> = best.into_values().collect();
        fused.sort_by(|(rank_a, candidate_a), (rank_b, candidate_b)| {
            rank_a
                .cmp(rank_b)
                .then_with(|| candidate_a.id.0.cmp(&candidate_b.id.0))
        });

        EnsembleOutput {
            candidates: fused
                .into_iter()
                .map(|(_rank, candidate)| candidate)
                .collect(),
            provenance,
            generator_errors,
        }
    }
}

impl PrecursorCatalog for CandidateGeneratorEnsemble {
    fn candidates_for(
        &self,
        target: &Composition,
        constraints: &PlanningConstraints,
    ) -> std::result::Result<Vec<PrecursorCandidate>, ProviderError> {
        Ok(self
            .generate_with_provenance(target, constraints)
            .candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn element(symbol: &str) -> Element {
        Element::new(symbol).unwrap()
    }

    fn composition(pairs: &[(&str, f64)]) -> Composition {
        Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
    }

    fn candidate(id: &str, pairs: &[(&str, f64)]) -> PrecursorCandidate {
        PrecursorCandidate {
            id: PrecursorId(id.to_string()),
            composition: composition(pairs),
            availability: None,
        }
    }

    fn no_constraints() -> PlanningConstraints {
        PlanningConstraints::default()
    }

    fn barium_titanate_target() -> Composition {
        composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)])
    }

    /// Always fails, to exercise `CandidateGeneratorEnsemble`'s per-
    /// generator error handling without needing a second real generator.
    struct AlwaysFailsGenerator;

    impl CandidateGenerator for AlwaysFailsGenerator {
        fn id(&self) -> GeneratorId {
            GeneratorId("always-fails")
        }

        fn generate(
            &self,
            _target: &Composition,
            _constraints: &PlanningConstraints,
        ) -> std::result::Result<Vec<GeneratedCandidate>, ProviderError> {
            Err(ProviderError::Unavailable("test failure".to_string()))
        }
    }

    #[test]
    fn catalog_exact_generator_delegates_and_stamps_rank_by_output_position() {
        let catalog = InMemoryPrecursorCatalog::new(vec![
            candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
            candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]),
        ]);
        let generator = CatalogExactGenerator::new(catalog);

        let generated = generator
            .generate(&barium_titanate_target(), &no_constraints())
            .unwrap();

        // NaCl shares no element with Ba-Ti-O, so InMemoryPrecursorCatalog's
        // own element-overlap filter drops it; BaCO3/TiO2 survive, sorted
        // by id (InMemoryPrecursorCatalog::new's own sort).
        let ids: Vec<&str> = generated
            .iter()
            .map(|gc| gc.candidate.id.0.as_str())
            .collect();
        assert_eq!(ids, vec!["BaCO3", "TiO2"]);
        assert!(
            generated
                .iter()
                .all(|gc| gc.generator == GeneratorId("catalog-exact"))
        );
        assert_eq!(generated[0].rank, 0);
        assert_eq!(generated[1].rank, 1);
    }

    #[test]
    fn frequency_prior_generator_filters_by_element_overlap_and_preserves_frequency_order() {
        let generator = FrequencyPriorGenerator::new(vec![
            (candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]), 5),
            (
                candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
                50,
            ),
            // Irrelevant to the Ba-Ti-O target -- must be filtered out
            // regardless of its (highest) frequency.
            (candidate("NaCl", &[("Na", 1.0), ("Cl", 1.0)]), 1000),
        ]);

        let generated = generator
            .generate(&barium_titanate_target(), &no_constraints())
            .unwrap();

        let ids: Vec<&str> = generated
            .iter()
            .map(|gc| gc.candidate.id.0.as_str())
            .collect();
        assert_eq!(
            ids,
            vec!["BaCO3", "TiO2"],
            "higher frequency (50) must rank first"
        );
        assert!(
            generated
                .iter()
                .all(|gc| gc.generator == GeneratorId("frequency-prior"))
        );
        assert_eq!(generated[0].rank, 0);
        assert_eq!(generated[1].rank, 1);
    }

    #[test]
    fn ensemble_min_rank_fuses_candidates_proposed_by_either_generator() {
        // catalog-exact ranks BaCO3 (0), TiO2 (1) -- both sorted by id.
        let catalog_exact = CatalogExactGenerator::new(InMemoryPrecursorCatalog::new(vec![
            candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
            candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
        ]));
        // frequency-prior ranks TiO2 (0), BaCO3 (1) -- opposite order.
        let frequency_prior = FrequencyPriorGenerator::new(vec![
            (candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]), 100),
            (
                candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
                1,
            ),
        ]);

        let ensemble = CandidateGeneratorEnsemble::new(vec![
            Box::new(catalog_exact),
            Box::new(frequency_prior),
        ]);
        let output =
            ensemble.generate_with_provenance(&barium_titanate_target(), &no_constraints());

        // Min-rank fusion: BaCO3's best rank is 0 (from catalog-exact),
        // TiO2's best rank is also 0 (from frequency-prior) -- tie broken
        // alphabetically by id.
        let ids: Vec<&str> = output.candidates.iter().map(|c| c.id.0.as_str()).collect();
        assert_eq!(ids, vec!["BaCO3", "TiO2"]);
        assert!(output.generator_errors.is_empty());

        // Both generators proposed both candidates -- provenance keeps
        // every one, nothing silently collapsed.
        assert_eq!(
            output.provenance[&PrecursorId("BaCO3".to_string())].len(),
            2
        );
        assert_eq!(output.provenance[&PrecursorId("TiO2".to_string())].len(), 2);
    }

    #[test]
    fn ensemble_duplicate_id_conflict_keeps_first_generators_payload_but_records_every_proposer() {
        // Two generators proposing the same id with *different*
        // composition data (a malformed-input scenario, deliberately
        // constructed to test the conflict rule).
        let first = CatalogExactGenerator::new(InMemoryPrecursorCatalog::new(vec![candidate(
            "BaCO3",
            &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)],
        )]));
        let second = FrequencyPriorGenerator::new(vec![(
            candidate("BaCO3", &[("Ba", 2.0), ("C", 1.0), ("O", 3.0)]),
            10,
        )]);

        let ensemble = CandidateGeneratorEnsemble::new(vec![Box::new(first), Box::new(second)]);
        let output =
            ensemble.generate_with_provenance(&barium_titanate_target(), &no_constraints());

        assert_eq!(output.candidates.len(), 1);
        // First-generator-in-list (catalog-exact) wins the payload.
        assert_eq!(
            output.candidates[0].composition,
            composition(&[("Ba", 1.0), ("C", 1.0), ("O", 3.0)])
        );
        // But both proposals are still visible in provenance.
        assert_eq!(
            output.provenance[&PrecursorId("BaCO3".to_string())].len(),
            2
        );
    }

    #[test]
    fn ensemble_records_a_failed_generators_error_and_still_returns_the_others_candidates() {
        let catalog_exact =
            CatalogExactGenerator::new(InMemoryPrecursorCatalog::new(vec![candidate(
                "BaCO3",
                &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)],
            )]));

        let ensemble = CandidateGeneratorEnsemble::new(vec![
            Box::new(catalog_exact),
            Box::new(AlwaysFailsGenerator),
        ]);
        let output =
            ensemble.generate_with_provenance(&barium_titanate_target(), &no_constraints());

        assert_eq!(output.candidates.len(), 1);
        assert_eq!(output.candidates[0].id, PrecursorId("BaCO3".to_string()));
        assert_eq!(output.generator_errors.len(), 1);
        assert_eq!(output.generator_errors[0].0, GeneratorId("always-fails"));
    }

    #[test]
    fn ensemble_as_precursor_catalog_returns_the_same_candidates_as_generate_with_provenance() {
        let catalog_exact =
            CatalogExactGenerator::new(InMemoryPrecursorCatalog::new(vec![candidate(
                "BaCO3",
                &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)],
            )]));
        let ensemble = CandidateGeneratorEnsemble::new(vec![Box::new(catalog_exact)]);

        let via_trait = PrecursorCatalog::candidates_for(
            &ensemble,
            &barium_titanate_target(),
            &no_constraints(),
        )
        .unwrap();
        let via_inherent =
            ensemble.generate_with_provenance(&barium_titanate_target(), &no_constraints());

        assert_eq!(via_trait, via_inherent.candidates);
    }
}
