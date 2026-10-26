//! `zsh-tool doctor`: checks the installation without touching the network.

use std::fmt;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use crate::config::{Config, Engine};
use crate::git;
use crate::lock::{Lock, Source};
use crate::manage::{local_change, location};
use crate::paths::Paths;
use crate::plugin::{Plugin, Resolved};

/// Oldest supported zsh.
const MIN_ZSH: (u32, u32) = (5, 8);

/// Severity of a check; any [`Level::Error`] makes `doctor` fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Info,
    Warning,
    Error,
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ok => "ok",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        })
    }
}

/// One line of the report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub level: Level,
    pub message: String,
}

#[derive(Default)]
struct Report(Vec<Check>);

impl Report {
    fn add(&mut self, level: Level, message: impl Into<String>) {
        self.0.push(Check {
            level,
            message: message.into(),
        });
    }
}

/// Runs every check, in a stable order.
pub fn run(paths: &Paths) -> Vec<Check> {
    let mut report = Report::default();
    let config = check_config(paths, &mut report);
    if let Some(config) = &config {
        check_plugins(paths, config, &mut report);
    }
    check_lock(paths, &mut report);
    check_zsh(paths, &mut report);
    check_starship(paths, config.as_ref(), &mut report);
    match &paths.omz_dir {
        Some(dir) => report.add(Level::Info, format!("Oh My Zsh found in {}", dir.display())),
        None => report.add(Level::Info, "Oh My Zsh not found (not required)"),
    }
    check_cache(paths, &mut report);
    report.0
}

/// Formats the report, one check per line.
pub fn render(checks: &[Check]) -> String {
    let mut text: String = checks
        .iter()
        .map(|check| format!("{:<8}{}\n", check.level.to_string(), check.message))
        .collect();
    let errors = checks.iter().filter(|c| c.level == Level::Error).count();
    let warnings = checks.iter().filter(|c| c.level == Level::Warning).count();
    text.push_str(&format!("\n{errors} error(s), {warnings} warning(s)\n"));
    text
}

fn check_config(paths: &Paths, report: &mut Report) -> Option<Config> {
    let file = paths.config_file.display();
    match Config::load(&paths.config_file) {
        Ok(loaded) if loaded.source.is_none() => {
            report.add(
                Level::Info,
                format!("no configuration at {file}, using defaults"),
            );
            Some(loaded.config)
        }
        Ok(loaded) => {
            report.add(Level::Ok, format!("configuration {file}"));
            Some(loaded.config)
        }
        Err(err) => {
            report.add(Level::Error, format!("{err:#}"));
            None
        }
    }
}

fn check_plugins(paths: &Paths, config: &Config, report: &mut Report) {
    let search_dirs = paths.plugin_dirs();
    for name in &config.plugins {
        let plugin = Plugin::resolve(name, &search_dirs);
        match &plugin.resolved {
            Resolved::Legacy { dir, .. } => report.add(
                Level::Ok,
                format!(
                    "plugin {name} ({}, {})",
                    location(paths, &plugin.resolved),
                    dir.display()
                ),
            ),
            Resolved::NotFound => report.add(
                Level::Error,
                format!("plugin {name} not found in {}", join(&search_dirs)),
            ),
        }
    }
}

fn check_lock(paths: &Paths, report: &mut Report) {
    let lock = match Lock::load(&paths.lock_file) {
        Ok(lock) => lock,
        Err(err) => return report.add(Level::Error, format!("{err:#}")),
    };
    for (name, entry) in &lock.plugins {
        let dir = paths.managed_dir().join(name);
        if !dir.is_dir() {
            report.add(
                Level::Warning,
                format!(
                    "managed plugin {name} is missing from {}; run `zsh-tool install`",
                    dir.display()
                ),
            );
            continue;
        }
        if let Source::Git(_) = entry.source
            && let Ok(head) = git::head(&dir)
            && head != entry.commit
        {
            report.add(
                Level::Warning,
                format!(
                    "managed plugin {name} is at {head}, the lock file says {}",
                    entry.commit
                ),
            );
            continue;
        }
        match local_change(paths, name, entry) {
            Ok(None) => report.add(
                Level::Ok,
                format!("managed plugin {name} matches the lock file"),
            ),
            Ok(Some(path)) => report.add(
                Level::Warning,
                format!(
                    "managed plugin {name} differs from commit {} of the lock file ({})",
                    entry.commit,
                    path.display()
                ),
            ),
            Err(err) => report.add(Level::Warning, format!("{err:#}")),
        }
    }
}

