// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Installing an add-on from a folder the user picked: the manifest is
//! validated first, then the folder is copied into `<data>/addons/<id>/`.
//!
//! Nothing here runs the add-on. The copy is discovered like any other
//! folder, and the existing trust gate (a process add-on stays disabled until
//! the user trusts this version) applies unchanged. Zip archives are not
//! accepted: no zip reader is a dependency of the workspace.

use std::fs;
use std::path::{Path, PathBuf};

use alas_addons::discovery::MANIFEST_FILE;
use alas_addons::AddonManifest;

use crate::state::{AppState, LogKind};
use crate::views::tr_fields;

/// Most files an installed add-on may hold.
const MAX_FILES: usize = 2_000;
/// Most bytes an installed add-on may hold.
const MAX_BYTES: u64 = 64 * 1024 * 1024;

/// A folder name usable as an add-on id: ASCII letters, digits, `-`, `_`, `.`.
fn id_for(source: &Path) -> Result<String, String> {
    let name = source
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let valid = !name.is_empty()
        && !name.starts_with('.')
        && !name.eq_ignore_ascii_case(alas_addons::settings::SETTINGS_FILE)
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if valid {
        Ok(name)
    } else {
        Err(format!(
            "the folder name `{name}` is not a valid add-on id (letters, digits, - _ . only)"
        ))
    }
}

/// Copy `source` into `target`, skipping links and refusing oversize trees.
fn copy_tree(
    source: &Path,
    target: &Path,
    files: &mut usize,
    bytes: &mut u64,
) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|error| format!("{}: {error}", target.display()))?;
    let entries = fs::read_dir(source).map_err(|error| format!("{}: {error}", source.display()))?;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let kind = entry.file_type().map_err(|error| error.to_string())?;
        let destination = target.join(entry.file_name());
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            copy_tree(&path, &destination, files, bytes)?;
        } else if kind.is_file() {
            *files += 1;
            *bytes += entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            if *files > MAX_FILES || *bytes > MAX_BYTES {
                return Err("the folder is too large for an add-on".to_owned());
            }
            fs::copy(&path, &destination)
                .map_err(|error| format!("{}: {error}", path.display()))?;
        }
    }
    Ok(())
}

/// Validate the add-on in `source` and copy it into `addons_dir`. Returns the
/// new add-on's id. An add-on with that id already installed is never
/// overwritten.
pub(crate) fn install_from_folder(source: &Path, addons_dir: &Path) -> Result<String, String> {
    if !source.is_dir() {
        return Err(format!("{} is not a folder", source.display()));
    }
    let id = id_for(source)?;
    let text = fs::read_to_string(source.join(MANIFEST_FILE))
        .map_err(|error| format!("{MANIFEST_FILE} not readable: {error}"))?;
    let manifest = AddonManifest::parse(&text).map_err(|error| error.to_string())?;
    if !source.join(&manifest.entry).is_file() {
        return Err(format!("entry file `{}` not found", manifest.entry));
    }
    let target: PathBuf = addons_dir.join(&id);
    if target.exists() {
        return Err(format!("an add-on named `{id}` is already installed"));
    }
    let (mut files, mut bytes) = (0, 0);
    if let Err(error) = copy_tree(source, &target, &mut files, &mut bytes) {
        let _ = fs::remove_dir_all(&target);
        return Err(error);
    }
    Ok(id)
}

impl AppState {
    /// Install the add-on in the folder `source` (see [`install_from_folder`])
    /// and select it. It stays disabled until trusted, like any other.
    pub(crate) fn install_addon_from_folder(&mut self, source: &Path) {
        self.ensure_addons_loaded();
        let dir = self.addons.dir.clone();
        let message = match install_from_folder(source, &dir) {
            Ok(id) => {
                self.reload_addons();
                self.addons.selected = Some(id.clone());
                self.log(
                    tr_fields("Installed add-on {id}.", &[("id", id.clone())]),
                    LogKind::Info,
                );
                tr_fields(
                    "Installed add-on {id}. It stays disabled until you trust it.",
                    &[("id", id)],
                )
            }
            Err(error) => tr_fields("Add-on not installed: {error}", &[("error", error)]),
        };
        self.addons.status = Some(message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("alas-addon-install-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    const MANIFEST: &str =
        "name = \"Demo\"\nversion = \"1.0\"\nkind = \"script\"\nentry = \"main.rhai\"\n[[outputs]]\nname = \"mass\"\n";

    #[test]
    fn a_valid_folder_is_copied_and_discovered() {
        let root = temp("ok");
        let source = root.join("demo_addon");
        fs::create_dir_all(source.join("lib")).unwrap();
        fs::write(source.join(MANIFEST_FILE), MANIFEST).unwrap();
        fs::write(source.join("main.rhai"), "1").unwrap();
        fs::write(source.join("lib").join("helper.rhai"), "2").unwrap();
        let addons = root.join("addons");
        let id = install_from_folder(&source, &addons).unwrap();
        assert_eq!(id, "demo_addon");
        assert!(addons.join("demo_addon/lib/helper.rhai").is_file());
        assert_eq!(alas_addons::discover(&addons).addons.len(), 1);
        // A second install of the same id is refused and leaves the first.
        assert!(install_from_folder(&source, &addons).is_err());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn an_invalid_manifest_or_missing_entry_installs_nothing() {
        let root = temp("bad");
        let addons = root.join("addons");
        let broken = root.join("broken");
        fs::create_dir_all(&broken).unwrap();
        fs::write(broken.join(MANIFEST_FILE), "not = [toml").unwrap();
        assert!(install_from_folder(&broken, &addons).is_err());
        let no_entry = root.join("no_entry");
        fs::create_dir_all(&no_entry).unwrap();
        fs::write(no_entry.join(MANIFEST_FILE), MANIFEST).unwrap();
        assert!(install_from_folder(&no_entry, &addons).is_err());
        assert!(!addons.join("broken").exists() && !addons.join("no_entry").exists());
        let _ = fs::remove_dir_all(&root);
    }
}
