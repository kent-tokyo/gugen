//! Minimal CLI, extended as later phases land (AGENTS.md §19). v0.1 Phase 2
//! only implements `gugen balance`; `plan`, `explain`, `validate-target`,
//! `doctor`, and `batch` are Phase 7 work.

use clap::{Parser, Subcommand};
use gugen::Composition;
use std::path::PathBuf;

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
}

#[derive(serde::Deserialize)]
struct ReactionInput {
    reactants: Vec<Composition>,
    products: Vec<Composition>,
}

fn main() {
    if let Err(err) = run(Cli::parse()) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Command::Balance { path } => {
            let text = std::fs::read_to_string(&path)?;
            let input: ReactionInput = serde_json::from_str(&text)?;
            let results = gugen::balance(&input.reactants, &input.products)?;
            println!("{}", serde_json::to_string_pretty(&results)?);
            if results.is_empty() {
                eprintln!("no valid balance found for the given reactants/products");
            }
            Ok(())
        }
    }
}
