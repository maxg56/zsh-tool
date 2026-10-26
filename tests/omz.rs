//! End-to-end tests of the Oh My Zsh migration and of the prompt engines,
//! with a real zsh and the Oh My Zsh subset in `tests/fixtures/ohmyzsh`.
//!
//! Without zsh on the machine the tests are skipped, unless
//! `ZSH_TOOL_REQUIRE_ZSH` is set (as in CI), in which case they fail.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The binary under test.
const BIN: &str = env!("CARGO_BIN_EXE_zsh-tool");

/// The Oh My Zsh subset shipped with the tests.
fn omz_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ohmyzsh")
}

/// An isolated home directory.
struct Home {
    dir: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    /// A home with the Oh My Zsh fixture copied to `~/.oh-my-zsh`.
    fn with_oh_my_zsh() -> Self {
        let home = Self::new();
        copy_dir(&omz_fixture(), &home.omz());
        home
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn omz(&self) -> PathBuf {
        self.path().join(".oh-my-zsh")
    }

    fn config(&self, text: &str) {
        let dir = self.path().join(".config/zsh-tool");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("config.toml"), text).unwrap();
    }

    /// `PATH` with the directory of the binary under test first.
    fn path_var(&self, extra: &[&Path]) -> std::ffi::OsString {
        let bin_dir = Path::new(BIN).parent().unwrap().to_owned();
        let system = std::env::var_os("PATH").unwrap_or_default();
        let dirs = extra
            .iter()
            .map(|dir| dir.to_path_buf())
            .chain([bin_dir])
            .chain(std::env::split_paths(&system));
        std::env::join_paths(dirs).unwrap()
    }

    /// A command with a clean environment: `HOME`, `PATH`, `TERM=dumb`
    /// and a UTF-8 locale.
    fn command(&self, program: impl AsRef<std::ffi::OsStr>) -> Command {
        let mut command = Command::new(program);
        command
            .env_clear()
            .env("HOME", self.path())
            .env("PATH", self.path_var(&[]))
            .env("TERM", "dumb")
            .env("LANG", "C.UTF-8")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        command
    }

    /// Runs `body` in `zsh -f` after evaluating the init script; `None`
    /// when zsh is not installed.
    fn zsh(&self, body: &str) -> Option<Output> {
        self.zsh_with(|_| {}, body)
    }

    fn zsh_with(&self, setup: impl FnOnce(&mut Command), body: &str) -> Option<Output> {
        if !zsh_available() {
            return None;
        }
        let script = format!("eval \"$('{BIN}' init zsh)\" || exit 90\n{body}");
        let mut command = self.command("zsh");
        setup(&mut command);
        command.args(["-f", "-c", &script]);
        Some(command.output().unwrap())
    }

