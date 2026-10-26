//! `zsh-tool migrate`: turns an Oh My Zsh `.zshrc` into a zsh-tool
//! configuration and rewrites the `.zshrc` to use `zsh-tool init zsh`.

use std::collections::HashSet;
use std::fs;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};

use crate::config::Engine;
use crate::paths::Paths;
use crate::plugin::{Plugin, Resolved, valid_name};
use crate::prompt::{find_theme, theme_dirs};
use crate::zsh::{comment, quote};

/// The line that replaces `source $ZSH/oh-my-zsh.sh`.
pub const INIT_LINE: &str = "eval \"$(zsh-tool init zsh)\"";
/// Prefix of the lines the migration turns into comments.
const COMMENT_PREFIX: &str = "# zsh-tool: ";
/// Theme used in place of `ZSH_THEME="random"`.
const RANDOM_FALLBACK: &str = "robbyrussell";

/// Command-line options of `zsh-tool migrate`.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// `.zshrc` to migrate; `$ZDOTDIR/.zshrc` or `~/.zshrc` by default.
    pub zshrc: Option<PathBuf>,
    /// Print the result without writing anything.
    pub dry_run: bool,
    /// Overwrite an existing `config.toml`.
    pub force: bool,
}

/// Runs the migration; `env` looks environment variables up.
pub fn run(
    options: &Options,
    env: impl Fn(&str) -> Option<PathBuf>,
    out: &mut impl Write,
) -> Result<()> {
    let paths = Paths::resolve(&env)?;
    let zshrc = match &options.zshrc {
        Some(path) => path.clone(),
        None => default_zshrc(&env)?,
    };
    let text =
        fs::read_to_string(&zshrc).with_context(|| format!("cannot read {}", zshrc.display()))?;
    let migration = migrate(&text, &zshrc, &env)
        .with_context(|| format!("cannot migrate {}", zshrc.display()))?;
    for warning in &migration.warnings {
        eprintln!("zsh-tool: warning: {warning}");
    }
    let config_exists = paths.config_file.exists();

    if options.dry_run {
        writeln!(out, "# {}", paths.config_file.display())?;
        if config_exists && !options.force {
            writeln!(out, "# (exists: --force is needed to overwrite it)")?;
        }
        writeln!(out, "{}", migration.config)?;
        out.write_all(diff(&zshrc, &text, &migration.zshrc).as_bytes())?;
        return Ok(());
    }

    if config_exists && !options.force {
        bail!(
            "{} already exists; use --force to overwrite it",
            paths.config_file.display()
        );
    }
    let backup = backup(&zshrc, &text)?;
    writeln!(
        out,
        "zsh-tool: saved {} to {}",
        zshrc.display(),
        backup.display()
    )?;
    if let Some(dir) = paths.config_file.parent() {
        fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    }
    fs::write(&paths.config_file, &migration.config)
        .with_context(|| format!("cannot write {}", paths.config_file.display()))?;
    writeln!(out, "zsh-tool: wrote {}", paths.config_file.display())?;
    write_in_place(&zshrc, &migration.zshrc)?;
    writeln!(out, "zsh-tool: updated {}", zshrc.display())?;
    Ok(())
}

fn default_zshrc(env: impl Fn(&str) -> Option<PathBuf>) -> Result<PathBuf> {
    let non_empty = |name: &str| env(name).filter(|value| !value.as_os_str().is_empty());
    match non_empty("ZDOTDIR").or_else(|| non_empty("HOME")) {
        Some(dir) => Ok(dir.join(".zshrc")),
        None => bail!("HOME is not set"),
    }
}

/// Result of a migration, before anything is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Migration {
    /// Contents of the new `config.toml`.
    pub config: String,
    /// The rewritten `.zshrc`.
    pub zshrc: String,
    pub warnings: Vec<String>,
}

/// Converts the `.zshrc` `text` read from `zshrc`.
pub fn migrate(
    text: &str,
    zshrc: &Path,
    env: impl Fn(&str) -> Option<PathBuf>,
) -> Result<Migration> {
    let parsed = parse(text, &env)?;
    if let Some(line) = parsed.initialised {
        bail!("line {}: already runs zsh-tool init zsh", line + 1);
    }
    if parsed.source_lines.is_empty() {
        bail!(
            "no line sourcing oh-my-zsh.sh (such as `source $ZSH/oh-my-zsh.sh`) was found; \
             replace it by hand with {INIT_LINE}"
        );
    }
    let mut warnings = parsed.warnings.clone();

    let paths = Paths::resolve(|name| match name {
        "ZSH" => parsed.zsh.value().map(PathBuf::from).or_else(|| env(name)),
        "ZSH_CUSTOM" => parsed
            .zsh_custom
            .value()
            .map(PathBuf::from)
            .or_else(|| env(name)),
        _ => env(name),
    })?;
    let search_dirs = paths.plugin_dirs();
    for name in &parsed.plugins {
        if Plugin::resolve(name, &search_dirs).resolved == Resolved::NotFound {
            warnings.push(format!("plugin '{name}' not found"));
        }
    }

    let (engine, theme) = if parsed.starship {
        (Engine::Starship, None)
    } else {
        match &parsed.theme {
            None => (Engine::None, None),
            Some(None) => {
                warnings.push("cannot read the value of ZSH_THEME; no prompt configured".into());
                (Engine::None, None)
            }
            Some(Some(name)) if name.is_empty() => (Engine::None, None),
            Some(Some(name)) => legacy_theme(name, &paths, &mut warnings),
        }
    };

    Ok(Migration {
        config: config_toml(zshrc, &parsed.plugins, engine, theme.as_deref()),
        zshrc: rewrite(text, &parsed, engine),
        warnings,
    })
}

