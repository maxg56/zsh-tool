//! What `zsh-tool plugin add` was given: a plugin name or a Git source.

use anyhow::{Result, bail};

use crate::plugin::check_name;

/// A parsed `plugin add` argument.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Spec {
    /// A plugin name: already installed, or looked up in the registry.
    Name(String),
    /// A Git repository, cloned as plugin `name`.
    Git { url: String, name: String },
}

/// Parses a `plugin add` argument; `name` overrides the plugin name
/// derived from a Git source.
///
/// Accepted sources are `owner/repo` (GitHub), `https://` URLs and, as an
/// explicit local choice, absolute paths to a repository.
pub fn parse(arg: &str, name: Option<&str>) -> Result<Spec> {
    let url = if arg.contains("://") || arg.starts_with('/') || arg.contains(':') {
        check_url(arg)?;
        arg.to_owned()
    } else if let Some((owner, repo)) = arg.split_once('/') {
        if check_name(owner).is_err() || check_name(repo).is_err() || repo.contains('/') {
            bail!("invalid GitHub repository {arg:?}: expected owner/repo");
        }
        format!("https://github.com/{owner}/{repo}.git")
    } else {
        check_name(arg)?;
        if name.is_some_and(|name| name != arg) {
            bail!("--name only applies to Git sources");
        }
        return Ok(Spec::Name(arg.to_owned()));
    };
    let name = match name {
        Some(name) => name.to_owned(),
        None => name_from_url(&url),
    };
    check_name(&name).map_err(|err| anyhow::anyhow!("{err}; choose another one with --name"))?;
    Ok(Spec::Git { url, name })
}

/// Accepts `https://host/...` URLs and absolute local paths only, so that
/// plugins never travel over an unauthenticated transport.
pub fn check_url(url: &str) -> Result<()> {
    if url.chars().any(|c| c.is_control() || c.is_whitespace()) {
        bail!("invalid Git source {url:?}: spaces and control characters are not allowed");
    }
    if let Some(rest) = url.strip_prefix("https://") {
        let host = rest.split(['/', '?', '#']).next().unwrap_or("");
        if host.is_empty() || host.contains('@') {
            bail!("invalid Git source {url:?}: expected https://host/path without credentials");
        }
        return Ok(());
    }
    if url.starts_with('/') && !url.split('/').any(|part| part == "..") {
        return Ok(());
    }
    bail!(
        "unsupported Git source {url:?}: only https:// URLs (or owner/repo for GitHub) are allowed"
    )
}

/// Last path segment of `url`, without a `.git` suffix.
fn name_from_url(url: &str) -> String {
    let path = match url.strip_prefix("https://") {
        Some(rest) => rest.split_once('/').map_or("", |(_host, path)| path),
        None => url,
    };
    let path = path.split(['?', '#']).next().unwrap_or(path);
    let last = path.trim_end_matches('/').rsplit('/').next().unwrap_or("");
    last.strip_suffix(".git").unwrap_or(last).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(url: &str, name: &str) -> Spec {
        Spec::Git {
            url: url.to_owned(),
            name: name.to_owned(),
        }
    }

    #[test]
    fn parses_names() {
        assert_eq!(parse("git", None).unwrap(), Spec::Name("git".to_owned()));
        assert!(parse("..", None).is_err());
        assert!(parse("-x", None).is_err());
        assert!(parse("git", Some("other")).is_err());
    }

    #[test]
    fn expands_github_shorthand() {
        assert_eq!(
            parse("zsh-users/zsh-autosuggestions", None).unwrap(),
            git(
                "https://github.com/zsh-users/zsh-autosuggestions.git",
                "zsh-autosuggestions"
            )
        );
        for bad in ["a/b/c", "../b", "a/..", "a/", "a b/c", "-a/b"] {
            assert!(parse(bad, None).is_err(), "{bad}");
        }
    }

    #[test]
    fn accepts_https_urls() {
        assert_eq!(
            parse("https://example.com/me/plug.git/", None).unwrap(),
            git("https://example.com/me/plug.git/", "plug")
        );
        assert_eq!(
            parse("https://example.com/me/plug", Some("other")).unwrap(),
            git("https://example.com/me/plug", "other")
        );
    }

    #[test]
    fn refuses_insecure_transports() {
        for bad in [
            "http://example.com/a.git",
            "git://example.com/a.git",
            "ssh://git@example.com/a.git",
            "git@github.com:a/b.git",
            "file:///tmp/a",
            "https://user:pw@example.com/a.git",
            "https:///a.git",
            "https://example.com/a b",
            "/tmp/../etc",
        ] {
            assert!(parse(bad, None).is_err(), "{bad}");
        }
    }

    #[test]
    fn accepts_absolute_paths() {
        assert_eq!(
            parse("/srv/repos/my-plugin.git", None).unwrap(),
            git("/srv/repos/my-plugin.git", "my-plugin")
        );
    }

    #[test]
    fn validates_derived_names() {
        let err = parse("https://example.com/", None).unwrap_err().to_string();
        assert!(err.contains("--name"), "{err}");
        assert!(parse("https://example.com/a", Some("../x")).is_err());
        assert!(parse("https://example.com/.git", None).is_err());
    }
}