fn check_zsh(paths: &Paths, report: &mut Report) {
    let Some(zsh) = find_program(&paths.bin_dirs, "zsh") else {
        return report.add(Level::Error, "zsh not found in PATH");
    };
    let output = Command::new(&zsh).arg("--version").output();
    let text = output
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned());
    match text.as_deref().and_then(zsh_version) {
        Some(version) if version >= MIN_ZSH => report.add(
            Level::Ok,
            format!("zsh {}.{} ({})", version.0, version.1, zsh.display()),
        ),
        Some(version) => report.add(
            Level::Error,
            format!(
                "zsh {}.{} is too old ({}); zsh {}.{} or later is required",
                version.0,
                version.1,
                zsh.display(),
                MIN_ZSH.0,
                MIN_ZSH.1
            ),
        ),
        None => report.add(
            Level::Error,
            format!("cannot read the version of {}", zsh.display()),
        ),
    }
}

fn check_starship(paths: &Paths, config: Option<&Config>, report: &mut Report) {
    let required = config.is_some_and(|config| config.prompt.engine == Engine::Starship);
    match (find_program(&paths.bin_dirs, "starship"), required) {
        (Some(path), _) => report.add(Level::Ok, format!("starship found ({})", path.display())),
        (None, true) => report.add(
            Level::Error,
            "starship not found in PATH, but the prompt engine is \"starship\"",
        ),
        (None, false) => report.add(Level::Info, "starship not found (not required)"),
    }
}

fn check_cache(paths: &Paths, report: &mut Report) {
    let dir = &paths.cache_dir;
    let probe = dir.join(format!(".doctor.{}", std::process::id()));
    let writable = fs::create_dir_all(dir)
        .and_then(|()| fs::write(&probe, ""))
        .is_ok();
    // The probe may not exist if the write failed; nothing to clean then.
    let _ = fs::remove_file(&probe);
    if writable {
        report.add(
            Level::Ok,
            format!("cache directory {} is writable", dir.display()),
        );
    } else {
        report.add(
            Level::Error,
            format!("cache directory {} is not writable", dir.display()),
        );
    }
}

/// Parses the `major.minor` version from `zsh --version` output, such as
/// `zsh 5.9 (x86_64-pc-linux-gnu)`.
fn zsh_version(text: &str) -> Option<(u32, u32)> {
    let version = text.strip_prefix("zsh ")?.split_whitespace().next()?;
    let mut parts = version.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |minor| {
        let digits: String = minor.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    })?;
    Some((major, minor))
}

/// First executable file named `name` in `dirs`.
fn find_program(dirs: &[PathBuf], name: &str) -> Option<PathBuf> {
    dirs.iter().map(|dir| dir.join(name)).find(|path| {
        fs::metadata(path)
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    })
}

