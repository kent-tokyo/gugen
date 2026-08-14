/// Everything needed to answer "why did this report look the way it did"
/// without re-deriving it from logs (AGENTS.md §7).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PlanningProvenance {
    pub gugen_version: String,
    pub build_identifier: Option<String>,
    pub schema_version: u32,
    pub chematic_crystal_version: Option<String>,
    pub mikiwame_version: Option<String>,
    pub precursor_catalog_version: Option<String>,
    pub thermodynamic_provider_version: Option<String>,
    pub process_template_version: Option<String>,
    pub ranking_config_digest: Option<String>,
    /// Caller-supplied. Core never reads the system clock — determinism is
    /// a hard requirement (AGENTS.md §25) and wall-clock reads inside the
    /// library would silently break it.
    pub execution_timestamp: String,
    pub deterministic_seed: u64,
    pub enabled_features: Vec<String>,
}

impl PlanningProvenance {
    /// The crate's own version, read from `Cargo.toml` at compile time.
    pub fn gugen_version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}
