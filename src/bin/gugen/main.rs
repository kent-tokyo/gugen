//! CLI for gugen (AGENTS.md §19): `plan`, `balance`, `explain`,
//! `validate-target`, `doctor`, `batch`, and (with the `commercial_catalog`
//! feature) `commercial-plan`.
//!
//! This binary is the one place in the crate allowed to read the system
//! clock (`now_rfc3339`, used for `execution_timestamp`) -- the planning
//! core never does (AGENTS.md §25).

mod cli;
mod commands;
mod render;

use clap::Parser;

fn main() {
    if let Err(err) = commands::run(cli::Cli::parse()) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
