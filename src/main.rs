mod cache;
mod commands;
mod workspace;

use clap::{Parser, Subcommand};
use std::num::NonZeroUsize;
use std::process;

#[derive(Debug, Parser)]
#[command(
    name = "lazel",
    version,
    about = "A lightweight, language-agnostic monorepo task orchestrator"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run tasks in the workspace
    Run {
        /// Bare task names or qualified project:task selectors
        #[arg(required = true, value_name = "TASK")]
        tasks: Vec<String>,
        /// Select targets in projects matching this name pattern
        #[arg(long, value_name = "PROJECT-PATTERN")]
        filter: Option<String>,
        /// Maximum number of concurrent tasks
        #[arg(long, default_value = "1", value_name = "N")]
        concurrency: NonZeroUsize,
        /// Run recipes with the inherited environment and bypass cached results
        #[arg(long)]
        no_cache: bool,
    },
    /// Update Lazel to the latest stable release
    #[command(visible_alias = "upgrade")]
    Update,
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Run {
            tasks,
            filter,
            concurrency,
            no_cache,
        } => commands::run::execute(&tasks, filter.as_deref(), concurrency.get(), no_cache),
        Commands::Update => commands::update::execute(),
    };

    let code = match result {
        Ok(code) => code,
        Err(error) => {
            eprintln!("lazel: {error:#}");
            1
        }
    };
    process::exit(code);
}
