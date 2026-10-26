//! `zsh-tool plugin add/remove/list/update`, `upgrade` and `install`.
//!
//! Plugins cloned or copied by zsh-tool live in `$ZSH_TOOL_HOME/plugins`
//! and are recorded in `zsh-tool.lock`; nothing else is ever modified or
//! deleted. Network access only happens in the command that needs it.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::config::{self, Config};
use crate::files::remove_all;
use crate::git::{self, ObjectId};
use crate::lock::{Entry, Lock, Source};
use crate::paths::Paths;
use crate::plugin::{Plugin, Resolved, check_name};
use crate::source::{self, Spec, check_url};

/// `plugin add`: installs the plugin if needed, then enables it.
pub fn add(paths: &Paths, arg: &str, name: Option<&str>, out: &mut dyn Write) -> Result<()> {
    // Refuse to install anything for a configuration that cannot be edited.
    Config::load(&paths.config_file)?;
    let mut lock = Lock::load(&paths.lock_file)?;
    let name = match source::parse(arg, name)? {
        Spec::Git { url, name } => {
            add_git(paths, &mut lock, &name, &url, out)?;
            name
        }
        Spec::Name(name) => {
            let resolved = Plugin::resolve(&name, &paths.plugin_dirs()).resolved;
            if lock.plugins.contains_key(&name) || resolved != Resolved::NotFound {
                writeln!(out, "{name} is already installed")?;
            } else {
                add_from_registry(paths, &mut lock, &name, out)?;
            }
            name
        }
    };
    if config::add_plugin(&paths.config_file, &name)? {
        writeln!(out, "enabled {name} in {}", paths.config_file.display())?;
    } else {
        writeln!(out, "{name} is already enabled")?;
    }
    Ok(())
}

fn add_git(
    paths: &Paths,
    lock: &mut Lock,
    name: &str,
    url: &str,
    out: &mut dyn Write,
) -> Result<()> {
    let dest = paths.managed_dir().join(name);
    match lock.plugins.get(name) {
        Some(entry) if entry.source == Source::Git(url.to_owned()) => {
            writeln!(out, "{name} is already installed")?;
            return Ok(());
        }
        Some(entry) => bail!(
            "plugin {name} is already installed from {}; remove it first or pick another --name",
            entry.source
        ),
        None if dest.exists() => {
            bail!(
                "{} already exists and is not managed by zsh-tool",
                dest.display()
            )
        }
        None => {}
    }
    writeln!(out, "cloning {url}")?;
    let commit = staged(&dest, |tmp| git::clone(url, tmp, None))?;
    record(paths, lock, name, Source::Git(url.to_owned()), commit)?;
    writeln!(out, "installed {name} at {}", short(commit))?;
    Ok(())
}

fn add_from_registry(
    paths: &Paths,
    lock: &mut Lock,
    name: &str,
    out: &mut dyn Write,
) -> Result<()> {
    writeln!(out, "looking up {name} in {}", paths.registry_url)?;
    let commit = sync_registry(paths)?;
    let dest = paths.managed_dir().join(name);
    if dest.exists() {
        bail!(
            "{} already exists and is not managed by zsh-tool",
            dest.display()
        );
    }
    staged(&dest, |tmp| export_from_registry(paths, name, commit, tmp)).with_context(|| {
        format!("plugin {name} is not installed and cannot be taken from the registry")
    })?;
    record(paths, lock, name, Source::Registry, commit)?;
    writeln!(
        out,
        "installed {name} from the registry at {}",
        short(commit)
    )?;
    Ok(())
}

/// `plugin remove`: disables the plugin and deletes it if zsh-tool
/// installed it.
pub fn remove(paths: &Paths, name: &str, out: &mut dyn Write) -> Result<()> {
    check_name(name)?;
    let mut lock = Lock::load(&paths.lock_file)?;
    let disabled = config::remove_plugin(&paths.config_file, name)?;
    if disabled {
        writeln!(out, "disabled {name}")?;
    }
    if lock.plugins.remove(name).is_some() {
        let dir = paths.managed_dir().join(name);
        remove_all(&dir)?;
        lock.save(&paths.lock_file)?;
        writeln!(out, "deleted {}", dir.display())?;
    } else if !disabled {
        bail!("plugin {name} is neither enabled nor installed by zsh-tool");
    }
    Ok(())
}

