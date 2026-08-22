//! A feature-gated `ThermodynamicProvider` over pre-fetched Materials
//! Project-shaped data (AGENTS.md §8, docs/integration.md). Mirrors
//! `mikiwame_adapter.rs`'s pattern: this module doesn't exist in the
//! compiled crate unless the `materials_project` feature is enabled.
//!
//! **gugen performs no network call anywhere in this module** (AGENTS.md
//! §8, §25). `materials_project = []` in `Cargo.toml` declares zero new
//! dependencies -- that's what makes this structurally true rather than
//! merely a policy gugen's own code happens to follow: there is no HTTP
//! client, no API key field, nothing to fetch with even if this code
//! wanted to. A caller that has already queried the real Materials Project
//! REST API (or any other source of formation-energy data) constructs
//! [`CompetingPhase`](crate::CompetingPhase) entries itself and passes
//! them to [`MaterialsProjectSnapshotProvider::from_entries`].
//!
//! No formula parser exists in gugen (`Composition` has no `Display`/
//! `FromStr`) -- a caller converts a Materials Project `formula_pretty`
//! response field (or their own already-parsed composition data) into a
//! [`Composition`] during their pre-fetch step, before it ever reaches
//! this adapter. See `docs/integration.md` for a worked example and the
//! specific field names this was verified against.

use crate::composition::Composition;
use crate::error::ProviderError;
use crate::provider::ThermodynamicProvider;
use crate::reaction::{
    BalancedReaction, CompetingPhase, ReactionEnergy, ThermodynamicConditions,
    check_element_conservation,
};
use std::collections::BTreeSet;

/// A `ThermodynamicProvider` over a fixed, caller-supplied snapshot of
/// competing-phase formation energies -- e.g. a pre-fetched slice of a
/// Materials Project query. See the module doc comment: this type never
/// fetches anything itself, and has no notion of "stale" or "refresh".
pub struct MaterialsProjectSnapshotProvider {
    entries: Vec<CompetingPhase>,
}

impl MaterialsProjectSnapshotProvider {
    pub fn from_entries(entries: Vec<CompetingPhase>) -> Self {
        Self { entries }
    }

    /// A real Materials Project query snapshot can legitimately contain
    /// more than one entry for the same exact `Composition` -- distinct
    /// polymorphs (e.g. rutile vs. anatase TiO2) are different
    /// `material_id`s that happen to share a formula. Picking whichever
    /// entry came first in the caller-supplied `Vec` would make
    /// `reaction_energy` silently depend on that ordering (AGENTS.md
    /// §21.4) -- instead this takes the lowest (most stable) formation
    /// energy among all matches, the standard "most stable known phase"
    /// convention, which is also order-independent by construction.
    fn energy_for(&self, composition: &Composition) -> Option<f64> {
        self.entries
            .iter()
            .filter(|entry| &entry.composition == composition)
            .map(CompetingPhase::formation_energy_ev_per_atom)
            .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |a| a.min(v))))
    }
}

/// Sum of a composition's amounts -- the atom count of one formula unit.
fn atoms_in_formula(composition: &Composition) -> f64 {
    composition.iter().map(|(_, amount)| amount).sum()
}

impl ThermodynamicProvider for MaterialsProjectSnapshotProvider {
    /// Delta = (sum of product formula-unit energies) - (sum of reactant
    /// formula-unit energies), each species weighted by its
    /// `ReactionSpecies::coefficient`, normalized per atom of the reactant
    /// side's total atom count. For any reaction `balance.rs` actually
    /// produced, element conservation makes the reactant- and
    /// product-side atom totals equal, so this is "per atom of the
    /// reaction" either way; the reactant side is used simply because it's
    /// available without a second pass over `products`.
    ///
    /// Returns `Ok(None)` -- never a partial sum -- the moment any
    /// participating species' exact `Composition` isn't in the snapshot:
    /// a reaction energy computed from some-but-not-all species would
    /// silently misrepresent "no data for this reaction" as "a real,
    /// if incomplete, answer."
    ///
    /// `Err(ProviderError::MalformedRecord(_))` if `reaction` doesn't
    /// conserve elements -- unreachable in practice since
    /// `BalancedReaction::new` (v0.5.0, Phase 23A) already guarantees this
    /// via the same `reaction::check_element_conservation` check (the same
    /// check `balanced_reaction_delta_ev_per_atom` runs), run again here
    /// only as a defensive, redundant guard rather than silently computing
    /// a meaningless-but-plausible-looking energy for an unbalanced
    /// reaction.
    fn reaction_energy(
        &self,
        reaction: &BalancedReaction,
        _conditions: &ThermodynamicConditions,
    ) -> std::result::Result<Option<ReactionEnergy>, ProviderError> {
        check_element_conservation(reaction.reactants(), reaction.products())
            .map_err(|e| ProviderError::MalformedRecord(e.to_string()))?;

        let mut product_total = 0.0;
        for species in reaction.products() {
            let Some(energy) = self.energy_for(&species.composition) else {
                return Ok(None);
            };
            product_total +=
                species.coefficient() as f64 * atoms_in_formula(&species.composition) * energy;
        }

        let mut reactant_total = 0.0;
        let mut reactant_atoms = 0.0;
        for species in reaction.reactants() {
            let Some(energy) = self.energy_for(&species.composition) else {
                return Ok(None);
            };
            let atoms = atoms_in_formula(&species.composition);
            reactant_total += species.coefficient() as f64 * atoms * energy;
            reactant_atoms += species.coefficient() as f64 * atoms;
        }

        let delta_per_atom = (product_total - reactant_total) / reactant_atoms;
        ReactionEnergy::new(delta_per_atom)
            .map(Some)
            .map_err(|e| ProviderError::MalformedRecord(e.to_string()))
    }

