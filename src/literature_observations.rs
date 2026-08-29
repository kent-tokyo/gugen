//! Phase 20B: bulk literature-corpus snapshot and observation provider.
//!
//! Scope, per the owner's explicit correction from the original "bulk
//! condition provider" plan: this module loads an already-prepared JSON
//! snapshot (gugen performs no network fetch, no corpus parsing of any raw
//! external format -- see below) and answers exact target+precursor-set
//! lookups against it, returning [`CorpusHeatingObservation`] values.
//! Nothing here is promoted to a [`ConditionPrecedent`](crate::ConditionPrecedent)
//! or fed to [`Planner`](crate::Planner)/`score_plan`/ranking, and this is
//! not just a convention this module happens to follow: `ConditionPrecedent::purpose`
//! is a required [`HeatingPurpose`](crate::HeatingPurpose) (not optional), while
//! every [`CorpusHeatingObservation`] this crate's own snapshot-loading path
//! (`Deserialize`, see below) can produce has `heating_purpose: None` -- there
//! is no lossless conversion from one to the other for any value reachable
//! that way. (`heating_purpose` is a `pub` field with no smart constructor,
//! so this holds for values loaded from a snapshot, not for a struct literal
//! an external caller chooses to hand-construct instead.) Promotion to
//! `ConditionPrecedent` is deliberately deferred to a future phase gated on
//! a manual purpose-matching accuracy audit, which does not exist yet.
//!
//! **Two-stage architecture.** The raw corpus this module was built against
//! (Kononova et al. 2019, *Scientific Data* 6:203, DOI
//! `10.1038/s41597-019-0224-1`, hosted at figshare DOI
//! `10.6084/m9.figshare.9722159`, CC BY 4.0) has a schema full of
//! corpus-specific quirks: string-typed element amounts, a free-text
//! ~10-word atmosphere vocabulary, and `HeatingOperation` entries that
//! sometimes carry more than one candidate temperature reading per
//! operation (verified live against the full 19,488-record corpus:
//! 2,377 heating operations have 2+ temperature entries, and 2,311 of
//! those disagree rather than repeating the same value). None of that
//! belongs in gugen's public API. A separate offline tool
//! (`benchmarks/build_literature_observation_snapshot.py`, not part of
//! this crate) does the corpus-specific extraction and writes a snapshot
//! in gugen's own schema (see [`CorpusManifest`]/[`CorpusHeatingObservation`]
//! below); this module only ever parses *that* schema, and re-validates
//! every value through gugen's own existing constructors
//! ([`TemperatureRange::new`](crate::TemperatureRange::new),
//! [`Composition::new`](crate::Composition::new), etc.) rather than
//! trusting the snapshot file blindly.
//!
//! **What is deliberately not attempted here**: inferring
//! [`HeatingPurpose`](crate::HeatingPurpose) from context (the corpus
//! carries no purpose label at all); resolving a `HeatingOperation`'s
//! multiple disagreeing temperature-reading entries to a single value
//! (left `None` rather than guessed -- see
//! [`CorpusHeatingObservation::temperature`]); mapping every raw
//! atmosphere string to a structured [`Atmosphere`](crate::Atmosphere)
//! variant (only the six unambiguous ones are; everything else becomes
//! `Atmosphere::Controlled { description }`, preserving the original text
//! rather than asserting an interpretation); cross-record conflict
//! resolution across multiple observations of the same target+precursor
//! set (deferred to a future Phase 20C, corpus-scale analogue of Phase
//! 19's `apply_condition_precedents`); and any route family other than
//! `ConventionalSolidState` (the source corpus has zero evidence for
//! `Mechanochemical` -- see [`LiteratureObservationCorpus::find_exact`]).

use crate::composition::Composition;
use crate::error::ProviderError;
use crate::process::{Atmosphere, DurationRange, HeatingPurpose, RouteFamily, TemperatureRange};
use std::collections::BTreeSet;

