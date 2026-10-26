//! End-to-end tests of native plugins (`plugin.toml`), loaded from the
//! fixtures in `tests/fixtures/plugins` by a real `zsh -f`.
//!
//! Without zsh on the machine the tests are skipped, unless
//! `ZSH_TOOL_REQUIRE_ZSH` is set (as in CI), in which case they fail.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

/// Runs `body` in `zsh -f` after evaluating the init script generated for
/// `plugins`, with the fixtures as the zsh-tool directory.
fn zsh(plugins: &[&str], body: &str) -> Option<Output> {
    if !zsh_available() {
        return None;
    }
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join(".config/zsh-tool");
    fs::create_dir_all(&config).unwrap();
    let list: Vec<String> = plugins.iter().map(|name| format!("\"{name}\"")).collect();
    fs::write(
        config.join("config.toml"),
        format!("plugins = [{}]\n", list.join(", ")),
    )
    .unwrap();
    let script = format!(
        "eval \"$('{}' init zsh)\" || exit 90\n{body}",
        env!("CARGO_BIN_EXE_zsh-tool")
    );
    let output = Command::new("zsh")
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("ZSH_TOOL_HOME", Path::new(FIXTURES))
        .args(["-f", "-c", &script])
        .output()
        .unwrap();
    Some(output)
}

fn zsh_available() -> bool {
    let found = Command::new("zsh")
        .args(["-f", "-c", "exit 0"])
        .output()
        .is_ok_and(|out| out.status.success());
    if !found {
        assert!(
            std::env::var_os("ZSH_TOOL_REQUIRE_ZSH").is_none(),
            "zsh is required (ZSH_TOOL_REQUIRE_ZSH is set) but was not found"
        );
        eprintln!("zsh not found, skipping");
    }
    found
}

fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "status {:?}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn loads_aliases_variables_and_fragments() {
    let Some(output) = zsh(
        &["greet"],
        "setopt aliases\nprint -r -- $aliases[hi]\nprint -r -- \"$GREET_WORD\"\ngreet you\nprint -r -- $aliases[hi!]\nprint -r -- ${GREET_LEGACY:-native}",
    ) else {
        return;
    };
    assert_eq!(stderr(&output), "");
    assert_eq!(
        stdout(&output),
        "greet world\nHello $(not run)\nHello $(not run), you\nprint -r -- bang\nnative\n"
    );
}

#[test]
fn alias_values_are_escaped() {
    // Aliases are expanded when a line is parsed, so run them through eval.
    let Some(output) = zsh(&["greet"], "eval quoted\neval 'hi!'") else {
        return;
    };
    assert_eq!(stdout(&output), "it's Hello $(not run)\nbang\n");
}

#[test]
fn native_fpath_and_compdef_reach_compinit() {
    let Some(output) = zsh(
        &["greet"],
        "print -r -- $_comps[greet] $_comps[hello-greet]",
    ) else {
        return;
    };
    assert_eq!(stdout(&output), "_greet _greet\n");
}

#[test]
fn variables_are_set_before_any_fragment() {
    let Some(output) = zsh(&["early", "late"], "print -r -- $EARLY_SAW") else {
        return;
    };
    assert_eq!(stdout(&output), "late\n");
}

#[test]
fn invalid_manifest_does_not_break_the_shell() {
    let Some(output) = zsh(&["broken", "greet"], "greet ok") else {
        return;
    };
    let err = stderr(&output);
    assert!(
        err.contains("plugin broken: invalid manifest")
            && err.contains("plugin.toml, line 3")
            && err.contains("init file `missing.zsh` does not exist"),
        "{err}"
    );
    assert_eq!(stdout(&output), "Hello $(not run), ok\n");
}