/// Where a plugin was found, as shown by `plugin list` and `doctor`.
pub fn location(paths: &Paths, resolved: &Resolved) -> &'static str {
    if let Resolved::InvalidManifest { .. } = resolved {
        return "invalid manifest";
    }
    let Some(dir) = resolved.dir() else {
        return "not found";
    };
    let parent = dir.parent();
    if parent == Some(paths.managed_dir().as_path()) {
        "zsh-tool"
    } else if parent == Some(paths.omz_custom.join("plugins").as_path()) {
        "ZSH_CUSTOM"
    } else {
        "Oh My Zsh"
    }
}

/// `plugin list`: enabled plugins, where they load from, and the source
/// and commit of the ones zsh-tool manages.
pub fn list(paths: &Paths, out: &mut dyn Write) -> Result<()> {
    let config = Config::load(&paths.config_file)?.config;
    let lock = Lock::load(&paths.lock_file)?;
    let search_dirs = paths.plugin_dirs();
    let mut rows = vec![["NAME", "STATUS", "SOURCE", "COMMIT"].map(str::to_owned)];
    for name in &config.plugins {
        let plugin = Plugin::resolve(name, &search_dirs);
        let (source, commit) = match lock.plugins.get(name) {
            Some(entry) => (entry.source.to_string(), short(entry.commit)),
            None => ("-".to_owned(), "-".to_owned()),
        };
        rows.push([
            name.clone(),
            location(paths, &plugin.resolved).to_owned(),
            source,
            commit,
        ]);
    }
    let widths: Vec<usize> = (0..3)
        .map(|col| {
            rows.iter()
                .map(|row| row[col].chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();
    for row in rows {
        let line = format!(
            "{:w0$}  {:w1$}  {:w2$}  {}",
            row[0],
            row[1],
            row[2],
            row[3],
            w0 = widths[0],
            w1 = widths[1],
            w2 = widths[2]
        );
        writeln!(out, "{}", line.trim_end())?;
    }
    Ok(())
}

/// `plugin update`: moves the given managed plugins (all when `names` is
/// empty) to the head of their source.
pub fn update(paths: &Paths, names: &[String], out: &mut dyn Write) -> Result<()> {
    let mut lock = Lock::load(&paths.lock_file)?;
    let targets = if names.is_empty() {
        lock.plugins.keys().cloned().collect()
    } else {
        for name in names {
            check_name(name)?;
            if !lock.plugins.contains_key(name) {
                bail!(
                    "plugin {name} is not managed by zsh-tool; only plugins it installed can be updated"
                );
            }
        }
        names.to_vec()
    };
    if targets.is_empty() {
        writeln!(out, "no plugin installed by zsh-tool")?;
        return Ok(());
    }
    let mut registry = None;
    update_plugins(paths, &mut lock, &targets, &mut registry, out)
}

/// `upgrade`: refreshes the registry and updates every managed plugin.
pub fn upgrade(paths: &Paths, out: &mut dyn Write) -> Result<()> {
    let mut lock = Lock::load(&paths.lock_file)?;
    let mut registry = None;
    let old = git::remote_head(&paths.registry_dir()).ok();
    let uses_registry = lock.plugins.values().any(|e| e.source == Source::Registry);
    // Only contact the registry if it was ever used.
    if old.is_some() || uses_registry {
        let new = sync_registry(paths)?;
        report(out, "registry", old, new)?;
        registry = Some(new);
    }
    let targets: Vec<String> = lock.plugins.keys().cloned().collect();
    update_plugins(paths, &mut lock, &targets, &mut registry, out)?;
    // Rewrite even when nothing moved, so the file is in canonical form.
    lock.save(&paths.lock_file)
}

fn update_plugins(
    paths: &Paths,
    lock: &mut Lock,
    targets: &[String],
    registry: &mut Option<ObjectId>,
    out: &mut dyn Write,
) -> Result<()> {
    if registry.is_none()
        && targets
            .iter()
            .any(|name| lock.plugins[name].source == Source::Registry)
    {
        *registry = Some(sync_registry(paths)?);
    }
    // Check everything first: an update replaces the plugin's files.
    for name in targets {
        let dir = paths.managed_dir().join(name);
        if !dir.is_dir() {
            bail!(
                "plugin {name} is missing from {}; run `zsh-tool install`",
                dir.display()
            );
        }
        if let Some(path) = local_change(paths, name, &lock.plugins[name])? {
            bail!(
                "plugin {name} has local modifications ({}); refusing to update {}",
                path.display(),
                dir.display()
            );
        }
    }
    for name in targets {
        let entry = lock.plugins[name].clone();
        let dir = paths.managed_dir().join(name);
        let new = match &entry.source {
            Source::Git(url) => {
                let new = git::fetch(&dir, url)?;
                if new != entry.commit {
                    git::checkout(&dir, new)?;
                }
                new
            }
            Source::Registry => {
                let new = registry.context("registry not fetched")?;
                if new != entry.commit {
                    let tmp = staged_path(&dir);
                    remove_all(&tmp)?;
                    export_from_registry(paths, name, new, &tmp)
                        .with_context(|| format!("cannot update {name} from the registry"))?;
                    remove_all(&dir)?;
                    fs::rename(&tmp, &dir)
                        .with_context(|| format!("cannot write {}", dir.display()))?;
                }
                new
            }
        };
        if new != entry.commit {
            record(paths, lock, name, entry.source.clone(), new)?;
        }
        report(out, name, Some(entry.commit), new)?;
    }
    Ok(())
}

/// `install`: restores managed plugins missing from disk at the exact
/// commit of the lock file.
pub fn install(paths: &Paths, out: &mut dyn Write) -> Result<()> {
    let lock = Lock::load(&paths.lock_file)?;
    for (name, entry) in &lock.plugins {
        let dest = paths.managed_dir().join(name);
        if dest.exists() {
            writeln!(out, "{name}: present")?;
            continue;
        }
        let commit = entry.commit;
        match &entry.source {
            Source::Git(url) => {
                staged(&dest, |tmp| git::clone(url, tmp, Some(commit)))?;
            }
            Source::Registry => {
                let registry = paths.registry_dir();
                if !git::has_commit(&registry, commit) {
                    sync_registry(paths)?;
                }
                staged(&dest, |tmp| export_from_registry(paths, name, commit, tmp))?;
            }
        }
        writeln!(out, "{name}: installed at {}", short(commit))?;
    }
    Ok(())
}

/// First file of a managed plugin that differs from its locked commit.
pub fn local_change(paths: &Paths, name: &str, entry: &Entry) -> Result<Option<PathBuf>> {
    let dir = paths.managed_dir().join(name);
    let change = match &entry.source {
        Source::Git(_) => git::compare(&dir, entry.commit, "", &dir),
        Source::Registry => git::compare(
            &paths.registry_dir(),
            entry.commit,
            &registry_path(name),
            &dir,
        ),
    };
    change.with_context(|| format!("cannot check {} for local modifications", dir.display()))
}

/// Fetches the registry into its local clone and returns its head.
fn sync_registry(paths: &Paths) -> Result<ObjectId> {
    check_url(&paths.registry_url).context("invalid ZSH_TOOL_REGISTRY")?;
    let dir = paths.registry_dir();
    let fresh = !dir.join(".git").is_dir();
    if fresh {
        remove_all(&dir)?;
        git::init(&dir)?;
    }
    let head = git::fetch(&dir, &paths.registry_url);
    if fresh && head.is_err() {
        remove_all(&dir)?;
    }
    head
}

fn registry_path(name: &str) -> String {
    format!("plugins/{name}")
}

fn export_from_registry(paths: &Paths, name: &str, commit: ObjectId, dest: &Path) -> Result<()> {
    git::export(&paths.registry_dir(), commit, &registry_path(name), dest)
}

fn record(
    paths: &Paths,
    lock: &mut Lock,
    name: &str,
    source: Source,
    commit: ObjectId,
) -> Result<()> {
    lock.plugins
        .insert(name.to_owned(), Entry { source, commit });
    lock.save(&paths.lock_file)
}

/// Temporary sibling of `dest`; plugin names never start with a dot, so it
/// cannot clash with a plugin.
fn staged_path(dest: &Path) -> PathBuf {
    let name = dest.file_name().unwrap_or_default().to_string_lossy();
    dest.with_file_name(format!(".{name}.tmp"))
}

/// Builds `dest` in a temporary directory and moves it in place only when
/// `build` succeeds.
fn staged<T>(dest: &Path, build: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    let tmp = staged_path(dest);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create {}", parent.display()))?;
    }
    remove_all(&tmp)?;
    let value = match build(&tmp) {
        Ok(value) => value,
        Err(err) => {
            remove_all(&tmp)?;
            return Err(err);
        }
    };
    fs::rename(&tmp, dest).with_context(|| format!("cannot write {}", dest.display()))?;
    Ok(value)
}

fn short(commit: ObjectId) -> String {
    commit.to_hex_with_len(12).to_string()
}

fn report(out: &mut dyn Write, name: &str, old: Option<ObjectId>, new: ObjectId) -> Result<()> {
    match old {
        Some(old) if old == new => writeln!(out, "{name}: up to date ({})", short(new))?,
        Some(old) => writeln!(out, "{name}: {} -> {}", short(old), short(new))?,
        None => writeln!(out, "{name}: {}", short(new))?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::tests::Upstream;

    struct Fixture {
        root: tempfile::TempDir,
        paths: Paths,
        registry: Upstream,
    }

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let registry = Upstream::new();
            registry.commit(&[("plugins/nat/nat.plugin.zsh", "NAT=1\n")]);
            let home = root.path().to_owned();
            fs::create_dir_all(home.join(".oh-my-zsh/plugins/omz")).unwrap();
            fs::write(home.join(".oh-my-zsh/plugins/omz/omz.plugin.zsh"), "").unwrap();
            let registry_url = PathBuf::from(registry.url());
            let paths = Paths::resolve(|name| match name {
                "HOME" => Some(home.clone()),
                "ZSH_TOOL_REGISTRY" => Some(registry_url.clone()),
                _ => None,
            })
            .unwrap();
            Self {
                root,
                paths,
                registry,
            }
        }

        fn run(&self, action: impl FnOnce(&Paths, &mut dyn Write) -> Result<()>) -> Result<String> {
            let mut out = Vec::new();
            action(&self.paths, &mut out)?;
            Ok(String::from_utf8(out).unwrap())
        }

        fn add(&self, arg: &str) -> Result<String> {
            self.run(|paths, out| add(paths, arg, None, out))
        }

        fn plugins(&self) -> Vec<String> {
            Config::load(&self.paths.config_file)
                .unwrap()
                .config
                .plugins
        }

        fn lock(&self) -> Lock {
            Lock::load(&self.paths.lock_file).unwrap()
        }

        fn managed(&self, name: &str) -> PathBuf {
            self.paths.managed_dir().join(name)
        }
    }

    fn plugin_repo(name: &str) -> (Upstream, ObjectId) {
        let upstream = Upstream::named(name);
        let commit = upstream.commit(&[(&format!("{name}.plugin.zsh"), "V=1\n")]);
        (upstream, commit)
    }

    #[test]
    fn adds_git_plugin_once() {
        let fx = Fixture::new();
        let (upstream, commit) = plugin_repo("up");
        let url = upstream.url();
        let out = fx.add(&url).unwrap();
        assert!(out.contains("installed up"), "{out}");
        assert!(fx.managed("up/up.plugin.zsh").is_file());
        assert_eq!(fx.plugins(), ["up"]);
        let entry = &fx.lock().plugins["up"];
        assert_eq!(
            (&entry.source, entry.commit),
            (&Source::Git(url.clone()), commit)
        );

        let out = fx.add(&url).unwrap();
        assert!(
            out.contains("already installed") && out.contains("already enabled"),
            "{out}"
        );
        assert_eq!(fx.plugins(), ["up"]);
    }

    #[test]
    fn add_refuses_conflicts() {
        let fx = Fixture::new();
        let (first, _) = plugin_repo("up");
        let (second, _) = plugin_repo("up");
        fx.add(&first.url()).unwrap();
        let err = fx.add(&second.url()).unwrap_err().to_string();
        assert!(err.contains("--name"), "{err}");
        fx.run(|paths, out| add(paths, &second.url(), Some("up2"), out))
            .unwrap();
        assert_eq!(fx.plugins(), ["up", "up2"]);

        fs::create_dir_all(fx.managed("mine")).unwrap();
        let (mine, _) = plugin_repo("mine");
        let err = fx.add(&mine.url()).unwrap_err().to_string();
        assert!(err.contains("not managed by zsh-tool"), "{err}");
    }

    #[test]
    fn failed_clone_leaves_nothing_behind() {
        let fx = Fixture::new();
        let missing = fx.root.path().join("missing");
        assert!(fx.add(missing.to_str().unwrap()).is_err());
        assert!(
            fs::read_dir(fx.paths.managed_dir())
                .unwrap()
                .next()
                .is_none()
        );
        assert!(fx.plugins().is_empty());
        assert_eq!(fx.lock(), Lock::default());
    }

    #[test]
    fn enables_installed_plugin_without_cloning() {
        let fx = Fixture::new();
        let out = fx.add("omz").unwrap();
        assert!(out.contains("omz is already installed"), "{out}");
        assert_eq!(fx.plugins(), ["omz"]);
        assert_eq!(fx.lock(), Lock::default());
        assert!(!fx.paths.registry_dir().exists());
    }

    #[test]
    fn adds_plugin_from_registry() {
        let fx = Fixture::new();
        fx.add("nat").unwrap();
        assert_eq!(
            fs::read_to_string(fx.managed("nat/nat.plugin.zsh")).unwrap(),
            "NAT=1\n"
        );
        assert_eq!(fx.lock().plugins["nat"].source, Source::Registry);
        let err = format!("{:#}", fx.add("absent").unwrap_err());
        assert!(err.contains("absent") && err.contains("registry"), "{err}");
        assert!(!fx.managed("absent").exists() && !fx.managed(".absent.tmp").exists());
        assert_eq!(fx.plugins(), ["nat"]);
    }

    #[test]
    fn removes_only_managed_directories() {
        let fx = Fixture::new();
        fx.add("nat").unwrap();
        fx.add("omz").unwrap();
        fx.run(|paths, out| remove(paths, "nat", out)).unwrap();
        assert!(!fx.managed("nat").exists());
        assert!(!fx.lock().plugins.contains_key("nat"));
        fx.run(|paths, out| remove(paths, "omz", out)).unwrap();
        assert!(fx.root.path().join(".oh-my-zsh/plugins/omz").is_dir());
        assert!(fx.plugins().is_empty());
        assert!(fx.run(|paths, out| remove(paths, "omz", out)).is_err());
        assert!(fx.run(|paths, out| remove(paths, "../x", out)).is_err());
    }

    #[test]
    fn lists_plugins_with_status() {
        let fx = Fixture::new();
        fx.add("nat").unwrap();
        fx.add("omz").unwrap();
        config::add_plugin(&fx.paths.config_file, "gone").unwrap();
        let out = fx.run(list).unwrap();
        let lines: Vec<Vec<&str>> = out
            .lines()
            .map(|l| l.split_whitespace().collect())
            .collect();
        let commit = short(fx.lock().plugins["nat"].commit);
        assert_eq!(lines[0], ["NAME", "STATUS", "SOURCE", "COMMIT"]);
        assert_eq!(lines[1], ["nat", "zsh-tool", "registry", commit.as_str()]);
        assert_eq!(lines[2], ["omz", "Oh", "My", "Zsh", "-", "-"]);
        assert_eq!(lines[3], ["gone", "not", "found", "-", "-"]);
        let header = out.lines().next().unwrap();
        assert_eq!(
            out.lines().nth(1).unwrap().find("zsh-tool"),
            header.find("STATUS")
        );
    }

    #[test]
    fn updates_and_refuses_local_changes() {
        let fx = Fixture::new();
        let (upstream, first) = plugin_repo("up");
        fx.add(&upstream.url()).unwrap();
        let second = upstream.commit(&[("up.plugin.zsh", "V=2\n")]);

        fs::write(fx.managed("up/up.plugin.zsh"), "edited").unwrap();
        let err = fx
            .run(|paths, out| update(paths, &[], out))
            .unwrap_err()
            .to_string();
        assert!(err.contains("local modifications (up.plugin.zsh)"), "{err}");
        assert_eq!(fx.lock().plugins["up"].commit, first);

        fs::write(fx.managed("up/up.plugin.zsh"), "V=1\n").unwrap();
        let out = fx
            .run(|paths, out| update(paths, &["up".to_owned()], out))
            .unwrap();
        assert_eq!(out, format!("up: {} -> {}\n", short(first), short(second)));
        assert_eq!(
            fs::read_to_string(fx.managed("up/up.plugin.zsh")).unwrap(),
            "V=2\n"
        );
        assert_eq!(fx.lock().plugins["up"].commit, second);
        assert!(
            fx.run(|paths, out| update(paths, &["omz".to_owned()], out))
                .is_err()
        );
    }

    #[test]
    fn upgrade_reports_registry_and_plugins() {
        let fx = Fixture::new();
        fx.add("nat").unwrap();
        let old = fx.lock().plugins["nat"].commit;
        let new = fx
            .registry
            .commit(&[("plugins/nat/nat.plugin.zsh", "NAT=2\n")]);
        let out = fx.run(upgrade).unwrap();
        let change = format!("{} -> {}", short(old), short(new));
        assert_eq!(out, format!("registry: {change}\nnat: {change}\n"));
        assert_eq!(
            fs::read_to_string(fx.managed("nat/nat.plugin.zsh")).unwrap(),
            "NAT=2\n"
        );
        assert!(fx.run(upgrade).unwrap().contains("nat: up to date"));
    }

    #[test]
    fn upgrade_without_registry_stays_offline() {
        let mut fx = Fixture::new();
        fx.paths.registry_url = fx.root.path().join("nowhere").display().to_string();
        assert_eq!(fx.run(upgrade).unwrap(), "");
    }

    #[test]
    fn install_restores_locked_commits() {
        let fx = Fixture::new();
        let (upstream, first) = plugin_repo("up");
        fx.add(&upstream.url()).unwrap();
        fx.add("nat").unwrap();
        let nat = fx.lock().plugins["nat"].commit;
        upstream.commit(&[("up.plugin.zsh", "V=2\n")]);
        fx.registry
            .commit(&[("plugins/nat/nat.plugin.zsh", "NAT=2\n")]);
        remove_all(&fx.paths.managed_dir()).unwrap();
        remove_all(&fx.paths.registry_dir()).unwrap();

        let out = fx.run(install).unwrap();
        assert!(
            out.contains("up: installed") && out.contains("nat: installed"),
            "{out}"
        );
        assert_eq!(git::head(&fx.managed("up")).unwrap(), first);
        assert_eq!(
            fs::read_to_string(fx.managed("up/up.plugin.zsh")).unwrap(),
            "V=1\n"
        );
        assert_eq!(
            fs::read_to_string(fx.managed("nat/nat.plugin.zsh")).unwrap(),
            "NAT=1\n"
        );
        assert_eq!(fx.lock().plugins["nat"].commit, nat);
        assert_eq!(fx.run(install).unwrap(), "nat: present\nup: present\n");
    }
}
