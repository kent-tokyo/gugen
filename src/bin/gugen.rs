//! CLI for gugen (AGENTS.md §19): `plan`, `balance`, `explain`,
//! `validate-target`, `doctor`, `batch`, and (with the `commercial_catalog`
//! feature) `commercial-plan`.
//!
//! This binary is the one place in the crate allowed to read the system
//! clock (`now_rfc3339`, used for `execution_timestamp`) -- the planning
//! core never does (AGENTS.md §25).

use clap::{Parser, Subcommand, ValueEnum};
use gugen::{
    BalancedReaction, Composition, Element, InMemoryPrecursorCatalog, Planner, PlanningConfig,
    PrecursorCandidate, ProcessStep, RankingWeights, ReactionSpecies, RouteFamily, SynthesisPlan,
    SynthesisPlanningReport, TargetSpecification, TargetSummary, ranking_weights_digest,
};
#[cfg(feature = "commercial_catalog")]
use gugen::{
    CommercialCatalogLoadMode, CommercialCatalogLoadReport, CommercialPlanAssessment,
    CommercialPlanningConfig, CommercialPlanningRequest, CommercialPrecursorCatalog, CurrencyCode,
    Money, PurityFraction, assess_commercial_plans,
};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "gugen",
    version,
    about = "Explainable materials synthesis and process planning"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
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
        #[arg(long)]
        output: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "json")]
        format: CommercialOutputFormat,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    Markdown,
}

#[cfg(feature = "commercial_catalog")]
#[derive(Debug, Clone, Copy, ValueEnum)]
enum CommercialCatalogModeArg {
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
enum CommercialOutputFormat {
    Json,
    Markdown,
    Csv,
}

/// One assessed plan within a `commercial-plan` run's JSON output -- pairs
/// `CommercialPlanAssessment` (which carries `plan_id` but not the plan's
/// route family) with the context needed to identify it. Kept CLI-local,
/// same "not added to the public library schema" convention as `BatchEntry`.
#[cfg(feature = "commercial_catalog")]
#[derive(serde::Serialize)]
struct CommercialPlanResult {
    route_family: RouteFamily,
    assessment: CommercialPlanAssessment,
}

#[derive(serde::Deserialize)]
struct ReactionInput {
    reactants: Vec<Composition>,
    products: Vec<Composition>,
}

/// One target's outcome within a `batch` run -- kept CLI-local rather than
/// added to the public library schema, since AGENTS.md §6/§20 only specify
/// the single-target `SynthesisPlanningReport` shape. Exactly one of
/// `report`/`error` is ever `Some` -- no separate `ok` flag, since that
/// would just restate `error.is_none()`.
#[derive(serde::Serialize)]
struct BatchEntry {
    index: usize,
    report: Option<SynthesisPlanningReport>,
    error: Option<String>,
}

fn main() {
    if let Err(err) = run(Cli::parse()) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Balance { path } => run_balance(&path),
        Command::Plan {
            target,
            catalog,
            output,
            format,
        } => run_plan(&target, &catalog, output.as_deref(), format),
        Command::Explain { report, plan_id } => run_explain(&report, &plan_id),
        Command::ValidateTarget { path } => run_validate_target(&path),
        Command::Doctor => {
            println!("{}", doctor_report());
            Ok(())
        }
        Command::Batch {
            input,
            catalog,
            output,
        } => run_batch(&input, &catalog, output.as_deref()),
        #[cfg(feature = "commercial_catalog")]
        Command::CommercialPlan {
            target,
            catalog,
            commercial_catalog,
            commercial_catalog_mode,
            plan_id,
            target_mass_g,
            min_purity,
            max_lead_time_days,
            max_total_cost,
            currency,
            allowed_manufacturers,
            excluded_manufacturers,
            output,
            format,
        } => run_commercial_plan(
            &target,
            &catalog,
            &commercial_catalog,
            commercial_catalog_mode.into(),
            plan_id.as_deref(),
            target_mass_g,
            min_purity,
            max_lead_time_days,
            max_total_cost,
            currency.as_deref(),
            &allowed_manufacturers,
            &excluded_manufacturers,
            output.as_deref(),
            format,
        ),
    }
}

fn run_balance(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let input: ReactionInput = serde_json::from_str(&text)?;
    let results = gugen::balance(&input.reactants, &input.products)?;
    println!("{}", serde_json::to_string_pretty(&results)?);
    if results.is_empty() {
        eprintln!("no valid balance found for the given reactants/products");
    }
    Ok(())
}

fn load_catalog(path: &Path) -> Result<InMemoryPrecursorCatalog, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    let candidates: Vec<PrecursorCandidate> = serde_json::from_str(&text)?;
    Ok(InMemoryPrecursorCatalog::new(candidates))
}

fn load_target(path: &Path) -> Result<TargetSpecification, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

fn write_output(output: Option<&Path>, text: &str) -> Result<(), Box<dyn std::error::Error>> {
    match output {
        Some(path) => std::fs::write(path, text)?,
        None => println!("{text}"),
    }
    Ok(())
}

fn run_plan(
    target_path: &Path,
    catalog_path: &Path,
    output: Option<&Path>,
    format: OutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let target = load_target(target_path)?;
    let catalog = load_catalog(catalog_path)?;
    let planner = Planner::builder(catalog, PlanningConfig::default()).build();
    let report = planner.plan(&target, &now_rfc3339())?;

    let rendered = match format {
        OutputFormat::Json => serde_json::to_string_pretty(&report)?,
        OutputFormat::Markdown => render_report_markdown(&report),
    };
    write_output(output, &rendered)
}

fn run_explain(report_path: &Path, plan_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(report_path)?;
    let report: SynthesisPlanningReport = serde_json::from_str(&text)?;
    match report.plans.iter().find(|p| p.plan_id.0 == plan_id) {
        Some(plan) => {
            println!("{}", render_plan_detail(&report.target, plan));
            Ok(())
        }
        None => {
            let available: Vec<&str> = report.plans.iter().map(|p| p.plan_id.0.as_str()).collect();
            Err(format!("no plan '{plan_id}' in this report; available: {available:?}").into())
        }
    }
}

