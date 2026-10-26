//! Native plugin manifests (`plugin.toml`).

use std::fmt;
use std::fs;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use serde::de::{Deserializer, MapAccess, Visitor};
use toml::Spanned;

/// File name that marks a directory as a native plugin.
pub const FILE: &str = "plugin.toml";

/// Descriptive fields of a manifest, as shown to users.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    pub name: String,
    pub version: Option<String>,
    pub description: Option<String>,
}

/// A validated `plugin.toml`, with paths made absolute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub metadata: Metadata,
    /// Fragments to source, in order.
    pub init: Vec<PathBuf>,
    /// Directories prepended to `fpath`, in order.
    pub fpath: Vec<PathBuf>,
    /// `[aliases]`, in declaration order.
    pub aliases: Vec<(String, String)>,
    /// `[env]`, in declaration order.
    pub env: Vec<(String, String)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    plugin: RawPlugin,
    #[serde(default, deserialize_with = "ordered")]
    aliases: Vec<(Spanned<String>, String)>,
    #[serde(default, deserialize_with = "ordered")]
    env: Vec<(Spanned<String>, String)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPlugin {
    name: Spanned<String>,
    version: Option<String>,
    description: Option<String>,
    #[serde(default)]
    init: Vec<Spanned<String>>,
    #[serde(default)]
    fpath: Vec<Spanned<String>>,
}

/// A validation failure, located by its byte span in the manifest.
struct Invalid {
    span: Option<Range<usize>>,
    message: String,
}

impl Invalid {
    fn at<T>(spanned: &Spanned<T>, message: String) -> Self {
        Self {
            span: Some(spanned.span()),
            message,
        }
    }
}

impl Manifest {
    /// Reads and validates `<dir>/plugin.toml`; errors name the file and,
    /// when known, the line.
    pub fn load(dir: &Path) -> Result<Self> {
        let path = dir.join(FILE);
        let text =
            fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        Self::parse(&text, dir).map_err(|invalid| {
            let location = match invalid.span {
                Some(span) => format!("{}, line {}", path.display(), line_of(&text, span.start)),
                None => path.display().to_string(),
            };
            anyhow!("{location}: {}", invalid.message)
        })
    }

    fn parse(text: &str, dir: &Path) -> Result<Self, Invalid> {
        let raw: Raw = toml::from_str(text).map_err(|err| Invalid {
            span: err.span(),
            message: err.message().to_owned(),
        })?;
        let plugin = raw.plugin;
        check_name(&plugin.name, dir)?;
        let init = plugin
            .init
            .iter()
            .map(|entry| relative(entry, dir, "init file", Path::is_file))
            .collect::<Result<_, _>>()?;
        let fpath = plugin
            .fpath
            .iter()
            .map(|entry| relative(entry, dir, "fpath directory", Path::is_dir))
            .collect::<Result<_, _>>()?;
        let aliases = checked(raw.aliases, valid_alias, "alias name")?;
        let env = checked(raw.env, valid_variable, "variable name")?;
        Ok(Self {
            metadata: Metadata {
                name: plugin.name.into_inner(),
                version: plugin.version,
                description: plugin.description,
            },
            init,
            fpath,
            aliases,
            env,
        })
    }
}

/// 1-based line of the byte at `offset`.
fn line_of(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset.min(text.len())]
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count()
        + 1
}

fn check_name(name: &Spanned<String>, dir: &Path) -> Result<(), Invalid> {
    let value = name.get_ref();
    if value.is_empty()
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
    {
        return Err(Invalid::at(
            name,
            format!("invalid plugin name `{value}`: use only letters, digits, `.`, `_` and `-`"),
        ));
    }
    let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if value != dir_name {
        return Err(Invalid::at(
            name,
            format!("plugin name `{value}` does not match its directory `{dir_name}`"),
        ));
    }
    Ok(())
}

/// Resolves a manifest path inside `dir`, which must stay inside it and
/// pass `exists`.
fn relative(
    entry: &Spanned<String>,
    dir: &Path,
    what: &str,
    exists: fn(&Path) -> bool,
) -> Result<PathBuf, Invalid> {
    let value = entry.get_ref();
    let path = Path::new(value);
    let inside = !value.is_empty()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir));
    if !inside {
        return Err(Invalid::at(
            entry,
            format!("{what} `{value}` must be a relative path without `..`"),
        ));
    }
    let full = dir.join(path);
    if !exists(&full) {
        return Err(Invalid::at(
            entry,
            format!("{what} `{value}` does not exist"),
        ));
    }
    Ok(full)
}