/// The legacy engine with theme `name`, if it is usable.
fn legacy_theme(name: &str, paths: &Paths, warnings: &mut Vec<String>) -> (Engine, Option<String>) {
    let name = if name == "random" {
        warnings.push(format!(
            "ZSH_THEME=\"random\" is not supported; using {RANDOM_FALLBACK}"
        ));
        RANDOM_FALLBACK
    } else {
        name
    };
    if !valid_name(name) {
        warnings.push(format!(
            "invalid theme name {}; no prompt configured",
            quote(name)
        ));
        return (Engine::None, None);
    }
    if find_theme(name, &theme_dirs(paths)).is_none() {
        warnings.push(format!("theme '{name}' not found"));
    }
    (Engine::Legacy, Some(name.to_owned()))
}

/// Assignments of one variable, in file order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Assignments(Vec<Assignment>);

#[derive(Debug, Clone, PartialEq, Eq)]
struct Assignment {
    /// 0-based line.
    line: usize,
    /// The value, `None` when it cannot be worked out statically.
    value: Option<String>,
    exported: bool,
}

impl Assignments {
    /// Value of the last assignment, which is the one that wins.
    fn value(&self) -> Option<&str> {
        self.0.last()?.value.as_deref()
    }
}

/// What the migration needs to know about a `.zshrc`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Parsed {
    plugins: Vec<String>,
    /// Every line of every `plugins=(...)` assignment.
    plugin_lines: HashSet<usize>,
    /// Last `ZSH_THEME` value: `Some(None)` when it cannot be read.
    theme: Option<Option<String>>,
    theme_lines: HashSet<usize>,
    zsh: Assignments,
    zsh_custom: Assignments,
    starship: bool,
    /// `eval "$(starship init zsh)"` lines.
    starship_lines: HashSet<usize>,
    /// Lines sourcing `oh-my-zsh.sh`.
    source_lines: HashSet<usize>,
    /// First line already running `zsh-tool init zsh`.
    initialised: Option<usize>,
    warnings: Vec<String>,
}

/// Reads the assignments and commands the migration cares about. Lines
/// are analysed one statement each, which is how `.zshrc` files written
/// from the Oh My Zsh template look.
fn parse(text: &str, env: &impl Fn(&str) -> Option<PathBuf>) -> Result<Parsed> {
    let mut parsed = Parsed::default();
    let starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(at, _)| at + 1))
        .filter(|&at| at < text.len())
        .collect();
    let line_of = |offset: usize| starts.partition_point(|&start| start <= offset) - 1;

    let mut next = 0;
    while next < starts.len() {
        let this = next;
        next += 1;
        let start = starts[this];
        let end = text[start..].find('\n').map_or(text.len(), |at| start + at);
        let code = text[start..end].trim_start();
        if code.is_empty() || code.starts_with('#') {
            continue;
        }
        if code.contains("zsh-tool init zsh") {
            parsed.initialised.get_or_insert(this);
        }
        if code.contains("starship init zsh") {
            parsed.starship = true;
            if code.starts_with("eval") {
                parsed.starship_lines.insert(this);
            }
        }
        if is_source_line(code) {
            parsed.source_lines.insert(this);
            continue;
        }

        let (exported, rest) = match code.strip_prefix("export") {
            Some(rest) if rest.starts_with([' ', '\t']) => (true, rest.trim_start()),
            _ => (false, code),
        };
        let name_len = rest
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
            .count();
        let (name, after) = rest.split_at(name_len);
        let (append, value) = match after.strip_prefix("+=") {
            Some(value) => (true, value),
            None => match after.strip_prefix('=') {
                Some(value) => (false, value),
                None => continue,
            },
        };
        let value_at = end - value.len();
        let vars = |var: &str| lookup(var, &parsed, env);

        match name {
            "plugins" if value.starts_with('(') => {
                let (words, close) = array(text, value_at + 1, &vars)
                    .with_context(|| format!("line {}: unterminated plugins=( list", this + 1))?;
                let last = line_of(close);
                parsed.plugin_lines.extend(this..=last);
                next = last + 1;
                if !append {
                    parsed.plugins.clear();
                }
                for word in words {
                    match word.value {
                        Some(name) if valid_name(&name) => {
                            if !parsed.plugins.contains(&name) {
                                parsed.plugins.push(name);
                            }
                        }
                        _ => parsed.warnings.push(format!(
                            "line {}: cannot use plugin {}; skipped",
                            line_of(word.start) + 1,
                            quote(&word.raw)
                        )),
                    }
                }
            }
            "ZSH_THEME" => {
                parsed.theme = Some(word(text, value_at, &vars).value);
                parsed.theme_lines.insert(this);
            }
            "ZSH" | "ZSH_CUSTOM" => {
                let value = word(text, value_at, &vars).value;
                let assignment = Assignment {
                    line: this,
                    value,
                    exported,
                };
                if name == "ZSH" {
                    parsed.zsh.0.push(assignment);
                } else {
                    parsed.zsh_custom.0.push(assignment);
                }
            }
            _ => {}
        }
    }
    Ok(parsed)
}