    /// Runs git in `dir` with a fixed identity.
    fn git(&self, dir: &Path, args: &[&str]) {
        let output = self
            .command("git")
            .current_dir(dir)
            .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
            .args([
                "-c",
                "init.defaultBranch=main",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success(), "git {args:?}: {}", stderr(&output));
    }

    /// A repository on branch `main` with one committed file.
    fn repo(&self, name: &str) -> PathBuf {
        let dir = self.path().join(name);
        fs::create_dir_all(&dir).unwrap();
        self.git(&dir, &["init", "-q"]);
        fs::write(dir.join("file.txt"), "one\n").unwrap();
        self.git(&dir, &["add", "file.txt"]);
        self.git(&dir, &["commit", "-q", "-m", "first"]);
        dir
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

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Stdout of a successful command that wrote nothing on stderr.
fn clean_stdout(output: &Output) -> String {
    assert!(
        output.status.success() && output.stderr.is_empty(),
        "status {:?}\nstdout: {}\nstderr: {}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        stderr(output)
    );
    String::from_utf8(output.stdout.clone()).unwrap()
}

/// Removes ANSI escape sequences.
fn strip_ansi(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if chars.next() == Some('[') {
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(ch);
        }
    }
    out
}

const TEN_PLUGINS: &str = "git sudo z extract colored-man-pages history docker kubectl npm python";

/// The `.zshrc` Oh My Zsh's installer writes, with ten plugins.
fn template_zshrc() -> String {
    let template = fs::read_to_string(omz_fixture().join("templates/zshrc.zsh-template")).unwrap();
    assert!(template.contains("\nplugins=(git)\n"));
    template.replace("\nplugins=(git)\n", &format!("\nplugins=({TEN_PLUGINS})\n"))
}

fn migrate(home: &Home, args: &[&str]) -> Output {
    home.command(BIN)
        .arg("migrate")
        .args(args)
        .output()
        .unwrap()
}

/// Runs `body` in an interactive zsh that reads the home's `.zshrc`.
///
/// Debian and Ubuntu ship an `/etc/zsh/zshrc` that runs its own `compinit`
/// before `~/.zshrc`; its security check aborts when a system directory
/// looks insecure, as on the GitHub Ubuntu runners. `skip_global_compinit`
/// in `~/.zshenv` turns it off, as those systems tell their users to do, so
/// the test sees only the `compinit` from zsh-tool.
fn interactive(home: &Home, body: &str) -> Option<Output> {
    if !zsh_available() {
        return None;
    }
    fs::write(home.path().join(".zshenv"), "skip_global_compinit=1\n").unwrap();
    Some(
        home.command("zsh")
            .args(["-i", "-c", body])
            .output()
            .unwrap(),
    )
}

/// `$PROMPT` as robbyrussell sets it, computed by sourcing the theme in a
/// bare zsh with only the colors loaded.
fn robbyrussell_prompt(home: &Home) -> String {
    let theme = home.omz().join("themes/robbyrussell.zsh-theme");
    let output = home
        .command("zsh")
        .args([
            "-f",
            "-c",
            "autoload -U colors && colors; source \"$1\"; print -r -- \"$PROMPT\"",
            "zsh",
        ])
        .arg(&theme)
        .output()
        .unwrap();
    clean_stdout(&output)
}

#[test]
fn migrated_ten_plugin_zshrc_gives_a_working_shell() {
    let home = Home::with_oh_my_zsh();
    let zshrc = home.path().join(".zshrc");
    let original = template_zshrc();
    fs::write(&zshrc, &original).unwrap();

    let output = migrate(&home, &[]);
    let out = clean_stdout(&output);
    assert!(out.contains("wrote"), "{out}");
    assert_eq!(
        fs::read_to_string(home.path().join(".zshrc.zsh-tool.bak")).unwrap(),
        original
    );
    let migrated = fs::read_to_string(&zshrc).unwrap();
    assert!(migrated.contains("\nexport ZSH=\"$HOME/.oh-my-zsh\"\n"));
    assert!(migrated.contains("\neval \"$(zsh-tool init zsh)\"\n"));
    assert!(migrated.contains(&format!("\n# zsh-tool: plugins=({TEN_PLUGINS})\n")));
    assert!(migrated.contains("\n# zsh-tool: ZSH_THEME=\"robbyrussell\"\n"));
    let config = fs::read_to_string(home.path().join(".config/zsh-tool/config.toml")).unwrap();
    for plugin in TEN_PLUGINS.split(' ') {
        assert!(config.contains(&format!("    \"{plugin}\",\n")), "{config}");
    }
    assert!(config.contains("engine = \"legacy\"\ntheme = \"robbyrussell\"\n"));

    let checks = r#"
print -r -- $aliases[gst]                    # git
print -r -- ${+functions[sudo-command-line]} # sudo
print -r -- ${+functions[zshz]} $aliases[z]  # z
print -r -- ${+functions[extract]}           # extract
print -r -- ${+functions[colored]}           # colored-man-pages
print -r -- $aliases[h]                      # history
print -r -- $aliases[dps]                    # docker
print -r -- $aliases[npmg]                   # npm
print -r -- ${+functions[pyclean]}           # python
print -r -- ${+functions[git_current_branch]} $_comps[extract]
print -r -- "$PROMPT"
"#;
    let Some(output) = interactive(&home, checks) else {
        return;
    };
    let out = clean_stdout(&output);
    let lines: Vec<_> = out.lines().collect();
    let prompt = robbyrussell_prompt(&home);
    assert_eq!(
        lines,
        [
            "git status",
            "1",
            "1 zshz 2>&1",
            "1",
            "1",
            "history",
            "docker ps",
            "npm i -g ",
            "1",
            "1 _extract",
            prompt.trim_end(),
        ]
    );
}

fn legacy_config(theme: &str) -> String {
    format!("[prompt]\nengine = \"legacy\"\ntheme = \"{theme}\"\n")
}

/// Renders the prompts of `theme` in `repo`, without colors.
fn render(home: &Home, theme: &str, repo: &Path) -> Option<String> {
    home.config(&legacy_config(theme));
    let body = format!(
        "cd '{}' || exit 91\nprint -P -- \"$PROMPT\"\nprint -P -- \"${{RPROMPT:-$RPS1}}\"",
        repo.display()
    );
    let output = home.zsh(&body)?;
    Some(strip_ansi(&clean_stdout(&output)))
}

#[test]
fn legacy_themes_render_branch_and_dirty_state() {
    // (theme, expected when clean, expected when dirty, absent when clean)
    let cases = [
        ("robbyrussell", "git:(main)", "git:(main) ✗", "✗"),
        ("agnoster", " main", " main ±", "±"),
        ("af-magic", "(main)", "(main*)", "*"),
        ("ys", "on git:main o", "on git:main x", " x"),
    ];
    let home = Home::with_oh_my_zsh();
    let repo = home.repo("repo");
    for (theme, clean, dirty, marker) in cases {
        fs::write(repo.join("file.txt"), "one\n").unwrap();
        let Some(prompt) = render(&home, theme, &repo) else {
            return;
        };
        assert!(prompt.contains(clean), "{theme} clean: {prompt:?}");
        assert!(!prompt.contains(marker), "{theme} clean: {prompt:?}");

        fs::write(repo.join("file.txt"), "two\n").unwrap();
        let prompt = render(&home, theme, &repo).unwrap();
        assert!(prompt.contains(dirty), "{theme} dirty: {prompt:?}");
    }
}

#[test]
fn legacy_theme_outside_a_repository() {
    let home = Home::with_oh_my_zsh();
    let Some(prompt) = render(&home, "robbyrussell", home.path()) else {
        return;
    };
    assert!(!prompt.contains("git:"), "{prompt:?}");
}

#[test]
fn missing_theme_warns_without_breaking_the_shell() {
    let home = Home::with_oh_my_zsh();
    home.config(&legacy_config("nope"));
    let Some(output) = home.zsh("print -r -- ok ${+functions[git_prompt_info]}") else {
        return;
    };
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout), "ok 1\n");
    assert_eq!(
        stderr(&output),
        "zsh-tool: warning: theme 'nope' not found\n"
    );
}

