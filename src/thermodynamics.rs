//! Phase 19P: finite-temperature Gibbs-energy estimation, scoped
//! deliberately narrow (owner's explicit instruction): gas-free, closed
//! solid-phase systems only. No gas chemical potential, no liquid/aqueous
//! species, no absolute formation-Gibbs claim, no `Score01`/ranking
//! connection -- `thermodynamic_support` stays `None` regardless of
//! anything computed here.
//!
//! Model: Bartel, C. J., Millican, S. L., Deml, A. M., Rumptz, J. R.,
//! Tumas, W., Weimer, A. W., Lany, S., Stevanović, V., Musgrave, C. B.,
//! Holder, A. M. "Physical descriptor for the Gibbs energy of inorganic
//! crystalline solids and temperature-dependent materials chemistry."
//! *Nature Communications* 9, 4168 (2018), DOI
//! `10.1038/s41467-018-06682-4` -- confirmed **CC BY 4.0** via Crossref's
//! own license metadata (verified live, not assumed from "Nature
//! Communications is open access"). Coefficients (Eq. 4) and the reduced
//! mass formula (Eq. 6) were read directly from pymatgen's own independent
//! implementation (`GibbsComputedStructureEntry`, MIT-licensed code) as a
//! primary-source cross-check, not reconstructed from memory (AGENTS.md
//! §21.3).
//!
//! **Deliberately excluded**: pymatgen's own `GibbsComputedStructureEntry`
//! also subtracts a temperature-dependent elemental-reference term
//! (`sum_g_i`) to make its result an absolute, cross-composition-comparable
//! energy for its own `PhaseDiagram` machinery. This module never computes
//! that term: every quantity here is a same-total-composition comparison
//! (a balanced reaction's product side vs. reactant side, or a
//! decomposition margin against an alternative assemblage of the same
//! total composition) -- verified, numerically and geometrically, that the
//! elemental-reference term cancels exactly for any such comparison (see
//! Phase 19P's plan record, `tasks/todo.md`). This means gugen needs no
//! bundled elemental-Gibbs-energy table and no NIST-derived data anywhere
//! in this module.
//!
//! **Temperature range**: gugen restricts to `[300.0, 1800.0]` K, the
//! range Bartel et al. 2018 actually validated the SISSO descriptor
//! against (~50 meV/atom reported resolution) -- deliberately *not*
//! pymatgen's `[300.0, 2000.0]` K, which extrapolates past the paper's own
//! validated range via interpolation of lookup tables this module doesn't
//! use anyway.
//!
//! **Polymorph transitions are out of scope.** The SISSO descriptor uses
//! only volume as structural information (Bartel et al. 2018's own stated
//! limitation), so it cannot reliably predict which polymorph of a given
//! composition becomes stable at a given temperature. Where
//! [`balanced_reaction_delta_ev_per_atom`] looks up a reactant or product's
//! entry among more than one [`SolidThermodynamicEntry`] sharing that exact
//! composition, it always selects the one with the lowest 0 K
//! `formation_enthalpy_ev_per_atom` (the same "most stable known phase"
//! convention `materials_project_adapter.rs`'s
//! `MaterialsProjectSnapshotProvider` already uses, order-independent by
//! construction) -- not the lowest finite-temperature value, which would
//! implicitly claim a T-dependent polymorph-switching prediction this
//! model cannot support. This lookup-and-select behavior is specific to
//! that one function: [`decomposition_margin_ev_per_atom`] takes its
//! `alternative_assemblage` entries as given and sums them directly (the
//! caller already named the exact assemblage), so duplicate compositions
//! there are not deduplicated by this module at all -- a caller who lists
//! the same composition twice gets both counted.

use crate::composition::{Composition, Element};
use crate::error::{GugenError, Result, require_finite};
use crate::reaction::BalancedReaction;
use std::collections::BTreeMap;

/// IUPAC 2021 standard atomic weights (Commission on Isotopic Abundances
/// and Atomic Weights, <https://iupac.qmul.ac.uk/AtWt/AtWt21.html>,
/// verified live 2026-08-15), conventional/abridged value where the source
/// gives an uncertainty range, the bracketed mass number of the
/// longest-lived known isotope for elements with no stable isotopes (IUPAC's
/// own convention, stripped of brackets for a plain `f64`) -- cross-checked
/// against pymatgen's independently-sourced periodic table (max relative
/// difference 1.02%, entirely on radioisotope elements whose "longest-lived
/// known isotope" designation legitimately drifts between reference
/// snapshots as new isotopes are characterized; irrelevant to any real
/// solid-state synthesis target). [amu]
const ATOMIC_WEIGHTS: [(&str, f64); 118] = [
    ("H", 1.008),
    ("He", 4.002602),
    ("Li", 6.94),
    ("Be", 9.0121831),
    ("B", 10.81),
    ("C", 12.011),
    ("N", 14.007),
    ("O", 15.999),
    ("F", 18.99840316),
    ("Ne", 20.1797),
    ("Na", 22.98976928),
    ("Mg", 24.305),
    ("Al", 26.9815384),
    ("Si", 28.085),
    ("P", 30.973761998),
    ("S", 32.06),
    ("Cl", 35.45),
    ("Ar", 39.95),
    ("K", 39.0983),
    ("Ca", 40.078),
    ("Sc", 44.955907),
    ("Ti", 47.867),
    ("V", 50.9415),
    ("Cr", 51.9961),
    ("Mn", 54.938043),
    ("Fe", 55.845),
    ("Co", 58.933194),
    ("Ni", 58.6934),
    ("Cu", 63.546),
    ("Zn", 65.38),
    ("Ga", 69.723),
    ("Ge", 72.630),
    ("As", 74.921595),
    ("Se", 78.971),
    ("Br", 79.904),
    ("Kr", 83.798),
    ("Rb", 85.4678),
    ("Sr", 87.62),
    ("Y", 88.905838),
    ("Zr", 91.224),
    ("Nb", 92.90637),
    ("Mo", 95.95),
    ("Tc", 97.0),
    ("Ru", 101.07),
    ("Rh", 102.90549),
    ("Pd", 106.42),
    ("Ag", 107.8682),
    ("Cd", 112.414),
    ("In", 114.818),
    ("Sn", 118.710),
    ("Sb", 121.760),
    ("Te", 127.60),
    ("I", 126.90447),
    ("Xe", 131.293),
    ("Cs", 132.90545196),
    ("Ba", 137.327),
    ("La", 138.90547),
    ("Ce", 140.116),
    ("Pr", 140.90766),
    ("Nd", 144.242),
    ("Pm", 145.0),
    ("Sm", 150.36),
    ("Eu", 151.964),
    ("Gd", 157.25),
    ("Tb", 158.925354),
    ("Dy", 162.500),
    ("Ho", 164.930329),
    ("Er", 167.259),
    ("Tm", 168.934219),
    ("Yb", 173.045),
    ("Lu", 174.9668),
    ("Hf", 178.486),
    ("Ta", 180.94788),
    ("W", 183.84),
    ("Re", 186.207),
    ("Os", 190.23),
    ("Ir", 192.217),
    ("Pt", 195.084),
    ("Au", 196.966570),
    ("Hg", 200.592),
    ("Tl", 204.38),
    ("Pb", 207.2),
    ("Bi", 208.98040),
    ("Po", 209.0),
    ("At", 210.0),
    ("Rn", 222.0),
    ("Fr", 223.0),
    ("Ra", 226.0),
    ("Ac", 227.0),
    ("Th", 232.0377),
    ("Pa", 231.03588),
    ("U", 238.02891),
    ("Np", 237.0),
    ("Pu", 244.0),
    ("Am", 243.0),
    ("Cm", 247.0),
    ("Bk", 247.0),
    ("Cf", 251.0),
    ("Es", 252.0),
    ("Fm", 257.0),
    ("Md", 258.0),
    ("No", 259.0),
    ("Lr", 262.0),
    ("Rf", 267.0),
    ("Db", 270.0),
    ("Sg", 269.0),
    ("Bh", 270.0),
    ("Hs", 270.0),
    ("Mt", 278.0),
    ("Ds", 281.0),
    ("Rg", 281.0),
    ("Cn", 285.0),
    ("Nh", 286.0),
    ("Fl", 289.0),
    ("Mc", 289.0),
    ("Lv", 293.0),
    ("Ts", 293.0),
    ("Og", 294.0),
];

