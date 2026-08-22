# Commercial Precursor Catalog (Phase 22 design)

Connects gugen's chemical planning output to real purchasable products,
without ever feeding commercial data back into scientific planning. Behind
the optional `commercial_catalog` feature.

## Scope

**In scope**: CSV/JSON commercial-offer catalog import with a structured
accepted/rejected load report; a from-scratch chemical formula parser
(nothing in gugen parses formula strings otherwise); exact `Composition`
matching between a `SynthesisPlan`'s precursors and catalog offers; hard
commercial constraints (purity, manufacturer, lead time, availability, tags,
currency, price/package-size requiredness); stoichiometric quantity
calculation and purity-adjusted purchase mass; package-count rounding;
currency-safe, overflow-checked cost totals; a bounded search over complete
purchasing combinations.

**Out of scope, deliberately, this phase**: manufacturer web crawling, any
e-commerce/inventory API integration, real-time stock checks, currency
conversion, patent search, customer/market analysis, semantic search over
use-case text, formulation property prediction, machine learning, weight
calibration from experimental results, order placement, regulatory
determination, SDS generation, organic-molecule similarity search, a GUI, or
a separate crate. Alias/substitute-precursor inference (treating two
different-but-related compounds as interchangeable) is also out of scope —
see "Exact match policy" below.

No real catalog data ships with gugen. Every fixture under
`tests/fixtures/commercial_catalog_*` is fictional (`"Example Materials
Ltd."`, `"TestChem-001"`, `"Demo Oxide Grade A"`); CAS numbers in those
fixtures are real, public chemical facts (not proprietary vendor data), used
only so the checksum-verification logic has something real to check against.

## Two strictly separate stages

1. **Chemical planning** (existing `Planner`/`score_plan`) produces a
   `SynthesisPlan` — composition, balanced reaction, process steps, score,
   confidence — entirely unaware commercial data exists.
2. **Commercial offer resolution** (`assess_commercial_precursors`/
   `assess_commercial_plans`) takes that `SynthesisPlan` read-only and
   matches it against a caller-supplied `CommercialPrecursorCatalog`.

Nothing in stage 2 can reach `SynthesisPlan.score`, `.confidence`,
`.balanced_reaction`, or `.steps` — the assessment functions take `&SynthesisPlan`
and return a wholly separate `CommercialPlanAssessment`. This isn't a
convention the code happens to follow; the return type is structurally
different from anything `score_plan` ever consumes, the same boundary
`literature_evidence.rs` already uses for its own reference-only data.
`tests/commercial_catalog_planner_invariance.rs` proves this two ways: a
`Planner::plan` call before and after an assessment produces byte-identical
output, and the same `SynthesisPlan` assessed against two different catalogs
stays byte-identical itself.

## Chemical identity vs. commercial offer

The same chemical substance can have many manufacturers and products. Phase
22 keeps these as separate concepts: a `Composition` is chemical identity; a
`CommercialPrecursorOffer` is one manufacturer's specific product for sale.
Multiple offers may share one `Composition`.

## Exact match policy

Matching is literal `Composition::eq` — the same exact-rational equality
`Composition` already uses everywhere else in gugen, with **no ratio
normalization added**. Two formulas at different formula-unit scale
(`Fe2O3` vs. `Fe4O6`, chemically identical, written differently) do **not**
match. This is deliberate, not an oversight: `Composition` is designed
never to reduce to a minimal integer ratio (a load-bearing choice for
doped/solid-solution formulas elsewhere in the crate), and adding
normalization here risks conflating exactly the things this phase must
keep distinct — anhydrous vs. hydrate, different oxidation states,
carbonate vs. oxide, mixture vs. single compound, doped material vs. parent
phase, polymorphs, solid solutions, and products that merely have similar
names. Alias or substitute-precursor inference is explicitly deferred to a
later phase.

Hydrate vs. anhydrous falls out of this for free, with no special-case
logic: the formula parser folds a hydrate's water into the same flat
element-amount map as the rest of the formula (`CaSO4` → `{Ca:1, S:1, O:4}`,
`CaSO4·2H2O` → `{Ca:1, S:1, O:6, H:4}`), so they're simply different
`Composition`s under ordinary equality.

CAS numbers are **not** a basis for chemical-identity matching in this
phase — they're recorded (with checksum-verification status: verified /
present-but-unverified / absent) purely as offer metadata.

## Formula grammar

```
formula   := unit+
unit      := element number? | '(' formula ')' number?
number    := digit+ ('.' digit+)?
hydrate   := formula '·' number? formula        -- U+00B7 (middle dot) ONLY
```

The hydrate separator is deliberately *only* the middle dot (`·`, U+00B7),
not the ASCII period. A period is already the decimal point inside
`number`, and a formula mixing decimal subscripts with an ASCII-dot hydrate
separator is genuinely ambiguous character-by-character (e.g. `SO4.2H2O`
parses equally validly as "O subscript 4.2" or "O subscript 4, then a
hydrate multiplier of 2" — no amount of lookahead resolves that without
guessing). The middle dot never collides with decimal notation, so it has
no such ambiguity. `CaSO4.2H2O` is a syntactically valid formula under this
grammar — it parses as `O` subscript `5.2`, **not** as a hydrate — which is
almost certainly not what a CSV author intended, so catalog data must use
`·`, not `.`, for hydrates.

