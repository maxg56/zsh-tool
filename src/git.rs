//! Git operations on plugin repositories, through gix.
//!
//! Work trees are written from the object database by zsh-tool itself:
//! every checkout is a fresh export of one commit's tree, which keeps the
//! code small and lets [`compare`] detect any local change, including new
//! files.

use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use anyhow::{Context, Result, anyhow, bail};
use gix::bstr::ByteSlice;
use gix::objs::tree::EntryKind;

pub use gix::ObjectId;

/// Local ref that records the remote's default branch after each fetch.
const REMOTE_HEAD: &str = "refs/remotes/origin/HEAD";

/// Parses a full hexadecimal commit id.
pub fn parse_id(hex: &str) -> Result<ObjectId> {
    if hex.len() != 40 || !hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        bail!("invalid commit id {hex:?}: expected 40 lowercase hexadecimal digits");
    }
    ObjectId::from_hex(hex.as_bytes()).map_err(|err| anyhow!("invalid commit id {hex:?}: {err}"))
}

/// Creates a repository in `dest`, which must not exist, fetches `url`,
/// and checks out `commit`, or the remote's default branch when `None`.
/// Returns the commit checked out.
pub fn clone(url: &str, dest: &Path, commit: Option<ObjectId>) -> Result<ObjectId> {
    init(dest)?;
    let head = fetch(dest, url)?;
    let commit = commit.unwrap_or(head);
    checkout(dest, commit).with_context(|| format!("cannot check out {url}"))?;
    Ok(commit)
}

/// Creates an empty repository in `dest`, which must not exist.
pub fn init(dest: &Path) -> Result<()> {
    if dest.exists() {
        bail!("{} already exists", dest.display());
    }
    gix::init(dest).with_context(|| format!("cannot create a repository in {}", dest.display()))?;
    Ok(())
}

/// Fetches the default branch of `url` into the repository at `dir` and
/// returns its commit. The URL is used as given: rewrite rules from the
/// user's Git configuration do not apply.
pub fn fetch(dir: &Path, url: &str) -> Result<ObjectId> {
    let repo = open(dir)?;
    let interrupt = AtomicBool::new(false);
    repo.remote_at_without_url_rewrite(url)
        .and_then(|remote| {
            remote.with_refspecs(
                Some(format!("+HEAD:{REMOTE_HEAD}").as_str()),
                gix::remote::Direction::Fetch,
            )
        })
        .and_then(|remote| {
            remote
                .connect(gix::remote::Direction::Fetch)?
                .prepare_fetch(gix::progress::Discard, Default::default())?
                .receive(gix::progress::Discard, &interrupt)
        })
        .with_context(|| format!("cannot fetch {url}"))?;
    remote_head(dir).with_context(|| format!("cannot find the default branch of {url}"))
}

/// Commit of the remote's default branch as of the last [`fetch`].
pub fn remote_head(dir: &Path) -> Result<ObjectId> {
    let repo = open(dir)?;
    let mut reference = repo
        .find_reference(REMOTE_HEAD)
        .with_context(|| format!("{} has never been fetched", dir.display()))?;
    Ok(reference.peel_to_id()?.detach())
}

/// Commit checked out in the repository at `dir`.
pub fn head(dir: &Path) -> Result<ObjectId> {
    let repo = open(dir)?;
    let id = repo
        .head_id()
        .with_context(|| format!("{} has no commit checked out", dir.display()))?;
    Ok(id.detach())
}

/// Whether the repository at `dir` holds `commit`.
pub fn has_commit(dir: &Path, commit: ObjectId) -> bool {
    open(dir).is_ok_and(|repo| repo.find_commit(commit).is_ok())
}

/// Replaces the work tree of the repository at `dir` by the tree of
/// `commit`, then points a detached `HEAD` and the index at it.
pub fn checkout(dir: &Path, commit: ObjectId) -> Result<()> {
    let repo = open(dir)?;
    let tree = repo
        .find_commit(commit)
        .with_context(|| format!("commit {commit} is not available"))?
        .tree_id()?
        .detach();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_name() != ".git" {
            crate::files::remove_all(&entry.path())?;
        }
    }
    export(dir, commit, "", dir)?;
    let mut index = repo.index_from_tree(&tree)?;
    index.write(Default::default())?;
    fs::write(repo.git_dir().join("HEAD"), format!("{commit}\n"))
        .with_context(|| format!("cannot update HEAD in {}", dir.display()))?;
    Ok(())
}

