//! Prompt engines: Starship and Oh My Zsh `*.zsh-theme` files.

use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::{Engine, Prompt};
use crate::paths::Paths;
use crate::plugin::valid_name;
use crate::zsh::{comment, quote, quote_path};

/// Functions and variables Oh My Zsh themes expect (git_prompt_info and
/// friends, colors, ls colors), emitted as is for the legacy engine.
const THEME_API: &str = include_str!("prompt/theme_api.zsh");
/// The `$FG`, `$BG` and `$FX` tables, emitted when the theme uses them.
const SPECTRUM: &str = include_str!("prompt/spectrum.zsh");

/// What the `[prompt]` table resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// The prompt is left alone.
    None,
    /// Starship, with its binary when found in `PATH`.
    Starship(Option<PathBuf>),
    /// A legacy theme file.
    Theme { name: String, file: PathBuf },
    /// The legacy engine is selected but no theme can be loaded; the text
    /// says why.
    NoTheme(String),
}

impl Resolved {
    /// Resolves `prompt`; `path_var` is the `PATH` searched for Starship.
    pub fn resolve(prompt: &Prompt, paths: &Paths, path_var: Option<&OsStr>) -> Self {
        match prompt.engine {
            Engine::None => Self::None,
            Engine::Starship => Self::Starship(find_program("starship", path_var)),
            Engine::Legacy => match prompt.theme.as_deref() {
                None | Some("") => Self::NoTheme("no theme set in [prompt]".to_owned()),
                Some(name) if !valid_name(name) => {
                    Self::NoTheme(format!("invalid theme name {}", quote(&comment(name))))
                }
                Some(name) => match find_theme(name, &theme_dirs(paths)) {
                    Some(file) => Self::Theme {
                        name: name.to_owned(),
                        file,
                    },
                    None => Self::NoTheme(format!("theme '{name}' not found")),
                },
            },
        }
    }

    fn is_legacy(&self) -> bool {
        matches!(self, Self::Theme { .. } | Self::NoTheme(_))
    }

    /// Writes what must exist before the plugins: the theme API for the
    /// legacy engine, so that plugins may replace parts of it.
    pub fn write_helpers(&self, out: &mut String) -> Result<()> {
        if !self.is_legacy() {
            return Ok(());
        }
        writeln!(out, "\n# prompt helpers (Oh My Zsh theme API)")?;
        out.push_str(THEME_API);
        if let Self::Theme { file, .. } = self {
            if uses_spectrum(file) {
                out.push_str(SPECTRUM);
            }
        }
        Ok(())
    }

    /// Writes the prompt set-up, which runs after compinit.
    pub fn write_prompt(&self, out: &mut String) -> Result<()> {
        match self {
            Self::None => {}
            Self::Starship(Some(_)) => {
                writeln!(out, "\n# prompt: starship")?;
                writeln!(
                    out,
                    "if (( $+commands[starship] )); then eval \"$(starship init zsh)\"; fi"
                )?;
            }
            Self::Starship(None) => {
                writeln!(out, "\n# prompt: starship not found in PATH")?;
            }
            Self::Theme { name, file } => {
                writeln!(out, "\n# prompt: theme {name}")?;
                writeln!(out, "source {}", quote_path(file)?)?;
                // As Oh My Zsh does once the theme is loaded.
                writeln!(
                    out,
                    "[[ -z $LS_COLORS ]] || zstyle ':completion:*' list-colors \"${{(s.:.)LS_COLORS}}\""
                )?;
            }
            Self::NoTheme(reason) => {
                writeln!(out, "\n# prompt: {}", comment(reason))?;
                writeln!(
                    out,
                    "print -ru2 -- {}",
                    quote(&format!("zsh-tool: warning: {reason}"))
                )?;
            }
        }
        Ok(())
    }
}

/// Directories searched for `<theme>.zsh-theme`, in order.
pub fn theme_dirs(paths: &Paths) -> Vec<PathBuf> {
    let mut dirs = vec![
        paths.data_dir.join("themes"),
        paths.omz_custom.join("themes"),
        paths.omz_custom.clone(),
    ];
    if let Some(omz) = &paths.omz_dir {
        dirs.push(omz.join("themes"));
    }
    dirs
}

/// First `<name>.zsh-theme` file found in `dirs`.
pub fn find_theme(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter()
        .map(|dir| dir.join(format!("{name}.zsh-theme")))
        .find(|file| file.is_file())
}

/// Whether the theme refers to the 256-color tables; an unreadable file
/// gets them, to be safe.
fn uses_spectrum(file: &Path) -> bool {
    fs::read(file).map_or(true, |text| {
        [b"FG[", b"BG[", b"FX["]
            .iter()
            .any(|needle| text.windows(3).any(|window| window == *needle))
    })
}

