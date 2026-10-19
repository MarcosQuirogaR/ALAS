// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Finding add-ons on disk and installing the bundled examples.

use std::path::{Path, PathBuf};

use crate::error::AddonError;
use crate::manifest::AddonManifest;

/// Manifest file name.
pub const MANIFEST_FILE: &str = "addon.toml";
/// Marker written once the examples have been offered, so an example the user
/// deletes is not brought back.
pub const EXAMPLES_MARKER: &str = ".examples-installed";

/// An add-on folder with a valid manifest.
#[derive(Debug, Clone, PartialEq)]
pub struct InstalledAddon {
    /// Folder name; stable identity used by the settings store.
    pub id: String,
    /// Folder holding `addon.toml` and the entry file.
    pub dir: PathBuf,
    /// Parsed manifest.
    pub manifest: AddonManifest,
}

/// Result of scanning the add-ons folder.
#[derive(Debug, Default)]
pub struct Discovery {
    /// Valid add-ons, sorted by folder name.
    pub addons: Vec<InstalledAddon>,
    /// Folders skipped, with the reason.
    pub problems: Vec<(PathBuf, String)>,
}

/// Scan `root` for folders containing `addon.toml`. A missing root is empty.
pub fn discover(root: &Path) -> Discovery {
    let mut found = Discovery::default();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let manifest_path = dir.join(MANIFEST_FILE);
        if !manifest_path.is_file() {
            continue;
        }
        let id = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let loaded = std::fs::read_to_string(&manifest_path)
            .map_err(|error| AddonError::Io(error.to_string()))
            .and_then(|text| AddonManifest::parse(&text))
            .and_then(|manifest| {
                if dir.join(&manifest.entry).is_file() {
                    Ok(manifest)
                } else {
                    Err(AddonError::Manifest(format!(
                        "entry file `{}` not found",
                        manifest.entry
                    )))
                }
            });
        match loaded {
            Ok(manifest) => found.addons.push(InstalledAddon { id, dir, manifest }),
            Err(error) => found.problems.push((dir, error.to_string())),
        }
    }
    found
}

/// One bundled example: folder name and its files.
struct Example {
    id: &'static str,
    files: &'static [(&'static str, &'static str)],
}

const EXAMPLES: &[Example] = &[
    Example {
        id: "raymer_wing_weight",
        files: &[
            (
                "addon.toml",
                include_str!("../builtin/raymer_wing_weight/addon.toml"),
            ),
            (
                "main.rhai",
                include_str!("../builtin/raymer_wing_weight/main.rhai"),
            ),
        ],
    },
    Example {
        id: "raymer_wing_weight_py",
        files: &[
            (
                "addon.toml",
                include_str!("../builtin/raymer_wing_weight_py/addon.toml"),
            ),
            (
                "main.py",
                include_str!("../builtin/raymer_wing_weight_py/main.py"),
            ),
        ],
    },
];

/// On the first run (no marker), write each bundled example that is absent.
/// Returns the folders created. Existing folders are never touched.
pub fn install_examples(root: &Path) -> Result<Vec<String>, AddonError> {
    let io =
        |path: &Path, error: std::io::Error| AddonError::Io(format!("{}: {error}", path.display()));
    if root.join(EXAMPLES_MARKER).exists() {
        return Ok(Vec::new());
    }
    std::fs::create_dir_all(root).map_err(|error| io(root, error))?;
    let mut created = Vec::new();
    for example in EXAMPLES {
        let dir = root.join(example.id);
        if dir.exists() {
            continue;
        }
        std::fs::create_dir_all(&dir).map_err(|error| io(&dir, error))?;
        for (name, text) in example.files {
            let path = dir.join(name);
            std::fs::write(&path, text).map_err(|error| io(&path, error))?;
        }
        created.push(example.id.to_owned());
    }
    let marker = root.join(EXAMPLES_MARKER);
    std::fs::write(
        &marker,
        "examples offered; delete this file to reinstall missing ones\n",
    )
    .map_err(|error| io(&marker, error))?;
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("alas-addons-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn examples_install_once_and_load_as_valid_addons() {
        let root = temp("install");
        let created = install_examples(&root).unwrap();
        assert_eq!(created.len(), 2);
        let found = discover(&root);
        assert!(found.problems.is_empty(), "{:?}", found.problems);
        assert_eq!(found.addons.len(), 2);
        std::fs::remove_dir_all(root.join("raymer_wing_weight")).unwrap();
        assert!(install_examples(&root).unwrap().is_empty());
        assert_eq!(discover(&root).addons.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn broken_folders_are_reported_not_fatal() {
        let root = temp("broken");
        std::fs::create_dir_all(root.join("bad")).unwrap();
        std::fs::write(root.join("bad").join(MANIFEST_FILE), "name = 1").unwrap();
        std::fs::create_dir_all(root.join("noentry")).unwrap();
        std::fs::write(
            root.join("noentry").join(MANIFEST_FILE),
            "name=\"n\"\nversion=\"1\"\nkind=\"script\"\nentry=\"x.rhai\"\n[[outputs]]\nname=\"y\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("plain_folder")).unwrap();
        let found = discover(&root);
        assert!(found.addons.is_empty());
        assert_eq!(found.problems.len(), 2);
        assert!(discover(&root.join("missing")).addons.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }
}
