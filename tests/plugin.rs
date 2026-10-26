//! End-to-end tests of plugin management: the real binary, a temporary
//! `HOME`, and local Git repositories standing in for remote ones (no
//! network access). Fixtures are prepared with the `git` command.
//!
//! Tests that evaluate the script need zsh; without it they are skipped,
//! unless `ZSH_TOOL_REQUIRE_ZSH` is set, in which case they fail.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// A Git repository playing the part of a plugin source or the registry.
struct Upstream {
    _root: tempfile::TempDir,
    path: PathBuf,
}

impl Upstream {
    fn new(name: &str) -> Self {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(name);
        fs::create_dir(&path).unwrap();
        let upstream = Self { _root: root, path };
        upstream.git(&["init", "-q", "-b", "main"]);
        upstream
    }

    fn url(&self) -> &str {
        self.path.to_str().unwrap()
    }

    fn git(&self, args: &[&str]) -> String {
        let output = Command::new("git")
            .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
            .args(args)
            .current_dir(&self.path)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {output:?}");
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    }

    fn commit(&self, files: &[(&str, &str)]) -> String {
        for (path, body) in files {
            let path = self.path.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, body).unwrap();
        }
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", "change"]);
        self.git(&["rev-parse", "HEAD"])
    }
}

struct Home {
    dir: tempfile::TempDir,
    registry: Upstream,
}

impl Home {
    fn new() -> Self {
        let registry = Upstream::new("registry");
        registry.commit(&[(
            "plugins/greet/greet.plugin.zsh",
            "alias greet='echo hello from the registry'\n",
        )]);
        let home = Self {
            dir: tempfile::tempdir().unwrap(),
            registry,
        };
        let omz = home.path().join(".oh-my-zsh/plugins/omz");
        fs::create_dir_all(&omz).unwrap();
        fs::write(omz.join("omz.plugin.zsh"), "OMZ_LOADED=1\n").unwrap();
        home
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn config_file(&self) -> PathBuf {
        self.path().join(".config/zsh-tool/config.toml")
    }

    fn lock_text(&self) -> String {
        fs::read_to_string(self.path().join(".config/zsh-tool/zsh-tool.lock")).unwrap_or_default()
    }

    fn managed(&self, name: &str) -> PathBuf {
        self.path().join(".local/share/zsh-tool/plugins").join(name)
    }

    fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        command
            .env_clear()
            .env("HOME", self.path())
            .env("ZSH_TOOL_REGISTRY", self.registry.url())
            .env("PATH", std::env::var_os("PATH").unwrap_or_default());
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command(env!("CARGO_BIN_EXE_zsh-tool"))
            .args(args)
            .output()
            .unwrap()
    }

    /// Runs `args` and returns stdout, failing the test if it fails.
    fn ok(&self, args: &[&str]) -> String {
        stdout(&self.run(args))
    }

    /// Runs `args`, expects a failure and returns stderr.
    fn fails(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(!output.status.success(), "{args:?} succeeded: {output:?}");
        String::from_utf8(output.stderr).unwrap()
    }