    /// Every snapshot entry sharing at least one element with `target`,
    /// excluding an entry whose composition exactly matches `target`
    /// itself (that's the target, not something competing with it).
    /// Unlike `reaction_energy`'s `energy_for`, this does **not** collapse
    /// distinct entries that share a composition (e.g. TiO2 polymorphs) to
    /// one -- each known phase is itself the evidence here, not an input
    /// to an arithmetic result that needs exactly one value per
    /// composition. `Planner::plan` additionally excludes phases that
    /// exactly match the specific reaction's own reactants/products before
    /// attaching this as evidence, so "competing phase" only ever
    /// describes something outside that plan's own reaction.
    fn competing_phases(
        &self,
        target: &Composition,
    ) -> std::result::Result<Vec<CompetingPhase>, ProviderError> {
        let target_elements: BTreeSet<_> = target.elements().collect();
        Ok(self
            .entries
            .iter()
            .filter(|entry| &entry.composition != target)
            .filter(|entry| {
                entry
                    .composition
                    .elements()
                    .any(|element| target_elements.contains(&element))
            })
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composition::Element;
    use crate::error::GugenError;
    use crate::reaction::ReactionSpecies;

    fn element(symbol: &str) -> Element {
        Element::new(symbol).unwrap()
    }

    fn composition(pairs: &[(&str, f64)]) -> Composition {
        Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
    }

    fn species(pairs: &[(&str, f64)], coefficient: u64) -> ReactionSpecies {
        ReactionSpecies::new(composition(pairs), coefficient).unwrap()
    }

    /// Hand-checked arithmetic (AGENTS.md §21.4-style pinned-value test,
    /// same discipline as `balance.rs`'s exact-arithmetic tests): 2 FeO ->
    /// Fe2O2 is a trivial, genuinely element-balanced two-species reaction
    /// (one reactant species, one product species).
    ///
    /// - reactant: FeO (2 atoms/formula), formation energy -2.0 eV/atom,
    ///   coefficient 2 -> total energy = 2 * 2 * -2.0 = -8.0 eV, atoms = 4.
    /// - product: Fe2O2 (4 atoms/formula), formation energy -3.0 eV/atom,
    ///   coefficient 1 -> total energy = 1 * 4 * -3.0 = -12.0 eV.
    /// - delta = -12.0 - (-8.0) = -4.0 eV, / 4 reactant atoms = -1.0 eV/atom.
    #[test]
    fn reaction_energy_computes_the_hand_checked_delta_for_a_two_species_reaction() {
        let feo = composition(&[("Fe", 1.0), ("O", 1.0)]);
        let fe2o2 = composition(&[("Fe", 2.0), ("O", 2.0)]);
        let provider = MaterialsProjectSnapshotProvider::from_entries(vec![
            CompetingPhase::new(feo, -2.0).unwrap(),
            CompetingPhase::new(fe2o2, -3.0).unwrap(),
        ]);
        let reaction = BalancedReaction::new(
            vec![species(&[("Fe", 1.0), ("O", 1.0)], 2)],
            vec![species(&[("Fe", 2.0), ("O", 2.0)], 1)],
        )
        .unwrap();

        let energy = provider
            .reaction_energy(&reaction, &ThermodynamicConditions::default())
            .unwrap()
            .expect("both species are in the snapshot");
        assert_eq!(energy.value_ev_per_atom(), -1.0);
    }

    #[test]
    fn reaction_energy_returns_none_not_a_partial_sum_when_a_species_is_missing() {
        let feo = composition(&[("Fe", 1.0), ("O", 1.0)]);
        // Only the reactant is in the snapshot -- the product is missing.
        let provider = MaterialsProjectSnapshotProvider::from_entries(vec![
            CompetingPhase::new(feo, -2.0).unwrap(),
        ]);
        let reaction = BalancedReaction::new(
            vec![species(&[("Fe", 1.0), ("O", 1.0)], 2)],
            vec![species(&[("Fe", 2.0), ("O", 2.0)], 1)],
        )
        .unwrap();

        let result = provider
            .reaction_energy(&reaction, &ThermodynamicConditions::default())
            .unwrap();
        assert_eq!(
            result, None,
            "a missing product must abstain entirely, not sum only the reactant side"
        );
    }

    /// Two polymorph entries share the exact same `Composition` (e.g. two
    /// distinct Materials Project `material_id`s for TiO2) -- `energy_for`
    /// must use the lower (more stable) one, and get the same answer
    /// regardless of which order the caller happened to list them in. Only
    /// the reactant's energy varies between the two provider instances
    /// below, so a passing test here directly proves order-independence,
    /// not just "some deterministic value came out."
    #[test]
    fn reaction_energy_uses_the_lowest_energy_among_duplicate_compositions_regardless_of_order() {
        let feo = composition(&[("Fe", 1.0), ("O", 1.0)]);
        let fe2o2 = composition(&[("Fe", 2.0), ("O", 2.0)]);
        let reaction = BalancedReaction::new(
            vec![species(&[("Fe", 1.0), ("O", 1.0)], 2)],
            vec![species(&[("Fe", 2.0), ("O", 2.0)], 1)],
        )
        .unwrap();

        let ascending = MaterialsProjectSnapshotProvider::from_entries(vec![
            CompetingPhase::new(feo.clone(), -2.0).unwrap(),
            CompetingPhase::new(feo.clone(), -5.0).unwrap(),
            CompetingPhase::new(fe2o2.clone(), -3.0).unwrap(),
        ]);
        let descending = MaterialsProjectSnapshotProvider::from_entries(vec![
            CompetingPhase::new(feo.clone(), -5.0).unwrap(),
            CompetingPhase::new(feo, -2.0).unwrap(),
            CompetingPhase::new(fe2o2, -3.0).unwrap(),
        ]);

        let a = ascending
            .reaction_energy(&reaction, &ThermodynamicConditions::default())
            .unwrap()
            .unwrap();
        let b = descending
            .reaction_energy(&reaction, &ThermodynamicConditions::default())
            .unwrap()
            .unwrap();
        assert_eq!(a.value_ev_per_atom(), b.value_ev_per_atom());
        // Reactant total with the -5.0 (more stable) entry: 2 * 2 * -5.0 =
        // -20.0 eV / 4 atoms. Product: 1 * 4 * -3.0 = -12.0 eV. Delta =
        // (-12.0 - -20.0) / 4 = 2.0 eV/atom -- if this used -2.0 instead
        // the result would be -1.0 (this test's other case), so this also
        // proves the *lower* energy was picked, not just "a consistent one".
        assert_eq!(a.value_ev_per_atom(), 2.0);
    }

    /// Mirrors `thermodynamics::balanced_reaction_new_rejects_element_imbalance`:
    /// `BalancedReaction::new` (v0.5.0, Phase 23A) itself now rejects a
    /// reaction with mismatched reactant/product element totals (1 Fe + 1
    /// O vs. 2 Fe + 3 O here), so `reaction_energy`'s own
    /// `check_element_conservation` call can no longer observe such an
    /// input at all -- this asserts the rejection at the point it now
    /// actually happens, construction, rather than exercising
    /// `reaction_energy`'s now-unreachable defensive check.
    #[test]
    fn reaction_energy_rejects_element_imbalance() {
        let result = BalancedReaction::new(
            vec![species(&[("Fe", 1.0), ("O", 1.0)], 1)],
            vec![species(&[("Fe", 2.0), ("O", 3.0)], 1)],
        );
        assert!(
            matches!(result, Err(GugenError::UnbalancedReaction { .. })),
            "expected UnbalancedReaction, got {result:?}"
        );
    }

    #[test]
    fn competing_phases_excludes_the_target_itself_and_unrelated_elements() {
        let target = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let shares_an_element = composition(&[("Ba", 1.0), ("O", 1.0)]);
        let unrelated = composition(&[("Na", 1.0), ("Cl", 1.0)]);
        let provider = MaterialsProjectSnapshotProvider::from_entries(vec![
            CompetingPhase::new(target.clone(), -1.0).unwrap(),
            CompetingPhase::new(shares_an_element.clone(), -2.0).unwrap(),
            CompetingPhase::new(unrelated, -3.0).unwrap(),
        ]);

        let phases = provider.competing_phases(&target).unwrap();
        assert_eq!(phases.len(), 1);
        assert_eq!(phases[0].composition, shares_an_element);
    }
}