/// Target elements that `constraints.forbidden_elements` also forbids --
/// the same self-contradiction `Planner::plan` abstains on (its own check
/// is private, and duplicating three lines here is simpler than exposing
/// an internal helper just for this).
fn contradictory_elements(target: &TargetSpecification) -> Vec<Element> {
    target
        .composition
        .elements()
        .filter(|e| target.constraints.forbidden_elements.contains(e))
        .collect()
}

fn run_validate_target(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let target = load_target(path)?;
    let contradictory = contradictory_elements(&target);

    println!("target: {}", format_composition(&target.composition));
    println!(
        "structure: {}",
        if target.structure.is_some() {
            "present"
        } else {
            "none"
        }
    );
    println!(
        "desired phase: {}",
        target
            .desired_phase
            .as_ref()
            .map(|p| p.phase_name.as_str())
            .unwrap_or("none")
    );
    println!(
        "forbidden elements: {}",
        if target.constraints.forbidden_elements.is_empty() {
            "none".to_string()
        } else {
            target
                .constraints
                .forbidden_elements
                .iter()
                .map(Element::symbol)
                .collect::<Vec<_>>()
                .join(", ")
        }
    );

    if contradictory.is_empty() {
        println!("valid: target and constraints are not self-contradictory");
        Ok(())
    } else {
        let symbols: Vec<&str> = contradictory.iter().map(Element::symbol).collect();
        println!(
            "invalid: element(s) {symbols:?} are both required by the composition and \
            forbidden by constraints -- no plan could ever satisfy both"
        );
        Err("target is self-contradictory".into())
    }
}

fn doctor_report() -> String {
    let mikiwame_status = if cfg!(feature = "mikiwame") {
        "enabled (adapter compiled in; not auto-wired into Planner::plan -- see docs/integration.md)"
    } else {
        "disabled (build with --features mikiwame to enable)"
    };
    let chematic_crystal_status = if cfg!(feature = "chematic_crystal") {
        "enabled (chematic_crystal_adapter::to_mikiwame_structure compiled in; a caller-driven \
        bridge to mikiwame, not auto-wired into Planner::plan -- see docs/integration.md)"
    } else {
        "disabled (build with --features chematic_crystal to enable)"
    };
    format!(
        "gugen version: {}\n\
        schema version: {}\n\
        chematic-crystal integration status: {chematic_crystal_status}\n\
        mikiwame integration status: {mikiwame_status}\n\
        enabled route families: {:?}\n\
        precursor catalog version: not applicable outside a `plan`/`batch` run (no persistent catalog is configured by this CLI)\n\
        thermodynamic provider: none (this CLI builds every `Planner` via `offline_minimal`)\n\
        process evidence provider: none (this CLI builds every `Planner` via `offline_minimal`)\n\
        route suitability provider: none (this CLI builds every `Planner` via `offline_minimal`)\n\
        literature evidence provider: none (this CLI builds every `Planner` via `offline_minimal`; \
        see docs/literature_evidence_integration.md for the reference-only report field this \
        provider type populates when configured programmatically)\n\
        ranking config digest: {}\n\
        deterministic mode: yes -- the planning core never reads the system clock; \
        execution_timestamp is supplied by this CLI at the moment each `plan`/`batch` runs\n\
        supported domain: bulk polycrystalline inorganic solids via conventional solid-state \
        or mechanochemical (structural route only, no detailed milling conditions -- \
        AGENTS.md §3) synthesis (see docs/scientific_scope.md for the full scope)\n\
        known limitations: route-family suitability findings never affect ranking scores -- a \
        configured RouteSuitabilityProvider (Phase 15B) can only move a plan with strong, \
        uncontested contradicting evidence into a report's not_recommended list, never reorder \
        or rescore the plans that remain; this CLI never configures one, so every applicable \
        route family is still offered here regardless; no hazard/safety database \
        (manual_review_required is always true); TargetSpecification still has no field for real \
        geometry, so even with chematic_crystal enabled nothing in this CLI can drive \
        mikiwame::analyze automatically; full list in CHANGELOG.md's \"Known limitations\" \
        section",
        env!("CARGO_PKG_VERSION"),
        gugen::SCHEMA_VERSION,
        [
            RouteFamily::ConventionalSolidState,
            RouteFamily::Mechanochemical,
        ],
        ranking_weights_digest(&RankingWeights::default()),
    )
}

/// Plans every target against `planner` independently -- one target's
/// `GugenError` does not stop the rest from being attempted (AGENTS.md §26
/// Phase 7's explicit batch requirement). `InMemoryPrecursorCatalog` (the
/// only catalog this CLI builds) never actually errors, so this isolation
/// is currently only exercised by custom `PrecursorCatalog` impls (see
/// this module's tests) -- kept regardless, since any future catalog might.
fn batch_plan(
    planner: &Planner,
    targets: &[TargetSpecification],
    timestamp: &str,
) -> Vec<BatchEntry> {
    targets
        .iter()
        .enumerate()
        .map(|(index, target)| match planner.plan(target, timestamp) {
            Ok(report) => BatchEntry {
                index,
                report: Some(report),
                error: None,
            },
            Err(err) => BatchEntry {
                index,
                report: None,
                error: Some(err.to_string()),
            },
        })
        .collect()
}

fn run_batch(
    input: &Path,
    catalog_path: &Path,
    output: Option<&Path>,
) -> Result<(), Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(input)?;
    let targets: Vec<TargetSpecification> = serde_json::from_str(&text)?;
    let catalog = load_catalog(catalog_path)?;
    let planner = Planner::builder(catalog, PlanningConfig::default()).build();

    let entries = batch_plan(&planner, &targets, &now_rfc3339());
    write_output(output, &serde_json::to_string_pretty(&entries)?)
}

#[cfg(feature = "commercial_catalog")]
fn load_commercial_catalog(
    path: &Path,
    mode: CommercialCatalogLoadMode,
) -> Result<(CommercialPrecursorCatalog, CommercialCatalogLoadReport), Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(path)?;
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("csv") => Ok(CommercialPrecursorCatalog::load_csv(&text, mode)?),
        Some("json") => Ok(CommercialPrecursorCatalog::load_json(&text, mode)?),
        other => {
            Err(format!("commercial catalog path must end in .csv or .json, got {other:?}").into())
        }
    }
}

