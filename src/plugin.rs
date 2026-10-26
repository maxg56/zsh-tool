//! Plugin names and resolution.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer};

/// A plugin name that is not safe to use as a directory name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidName {
    name: String,
    reason: &'static str,
}

impl fmt::Display for InvalidName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid plugin name {:?}: {}", self.name, self.reason)
    }
}

impl std::error::Error for InvalidName {}

/// Checks that `name` can be used as a single path component: only
/// `[A-Za-z0-9._-]`, not empty, no `..`, and no leading `-` or `.`.
pub fn check_name(name: &str) -> Result<(), InvalidName> {
    let reason = if name.is_empty() {
        Some("it is empty")
    } else if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    {
        Some("only letters, digits, '.', '_' and '-' are allowed")
    } else if name.starts_with(['-', '.']) {
        Some("it must not start with '-' or '.'")
    } else if name.contains("..") {
        Some("it must not contain '..'")
    } else {
        None
    };
    match reason {
        Some(reason) => Err(InvalidName {
            name: name.to_owned(),
            reason,
        }),
        None => Ok(()),
    }
}

/// A plugin name that passed [`check_name`] while being deserialized.
pub struct ValidName(pub String);

impl<'de> Deserialize<'de> for ValidName {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Failing inside the visitor, rather than after deserializing a
        // string, lets the TOML deserializer point at the name's own line.
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = ValidName;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a plugin name")
            }

            fn visit_str<E: serde::de::Error>(self, name: &str) -> Result<ValidName, E> {
                check_name(name).map_err(E::custom)?;
                Ok(ValidName(name.to_owned()))
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

use crate::manifest::{self, Manifest, Metadata};

/// Where a plugin name led.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// A directory with a valid `plugin.toml`.
    Native { dir: PathBuf, manifest: Manifest },
    /// A directory whose `plugin.toml` is invalid; the plugin is skipped.
    InvalidManifest {
        dir: PathBuf,
        /// What is wrong, with file and line.
        error: String,
    },
    /// An Oh My Zsh style plugin directory.
    Legacy {
        dir: PathBuf,
        /// `<name>.plugin.zsh`, absent for completion-only plugins.
        script: Option<PathBuf>,
    },
    /// No search directory holds the plugin.
    NotFound,
}

impl Resolved {
    /// The plugin directory, unless the plugin was not found.
    pub fn dir(&self) -> Option<&Path> {
        match self {
            Self::Native { dir, .. }
            | Self::InvalidManifest { dir, .. }
            | Self::Legacy { dir, .. } => Some(dir),
            Self::NotFound => None,
        }
    }
}

/// A configured plugin and its resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    pub name: String,
    pub resolved: Resolved,
}

impl Plugin {
    /// Looks `name` up in `search_dirs`, first match wins. Within a
    /// directory, `plugin.toml` takes precedence over `*.plugin.zsh`. An
    /// invalid name is never joined to a directory and is reported as not
    /// found.
    pub fn resolve(name: &str, search_dirs: &[PathBuf]) -> Self {
        let resolved = check_name(name)
            .ok()
            .and_then(|()| {
                search_dirs.iter().find_map(|base| {
                    let dir = base.join(name);
                    native(&dir).or_else(|| legacy(&dir, name))
                })
            })
            .unwrap_or(Resolved::NotFound);
        Self {
            name: name.to_owned(),
            resolved,
        }
    }

    /// Name, version and description declared by a native plugin.
    pub fn metadata(&self) -> Option<&Metadata> {
        match &self.resolved {
            Resolved::Native { manifest, .. } => Some(&manifest.metadata),
            _ => None,
        }
    }
}

/// Recognises a native plugin directory, valid or not.
fn native(dir: &Path) -> Option<Resolved> {
    if !dir.join(manifest::FILE).is_file() {
        return None;
    }
    Some(match Manifest::load(dir) {
        Ok(manifest) => Resolved::Native {
            dir: dir.to_owned(),
            manifest,
        },
        Err(err) => Resolved::InvalidManifest {
            dir: dir.to_owned(),
            error: format!("{err:#}"),
        },
    })
}

/// Whether `name` is safe as a plugin or theme name (see [`check_name`]).
pub fn valid_name(name: &str) -> bool {
    check_name(name).is_ok()
}