Anything outside this grammar — a variable hydrate (`CuSO4·xH2O`), `*` as a
separator, unbalanced parentheses, an unrecognized element symbol — is a
hard parse error, surfaced as a rejected catalog row, never guessed at.

## CSV schema

Required columns: `offer_id`, `manufacturer`, `product_name`, `formula`,
`source`.

`source` names the *ingestion mechanism* (`user_supplied_csv`,
`vendor_export`, `distributor_export`, `manually_transcribed`,
`synthetic_fixture`), not a free-text description — it becomes
`OfferProvenance.source_type`. `source_identifier` defaults to the row's own
`offer_id`, since a single loaded file has no other natural per-row source
handle.

Optional columns: `catalog_number`, `cas_number`, `grade`, `purity_fraction`,
`package_mass_g`, `price_minor_units`, `currency`, `availability`,
`lead_time_days`, `physical_form`, `particle_size_min_um`,
`particle_size_max_um`, `country_region`, `product_url`, `retrieved_at`,
`tags` (`;`-separated within the cell), `notes`.

`price_minor_units` and `currency` must be present together or both
absent — a row with exactly one is rejected, never treated as "price
unknown." `particle_size_min_um`/`particle_size_max_um` follow the same
both-or-neither rule.

Loading is `Strict` (the first malformed row fails the whole load) or
`Lenient` (malformed rows are collected into a `CommercialCatalogLoadReport`
— row number, offer_id, field, reason, original value — and everything else
still loads), mirroring `LiteratureObservationCorpus::load`'s existing
Strict/Lenient split. A duplicate `offer_id` within one load is always a
soft rejection in either mode.

## JSON schema

```json
{
  "offers": [
    {
      "offer_id": "EML-BACO3-99",
      "manufacturer": "Example Materials Ltd.",
      "product_name": "Demo Oxide Grade A Barium Carbonate",
      "formula": "BaCO3",
      "source_type": "synthetic_fixture",
      "purity": 0.99,
      "package_mass_g": 500.0,
      "price_minor_units": 4550,
      "currency": "USD",
      "tags": ["oxide", "ceramic-precursor"]
    }
  ]
}
```

JSON offers a fuller structured representation than CSV's flat columns —
`tags` is a native array, and `source_type`/`source_identifier` are
explicit rather than defaulted. Requires the `serde` feature in addition to
`commercial_catalog`.

## Purity, money, and package mass are their own validated types

`PurityFraction` (`0 < x <= 1`) is a distinct type from `Score01` — `Score01`
permits `0.0` (a meaningful "no support" score elsewhere in gugen); a `0.0`
purity is meaningless. `Money` holds price as an integer minor-unit count
plus a `CurrencyCode`, never `f64` — floating point has no checked
arithmetic, and the spec's own requirement ("overflow becomes an exclusion,
never 0") is only implementable with integers. `PackageMass` is canonically
grams, with `from_milligrams`/`from_kilograms` convenience constructors;
volume packaging is out of scope. `CurrencyCode` is format-validated (3
uppercase ASCII letters) rather than checked against a full ISO 4217
table — the only thing this module's arithmetic needs is "is this the same
currency as that one," which format validation plus never summing across
currencies already guarantees.

## Quantity calculation

For each precursor row (`plan.balanced_reaction.reactants[i]`, index-aligned
with `plan.precursors[i]`):

```
theoretical_pure_mass_required_grams
    = coefficient × molar_mass(composition) × scale

purity_adjusted_purchase_mass_grams   (only if the offer's purity is known)
    = theoretical_pure_mass_required_grams / purity

package_count   (only if purity_adjusted mass and package_mass are both known)
    = ceil(purity_adjusted_purchase_mass_grams / package_mass_grams)

purchased_mass_grams = package_count × package_mass_grams
excess_mass_grams    = purchased_mass_grams − purity_adjusted_purchase_mass_grams
subtotal             = unit_price.checked_mul_quantity(package_count)
```

`scale` is `1.0` unless the caller supplied both
`CommercialPlanningRequest.target_batch_mass_grams` and
`target_composition`, and `target_composition` is actually found among
*this specific plan's* `balanced_reaction.products` (products also include
curated byproducts, so this lookup matters). `theoretical_pure_mass_required_grams`
is always the **stoichiometric theoretical requirement** — never a yield
claim, never adjusted for process loss or weighing margin. Molar mass is
computed from gugen's own IUPAC 2021 atomic-weight table (already used
internally for the Bartel SISSO reduced-mass descriptor).

**Purchase mass was adjusted using the catalog purity value. This does not
establish that the unspecified impurities are inert or acceptable for the
synthesis.** This caveat is attached as an explicit assumption on every
selection where a purity adjustment was actually applied.

