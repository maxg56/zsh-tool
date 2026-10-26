mod config;
mod doctor;
mod files;
mod git;
mod init;
mod lock;
mod manage;
mod paths;
mod plugin;
mod source;
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
    /// Manage plugins.
    Plugin {
        #[command(subcommand)]
        command: PluginCommand,
    },
    /// Update the registry and every plugin installed by zsh-tool.
    Upgrade,
    /// Install missing plugins at the commits recorded in zsh-tool.lock.
    Install,
    /// Check the installation.
    Doctor,
}

#[derive(Debug, Subcommand)]
enum PluginCommand {
    /// Install a plugin if needed and enable it.
    ///
    /// SOURCE is a plugin name (already installed, or taken from the
    /// registry), owner/repo on GitHub, an https:// URL, or an absolute
    /// path to a local repository.
    Add {
        source: String,
        /// Plugin name to use instead of the repository name.
        #[arg(long)]
        name: Option<String>,
    },
    /// Disable a plugin and delete it if zsh-tool installed it.
    Remove { name: String },
    /// List enabled plugins.
    List,
    /// Update plugins installed by zsh-tool (all of them by default).
    Update { names: Vec<String> },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Shell {
    Zsh,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("zsh-tool: error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let paths = Paths::from_env()?;
    let stdout = &mut std::io::stdout().lock();
    match cli.command {
        Command::Init { shell: Shell::Zsh } => {
            stdout.write_all(init::run(&paths)?.as_bytes())?;
        }
        Command::Plugin { command } => match command {
            PluginCommand::Add { source, name } => {
                manage::add(&paths, &source, name.as_deref(), stdout)?;
            }
            PluginCommand::Remove { name } => manage::remove(&paths, &name, stdout)?,
            PluginCommand::List => manage::list(&paths, stdout)?,
            PluginCommand::Update { names } => manage::update(&paths, &names, stdout)?,
        },
        Command::Upgrade => manage::upgrade(&paths, stdout)?,
        Command::Install => manage::install(&paths, stdout)?,
        Command::Doctor => {
            let checks = doctor::run(&paths);
            stdout.write_all(doctor::render(&checks).as_bytes())?;
            if checks
                .iter()
                .any(|check| check.level == doctor::Level::Error)
            {
                return Ok(ExitCode::FAILURE);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}