fn atomic_weight_amu(element: Element) -> f64 {
    ATOMIC_WEIGHTS
        .iter()
        .find(|&&(sym, _)| sym == element.symbol())
        .map(|&(_, w)| w)
        .expect("ATOMIC_WEIGHTS covers every ELEMENT_SYMBOLS entry -- checked in tests")
}

/// Bartel et al. 2018 Eq. 6: a composition-ratio-weighted pairwise
/// combination of atomic masses. `None` for a single-element composition
/// (no pairs exist -- callers must special-case pure elements the same way
/// pymatgen's `gf_sisso` does, returning a zero correction, since a pure
/// element's formation enthalpy is 0 by definition and has no SISSO
/// correction to add).
///
/// Deliberately computed over gugen's own un-reduced `Composition` (which
/// never GCD-reduces, unlike pymatgen's `reduced_composition` -- a
/// load-bearing gugen design choice for doped/solid-solution formulas,
/// `composition.rs`) rather than reducing to a minimal integer formula
/// first: verified via pymatgen directly that this formula is scale
/// invariant (`BaTiO3` and `Ba2Ti2O6` give the identical reduced mass,
/// since every term is a ratio of same-degree homogeneous sums of the
/// composition's amounts), so no reduction step is needed or correct to
/// add.
pub fn reduced_mass_amu(composition: &Composition) -> Option<f64> {
    let n_elems = composition.len();
    if n_elems < 2 {
        return None;
    }
    let total_atoms: f64 = composition.iter().map(|(_, amount)| amount).sum();
    let denominator = (n_elems as f64 - 1.0) * total_atoms;

    let entries: Vec<(Element, f64)> = composition.iter().collect();
    let mut mass_sum = 0.0;
    for i in 0..entries.len() {
        for j in (i + 1)..entries.len() {
            let (elem_i, alpha_i) = entries[i];
            let (elem_j, alpha_j) = entries[j];
            let m_i = atomic_weight_amu(elem_i);
            let m_j = atomic_weight_amu(elem_j);
            mass_sum += (alpha_i + alpha_j) * (m_i * m_j) / (m_i + m_j);
        }
    }
    Some(mass_sum / denominator)
}

/// A validated temperature within the range Bartel et al. 2018 actually
/// validated the SISSO descriptor against, `[300.0, 1800.0]` K
/// (deliberately narrower than pymatgen's `[300.0, 2000.0]` K -- see this
/// module's doc comment).
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Kelvin(f64);

impl Kelvin {
    pub const MIN: f64 = 300.0;
    pub const MAX: f64 = 1800.0;

    pub fn new(value: f64) -> Result<Self> {
        require_finite("Kelvin", value)?;
        if !(Self::MIN..=Self::MAX).contains(&value) {
            return Err(GugenError::InvalidRange {
                min: Self::MIN,
                max: Self::MAX,
            });
        }
        Ok(Self(value))
    }