`Money::checked_mul_quantity`/`checked_add` are used throughout — an
overflow excludes the offer (`CommercialExclusionCode::CostOverflow`),
never treated as a cost of 0, never a panic. A combination's total cost is
`None` — not 0 — whenever any selection's price or package size is unknown,
or the selections span more than one currency; currencies are never
converted or summed across each other.

## Missing data

`purity`, `package_mass`, and `unit_price` are all optional on an offer.
`CommercialPlanningRequest.require_known_price`/`require_known_package_size`
control whether a missing value excludes the offer outright (default:
`false` — the offer stays selectable, and the missing field is recorded in
`CommercialPlanAssessment.unresolved_commercial_fields`, deduplicated across
the offers actually selected in the returned `combinations` — not every
surviving catalog candidate, so this list is bounded by
`max_results_returned`, not by catalog size).
`MissingCommercialDataPolicy` (`Reject` default, or `KeepWithWarning`)
separately governs what happens when a *hard constraint* the request set
(minimum purity, maximum lead time, allowed availability/physical
form/currency) can't be checked because the offer doesn't report that
field at all.

Unreported availability follows the same principle: gugen's precursor
matching already treats missing availability metadata as a gap, not
evidence of unavailability, and `CommercialCombination.all_availability_acceptable`
follows suit — it is `true` unless a selected offer is explicitly
`Discontinued`; an offer that simply never reported an availability status
counts as acceptable-but-unknown.

## Procurement ordering

Survivors of hard-constraint filtering are ranked, and complete purchasing
combinations (one offer per precursor row) are searched up to
`CommercialPlanningConfig.max_combinations_evaluated`, by:

1. fewer unresolved commercial fields
2. lower total cost, but only among combinations whose cost is fully known
   and shares one currency — see "Search algorithm" for why this can't be a
   naive "compare when possible, else tied" rule
3. shorter maximum lead time
4. higher (worst-case, i.e. minimum) purity across the combination
5. manufacturer, then catalog number, then offer id (deterministic
   tiebreaks; offer id is always unique, so ties always fully resolve)

This is called **procurement ordering** in gugen's own naming, deliberately
not "ranking" or "score" — it is never combined with, and never influences,
`SynthesisPlan.score`/`RankingWeights`/scientific ranking.

## Search algorithm

Whenever the *entire* combination space (product of each row's candidate
count) fits within `max_combinations_evaluated`, gugen enumerates it
exactly and ranks by the real comparator — no approximation, and this is
the common case for realistic per-precursor offer counts. Only when the
space is too large does gugen fall back to a bounded, lazy "k smallest
combinations from k sorted lists" frontier search (evaluating at most
`max_combinations_evaluated` combinations, never materializing the full
product) — this fallback is an honest best-effort heuristic, not a
guaranteed global optimum, specifically for the total-cost criterion:
"same currency as every other selected offer" is a joint property across
rows, not a per-row-local one, so no fixed per-row pre-sort can make the
lazy search's usual optimality guarantee hold for that one criterion (this
was a real bug caught by this module's own test suite during
implementation, not a hypothetical). `SearchBudgetSummary.is_exhaustive` is
`false` whenever either the evaluation budget was hit or any row was
truncated by `max_offers_per_precursor` — either one means the returned
combinations are not a complete accounting.

`max_total_cost` (if set) is applied as a hard filter *inside* the search,
before results are truncated to `max_results_returned` — never after.
Filtering after truncation would be wrong: it could return zero
combinations even when a lower-ranked, budget-satisfying one exists further
down the full ranking. A combination whose cost can't be compared to the
ceiling (different or unknown currency) is kept with a warning rather than
silently passed or failed. If every matched combination exceeds the
ceiling, the assessment reports an empty `combinations` list together with
a warning explaining why — never a silent empty result indistinguishable
from "nothing matched".

## Provenance and disclaimers

Every offer carries an `OfferProvenance` (source type, source identifier,
retrieved-at, supplied-by, license/terms, checksum — all but source type
optional). A caller can resolve any `CommercialOfferSelection.offer_id` back
to its full provenance via the `CommercialPrecursorCatalog` they already
hold — the assessment itself doesn't duplicate provenance data.

**gugen does not certify commercial data.** Catalog values are supplied
data; prices are estimates; availability may be stale; product suitability
for a given synthesis is not certified; vendor documentation and SDS sheets
must be checked separately. No network access happens anywhere in this
module — product URLs are stored strings, never fetched.

This module produces **catalog-matched commercial offers** and
**procurement-oriented estimates** of **theoretical required quantity** —
never "actual purchaseable," "best product," "recommended vendor," or
"guaranteed available."

## Complete example

See `examples/commercial_catalog_assessment.rs`
(`cargo run --example commercial_catalog_assessment --features commercial_catalog`)
for a full worked plan → catalog → assessment pipeline against the
synthetic fixture at `tests/fixtures/commercial_catalog_sample.csv`.
