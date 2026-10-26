//! Plugin resolution.

use std::path::{Path, PathBuf};

/// Where a plugin name led.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
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
    /// Looks `name` up in `search_dirs`, first match wins.
    pub fn resolve(name: &str, search_dirs: &[PathBuf]) -> Self {
        let resolved = search_dirs
            .iter()
            .find_map(|base| legacy(&base.join(name), name))
            .unwrap_or(Resolved::NotFound);
        Self {
            name: name.to_owned(),
            resolved,
        }
    }
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
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "").unwrap();
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
    fn reports_missing_plugin() {
        let root = tempfile::tempdir().unwrap();
        let plugin = Plugin::resolve("nope", &[root.path().to_owned()]);
        assert_eq!(plugin.resolved, Resolved::NotFound);
    }
}