    pub fn value(&self) -> f64 {
        self.0
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Kelvin {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Kelvin::new(value).map_err(serde::de::Error::custom)
    }
}

/// Bartel et al. 2018 Eq. 4: the SISSO-learned correction added to a 0 K
/// formation enthalpy to estimate a finite-temperature formation Gibbs
/// energy, *before* any elemental-reference re-basing (this module never
/// applies that re-basing -- see the module doc comment). Coefficients
/// read directly from pymatgen's `GibbsComputedStructureEntry._g_delta_sisso`
/// (MIT-licensed code implementing the CC BY 4.0 paper's own published
/// equation), not recalled from memory.
///
/// `volume_angstrom3_per_atom` and `reduced_mass_amu` are assumed already
/// valid (finite, positive) -- this is an internal pure-math function
/// called only from [`relative_solid_gibbs_ev_per_atom`], which sources
/// both from an already-validated [`SolidThermodynamicEntry`].
fn g_delta_sisso_ev_per_atom(
    volume_angstrom3_per_atom: f64,
    reduced_mass_amu: f64,
    temperature: Kelvin,
) -> f64 {
    let t = temperature.value();
    (-2.48e-4 * volume_angstrom3_per_atom.ln()
        - 8.94e-5 * reduced_mass_amu / volume_angstrom3_per_atom)
        * t
        + 0.181 * t.ln()
        - 0.882
}

/// Identifies the specific dataset a batch of [`SolidThermodynamicEntry`]
/// values was drawn from -- deliberately separate from `CompetingPhase`
/// (`reaction.rs`), which stays exactly as it was in v0.3.0: adding these
/// fields to that existing type would have changed its meaning from "a
/// simple competing-candidate context note" to a data-provenance-tracking
/// type, a caller-facing contract change advisor recommended against.
/// Every field is caller-supplied, free text -- gugen never fetches or
/// validates dataset identity itself (AGENTS.md §8/§25, same "gugen never
/// queries an external API" discipline `materials_project_adapter.rs`
/// documents for `CompetingPhase`).
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ThermodynamicDatasetIdentity {
    pub source: String,
    pub release: String,
    pub compatibility_scheme: String,
    pub snapshot_checksum: String,
}

/// A caller-supplied 0 K formation enthalpy and crystal-structure volume
/// for one solid phase, plus which dataset/release/correction-scheme it
/// came from -- the Phase 19P input type, deliberately new rather than an
/// extension of `CompetingPhase` (see [`ThermodynamicDatasetIdentity`]'s
/// doc comment). gugen never fetches this data itself; the caller has
/// already queried a real thermochemical/structural database (e.g.
/// Materials Project) and pre-fetched it, mirroring
/// `MaterialsProjectSnapshotProvider`'s existing contract.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SolidThermodynamicEntry {
    pub composition: Composition,
    pub phase_id: Option<String>,
    pub formation_enthalpy_ev_per_atom: f64,
    pub volume_angstrom3_per_atom: f64,
    pub dataset: ThermodynamicDatasetIdentity,
}

impl SolidThermodynamicEntry {
    pub fn new(
        composition: Composition,
        phase_id: Option<String>,
        formation_enthalpy_ev_per_atom: f64,
        volume_angstrom3_per_atom: f64,
        dataset: ThermodynamicDatasetIdentity,
    ) -> Result<Self> {
        require_finite(
            "formation_enthalpy_ev_per_atom",
            formation_enthalpy_ev_per_atom,
        )?;
        require_finite("volume_angstrom3_per_atom", volume_angstrom3_per_atom)?;
        if volume_angstrom3_per_atom <= 0.0 {
            return Err(GugenError::NonPositiveMagnitude {
                field: "volume_angstrom3_per_atom",
                value: volume_angstrom3_per_atom,
            });
        }
        Ok(Self {
            composition,
            phase_id,
            formation_enthalpy_ev_per_atom,
            volume_angstrom3_per_atom,
            dataset,
        })
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for SolidThermodynamicEntry {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct Raw {
            composition: Composition,
            phase_id: Option<String>,
            formation_enthalpy_ev_per_atom: f64,
            volume_angstrom3_per_atom: f64,
            dataset: ThermodynamicDatasetIdentity,
        }
        let raw = Raw::deserialize(deserializer)?;
        SolidThermodynamicEntry::new(
            raw.composition,
            raw.phase_id,
            raw.formation_enthalpy_ev_per_atom,
            raw.volume_angstrom3_per_atom,
            raw.dataset,
        )
        .map_err(serde::de::Error::custom)
    }
}

/// `formation_enthalpy_ev_per_atom + Gδ(T)` -- a finite-temperature
/// formation-Gibbs-energy estimate *relative to the same 0 K elemental
/// reference frame `formation_enthalpy_ev_per_atom` already uses*, not an
/// absolute value (this module never computes or needs the elemental
/// re-basing term -- see the module doc comment). Meaningful only in
/// *differences* between entries sharing the same total elemental
/// inventory (a balanced reaction, or a decomposition margin) -- see
/// [`balanced_reaction_delta_ev_per_atom`].
///
/// A pure element (`entry.composition.len() == 1`) returns
/// `formation_enthalpy_ev_per_atom` unchanged with no SISSO correction,
/// matching pymatgen's `gf_sisso`'s `if comp.is_element(): return 0` early
/// return (a pure element's formation enthalpy is 0 by definition; nothing
/// here checks or enforces that gugen's caller actually passed 0 for an
/// elemental entry -- same "trust the caller-supplied value" contract
/// `CompetingPhase` already has).
pub fn relative_solid_gibbs_ev_per_atom(
    entry: &SolidThermodynamicEntry,
    temperature: Kelvin,
) -> f64 {
    let Some(reduced_mass) = reduced_mass_amu(&entry.composition) else {
        return entry.formation_enthalpy_ev_per_atom;
    };
    let g_delta =
        g_delta_sisso_ev_per_atom(entry.volume_angstrom3_per_atom, reduced_mass, temperature);
    entry.formation_enthalpy_ev_per_atom + g_delta
}

/// Among entries sharing `composition` exactly, the one with the lowest 0 K
/// `formation_enthalpy_ev_per_atom` -- the "most stable known phase"
/// convention `MaterialsProjectSnapshotProvider::energy_for` already uses,
/// order-independent by construction (`fold` over all matches, not the
/// first one found). Deliberately selects by the *0 K* value, not the
/// finite-temperature one -- see the module doc comment's polymorph-scope
/// note.
fn most_stable_entry_for<'a>(
    entries: &'a [SolidThermodynamicEntry],
    composition: &Composition,
) -> Option<&'a SolidThermodynamicEntry> {
    // `Iterator::min_by` returns the *first* minimal element on a tie, so
    // comparing by `formation_enthalpy_ev_per_atom` alone would silently
    // reintroduce input-order dependence for two entries that happen to
    // share the same 0 K enthalpy but differ in volume -- exactly the
    // class of bug Phase 19 exists to close. Breaking the tie by volume
    // too doesn't guarantee a *unique* winner when both are equal, but at
    // that point the two entries are indistinguishable for every purpose
    // this function's caller has (`relative_solid_gibbs_ev_per_atom` only
    // reads `formation_enthalpy_ev_per_atom`/`volume_angstrom3_per_atom`/
    // `composition`), so either produces the identical output.
    entries
        .iter()
        .filter(|e| &e.composition == composition)
        .min_by(|a, b| {
            a.formation_enthalpy_ev_per_atom
                .total_cmp(&b.formation_enthalpy_ev_per_atom)
                .then_with(|| {
                    a.volume_angstrom3_per_atom
                        .total_cmp(&b.volume_angstrom3_per_atom)
                })
        })
}

