mod commands;
mod workspace;

use clap::{Parser, Subcommand};
use std::process::ExitCode;

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
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Run { task } => commands::run::execute(&task),
    };

    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("lazel: {error:#}");
            ExitCode::FAILURE
        }
    }
}