fn checked(
    entries: Vec<(Spanned<String>, String)>,
    valid: fn(&str) -> bool,
    what: &str,
) -> Result<Vec<(String, String)>, Invalid> {
    entries
        .into_iter()
        .map(|(key, value)| {
            if valid(key.get_ref()) {
                Ok((key.into_inner(), value))
            } else {
                Err(Invalid::at(
                    &key,
                    format!("invalid {what} `{}`", key.get_ref()),
                ))
            }
        })
        .collect()
}

/// An alias name can be anything zsh accepts on the left of `=`, minus
/// whitespace and quotes.
fn valid_alias(name: &str) -> bool {
    !name.is_empty()
        && !name
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || matches!(c, '=' | '\'' | '"'))
}

/// `[A-Za-z_][A-Za-z0-9_]*`.
fn valid_variable(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Deserializes a table of strings, keeping the order of the file.
fn ordered<'de, D>(deserializer: D) -> Result<Vec<(Spanned<String>, String)>, D::Error>
where
    D: Deserializer<'de>,
{
    struct Entries;

    impl<'de> Visitor<'de> for Entries {
        type Value = Vec<(Spanned<String>, String)>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a table of strings")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut entries = Vec::new();
            while let Some(entry) = map.next_entry()? {
                entries.push(entry);
            }
            Ok(entries)
        }
    }

    deserializer.deserialize_map(Entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Plugin {
        _root: tempfile::TempDir,
        dir: PathBuf,
    }

    /// A plugin directory named `demo` holding `files` (empty) and `dirs`.
    fn plugin(files: &[&str], dirs: &[&str]) -> Plugin {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("demo");
        fs::create_dir(&dir).unwrap();
        for sub in dirs {
            fs::create_dir_all(dir.join(sub)).unwrap();
        }
        for file in files {
            fs::write(dir.join(file), "").unwrap();
        }
        Plugin { _root: root, dir }
    }

    fn load(plugin: &Plugin, text: &str) -> Result<Manifest> {
        fs::write(plugin.dir.join(FILE), text).unwrap();
        Manifest::load(&plugin.dir)
    }

    fn error(plugin: &Plugin, text: &str) -> String {
        format!("{:#}", load(plugin, text).unwrap_err())
    }

    #[test]
    fn parses_full_manifest() {
        let fx = plugin(&["a.zsh", "b.zsh"], &["completions"]);
        let manifest = load(
            &fx,
            r#"
            [plugin]
            name = "demo"
            version = "1.2.3"
            description = "Demo plugin"
            init = ["b.zsh", "./a.zsh"]
            fpath = ["completions"]

            [aliases]
            zz = "echo z"
            aa = "echo 'a'"
            "gca!" = "git commit --amend"

            [env]
            DEMO_B = "2"
            DEMO_A = "1"
            "#,
        )
        .unwrap();
        assert_eq!(
            manifest.metadata,
            Metadata {
                name: "demo".into(),
                version: Some("1.2.3".into()),
                description: Some("Demo plugin".into()),
            }
        );
        assert_eq!(
            manifest.init,
            [fx.dir.join("b.zsh"), fx.dir.join("./a.zsh")]
        );
        assert_eq!(manifest.fpath, [fx.dir.join("completions")]);
        let names: Vec<_> = manifest.aliases.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(names, ["zz", "aa", "gca!"]);
        assert_eq!(manifest.aliases[1].1, "echo 'a'");
        assert_eq!(
            manifest.env,
            [("DEMO_B".into(), "2".into()), ("DEMO_A".into(), "1".into())]
        );
    }

    #[test]
    fn only_name_is_required() {
        let fx = plugin(&[], &[]);
        let manifest = load(&fx, "[plugin]\nname = \"demo\"\n").unwrap();
        assert_eq!(manifest.metadata.version, None);
        assert!(manifest.init.is_empty() && manifest.aliases.is_empty());
    }

    #[test]
    fn error_names_file_and_line() {
        let fx = plugin(&[], &[]);
        let err = error(&fx, "[plugin]\nname = \"demo\"\nhomepage = \"x\"\n");
        assert!(
            err.contains(&fx.dir.join(FILE).display().to_string()),
            "{err}"
        );
        assert!(
            err.contains("line 3") && err.contains("unknown field `homepage`"),
            "{err}"
        );
    }

    #[test]
    fn reports_syntax_error_line() {
        let fx = plugin(&[], &[]);
        let err = error(&fx, "[plugin]\nname = \"demo\n");
        assert!(err.contains("line 2"), "{err}");
    }

    #[test]
    fn rejects_unknown_table() {
        let fx = plugin(&[], &[]);
        let err = error(&fx, "[plugin]\nname = \"demo\"\n[functions]\n");
        assert!(err.contains("unknown field `functions`"), "{err}");
    }

    #[test]
    fn requires_plugin_table() {
        let fx = plugin(&[], &[]);
        assert!(error(&fx, "[aliases]\n").contains("missing field `plugin`"));
    }

    #[test]
    fn name_must_match_directory() {
        let fx = plugin(&[], &[]);
        let err = error(&fx, "[plugin]\nname = \"other\"\n");
        assert!(
            err.contains("line 2: plugin name `other` does not match its directory `demo`"),
            "{err}"
        );
    }

    #[test]
    fn rejects_invalid_name_characters() {
        let fx = plugin(&[], &[]);
        for name in ["", "de mo", "de/mo", "dé"] {
            let err = error(&fx, &format!("[plugin]\nname = \"{name}\"\n"));
            assert!(err.contains("invalid plugin name"), "{name}: {err}");
        }
    }

    #[test]
    fn rejects_paths_leaving_the_plugin() {
        let fx = plugin(&["a.zsh"], &[]);
        for path in ["../a.zsh", "/etc/passwd", "sub/../a.zsh", ""] {
            let err = error(
                &fx,
                &format!("[plugin]\nname = \"demo\"\ninit = [\"{path}\"]\n"),
            );
            assert!(
                err.contains("line 3") && err.contains("must be a relative path"),
                "{path}: {err}"
            );
        }
    }

    #[test]
    fn rejects_missing_paths() {
        let fx = plugin(&["file"], &["dir"]);
        let err = error(&fx, "[plugin]\nname = \"demo\"\ninit = [\"nope.zsh\"]\n");
        assert!(err.contains("init file `nope.zsh` does not exist"), "{err}");
        let err = error(&fx, "[plugin]\nname = \"demo\"\ninit = [\"dir\"]\n");
        assert!(err.contains("init file `dir` does not exist"), "{err}");
        let err = error(&fx, "[plugin]\nname = \"demo\"\nfpath = [\"file\"]\n");
        assert!(
            err.contains("fpath directory `file` does not exist"),
            "{err}"
        );
    }

    #[test]
    fn rejects_invalid_alias_names() {
        let fx = plugin(&[], &[]);
        for name in ["", "a b", "a=b", "a'b", "a\\\"b", "a\\tb"] {
            let err = error(
                &fx,
                &format!("[plugin]\nname = \"demo\"\n[aliases]\nok = \"x\"\n\"{name}\" = \"x\"\n"),
            );
            assert!(
                err.contains("line 5") && err.contains("invalid alias name"),
                "{name}: {err}"
            );
        }
    }

    #[test]
    fn rejects_invalid_variable_names() {
        let fx = plugin(&[], &[]);
        for name in ["1A", "A-B", "A B", "", "É"] {
            let err = error(
                &fx,
                &format!("[plugin]\nname = \"demo\"\n[env]\n\"{name}\" = \"x\"\n"),
            );
            assert!(
                err.contains("line 4") && err.contains("invalid variable name"),
                "{name}: {err}"
            );
        }
        assert!(valid_variable("_A1") && valid_variable("a"));
    }

    #[test]
    fn values_must_be_strings() {
        let fx = plugin(&[], &[]);
        let err = error(&fx, "[plugin]\nname = \"demo\"\n[env]\nDEBUG = 0\n");
        assert!(err.contains("line 4"), "{err}");
        let err = error(&fx, "[plugin]\nname = \"demo\"\n[aliases]\ng = [\"git\"]\n");
        assert!(err.contains("line 4"), "{err}");
    }

    #[test]
    fn missing_manifest_is_an_error() {
        let fx = plugin(&[], &[]);
        let err = format!("{:#}", Manifest::load(&fx.dir).unwrap_err());
        assert!(err.contains("cannot read"), "{err}");
    }

    #[test]
    fn counts_lines() {
        assert_eq!(line_of("a\nb\nc", 0), 1);
        assert_eq!(line_of("a\nb\nc", 2), 2);
        assert_eq!(line_of("a\nb\nc", 99), 3);
    }
}
