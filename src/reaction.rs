use crate::composition::{Composition, Element};
use crate::error::{GugenError, Result, require_finite};
use std::collections::BTreeMap;

/// How far apart reactant- and product-side element totals must sum to
/// still count as "conserved" -- generous enough to absorb ordinary
/// floating-point summation error across a handful of terms, tight enough
/// that a genuinely different composition can never pass by coincidence
/// (real synthesis-target element amounts are never within `1e-6` of each
/// other by chance in this crate's own fixtures, `AGENTS.md`'s worked
/// examples among them). Also reused by
/// `thermodynamics::decomposition_margin_ev_per_atom` for the same kind of
/// "same total composition" comparison.
pub(crate) const COMPOSITION_CONSERVATION_TOLERANCE: f64 = 1e-6;

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ReactionSpecies {
    pub composition: Composition,
    /// Must be positive; the Phase 2 solver removes zero-coefficient
    /// species rather than representing them (AGENTS.md §10).
    coefficient: u64,
}

impl ReactionSpecies {
    pub fn new(composition: Composition, coefficient: u64) -> Result<Self> {
        if coefficient == 0 {
            return Err(GugenError::ZeroCoefficient);
        }
        Ok(Self {
            composition,
            coefficient,
        })
    }

    pub fn coefficient(&self) -> u64 {
        self.coefficient
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ReactionSpecies {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Raw {
            composition: Composition,
            coefficient: u64,
        }
        let raw = Raw::deserialize(deserializer)?;
        ReactionSpecies::new(raw.composition, raw.coefficient).map_err(serde::de::Error::custom)
    }
}

/// Sums coefficient-weighted per-element amounts, reactants positive and
/// products negative, and rejects (Phase 19P.1 Fix 3) if any element's
/// residual exceeds `COMPOSITION_CONSERVATION_TOLERANCE`. Called from
/// `BalancedReaction::new` itself (v0.5.0, Phase 23A) as the primary guard
/// against a hand-constructed, non-conserving reaction; also reused as a
/// defensive, now-redundant check by
/// `thermodynamics::balanced_reaction_delta_ev_per_atom` and
/// `MaterialsProjectSnapshotProvider::reaction_energy`, both of which
/// operate on an already-validated `&BalancedReaction` and so can never
/// actually observe this returning `Err` -- kept rather than removed since
/// the cost is negligible and it protects those call sites against any
/// future weakening of `BalancedReaction::new`'s own guarantee.
pub(crate) fn check_element_conservation(
    reactants: &[ReactionSpecies],
    products: &[ReactionSpecies],
) -> Result<()> {
    let mut residual: BTreeMap<Element, f64> = BTreeMap::new();
    for species in reactants {
        for (element, amount) in species.composition.iter() {
            *residual.entry(element).or_insert(0.0) += species.coefficient as f64 * amount;
        }
    }
    for species in products {
        for (element, amount) in species.composition.iter() {
            *residual.entry(element).or_insert(0.0) -= species.coefficient as f64 * amount;
        }
    }
    for (element, imbalance) in residual {
        if imbalance.abs() > COMPOSITION_CONSERVATION_TOLERANCE {
            return Err(GugenError::UnbalancedReaction {
                element: element.symbol().to_string(),
                imbalance,
            });
        }
    }
    Ok(())
}

/// An element-balanced reaction with integer, gcd-normalized coefficients
/// (AGENTS.md §10). The exact-rational null-space solver that produces
/// these from a target/precursor set is Phase 2 work (docs/architecture.md);
/// this type is the Phase 1 foundation it returns into.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct BalancedReaction {
    reactants: Vec<ReactionSpecies>,
    products: Vec<ReactionSpecies>,
}

