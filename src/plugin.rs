//! Plugin resolution.

use std::path::{Path, PathBuf};

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

/// A configured plugin and its resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plugin {
    pub name: String,
    pub resolved: Resolved,
}

impl Plugin {
    /// Looks `name` up in `search_dirs`, first match wins. Within a
    /// directory, `plugin.toml` takes precedence over `*.plugin.zsh`.
    pub fn resolve(name: &str, search_dirs: &[PathBuf]) -> Self {
        let resolved = search_dirs
            .iter()
            .find_map(|base| {
                let dir = base.join(name);
                native(&dir).or_else(|| legacy(&dir, name))
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

/// Whether `name` is safe as a plugin or theme name: ASCII letters,
/// digits, `.`, `_` and `-` only, and no `..`.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains("..")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
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
    fn validates_names() {
        for name in ["git", "colored-man-pages", "zsh_z", "v1.2"] {
            assert!(valid_name(name), "{name}");
        }
        for name in ["", "..", "a..b", "a/b", "$x", "a b", "é"] {
            assert!(!valid_name(name), "{name}");
        }
    }

    #[test]
    fn reports_missing_plugin() {
        let root = tempfile::tempdir().unwrap();
        let plugin = Plugin::resolve("nope", &[root.path().to_owned()]);
        assert_eq!(plugin.resolved, Resolved::NotFound);
    }
}