/// Runs `body` in `repo` with the theme API loaded (theme robbyrussell).
fn in_repo(home: &Home, repo: &Path, body: &str) -> Option<String> {
    home.config(&legacy_config("robbyrussell"));
    let output = home.zsh(&format!("cd '{}' || exit 91\n{body}", repo.display()))?;
    Some(clean_stdout(&output))
}

#[test]
fn git_dirty_state_honours_oh_my_zsh_settings() {
    let home = Home::with_oh_my_zsh();
    let repo = home.repo("repo");
    let dirty = "ZSH_THEME_GIT_PROMPT_DIRTY=D ZSH_THEME_GIT_PROMPT_CLEAN=C; parse_git_dirty";
    let Some(out) = in_repo(&home, &repo, dirty) else {
        return;
    };
    assert_eq!(out, "C\n");

    fs::write(repo.join("new.txt"), "").unwrap();
    assert_eq!(in_repo(&home, &repo, dirty).unwrap(), "D\n");
    let no_untracked = format!("DISABLE_UNTRACKED_FILES_DIRTY=true; {dirty}");
    assert_eq!(in_repo(&home, &repo, &no_untracked).unwrap(), "C\n");

    fs::write(repo.join("file.txt"), "changed\n").unwrap();
    assert_eq!(in_repo(&home, &repo, &no_untracked).unwrap(), "D\n");
    home.git(&repo, &["config", "oh-my-zsh.hide-dirty", "1"]);
    assert_eq!(in_repo(&home, &repo, dirty).unwrap(), "C\n");
}

#[test]
fn git_prompt_status_lists_states() {
    let home = Home::with_oh_my_zsh();
    let repo = home.repo("repo");
    let status = "ZSH_THEME_GIT_PROMPT_UNTRACKED='?' ZSH_THEME_GIT_PROMPT_MODIFIED=M \
                  ZSH_THEME_GIT_PROMPT_ADDED=A ZSH_THEME_GIT_PROMPT_AHEAD='>' \
                  ZSH_THEME_GIT_PROMPT_STASHED=S; print -r -- \"[$(git_prompt_status)]\"";
    let Some(out) = in_repo(&home, &repo, status) else {
        return;
    };
    assert_eq!(out, "[]\n");

    fs::write(repo.join("new.txt"), "").unwrap();
    fs::write(repo.join("file.txt"), "changed\n").unwrap();
    fs::write(repo.join("staged.txt"), "").unwrap();
    home.git(&repo, &["add", "staged.txt"]);
    // Markers come in reverse order of Oh My Zsh's list.
    assert_eq!(in_repo(&home, &repo, status).unwrap(), "[MA?]\n");

    home.git(&repo, &["config", "oh-my-zsh.hide-status", "1"]);
    assert_eq!(in_repo(&home, &repo, status).unwrap(), "[]\n");
}

