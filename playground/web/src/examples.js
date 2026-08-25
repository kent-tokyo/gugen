// Curated, cited example targets -- mirrors tests/validation.rs's own
// fixtures() (5 curated literature routes, verified by
// every_literature_route_is_recovered_exactly). Not imported directly
// (tests/ isn't part of the published crate or reachable from JS); the
// target/candidate compositions below are transcribed exactly, so any
// future change to the Rust fixtures should be mirrored here too.

export const EXAMPLES = [
  {
    id: "batio3",
    name: "BaTiO₃",
    category: "Perovskite oxide — the strongest-attested route in this set",
    what: "Barium titanate, a ferroelectric ceramic widely used in capacitors.",
    look_for:
      "Both the cited BaCO₃ route and an alternative BaO route are accepted as separate, independently valid plans — a real multi-candidate case, not a single answer.",
    unresolved: "Firing temperature, hold time, atmosphere, and phase purity are all left unresolved.",
    citation:
      "1 BaCO₃ + 1 TiO₂ → BaTiO₃ + CO₂, attested by 83 independent paper DOIs in the Kononova et al. 2019 dataset (CC BY 4.0); confirmed by direct reading of Qi et al., “The Effect of Sputtering Target Density on the Crystal and Electronic Structure of Epitaxial BaTiO3 Thin Films,” Crystals 14(4), 304 (2024), DOI 10.3390/cryst14040304.",
    target_elements: { Ba: 1.0, Ti: 1.0, O: 3.0 },
    candidates: [
      { id: "BaCO3", elements: { Ba: 1.0, C: 1.0, O: 3.0 } },
      { id: "TiO2", elements: { Ti: 1.0, O: 2.0 } },
      { id: "BaO", elements: { Ba: 1.0, O: 1.0 } },
      { id: "NaCl", elements: { Na: 1.0, Cl: 1.0 } },
    ],
  },
  {
    id: "cao",
    name: "CaO",
    category: "Simple binary oxide — the smallest example in this set",
    what: "Calcium oxide (quicklime), the simplest possible target: one precursor, one decomposition step.",
    look_for:
      "A minimal route — useful for seeing what an uncluttered accepted plan and process-step table look like before the more complex examples.",
    unresolved: "Firing temperature and hold time are unresolved.",
    citation:
      "CaCO₃ → CaO + CO₂ at 900°C. Seesanong, Seangarun, Boonchom, Laohavisuti, Boonmee, Thompho, Rungrojchaipon, “Low-Cost and Eco-Friendly Calcium Oxide Prepared via Thermal Decompositions of Calcium Carbonate and Calcium Acetate Precursors Derived from Waste Oyster Shells,” Materials 17(15), 3875 (2024), DOI 10.3390/ma17153875.",
    target_elements: { Ca: 1.0, O: 1.0 },
    candidates: [
      { id: "CaCO3", elements: { Ca: 1.0, C: 1.0, O: 3.0 } },
      { id: "NaCl", elements: { Na: 1.0, Cl: 1.0 } },
    ],
  },
  {
    id: "lifepo4",
    name: "LiFePO₄",
    category: "Phosphate — a byproduct-releasing route",
    what: "Lithium iron phosphate, a lithium-ion battery cathode material.",
    look_for:
      "The balanced reaction releases CO₂ and O₂ as byproducts — gugen finds this by balancing against its own curated byproduct allow-list, not by assuming a simple 1:1 combination.",
    unresolved:
      "Firing temperature, hold time, and atmosphere are unresolved; carbon-coating/carbothermal-reduction routes reported in some cited papers are a different mechanism this route does not model.",
    citation:
      "FePO₄ + Li₂CO₃ attested across 6 independent DOIs in the Kononova et al. 2019 dataset; representative entry Chang, Lv, Tang, Li, Yuan, Wang, “Synthesis and characterization of high-density LiFePO4/C composites as cathode materials for lithium-ion batteries,” Electrochimica Acta (2009), DOI 10.1016/j.electacta.2009.03.063. The exact byproduct-releasing reaction (4 FePO₄ + 2 Li₂CO₃ → 4 LiFePO₄ + 2 CO₂ + O₂) is gugen's own balance() output, not a claim about which mechanism any specific paper used.",
    target_elements: { Li: 1.0, Fe: 1.0, P: 1.0, O: 4.0 },
    candidates: [
      { id: "FePO4", elements: { Fe: 1.0, P: 1.0, O: 4.0 } },
      { id: "Li2CO3", elements: { Li: 2.0, C: 1.0, O: 3.0 } },
      { id: "NaCl", elements: { Na: 1.0, Cl: 1.0 } },
    ],
  },
  {
    id: "mgal2o4",
    name: "MgAl₂O₄",
    category: "Spinel oxide — a second multi-candidate case",
    what: "Magnesium aluminate spinel, used in refractories and optical windows.",
    look_for:
      "Both the cited MgO route and an alternative MgCO₃ route are accepted as separate plans — the same “more than one valid precursor” pattern as BaTiO₃, with a different chemistry.",
    unresolved: "Firing temperature, hold time, and atmosphere are unresolved.",
    citation:
      "1 Al₂O₃ + 1 MgO → MgAl₂O₄, attested by 16 independent paper DOIs in the Kononova et al. 2019 dataset; representative entry DOI 10.1007/s11663-014-0207-8.",
    target_elements: { Mg: 1.0, Al: 2.0, O: 4.0 },
    candidates: [
      { id: "MgO", elements: { Mg: 1.0, O: 1.0 } },
      { id: "Al2O3", elements: { Al: 2.0, O: 3.0 } },
      { id: "MgCO3", elements: { Mg: 1.0, C: 1.0, O: 3.0 } },
      { id: "NaCl", elements: { Na: 1.0, Cl: 1.0 } },
    ],
  },
];