/// Value of `$name` while parsing: Oh My Zsh variables assigned earlier in
/// the file, then the environment.
fn lookup(name: &str, parsed: &Parsed, env: &impl Fn(&str) -> Option<PathBuf>) -> Option<String> {
    let assigned = match name {
        "ZSH" => Some(&parsed.zsh),
        "ZSH_CUSTOM" => Some(&parsed.zsh_custom),
        _ => None,
    };
    if let Some(assignments) = assigned.filter(|a| !a.0.is_empty()) {
        return assignments.value().map(str::to_owned);
    }
    env(name)?.into_os_string().into_string().ok()
}

/// `source <file>` or `. <file>` where the file is Oh My Zsh's entry
/// point, alone on its line apart from a comment.
fn is_source_line(code: &str) -> bool {
    let Some(rest) = code
        .strip_prefix("source")
        .or_else(|| code.strip_prefix('.'))
        .filter(|rest| rest.starts_with([' ', '\t']))
    else {
        return false;
    };
    let rest = rest.trim_start();
    let word = word(rest, 0, &|_| None);
    let tail = rest[word.end..].trim();
    (word.raw == "oh-my-zsh.sh" || word.raw.ends_with("/oh-my-zsh.sh"))
        && (tail.is_empty() || tail.starts_with('#'))
}

/// A shell word.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Word {
    /// Byte offset of the word in the text.
    start: usize,
    /// Offset just past the word.
    end: usize,
    /// The word without its quotes and with nothing expanded.
    raw: String,
    /// The expanded value, `None` when it depends on something other than
    /// plain variables (command substitution, `${x:-y}`, unknown variable).
    value: Option<String>,
}

/// Reads the shell word starting at `start`, expanding `~`, `$name` and
/// `${name}` with `vars`.
fn word(text: &str, start: usize, vars: &dyn Fn(&str) -> Option<String>) -> Word {
    let mut raw = String::new();
    let mut value = Some(String::new());
    let mut chars = text[start..].char_indices().peekable();
    let mut end = text.len();
    let push = |value: &mut Option<String>, text: Option<&str>| match (value.as_mut(), text) {
        (Some(value), Some(text)) => value.push_str(text),
        _ => *value = None,
    };

    if text[start..].starts_with('~')
        && matches!(
            text[start + 1..].chars().next(),
            None | Some('/' | ' ' | '\t' | '\n' | ';' | ')')
        )
    {
        chars.next();
        raw.push('~');
        push(&mut value, vars("HOME").as_deref());
    }
    let mut quote: Option<char> = None;
    while let Some((at, ch)) = chars.next() {
        match (quote, ch) {
            (None, ' ' | '\t' | '\n' | '\r' | ';' | '(' | ')' | '|' | '&' | '<' | '>') => {
                end = start + at;
                break;
            }
            (None, '\'' | '"') => quote = Some(ch),
            (Some(open), _) if ch == open => quote = None,
            (Some('\''), _) => {
                raw.push(ch);
                push(&mut value, Some(ch.encode_utf8(&mut [0; 4])));
            }
            (_, '\\') => {
                if let Some((_, next)) = chars.next() {
                    // Inside double quotes a backslash only escapes these.
                    if quote.is_some() && !matches!(next, '$' | '`' | '"' | '\\' | '\n') {
                        raw.push('\\');
                        push(&mut value, Some("\\"));
                    }
                    if next != '\n' {
                        raw.push(next);
                        push(&mut value, Some(next.encode_utf8(&mut [0; 4])));
                    }
                }
            }
            (_, '`') => {
                raw.push('`');
                value = None;
                for (_, ch) in chars.by_ref() {
                    raw.push(ch);
                    if ch == '`' {
                        break;
                    }
                }
            }
            (_, '$') => {
                raw.push('$');
                let rest = &text[start + at + 1..];
                let braced = rest.starts_with('{');
                let name_at = usize::from(braced);
                let name_len = rest[name_at..]
                    .bytes()
                    .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
                    .count();
                let name = &rest[name_at..name_at + name_len];
                let closed = !braced || rest[name_at + name_len..].starts_with('}');
                if name_len > 0 && closed {
                    let consumed = name_at + name_len + usize::from(braced);
                    raw.push_str(&rest[..consumed]);
                    for _ in 0..rest[..consumed].chars().count() {
                        chars.next();
                    }
                    push(&mut value, vars(name).as_deref());
                } else {
                    // $(...), ${x:-y}, $1 and the like cannot be worked
                    // out: skip any bracketed part and give up on the value.
                    value = None;
                    if let Some(&(_, open @ ('(' | '{'))) = chars.peek() {
                        let close = if open == '(' { ')' } else { '}' };
                        let mut depth = 0usize;
                        for (_, next) in chars.by_ref() {
                            raw.push(next);
                            if next == open {
                                depth += 1;
                            } else if next == close {
                                depth = depth.saturating_sub(1);
                                if depth == 0 {
                                    break;
                                }
                            }
                        }
                    }
                }
            }
            _ => {
                raw.push(ch);
                push(&mut value, Some(ch.encode_utf8(&mut [0; 4])));
            }
        }
    }
    if quote.is_some() {
        value = None;
    }
    Word {
        start,
        end,
        raw,
        value,
    }
}

