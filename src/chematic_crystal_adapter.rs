//! Converts a caller-supplied `chematic_crystal::PeriodicStructure` into a
//! `mikiwame::OwnedStructure`, so a caller with real chematic-crystal
//! structure data can drive `mikiwame::analyze` (see
//! [`crate::structural_effects`]) without hand-building `mikiwame::Site`
//! vectors themselves. Closes the specific gap `mikiwame_adapter.rs` has
//! named since Phase 6: gugen's own `TargetStructure` is free text, and
//! building a real structure "depends on chematic-crystal" -- now
//! published (0.15.0).
//!
//! Not a plain field-by-field copy. Two correctness details, both
//! confirmed against the real 0.15.0/0.1.0 source (not guessed) before
//! being handled here:
//!
//! - **Same-element consolidation, within one `PeriodicSite` only.**
//!   `chematic_crystal::PeriodicSite` allows multiple `SiteSpecies` for the
//!   same element at one position (disorder modeling), and occupancy
//!   `0.0` is valid. `mikiwame::Site` is flat (one element, one occupancy
//!   per entry) and its `SITE_DUPLICATE` check flags any two entries with
//!   the same element and a minimum-image distance under `1e-6` Angstrom
//!   -- a naive per-species flat-map would therefore produce a false
//!   positive. Species sharing an element *within one `PeriodicSite`* are
//!   summed and emitted once, dropped entirely if the sum is exactly
//!   `0.0`. Species belonging to *different* `PeriodicSite`s are never
//!   merged, even when they land at an identical element and position --
//!   that case is a genuine duplicate mikiwame should still catch (see
//!   `two_periodic_sites_at_the_same_element_and_position_are_kept_distinct_and_mikiwame_still_flags_them`).
//! - **Lattice handedness.** `chematic_crystal::Lattice` accepts a
//!   negative-determinant (left-handed) matrix as physically valid --
//!   volume is `abs()` of the signed determinant, and its reciprocal
//!   vectors are derived from the *signed* volume specifically so the
//!   inverse stays algebraically exact for left-handed input (see
//!   `chematic_crystal::lattice`'s own module doc comment). mikiwame
//!   treats a negative-determinant lattice as a Critical `InvalidInput`.
//!   A left-handed input is corrected by swapping the `b`/`c` lattice rows
//!   and the matching fractional `y`/`z` component of every site -- an
//!   exact basis change, not a geometry-altering heuristic (verified by
//!   `left_handed_lattice_is_corrected_without_moving_any_site`'s direct
//!   Cartesian-invariance check). Only the *sign* is corrected: a
//!   genuinely singular or near-singular lattice is passed through
//!   unchanged for mikiwame to flag on its own merits -- this adapter
//!   does not paper over real degeneracy.
//!
//! Both crates agree on the fractional/Cartesian convention (row vectors
//! `[a, b, c]`, `cartesian_k = sum_j fractional_j * matrix[j][k]`) --
//! confirmed directly against both crates' source
//! (`chematic_crystal::lattice`'s module doc, `mikiwame::structure_view`'s
//! `frac_to_cart`), not assumed by analogy.
//!
//! **Not called automatically by `Planner::plan`.** `TargetSpecification`
//! still has no field for real geometry; a caller builds their own
//! `chematic_crystal::PeriodicStructure`, converts it with
//! [`to_mikiwame_structure`], runs `mikiwame::analyze`, and applies
//! [`crate::structural_effects`]'s result themselves -- unchanged from
//! Phase 6.
//!
//! Known, inherited limitations this adapter does not solve:
//! - `label` is dropped: `mikiwame::Site` has no such field.
//! - Same-element consolidation can itself produce an occupancy above
//!   `1.0`: `chematic_crystal::PeriodicSite::validate` accepts a species-
//!   occupancy sum up to `1.0 + Occupancy::SUM_TOLERANCE` (`1e-6`), so a
//!   site right at that boundary converts to a single `mikiwame::Site`
//!   whose occupancy is slightly over `1.0` -- which mikiwame's own
//!   per-site range check reports as `INPUT_INVALID_OCCUPANCY` (expected
//!   `[0.0, 1.0]`, per `mikiwame::structure_view::Site`'s doc comment).
//!   This never happens with an *unconsolidated* flat-map (each species
//!   individually stays `<= 1.0`) -- it is a genuine consequence of
//!   consolidation, not an unrelated pair of tolerances. Not silently
//!   clamped or renormalized (see
//!   `consolidation_can_produce_an_occupancy_slightly_above_one`); the
//!   value is passed through as computed, and mikiwame's own diagnostics
//!   surface the disagreement rather than this adapter hiding it.
//! - mikiwame's minimum-image neighbor search checks each fractional axis
//!   independently and, in its own words, "may miss the true minimum
//!   image for highly skewed cells" (`mikiwame::structure_view`'s
//!   `minimum_image_distance` doc comment) -- inherited here, not
//!   addressed by this conversion.