/// gugen's own snapshot schema identifier, unrelated to the source
/// corpus's own versioning. [`LiteratureObservationCorpus::load`] rejects
/// a snapshot whose `manifest.schema_version` does not match this exactly
/// -- there is no cross-version compatibility logic, deliberately: a
/// schema change is a breaking change to what this module can parse, not
/// something to silently paper over.
pub const CORPUS_SNAPSHOT_SCHEMA_VERSION: &str = "gugen-literature-observation-snapshot-v1";

/// Identifies the snapshot file itself -- source corpus, the offline
/// build's own release/checksum, and gugen's schema version. `checksum`
/// and `release` are informational provenance only: gugen never has
/// access to the original upstream corpus file to verify `checksum`
/// against, so it is recorded, not independently re-verified.
/// `record_count` *is* actively checked at load time (must equal the
/// actual number of observation entries in the file), since that catches
/// a truncated or corrupted snapshot cheaply without needing the upstream
/// source at all.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CorpusManifest {
    pub source: String,
    pub release: String,
    pub schema_version: String,
    pub checksum: String,
    pub record_count: usize,
}

/// One reported heating operation from the literature corpus: a target,
/// the precursor set used to reach it, and whatever this operation's
/// temperature/duration/atmosphere conditions resolved to -- `None` for
/// any field the source paragraph didn't report, or reported ambiguously
/// (see the module doc comment). Never carries a [`HeatingPurpose`] --
/// see this module's doc comment for why that is structural, not
/// conventional.
///
/// `precursors` is a [`BTreeSet`] rather than a `Vec`: precursor-set
/// identity is order-invariant by construction (mirrors
/// `InMemoryLiteratureConditionProvider`'s existing
/// `BTreeSet<PrecursorId>` pattern in `literature_conditions.rs`), so two
/// callers naming the same precursors in different orders match the same
/// observations.
///
/// `operation_index` is this operation's position among *all*
/// `HeatingOperation` entries reported for the same underlying corpus
/// record (0-based) -- preserved so a caller can reconstruct that a
/// record reported, say, a calcination step followed by a separate
/// sintering step, without gugen ever asserting which is which (no
/// `HeatingPurpose` is attached). `corpus_record_index` identifies which
/// raw corpus record this operation came from (the record's position in
/// the source corpus array) -- the corpus has no other stable per-record
/// identifier; `doi` alone is not unique (a single DOI can cover many
/// records, and is not litigable as an operation-level ID either, only a
/// record group). Neither index is a citation key by itself; `doi` is.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CorpusHeatingObservation {
    pub target: Composition,
    pub precursors: BTreeSet<Composition>,
    pub route_family: RouteFamily,
    #[serde(skip)]
    pub heating_purpose: Option<HeatingPurpose>,
    pub operation_index: usize,
    pub temperature: Option<TemperatureRange>,
    pub duration: Option<DurationRange>,
    pub atmosphere: Option<Atmosphere>,
    pub doi: Option<String>,
    pub corpus_record_index: usize,
}

impl<'de> serde::Deserialize<'de> for CorpusHeatingObservation {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // `heating_purpose` is deliberately absent from this shape (not
        // merely `Option` and left `null`): a snapshot that tries to set
        // one is a schema violation, not a value this module silently
        // drops. Combined with `deny_unknown_fields`, a `"heating_purpose"`
        // key anywhere in an observation object is rejected outright,
        // matching this crate's established "reject, don't silently
        // coerce" convention for exactly this class of invariant (Phase
        // 19P.1).
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            target: Composition,
            precursors: BTreeSet<Composition>,
            route_family: RouteFamily,
            operation_index: usize,
            temperature: Option<TemperatureRange>,
            duration: Option<DurationRange>,
            atmosphere: Option<Atmosphere>,
            doi: Option<String>,
            corpus_record_index: usize,
        }

        let raw = Raw::deserialize(deserializer)?;
        if raw.route_family != RouteFamily::ConventionalSolidState {
            return Err(serde::de::Error::custom(format!(
                "literature observation snapshots (schema {CORPUS_SNAPSHOT_SCHEMA_VERSION}) may \
                 only contain ConventionalSolidState observations, got {:?}",
                raw.route_family
            )));
        }
        Ok(CorpusHeatingObservation {
            target: raw.target,
            precursors: raw.precursors,
            route_family: raw.route_family,
            heating_purpose: None,
            operation_index: raw.operation_index,
            temperature: raw.temperature,
            duration: raw.duration,
            atmosphere: raw.atmosphere,
            doi: raw.doi,
            corpus_record_index: raw.corpus_record_index,
        })
    }
}

