//! Reading and editing `config.toml`.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer};
use toml_edit::{Array, DocumentMut, Item, RawString, Value};

use crate::files::write_atomic;
use crate::plugin::{ValidName, check_name};

/// The user configuration.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Plugins to load, in order; every name passes [`check_name`].
    #[serde(deserialize_with = "plugin_names")]
    pub plugins: Vec<String>,
    pub prompt: Prompt,
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

fn plugin_names<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    let names = Vec::<ValidName>::deserialize(deserializer)?;
    Ok(names.into_iter().map(|name| name.0).collect())
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

/// Appends `name` to `plugins` in `path`, creating the file if needed.
/// Returns `false` when the plugin was already listed.
pub fn add_plugin(path: &Path, name: &str) -> Result<bool> {
    edit(path, |text| with_plugin_added(text, name))
}

/// Removes `name` from `plugins` in `path`. Returns `false` when it was not
/// listed.
pub fn remove_plugin(path: &Path, name: &str) -> Result<bool> {
    edit(path, |text| with_plugin_removed(text, name))
}

fn edit(path: &Path, change: impl FnOnce(&str) -> Result<Option<String>>) -> Result<bool> {
    let loaded = Config::load(path)?;
    let Some(text) = change(loaded.source.as_deref().unwrap_or(""))? else {
        return Ok(false);
    };
    // Never write a file that zsh-tool itself would refuse to load.
    Config::parse(&text).context("internal error: edited configuration is invalid")?;
    write_atomic(path, &text)?;
    Ok(true)
}

/// Returns `text` with `name` appended to `plugins`, keeping comments and
/// layout, or `None` when it is already listed.
pub fn with_plugin_added(text: &str, name: &str) -> Result<Option<String>> {
    check_name(name)?;
    let mut doc = document(text)?;
    let plugins = plugins_array(&mut doc)?;
    if plugins.iter().any(|value| value.as_str() == Some(name)) {
        return Ok(None);
    }
    let mut value = Value::from(name);
    if let Some(last) = plugins.iter_mut().last() {
        // Same line as the previous entry, or same indentation on a new line.
        let prefix = raw(last.decor().prefix());
        let indent = prefix
            .rfind('\n')
            .map_or(" ", |at| &prefix[at..])
            .to_owned();
        // Whatever followed the last entry (spaces, a comment) stays with
        // it; only the final line break remains before `]`.
        let after = raw(last.decor().suffix());
        last.decor_mut().set_suffix("");
        let after = after + &raw(Some(plugins.trailing()));
        let (kept, closing) = after.split_at(after.rfind('\n').unwrap_or(after.len()));
        value.decor_mut().set_prefix(format!("{kept}{indent}"));
        plugins.set_trailing(closing);
    }
    plugins.push_formatted(value);
    Ok(Some(doc.to_string()))
}

/// Returns `text` without `name` in `plugins`, keeping comments and layout,
/// or `None` when it is not listed.
pub fn with_plugin_removed(text: &str, name: &str) -> Result<Option<String>> {
    let mut doc = document(text)?;
    let plugins = plugins_array(&mut doc)?;
    let Some(index) = plugins
        .iter()
        .position(|value| value.as_str() == Some(name))
    else {
        return Ok(None);
    };
    let removed = plugins.remove(index);
    if index == 0
        && let Some(first) = plugins.get_mut(0)
    {
        *first.decor_mut() = removed.decor().clone();
    }
    Ok(Some(doc.to_string()))
}

fn raw(text: Option<&RawString>) -> String {
    text.and_then(RawString::as_str).unwrap_or("").to_owned()
}

fn document(text: &str) -> Result<DocumentMut> {
    Config::parse(text)?;
    Ok(text.parse()?)
}