use chematic_crystal::PeriodicStructure;
use mikiwame::{OwnedStructure, Site};

/// Converts a chematic-crystal structure into a mikiwame one. See the
/// module doc comment for why this isn't a plain field-by-field copy.
pub fn to_mikiwame_structure(structure: &PeriodicStructure) -> OwnedStructure {
    let mut lattice = structure.lattice().matrix();
    let inverted = determinant(&lattice) < 0.0;
    if inverted {
        lattice.swap(1, 2); // b <-> c: flips handedness, sign of determinant
    }

    let mut sites = Vec::new();
    for site in structure.sites() {
        let mut fractional = site.fractional.0;
        if inverted {
            fractional.swap(1, 2); // same basis change, applied to every site
        }

        // Consolidate same-element species *within this one PeriodicSite
        // only* -- never across separate PeriodicSite objects, so
        // mikiwame's own SITE_DUPLICATE check still sees genuinely
        // distinct sites as distinct.
        let mut per_element: Vec<(String, f64)> = Vec::new();
        for species in &site.species {
            let symbol = species.element.symbol().to_string();
            let occupancy = species.occupancy.value();
            match per_element
                .iter_mut()
                .find(|(element, _)| *element == symbol)
            {
                Some((_, total)) => *total += occupancy,
                None => per_element.push((symbol, occupancy)),
            }
        }

        for (element, occupancy) in per_element {
            if occupancy == 0.0 {
                continue; // exact zero only -- no epsilon, no silent renormalization
            }
            sites.push(Site {
                element,
                fractional,
                occupancy,
            });
        }
    }

    OwnedStructure::new(lattice, sites)
}