/// Writes the content of `subdir` (`""` for the root) in the tree of
/// `commit` from the repository at `repo_dir` into `dest`.
pub fn export(repo_dir: &Path, commit: ObjectId, subdir: &str, dest: &Path) -> Result<()> {
    let repo = open(repo_dir)?;
    let tree = subtree(&repo, commit, subdir)?;
    fs::create_dir_all(dest).with_context(|| format!("cannot create {}", dest.display()))?;
    write_tree(&repo, &tree, dest)
}

/// Compares `dir` with `subdir` in the tree of `commit`, ignoring a
/// top-level `.git`. Returns the first path that differs, relative to
/// `dir`, or `None` when they match.
pub fn compare(
    repo_dir: &Path,
    commit: ObjectId,
    subdir: &str,
    dir: &Path,
) -> Result<Option<PathBuf>> {
    let repo = open(repo_dir)?;
    let tree = subtree(&repo, commit, subdir)?;
    compare_tree(&repo, &tree, dir, Path::new(""))
}

fn open(dir: &Path) -> Result<gix::Repository> {
    // Ref updates write reflogs, which need a committer: provide one for
    // users who never configured Git.
    let options = gix::open::Options::default().config_overrides([
        "gitoxide.committer.nameFallback=zsh-tool",
        "gitoxide.committer.emailFallback=zsh-tool@localhost",
    ]);
    gix::open_opts(dir, options)
        .with_context(|| format!("cannot open the repository in {}", dir.display()))
}

fn subtree<'repo>(
    repo: &'repo gix::Repository,
    commit: ObjectId,
    subdir: &str,
) -> Result<gix::Tree<'repo>> {
    let tree = repo
        .find_commit(commit)
        .with_context(|| format!("commit {commit} is not available"))?
        .tree()?;
    if subdir.is_empty() {
        return Ok(tree);
    }
    let entry = tree
        .lookup_entry_by_path(subdir)?
        .with_context(|| format!("{subdir} does not exist in commit {commit}"))?;
    if !entry.mode().is_tree() {
        bail!("{subdir} is not a directory in commit {commit}");
    }
    Ok(entry.object()?.into_tree())
}

/// Turns a tree entry name into a single, safe path component.
fn component(name: &[u8]) -> Result<&str> {
    let name = name
        .to_str()
        .map_err(|_| anyhow!("file name is not valid UTF-8: {:?}", name.as_bstr()))?;
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\0'])
        || name.eq_ignore_ascii_case(".git")
    {
        bail!("refusing unsafe file name {name:?}");
    }
    Ok(name)
}

/// A symbolic link's target, stored as the content of its blob.
fn link_target(data: &[u8]) -> &Path {
    Path::new(OsStr::from_bytes(data))
}