/// Products' total `relative_solid_gibbs` minus reactants' total, weighted
/// by each species' `ReactionSpecies::coefficient` and formula-unit atom
/// count, normalized per atom of the reactant side (mirrors
/// `MaterialsProjectSnapshotProvider::reaction_energy`'s exact convention
/// and doc comment for why the reactant side is used).
///
/// Returns `None` -- never a partial sum -- the moment any participating
/// species' exact `Composition` has no matching entry in `entries`. This
/// is the concrete mechanism that keeps this module gas-free without ever
/// classifying a species as "gas": a caller who only supplies solid-phase
/// `SolidThermodynamicEntry` values (the only kind this type can represent
/// -- it has a crystal volume) simply never has an entry for `CO2`,
/// `H2O`, `O2`, etc., so any reaction releasing or consuming one abstains
/// here automatically.
///
/// **Precondition, not checked at runtime**: `reaction` must be genuinely
/// element-balanced (every `balance.rs`-produced `BalancedReaction` is).
/// `BalancedReaction::new` itself only rejects an empty side or a zero
/// coefficient, not an element imbalance -- for a hand-constructed,
/// unbalanced reaction, "normalize per atom of the reactant side" would
/// silently misrepresent the result, since the per-atom convention (like
/// `MaterialsProjectSnapshotProvider::reaction_energy`'s identical one)
/// relies on the reactant- and product-side atom totals being equal by
/// element conservation.
pub fn balanced_reaction_delta_ev_per_atom(
    reaction: &BalancedReaction,
    entries: &[SolidThermodynamicEntry],
    temperature: Kelvin,
) -> Option<f64> {
    let mut product_total = 0.0;
    for species in &reaction.products {
        let entry = most_stable_entry_for(entries, &species.composition)?;
        let atoms: f64 = species.composition.iter().map(|(_, amt)| amt).sum();
        product_total += species.coefficient as f64
            * atoms
            * relative_solid_gibbs_ev_per_atom(entry, temperature);
    }

    let mut reactant_total = 0.0;
    let mut reactant_atoms = 0.0;
    for species in &reaction.reactants {
        let entry = most_stable_entry_for(entries, &species.composition)?;
        let atoms: f64 = species.composition.iter().map(|(_, amt)| amt).sum();
        reactant_total += species.coefficient as f64
            * atoms
            * relative_solid_gibbs_ev_per_atom(entry, temperature);
        reactant_atoms += species.coefficient as f64 * atoms;
    }

    Some((product_total - reactant_total) / reactant_atoms)
}

/// How far apart `atoms`-weighted amounts must sum to still count as "the
/// same total composition" -- generous enough to absorb ordinary floating-
/// point summation error across a handful of terms, tight enough that a
/// genuinely different composition can never pass by coincidence (real
/// synthesis-target element amounts are never within `1e-6` of each other
/// by chance in this crate's own fixtures, `AGENTS.md`'s worked examples
/// among them).
const COMPOSITION_CONSERVATION_TOLERANCE: f64 = 1e-6;

/// The energy margin between `target` and one specific, caller-named
/// alternative combination of phases covering the exact same total
/// elemental composition -- e.g. "does `BaTiO3` have lower Gibbs energy
/// than `BaO + TiO2`". **Deliberately not a hull search or an automatic
/// "find the best decomposition" function** (the owner's original Phase
/// 19P sketch asked for a "small binary/ternary hull"; this is a
/// considered substitution, not a silent scope cut -- see this phase's
/// work report). A hull search would require gugen itself to decide which
/// candidate assemblages to enumerate from `entries`, so a returned margin
/// would only ever mean "no cheaper decomposition among the phases this
/// particular caller happened to supply," not "the target is
/// thermodynamically stable" -- a reader could easily misread the former
/// as the latter, which is exactly the false-confidence risk the owner's
/// own stop-and-report list names. Requiring the caller to name the
/// specific alternative assemblage keeps the claim exactly as narrow as
/// what was actually computed.
///
/// Sign convention matches [`ReactionEnergy`](crate::ReactionEnergy) and
/// [`balanced_reaction_delta_ev_per_atom`]'s "later state minus reference
/// state": `alternative_assemblage`'s total minus `target`'s total,
/// normalized per atom of `target`'s own composition. **Negative** means
/// the alternative assemblage is lower in energy (more stable) than
/// `target` -- i.e. `target` is thermodynamically disfavored relative to
/// that specific alternative. **Positive** means `target` is favored over
/// that alternative.
///
/// `alternative_assemblage` is `(entry, amount)` pairs, `amount` being how
/// many formula units of that phase the assemblage contains. Returns
/// `None` -- never a best-effort number -- if: any referenced entry
/// (`target` or any assemblage member) has no `SolidThermodynamicEntry`
/// data reachable (this function takes entries directly rather than
/// looking them up, so this only applies to `target` itself needing valid
/// data, which its type already guarantees); or, the concrete check this
/// function exists to make, if `alternative_assemblage`'s amount-weighted
/// total composition does not match `target.composition` element-for-
/// element within `COMPOSITION_CONSERVATION_TOLERANCE` (`1e-6`) -- an assemblage
/// that doesn't conserve composition is not a real alternative to
/// `target` and comparing their energies would not mean what a caller
/// would assume it means.
pub fn decomposition_margin_ev_per_atom(
    target: &SolidThermodynamicEntry,
    alternative_assemblage: &[(SolidThermodynamicEntry, f64)],
    temperature: Kelvin,
) -> Option<f64> {
    let mut assemblage_composition: BTreeMap<Element, f64> = BTreeMap::new();
    for (entry, amount) in alternative_assemblage {
        for (element, elem_amount) in entry.composition.iter() {
            *assemblage_composition.entry(element).or_insert(0.0) += amount * elem_amount;
        }
    }

    let target_composition: BTreeMap<Element, f64> = target.composition.iter().collect();

    if assemblage_composition.len() != target_composition.len() {
        return None;
    }
    for (element, target_amount) in &target_composition {
        let assemblage_amount = assemblage_composition.get(element)?;
        if (assemblage_amount - target_amount).abs() > COMPOSITION_CONSERVATION_TOLERANCE {
            return None;
        }
    }

    let target_atoms: f64 = target.composition.iter().map(|(_, amt)| amt).sum();
    let target_total = target_atoms * relative_solid_gibbs_ev_per_atom(target, temperature);

    let assemblage_total: f64 = alternative_assemblage
        .iter()
        .map(|(entry, amount)| {
            let atoms: f64 = entry.composition.iter().map(|(_, amt)| amt).sum();
            amount * atoms * relative_solid_gibbs_ev_per_atom(entry, temperature)
        })
        .sum();

    Some((assemblage_total - target_total) / target_atoms)
}

