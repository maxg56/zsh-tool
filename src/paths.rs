//! Locations used by zsh-tool, derived from the environment.

use std::path::PathBuf;

use anyhow::{Result, bail};

/// Every directory and file zsh-tool reads or writes.
#[derive(Debug, Clone)]
pub struct Paths {
    /// `config.toml`, overridden by `ZSH_TOOL_CONFIG`.
    pub config_file: PathBuf,
    /// Plugins and themes managed by zsh-tool, overridden by `ZSH_TOOL_HOME`.
    pub data_dir: PathBuf,
    /// Generated init script and completion dump.
    pub cache_dir: PathBuf,
    /// Oh My Zsh installation (`ZSH`, or `~/.oh-my-zsh`), when it exists.
    pub omz_dir: Option<PathBuf>,
    /// Oh My Zsh custom directory (`ZSH_CUSTOM`).
    pub omz_custom: PathBuf,
    /// Cache directory handed to Oh My Zsh plugins (`ZSH_CACHE_DIR`).
    pub omz_cache: PathBuf,
}

impl Paths {
    /// Reads the locations from the process environment.
    pub fn from_env() -> Result<Self> {
        Self::resolve(|name| std::env::var_os(name).map(PathBuf::from))
    }

    /// Computes the locations from an environment lookup function.
    pub fn resolve(env: impl Fn(&str) -> Option<PathBuf>) -> Result<Self> {
        let env = |name: &str| env(name).filter(|value| !value.as_os_str().is_empty());
        let Some(home) = env("HOME") else {
            bail!("HOME is not set");
        };
        let xdg = |name: &str, default: &str| env(name).unwrap_or_else(|| home.join(default));

        let config_file = env("ZSH_TOOL_CONFIG")
            .unwrap_or_else(|| xdg("XDG_CONFIG_HOME", ".config").join("zsh-tool/config.toml"));
        let data_dir = env("ZSH_TOOL_HOME")
            .unwrap_or_else(|| xdg("XDG_DATA_HOME", ".local/share").join("zsh-tool"));
        let cache_dir = xdg("XDG_CACHE_HOME", ".cache").join("zsh-tool");

        let omz_dir =
            Some(env("ZSH").unwrap_or_else(|| home.join(".oh-my-zsh"))).filter(|dir| dir.is_dir());
        let omz_custom = env("ZSH_CUSTOM").unwrap_or_else(|| match &omz_dir {
            Some(omz) => omz.join("custom"),
            None => data_dir.join("custom"),
        });
        let omz_cache = env("ZSH_CACHE_DIR").unwrap_or_else(|| cache_dir.join("omz"));

        Ok(Self {
            config_file,
            data_dir,
            cache_dir,
            omz_dir,
            omz_custom,
            omz_cache,
        })
    }

    /// Directories searched for a plugin, in resolution order.
    pub fn plugin_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = vec![
            self.data_dir.join("plugins"),
            self.omz_custom.join("plugins"),
        ];
        if let Some(omz) = &self.omz_dir {
            dirs.push(omz.join("plugins"));
        }
        dirs
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use super::*;

    fn resolve(vars: &[(&str, &Path)]) -> Paths {
        let vars: HashMap<_, _> = vars.iter().map(|(k, v)| (*k, v.to_path_buf())).collect();
        Paths::resolve(|name| vars.get(name).cloned()).unwrap()
    }

    #[test]
    fn uses_home_defaults() {
        let paths = resolve(&[("HOME", Path::new("/home/u"))]);
        assert_eq!(
            paths.config_file,
            Path::new("/home/u/.config/zsh-tool/config.toml")
        );
        assert_eq!(paths.data_dir, Path::new("/home/u/.local/share/zsh-tool"));
        assert_eq!(paths.cache_dir, Path::new("/home/u/.cache/zsh-tool"));
        assert_eq!(paths.omz_dir, None);
        assert_eq!(
            paths.omz_custom,
            Path::new("/home/u/.local/share/zsh-tool/custom")
        );
        assert_eq!(paths.omz_cache, Path::new("/home/u/.cache/zsh-tool/omz"));
    }

    #[test]
    fn honours_overrides() {
        let paths = resolve(&[
            ("HOME", Path::new("/home/u")),
            ("ZSH_TOOL_CONFIG", Path::new("/etc/zt.toml")),
            ("ZSH_TOOL_HOME", Path::new("/opt/zt")),
            ("XDG_CACHE_HOME", Path::new("/var/cache")),
            ("ZSH_CUSTOM", Path::new("/custom")),
        ]);
        assert_eq!(paths.config_file, Path::new("/etc/zt.toml"));
        assert_eq!(paths.data_dir, Path::new("/opt/zt"));
        assert_eq!(paths.cache_dir, Path::new("/var/cache/zsh-tool"));
        assert_eq!(paths.omz_custom, Path::new("/custom"));
    }

    #[test]
    fn detects_existing_oh_my_zsh() {
        let home = tempfile::tempdir().unwrap();
        let omz = home.path().join(".oh-my-zsh");
        std::fs::create_dir(&omz).unwrap();
        let paths = resolve(&[("HOME", home.path())]);
        assert_eq!(paths.omz_dir.as_deref(), Some(omz.as_path()));
        assert_eq!(paths.omz_custom, omz.join("custom"));
        assert_eq!(
            paths.plugin_dirs(),
            vec![
                home.path().join(".local/share/zsh-tool/plugins"),
                omz.join("custom/plugins"),
                omz.join("plugins"),
            ]
        );
    }

    #[test]
    fn ignores_empty_variables() {
        let paths = resolve(&[
            ("HOME", Path::new("/home/u")),
            ("ZSH_TOOL_HOME", Path::new("")),
        ]);
        assert_eq!(paths.data_dir, Path::new("/home/u/.local/share/zsh-tool"));
    }

    #[test]
    fn requires_home() {
        assert!(Paths::resolve(|_| None).is_err());
    }
}