#[cfg(feature = "commercial_catalog")]
#[allow(clippy::too_many_arguments)]
fn build_commercial_planning_request(
    target_composition: Composition,
    target_mass_g: Option<f64>,
    min_purity: Option<f64>,
    max_lead_time_days: Option<u32>,
    max_total_cost_minor_units: Option<u64>,
    currency: Option<&str>,
    allowed_manufacturers: &[String],
    excluded_manufacturers: &[String],
) -> Result<CommercialPlanningRequest, Box<dyn std::error::Error>> {
    let max_total_cost = match (max_total_cost_minor_units, currency) {
        (None, None) => None,
        (Some(minor_units), Some(code)) => Some(Money::new(minor_units, CurrencyCode::new(code)?)),
        (Some(_), None) => return Err("--max-total-cost requires --currency".into()),
        (None, Some(_)) => return Err("--currency requires --max-total-cost".into()),
    };
    let min_purity = min_purity.map(PurityFraction::new).transpose()?;

    Ok(CommercialPlanningRequest {
        target_composition: target_mass_g.map(|_| target_composition),
        target_batch_mass_grams: target_mass_g,
        allowed_manufacturers: if allowed_manufacturers.is_empty() {
            None
        } else {
            Some(allowed_manufacturers.iter().cloned().collect())
        },
        excluded_manufacturers: excluded_manufacturers.iter().cloned().collect(),
        min_purity,
        max_lead_time_days,
        max_total_cost,
        ..CommercialPlanningRequest::default()
    })
}

#[cfg(feature = "commercial_catalog")]
#[allow(clippy::too_many_arguments)]
fn run_commercial_plan(
    target_path: &Path,
    catalog_path: &Path,
    commercial_catalog_path: &Path,
    commercial_catalog_mode: CommercialCatalogLoadMode,
    plan_id: Option<&str>,
    target_mass_g: Option<f64>,
    min_purity: Option<f64>,
    max_lead_time_days: Option<u32>,
    max_total_cost_minor_units: Option<u64>,
    currency: Option<&str>,
    allowed_manufacturers: &[String],
    excluded_manufacturers: &[String],
    output: Option<&Path>,
    format: CommercialOutputFormat,
) -> Result<(), Box<dyn std::error::Error>> {
    let target = load_target(target_path)?;
    let catalog = load_catalog(catalog_path)?;
    let planner = Planner::builder(catalog, PlanningConfig::default()).build();
    let report = planner.plan(&target, &now_rfc3339())?;

    let plans: Vec<SynthesisPlan> = match plan_id {
        Some(id) => {
            let plan = report
                .plans
                .iter()
                .find(|p| p.plan_id.0 == id)
                .cloned()
                .ok_or_else(|| {
                    let available: Vec<&str> =
                        report.plans.iter().map(|p| p.plan_id.0.as_str()).collect();
                    format!("no plan '{id}' in this report; available: {available:?}")
                })?;
            vec![plan]
        }
        None => report.plans.clone(),
    };

    let (commercial_catalog, load_report) =
        load_commercial_catalog(commercial_catalog_path, commercial_catalog_mode)?;
    eprintln!(
        "commercial catalog: {} accepted, {} duplicate offer id(s) collapsed, {} rejected",
        load_report.accepted,
        load_report.duplicate_offer_ids_collapsed,
        load_report.rejected.len()
    );

    let request = build_commercial_planning_request(
        target.composition.clone(),
        target_mass_g,
        min_purity,
        max_lead_time_days,
        max_total_cost_minor_units,
        currency,
        allowed_manufacturers,
        excluded_manufacturers,
    )?;

    let assessments = assess_commercial_plans(
        &plans,
        &commercial_catalog,
        &request,
        &CommercialPlanningConfig::default(),
    )?;

    let rendered = match format {
        CommercialOutputFormat::Json => {
            let results: Vec<CommercialPlanResult> = plans
                .iter()
                .zip(assessments.iter())
                .map(|(plan, assessment)| CommercialPlanResult {
                    route_family: plan.route_family,
                    assessment: assessment.clone(),
                })
                .collect();
            serde_json::to_string_pretty(&results)?
        }
        CommercialOutputFormat::Markdown => {
            render_commercial_report_markdown(&report.target, &plans, &assessments, &load_report)
        }
        CommercialOutputFormat::Csv => render_commercial_report_csv(&assessments)?,
    };
    write_output(output, &rendered)
}

// --- Rendering (JSON output goes through serde directly; the helpers below
// are only for `--format markdown` and `gugen explain`) ---

fn format_number(amount: f64) -> String {
    let rounded = amount.round();
    if (amount - rounded).abs() < 1e-9 {
        format!("{}", rounded as i64)
    } else {
        format!("{amount:.3}")
    }
}

