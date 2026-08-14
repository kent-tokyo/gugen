use crate::composition::Composition;
use crate::error::{GugenError, Result, require_finite};

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ReactionSpecies {
    pub composition: Composition,
    /// Must be positive; the Phase 2 solver removes zero-coefficient
    /// species rather than representing them (AGENTS.md §10).
    pub coefficient: u64,
}

/// An element-balanced reaction with integer, gcd-normalized coefficients
/// (AGENTS.md §10). The exact-rational null-space solver that produces
/// these from a target/precursor set is Phase 2 work (docs/architecture.md);
/// this type is the Phase 1 foundation it returns into.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BalancedReaction {
    pub reactants: Vec<ReactionSpecies>,
    pub products: Vec<ReactionSpecies>,
}

impl BalancedReaction {
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
        Ok(Self {
            reactants,
            products,
        })
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