fn plugins_array(doc: &mut DocumentMut) -> Result<&mut Array> {
    let item = doc
        .entry("plugins")
        .or_insert_with(|| Item::Value(Value::Array(Array::new())));
    item.as_array_mut()
        .context("`plugins` must be an array of strings")
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
            "#,
        )
        .unwrap();
        assert_eq!(config.plugins, ["git", "sudo"]);
        assert_eq!(config.prompt.engine, Engine::Legacy);
        assert_eq!(config.prompt.theme.as_deref(), Some("robbyrussell"));
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
    fn rejects_invalid_plugin_names_with_line() {
        for name in ["../etc", "a/b", "", "-x", ".x", "a b"] {
            let text = format!("plugins = [\n  \"git\",\n  {name:?},\n]\n");
            let err = format!("{:#}", Config::parse(&text).unwrap_err());
            assert!(
                err.contains("invalid plugin name") && err.contains("line 3"),
                "{name:?}: {err}"
            );
        }
    }

    #[test]
    fn adds_plugin_keeping_comments() {
        let text = "# my config\nplugins = [\"git\"] # main\n\n[prompt]\nengine = \"none\"\n";
        let edited = with_plugin_added(text, "sudo").unwrap().unwrap();
        assert_eq!(
            edited,
            "# my config\nplugins = [\"git\", \"sudo\"] # main\n\n[prompt]\nengine = \"none\"\n"
        );
        assert_eq!(with_plugin_added(&edited, "sudo").unwrap(), None);
    }

    #[test]
    fn adds_plugin_to_multiline_array() {
        let text = "plugins = [\n    \"git\", # vcs\n    \"sudo\",\n]\n";
        let edited = with_plugin_added(text, "z").unwrap().unwrap();
        assert_eq!(
            edited,
            "plugins = [\n    \"git\", # vcs\n    \"sudo\",\n    \"z\",\n]\n"
        );
    }

    #[test]
    fn adds_plugin_after_trailing_comment() {
        let text = "plugins = [\n  \"git\", # vcs\n]\n";
        assert_eq!(
            with_plugin_added(text, "z").unwrap().unwrap(),
            "plugins = [\n  \"git\", # vcs\n  \"z\",\n]\n"
        );
        let text = "plugins = [\n  \"git\" # vcs\n]\n";
        let edited = with_plugin_added(text, "z").unwrap().unwrap();
        assert_eq!(edited, "plugins = [\n  \"git\", # vcs\n  \"z\"\n]\n");
        assert_eq!(
            with_plugin_added("plugins = []\n", "z").unwrap().unwrap(),
            "plugins = [\"z\"]\n"
        );
    }

    #[test]
    fn adds_plugins_key_when_missing() {
        let edited = with_plugin_added("", "git").unwrap().unwrap();
        assert_eq!(Config::parse(&edited).unwrap().plugins, ["git"]);
        let edited = with_plugin_added("[prompt]\nengine = \"starship\"\n", "git")
            .unwrap()
            .unwrap();
        let config = Config::parse(&edited).unwrap();
        assert_eq!(config.plugins, ["git"]);
        assert_eq!(config.prompt.engine, Engine::Starship);
    }

    #[test]
    fn add_refuses_invalid_input() {
        assert!(with_plugin_added("", "../x").is_err());
        let err = with_plugin_added("plugins = [\n", "git").unwrap_err();
        assert!(format!("{err}").contains("line"), "{err}");
    }

    #[test]
    fn removes_plugin_keeping_layout() {
        let text = "plugins = [\"git\", \"sudo\", \"z\"] # list\n";
        assert_eq!(
            with_plugin_removed(text, "git").unwrap().unwrap(),
            "plugins = [\"sudo\", \"z\"] # list\n"
        );
        assert_eq!(
            with_plugin_removed(text, "sudo").unwrap().unwrap(),
            "plugins = [\"git\", \"z\"] # list\n"
        );
        assert_eq!(with_plugin_removed(text, "nope").unwrap(), None);
        assert_eq!(with_plugin_removed("", "git").unwrap(), None);
    }

    #[test]
    fn edits_file_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub/config.toml");
        assert!(add_plugin(&path, "git").unwrap());
        assert!(!add_plugin(&path, "git").unwrap());
        assert!(add_plugin(&path, "sudo").unwrap());
        assert!(remove_plugin(&path, "git").unwrap());
        assert!(!remove_plugin(&path, "git").unwrap());
        assert_eq!(Config::load(&path).unwrap().config.plugins, ["sudo"]);
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