/// Whether [`LiteratureObservationCorpus::load`] fails the entire load on
/// the first malformed observation (`Strict`), or skips and counts
/// malformed observations while still returning every observation that
/// did parse (`Lenient`). Manifest-level problems (schema version,
/// record-count mismatch) are hard failures in *either* mode -- they mean
/// "this is not a snapshot this gugen version can read at all," not "this
/// snapshot has some noisy rows."
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadMode {
    Strict,
    Lenient,
}

/// One observation entry that failed to parse, `Lenient`-mode only.
/// `position` is this entry's index within the snapshot's `observations`
/// JSON array (0-based) -- the entry may have failed before enough of it
/// parsed to recover a `corpus_record_index`, so this is the only
/// coordinate guaranteed available for every rejection.
#[derive(Debug, Clone, PartialEq)]
pub struct RejectedObservation {
    pub position: usize,
    pub reason: String,
}

/// What happened during a [`LiteratureObservationCorpus::load`] call. A
/// complete accounting of the input: `accepted + rejected.len()` always
/// equals the snapshot's `observations` array length -- every input
/// record either parsed successfully (contributing to `accepted`) or
/// didn't (contributing to `rejected`), mutually exclusively.
/// `duplicates_collapsed` is *not* a third disjoint count on top of
/// those two -- it's a subset breakdown *within* `accepted`, counting
/// how many of the successfully-parsed entries were then found to be
/// duplicates of another and collapsed. `accepted` itself is counted
/// *before* deduplication (how many entries parsed successfully at
/// all) -- it is deliberately not the same number as the final loaded
/// corpus's own `len()`, which is `accepted - duplicates_collapsed`; a
/// reader wanting "how many end up queryable" wants the corpus's
/// `len()`, not this field.
#[derive(Debug, Clone, PartialEq)]
pub struct LoadReport {
    pub accepted: usize,
    pub duplicates_collapsed: usize,
    pub rejected: Vec<RejectedObservation>,
}

/// A loaded, deduplicated, deterministically-ordered snapshot of the
/// corpus, queryable by exact target and exact precursor set. Does not
/// implement [`ProcessEvidenceProvider`](crate::ProcessEvidenceProvider)
/// or any other Planner-facing trait -- see the module doc comment for
/// why that connection is structurally unavailable, not merely unwired.
#[derive(Debug, Clone, PartialEq)]
pub struct LiteratureObservationCorpus {
    manifest: CorpusManifest,
    observations: Vec<CorpusHeatingObservation>,
}