fn determinant(m: &[[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}

#[cfg(test)]
mod tests {
    use super::*;
    use chematic_core::Element;
    use chematic_crystal::{FractionalCoord, Lattice, Occupancy, PeriodicSite, SiteSpecies};
    use mikiwame::{AnalysisConfig, FindingCode, PeriodicStructureView, Verdict};

    fn structure(lattice: Lattice, sites: Vec<PeriodicSite>) -> PeriodicStructure {
        PeriodicStructure::new(lattice, sites).unwrap()
    }

    #[test]
    fn single_element_full_occupancy_produces_one_site() {
        let s = structure(
            Lattice::cubic(4.0).unwrap(),
            vec![
                PeriodicSite::new(
                    vec![SiteSpecies::full(Element::FE)],
                    FractionalCoord::new([0.1, 0.2, 0.3]),
                    None,
                )
                .unwrap(),
            ],
        );

        let converted = to_mikiwame_structure(&s);
        let sites = converted.sites();
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].element, "Fe");
        assert_eq!(sites[0].occupancy, 1.0);
        assert_eq!(sites[0].fractional, [0.1, 0.2, 0.3]);
    }

    #[test]
    fn mixed_occupancy_site_expands_into_two_mikiwame_sites() {
        let s = structure(
            Lattice::cubic(4.0).unwrap(),
            vec![
                PeriodicSite::new(
                    vec![
                        SiteSpecies {
                            element: Element::FE,
                            occupancy: Occupancy::new(0.6).unwrap(),
                        },
                        SiteSpecies {
                            element: Element::NI,
                            occupancy: Occupancy::new(0.4).unwrap(),
                        },
                    ],
                    FractionalCoord::new([0.25, 0.25, 0.25]),
                    None,
                )
                .unwrap(),
            ],
        );

        let converted = to_mikiwame_structure(&s);
        let sites = converted.sites();
        assert_eq!(sites.len(), 2);
        for site in sites {
            assert_eq!(site.fractional, [0.25, 0.25, 0.25]);
        }
        let fe = sites.iter().find(|s| s.element == "Fe").unwrap();
        let ni = sites.iter().find(|s| s.element == "Ni").unwrap();
        assert_eq!(fe.occupancy, 0.6);
        assert_eq!(ni.occupancy, 0.4);
    }

    #[test]
    fn same_element_within_one_site_is_consolidated() {
        let s = structure(
            Lattice::cubic(4.0).unwrap(),
            vec![
                PeriodicSite::new(
                    vec![
                        SiteSpecies {
                            element: Element::FE,
                            occupancy: Occupancy::new(0.3).unwrap(),
                        },
                        SiteSpecies {
                            element: Element::FE,
                            occupancy: Occupancy::new(0.3).unwrap(),
                        },
                    ],
                    FractionalCoord::new([0.1, 0.1, 0.1]),
                    None,
                )
                .unwrap(),
            ],
        );

        let converted = to_mikiwame_structure(&s);
        let sites = converted.sites();
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].element, "Fe");
        assert!((sites[0].occupancy - 0.6).abs() < 1e-12);
    }

    #[test]
    fn consolidation_can_produce_an_occupancy_slightly_above_one() {
        // 0.5 + 0.5000005 = 1.0000005, inside chematic_crystal's own
        // Occupancy::SUM_TOLERANCE (1e-6, so PeriodicSite::new accepts
        // it), but consolidation folds this into a single mikiwame::Site
        // whose occupancy exceeds 1.0 -- unreachable via an unconsolidated
        // flat-map, where each species individually stays <= 1.0. Not
        // clamped: the value passes through, and mikiwame's own per-site
        // range check reports it as InputInvalidOccupancy.
        let s = structure(
            Lattice::cubic(4.0).unwrap(),
            vec![
                PeriodicSite::new(
                    vec![
                        SiteSpecies {
                            element: Element::FE,
                            occupancy: Occupancy::new(0.5).unwrap(),
                        },
                        SiteSpecies {
                            element: Element::FE,
                            occupancy: Occupancy::new(0.5000005).unwrap(),
                        },
                    ],
                    FractionalCoord::new([0.1, 0.1, 0.1]),
                    None,
                )
                .unwrap(),
            ],
        );

        let converted = to_mikiwame_structure(&s);
        let sites = converted.sites();
        assert_eq!(sites.len(), 1);
        assert!(
            sites[0].occupancy > 1.0,
            "expected an occupancy above 1.0, got {}",
            sites[0].occupancy
        );

        let report = mikiwame::analyze(&converted, &AnalysisConfig::default());
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == FindingCode::InputInvalidOccupancy),
            "mikiwame should flag the consolidated occupancy itself: {:?}",
            report.findings
        );
    }

    #[test]
    fn zero_occupancy_after_consolidation_is_excluded() {
        let zero_site = PeriodicSite::new(
            vec![
                SiteSpecies {
                    element: Element::FE,
                    occupancy: Occupancy::new(0.0).unwrap(),
                },
                SiteSpecies {
                    element: Element::FE,
                    occupancy: Occupancy::new(0.0).unwrap(),
                },
            ],
            FractionalCoord::new([0.1, 0.1, 0.1]),
            None,
        )
        .unwrap();
        let normal_site = PeriodicSite::new(
            vec![SiteSpecies::full(Element::NA)],
            FractionalCoord::new([0.5, 0.5, 0.5]),
            None,
        )
        .unwrap();
        let s = structure(Lattice::cubic(4.0).unwrap(), vec![zero_site, normal_site]);

        let converted = to_mikiwame_structure(&s);
        let sites = converted.sites();
        assert_eq!(sites.len(), 1);
        assert_eq!(sites[0].element, "Na");
    }

    #[test]
    fn two_periodic_sites_at_the_same_element_and_position_are_kept_distinct_and_mikiwame_still_flags_them()
     {
        let a = PeriodicSite::new(
            vec![SiteSpecies::full(Element::FE)],
            FractionalCoord::new([0.25, 0.25, 0.25]),
            Some("Fe-A".to_string()),
        )
        .unwrap();
        let b = PeriodicSite::new(
            vec![SiteSpecies::full(Element::FE)],
            FractionalCoord::new([0.25, 0.25, 0.25]),
            Some("Fe-B".to_string()),
        )
        .unwrap();
        let s = structure(Lattice::cubic(5.0).unwrap(), vec![a, b]);

        let converted = to_mikiwame_structure(&s);
        assert_eq!(
            converted.sites().len(),
            2,
            "distinct sites must not be merged"
        );

        let report = mikiwame::analyze(&converted, &AnalysisConfig::default());
        assert!(
            report
                .findings
                .iter()
                .any(|f| f.code == FindingCode::SiteDuplicate),
            "a genuine cross-site duplicate must still be caught: {:?}",
            report.findings
        );
    }

    #[test]
    fn non_orthogonal_lattice_with_asymmetric_rows_is_not_transposed() {
        // Three distinguishable, non-symmetric rows and an already-positive
        // determinant: no swap should happen at all, so this independently
        // pins the row/column convention regardless of the handedness path
        // (test 7 below only exercises that path and wouldn't by itself
        // catch a transpose bug).
        let matrix = [[1.0, 0.0, 0.0], [0.5, 2.0, 0.0], [0.3, 0.4, 3.0]];
        let s = structure(
            Lattice::from_matrix(matrix).unwrap(),
            vec![
                PeriodicSite::new(
                    vec![SiteSpecies::full(Element::FE)],
                    FractionalCoord::new([0.2, 0.3, 0.4]),
                    None,
                )
                .unwrap(),
            ],
        );

        let converted = to_mikiwame_structure(&s);
        assert_eq!(*converted.lattice(), matrix);
        assert_eq!(converted.sites()[0].fractional, [0.2, 0.3, 0.4]);
    }

    #[test]
    fn left_handed_lattice_is_corrected_without_moving_any_site() {
        // a x b . c < 0: left-handed, but chematic-crystal accepts it
        // (Lattice::from_matrix validates degeneracy/length, not sign).
        let matrix = [[2.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.5, 0.5, -4.0]];
        assert!(determinant(&matrix) < 0.0, "fixture must be left-handed");

        let original_fractional = [0.3, 0.4, 0.5];
        let s = structure(
            Lattice::from_matrix(matrix).unwrap(),
            vec![
                PeriodicSite::new(
                    vec![SiteSpecies::full(Element::FE)],
                    FractionalCoord::new(original_fractional),
                    None,
                )
                .unwrap(),
            ],
        );

        let converted = to_mikiwame_structure(&s);
        let converted_lattice = *converted.lattice();
        assert!(
            determinant(&converted_lattice) > 0.0,
            "handedness must be corrected"
        );

        // Cartesian-invariance, computed independently of the adapter's own
        // logic (a separate row-vector dot product, not a call into
        // `to_mikiwame_structure` or `determinant`): the site must not have
        // physically moved, only been re-expressed in a right-handed basis.
        fn cart(frac: [f64; 3], lattice: [[f64; 3]; 3]) -> [f64; 3] {
            [
                frac[0] * lattice[0][0] + frac[1] * lattice[1][0] + frac[2] * lattice[2][0],
                frac[0] * lattice[0][1] + frac[1] * lattice[1][1] + frac[2] * lattice[2][1],
                frac[0] * lattice[0][2] + frac[1] * lattice[1][2] + frac[2] * lattice[2][2],
            ]
        }

        let before = cart(original_fractional, matrix);
        let after = cart(converted.sites()[0].fractional, converted_lattice);
        for i in 0..3 {
            assert!(
                (before[i] - after[i]).abs() < 1e-9,
                "axis {i}: {before:?} vs {after:?}"
            );
        }
    }

    #[test]
    fn converted_structure_runs_through_real_mikiwame_analyze() {
        // A CsCl-type two-site structure -- structurally unremarkable, no
        // finding mikiwame's v0.1 checks should flag.
        let s = structure(
            Lattice::cubic(5.64).unwrap(),
            vec![
                PeriodicSite::new(
                    vec![SiteSpecies::full(Element::NA)],
                    FractionalCoord::new([0.0, 0.0, 0.0]),
                    None,
                )
                .unwrap(),
                PeriodicSite::new(
                    vec![SiteSpecies::full(Element::CL)],
                    FractionalCoord::new([0.5, 0.5, 0.5]),
                    None,
                )
                .unwrap(),
            ],
        );

        let converted = to_mikiwame_structure(&s);
        let report = mikiwame::analyze(&converted, &AnalysisConfig::default());
        assert_eq!(
            report.overall.verdict,
            Verdict::StructurallyConsistent,
            "unexpected findings: {:?}",
            report.findings
        );
    }
}
