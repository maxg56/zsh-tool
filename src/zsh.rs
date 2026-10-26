//! Helpers for writing zsh source code.

use std::path::Path;

use anyhow::{Result, anyhow};

/// Quotes `value` as a single-quoted zsh word.
///
/// Inside single quotes zsh interprets nothing, so the only character to
/// handle is the quote itself, written as `'\''`.
pub fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Quotes a path, refusing paths that are not valid UTF-8 rather than
/// silently altering them.
pub fn quote_path(path: &Path) -> Result<String> {
    path.to_str()
        .map(quote)
        .ok_or_else(|| anyhow!("path is not valid UTF-8: {}", path.display()))
}

/// Keeps arbitrary text on a single comment line.
pub fn comment(text: &str) -> String {
    text.replace(['\n', '\r'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_plain_value() {
        assert_eq!(quote("git status"), "'git status'");
    }

    #[test]
    fn quotes_empty_value() {
        assert_eq!(quote(""), "''");
    }

    #[test]
    fn escapes_single_quotes() {
        assert_eq!(quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn comment_stays_on_one_line() {
        assert_eq!(comment("a\nrm -rf ~\r"), "a rm -rf ~ ");
    }

    #[test]
    fn leaves_shell_metacharacters_inert() {
        assert_eq!(quote("$(rm -rf ~); `x` \"y\""), "'$(rm -rf ~); `x` \"y\"'");
    }
}