impl LiteratureObservationCorpus {
    /// Parses `json` (a full snapshot file's contents -- gugen performs no
    /// file or network I/O anywhere in this module, matching
    /// `materials_project_adapter.rs`'s existing "caller already has the
    /// data" contract; the caller reads the file), validates the manifest,
    /// parses every observation, deduplicates, and returns the result
    /// alongside a [`LoadReport`].
    ///
    /// **Manifest validation (always enforced, regardless of `mode`)**:
    /// `manifest.schema_version` must equal
    /// [`CORPUS_SNAPSHOT_SCHEMA_VERSION`] exactly, and
    /// `manifest.record_count` must equal the actual number of entries in
    /// the snapshot's `observations` array. Either mismatch is an
    /// unconditional [`ProviderError::MalformedRecord`], never downgraded
    /// by `Lenient` mode -- a schema/record-count mismatch means this
    /// file is not trustworthy as a whole, not that one row is noisy.
    ///
    /// **Per-observation validation**: every observation is parsed through
    /// [`CorpusHeatingObservation`]'s own `Deserialize` impl, which itself
    /// routes every numeric sub-value through gugen's already-validated
    /// constructors (e.g. [`TemperatureRange::new`](crate::TemperatureRange::new)
    /// rejects non-finite or inverted ranges). In `Strict` mode, the first
    /// such failure fails the whole load. In `Lenient` mode, a failing
    /// observation is skipped and recorded in the returned
    /// [`LoadReport::rejected`]; every observation that did parse is kept.
    ///
    /// **Deduplication**: two observations are exact duplicates if they
    /// agree on every field *except* `corpus_record_index` (target,
    /// precursors, route_family, operation_index, temperature, duration,
    /// atmosphere, doi) -- i.e. the same reported operation appears more
    /// than once in the source corpus itself, a data artifact (Phase 20A's
    /// audit found 19 such groups in the raw 19,488-record corpus), not
    /// independent evidence. Two observations that merely *report the same
    /// conditions* but differ in `doi` are never collapsed -- they are
    /// independent literature reports and both are kept (a corpus-scale
    /// conflict-resolution pass across genuinely independent reports is
    /// Phase 20C, not this module). Deduplication is order-independent:
    /// the result does not depend on the order observations appear in the
    /// snapshot file, and among a group of true duplicates the one with
    /// the lowest `corpus_record_index` is kept, deterministically
    /// regardless of input order.
    ///
    /// The returned corpus's observation order is deterministic (sorted by
    /// content, not insertion order) -- re-serializing it is byte-for-byte
    /// reproducible regardless of the snapshot file's own row order.
    pub fn load(
        json: &str,
        mode: LoadMode,
    ) -> std::result::Result<(Self, LoadReport), ProviderError> {
        #[derive(serde::Deserialize)]
        struct SnapshotFile {
            manifest: CorpusManifest,
            observations: Vec<serde_json::Value>,
        }

        let snapshot: SnapshotFile = serde_json::from_str(json)
            .map_err(|e| ProviderError::MalformedRecord(format!("snapshot file: {e}")))?;

        if snapshot.manifest.schema_version != CORPUS_SNAPSHOT_SCHEMA_VERSION {
            return Err(ProviderError::MalformedRecord(format!(
                "manifest schema_version {:?} is not supported by this gugen version \
                 (expects {CORPUS_SNAPSHOT_SCHEMA_VERSION:?})",
                snapshot.manifest.schema_version
            )));
        }
        if snapshot.manifest.record_count != snapshot.observations.len() {
            return Err(ProviderError::MalformedRecord(format!(
                "manifest declares record_count={} but the snapshot contains {} observation \
                 entries",
                snapshot.manifest.record_count,
                snapshot.observations.len()
            )));
        }

        let mut accepted: Vec<CorpusHeatingObservation> = Vec::new();
        let mut rejected: Vec<RejectedObservation> = Vec::new();
        for (position, value) in snapshot.observations.into_iter().enumerate() {
            match serde_json::from_value::<CorpusHeatingObservation>(value) {
                Ok(obs) => accepted.push(obs),
                Err(e) => {
                    if mode == LoadMode::Strict {
                        return Err(ProviderError::MalformedRecord(format!(
                            "observation at position {position}: {e}"
                        )));
                    }
                    rejected.push(RejectedObservation {
                        position,
                        reason: e.to_string(),
                    });
                }
            }
        }

        let accepted_count = accepted.len();
        accepted.sort_by(|a, b| Self::dedup_sort_key(a).cmp(&Self::dedup_sort_key(b)));
        accepted.dedup_by(Self::same_content_ignoring_provenance);
        let duplicates_collapsed = accepted_count - accepted.len();

        let report = LoadReport {
            accepted: accepted_count,
            duplicates_collapsed,
            rejected,
        };
        Ok((
            Self {
                manifest: snapshot.manifest,
                observations: accepted,
            },
            report,
        ))
    }