/// Looks an executable up in `path_var` like zsh does, skipping empty
/// entries (which would mean the current directory).
pub fn find_program(name: &str, path_var: Option<&OsStr>) -> Option<PathBuf> {
    std::env::split_paths(path_var?)
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(name))
        .find(|file| is_executable(file))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    struct Fixture {
        _root: tempfile::TempDir,
        paths: Paths,
    }

    fn fixture() -> Fixture {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().to_owned();
        fs::create_dir_all(home.join(".oh-my-zsh/themes")).unwrap();
        let paths = Paths::resolve(|name| (name == "HOME").then(|| home.clone())).unwrap();
        Fixture { _root: root, paths }
    }

    fn legacy(theme: Option<&str>) -> Prompt {
        Prompt {
            engine: Engine::Legacy,
            theme: theme.map(str::to_owned),
        }
    }

    fn touch(path: &Path, body: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    #[test]
    fn theme_resolution_order() {
        let fx = fixture();
        let omz = fx.paths.omz_dir.clone().unwrap();
        let candidates = [
            fx.paths.data_dir.join("themes/t.zsh-theme"),
            fx.paths.omz_custom.join("themes/t.zsh-theme"),
            fx.paths.omz_custom.join("t.zsh-theme"),
            omz.join("themes/t.zsh-theme"),
        ];
        for file in &candidates {
            touch(file, "");
        }
        for file in &candidates {
            let resolved = Resolved::resolve(&legacy(Some("t")), &fx.paths, None);
            assert_eq!(
                resolved,
                Resolved::Theme {
                    name: "t".to_owned(),
                    file: file.clone()
                }
            );
            fs::remove_file(file).unwrap();
        }
        assert_eq!(
            Resolved::resolve(&legacy(Some("t")), &fx.paths, None),
            Resolved::NoTheme("theme 't' not found".to_owned())
        );
    }

    #[test]
    fn rejects_unsafe_theme_names() {
        let fx = fixture();
        touch(&fx.paths.data_dir.join("evil.zsh-theme"), "");
        for name in ["../evil", "a/b", "x'y"] {
            let resolved = Resolved::resolve(&legacy(Some(name)), &fx.paths, None);
            assert!(
                matches!(&resolved, Resolved::NoTheme(reason) if reason.starts_with("invalid theme name")),
                "{resolved:?}"
            );
        }
    }

    #[test]
    fn missing_theme_is_reported_in_the_script() {
        let fx = fixture();
        let resolved = Resolved::resolve(&legacy(Some("nope")), &fx.paths, None);
        let mut out = String::new();
        resolved.write_helpers(&mut out).unwrap();
        resolved.write_prompt(&mut out).unwrap();
        assert!(out.contains("function git_prompt_info()"));
        assert!(out.contains("# prompt: theme 'nope' not found\n"));
        assert!(out.contains("print -ru2 -- 'zsh-tool: warning: theme '\\''nope'\\'' not found'"));
        assert_eq!(
            Resolved::resolve(&legacy(None), &fx.paths, None),
            Resolved::NoTheme("no theme set in [prompt]".to_owned())
        );
    }

    #[test]
    fn invalid_name_stays_quoted() {
        let fx = fixture();
        let resolved = Resolved::resolve(&legacy(Some("a\n$(x)")), &fx.paths, None);
        let mut out = String::new();
        resolved.write_prompt(&mut out).unwrap();
        assert!(
            out.contains("# prompt: invalid theme name 'a $(x)'\n"),
            "{out}"
        );
        assert!(!out.lines().any(|line| line.starts_with("$(x)")), "{out}");
    }

    #[test]
    fn theme_is_sourced_after_helpers() {
        let fx = fixture();
        let file = fx.paths.data_dir.join("themes/plain.zsh-theme");
        touch(&file, "PROMPT='%~ '\n");
        let resolved = Resolved::resolve(&legacy(Some("plain")), &fx.paths, None);
        let mut helpers = String::new();
        resolved.write_helpers(&mut helpers).unwrap();
        assert!(helpers.contains("setopt prompt_subst"));
        assert!(!helpers.contains("typeset -AHg FX FG BG"));
        let mut prompt = String::new();
        resolved.write_prompt(&mut prompt).unwrap();
        assert!(prompt.contains(&format!("source '{}'", file.display())));
    }

    #[test]
    fn spectrum_only_when_used() {
        let fx = fixture();
        let file = fx.paths.data_dir.join("themes/colorful.zsh-theme");
        touch(&file, "PROMPT=\"${FG[032]}%~\"\n");
        let resolved = Resolved::resolve(&legacy(Some("colorful")), &fx.paths, None);
        let mut out = String::new();
        resolved.write_helpers(&mut out).unwrap();
        assert!(out.contains("typeset -AHg FX FG BG"));
    }

    #[test]
    fn other_engines_have_no_helpers() {
        let mut out = String::new();
        Resolved::None.write_helpers(&mut out).unwrap();
        Resolved::None.write_prompt(&mut out).unwrap();
        Resolved::Starship(None).write_helpers(&mut out).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn starship_found_in_path() {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("bin");
        let starship = bin.join("starship");
        touch(&starship, "#!/bin/sh\n");
        let path_var = std::env::join_paths([root.path().join("empty"), bin.clone()]).unwrap();
        // Not executable yet.
        assert_eq!(find_program("starship", Some(&path_var)), None);
        fs::set_permissions(&starship, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(
            find_program("starship", Some(&path_var)),
            Some(starship.clone())
        );

        let prompt = Prompt {
            engine: Engine::Starship,
            theme: None,
        };
        let paths =
            Paths::resolve(|name| (name == "HOME").then(|| root.path().to_owned())).unwrap();
        let resolved = Resolved::resolve(&prompt, &paths, Some(&path_var));
        assert_eq!(resolved, Resolved::Starship(Some(starship)));
        let mut out = String::new();
        resolved.write_prompt(&mut out).unwrap();
        assert!(
            out.contains("if (( $+commands[starship] )); then eval \"$(starship init zsh)\"; fi")
        );
    }

    #[test]
    fn starship_missing_is_a_comment() {
        let resolved = Resolved::Starship(find_program("starship", None));
        let mut out = String::new();
        resolved.write_prompt(&mut out).unwrap();
        assert_eq!(out, "\n# prompt: starship not found in PATH\n");
    }

    #[test]
    fn empty_path_entries_are_ignored() {
        assert_eq!(find_program("sh", Some(OsStr::new(""))), None);
        assert_eq!(
            find_program("sh", Some(OsStr::new("/bin"))),
            Some(PathBuf::from("/bin/sh"))
        );
    }
}
