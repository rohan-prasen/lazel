mod cache;
mod commands;
mod workspace;

use clap::{Parser, Subcommand};
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
    /// Run a task in the workspace
    Run {
        /// Task to execute, in project:task form
        #[arg(value_name = "PROJECT:TASK")]
        task: String,
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
        Commands::Run { task, no_cache } => commands::run::execute(&task, no_cache),
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