    /// Every field except `corpus_record_index`, formatted so the whole
    /// tuple is `Ord` despite `temperature`/`duration`/`atmosphere`
    /// containing `f64`/no-`Ord` types -- `Debug`-formatting those three
    /// fields is only ever used to group identical values adjacently for
    /// [`Self::same_content_ignoring_provenance`]'s dedup pass, never
    /// exposed as a real ordering. `corpus_record_index` is included last
    /// so that, among a group of otherwise-identical entries, sorting
    /// alone (not input order) determines which one ends up first --
    /// `dedup_by` then deterministically keeps that one.
    fn dedup_sort_key(
        obs: &CorpusHeatingObservation,
    ) -> (
        &Composition,
        &BTreeSet<Composition>,
        &Option<String>,
        usize,
        String,
        String,
        String,
        usize,
    ) {
        (
            &obs.target,
            &obs.precursors,
            &obs.doi,
            obs.operation_index,
            format!("{:?}", obs.temperature),
            format!("{:?}", obs.duration),
            format!("{:?}", obs.atmosphere),
            obs.corpus_record_index,
        )
    }

    fn same_content_ignoring_provenance(
        a: &mut CorpusHeatingObservation,
        b: &mut CorpusHeatingObservation,
    ) -> bool {
        a.target == b.target
            && a.precursors == b.precursors
            && a.route_family == b.route_family
            && a.operation_index == b.operation_index
            && a.temperature == b.temperature
            && a.duration == b.duration
            && a.atmosphere == b.atmosphere
            && a.doi == b.doi
    }

    pub fn manifest(&self) -> &CorpusManifest {
        &self.manifest
    }

    /// Every loaded observation, in the corpus's own deterministic order
    /// (see [`Self::load`]) -- for a caller that wants to iterate or
    /// aggregate over the whole corpus rather than run an exact-match
    /// query (e.g. building statistics, or a future phase's corpus-scale
    /// conflict analysis).
    pub fn observations(&self) -> &[CorpusHeatingObservation] {
        &self.observations
    }

    pub fn len(&self) -> usize {
        self.observations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.observations.is_empty()
    }

    /// Every observation whose `target` exactly equals `target` and whose
    /// `precursors` exactly equal the set of `precursors` (order-invariant
    /// -- see [`CorpusHeatingObservation`]'s doc comment). Matching is
    /// exact-composition equality (the same convention
    /// `InMemoryLiteratureConditionProvider::precedents` already uses in
    /// `literature_conditions.rs`), not a normalized-ratio match.
    ///
    /// `route_family` must be
    /// [`RouteFamily::ConventionalSolidState`](crate::RouteFamily::ConventionalSolidState)
    /// or this returns `&[]` immediately, without scanning -- the source
    /// corpus has zero evidence for any other route family (Phase 20A's
    /// audit), so a `Mechanochemical` query is explicitly, unambiguously
    /// inapplicable rather than silently returning nothing for an
    /// unrelated reason (e.g. a genuine no-match).
    ///
    /// Returned observations are in the corpus's own deterministic order
    /// (see [`Self::load`]); a target+precursor pair with many independent
    /// reports (common -- Phase 20A found 1,056 of 5,631 unique routes
    /// have 2+ independent DOIs) returns all of them, never one arbitrarily
    /// chosen or averaged.
    pub fn find_exact(
        &self,
        route_family: RouteFamily,
        target: &Composition,
        precursors: &[Composition],
    ) -> Vec<&CorpusHeatingObservation> {
        if route_family != RouteFamily::ConventionalSolidState {
            return Vec::new();
        }
        let queried: BTreeSet<Composition> = precursors.iter().cloned().collect();
        self.observations
            .iter()
            .filter(|obs| &obs.target == target && obs.precursors == queried)
            .collect()
    }
}