/// Reads the words of an array whose `(` ends just before `start`, up to
/// the closing `)`; returns them with the offset of that `)`.
fn array(
    text: &str,
    start: usize,
    vars: &dyn Fn(&str) -> Option<String>,
) -> Result<(Vec<Word>, usize)> {
    let mut words = Vec::new();
    let mut at = start;
    loop {
        let rest = &text[at..];
        let Some(ch) = rest.chars().next() else {
            bail!("missing )");
        };
        if ch.is_whitespace() {
            at += ch.len_utf8();
        } else if rest.starts_with("\\\n") {
            at += 2;
        } else if ch == '#' {
            at = rest.find('\n').map_or(text.len(), |nl| at + nl);
        } else if ch == ')' {
            return Ok((words, at));
        } else {
            let word = word(text, at, vars);
            // A stray `(` or `|` would not move the cursor.
            at = if word.end > at {
                word.end
            } else {
                at + ch.len_utf8()
            };
            if !word.raw.is_empty() || word.value.as_deref().is_some_and(|v| !v.is_empty()) {
                words.push(word);
            }
        }
    }
}

/// The `.zshrc` with Oh My Zsh replaced by zsh-tool: the source line
/// becomes the init line, plugin and theme settings become comments, and
/// `ZSH`/`ZSH_CUSTOM` are exported so that `zsh-tool init` sees them.
fn rewrite(text: &str, parsed: &Parsed, engine: Engine) -> String {
    let mut out = String::with_capacity(text.len() + 256);
    let exports: HashSet<usize> = parsed
        .zsh
        .0
        .iter()
        .chain(&parsed.zsh_custom.0)
        .filter(|assignment| !assignment.exported)
        .map(|assignment| assignment.line)
        .collect();
    for (index, line) in text.split_inclusive('\n').enumerate() {
        let body = line.trim_end_matches(['\n', '\r']);
        let eol = &line[body.len()..];
        let code = body.trim_start();
        let indent = &body[..body.len() - code.len()];
        let commented = parsed.plugin_lines.contains(&index)
            || parsed.theme_lines.contains(&index)
            || (engine == Engine::Starship && parsed.starship_lines.contains(&index));
        if parsed.source_lines.contains(&index) {
            out.push_str(indent);
            out.push_str(INIT_LINE);
        } else if commented {
            out.push_str(indent);
            out.push_str(COMMENT_PREFIX);
            out.push_str(code);
        } else if exports.contains(&index) {
            out.push_str(indent);
            out.push_str("export ");
            out.push_str(code);
        } else {
            out.push_str(body);
        }
        out.push_str(eol);
    }
    out
}

/// The `config.toml` text.
fn config_toml(zshrc: &Path, plugins: &[String], engine: Engine, theme: Option<&str>) -> String {
    let string = |text: &str| toml::Value::String(text.to_owned()).to_string();
    let mut out = format!(
        "# Generated by `zsh-tool migrate` from {}.\n",
        comment(&zshrc.display().to_string())
    );
    if plugins.is_empty() {
        out.push_str("plugins = []\n");
    } else {
        out.push_str("plugins = [\n");
        for plugin in plugins {
            out.push_str(&format!("    {},\n", string(plugin)));
        }
        out.push_str("]\n");
    }
    let engine = match engine {
        Engine::None => "none",
        Engine::Starship => "starship",
        Engine::Legacy => "legacy",
    };
    out.push_str(&format!("\n[prompt]\nengine = {}\n", string(engine)));
    if let Some(theme) = theme {
        out.push_str(&format!("theme = {}\n", string(theme)));
    }
    out.push_str(
        "\n[compat]\n\
         # true sources Oh My Zsh's lib/*.zsh files before the plugins.\n\
         lib = false\n",
    );
    out
}