fn join(dirs: &[PathBuf]) -> String {
    dirs.iter()
        .map(|dir| dir.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: tempfile::TempDir,
        paths: Paths,
    }

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let home = root.path().to_owned();
            fs::create_dir_all(home.join(".oh-my-zsh/plugins")).unwrap();
            let bin = home.join("bin");
            fs::create_dir(&bin).unwrap();
            let paths = Paths::resolve(|name| match name {
                "HOME" => Some(home.clone()),
                "PATH" => Some(bin.clone()),
                _ => None,
            })
            .unwrap();
            let fixture = Self { root, paths };
            fixture.program("zsh", "echo 'zsh 5.9 (x86_64-pc-linux-gnu)'");
            fixture
        }

        fn program(&self, name: &str, body: &str) {
            let path = self.root.path().join("bin").join(name);
            fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }

        fn config(&self, text: &str) {
            fs::create_dir_all(self.paths.config_file.parent().unwrap()).unwrap();
            fs::write(&self.paths.config_file, text).unwrap();
        }

        fn omz_plugin(&self, name: &str) {
            let dir = self.root.path().join(".oh-my-zsh/plugins").join(name);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join(format!("{name}.plugin.zsh")), "").unwrap();
        }

        fn levels(&self, needle: &str) -> Vec<Level> {
            run(&self.paths)
                .into_iter()
                .filter(|check| check.message.contains(needle))
                .map(|check| check.level)
                .collect()
        }
    }

    #[test]
    fn clean_installation_has_no_error() {
        let fx = Fixture::new();
        fx.omz_plugin("git");
        fx.config("plugins = [\"git\"]\n");
        let checks = run(&fx.paths);
        assert!(checks.iter().all(|c| c.level != Level::Error), "{checks:?}");
        assert_eq!(fx.levels("plugin git"), [Level::Ok]);
        assert_eq!(fx.levels("Oh My Zsh found"), [Level::Info]);
        assert_eq!(fx.levels("zsh 5.9"), [Level::Ok]);
        assert_eq!(fx.levels("starship"), [Level::Info]);
        assert_eq!(fx.levels("is writable"), [Level::Ok]);
        assert!(render(&checks).ends_with("\n0 error(s), 0 warning(s)\n"));
    }

    #[test]
    fn reports_missing_plugin() {
        let fx = Fixture::new();
        fx.config("plugins = [\"absent\"]\n");
        assert_eq!(fx.levels("plugin absent not found"), [Level::Error]);
    }

    #[test]
    fn reports_invalid_configuration_with_line() {
        let fx = Fixture::new();
        fx.config("plugins = [\"git\"]\n[prompt\n");
        let checks = run(&fx.paths);
        let error = checks.iter().find(|c| c.level == Level::Error).unwrap();
        assert!(error.message.contains("line 2"), "{error:?}");
    }

    #[test]
    fn checks_zsh_version() {
        let fx = Fixture::new();
        assert_eq!(fx.levels("zsh 5.9"), [Level::Ok]);
        fx.program("zsh", "echo 'zsh 5.7.1 (x86_64-apple-darwin)'");
        assert_eq!(fx.levels("too old"), [Level::Error]);
        fx.program("zsh", "echo 'zsh 5.8 (x86_64-apple-darwin)'");
        assert_eq!(fx.levels("zsh 5.8"), [Level::Ok]);
        fx.program("zsh", "exit 1");
        assert_eq!(fx.levels("cannot read the version"), [Level::Error]);
        fs::remove_file(fx.root.path().join("bin/zsh")).unwrap();
        assert_eq!(fx.levels("zsh not found"), [Level::Error]);
    }

    #[test]
    fn requires_starship_only_for_its_engine() {
        let fx = Fixture::new();
        fx.config("[prompt]\nengine = \"starship\"\n");
        assert_eq!(fx.levels("starship not found"), [Level::Error]);
        fx.program("starship", "exit 0");
        assert_eq!(fx.levels("starship found"), [Level::Ok]);
    }

    #[test]
    fn reports_unwritable_cache() {
        let mut fx = Fixture::new();
        let blocker = fx.root.path().join("file");
        fs::write(&blocker, "").unwrap();
        fx.paths.cache_dir = blocker.join("cache");
        assert_eq!(fx.levels("not writable"), [Level::Error]);
    }

    #[test]
    fn reports_missing_oh_my_zsh_as_info() {
        let mut fx = Fixture::new();
        fx.paths.omz_dir = None;
        assert_eq!(fx.levels("Oh My Zsh not found"), [Level::Info]);
    }

    #[test]
    fn reports_invalid_lock_file() {
        let fx = Fixture::new();
        fs::create_dir_all(fx.paths.lock_file.parent().unwrap()).unwrap();
        fs::write(&fx.paths.lock_file, "[[plugin]]\nname = \"..\"\n").unwrap();
        assert_eq!(fx.levels("zsh-tool.lock"), [Level::Error]);
    }

    #[test]
    fn parses_zsh_versions() {
        assert_eq!(zsh_version("zsh 5.9 (x86_64-pc-linux-gnu)\n"), Some((5, 9)));
        assert_eq!(
            zsh_version("zsh 5.8.1 (x86_64-apple-darwin21.0)"),
            Some((5, 8))
        );
        assert_eq!(zsh_version("zsh 6"), Some((6, 0)));
        assert_eq!(zsh_version("bash 5.2"), None);
        assert_eq!(zsh_version(""), None);
    }
}
