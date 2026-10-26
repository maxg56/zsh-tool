mod config;
mod init;
mod paths;
mod plugin;
mod zsh;

use std::io::Write;
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};

use crate::paths::Paths;

/// Fast zsh configuration manager, compatible with Oh My Zsh plugins.
#[derive(Debug, Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the shell init script; use as `eval "$(zsh-tool init zsh)"`.
    Init {
        #[arg(value_enum)]
        shell: Shell,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Shell {
    Zsh,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("zsh-tool: error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Init { shell: Shell::Zsh } => {
            let script = init::run(&Paths::from_env()?)?;
            std::io::stdout().lock().write_all(script.as_bytes())?;
        }
    }
    Ok(())
}