/// A line diff of two texts with the same number of lines.
fn diff(path: &Path, old: &str, new: &str) -> String {
    let old: Vec<_> = old.split_inclusive('\n').collect();
    let new: Vec<_> = new.split_inclusive('\n').collect();
    let mut out = format!("--- {0}\n+++ {0} (migrated)\n", path.display());
    let mut index = 0;
    while index < old.len() {
        if old[index] == new[index] {
            index += 1;
            continue;
        }
        let first = index;
        while index < old.len() && old[index] != new[index] {
            index += 1;
        }
        let count = index - first;
        out.push_str(&format!("@@ -{0},{1} +{0},{1} @@\n", first + 1, count));
        for (sign, lines) in [('-', &old), ('+', &new)] {
            for line in &lines[first..index] {
                out.push(sign);
                out.push_str(line.trim_end_matches(['\n', '\r']));
                out.push('\n');
            }
        }
    }
    out
}

/// Saves `text` next to `zshrc` as `.zshrc.zsh-tool.bak`, or a timestamped
/// name when that file already exists; never overwrites a file.
fn backup(zshrc: &Path, text: &str) -> Result<PathBuf> {
    let name = zshrc.file_name().map_or_else(
        || ".zshrc".into(),
        |name| name.to_string_lossy().into_owned(),
    );
    let stamp = utc_stamp(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs()),
    );
    let candidates = std::iter::once(format!("{name}.zsh-tool.bak"))
        .chain(std::iter::once(format!("{name}.zsh-tool.{stamp}.bak")))
        .chain((2..100).map(|n| format!("{name}.zsh-tool.{stamp}-{n}.bak")));
    for candidate in candidates {
        let path = zshrc.with_file_name(candidate);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut file) => {
                file.write_all(text.as_bytes())
                    .with_context(|| format!("cannot write {}", path.display()))?;
                return Ok(path);
            }
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {}
            Err(err) => {
                return Err(err).with_context(|| format!("cannot write {}", path.display()));
            }
        }
    }
    bail!("cannot find a free backup name next to {}", zshrc.display())
}

/// Replaces the contents of `path` atomically, following a symlink (as
/// dotfile managers use) and keeping the file's permissions.
fn write_in_place(path: &Path, text: &str) -> Result<()> {
    let target =
        fs::canonicalize(path).with_context(|| format!("cannot read {}", path.display()))?;
    let permissions = fs::metadata(&target)
        .with_context(|| format!("cannot read {}", target.display()))?
        .permissions();
    let tmp = target.with_file_name(format!(
        ".{}.zsh-tool.{}.tmp",
        target
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
        std::process::id()
    ));
    fs::write(&tmp, text).with_context(|| format!("cannot write {}", tmp.display()))?;
    fs::set_permissions(&tmp, permissions)
        .and_then(|()| fs::rename(&tmp, &target))
        .with_context(|| format!("cannot write {}", target.display()))
}