/// `Composition` iterates by element symbol (`BTreeMap` order), which is
/// alphabetical, not conventional chemical-formula order (e.g. Ba/Ti/O, not
/// Ba/O/Ti) -- concatenating symbols+amounts directly would print something
/// that looks like a formula but isn't one (`BaO3Ti` for BaTiO3). Rendered
/// as explicit element:amount pairs instead, so it's honest about what it
/// is: gugen has no formula-notation orderer.
fn format_composition(c: &Composition) -> String {
    c.iter()
        .map(|(el, amt)| format!("{}:{}", el.symbol(), format_number(amt)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_species(species: &[ReactionSpecies]) -> String {
    species
        .iter()
        .map(|s| {
            format!(
                "{}x({})",
                s.coefficient(),
                format_composition(&s.composition)
            )
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn format_reaction(r: &BalancedReaction) -> String {
    format!(
        "{} -> {}",
        format_species(r.reactants()),
        format_species(r.products())
    )
}

/// `None` is rendered as "unresolved" rather than left blank: an unresolved
/// condition is a stated fact about this plan (AGENTS.md §4.1), not an
/// absence of output.
fn opt_debug<T: std::fmt::Debug>(value: &Option<T>) -> String {
    value
        .as_ref()
        .map_or_else(|| "unresolved".to_string(), |v| format!("{v:?}"))
}

fn format_step(step: &ProcessStep) -> String {
    use ProcessStep::*;
    match step {
        Weigh { materials } => {
            let parts: Vec<String> = materials
                .iter()
                .map(|m| format!("{} x{}", m.precursor, m.formula_units))
                .collect();
            format!("Weigh: {}", parts.join(", "))
        }
        Mix { method } => format!("Mix ({method:?})"),
        Grind { method, duration } => {
            format!("Grind ({method:?}), duration={}", opt_debug(duration))
        }
        Form { method, pressure } => {
            format!("Form ({method:?}), pressure={}", opt_debug(pressure))
        }
        Heat {
            purpose,
            temperature,
            duration,
            atmosphere,
            ramp,
        } => format!(
            "Heat ({purpose:?}): temperature={}, duration={}, atmosphere={}, ramp={}",
            opt_debug(temperature),
            opt_debug(duration),
            opt_debug(atmosphere),
            opt_debug(ramp)
        ),
        Cool { mode } => format!("Cool ({mode:?})"),
        IntermediateCharacterization { method, purpose } => {
            format!("Characterize ({method:?}): {purpose}")
        }
    }
}

fn render_plan_detail(target: &TargetSummary, plan: &SynthesisPlan) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "## Plan {} (score {:.3})\n\n",
        plan.plan_id,
        plan.score.total_ranking_score.value()
    ));
    out.push_str(&format!(
        "- Target: {}\n",
        format_composition(&target.composition)
    ));
    out.push_str(&format!("- Route family: {:?}\n", plan.route_family));
    if let Some(reaction) = &plan.balanced_reaction {
        out.push_str(&format!("- Reaction: {}\n", format_reaction(reaction)));
    }
    out.push_str(&format!(
        "- Manual review required: {}\n",
        plan.manual_review_required
    ));
    out.push_str(&format!(
        "- Applicability: {:?} -- {}\n\n",
        plan.applicability.level,
        plan.applicability.rationale.join("; ")
    ));

    out.push_str("### Steps\n\n");
    for planned in &plan.steps {
        out.push_str(&format!(
            "- [{:?}] {}\n",
            planned.requirement,
            format_step(&planned.step)
        ));
    }
    out.push('\n');

    out.push_str("### Score breakdown\n\n```\n");
    out.push_str(&format!("{:#?}\n", plan.score));
    out.push_str("```\n\n### Confidence\n\n```\n");
    out.push_str(&format!("{:#?}\n", plan.confidence));
    out.push_str("```\n\n");

    if !plan.evidence.is_empty() {
        out.push_str("### Evidence\n\n");
        for e in &plan.evidence {
            out.push_str(&format!(
                "- [{:?}/{:?}] {}\n",
                e.strength, e.kind, e.statement
            ));
        }
        out.push('\n');
    }

    if !plan.warnings.is_empty() {
        out.push_str("### Warnings\n\n");
        for w in &plan.warnings {
            out.push_str(&format!("- [{:?}] {}\n", w.severity, w.message));
        }
        out.push('\n');
    }

    if !plan.assumptions.is_empty() {
        out.push_str("### Assumptions\n\n");
        for a in &plan.assumptions {
            out.push_str(&format!("- {}\n", a.statement));
        }
        out.push('\n');
    }

    if !plan.unresolved.is_empty() {
        out.push_str("### Unresolved\n\n");
        for u in &plan.unresolved {
            out.push_str(&format!("- {}: {}\n", u.description, u.reason));
        }
        out.push('\n');
    }

    out
}

fn render_report_markdown(report: &SynthesisPlanningReport) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Synthesis Planning Report (schema v{})\n\n",
        report.schema_version
    ));
    out.push_str(&format!(
        "**Target:** {}\n\n",
        format_composition(&report.target.composition)
    ));
    out.push_str(&format!(
        "**Applicability:** {:?} -- {}\n\n",
        report.applicability.level,
        report.applicability.rationale.join("; ")
    ));

    if !report.warnings.is_empty() {
        out.push_str("**Report-level warnings:**\n\n");
        for w in &report.warnings {
            out.push_str(&format!("- [{:?}] {}\n", w.severity, w.message));
        }
        out.push('\n');
    }

    if report.plans.is_empty() {
        out.push_str("_No plans were produced for this target._\n\n");
    }
    for plan in &report.plans {
        out.push_str(&render_plan_detail(&report.target, plan));
    }

    if !report.rejected_candidates.is_empty() {
        out.push_str("## Rejected candidates\n\n");
        for r in &report.rejected_candidates {
            let ids: Vec<&str> = r.precursors.iter().map(|p| p.0.as_str()).collect();
            out.push_str(&format!(
                "- {ids:?} {:?}: {}\n",
                r.reason_codes, r.explanation
            ));
        }
        out.push('\n');
    }

    out.push_str(&format!(
        "_Generated {} by gugen {}._\n",
        report.provenance.execution_timestamp, report.provenance.gugen_version
    ));
    out
}

#[cfg(feature = "commercial_catalog")]
fn render_commercial_report_markdown(
    target: &TargetSummary,
    plans: &[SynthesisPlan],
    assessments: &[CommercialPlanAssessment],
    load_report: &CommercialCatalogLoadReport,
) -> String {
    let mut out = String::new();
    out.push_str("# Commercial Precursor Assessment\n\n");
    out.push_str(&format!(
        "**Target:** {}\n\n",
        format_composition(&target.composition)
    ));
    out.push_str(
        "_gugen does not certify commercial data: prices are estimates, availability may be \
        stale, and product suitability for a given synthesis is not certified. Verify vendor \
        documentation and SDS sheets separately._\n\n",
    );
    out.push_str(&format!(
        "**Commercial catalog load:** {} accepted, {} duplicate offer id(s) collapsed, {} \
        rejected.\n\n",
        load_report.accepted,
        load_report.duplicate_offer_ids_collapsed,
        load_report.rejected.len()
    ));

    for (plan, assessment) in plans.iter().zip(assessments.iter()) {
        out.push_str(&render_commercial_assessment_markdown(plan, assessment));
    }
    out
}

