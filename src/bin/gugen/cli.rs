use clap::{Parser, Subcommand, ValueEnum};
#[cfg(feature = "commercial_catalog")]
use gugen::{CommercialCatalogLoadMode, CommercialRankingPolicy};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "gugen",
    version,
    about = "Explainable materials synthesis and process planning"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Subcommand)]
#[allow(clippy::large_enum_variant)]
pub(crate) enum Command {
    /// Balance a reaction given as JSON: {"reactants": [...], "products": [...]},
    /// each a list of element-symbol -> amount maps (see AGENTS.md §10).
    Balance {
        /// Path to the reaction JSON file.
        path: PathBuf,
    },
    /// Plan candidate syntheses for a target composition (AGENTS.md §19).
    Plan {
        /// Path to a `TargetSpecification` JSON file.
        target: PathBuf,
        /// Path to a precursor catalog JSON file (a JSON array of `PrecursorCandidate`).
        #[arg(long)]
        catalog: PathBuf,
        /// Write the report here instead of stdout.
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "json")]
        format: OutputFormat,
    },
    /// Explain one plan from a previously generated report.
    Explain {
        /// Path to a `SynthesisPlanningReport` JSON file (`gugen plan`'s output).
        report: PathBuf,
        #[arg(long = "plan")]
        plan_id: String,
    },
    /// Check that a target JSON file is well-formed and not self-contradictory,
    /// without running a full search.
    ValidateTarget { path: PathBuf },
    /// Print build/configuration diagnostics (AGENTS.md §19).
    Doctor,
    /// Plan for every target in a JSON array (a JSON array of
    /// `TargetSpecification`). One target's failure does not abort the rest
    /// (AGENTS.md §26 Phase 7).
    Batch {
        input: PathBuf,
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Match a target's plans against a commercial precursor catalog
    /// (price, purity, manufacturer, lead time) -- a strictly read-only,
    /// post-planning stage that never affects score, confidence, the
    /// reaction, or process steps (docs/commercial_precursor_catalog.md).
    #[cfg(feature = "commercial_catalog")]
    CommercialPlan {
        /// Path to a `TargetSpecification` JSON file.
        target: PathBuf,
        /// Path to a precursor catalog JSON file (a JSON array of
        /// `PrecursorCandidate`) -- unchanged meaning from `plan`/`batch`.
        #[arg(long)]
        catalog: PathBuf,
        /// Path to a commercial-offers catalog; CSV or JSON, by file extension.
        #[arg(long = "commercial-catalog")]
        commercial_catalog: PathBuf,
        #[arg(
            long = "commercial-catalog-mode",
            value_enum,
            default_value = "lenient"
        )]
        commercial_catalog_mode: CommercialCatalogModeArg,
        /// Path to a JSON file mapping gugen's canonical CSV column names to
        /// this file's actual header names, e.g. `{"formula": "Chemical
        /// Formula", "manufacturer": "Supplier"}`. Only applicable when
        /// `--commercial-catalog` is a `.csv` file; only columns that
        /// differ need an entry.
        #[arg(long = "commercial-catalog-column-map")]
        commercial_catalog_column_map: Option<PathBuf>,
        /// Narrow to one plan by id (an id from a prior `gugen plan` run --
        /// plan ids are stable across runs). Default: assess every plan.
        #[arg(long = "plan-id")]
        plan_id: Option<String>,
        #[arg(long = "target-mass-g")]
        target_mass_g: Option<f64>,
        #[arg(long = "min-purity")]
        min_purity: Option<f64>,
        #[arg(long = "max-lead-time-days")]
        max_lead_time_days: Option<u32>,
        /// Integer minor units (e.g. cents); requires --currency.
        #[arg(long = "max-total-cost")]
        max_total_cost: Option<u64>,
        #[arg(long)]
        currency: Option<String>,
        #[arg(long = "allowed-manufacturer")]
        allowed_manufacturers: Vec<String>,
        #[arg(long = "excluded-manufacturer")]
        excluded_manufacturers: Vec<String>,
        #[arg(long = "ranking-policy", value_enum, default_value = "balanced")]
        ranking_policy: CommercialRankingPolicyArg,
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "json")]
        format: CommercialOutputFormat,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum OutputFormat {
    Json,
    Markdown,
}

#[cfg(feature = "commercial_catalog")]
#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum CommercialCatalogModeArg {
    Strict,
    Lenient,
}

#[cfg(feature = "commercial_catalog")]
impl From<CommercialCatalogModeArg> for CommercialCatalogLoadMode {
    fn from(mode: CommercialCatalogModeArg) -> Self {
        match mode {
            CommercialCatalogModeArg::Strict => CommercialCatalogLoadMode::Strict,
            CommercialCatalogModeArg::Lenient => CommercialCatalogLoadMode::Lenient,
        }
    }
}

#[cfg(feature = "commercial_catalog")]
#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum CommercialOutputFormat {
    Json,
    Markdown,
    Csv,
}

#[cfg(feature = "commercial_catalog")]
#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum CommercialRankingPolicyArg {
    Balanced,
    CostFirst,
    LeadTimeFirst,
    PurityFirst,
    MinimumUnresolvedData,
    Pareto,
}

#[cfg(feature = "commercial_catalog")]
impl From<CommercialRankingPolicyArg> for CommercialRankingPolicy {
    fn from(policy: CommercialRankingPolicyArg) -> Self {
        match policy {
            CommercialRankingPolicyArg::Balanced => CommercialRankingPolicy::Balanced,
            CommercialRankingPolicyArg::CostFirst => CommercialRankingPolicy::CostFirst,
            CommercialRankingPolicyArg::LeadTimeFirst => CommercialRankingPolicy::LeadTimeFirst,
            CommercialRankingPolicyArg::PurityFirst => CommercialRankingPolicy::PurityFirst,
            CommercialRankingPolicyArg::MinimumUnresolvedData => {
                CommercialRankingPolicy::MinimumUnresolvedData
            }
            CommercialRankingPolicyArg::Pareto => CommercialRankingPolicy::Pareto,
        }
    }
}