    /// Runs `body` in `zsh -f` after evaluating the init script.
    fn zsh(&self, body: &str) -> Option<String> {
        if !zsh_available() {
            return None;
        }
        let script = format!(
            "eval \"$('{}' init zsh)\" || exit 90\n{body}",
            env!("CARGO_BIN_EXE_zsh-tool")
        );
        let output = self
            .command("zsh")
            .args(["-f", "-c", &script])
            .output()
            .unwrap();
        Some(stdout(&output))
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

fn plugin_repo(name: &str, body: &str) -> Upstream {
    let upstream = Upstream::new(name);
    upstream.commit(&[(&format!("{name}.plugin.zsh"), body)]);
    upstream
}

#[test]
fn added_plugins_load_in_zsh() {
    let home = Home::new();
    let up = plugin_repo("up", "alias up='echo cloned'\n");
    home.ok(&["plugin", "add", up.url()]);
    home.ok(&["plugin", "add", "greet"]);
    home.ok(&["plugin", "add", "omz"]);
    let config = fs::read_to_string(home.config_file()).unwrap();
    assert_eq!(config, "plugins = [\"up\", \"greet\", \"omz\"]\n");
    let Some(out) =
        home.zsh("print -r -- $aliases[up]; print -r -- $aliases[greet]; print $OMZ_LOADED")
    else {
        return;
    };
    assert_eq!(out, "echo cloned\necho hello from the registry\n1\n");
}

#[test]
fn add_keeps_comments_and_is_idempotent() {
    let home = Home::new();
    fs::create_dir_all(home.config_file().parent().unwrap()).unwrap();
    fs::write(
        home.config_file(),
        "# my plugins\nplugins = [\n    \"omz\", # from Oh My Zsh\n]\n\n[prompt]\nengine = \"none\"\n",
    )
    .unwrap();
    let up = plugin_repo("up", "");
    home.ok(&["plugin", "add", up.url()]);
    let out = home.ok(&["plugin", "add", up.url()]);
    assert!(
        out.contains("already installed") && out.contains("already enabled"),
        "{out}"
    );
    assert_eq!(
        fs::read_to_string(home.config_file()).unwrap(),
        "# my plugins\nplugins = [\n    \"omz\", # from Oh My Zsh\n    \"up\",\n]\n\n[prompt]\nengine = \"none\"\n"
    );
}

#[test]
fn list_shows_status_source_and_commit() {
    let home = Home::new();
    // A repository whose name differs from its plugin file.
    let up = Upstream::new("zsh-renamed");
    up.commit(&[("renamed.plugin.zsh", "")]);
    let commit = up.git(&["rev-parse", "--short=12", "HEAD"]);
    home.ok(&["plugin", "add", up.url(), "--name", "renamed"]);
    home.ok(&["plugin", "add", "omz"]);
    fs::write(
        home.config_file(),
        "plugins = [\"renamed\", \"omz\", \"absent\"]\n",
    )
    .unwrap();
    let out = home.ok(&["plugin", "list"]);
    let rows: Vec<Vec<&str>> = out
        .lines()
        .map(|l| l.split_whitespace().collect())
        .collect();
    assert_eq!(
        rows,
        [
            vec!["NAME", "STATUS", "SOURCE", "COMMIT"],
            vec!["renamed", "zsh-tool", up.url(), &commit],
            vec!["omz", "Oh", "My", "Zsh", "-", "-"],
            vec!["absent", "not", "found", "-", "-"],
        ]
    );
}

#[test]
fn remove_deletes_only_managed_plugins() {
    let home = Home::new();
    home.ok(&["plugin", "add", "greet"]);
    home.ok(&["plugin", "add", "omz"]);
    assert!(home.managed("greet").is_dir());
    home.ok(&["plugin", "remove", "greet"]);
    home.ok(&["plugin", "remove", "omz"]);
    assert!(!home.managed("greet").exists());
    assert!(
        home.path()
            .join(".oh-my-zsh/plugins/omz/omz.plugin.zsh")
            .is_file()
    );
    assert!(!home.lock_text().contains("greet"));
    assert_eq!(
        fs::read_to_string(home.config_file()).unwrap(),
        "plugins = []\n"
    );
    let err = home.fails(&["plugin", "remove", "omz"]);
    assert!(err.contains("neither enabled nor installed"), "{err}");
}

#[test]
fn state_changes_regenerate_the_cached_script() {
    let home = Home::new();
    assert!(!home.ok(&["init", "zsh"]).contains("# plugin greet"));
    home.ok(&["plugin", "add", "greet"]);
    assert!(home.ok(&["init", "zsh"]).contains("# plugin greet\n"));
    home.ok(&["plugin", "remove", "greet"]);
    assert!(!home.ok(&["init", "zsh"]).contains("# plugin greet"));
}

#[test]
fn refuses_insecure_sources_and_bad_names() {
    let home = Home::new();
    for source in [
        "http://example.com/a.git",
        "git://example.com/a.git",
        "ssh://example.com/a.git",
        "git@github.com:a/b.git",
        "../evil",
        "-rf",
    ] {
        home.fails(&["plugin", "add", source]);
    }
    let up = plugin_repo("up", "");
    let err = home.fails(&["plugin", "add", up.url(), "--name", "../x"]);
    assert!(err.contains("invalid plugin name"), "{err}");
    assert!(!home.config_file().exists());
    assert!(!home.path().join(".local/share/zsh-tool/plugins").exists());
}

#[test]
fn unknown_registry_plugin_is_a_clear_error() {
    let home = Home::new();
    let err = home.fails(&["plugin", "add", "absent"]);
    assert!(err.contains("plugin absent is not installed"), "{err}");
    assert!(!home.config_file().exists());
}

#[test]
fn invalid_name_in_configuration_is_reported_with_line() {
    let home = Home::new();
    fs::create_dir_all(home.config_file().parent().unwrap()).unwrap();
    fs::write(
        home.config_file(),
        "plugins = [\n  \"omz\",\n  \"../../etc\",\n]\n",
    )
    .unwrap();
    for args in [
        &["init", "zsh"][..],
        &["plugin", "add", "omz"],
        &["plugin", "list"],
    ] {
        let err = home.fails(args);
        assert!(
            err.contains("invalid plugin name") && err.contains("line 3"),
            "{args:?}: {err}"
        );
    }
}

#[test]
fn update_moves_to_the_new_commit_and_refreshes_the_cache() {
    let home = Home::new();
    let up = plugin_repo("up", "UP_VERSION=1\n");
    let first = up.git(&["rev-parse", "HEAD"]);
    home.ok(&["plugin", "add", up.url()]);
    let before = home.ok(&["init", "zsh"]);
    assert_eq!(home.ok(&["init", "zsh"]), before, "cache is served");

    let second = up.commit(&[("up.plugin.zsh", "UP_VERSION=2\n")]);
    let out = home.ok(&["plugin", "update"]);
    assert_eq!(out, format!("up: {} -> {}\n", &first[..12], &second[..12]));
    assert!(home.lock_text().contains(&second));
    assert_ne!(
        home.ok(&["init", "zsh"]),
        before,
        "the cache was invalidated"
    );
    if let Some(out) = home.zsh("print $UP_VERSION") {
        assert_eq!(out, "2\n");
    }
    assert_eq!(
        home.ok(&["plugin", "update", "up"]),
        format!("up: up to date ({})\n", &second[..12])
    );
}

#[test]
fn update_refuses_local_modifications() {
    let home = Home::new();
    let up = plugin_repo("up", "UP_VERSION=1\n");
    home.ok(&["plugin", "add", up.url()]);
    up.commit(&[("up.plugin.zsh", "UP_VERSION=2\n")]);
    fs::write(home.managed("up/notes.txt"), "mine").unwrap();
    let err = home.fails(&["plugin", "update"]);
    assert!(err.contains("local modifications (notes.txt)"), "{err}");
    assert_eq!(
        fs::read_to_string(home.managed("up/notes.txt")).unwrap(),
        "mine"
    );
    let err = home.fails(&["plugin", "update", "omz"]);
    assert!(err.contains("not managed by zsh-tool"), "{err}");
}

#[test]
fn upgrade_reports_changes_and_install_reproduces_the_lock() {
    let home = Home::new();
    let up = plugin_repo("up", "UP_VERSION=1\n");
    home.ok(&["plugin", "add", up.url()]);
    home.ok(&["plugin", "add", "greet"]);
    let locked = home.lock_text();

    // Upstream moves on, but `install` sticks to the lock file.
    up.commit(&[("up.plugin.zsh", "UP_VERSION=2\n")]);
    home.registry
        .commit(&[("plugins/greet/greet.plugin.zsh", "alias greet='echo v2'\n")]);
    fs::remove_dir_all(home.path().join(".local/share/zsh-tool")).unwrap();
    let out = home.ok(&["install"]);
    assert!(
        out.contains("greet: installed") && out.contains("up: installed"),
        "{out}"
    );
    assert_eq!(home.lock_text(), locked);
    assert_eq!(
        fs::read_to_string(home.managed("up/up.plugin.zsh")).unwrap(),
        "UP_VERSION=1\n"
    );
    assert!(home.ok(&["doctor"]).contains("0 error(s), 0 warning(s)"));

    let out = home.ok(&["upgrade"]);
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(lines.len(), 3, "{out}");
    // `install` already fetched the registry to find the locked commit.
    assert!(lines[0].starts_with("registry: "), "{out}");
    assert!(
        lines[1].starts_with("greet: ") && lines[1].contains(" -> "),
        "{out}"
    );
    assert!(
        lines[2].starts_with("up: ") && lines[2].contains(" -> "),
        "{out}"
    );
    assert_ne!(home.lock_text(), locked);
    if let Some(out) = home.zsh("print $UP_VERSION; print -r -- $aliases[greet]") {
        assert_eq!(out, "2\necho v2\n");
    }
    assert!(
        home.ok(&["upgrade"])
            .lines()
            .all(|line| line.contains("up to date"))
    );
}

#[test]
fn lock_file_follows_the_configuration() {
    let home = Home::new();
    let config = home.path().join("elsewhere/zt.toml");
    let output = home
        .command(env!("CARGO_BIN_EXE_zsh-tool"))
        .env("ZSH_TOOL_CONFIG", &config)
        .args(["plugin", "add", "greet"])
        .output()
        .unwrap();
    stdout(&output);
    let lock = fs::read_to_string(home.path().join("elsewhere/zsh-tool.lock")).unwrap();
    assert!(
        lock.contains("name = \"greet\"\nsource = \"registry\"\ncommit = \""),
        "{lock}"
    );
}

#[test]
fn doctor_reports_problems_with_exit_code() {
    let home = Home::new();
    home.ok(&["plugin", "add", "omz"]);
    let out = home.ok(&["doctor"]);
    assert!(out.contains("ok      plugin omz (Oh My Zsh"), "{out}");
    assert!(out.contains("Oh My Zsh found"), "{out}");

    fs::write(home.config_file(), "plugins = [\"omz\", \"absent\"]\n").unwrap();
    let output = home.run(&["doctor"]);
    assert!(!output.status.success());
    let out = String::from_utf8(output.stdout).unwrap();
    assert!(out.contains("error   plugin absent not found"), "{out}");

    fs::write(home.config_file(), "plugins = [\n\"omz\",\n[prompt]\n").unwrap();
    let out = String::from_utf8(home.run(&["doctor"]).stdout).unwrap();
    assert!(out.contains("error") && out.contains("line 3"), "{out}");

    fs::write(home.config_file(), "[prompt]\nengine = \"starship\"\n").unwrap();
    let output = home
        .command(env!("CARGO_BIN_EXE_zsh-tool"))
        .env("PATH", home.path())
        .arg("doctor")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let out = String::from_utf8(output.stdout).unwrap();
    assert!(out.contains("error   zsh not found"), "{out}");
    assert!(out.contains("error   starship not found"), "{out}");
}

#[test]
fn doctor_warns_about_drifted_managed_plugins() {
    let home = Home::new();
    let up = plugin_repo("up", "UP_VERSION=1\n");
    home.ok(&["plugin", "add", up.url()]);
    home.ok(&["plugin", "add", "greet"]);
    fs::write(home.managed("greet/greet.plugin.zsh"), "edited").unwrap();
    fs::remove_dir_all(home.managed("up")).unwrap();
    let out = String::from_utf8(home.run(&["doctor"]).stdout).unwrap();
    assert!(
        out.contains("warning managed plugin greet differs"),
        "{out}"
    );
    assert!(
        out.contains("warning managed plugin up is missing"),
        "{out}"
    );
}