/// `YYYYMMDD-HHMMSS` in UTC for a Unix time.
fn utc_stamp(secs: u64) -> String {
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    // Civil date from a day count (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::config::Config;

    const HOME: &str = "/home/u";

    fn env(name: &str) -> Option<PathBuf> {
        (name == "HOME").then(|| PathBuf::from(HOME))
    }

    fn parse_text(text: &str) -> Parsed {
        parse(text, &env).unwrap()
    }

    fn migrate_text(text: &str) -> Migration {
        migrate(text, Path::new("/home/u/.zshrc"), env).unwrap()
    }

    fn no_vars(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn reads_single_line_plugins() {
        let parsed = parse_text("plugins=(git sudo z)\n");
        assert_eq!(parsed.plugins, ["git", "sudo", "z"]);
        assert_eq!(parsed.plugin_lines, HashSet::from([0]));
    }

    #[test]
    fn reads_multi_line_plugins_with_comments() {
        let text = "\
# plugins=(commented)
plugins=(
  git   # version control
  # docker
  'sudo' \"z\"
  extract)
echo done
";
        let parsed = parse_text(text);
        assert_eq!(parsed.plugins, ["git", "sudo", "z", "extract"]);
        assert_eq!(parsed.plugin_lines, HashSet::from([1, 2, 3, 4, 5]));
    }

    #[test]
    fn last_plugins_assignment_wins_and_appends_add() {
        let parsed = parse_text("plugins=(git)\nplugins=(sudo docker)\nplugins+=(z sudo)\n");
        assert_eq!(parsed.plugins, ["sudo", "docker", "z"]);
        assert_eq!(parsed.plugin_lines, HashSet::from([0, 1, 2]));
    }

    #[test]
    fn skips_unusable_plugin_names() {
        let parsed = parse_text("plugins=(git $extra ../x)\n");
        assert_eq!(parsed.plugins, ["git"]);
        assert_eq!(parsed.warnings.len(), 2, "{:?}", parsed.warnings);
        assert!(
            parsed.warnings[0].contains("line 1"),
            "{:?}",
            parsed.warnings
        );
    }

    #[test]
    fn unterminated_plugins_is_an_error() {
        let err = parse("x=1\nplugins=(git\n", &env).unwrap_err();
        assert!(format!("{err:#}").contains("line 2"), "{err:#}");
    }

    #[test]
    fn reads_theme_in_any_quoting() {
        for (text, theme) in [
            ("ZSH_THEME=\"agnoster\"\n", "agnoster"),
            ("ZSH_THEME='ys'\n", "ys"),
            ("ZSH_THEME=af-magic # mine\n", "af-magic"),
            ("ZSH_THEME=\"\"\n", ""),
            ("ZSH_THEME=\n", ""),
            ("ZSH_THEME=a\n  ZSH_THEME=b\n# ZSH_THEME=c\n", "b"),
        ] {
            assert_eq!(
                parse_text(text).theme,
                Some(Some(theme.to_owned())),
                "{text}"
            );
        }
        assert_eq!(parse_text("ZSH_THEME=$(pick)\n").theme, Some(None));
        assert_eq!(parse_text("plugins=(git)\n").theme, None);
    }

    #[test]
    fn expands_home_and_oh_my_zsh_variables() {
        for (text, zsh) in [
            ("export ZSH=\"$HOME/.oh-my-zsh\"\n", "/home/u/.oh-my-zsh"),
            ("export ZSH=${HOME}/omz\n", "/home/u/omz"),
            ("ZSH=~/.oh-my-zsh\n", "/home/u/.oh-my-zsh"),
            ("ZSH='$HOME/literal'\n", "$HOME/literal"),
            (
                "ZSH=/opt/omz\nZSH=/usr/share/oh-my-zsh\n",
                "/usr/share/oh-my-zsh",
            ),
        ] {
            assert_eq!(parse_text(text).zsh.value(), Some(zsh), "{text}");
        }
        let parsed = parse_text("export ZSH=/opt/omz\nZSH_CUSTOM=$ZSH/my-custom\n");
        assert_eq!(parsed.zsh_custom.value(), Some("/opt/omz/my-custom"));
        assert_eq!(parse_text("ZSH=${OMZ:-/x}\n").zsh.value(), None);
        assert_eq!(parse_text("ZSH=$UNKNOWN/x\n").zsh.value(), None);
    }

    #[test]
    fn recognises_source_line_variants() {
        for line in [
            "source $ZSH/oh-my-zsh.sh",
            "source \"$ZSH/oh-my-zsh.sh\"",
            "source ${ZSH}/oh-my-zsh.sh",
            ". $ZSH/oh-my-zsh.sh",
            "  source '/usr/share/oh-my-zsh/oh-my-zsh.sh'  # load it",
            "source ~/.oh-my-zsh/oh-my-zsh.sh",
            "source $HOME/.oh-my-zsh/oh-my-zsh.sh",
        ] {
            assert!(is_source_line(line.trim_start()), "{line}");
        }
        for line in [
            "source $ZSH/oh-my-zsh.sh.bak",
            "source $ZSH/oh-my-zsh.sh && echo",
            "sourcery $ZSH/oh-my-zsh.sh",
            "source ~/.zshrc.local",
        ] {
            assert!(!is_source_line(line), "{line}");
        }
    }

    #[test]
    fn words_handle_quotes_and_escapes() {
        let word = |text: &str| word(text, 0, &no_vars);
        assert_eq!(word("'a b'c d").value.as_deref(), Some("a bc"));
        assert_eq!(word("\"a\\\"b\\n\"").value.as_deref(), Some("a\"b\\n"));
        assert_eq!(word("a\\ b").value.as_deref(), Some("a b"));
        assert_eq!(word("`cmd` x").value, None);
        assert_eq!(word("$(a (b)) x").end, 8);
        assert_eq!(word("'open").value, None);
        assert_eq!(word("plain;next").end, 5);
    }

    #[test]
    fn rewrites_the_zshrc() {
        let text = "\
export ZSH=\"$HOME/.oh-my-zsh\"
ZSH_THEME=\"robbyrussell\"
plugins=(
  git
)
  source $ZSH/oh-my-zsh.sh # go
alias ll='ls -l'
";
        let migration = migrate_text(text);
        assert_eq!(
            migration.zshrc,
            "\
export ZSH=\"$HOME/.oh-my-zsh\"
# zsh-tool: ZSH_THEME=\"robbyrussell\"
# zsh-tool: plugins=(
  # zsh-tool: git
# zsh-tool: )
  eval \"$(zsh-tool init zsh)\"
alias ll='ls -l'
"
        );
    }

    #[test]
    fn exports_oh_my_zsh_variables() {
        let text = "ZSH=/opt/omz\n\tZSH_CUSTOM=/opt/custom\nsource $ZSH/oh-my-zsh.sh\n";
        let migration = migrate_text(text);
        assert_eq!(
            migration.zshrc,
            "export ZSH=/opt/omz\n\texport ZSH_CUSTOM=/opt/custom\neval \"$(zsh-tool init zsh)\"\n"
        );
    }

    #[test]
    fn keeps_line_endings_and_missing_final_newline() {
        let migration = migrate_text("ZSH_THEME=ys\r\nsource $ZSH/oh-my-zsh.sh");
        assert_eq!(
            migration.zshrc,
            "# zsh-tool: ZSH_THEME=ys\r\neval \"$(zsh-tool init zsh)\""
        );
    }

    fn config(migration: &Migration) -> Config {
        Config::parse(&migration.config).unwrap()
    }

    #[test]
    fn chooses_the_prompt_engine() {
        let source = "source $ZSH/oh-my-zsh.sh\n";
        let cases = [
            ("ZSH_THEME=\"\"\n", Engine::None, None),
            ("", Engine::None, None),
            ("ZSH_THEME=\"ys\"\n", Engine::Legacy, Some("ys")),
            ("ZSH_THEME=random\n", Engine::Legacy, Some("robbyrussell")),
            ("ZSH_THEME=\"$(x)\"\n", Engine::None, None),
        ];
        for (head, engine, theme) in cases {
            let migration = migrate_text(&format!("{head}{source}"));
            let config = config(&migration);
            assert_eq!(config.prompt.engine, engine, "{head}");
            assert_eq!(config.prompt.theme.as_deref(), theme, "{head}");
            assert!(!config.compat.lib);
        }
        let random = migrate_text(&format!("ZSH_THEME=random\n{source}"));
        assert!(random.warnings.iter().any(|w| w.contains("random")));
    }

    #[test]
    fn starship_line_selects_starship_and_is_commented() {
        let text = "ZSH_THEME=\"\"\nsource $ZSH/oh-my-zsh.sh\neval \"$(starship init zsh)\"\n";
        let migration = migrate_text(text);
        assert_eq!(config(&migration).prompt.engine, Engine::Starship);
        assert!(
            migration
                .zshrc
                .ends_with("# zsh-tool: eval \"$(starship init zsh)\"\n"),
            "{}",
            migration.zshrc
        );
        let commented = "source $ZSH/oh-my-zsh.sh\n# eval \"$(starship init zsh)\"\n";
        assert_eq!(config(&migrate_text(commented)).prompt.engine, Engine::None);
    }

    #[test]
    fn config_lists_plugins_and_parses_back() {
        let migration = migrate_text("plugins=(git colored-man-pages)\nsource $ZSH/oh-my-zsh.sh\n");
        let config = config(&migration);
        assert_eq!(config.plugins, ["git", "colored-man-pages"]);
        assert!(
            migration
                .config
                .starts_with("# Generated by `zsh-tool migrate` from /home/u/.zshrc.\n")
        );
        let empty = migrate_text("plugins=()\nsource $ZSH/oh-my-zsh.sh\n");
        assert!(config_toml_has_empty_plugins(&empty.config));
    }

    fn config_toml_has_empty_plugins(text: &str) -> bool {
        text.contains("plugins = []\n") && Config::parse(text).unwrap().plugins.is_empty()
    }

    #[test]
    fn config_comment_stays_on_one_line() {
        let text = config_toml(Path::new("/a\nplugins = [\"x\"]"), &[], Engine::None, None);
        assert!(Config::parse(&text).unwrap().plugins.is_empty(), "{text}");
    }

    #[test]
    fn warns_about_missing_plugins_and_theme() {
        let root = tempfile::tempdir().unwrap();
        let omz = root.path().join("omz");
        fs::create_dir_all(omz.join("plugins/git")).unwrap();
        fs::write(omz.join("plugins/git/git.plugin.zsh"), "").unwrap();
        let text = format!(
            "export ZSH={}\nZSH_THEME=nope\nplugins=(git absent)\nsource $ZSH/oh-my-zsh.sh\n",
            omz.display()
        );
        let migration = migrate(&text, Path::new(".zshrc"), env).unwrap();
        assert_eq!(
            migration.warnings,
            ["plugin 'absent' not found", "theme 'nope' not found"]
        );
        // Missing plugins are kept: they may be installed later.
        assert_eq!(config(&migration).plugins, ["git", "absent"]);
    }

    #[test]
    fn refuses_files_it_cannot_migrate() {
        let err = migrate("plugins=(git)\n", Path::new(".zshrc"), env).unwrap_err();
        assert!(format!("{err}").contains("oh-my-zsh.sh"), "{err}");
        let err = migrate(
            "eval \"$(zsh-tool init zsh)\"\nsource $ZSH/oh-my-zsh.sh\n",
            Path::new(".zshrc"),
            env,
        )
        .unwrap_err();
        assert!(format!("{err}").contains("line 1"), "{err}");
    }

    #[test]
    fn diff_shows_changed_lines() {
        let out = diff(Path::new("rc"), "a\nb\nc\nd\n", "a\nB\nC\nd\n");
        assert_eq!(
            out,
            "--- rc\n+++ rc (migrated)\n@@ -2,2 +2,2 @@\n-b\n-c\n+B\n+C\n"
        );
    }

    #[test]
    fn utc_stamps() {
        assert_eq!(utc_stamp(0), "19700101-000000");
        assert_eq!(utc_stamp(951_782_400), "20000229-000000");
        assert_eq!(utc_stamp(1_792_990_861), "20261026-050101");
    }

    struct Home {
        dir: tempfile::TempDir,
    }

    impl Home {
        fn new(zshrc: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            fs::write(dir.path().join(".zshrc"), zshrc).unwrap();
            Self { dir }
        }

        fn path(&self, rel: &str) -> PathBuf {
            self.dir.path().join(rel)
        }

        fn run(&self, options: &Options) -> Result<String> {
            let vars = HashMap::from([("HOME", self.dir.path().to_owned())]);
            let mut out = Vec::new();
            run(options, |name| vars.get(name).cloned(), &mut out)?;
            Ok(String::from_utf8(out).unwrap())
        }
    }

    const ZSHRC: &str = "ZSH_THEME=ys\nplugins=(git)\nsource $ZSH/oh-my-zsh.sh\n";

    #[test]
    fn run_writes_config_backup_and_zshrc() {
        let home = Home::new(ZSHRC);
        let out = home.run(&Options::default()).unwrap();
        assert!(out.contains("wrote"), "{out}");
        assert_eq!(
            fs::read_to_string(home.path(".zshrc.zsh-tool.bak")).unwrap(),
            ZSHRC
        );
        let config = Config::load(&home.path(".config/zsh-tool/config.toml")).unwrap();
        assert_eq!(config.config.plugins, ["git"]);
        assert!(
            fs::read_to_string(home.path(".zshrc"))
                .unwrap()
                .contains(INIT_LINE)
        );
    }

    #[test]
    fn dry_run_writes_nothing() {
        let home = Home::new(ZSHRC);
        let out = home
            .run(&Options {
                dry_run: true,
                ..Options::default()
            })
            .unwrap();
        assert!(out.contains("theme = \"ys\""), "{out}");
        assert!(out.contains("+eval \"$(zsh-tool init zsh)\""), "{out}");
        assert_eq!(fs::read_to_string(home.path(".zshrc")).unwrap(), ZSHRC);
        assert!(!home.path(".config").exists());
        assert!(!home.path(".zshrc.zsh-tool.bak").exists());
    }

    #[test]
    fn existing_config_needs_force() {
        let home = Home::new(ZSHRC);
        let config = home.path(".config/zsh-tool/config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(&config, "plugins = [\"mine\"]\n").unwrap();

        let err = home.run(&Options::default()).unwrap_err();
        assert!(format!("{err}").contains("--force"), "{err}");
        assert_eq!(fs::read_to_string(home.path(".zshrc")).unwrap(), ZSHRC);
        assert!(!home.path(".zshrc.zsh-tool.bak").exists());

        home.run(&Options {
            force: true,
            ..Options::default()
        })
        .unwrap();
        assert!(fs::read_to_string(&config).unwrap().contains("\"git\""));
    }

    #[test]
    fn second_backup_is_timestamped() {
        let home = Home::new(ZSHRC);
        fs::write(home.path(".zshrc.zsh-tool.bak"), "older").unwrap();
        home.run(&Options::default()).unwrap();
        assert_eq!(
            fs::read_to_string(home.path(".zshrc.zsh-tool.bak")).unwrap(),
            "older"
        );
        let stamped: Vec<_> = fs::read_dir(home.dir.path())
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().into_string().unwrap())
            .filter(|name| name.starts_with(".zshrc.zsh-tool.2") && name.ends_with(".bak"))
            .collect();
        assert_eq!(stamped.len(), 1, "{stamped:?}");
        assert_eq!(fs::read_to_string(home.path(&stamped[0])).unwrap(), ZSHRC);
    }

    #[test]
    fn symlinked_zshrc_stays_a_symlink() {
        let home = Home::new("");
        let real = home.path("dotfiles/zshrc");
        fs::create_dir_all(real.parent().unwrap()).unwrap();
        fs::write(&real, ZSHRC).unwrap();
        fs::remove_file(home.path(".zshrc")).unwrap();
        std::os::unix::fs::symlink(&real, home.path(".zshrc")).unwrap();
        home.run(&Options::default()).unwrap();
        assert!(
            fs::symlink_metadata(home.path(".zshrc"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(fs::read_to_string(&real).unwrap().contains(INIT_LINE));
    }

    #[test]
    fn explicit_zshrc_and_zdotdir() {
        let home = Home::new("");
        let custom = home.path("zdot/.zshrc");
        fs::create_dir_all(custom.parent().unwrap()).unwrap();
        fs::write(&custom, ZSHRC).unwrap();
        assert_eq!(
            default_zshrc(|name| (name == "ZDOTDIR").then(|| home.path("zdot"))).unwrap(),
            custom
        );
        home.run(&Options {
            zshrc: Some(custom.clone()),
            ..Options::default()
        })
        .unwrap();
        assert!(fs::read_to_string(&custom).unwrap().contains(INIT_LINE));
        assert!(home.path("zdot/.zshrc.zsh-tool.bak").exists());
    }

    #[test]
    fn missing_zshrc_is_an_error() {
        let home = Home::new("");
        let err = home
            .run(&Options {
                zshrc: Some(home.path("nope")),
                ..Options::default()
            })
            .unwrap_err();
        assert!(format!("{err}").contains("cannot read"), "{err}");
    }
}