#[cfg(feature = "commercial_catalog")]
fn render_commercial_assessment_markdown(
    plan: &SynthesisPlan,
    assessment: &CommercialPlanAssessment,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "## Plan {} ({:?})\n\n",
        assessment.plan_id, plan.route_family
    ));
    out.push_str(&format!(
        "- Every precursor has a catalog match: {}\n\n",
        assessment.every_precursor_has_a_match
    ));

    if !assessment.unmatched_precursors.is_empty() {
        out.push_str("### Unmatched precursors\n\n");
        for (id, comp) in &assessment.unmatched_precursors {
            out.push_str(&format!("- {id} ({})\n", format_composition(comp)));
        }
        out.push('\n');
    }

    if assessment.combinations.is_empty() {
        out.push_str("_No procurement combinations were found._\n\n");
    } else {
        out.push_str("### Procurement combinations (ranked)\n\n");
        for (rank, combo) in assessment.combinations.iter().enumerate() {
            out.push_str(&format!("{}. `{}`\n", rank + 1, combo.combination_id));
            for sel in &combo.selections {
                out.push_str(&format!(
                    "   - {} <- offer {} ({:.3} g theoretical)\n",
                    sel.precursor, sel.offer_id, sel.theoretical_pure_mass_required_grams
                ));
            }
            match combo.total_cost {
                Some(cost) => out.push_str(&format!(
                    "   - Estimated subtotal: {} {}\n",
                    cost.minor_units(),
                    cost.currency()
                )),
                None => out.push_str(
                    "   - Total cost unknown (unresolved price/package size, or the selected \
                    offers span more than one currency -- gugen never converts currencies).\n",
                ),
            }
        }
        out.push('\n');
    }

    if !assessment.rejected_offers.is_empty() {
        out.push_str("### Rejected offers\n\n");
        for r in &assessment.rejected_offers {
            out.push_str(&format!(
                "- {} {:?}: {}\n",
                r.precursor, r.reason_codes, r.explanation
            ));
        }
        out.push('\n');
    }

    if !assessment.unresolved_commercial_fields.is_empty() {
        out.push_str("### Unresolved fields\n\n");
        for u in &assessment.unresolved_commercial_fields {
            out.push_str(&format!(
                "- {} / offer {}: {}\n",
                u.precursor, u.offer_id, u.field
            ));
        }
        out.push('\n');
    }

    if !assessment.warnings.is_empty() {
        out.push_str("### Warnings\n\n");
        for w in &assessment.warnings {
            out.push_str(&format!("- [{:?}] {}\n", w.severity, w.message));
        }
        out.push('\n');
    }

    out.push_str("### Search budget\n\n```\n");
    out.push_str(&format!("{:#?}\n", assessment.search_budget));
    out.push_str("```\n\n");

    out
}

#[cfg(feature = "commercial_catalog")]
fn render_commercial_report_csv(
    assessments: &[CommercialPlanAssessment],
) -> Result<String, Box<dyn std::error::Error>> {
    let mut writer = csv::Writer::from_writer(vec![]);
    writer.write_record([
        "plan_id",
        "every_precursor_has_a_match",
        "combination_rank",
        "combination_id",
        "total_cost_minor_units",
        "currency",
        "all_costs_known",
        "max_lead_time_days",
        "all_availability_acceptable",
        "note",
    ])?;

    for assessment in assessments {
        if assessment.combinations.is_empty() {
            writer.write_record([
                assessment.plan_id.0.as_str(),
                &assessment.every_precursor_has_a_match.to_string(),
                "",
                "",
                "",
                "",
                "",
                "",
                "",
                "no procurement combination found",
            ])?;
            continue;
        }
        for (rank, combo) in assessment.combinations.iter().enumerate() {
            let (cost, currency) = match combo.total_cost {
                Some(cost) => (cost.minor_units().to_string(), cost.currency().to_string()),
                None => (String::new(), String::new()),
            };
            // `all_costs_known` is per-selection; a combination can still
            // have no `total_cost` if the selected offers span more than
            // one currency (gugen never converts currencies) -- state that
            // here so a blank cost next to `all_costs_known=true` doesn't
            // read as a bug in this writer.
            let note = if combo.total_cost.is_none() && combo.all_costs_known {
                "total unavailable: selected offers span more than one currency"
            } else {
                ""
            };
            writer.write_record([
                assessment.plan_id.0.as_str(),
                &assessment.every_precursor_has_a_match.to_string(),
                &(rank + 1).to_string(),
                &combo.combination_id,
                &cost,
                &currency,
                &combo.all_costs_known.to_string(),
                &combo
                    .max_lead_time_days
                    .map_or(String::new(), |d| d.to_string()),
                &combo.all_availability_acceptable.to_string(),
                note,
            ])?;
        }
    }

    Ok(String::from_utf8(writer.into_inner()?)?)
}

/// UTC RFC3339, second precision. The only wall-clock read in this crate
/// (the planning core is forbidden from doing this itself -- AGENTS.md §25).
fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (year, month, day) = civil_from_days((secs / 86_400) as i64);
    let time_of_day = secs % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60
    )
}

