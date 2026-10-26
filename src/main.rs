mod compat;
mod config;
mod init;
mod migrate;
mod paths;
mod plugin;
mod prompt;
mod zsh;

use std::io::Write;
use std::path::PathBuf;
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
    /// Convert an Oh My Zsh .zshrc: write config.toml and switch the
    /// .zshrc to zsh-tool (a backup is kept).
    Migrate {
        /// The .zshrc to migrate [default: $ZDOTDIR/.zshrc or ~/.zshrc]
        #[arg(long, value_name = "PATH")]
        zshrc: Option<PathBuf>,
        /// Print config.toml and the .zshrc changes without writing anything
        #[arg(long)]
        dry_run: bool,
        /// Overwrite an existing config.toml
        #[arg(long)]
        force: bool,
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
        Command::Migrate {
            zshrc,
            dry_run,
            force,
        } => {
            let options = migrate::Options {
                zshrc,
                dry_run,
                force,
            };
            migrate::run(
                &options,
                |name| std::env::var_os(name).map(PathBuf::from),
                &mut std::io::stdout().lock(),
            )?;
        }
    }
    Ok(())
}