/// Oh My Zsh's default git prompt settings, which robbyrussell overrides.
const DEFAULT_GIT_PROMPT: &str = "ZSH_THEME_GIT_PROMPT_PREFIX='git:(' ZSH_THEME_GIT_PROMPT_SUFFIX=')' \
                                  ZSH_THEME_GIT_PROMPT_DIRTY='*' ZSH_THEME_GIT_PROMPT_CLEAN=''";

#[test]
fn git_helpers_report_branch_remote_and_identity() {
    let home = Home::with_oh_my_zsh();
    let origin = home.repo("origin");
    home.git(home.path(), &["clone", "-q", "origin", "clone"]);
    let clone = home.path().join("clone");
    fs::write(clone.join("file.txt"), "ahead\n").unwrap();
    home.git(&clone, &["commit", "-q", "-am", "ahead"]);
    home.git(&clone, &["config", "user.name", "Ada"]);
    home.git(&clone, &["config", "user.email", "ada@example.com"]);

    let body = r#"
ZSH_THEME_GIT_PROMPT_AHEAD='>' ZSH_THEME_GIT_PROMPT_AHEAD_REMOTE=AR
ZSH_THEME_GIT_COMMITS_AHEAD_PREFIX='+' ZSH_THEME_GIT_PROMPT_SHA_BEFORE='<'
ZSH_THEME_GIT_PROMPT_SHA_AFTER='>' ZSH_THEME_GIT_SHOW_UPSTREAM=1
print -r -- $(git_current_branch) $(git_prompt_ahead) $(git_remote_status) $(git_commits_ahead)
print -r -- $(git_current_user_name) $(git_current_user_email) $(git_repo_name)
sha=$(git_prompt_long_sha)
print -r -- ${#sha} $(git_prompt_info)
"#;
    let body = format!("{DEFAULT_GIT_PROMPT}\n{body}");
    let Some(out) = in_repo(&home, &clone, &body) else {
        return;
    };
    let lines: Vec<_> = out.lines().collect();
    assert_eq!(lines[0], "main > AR +1");
    assert_eq!(lines[1], "Ada ada@example.com clone");
    assert_eq!(lines[2], "42 git:(main -> origin/main)");

    // Detached HEAD on a tag shows the tag, otherwise the short SHA.
    home.git(&origin, &["tag", "v1"]);
    home.git(&origin, &["checkout", "-q", "--detach"]);
    let detached = format!(
        "{DEFAULT_GIT_PROMPT}\nprint -r -- \"$(git_prompt_info) | $(git_current_branch) | $(git_prompt_short_sha)\""
    );
    let out = in_repo(&home, &origin, &detached).unwrap();
    let parts: Vec<_> = out.trim_end().split(" | ").collect();
    assert_eq!(parts[0], "git:(v1)", "{out}");
    assert!(
        parts[1].len() >= 7 && parts[1].bytes().all(|b| b.is_ascii_hexdigit()),
        "{out}"
    );
    assert_eq!(parts[2], parts[1], "{out}");
}

#[test]
fn theme_api_provides_colors_and_placeholders() {
    let home = Home::with_oh_my_zsh();
    let body = r#"
[[ -o prompt_subst ]] && print subst
print -r -- ${+fg[red]} ${+reset_color} ${+FG}
print -r -- "[$(virtualenv_prompt_info)] [$(ruby_prompt_info)] [$(hg_prompt_info)]"
print -r -- $ZSH_THEME_TERM_TITLE_IDLE $LSCOLORS ${${LS_COLORS:+set}:-unset}
print -r -- $aliases[ls]
"#;
    home.config(&legacy_config("robbyrussell"));
    // dircolors only knows colors for real terminals.
    let Some(output) = home.zsh_with(
        |cmd| {
            cmd.env("TERM", "xterm-256color");
        },
        body,
    ) else {
        return;
    };
    let out = clean_stdout(&output);
    let lines: Vec<_> = out.lines().collect();
    assert_eq!(lines[0], "subst");
    // robbyrussell does not use the 256-color tables.
    assert_eq!(lines[1], "1 1 0");
    assert_eq!(lines[2], "[] [] []");
    assert_eq!(lines[3], "%n@%m:%~ Gxfxcxdxbxegedabagacad set");
    assert!(lines[4].starts_with("ls "), "{out}");

    home.config(&legacy_config("af-magic"));
    let out = clean_stdout(&home.zsh("print -r -- ${#FG} ${#BG}").unwrap());
    assert_eq!(out, "256 256\n");
}

#[test]
fn starship_engine_runs_starship_when_installed() {
    let home = Home::new();
    home.config("[prompt]\nengine = \"starship\"\n");
    let bin = home.path().join("bin");
    fs::create_dir_all(&bin).unwrap();
    let path_var = home.path_var(&[&bin]);
    let body = "print -r -- ${STARSHIP_INIT:-absent}";

    // Not installed: a comment, no error.
    let Some(output) = home.zsh_with(
        |cmd| {
            cmd.env("PATH", &path_var);
        },
        body,
    ) else {
        return;
    };
    assert_eq!(clean_stdout(&output), "absent\n");
    let cached = fs::read_to_string(home.path().join(".cache/zsh-tool/init.zsh")).unwrap();
    assert!(
        cached.contains("# prompt: starship not found in PATH\n"),
        "{cached}"
    );

    // Installing starship regenerates the script.
    let starship = bin.join("starship");
    fs::write(&starship, "#!/bin/sh\necho \"STARSHIP_INIT='$*'\"\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&starship, fs::Permissions::from_mode(0o755)).unwrap();
    let output = home
        .zsh_with(
            |cmd| {
                cmd.env("PATH", &path_var);
            },
            body,
        )
        .unwrap();
    assert_eq!(clean_stdout(&output), "init zsh\n");
}

#[test]
fn compat_lib_loads_oh_my_zsh_lib_with_a_legacy_theme() {
    let home = Home::with_oh_my_zsh();
    fs::write(home.path().join(".zshrc"), template_zshrc()).unwrap();
    clean_stdout(&migrate(&home, &[]));
    let config = home.path().join(".config/zsh-tool/config.toml");
    let text = fs::read_to_string(&config).unwrap();
    fs::write(&config, text.replace("lib = false", "lib = true")).unwrap();

    let repo = home.repo("repo");
    let body = format!(
        "print -r -- ${{+functions[omz]}} ${{+functions[take]}} ${{+functions[_omz_async_request]}} $HISTSIZE\n\
         cd '{}' && print -P -- \"$(_omz_git_prompt_info)\"",
        repo.display()
    );
    let Some(output) = interactive(&home, &body) else {
        return;
    };
    let out = strip_ansi(&clean_stdout(&output));
    let lines: Vec<_> = out.lines().collect();
    assert_eq!(lines[0], "1 1 1 50000");
    assert!(lines[1].contains("git:(main)"), "{out}");
}

#[test]
fn dry_run_prints_without_writing() {
    let home = Home::with_oh_my_zsh();
    let zshrc = home.path().join(".zshrc");
    fs::write(&zshrc, template_zshrc()).unwrap();
    let out = clean_stdout(&migrate(
        &home,
        &["--dry-run", "--zshrc", zshrc.to_str().unwrap()],
    ));
    assert!(out.contains("theme = \"robbyrussell\""), "{out}");
    assert!(
        out.contains("-source $ZSH/oh-my-zsh.sh\n+eval \"$(zsh-tool init zsh)\"\n"),
        "{out}"
    );
    assert_eq!(fs::read_to_string(&zshrc).unwrap(), template_zshrc());
    assert!(!home.path().join(".config").exists());
}

#[test]
fn migrate_warns_about_missing_plugins_and_refuses_to_overwrite() {
    let home = Home::with_oh_my_zsh();
    let zshrc = home.path().join(".zshrc");
    let text = template_zshrc().replace("plugins=(git ", "plugins=(git absent ");
    fs::write(&zshrc, &text).unwrap();
    let output = migrate(&home, &[]);
    assert!(output.status.success());
    assert_eq!(
        stderr(&output),
        "zsh-tool: warning: plugin 'absent' not found\n"
    );

    // A second run sees the migrated .zshrc and stops before writing.
    let output = migrate(&home, &["--force"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("already runs zsh-tool init zsh"),
        "{}",
        stderr(&output)
    );

    fs::write(&zshrc, &text).unwrap();
    let output = migrate(&home, &[]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("--force"), "{}", stderr(&output));
    assert_eq!(fs::read_to_string(&zshrc).unwrap(), text);
}