/// Recognises a plugin directory the way Oh My Zsh does: it must contain
/// `<name>.plugin.zsh` or a completion function `_<name>`.
fn legacy(dir: &Path, name: &str) -> Option<Resolved> {
    let script = dir.join(format!("{name}.plugin.zsh"));
    if script.is_file() {
        return Some(Resolved::Legacy {
            dir: dir.to_owned(),
            script: Some(script),
        });
    }
    dir.join(format!("_{name}"))
        .is_file()
        .then(|| Resolved::Legacy {
            dir: dir.to_owned(),
            script: None,
        })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn touch(path: &Path) {
        write(path, "");
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn native_takes_precedence_over_legacy() {
        let root = tempfile::tempdir().unwrap();
        let dirs = [root.path().to_owned()];
        touch(&root.path().join("git/git.plugin.zsh"));
        write(
            &root.path().join("git/plugin.toml"),
            "[plugin]\nname = \"git\"\nversion = \"1.0.0\"\n",
        );
        let plugin = Plugin::resolve("git", &dirs);
        assert!(
            matches!(&plugin.resolved, Resolved::Native { dir, .. } if *dir == root.path().join("git"))
        );
        let metadata = plugin.metadata().unwrap();
        assert_eq!(metadata.name, "git");
        assert_eq!(metadata.version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn invalid_manifest_stops_resolution() {
        let root = tempfile::tempdir().unwrap();
        let dirs = [root.path().join("a"), root.path().join("b")];
        write(
            &dirs[0].join("git/plugin.toml"),
            "[plugin]\nname = \"tig\"\n",
        );
        touch(&dirs[0].join("git/git.plugin.zsh"));
        touch(&dirs[1].join("git/git.plugin.zsh"));
        let plugin = Plugin::resolve("git", &dirs);
        let Resolved::InvalidManifest { dir, error } = &plugin.resolved else {
            panic!("{:?}", plugin.resolved);
        };
        assert_eq!(*dir, dirs[0].join("git"));
        assert!(
            error.contains("line 2") && error.contains("`tig`"),
            "{error}"
        );
        assert_eq!(plugin.metadata(), None);
    }

    #[test]
    fn native_found_in_later_directory() {
        let root = tempfile::tempdir().unwrap();
        let dirs = [root.path().join("a"), root.path().join("b")];
        touch(&dirs[0].join("z/README.md"));
        write(&dirs[1].join("z/plugin.toml"), "[plugin]\nname = \"z\"\n");
        let plugin = Plugin::resolve("z", &dirs);
        assert!(
            matches!(&plugin.resolved, Resolved::Native { dir, .. } if *dir == dirs[1].join("z"))
        );
    }

    #[test]
    fn first_directory_wins() {
        let root = tempfile::tempdir().unwrap();
        let dirs = [root.path().join("a"), root.path().join("b")];
        touch(&dirs[0].join("git/git.plugin.zsh"));
        touch(&dirs[1].join("git/git.plugin.zsh"));
        let plugin = Plugin::resolve("git", &dirs);
        assert_eq!(
            plugin.resolved,
            Resolved::Legacy {
                dir: dirs[0].join("git"),
                script: Some(dirs[0].join("git/git.plugin.zsh")),
            }
        );
    }

    #[test]
    fn skips_directories_without_plugin_file() {
        let root = tempfile::tempdir().unwrap();
        let dirs = [root.path().join("a"), root.path().join("b")];
        touch(&dirs[0].join("git/README.md"));
        touch(&dirs[1].join("git/git.plugin.zsh"));
        let plugin = Plugin::resolve("git", &dirs);
        assert!(
            matches!(plugin.resolved, Resolved::Legacy { dir, .. } if dir == dirs[1].join("git"))
        );
    }

    #[test]
    fn accepts_completion_only_plugin() {
        let root = tempfile::tempdir().unwrap();
        let dirs = [root.path().to_owned()];
        touch(&root.path().join("docker/_docker"));
        let plugin = Plugin::resolve("docker", &dirs);
        assert_eq!(
            plugin.resolved,
            Resolved::Legacy {
                dir: root.path().join("docker"),
                script: None,
            }
        );
    }

    #[test]
    fn accepts_valid_names() {
        for name in ["git", "zsh-autosuggestions", "a.b_c-1", "Z9"] {
            assert_eq!(check_name(name), Ok(()), "{name}");
        }
    }

    #[test]
    fn rejects_invalid_names() {
        for name in [
            "", ".", "..", ".hidden", "-rf", "a/b", "../x", "a..b", "a b", "a\nb", "é", "a;b",
        ] {
            assert!(check_name(name).is_err(), "{name:?}");
        }
        let err = check_name("a/b").unwrap_err().to_string();
        assert!(err.contains("\"a/b\""), "{err}");
    }

    #[test]
    fn invalid_name_never_leaves_search_dirs() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().join("plugins");
        // Without validation, `plugins/../x` would find `x.plugin.zsh` here.
        fs::create_dir_all(root.path().join("x")).unwrap();
        touch(&root.path().join("x.plugin.zsh"));
        let plugin = Plugin::resolve("../x", &[base]);
        assert_eq!(plugin.resolved, Resolved::NotFound);
    }

    #[test]
    fn reports_missing_plugin() {
        let root = tempfile::tempdir().unwrap();
        let plugin = Plugin::resolve("nope", &[root.path().to_owned()]);
        assert_eq!(plugin.resolved, Resolved::NotFound);
    }
}