/// Howard Hinnant's days-from-civil algorithm, run in reverse (days since
/// 1970-01-01 -> proleptic Gregorian year/month/day). A well-known
/// public-domain algorithm (http://howardhinnant.github.io/date_algorithms.html),
/// not a gugen-specific date heuristic.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gugen::{PlanningConstraints, PrecursorId, ProviderError};

    fn element(symbol: &str) -> Element {
        Element::new(symbol).unwrap()
    }

    fn composition(pairs: &[(&str, f64)]) -> Composition {
        Composition::new(pairs.iter().map(|&(sym, amt)| (element(sym), amt))).unwrap()
    }

    fn candidate(id: &str, pairs: &[(&str, f64)]) -> PrecursorCandidate {
        PrecursorCandidate {
            id: PrecursorId(id.to_string()),
            composition: composition(pairs),
            availability: None,
        }
    }

    fn write_temp(name: &str, contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("gugen_cli_test_{name}.json"));
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[cfg(feature = "commercial_catalog")]
    fn write_temp_csv(name: &str, contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("gugen_cli_test_{name}.csv"));
        std::fs::write(&path, contents).unwrap();
        path
    }

    fn barium_titanate_target_json() -> String {
        serde_json::to_string(&TargetSpecification {
            composition: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
            structure: None,
            desired_phase: None,
            constraints: PlanningConstraints::default(),
        })
        .unwrap()
    }

    fn barium_titanate_catalog_json() -> String {
        serde_json::to_string(&vec![
            candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
        ])
        .unwrap()
    }

    /// Fixed input/timestamp (not `now_rfc3339`, which is real wall-clock
    /// and would make a byte-for-byte snapshot flaky) for the AGENTS.md
    /// §21.6 golden tests below.
    fn golden_report() -> SynthesisPlanningReport {
        let catalog = InMemoryPrecursorCatalog::new(vec![
            candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
            candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
        ]);
        let target = TargetSpecification {
            composition: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
            structure: None,
            desired_phase: None,
            constraints: PlanningConstraints::default(),
        };
        Planner::builder(catalog, PlanningConfig::default())
            .build()
            .plan(&target, "2026-08-14T00:00:00Z")
            .unwrap()
    }

    /// AGENTS.md §21.6: JSON output schema must not change unintentionally.
    /// Golden file generated by actually running this and copying its real
    /// output (same discipline as `examples/balance_batio3.rs`'s README
    /// snippet) -- if this fails after a deliberate schema change, update
    /// `tests/fixtures/batio3_report.json` from the new real output, don't
    /// hand-edit it to match. Generated under `--all-features`:
    /// `provenance.enabled_features` reflects whatever features this test
    /// binary was built with, so this test only runs (and only needs to
    /// pass) under `--all-features`; a feature-set mismatch here is not a
    /// schema regression.
    #[test]
    fn json_output_matches_the_golden_snapshot() {
        let rendered = serde_json::to_string_pretty(&golden_report()).unwrap();
        let golden = include_str!("../../tests/fixtures/batio3_report.json");
        assert_eq!(rendered.trim_end(), golden.trim_end());
    }

    /// AGENTS.md §21.6: markdown output schema must not change
    /// unintentionally either. Same discipline as the JSON golden test.
    #[test]
    fn markdown_output_matches_the_golden_snapshot() {
        let rendered = render_report_markdown(&golden_report());
        let golden = include_str!("../../tests/fixtures/batio3_report.md");
        assert_eq!(rendered.trim_end(), golden.trim_end());
    }

    #[test]
    fn plan_command_produces_a_report_with_at_least_one_plan() {
        let target_path = write_temp("plan_target", &barium_titanate_target_json());
        let catalog_path = write_temp("plan_catalog", &barium_titanate_catalog_json());
        let output_path = std::env::temp_dir().join("gugen_cli_test_plan_output.json");

        run_plan(
            &target_path,
            &catalog_path,
            Some(&output_path),
            OutputFormat::Json,
        )
        .unwrap();

        let report: SynthesisPlanningReport =
            serde_json::from_str(&std::fs::read_to_string(&output_path).unwrap()).unwrap();
        assert!(!report.plans.is_empty());
    }

    #[test]
    fn plan_command_markdown_format_contains_key_sections() {
        let target_path = write_temp("markdown_target", &barium_titanate_target_json());
        let catalog_path = write_temp("markdown_catalog", &barium_titanate_catalog_json());
        let output_path = std::env::temp_dir().join("gugen_cli_test_markdown_output.md");

        run_plan(
            &target_path,
            &catalog_path,
            Some(&output_path),
            OutputFormat::Markdown,
        )
        .unwrap();

        let text = std::fs::read_to_string(&output_path).unwrap();
        assert!(text.contains("# Synthesis Planning Report"));
        assert!(text.contains("## Plan plan-"));
        assert!(text.contains("### Score breakdown"));
    }

    #[test]
    fn explain_finds_a_plan_by_id_and_errors_on_an_unknown_one() {
        let target_path = write_temp("explain_target", &barium_titanate_target_json());
        let catalog_path = write_temp("explain_catalog", &barium_titanate_catalog_json());
        let report_path = std::env::temp_dir().join("gugen_cli_test_explain_report.json");
        run_plan(
            &target_path,
            &catalog_path,
            Some(&report_path),
            OutputFormat::Json,
        )
        .unwrap();

        let report: SynthesisPlanningReport =
            serde_json::from_str(&std::fs::read_to_string(&report_path).unwrap()).unwrap();
        let plan_id = report.plans[0].plan_id.0.clone();

        assert!(run_explain(&report_path, &plan_id).is_ok());
        assert!(run_explain(&report_path, "plan-does-not-exist").is_err());
    }

    #[test]
    fn validate_target_accepts_a_well_formed_target() {
        let path = write_temp("validate_ok_target", &barium_titanate_target_json());
        assert!(run_validate_target(&path).is_ok());
    }

    #[test]
    fn validate_target_rejects_a_self_contradictory_target() {
        let mut target = TargetSpecification {
            composition: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
            structure: None,
            desired_phase: None,
            constraints: PlanningConstraints::default(),
        };
        target.constraints.forbidden_elements.insert(element("Ba"));
        let path = write_temp(
            "validate_bad_target",
            &serde_json::to_string(&target).unwrap(),
        );
        assert!(run_validate_target(&path).is_err());
    }

    #[test]
    fn doctor_report_mentions_versions_and_mikiwame_status() {
        let text = doctor_report();
        assert!(text.contains("gugen version:"));
        assert!(text.contains("mikiwame integration status:"));
        assert!(text.contains("chematic-crystal integration status:"));
        assert!(text.contains("literature evidence provider:"));
        assert!(text.contains("known limitations:"));
    }

    /// One target failing to plan (a `PrecursorCatalog` error) must not
    /// stop the rest of the batch (AGENTS.md §26 Phase 7). `FlakyCatalog`
    /// fails only for Sr-containing targets so success and failure are
    /// both exercised in the same batch.
    struct FlakyCatalog;
    impl gugen::PrecursorCatalog for FlakyCatalog {
        fn candidates_for(
            &self,
            target: &Composition,
            constraints: &PlanningConstraints,
        ) -> Result<Vec<PrecursorCandidate>, ProviderError> {
            if target.elements().any(|e| e.symbol() == "Sr") {
                return Err(ProviderError::Unavailable("simulated outage".to_string()));
            }
            InMemoryPrecursorCatalog::new(vec![
                candidate("BaCO3", &[("Ba", 1.0), ("C", 1.0), ("O", 3.0)]),
                candidate("TiO2", &[("Ti", 1.0), ("O", 2.0)]),
            ])
            .candidates_for(target, constraints)
        }
    }

    #[test]
    fn batch_isolates_one_targets_failure_from_the_rest() {
        let planner = Planner::builder(FlakyCatalog, PlanningConfig::default()).build();
        let targets = vec![
            TargetSpecification {
                composition: composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]),
                structure: None,
                desired_phase: None,
                constraints: PlanningConstraints::default(),
            },
            TargetSpecification {
                composition: composition(&[("Sr", 1.0), ("Ti", 1.0), ("O", 3.0)]),
                structure: None,
                desired_phase: None,
                constraints: PlanningConstraints::default(),
            },
        ];

        let entries = batch_plan(&planner, &targets, "2026-08-14T00:00:00Z");

        assert_eq!(entries.len(), 2);
        assert!(entries[0].report.is_some(), "Ba-Ti-O target should succeed");
        assert!(entries[0].error.is_none());
        assert!(
            entries[1].report.is_none(),
            "Sr-Ti-O target should fail via FlakyCatalog"
        );
        assert!(entries[1].error.is_some());
    }

    #[test]
    fn now_rfc3339_round_trips_through_the_civil_calendar_conversion() {
        // 2024-01-01T00:00:00Z is a known epoch-day boundary (19723 days
        // after 1970-01-01) -- a fixed regression check on civil_from_days
        // rather than on the live clock.
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        let now = now_rfc3339();
        assert!(now.starts_with("20"), "unexpected timestamp: {now}");
        assert!(now.ends_with('Z'));
    }

    #[cfg(feature = "commercial_catalog")]
    fn golden_commercial_assessment() -> (
        SynthesisPlanningReport,
        Vec<CommercialPlanAssessment>,
        CommercialCatalogLoadReport,
    ) {
        let report = golden_report();
        let (catalog, load_report) = CommercialPrecursorCatalog::load_csv(
            include_str!("../../tests/fixtures/commercial_catalog_sample.csv"),
            CommercialCatalogLoadMode::Lenient,
        )
        .unwrap();
        let assessments = assess_commercial_plans(
            &report.plans,
            &catalog,
            &CommercialPlanningRequest::default(),
            &CommercialPlanningConfig::default(),
        )
        .unwrap();
        (report, assessments, load_report)
    }

    /// AGENTS.md §21.6: JSON output schema must not change unintentionally.
    #[cfg(feature = "commercial_catalog")]
    #[test]
    fn commercial_plan_json_output_matches_the_golden_snapshot() {
        let (report, assessments, _) = golden_commercial_assessment();
        let results: Vec<CommercialPlanResult> = report
            .plans
            .iter()
            .zip(assessments.iter())
            .map(|(plan, assessment)| CommercialPlanResult {
                route_family: plan.route_family,
                assessment: assessment.clone(),
            })
            .collect();
        let rendered = serde_json::to_string_pretty(&results).unwrap();
        let golden = include_str!("../../tests/fixtures/commercial_plan_report.json");
        assert_eq!(rendered.trim_end(), golden.trim_end());
    }

    /// AGENTS.md §21.6: markdown output schema must not change unintentionally.
    #[cfg(feature = "commercial_catalog")]
    #[test]
    fn commercial_plan_markdown_output_matches_the_golden_snapshot() {
        let (report, assessments, load_report) = golden_commercial_assessment();
        let rendered = render_commercial_report_markdown(
            &report.target,
            &report.plans,
            &assessments,
            &load_report,
        );
        let golden = include_str!("../../tests/fixtures/commercial_plan_report.md");
        assert_eq!(rendered.trim_end(), golden.trim_end());
    }

    #[cfg(feature = "commercial_catalog")]
    #[test]
    fn commercial_plan_json_output_is_a_non_empty_array() {
        let target_path = write_temp("commercial_target", &barium_titanate_target_json());
        let catalog_path = write_temp("commercial_catalog", &barium_titanate_catalog_json());
        let commercial_catalog_path = write_temp_csv(
            "commercial_offers",
            include_str!("../../tests/fixtures/commercial_catalog_sample.csv"),
        );
        let output_path = std::env::temp_dir().join("gugen_cli_test_commercial_plan_output.json");

        run_commercial_plan(
            &target_path,
            &catalog_path,
            &commercial_catalog_path,
            CommercialCatalogLoadMode::Lenient,
            None,
            None,
            None,
            None,
            None,
            None,
            &[],
            &[],
            Some(&output_path),
            CommercialOutputFormat::Json,
        )
        .unwrap();

        let text = std::fs::read_to_string(&output_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(!value.as_array().unwrap().is_empty());
    }

    #[cfg(feature = "commercial_catalog")]
    #[test]
    fn commercial_plan_accepts_a_json_commercial_catalog() {
        let target_path = write_temp(
            "commercial_target_json_catalog",
            &barium_titanate_target_json(),
        );
        let catalog_path = write_temp(
            "commercial_catalog_for_json_catalog",
            &barium_titanate_catalog_json(),
        );
        let commercial_catalog_path = write_temp(
            "commercial_offers_json",
            include_str!("../../tests/fixtures/commercial_catalog_sample.json"),
        );
        let output_path =
            std::env::temp_dir().join("gugen_cli_test_commercial_plan_json_catalog_output.json");

        run_commercial_plan(
            &target_path,
            &catalog_path,
            &commercial_catalog_path,
            CommercialCatalogLoadMode::Lenient,
            None,
            None,
            None,
            None,
            None,
            None,
            &[],
            &[],
            Some(&output_path),
            CommercialOutputFormat::Json,
        )
        .unwrap();

        let text = std::fs::read_to_string(&output_path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert!(!value.as_array().unwrap().is_empty());
    }

    #[cfg(feature = "commercial_catalog")]
    #[test]
    fn commercial_plan_markdown_format_contains_key_sections() {
        let target_path = write_temp("commercial_markdown_target", &barium_titanate_target_json());
        let catalog_path = write_temp(
            "commercial_markdown_catalog",
            &barium_titanate_catalog_json(),
        );
        let commercial_catalog_path = write_temp_csv(
            "commercial_markdown_offers",
            include_str!("../../tests/fixtures/commercial_catalog_sample.csv"),
        );
        let output_path =
            std::env::temp_dir().join("gugen_cli_test_commercial_plan_markdown_output.md");

        run_commercial_plan(
            &target_path,
            &catalog_path,
            &commercial_catalog_path,
            CommercialCatalogLoadMode::Lenient,
            None,
            None,
            None,
            None,
            None,
            None,
            &[],
            &[],
            Some(&output_path),
            CommercialOutputFormat::Markdown,
        )
        .unwrap();

        let text = std::fs::read_to_string(&output_path).unwrap();
        assert!(text.contains("# Commercial Precursor Assessment"));
        assert!(text.contains("gugen does not certify commercial data"));
        assert!(text.contains("## Plan plan-"));
    }

    #[cfg(feature = "commercial_catalog")]
    #[test]
    fn commercial_plan_csv_output_has_the_expected_header() {
        let target_path = write_temp("commercial_csv_target", &barium_titanate_target_json());
        let catalog_path = write_temp("commercial_csv_catalog", &barium_titanate_catalog_json());
        let commercial_catalog_path = write_temp_csv(
            "commercial_csv_offers",
            include_str!("../../tests/fixtures/commercial_catalog_sample.csv"),
        );
        let output_path = std::env::temp_dir().join("gugen_cli_test_commercial_plan_output.csv");

        run_commercial_plan(
            &target_path,
            &catalog_path,
            &commercial_catalog_path,
            CommercialCatalogLoadMode::Lenient,
            None,
            None,
            None,
            None,
            None,
            None,
            &[],
            &[],
            Some(&output_path),
            CommercialOutputFormat::Csv,
        )
        .unwrap();

        let text = std::fs::read_to_string(&output_path).unwrap();
        let mut lines = text.lines();
        assert_eq!(
            lines.next().unwrap(),
            "plan_id,every_precursor_has_a_match,combination_rank,combination_id,\
            total_cost_minor_units,currency,all_costs_known,max_lead_time_days,\
            all_availability_acceptable,note"
        );
        assert!(lines.next().is_some());
    }

    #[cfg(feature = "commercial_catalog")]
    #[test]
    fn commercial_plan_id_narrows_to_one_result_and_errors_on_unknown_id() {
        let target_path = write_temp("commercial_plan_id_target", &barium_titanate_target_json());
        let catalog_path = write_temp(
            "commercial_plan_id_catalog",
            &barium_titanate_catalog_json(),
        );
        let commercial_catalog_path = write_temp_csv(
            "commercial_plan_id_offers",
            include_str!("../../tests/fixtures/commercial_catalog_sample.csv"),
        );
        let plan_report_path =
            std::env::temp_dir().join("gugen_cli_test_commercial_plan_id_report.json");
        run_plan(
            &target_path,
            &catalog_path,
            Some(&plan_report_path),
            OutputFormat::Json,
        )
        .unwrap();
        let report: SynthesisPlanningReport =
            serde_json::from_str(&std::fs::read_to_string(&plan_report_path).unwrap()).unwrap();
        let plan_id = report.plans[0].plan_id.0.clone();

        let output_path =
            std::env::temp_dir().join("gugen_cli_test_commercial_plan_id_output.json");
        run_commercial_plan(
            &target_path,
            &catalog_path,
            &commercial_catalog_path,
            CommercialCatalogLoadMode::Lenient,
            Some(&plan_id),
            None,
            None,
            None,
            None,
            None,
            &[],
            &[],
            Some(&output_path),
            CommercialOutputFormat::Json,
        )
        .unwrap();
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&output_path).unwrap()).unwrap();
        assert_eq!(value.as_array().unwrap().len(), 1);

        assert!(
            run_commercial_plan(
                &target_path,
                &catalog_path,
                &commercial_catalog_path,
                CommercialCatalogLoadMode::Lenient,
                Some("plan-does-not-exist"),
                None,
                None,
                None,
                None,
                None,
                &[],
                &[],
                None,
                CommercialOutputFormat::Json,
            )
            .is_err()
        );
    }

    #[cfg(feature = "commercial_catalog")]
    #[test]
    fn load_commercial_catalog_dispatches_by_extension_and_rejects_unknown_ones() {
        let csv_path = write_temp_csv(
            "load_dispatch_csv",
            include_str!("../../tests/fixtures/commercial_catalog_sample.csv"),
        );
        let (_, csv_report) =
            load_commercial_catalog(&csv_path, CommercialCatalogLoadMode::Lenient).unwrap();
        assert!(csv_report.accepted > 0);

        let json_path = write_temp(
            "load_dispatch_json",
            include_str!("../../tests/fixtures/commercial_catalog_sample.json"),
        );
        let (_, json_report) =
            load_commercial_catalog(&json_path, CommercialCatalogLoadMode::Lenient).unwrap();
        assert!(json_report.accepted > 0);

        let txt_path = std::env::temp_dir().join("gugen_cli_test_load_dispatch.txt");
        std::fs::write(&txt_path, "irrelevant").unwrap();
        assert!(load_commercial_catalog(&txt_path, CommercialCatalogLoadMode::Lenient).is_err());
    }

    #[cfg(feature = "commercial_catalog")]
    #[test]
    fn build_commercial_planning_request_requires_cost_and_currency_together() {
        let comp = composition(&[("Ba", 1.0), ("Ti", 1.0), ("O", 3.0)]);

        assert!(
            build_commercial_planning_request(
                comp.clone(),
                None,
                None,
                None,
                Some(1000),
                None,
                &[],
                &[]
            )
            .is_err()
        );
        assert!(
            build_commercial_planning_request(
                comp.clone(),
                None,
                None,
                None,
                None,
                Some("USD"),
                &[],
                &[]
            )
            .is_err()
        );
        assert!(
            build_commercial_planning_request(comp.clone(), None, None, None, None, None, &[], &[])
                .is_ok()
        );
        assert!(
            build_commercial_planning_request(
                comp,
                None,
                None,
                None,
                Some(1000),
                Some("USD"),
                &[],
                &[]
            )
            .is_ok()
        );
    }
}
