mod commands;

use clap::{Parser, Subcommand};

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
        /// Name of the task to execute
        task: String,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { task } => {
            commands::run::execute(&task)?;
        }
    }

    Ok(())
}