fn write_tree(repo: &gix::Repository, tree: &gix::Tree<'_>, dest: &Path) -> Result<()> {
    for entry in tree.iter() {
        let entry = entry?;
        let path = dest.join(component(entry.filename())?);
        let mode = entry.mode();
        let write = || -> Result<()> {
            match mode.kind() {
                EntryKind::Tree => {
                    fs::create_dir(&path)?;
                    write_tree(repo, &entry.object()?.into_tree(), &path)?;
                }
                EntryKind::Blob | EntryKind::BlobExecutable => {
                    fs::write(&path, &repo.find_blob(entry.oid())?.data)?;
                    if mode.is_executable() {
                        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
                    }
                }
                EntryKind::Link => {
                    let blob = repo.find_blob(entry.oid())?;
                    symlink(link_target(&blob.data), &path)?;
                }
                // Submodules are not fetched: leave an empty directory, as git does.
                EntryKind::Commit => fs::create_dir(&path)?,
            }
            Ok(())
        };
        write().with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(())
}

fn compare_tree(
    repo: &gix::Repository,
    tree: &gix::Tree<'_>,
    dir: &Path,
    rel: &Path,
) -> Result<Option<PathBuf>> {
    let mut expected = Vec::new();
    for entry in tree.iter() {
        let entry = entry?;
        let name = component(entry.filename())?;
        expected.push(name.to_owned());
        let path = dir.join(name);
        let rel_path = rel.join(name);
        let Ok(meta) = fs::symlink_metadata(&path) else {
            return Ok(Some(rel_path));
        };
        let same = match entry.mode().kind() {
            EntryKind::Tree => {
                if !meta.is_dir() {
                    return Ok(Some(rel_path));
                }
                if let Some(diff) =
                    compare_tree(repo, &entry.object()?.into_tree(), &path, &rel_path)?
                {
                    return Ok(Some(diff));
                }
                true
            }
            EntryKind::Blob | EntryKind::BlobExecutable => {
                meta.is_file() && fs::read(&path)? == repo.find_blob(entry.oid())?.data
            }
            EntryKind::Link => {
                let target = repo.find_blob(entry.oid())?;
                meta.is_symlink() && fs::read_link(&path)? == link_target(&target.data)
            }
            EntryKind::Commit => meta.is_dir(),
        };
        if !same {
            return Ok(Some(rel_path));
        }
    }
    for entry in fs::read_dir(dir).with_context(|| format!("cannot read {}", dir.display()))? {
        let name = entry?.file_name();
        let top_git = rel.as_os_str().is_empty() && name == ".git";
        if !top_git && !expected.iter().any(|known| name == known.as_str()) {
            return Ok(Some(rel.join(name)));
        }
    }
    Ok(None)
}

#[cfg(test)]
pub mod tests {
    use std::process::Command;

    use super::*;

    /// A source repository built with the `git` command, for tests only.
    pub struct Upstream {
        _root: tempfile::TempDir,
        pub path: PathBuf,
    }

    impl Upstream {
        pub fn new() -> Self {
            Self::named("repo")
        }

        /// A repository in a directory called `name`.
        pub fn named(name: &str) -> Self {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join(name);
            fs::create_dir(&path).unwrap();
            let upstream = Self { _root: root, path };
            upstream.git(&["init", "-q", "-b", "main"]);
            upstream
        }

        pub fn url(&self) -> String {
            self.path.to_str().unwrap().to_owned()
        }

        pub fn git(&self, args: &[&str]) -> String {
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

        /// Writes `files` and commits them; returns the new commit.
        pub fn commit(&self, files: &[(&str, &str)]) -> ObjectId {
            for (path, body) in files {
                let path = self.path.join(path);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, body).unwrap();
            }
            self.git(&["add", "-A"]);
            self.git(&["commit", "-q", "--allow-empty", "-m", "change"]);
            parse_id(&self.git(&["rev-parse", "HEAD"])).unwrap()
        }
    }

    #[test]
    fn parses_ids() {
        let hex = "0123456789abcdef0123456789abcdef01234567";
        assert_eq!(parse_id(hex).unwrap().to_string(), hex);
        for bad in [
            "",
            "0123",
            &hex.to_uppercase(),
            &format!("{hex}0"),
            "../../../../../../../../../../../../etc",
        ] {
            assert!(parse_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn clones_fetches_and_checks_out() {
        let upstream = Upstream::new();
        let first = upstream.commit(&[("a.zsh", "one\n"), ("sub/b", "b\n")]);
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("clone");
        assert_eq!(clone(&upstream.url(), &dest, None).unwrap(), first);
        assert_eq!(fs::read_to_string(dest.join("a.zsh")).unwrap(), "one\n");
        assert_eq!(head(&dest).unwrap(), first);
        assert_eq!(compare(&dest, first, "", &dest).unwrap(), None);

        let second = upstream.commit(&[("a.zsh", "two\n")]);
        upstream.git(&["rm", "-q", "sub/b"]);
        upstream.git(&["commit", "-q", "-m", "rm"]);
        let third = parse_id(&upstream.git(&["rev-parse", "HEAD"])).unwrap();
        assert_ne!(second, third);
        assert_eq!(fetch(&dest, &upstream.url()).unwrap(), third);
        assert_eq!(remote_head(&dest).unwrap(), third);
        assert_eq!(
            head(&dest).unwrap(),
            first,
            "fetch must not touch the work tree"
        );
        checkout(&dest, third).unwrap();
        assert_eq!(head(&dest).unwrap(), third);
        assert_eq!(fs::read_to_string(dest.join("a.zsh")).unwrap(), "two\n");
        assert!(!dest.join("sub").exists());
        assert_eq!(compare(&dest, third, "", &dest).unwrap(), None);
    }

    #[test]
    fn clones_a_given_commit() {
        let upstream = Upstream::new();
        let first = upstream.commit(&[("a", "1")]);
        upstream.commit(&[("a", "2")]);
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("clone");
        assert_eq!(clone(&upstream.url(), &dest, Some(first)).unwrap(), first);
        assert_eq!(fs::read_to_string(dest.join("a")).unwrap(), "1");
        assert!(has_commit(&dest, first));
    }

    #[test]
    fn clone_refuses_existing_destination_and_unknown_commit() {
        let upstream = Upstream::new();
        upstream.commit(&[("a", "1")]);
        let root = tempfile::tempdir().unwrap();
        assert!(clone(&upstream.url(), root.path(), None).is_err());
        let missing = parse_id("0123456789abcdef0123456789abcdef01234567").unwrap();
        let err = clone(&upstream.url(), &root.path().join("c"), Some(missing)).unwrap_err();
        assert!(format!("{err:#}").contains("not available"), "{err:#}");
    }

    #[test]
    fn compare_reports_any_change() {
        let upstream = Upstream::new();
        let commit = upstream.commit(&[("a", "1"), ("d/b", "2")]);
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("c");
        clone(&upstream.url(), &dest, None).unwrap();
        fs::write(dest.join("d/b"), "changed").unwrap();
        assert_eq!(
            compare(&dest, commit, "", &dest).unwrap(),
            Some(PathBuf::from("d/b"))
        );
        fs::write(dest.join("d/b"), "2").unwrap();
        fs::write(dest.join("new"), "").unwrap();
        assert_eq!(
            compare(&dest, commit, "", &dest).unwrap(),
            Some(PathBuf::from("new"))
        );
        fs::remove_file(dest.join("new")).unwrap();
        fs::remove_file(dest.join("a")).unwrap();
        assert_eq!(
            compare(&dest, commit, "", &dest).unwrap(),
            Some(PathBuf::from("a"))
        );
    }

    #[test]
    fn exports_a_subdirectory() {
        let upstream = Upstream::new();
        upstream.commit(&[
            ("plugins/x/x.plugin.zsh", "X=1\n"),
            ("plugins/x/bin/tool", "#!/bin/sh\n"),
            ("plugins/y/y.plugin.zsh", ""),
        ]);
        fs::set_permissions(
            upstream.path.join("plugins/x/bin/tool"),
            fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        std::os::unix::fs::symlink("x.plugin.zsh", upstream.path.join("plugins/x/link")).unwrap();
        let commit = upstream.commit(&[]);
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("registry");
        init(&repo).unwrap();
        assert_eq!(fetch(&repo, &upstream.url()).unwrap(), commit);
        let dest = root.path().join("x");
        export(&repo, commit, "plugins/x", &dest).unwrap();
        assert_eq!(
            fs::read_to_string(dest.join("x.plugin.zsh")).unwrap(),
            "X=1\n"
        );
        assert_eq!(
            fs::read_link(dest.join("link")).unwrap(),
            Path::new("x.plugin.zsh")
        );
        let mode = fs::metadata(dest.join("bin/tool"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);
        assert!(!dest.join("y.plugin.zsh").exists());
        assert_eq!(compare(&repo, commit, "plugins/x", &dest).unwrap(), None);
        assert!(export(&repo, commit, "plugins/none", &root.path().join("n")).is_err());
    }

    #[test]
    fn fetch_reports_unreachable_source() {
        let root = tempfile::tempdir().unwrap();
        let dest = root.path().join("c");
        let err = clone(
            &root.path().join("missing").display().to_string(),
            &dest,
            None,
        )
        .unwrap_err();
        assert!(format!("{err:#}").contains("cannot fetch"), "{err:#}");
    }

    #[test]
    fn rejects_unsafe_names() {
        for name in ["", ".", "..", ".git", ".GIT", "a/b"] {
            assert!(component(name.as_bytes()).is_err(), "{name}");
        }
        assert_eq!(component(b".gitignore").unwrap(), ".gitignore");
    }
}