/// One named alternative assemblage compared against a target, and the
/// resulting margin -- the `decomposition_margin_ev_per_atom` result
/// paired with the caller's own free-text label for what it compared
/// against (e.g. `"BaO + TiO2"`), since the raw function alone has no
/// way to say what `alternative_assemblage` *was* once collapsed into a
/// single `Option<f64>`.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DecompositionComparison {
    pub alternative_description: String,
    pub margin_ev_per_atom: Option<f64>,
}

/// One target's finite-temperature thermodynamic picture at a given
/// temperature -- raw physical quantities only, structurally mirroring
/// [`RouteSuitabilityAssessment`](crate::RouteSuitabilityAssessment)'s
/// own shape (Phase 15A): a vessel of independent, never-force-merged
/// results plus its own `limitations`, not a verdict. Unlike
/// `RouteSuitabilityAssessment`'s `Supports`/`Contradicts`/`Unknown`
/// findings, nothing here is a judgment -- every field is a number this
/// module actually computed, or `None` where it couldn't. **Never read by
/// `score_plan`**: `thermodynamic_support` stays `None` regardless of
/// what this type holds (checked as a permanent regression guard by
/// `tests/thermodynamics_ranking_invariance.rs`, not left as an
/// unverified claim). `decomposition_comparisons` is a `Vec` for the same
/// reason `RouteSuitabilityAssessment.findings` is: a caller may name
/// more than one alternative assemblage, and none should be silently
/// dropped or merged into an aggregate.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ThermodynamicSelectivityAssessment {
    pub temperature: Kelvin,
    pub reaction_delta_ev_per_atom: Option<f64>,
    pub decomposition_comparisons: Vec<DecompositionComparison>,
    pub dataset: ThermodynamicDatasetIdentity,
    pub limitations: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reaction::ReactionSpecies;

    fn element(symbol: &str) -> Element {
        Element::new(symbol).unwrap()
    }

    fn composition(pairs: &[(&str, f64)]) -> Composition {
        Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
    }

    fn dataset() -> ThermodynamicDatasetIdentity {
        ThermodynamicDatasetIdentity {
            source: "test".to_string(),
            release: "2026.08".to_string(),
            compatibility_scheme: "test-scheme".to_string(),
            snapshot_checksum: "deadbeef".to_string(),
        }
    }

    #[test]
    fn atomic_weights_table_covers_every_element_symbol_exactly_once() {
        use crate::composition::ELEMENT_SYMBOLS;
        assert_eq!(ATOMIC_WEIGHTS.len(), ELEMENT_SYMBOLS.len());
        for sym in ELEMENT_SYMBOLS {
            assert!(
                ATOMIC_WEIGHTS.iter().any(|&(s, _)| s == sym),
                "missing atomic weight for {sym}"
            );
        }
        let unique: std::collections::BTreeSet<&str> =
            ATOMIC_WEIGHTS.iter().map(|&(s, _)| s).collect();
        assert_eq!(
            unique.len(),
            ATOMIC_WEIGHTS.len(),
            "duplicate symbol in table"
        );
    }

    #[test]
    fn kelvin_rejects_outside_bartel_2018_validated_range() {
        assert!(Kelvin::new(299.9).is_err());
        assert!(Kelvin::new(1800.1).is_err());
        assert!(Kelvin::new(300.0).is_ok());
        assert!(Kelvin::new(1800.0).is_ok());
        assert!(Kelvin::new(900.0).is_ok());
    }

    #[test]
    fn solid_thermodynamic_entry_rejects_non_positive_volume() {
        let result = SolidThermodynamicEntry::new(
            composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
            None,
            -1.5,
            0.0,
            dataset(),
        );
        assert!(result.is_err());
    }

    /// Scale invariance: `BaTiO3` and `Ba2Ti2O6` must give the identical
    /// reduced mass -- verified directly against pymatgen's own
    /// `_reduced_mass` before writing this implementation, which gave
    /// 17.627455183378554 for the same two compositions. This
    /// implementation gives 17.627236955076057 -- a 2.2e-4 amu (1.2e-5
    /// relative) difference traced directly to pymatgen's O atomic weight
    /// (15.9994, `Composition("O").weight`) vs. this module's IUPAC 2021
    /// "conventional/abridged" value (15.999, `AtWt21.html`'s own
    /// published table) -- both legitimate, independently-sourced
    /// standard atomic weights that were never going to match past ~5
    /// significant figures, not a formula bug. The generous tolerance
    /// below reflects that; the real formula-equivalence check
    /// (`g_delta_sisso_ev_per_atom` against pymatgen's `_g_delta_sisso`
    /// on identical explicit inputs, sidestepping atomic-weight-table
    /// choice entirely) is
    /// `g_delta_sisso_matches_pymatgen_across_the_validated_temperature_range`,
    /// below.
    #[test]
    fn reduced_mass_is_scale_invariant_matching_pymatgen() {
        let batio3 = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let scaled = composition(&[("Ba", 2.0), ("Ti", 2.0), ("O", 6.0)]);
        let rm1 = reduced_mass_amu(&batio3).unwrap();
        let rm2 = reduced_mass_amu(&scaled).unwrap();
        assert!((rm1 - rm2).abs() < 1e-9);
        assert!((rm1 - 17.627455183378554).abs() < 1e-3);
    }

    #[test]
    fn reduced_mass_is_none_for_a_pure_element() {
        let fe = composition(&[("Fe", 1.0)]);
        assert_eq!(reduced_mass_amu(&fe), None);
    }

    /// **pymatgen differential validation (Phase 19P).** Pinned values from
    /// pymatgen 2026.5.4's `GibbsComputedStructureEntry._g_delta_sisso`
    /// (MIT-licensed code implementing the CC BY 4.0 Bartel et al. 2018
    /// paper's own Eq. 4), called *directly* with explicit
    /// `(volume_angstrom3_per_atom, reduced_mass_amu, temperature_kelvin)`
    /// triples -- never through a `Composition`, so this is a pure
    /// formula-equivalence check, deliberately decoupled from
    /// atomic-weight-table choice (see
    /// `reduced_mass_is_scale_invariant_matching_pymatgen`'s own doc
    /// comment for why that coupling would have produced false failures).
    /// Synthetic inputs only -- no real material's volume, mass, or
    /// formation enthalpy -- so this validates that gugen's Rust
    /// reimplementation matches pymatgen's independent implementation of
    /// the same published equation, **not** that either implementation
    /// predicts real synthesis outcomes; that is a separate, unestablished
    /// claim this test does not make. Covers both ends of gugen's
    /// `Kelvin` range (300 K, 1800 K) plus interior points, and a range of
    /// volumes/masses spanning small/medium/large solids.
    ///
    /// Generated by, and reproducible from:
    /// ```python
    /// from pymatgen.analysis.compatibility.computed_entries import GibbsComputedStructureEntry
    /// for v, m, t in [(10.0, 10.0, 300.0), (10.0, 10.0, 1800.0), (20.0, 30.0, 300.0),
    ///                 (20.0, 30.0, 1800.0), (20.0, 30.0, 900.0), (45.0, 80.0, 500.0),
    ///                 (45.0, 80.0, 1300.0), (12.0, 17.627455183378554, 900.0),
    ///                 (5.0, 5.0, 300.0), (100.0, 150.0, 1800.0)]:
    ///     print(GibbsComputedStructureEntry._g_delta_sisso(v, m, t))
    /// ```
    #[test]
    fn g_delta_sisso_matches_pymatgen_across_the_validated_temperature_range() {
        let cases: [(f64, f64, f64, f64); 10] = [
            (10.0, 10.0, 300.0, -0.04774770300598463),
            (10.0, 10.0, 1800.0, -0.7141008936694916),
            (20.0, 30.0, 300.0, -0.11272785323964452),
            (20.0, 30.0, 1800.0, -1.103981795071451),
            (20.0, 30.0, 900.0, -0.44010399129555045),
            (45.0, 80.0, 500.0, -0.30864874958376987),
            (45.0, 80.0, 1300.0, -1.0180896826709018),
            (12.0, 17.627455183378554, 900.0, -0.32358979907553453),
            (5.0, 5.0, 300.0, 0.0038224472276753296),
            (100.0, 150.0, 1800.0, -1.8224348791820337),
        ];
        for (volume, reduced_mass, temp_k, expected) in cases {
            let t = Kelvin::new(temp_k).unwrap();
            let actual = g_delta_sisso_ev_per_atom(volume, reduced_mass, t);
            assert!(
                (actual - expected).abs() < 1e-9,
                "V={volume} m={reduced_mass} T={temp_k}: expected {expected}, got {actual}"
            );
        }
    }

    /// **pymatgen differential validation, reaction level.** The toy
    /// `AB + CD -> AC + BD` reaction from Phase 19P's plan review,
    /// re-verified at Rust-implementation time rather than only during
    /// planning: pymatgen's full `GibbsComputedStructureEntry.energy`
    /// pipeline gives the same reaction delta as gugen's `sum_g_i`-free
    /// `balanced_reaction_delta_ev_per_atom`, confirming the cancellation
    /// this module's whole design depends on still holds at the actual
    /// implementation, not just the hand-verified toy case from planning.
    /// A toy `NaCl + KBr -> NaBr + KCl` swap (real element symbols,
    /// synthetic volumes/formation-enthalpies -- no real material's data).
    ///
    /// The pinned `expected` value below is a genuine two-step
    /// cross-implementation check, not a self-comparison: step 1 used
    /// *gugen's own* `reduced_mass_amu` (this module's Eq. 6
    /// implementation, already separately validated above) to compute
    /// each species' reduced mass from its real composition; step 2 fed
    /// those exact numbers into *pymatgen's* `_g_delta_sisso` (Eq. 4) to
    /// get the reaction delta, entirely outside gugen. Reduced masses
    /// (from `reduced_mass_amu`, printed via a throwaway example binary):
    /// `rm(NaCl)=13.945765546595258`, `rm(KBr)=26.252522541160971`,
    /// `rm(NaBr)=17.853117223747990`, `rm(KCl)=18.592439197137963`.
    ///
    /// Generated by:
    /// ```python
    /// from pymatgen.analysis.compatibility.computed_entries import GibbsComputedStructureEntry
    /// g = GibbsComputedStructureEntry._g_delta_sisso
    /// T = 900.0
    /// g_ab = -1.5 + g(4.0**3, 13.945765546595258, T)   # NaCl
    /// g_cd = -1.2 + g(5.0**3, 26.252522541160971, T)   # KBr
    /// g_ac = -1.8 + g(4.3**3, 17.853117223747990, T)   # NaBr
    /// g_bd = -1.1 + g(4.7**3, 18.592439197137963, T)   # KCl
    /// print((2*g_ac + 2*g_bd - 2*g_ab - 2*g_cd) / 4.0)
    /// # -> -0.10251961226707129
    /// ```
    #[test]
    fn balanced_reaction_delta_matches_an_independent_pymatgen_computation() {
        let ab = composition(&[("Na", 1.0), ("Cl", 1.0)]);
        let cd = composition(&[("K", 1.0), ("Br", 1.0)]);
        let ac = composition(&[("Na", 1.0), ("Br", 1.0)]);
        let bd = composition(&[("K", 1.0), ("Cl", 1.0)]);

        let entries = vec![
            SolidThermodynamicEntry::new(ab.clone(), None, -1.5, 4.0_f64.powi(3), dataset())
                .unwrap(),
            SolidThermodynamicEntry::new(cd.clone(), None, -1.2, 5.0_f64.powi(3), dataset())
                .unwrap(),
            SolidThermodynamicEntry::new(ac.clone(), None, -1.8, 4.3_f64.powi(3), dataset())
                .unwrap(),
            SolidThermodynamicEntry::new(bd.clone(), None, -1.1, 4.7_f64.powi(3), dataset())
                .unwrap(),
        ];
        let reaction = BalancedReaction::new(
            vec![
                ReactionSpecies {
                    composition: ab,
                    coefficient: 1,
                },
                ReactionSpecies {
                    composition: cd,
                    coefficient: 1,
                },
            ],
            vec![
                ReactionSpecies {
                    composition: ac,
                    coefficient: 1,
                },
                ReactionSpecies {
                    composition: bd,
                    coefficient: 1,
                },
            ],
        )
        .unwrap();

        let t = Kelvin::new(900.0).unwrap();
        let actual = balanced_reaction_delta_ev_per_atom(&reaction, &entries, t).unwrap();
        let expected = -0.10251961226707129;

        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn balanced_reaction_delta_abstains_when_a_species_has_no_entry() {
        let feo = composition(&[("Fe", 1.0), ("O", 1.0)]);
        let fe2o2 = composition(&[("Fe", 2.0), ("O", 2.0)]);
        let entries = vec![
            SolidThermodynamicEntry::new(feo.clone(), None, -2.0, 12.0, dataset()).unwrap(),
            // fe2o2 (the product) is deliberately missing.
        ];
        let reaction = BalancedReaction::new(
            vec![ReactionSpecies {
                composition: feo,
                coefficient: 2,
            }],
            vec![ReactionSpecies {
                composition: fe2o2,
                coefficient: 1,
            }],
        )
        .unwrap();

        let result =
            balanced_reaction_delta_ev_per_atom(&reaction, &entries, Kelvin::new(900.0).unwrap());
        assert_eq!(result, None);
    }

    #[test]
    fn balanced_reaction_delta_picks_the_lowest_0k_energy_among_duplicate_compositions() {
        let feo = composition(&[("Fe", 1.0), ("O", 1.0)]);
        let fe2o2 = composition(&[("Fe", 2.0), ("O", 2.0)]);
        let reaction = BalancedReaction::new(
            vec![ReactionSpecies {
                composition: feo.clone(),
                coefficient: 2,
            }],
            vec![ReactionSpecies {
                composition: fe2o2.clone(),
                coefficient: 1,
            }],
        )
        .unwrap();

        let ascending = vec![
            SolidThermodynamicEntry::new(feo.clone(), None, -2.0, 12.0, dataset()).unwrap(),
            SolidThermodynamicEntry::new(feo.clone(), None, -5.0, 12.0, dataset()).unwrap(),
            SolidThermodynamicEntry::new(fe2o2.clone(), None, -3.0, 20.0, dataset()).unwrap(),
        ];
        let descending = vec![
            SolidThermodynamicEntry::new(feo.clone(), None, -5.0, 12.0, dataset()).unwrap(),
            SolidThermodynamicEntry::new(feo, None, -2.0, 12.0, dataset()).unwrap(),
            SolidThermodynamicEntry::new(fe2o2, None, -3.0, 20.0, dataset()).unwrap(),
        ];

        let t = Kelvin::new(900.0).unwrap();
        let a = balanced_reaction_delta_ev_per_atom(&reaction, &ascending, t).unwrap();
        let b = balanced_reaction_delta_ev_per_atom(&reaction, &descending, t).unwrap();
        assert_eq!(a, b, "must be order-independent regardless of entry order");
    }

    /// Advisor-caught tie-break bug: two entries sharing the exact same
    /// composition *and* 0 K formation enthalpy, differing only in volume,
    /// must not silently resolve to "whichever came first in the slice"
    /// (`Iterator::min_by`'s documented tie-break behavior) -- that would
    /// reintroduce input-order dependence at the entry-selection step even
    /// though `balanced_reaction_delta_picks_the_lowest_0k_energy_among_
    /// duplicate_compositions` above already covers the (more common)
    /// distinct-enthalpy case.
    #[test]
    fn balanced_reaction_delta_is_order_independent_even_when_duplicate_entries_tie_on_enthalpy() {
        let feo = composition(&[("Fe", 1.0), ("O", 1.0)]);
        let fe2o2 = composition(&[("Fe", 2.0), ("O", 2.0)]);
        let reaction = BalancedReaction::new(
            vec![ReactionSpecies {
                composition: feo.clone(),
                coefficient: 2,
            }],
            vec![ReactionSpecies {
                composition: fe2o2.clone(),
                coefficient: 1,
            }],
        )
        .unwrap();

        // Both feo entries share the identical -2.0 eV/atom enthalpy;
        // only their volume (12.0 vs. 20.0) differs, so a pure
        // enthalpy-only tie-break would pick whichever is listed first.
        let forward = vec![
            SolidThermodynamicEntry::new(feo.clone(), None, -2.0, 12.0, dataset()).unwrap(),
            SolidThermodynamicEntry::new(feo.clone(), None, -2.0, 20.0, dataset()).unwrap(),
            SolidThermodynamicEntry::new(fe2o2.clone(), None, -3.0, 20.0, dataset()).unwrap(),
        ];
        let reversed = vec![
            SolidThermodynamicEntry::new(feo.clone(), None, -2.0, 20.0, dataset()).unwrap(),
            SolidThermodynamicEntry::new(feo, None, -2.0, 12.0, dataset()).unwrap(),
            SolidThermodynamicEntry::new(fe2o2, None, -3.0, 20.0, dataset()).unwrap(),
        ];

        let t = Kelvin::new(900.0).unwrap();
        let a = balanced_reaction_delta_ev_per_atom(&reaction, &forward, t).unwrap();
        let b = balanced_reaction_delta_ev_per_atom(&reaction, &reversed, t).unwrap();
        assert_eq!(
            a, b,
            "an enthalpy tie must not let entry-list order decide the volume used"
        );
    }

    /// The owner's own worked example ("does `BaTiO3` have lower Gibbs
    /// energy than `BaO + TiO2`"): 1 mol `BaO` + 1 mol `TiO2` conserves
    /// `BaTiO3`'s exact composition (Ba:1, Ti:1, O:1+2=3), so this must
    /// succeed and its value must equal the direct hand-computed
    /// aggregation over the already-pymatgen-validated
    /// `relative_solid_gibbs_ev_per_atom` -- this test checks the
    /// aggregation/normalization arithmetic `decomposition_margin_ev_per_atom`
    /// adds on top of that already-validated primitive, not the SISSO
    /// formula itself again. Synthetic formation enthalpies/volumes, not
    /// real materials data.
    #[test]
    fn decomposition_margin_computes_batio3_vs_bao_plus_tio2() {
        let batio3 = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let bao = composition(&[("Ba", 1.0), ("O", 1.0)]);
        let tio2 = composition(&[("Ti", 1.0), ("O", 2.0)]);

        let target = SolidThermodynamicEntry::new(batio3, None, -3.5, 60.0, dataset()).unwrap();
        let bao_entry = SolidThermodynamicEntry::new(bao, None, -2.0, 20.0, dataset()).unwrap();
        let tio2_entry = SolidThermodynamicEntry::new(tio2, None, -3.0, 30.0, dataset()).unwrap();

        let t = Kelvin::new(900.0).unwrap();
        let actual = decomposition_margin_ev_per_atom(
            &target,
            &[(bao_entry.clone(), 1.0), (tio2_entry.clone(), 1.0)],
            t,
        )
        .unwrap();

        let g_target = relative_solid_gibbs_ev_per_atom(&target, t) * 5.0; // Ba+Ti+3*O
        let g_bao = relative_solid_gibbs_ev_per_atom(&bao_entry, t) * 2.0; // Ba+O
        let g_tio2 = relative_solid_gibbs_ev_per_atom(&tio2_entry, t) * 3.0; // Ti+2*O
        let expected = (g_bao + g_tio2 - g_target) / 5.0;

        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    /// A single `BaO` alone does not conserve `BaTiO3`'s composition (no
    /// `Ti`, wrong `O` count) -- this is not a real alternative to
    /// `BaTiO3` and must abstain rather than silently compute a
    /// meaningless number.
    #[test]
    fn decomposition_margin_abstains_when_composition_is_not_conserved() {
        let batio3 = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let bao = composition(&[("Ba", 1.0), ("O", 1.0)]);

        let target = SolidThermodynamicEntry::new(batio3, None, -3.5, 60.0, dataset()).unwrap();
        let bao_entry = SolidThermodynamicEntry::new(bao, None, -2.0, 20.0, dataset()).unwrap();

        let t = Kelvin::new(900.0).unwrap();
        let result = decomposition_margin_ev_per_atom(&target, &[(bao_entry, 1.0)], t);
        assert_eq!(result, None);
    }

    /// Sign convention, pinned directly: an alternative assemblage with a
    /// deliberately very negative (very stable) formation enthalpy must
    /// produce a **negative** margin (alternative below target -- target
    /// disfavored), matching this function's documented "alternative minus
    /// target" convention, the same direction `ReactionEnergy` and
    /// `balanced_reaction_delta_ev_per_atom` already use.
    #[test]
    fn decomposition_margin_is_negative_when_the_alternative_is_more_stable() {
        let batio3 = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let bao = composition(&[("Ba", 1.0), ("O", 1.0)]);
        let tio2 = composition(&[("Ti", 1.0), ("O", 2.0)]);

        // Target formation enthalpy deliberately much less negative
        // (less stable) than the alternative assemblage's.
        let target = SolidThermodynamicEntry::new(batio3, None, -1.0, 60.0, dataset()).unwrap();
        let bao_entry = SolidThermodynamicEntry::new(bao, None, -5.0, 20.0, dataset()).unwrap();
        let tio2_entry = SolidThermodynamicEntry::new(tio2, None, -5.0, 30.0, dataset()).unwrap();

        let t = Kelvin::new(900.0).unwrap();
        let margin =
            decomposition_margin_ev_per_atom(&target, &[(bao_entry, 1.0), (tio2_entry, 1.0)], t)
                .unwrap();
        assert!(
            margin < 0.0,
            "a much more stable alternative assemblage must give a negative margin, got {margin}"
        );
    }

    /// End-to-end construction, matching the intended usage pattern: every
    /// field sourced from this module's own already-tested primitives, no
    /// value invented ad hoc for the type. Not a `score_plan` input
    /// anywhere -- `tests/thermodynamics_ranking_invariance.rs` is the
    /// permanent regression guard for that boundary.
    #[test]
    fn thermodynamic_selectivity_assessment_assembles_from_the_primitives() {
        let batio3 = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);
        let bao = composition(&[("Ba", 1.0), ("O", 1.0)]);
        let tio2 = composition(&[("Ti", 1.0), ("O", 2.0)]);

        let target = SolidThermodynamicEntry::new(batio3, None, -3.5, 60.0, dataset()).unwrap();
        let bao_entry = SolidThermodynamicEntry::new(bao, None, -2.0, 20.0, dataset()).unwrap();
        let tio2_entry = SolidThermodynamicEntry::new(tio2, None, -3.0, 30.0, dataset()).unwrap();
        let t = Kelvin::new(900.0).unwrap();

        let margin =
            decomposition_margin_ev_per_atom(&target, &[(bao_entry, 1.0), (tio2_entry, 1.0)], t);

        let assessment = ThermodynamicSelectivityAssessment {
            temperature: t,
            reaction_delta_ev_per_atom: None,
            decomposition_comparisons: vec![DecompositionComparison {
                alternative_description: "BaO + TiO2".to_string(),
                margin_ev_per_atom: margin,
            }],
            dataset: dataset(),
            limitations: vec![
                "gas-free closed solid systems only; no thermodynamic_support connection"
                    .to_string(),
            ],
        };

        assert_eq!(assessment.decomposition_comparisons.len(), 1);
        assert!(
            assessment.decomposition_comparisons[0]
                .margin_ev_per_atom
                .is_some()
        );
    }
}
