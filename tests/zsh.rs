//! End-to-end tests: the generated script is evaluated by a real `zsh -f`.
//!
//! Without zsh on the machine the tests are skipped, unless
//! `ZSH_TOOL_REQUIRE_ZSH` is set (as in CI), in which case they fail.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Home {
    dir: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        let home = Self {
            dir: tempfile::tempdir().unwrap(),
        };
        fs::create_dir_all(home.omz().join("plugins")).unwrap();
        home
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn omz(&self) -> PathBuf {
        self.path().join(".oh-my-zsh")
    }

    fn plugin(&self, name: &str, file: &str, body: &str) {
        let dir = self.omz().join("plugins").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(file), body).unwrap();
    }

    fn config(&self, text: &str) {
        let dir = self.path().join(".config/zsh-tool");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.toml"), text).unwrap();
    }

    fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        command
            .env_clear()
            .env("HOME", self.path())
            .env("PATH", std::env::var_os("PATH").unwrap_or_default());
        command
    }

    fn init(&self) -> Output {
        self.command(env!("CARGO_BIN_EXE_zsh-tool"))
            .args(["init", "zsh"])
            .output()
            .unwrap()
    }

    /// Runs `body` in `zsh -f` after evaluating the init script.
    fn zsh(&self, body: &str) -> Option<Output> {
        self.zsh_with_env(&[], body)
    }

    fn zsh_with_env(&self, env: &[(&str, &Path)], body: &str) -> Option<Output> {
        if !zsh_available() {
            return None;
        }
        let script = format!(
            "eval \"$('{}' init zsh)\" || exit 90\n{body}",
            env!("CARGO_BIN_EXE_zsh-tool")
        );
        let mut command = self.command("zsh");
        command
            .envs(env.iter().copied())
            .args(["-f", "-c", &script]);
        Some(command.output().unwrap())
    }
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

#[test]
fn loads_legacy_plugins_with_oh_my_zsh_variables() {
    let home = Home::new();
    home.plugin(
        "hello",
        "hello.plugin.zsh",
        "alias hi='echo hello'\nHELLO_SAW_ZSH=$ZSH\nHELLO_SAW_CACHE=$ZSH_CACHE_DIR\n",
    );
    home.config("plugins = [\"hello\"]\n");
    let Some(output) = home.zsh("print -r -- $aliases[hi]; print -r -- $HELLO_SAW_ZSH; [[ -d $HELLO_SAW_CACHE/completions ]] && print cache-ok") else {
        return;
    };
    let out = stdout(&output);
    let lines: Vec<_> = out.lines().collect();
    assert_eq!(
        lines,
        ["echo hello", home.omz().to_str().unwrap(), "cache-ok"]
    );
}

#[test]
fn replays_compdef_calls_after_compinit() {
    let home = Home::new();
    home.plugin("comp", "comp.plugin.zsh", "compdef _comp mycmd\n");
    home.plugin("comp", "_comp", "#compdef comp\n_message hello\n");
    home.plugin("only", "_only", "#compdef only\n_message only\n");
    home.config("plugins = [\"comp\", \"only\"]\n");
    let Some(output) = home.zsh(
        "print -r -- $_comps[mycmd] $_comps[only]; print -r -- ${(t)_zsh_tool_compdefs:-unset}; [[ -f $ZSH_COMPDUMP ]] && print dump-ok",
    ) else {
        return;
    };
    assert_eq!(stdout(&output), "_comp _only\nunset\ndump-ok\n");
}

#[test]
fn compiled_completion_dump_is_used_on_next_start() {
    let home = Home::new();
    home.plugin("only", "_only", "#compdef only\n_message only\n");
    home.config("plugins = [\"only\"]\n");
    // Prints the compiled dump, whether it is up to date, the completion
    // read from it and any temporary file left behind.
    let check = "print -r -- $ZSH_COMPDUMP.zwc; [[ $ZSH_COMPDUMP.zwc -nt $ZSH_COMPDUMP ]] && print fresh; \
                 print -r -- $_comps[only]; for f in $ZSH_COMPDUMP.*.zwc(N); print -r -- $f";
    let Some(first) = home.zsh(check) else {
        return;
    };
    let first = stdout(&first);
    let lines: Vec<_> = first.lines().collect();
    assert_eq!(lines[1..], ["fresh", "_only"], "{first}");
    let modified = || fs::metadata(lines[0]).unwrap().modified().unwrap();
    let compiled = modified();

    // The second shell reads the compiled dump and leaves it alone.
    let second = home.zsh(check).unwrap();
    assert_eq!(stdout(&second), first);
    assert_eq!(modified(), compiled);
}

#[test]
fn missing_plugin_does_not_break_the_shell() {
    let home = Home::new();
    home.plugin("ok", "ok.plugin.zsh", "OK_LOADED=1\n");
    home.config("plugins = [\"absent\", \"ok\"]\n");
    let Some(output) = home.zsh("print -r -- ${OK_LOADED:-0}") else {
        return;
    };
    assert_eq!(stdout(&output), "1\n");
}

#[test]
fn plugin_paths_with_quotes_are_sourced() {
    let home = Home::new();
    let odd = home.path().join("it's $(here)");
    fs::create_dir_all(odd.join("plugins/odd")).unwrap();
    fs::write(odd.join("plugins/odd/odd.plugin.zsh"), "ODD_LOADED=1\n").unwrap();
    home.config("plugins = [\"odd\"]\n");
    let Some(output) = home.zsh_with_env(&[("ZSH_CUSTOM", &odd)], "print -r -- ${ODD_LOADED:-0}")
    else {
        return;
    };
    assert_eq!(stdout(&output), "1\n");
}

#[test]
fn second_run_serves_the_cache() {
    let home = Home::new();
    home.plugin("hello", "hello.plugin.zsh", "");
    home.config("plugins = [\"hello\"]\n");
    let first = stdout(&home.init());
    let cache = home.path().join(".cache/zsh-tool/init.zsh");
    assert_eq!(fs::read_to_string(&cache).unwrap(), first);
    assert_eq!(stdout(&home.init()), first);
}

#[test]
fn invalid_configuration_reports_line_and_fails() {
    let home = Home::new();
    home.config("plugins = [\"git\"]\n[prompt\n");
    let output = home.init();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("config.toml") && stderr.contains("line 2"),
        "{stderr}"
    );
}

#[test]
fn legacy_plugins_can_call_is_at_least() {
    let home = Home::new();
    home.plugin(
        "vercheck",
        "vercheck.plugin.zsh",
        "is-at-least 5.0 && VERCHECK=ok\n",
    );
    home.config("plugins = [\"vercheck\"]\n");
    let Some(output) = home.zsh("print -r -- ${VERCHECK:-missing}") else {
        return;
    };
    assert_eq!(stdout(&output), "ok\n");
}