impl BalancedReaction {
    /// Rejects an empty side, a zero coefficient (redundant with
    /// `ReactionSpecies::new`'s own check -- kept here too as a guard
    /// against any future path into a `Vec<ReactionSpecies>` that doesn't
    /// route through it, e.g. nested deserialize), and (v0.5.0, Phase 23A)
    /// an element imbalance via `check_element_conservation`. Before
    /// Phase 23A this constructor accepted non-conserving reactions --
    /// callers relied on downstream checks (`thermodynamics::
    /// check_element_conservation`, `MaterialsProjectSnapshotProvider::
    /// reaction_energy`) to catch that instead. Those checks still run,
    /// now redundantly, as belt-and-suspenders.
    pub fn new(reactants: Vec<ReactionSpecies>, products: Vec<ReactionSpecies>) -> Result<Self> {
        if reactants.is_empty() || products.is_empty() {
            return Err(GugenError::EmptyReaction);
        }
        if reactants
            .iter()
            .chain(products.iter())
            .any(|s| s.coefficient == 0)
        {
            return Err(GugenError::ZeroCoefficient);
        }
        check_element_conservation(&reactants, &products)?;
        Ok(Self {
            reactants,
            products,
        })
    }

    pub fn reactants(&self) -> &[ReactionSpecies] {
        &self.reactants
    }

    pub fn products(&self) -> &[ReactionSpecies] {
        &self.products
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for BalancedReaction {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Raw {
            reactants: Vec<ReactionSpecies>,
            products: Vec<ReactionSpecies>,
        }
        let raw = Raw::deserialize(deserializer)?;
        BalancedReaction::new(raw.reactants, raw.products).map_err(serde::de::Error::custom)
    }
}

/// Minimal Phase 1 placeholder for `ThermodynamicProvider` inputs; extended
/// once a real provider is implemented (Phase 2/6).
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ThermodynamicConditions {
    pub temperature_celsius: Option<f64>,
}

/// Deliberately carries only the energetic quantity. AGENTS.md §4.3
/// requires thermodynamic favorability to stay separate from experimental
/// likelihood, so this type must not accumulate unrelated "likelihood"
/// fields later.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ReactionEnergy {
    value_ev_per_atom: f64,
}

impl ReactionEnergy {
    pub fn new(value_ev_per_atom: f64) -> Result<Self> {
        require_finite("value_ev_per_atom", value_ev_per_atom)?;
        Ok(Self { value_ev_per_atom })
    }

    pub fn value_ev_per_atom(&self) -> f64 {
        self.value_ev_per_atom
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ReactionEnergy {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Raw {
            value_ev_per_atom: f64,
        }
        let raw = Raw::deserialize(deserializer)?;
        ReactionEnergy::new(raw.value_ev_per_atom).map_err(serde::de::Error::custom)
    }
}

/// A candidate phase's formation energy, offered for context alongside a
/// [`BalancedReaction`] -- e.g. "would this target's elements more readily
/// form some other known compound instead" (Phase 13,
/// `ThermodynamicProvider::competing_phases`). Additive to
/// `ThermodynamicProvider`, not to `ReactionEnergy`: that type's own doc
/// comment forbids growing unrelated fields onto *it* specifically, but
/// says nothing against a sibling type for a genuinely different quantity.
/// gugen does not compute a selectivity/likelihood score from this data
/// (AGENTS.md §4.3) -- it is surfaced only as `PlanningEvidence`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct CompetingPhase {
    pub composition: Composition,
    formation_energy_ev_per_atom: f64,
}

impl CompetingPhase {
    pub fn new(composition: Composition, formation_energy_ev_per_atom: f64) -> Result<Self> {
        require_finite("formation_energy_ev_per_atom", formation_energy_ev_per_atom)?;
        Ok(Self {
            composition,
            formation_energy_ev_per_atom,
        })
    }

    pub fn formation_energy_ev_per_atom(&self) -> f64 {
        self.formation_energy_ev_per_atom
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for CompetingPhase {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Raw {
            composition: Composition,
            formation_energy_ev_per_atom: f64,
        }
        let raw = Raw::deserialize(deserializer)?;
        CompetingPhase::new(raw.composition, raw.formation_energy_ev_per_atom)
            .map_err(serde::de::Error::custom)
    }
}
