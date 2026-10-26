//! Reading `config.toml`.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;

/// The user configuration.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Plugins to load, in order.
    pub plugins: Vec<String>,
    pub prompt: Prompt,
    pub compat: Compat,
}

/// The `[compat]` table: Oh My Zsh compatibility switches.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Compat {
    /// Source every `$ZSH/lib/*.zsh` file before the plugins.
    pub lib: bool,
}

/// The `[prompt]` table.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Prompt {
    pub engine: Engine,
    /// Theme name, used by the `legacy` engine.
    pub theme: Option<String>,
}

/// How the prompt is set up.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    /// zsh-tool leaves the prompt alone.
    #[default]
    None,
    /// The prompt is delegated to Starship.
    Starship,
    /// An Oh My Zsh `*.zsh-theme` file is loaded.
    Legacy,
}

/// A configuration together with the raw bytes it was parsed from.
#[derive(Debug, Clone, Default)]
pub struct Loaded {
    pub config: Config,
    /// File contents, or `None` when the file does not exist.
    pub source: Option<String>,
}

impl Config {
    /// Parses a configuration; errors include the line and column.
    pub fn parse(text: &str) -> Result<Self> {
        Ok(toml::from_str(text)?)
    }

    /// Loads `path`, falling back to the default configuration when the
    /// file does not exist.
    pub fn load(path: &Path) -> Result<Loaded> {
        let source = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Loaded::default()),
            Err(err) => return Err(err).with_context(|| format!("cannot read {}", path.display())),
        };
        let config = Self::parse(&source)
            .with_context(|| format!("invalid configuration {}", path.display()))?;
        Ok(Loaded {
            config,
            source: Some(source),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_configuration() {
        let config = Config::parse(
            r#"
            plugins = ["git", "sudo"]

            [prompt]
            engine = "legacy"
            theme = "robbyrussell"

            [compat]
            lib = true
            "#,
        )
        .unwrap();
        assert_eq!(config.plugins, ["git", "sudo"]);
        assert_eq!(config.prompt.engine, Engine::Legacy);
        assert_eq!(config.prompt.theme.as_deref(), Some("robbyrussell"));
        assert!(config.compat.lib);
    }

    #[test]
    fn compat_lib_is_off_by_default() {
        assert!(!Config::parse("plugins = []\n").unwrap().compat.lib);
    }

    #[test]
    fn rejects_non_boolean_compat_lib() {
        let err = Config::parse("[compat]\nlib = \"yes\"\n").unwrap_err();
        assert!(format!("{err}").contains("line 2"), "{err}");
    }

    #[test]
    fn empty_file_is_default() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn reports_line_of_syntax_error() {
        let err = Config::parse("plugins = [\"git\"]\n[prompt\n").unwrap_err();
        assert!(format!("{err}").contains("line 2"), "{err}");
    }

    #[test]
    fn rejects_unknown_keys() {
        let err = Config::parse("plugin = [\"git\"]\n").unwrap_err();
        assert!(format!("{err}").contains("unknown field `plugin`"), "{err}");
    }

    #[test]
    fn rejects_unknown_engine() {
        let err = Config::parse("[prompt]\nengine = \"p10k\"\n").unwrap_err();
        assert!(format!("{err}").contains("line 2"), "{err}");
    }

    #[test]
    fn missing_file_is_default() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = Config::load(&dir.path().join("config.toml")).unwrap();
        assert_eq!(loaded.config, Config::default());
        assert_eq!(loaded.source, None);
    }

    #[test]
    fn load_names_the_file_on_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "plugins = 3\n").unwrap();
        let err = Config::load(&path).unwrap_err();
        let message = format!("{err:#}");
        assert!(message.contains("config.toml"), "{message}");
        assert!(message.contains("line 1"), "{message}");
    }
}
