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
    /// Hashes of earlier bundled releases that were installed without a sidecar.
    legacy_hashes: &'static [u64],
    files: &'static [(&'static str, &'static str)],
}

const EXAMPLES: &[Example] = &[
    Example {
        id: "raymer_wing_weight",
        legacy_hashes: &[0x27cf_4410_1a7f_64eb],
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
        legacy_hashes: &[],
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

/// Sidecar in an installed example holding the hash of the files ALAS wrote,
/// so a later refresh can tell an unmodified copy from one the user edited.
pub const BUNDLED_HASH_FILE: &str = ".bundled-hash";

/// FNV-1a 64 over the files in order (CRLF read as LF, a 0xff separator after
/// each file).
fn content_hash(parts: &[Vec<u8>]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut step = |byte: u8| {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    };
    for part in parts {
        let mut previous_cr = false;
        for &byte in part {
            if previous_cr && byte != b'\n' {
                step(b'\r');
            }
            previous_cr = byte == b'\r';
            if !previous_cr {
                step(byte);
            }
        }
        if previous_cr {
            step(b'\r');
        }
        step(0xff);
    }
    hash
}

fn bundled_hash(example: &Example) -> u64 {
    let parts: Vec<Vec<u8>> = example
        .files
        .iter()
        .map(|(_, text)| text.as_bytes().to_vec())
        .collect();
    content_hash(&parts)
}

/// Dotted numeric version, missing or non-numeric parts read as 0.
fn version_key(version: &str) -> Vec<u64> {
    version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .map(|part| part.trim().parse().unwrap_or(0))
        .collect()
}

fn newer(bundled: &str, installed: &str) -> bool {
    let (mut a, mut b) = (version_key(bundled), version_key(installed));
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    a > b
}

fn write_example(dir: &Path, example: &Example) -> Result<(), AddonError> {
    let io =
        |path: &Path, error: std::io::Error| AddonError::Io(format!("{}: {error}", path.display()));
    std::fs::create_dir_all(dir).map_err(|error| io(dir, error))?;
    for (name, text) in example.files {
        let path = dir.join(name);
        std::fs::write(&path, text).map_err(|error| io(&path, error))?;
    }
    let sidecar = dir.join(BUNDLED_HASH_FILE);
    std::fs::write(&sidecar, format!("{:016x}\n", bundled_hash(example)))
        .map_err(|error| io(&sidecar, error))
}

/// Replace an installed example whose bundled version is newer, but only when
/// the installed files are still the ones ALAS wrote (matching the sidecar
/// hash, or a known earlier bundled release that had no sidecar). A copy the
/// user edited is left alone. Returns the folders refreshed.
fn refresh_examples(root: &Path) -> Result<Vec<String>, AddonError> {
    let mut refreshed = Vec::new();
    for example in EXAMPLES {
        let dir = root.join(example.id);
        let Ok(text) = std::fs::read_to_string(dir.join(MANIFEST_FILE)) else {
            continue;
        };
        let Ok(installed) = AddonManifest::parse(&text) else {
            continue;
        };
        let Some(bundled) = example
            .files
            .iter()
            .find(|(name, _)| *name == MANIFEST_FILE)
            .and_then(|(_, text)| AddonManifest::parse(text).ok())
        else {
            continue;
        };
        if !newer(&bundled.version, &installed.version) {
            continue;
        }
        let parts: Option<Vec<Vec<u8>>> = example
            .files
            .iter()
            .map(|(name, _)| std::fs::read(dir.join(name)).ok())
            .collect();
        let Some(parts) = parts else { continue };
        let current = content_hash(&parts);
        let unmodified = match std::fs::read_to_string(dir.join(BUNDLED_HASH_FILE)) {
            Ok(recorded) => u64::from_str_radix(recorded.trim(), 16) == Ok(current),
            Err(_) => example.legacy_hashes.contains(&current),
        };
        if unmodified {
            write_example(&dir, example)?;
            refreshed.push(example.id.to_owned());
        }
    }
    Ok(refreshed)
}

/// On the first run (no marker), write each bundled example that is absent.
/// Returns the folders created. Afterwards (marker present) an unmodified
/// installed example is refreshed when the bundled version is newer; those
/// folders are returned too. Edited copies are never touched.
pub fn install_examples(root: &Path) -> Result<Vec<String>, AddonError> {
    let io =
        |path: &Path, error: std::io::Error| AddonError::Io(format!("{}: {error}", path.display()));
    if root.join(EXAMPLES_MARKER).exists() {
        return refresh_examples(root);
    }
    std::fs::create_dir_all(root).map_err(|error| io(root, error))?;
    let mut created = Vec::new();
    for example in EXAMPLES {
        let dir = root.join(example.id);
        if dir.exists() {
            continue;
        }
        write_example(&dir, example)?;
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

#[cfg(test)]
mod refresh_tests {
    use super::*;

    fn root(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("alas-refresh-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// Pretend the installed copy is an older release by lowering its version.
    fn downgrade(dir: &Path) {
        let path = dir.join(MANIFEST_FILE);
        let text = std::fs::read_to_string(&path).unwrap();
        std::fs::write(
            path,
            text.replacen("version = \"1.0.1\"", "version = \"1.0.0\"", 1),
        )
        .unwrap();
    }

    #[test]
    fn unmodified_older_copy_is_refreshed_but_an_edited_one_is_not() {
        let base = root("refresh");
        install_examples(&base).unwrap();
        let dir = base.join("raymer_wing_weight");
        // Sidecar hash covers the pristine files: downgrading the version
        // changes the manifest, so record the downgraded state as ALAS-written.
        downgrade(&dir);
        let parts: Vec<Vec<u8>> = ["addon.toml", "main.rhai"]
            .iter()
            .map(|n| std::fs::read(dir.join(n)).unwrap())
            .collect();
        std::fs::write(
            dir.join(BUNDLED_HASH_FILE),
            format!("{:016x}\n", content_hash(&parts)),
        )
        .unwrap();
        assert_eq!(install_examples(&base).unwrap(), ["raymer_wing_weight"]);
        let text = std::fs::read_to_string(dir.join(MANIFEST_FILE)).unwrap();
        assert!(text.contains("version = \"1.0.1\""));
        // Up to date now: nothing to do.
        assert!(install_examples(&base).unwrap().is_empty());
        // Edited copy, older version: left alone.
        downgrade(&dir);
        std::fs::write(dir.join("main.rhai"), "#{ wing_mass_kg: 1.0 }\n").unwrap();
        assert!(install_examples(&base).unwrap().is_empty());
        assert!(std::fs::read_to_string(dir.join("main.rhai"))
            .unwrap()
            .contains("1.0 }"));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn version_order_is_numeric() {
        assert!(newer("1.0.1", "1.0.0"));
        assert!(newer("1.10", "1.9.9"));
        assert!(!newer("1.0", "1.0.0"));
        assert!(!newer("1.0.0", "1.0.1"));
    }
}
