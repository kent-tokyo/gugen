//! Phase 22: connects gugen's chemical planning output to real purchasable
//! products, without ever feeding commercial data back into scientific
//! planning. Two strictly separate stages: chemical planning (existing
//! `Planner`/`score_plan`) produces a `SynthesisPlan`; this module then
//! matches that plan's precursor compositions against a caller-supplied
//! catalog of commercial offers (price, purity, package size, supplier).
//!
//! **Never mutates `SynthesisPlan`.** `assess_commercial_precursors`/
//! `assess_commercial_plans` take `&SynthesisPlan` and return a wholly
//! separate `CommercialPlanAssessment` -- score, confidence, the balanced
//! reaction, and process steps are structurally unreachable from this
//! module (mirrors `literature_evidence.rs`'s "reference-only, by
//! construction" boundary: this is a separate return type from a separate
//! function, never threaded into `score_plan`'s signature or as a new
//! `SynthesisPlan` field).
//!
//! **No network access anywhere in this module.** Product URLs are stored
//! strings, never fetched. Catalogs are loaded from caller-supplied CSV/JSON
//! text only.
//!
//! **Commercial offer matching uses a canonical, scale-invariant
//! composition ratio -- not literal `Composition::eq`.** Two formulas
//! written at different formula-unit scale (`Fe2O3` vs `Fe4O6`) are the
//! same substance and match here, via exact-rational GCD reduction of the
//! whole element-ratio vector (never floating-point ratio comparison --
//! see `canonical_ratio_key`). This is scale-invariant canonicalization of
//! *one supplied formula*, not alias or substitute-precursor inference:
//! gugen's own `Composition::eq` (used everywhere else in the crate, e.g.
//! reaction balancing) is untouched and stays strictly literal. Formulas
//! with a genuinely different ratio (`FeO` vs `Fe2O3`), a different
//! element set (different oxides, carbonates, doped vs. undoped hosts), or
//! a hydrate vs. its anhydrous form (`CaSO4` vs `CaSO4·2H2O` -- the
//! formula parser folds hydrate water into the flat element-amount map,
//! giving them different, non-proportional atom counts) remain distinct:
//! canonicalization only reduces a shared ratio to lowest terms, it never
//! bridges formulas that aren't proportional to begin with.
//!
//! **gugen does not certify commercial data.** Catalog values are supplied
//! data; prices are estimates; availability may be stale; product
//! suitability for a given synthesis is not certified. Vendor documentation
//! and SDS sheets must be checked separately.

mod assessment;
mod formula;
mod loader;
mod matching;
mod model;
mod quantity;
mod search;
#[cfg(test)]
mod test_support;

pub use assessment::{assess_commercial_plans, assess_commercial_precursors};
pub use model::{
    AvailabilityStatus, CasNumber, CommercialCatalogColumnMap, CommercialCatalogError,
    CommercialCatalogLoadMode, CommercialCatalogLoadReport, CommercialCombination,
    CommercialExclusion, CommercialExclusionCode, CommercialOfferId, CommercialOfferSelection,
    CommercialPlanAssessment, CommercialPlanningConfig, CommercialPlanningRequest,
    CommercialPrecursorCatalog, CommercialPrecursorOffer, CommercialSourceType, CommercialWarning,
    CurrencyCode, MissingCommercialDataPolicy, Money, OfferProvenance, PackageMass,
    ParticleSizeRangeUm, PurityFraction, RejectedOffer, SearchBudgetSummary,
    UnresolvedCommercialField,
};
