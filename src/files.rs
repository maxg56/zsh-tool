//! Small file system helpers.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

/// Writes `text` to `path` through a temporary file in the same directory,
/// so that readers never see a partial file. Creates missing parents.
pub fn write_atomic(path: &Path, text: &str) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    fs::create_dir_all(dir).with_context(|| format!("cannot create {}", dir.display()))?;
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    fs::write(&tmp, text).with_context(|| format!("cannot write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| format!("cannot write {}", path.display()))
}

/// Removes `path` if it exists, whether it is a file or a directory.
pub fn remove_all(path: &Path) -> Result<()> {
    let result = match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => Err(err),
    };
    result.with_context(|| format!("cannot remove {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_replaces() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a/b.toml");
        write_atomic(&path, "one").unwrap();
        write_atomic(&path, "two").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "two");
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn removes_files_and_directories() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("t");
        fs::create_dir_all(tree.join("x")).unwrap();
        fs::write(tree.join("x/f"), "").unwrap();
        remove_all(&tree).unwrap();
        assert!(!tree.exists());
        remove_all(&tree).unwrap();
        let file = dir.path().join("f");
        fs::write(&file, "").unwrap();
        remove_all(&file).unwrap();
        assert!(!file.exists());
    }
}
