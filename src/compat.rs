//! Optional loading of Oh My Zsh's `lib/` directory (`compat.lib`).

use std::fs;
use std::path::PathBuf;

use crate::paths::Paths;

/// The `$ZSH/lib/*.zsh` files in the order Oh My Zsh sources them
/// (alphabetical), each replaced by `$ZSH_CUSTOM/lib/<file>` when that
/// exists; empty without Oh My Zsh.
pub fn lib_files(paths: &Paths) -> Vec<PathBuf> {
    let Some(omz) = &paths.omz_dir else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(omz.join("lib")) else {
        return Vec::new();
    };
    let mut names: Vec<_> = entries
        .flatten()
        .filter(|entry| entry.path().is_file())
        .map(|entry| entry.file_name())
        .filter(|name| name.as_encoded_bytes().ends_with(b".zsh"))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let custom = paths.omz_custom.join("lib").join(&name);
            if custom.is_file() {
                custom
            } else {
                omz.join("lib").join(name)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_lib_files_in_order_with_custom_overrides() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().to_owned();
        let lib = home.join(".oh-my-zsh/lib");
        fs::create_dir_all(&lib).unwrap();
        for name in [
            "theme-and-appearance.zsh",
            "git.zsh",
            "async_prompt.zsh",
            "notes.txt",
        ] {
            fs::write(lib.join(name), "").unwrap();
        }
        fs::create_dir(lib.join("tests.zsh")).unwrap();
        let paths = Paths::resolve(|name| (name == "HOME").then(|| home.clone())).unwrap();
        let custom = paths.omz_custom.join("lib");
        fs::create_dir_all(&custom).unwrap();
        fs::write(custom.join("git.zsh"), "").unwrap();
        fs::write(custom.join("extra.zsh"), "").unwrap();

        assert_eq!(
            lib_files(&paths),
            [
                lib.join("async_prompt.zsh"),
                custom.join("git.zsh"),
                lib.join("theme-and-appearance.zsh"),
            ]
        );
    }

    #[test]
    fn nothing_without_oh_my_zsh() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().to_owned();
        let paths = Paths::resolve(|name| (name == "HOME").then(|| home.clone())).unwrap();
        assert!(lib_files(&paths).is_empty());
    }
}
