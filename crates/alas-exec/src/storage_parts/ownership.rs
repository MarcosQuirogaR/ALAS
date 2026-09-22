// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Whether a configured location may be reclaimed, and whether a path may be
//! removed.
//!
//! The output, Airfoil CFD and navigation-data locations are free-text
//! settings, so nothing about a configured path proves this program wrote
//! what is inside it, and clearing is recursive and immediate. Both questions
//! are therefore answered here, at the moment of removal, rather than trusted
//! from the inventory that produced an entry: [`owns_configured_root`] decides
//! whether the application may reclaim a location at all, and
//! [`authorize_root`] with [`removal_target`] decides whether a given path
//! lies inside it.

use std::fs;
use std::path::{Component, Path, PathBuf};

use super::StorageCategoryId;

/// Marker file naming a directory the application manages.
///
/// The output, Airfoil CFD and navigation-data locations are free-text
/// settings, so nothing about a configured path proves the application wrote
/// what is inside it. A root carrying this file has been claimed explicitly,
/// which survives restarts and configuration edits; deleting the file is how
/// a user takes the directory back.
pub const OWNERSHIP_SENTINEL: &str = ".alas-storage";

/// First line of [`OWNERSHIP_SENTINEL`]; the rest of the file is explanation.
const SENTINEL_MARKER: &str = "ALAS storage root";

/// Top-level files that only a run of this application writes. One of them
/// is enough to admit an output root that predates the sentinel, without
/// enumerating everything a run may leave behind.
const RUN_ARTIFACTS: [&str; 5] = [
    "design_database.json",
    "run_manifest.json",
    "model_results.json",
    "plots/plot_manifest.json",
    "cpacs/run_manifest.json",
];

/// The navigation-data files the application downloads (`alas_route::assets`).
const NAVDATA_FILES: [&str; 3] = ["earth_fix.dat", "earth_awy.dat", "earth_nav.dat"];

/// Claim `root` for `id` by writing [`OWNERSHIP_SENTINEL`] into it.
///
/// Call this where a storage root is created, so later cleanup admits it by
/// the claim rather than by recognizing its contents. Clearing an output root
/// re-writes the sentinel, because the directory itself survives.
///
/// # Errors
///
/// The sentinel cannot be written.
pub fn mark_storage_root(id: StorageCategoryId, root: &Path) -> std::io::Result<()> {
    let body = format!(
        "{SENTINEL_MARKER}\nThis directory is managed by ALAS. Manage Storage may delete what is\ninside it. Delete this file to stop ALAS from reclaiming this directory.\ncategory={}\n",
        id.as_str()
    );
    fs::write(root.join(OWNERSHIP_SENTINEL), body)
}

/// Whether `root` carries a well-formed ownership sentinel.
pub(super) fn has_sentinel(root: &Path) -> bool {
    fs::read_to_string(root.join(OWNERSHIP_SENTINEL))
        .is_ok_and(|text| text.lines().next() == Some(SENTINEL_MARKER))
}

/// Whether the application may reclaim the configured location `root`.
///
/// A claimed root is admitted outright. Otherwise the location must carry
/// positive evidence that this application wrote it: a run artefact for an
/// output root, a study for the Airfoil CFD root, downloaded navigation data
/// for the navdata root, and a plain file for the globe texture, which is a
/// single file rather than a directory. Anything else -- an empty directory a
/// typo produced, or a directory of the user's own work -- is not owned, so
/// neither the inventory nor [`clear_storage`](super::clear_storage) will touch it. Solver scratch
/// is selected by [`SCRATCH_PREFIXES`](super::SCRATCH_PREFIXES) inside the system temporary directory
/// and is owned by that construction.
pub(super) fn owns_configured_root(id: StorageCategoryId, root: &Path) -> bool {
    if has_sentinel(root) {
        return true;
    }
    match id {
        StorageCategoryId::GeneratedOutputs => {
            RUN_ARTIFACTS.iter().any(|name| root.join(name).is_file())
        }
        StorageCategoryId::AirfoilCfdCases => holds_a_cfd_study(root),
        StorageCategoryId::DownloadedNavdata => holds_navigation_data(root),
        StorageCategoryId::DownloadedTexture => root.is_file(),
        StorageCategoryId::SolverScratch => true,
    }
}

/// Whether `root` holds an Airfoil CFD study, at its top level or one level
/// down, which is how studies are laid out.
pub(super) fn holds_a_cfd_study(root: &Path) -> bool {
    if root.join("study.json").is_file() {
        return true;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    entries
        .flatten()
        .any(|entry| entry.path().join("study.json").is_file())
}

/// Whether `root` holds downloaded navigation data.
pub(super) fn holds_navigation_data(root: &Path) -> bool {
    if NAVDATA_FILES.iter().any(|name| root.join(name).is_file()) {
        return true;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return false;
    };
    entries.flatten().any(|entry| {
        let path = entry.path();
        path.is_file() && path.extension().is_some_and(|extension| extension == "dat")
    })
}

/// The removable set of a configured location: the location itself when the
/// application owns it, nothing when it does not.
pub(super) fn admitted(id: StorageCategoryId, path: &Path) -> Vec<PathBuf> {
    if owns_configured_root(id, path) {
        vec![path.to_path_buf()]
    } else {
        Vec::new()
    }
}

/// Top-level folders of a user profile that hold the user's own documents
/// rather than application scratch.
const STANDARD_USER_FOLDERS: [&str; 8] = [
    "Desktop",
    "Documents",
    "Downloads",
    "Pictures",
    "Music",
    "Videos",
    "OneDrive",
    "AppData",
];

/// The home directory, canonicalized, when the platform reports one.
pub(super) fn home_directory() -> Option<PathBuf> {
    let raw = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    PathBuf::from(raw).canonicalize().ok()
}

/// The directory the running executable lives in, canonicalized.
pub(super) fn executable_directory() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    exe.parent()?.canonicalize().ok()
}

/// The canonical form of `root` when the application may reclaim it
/// recursively, or the reason it may not.
///
/// The configured output and navigation-data locations are free-text fields,
/// so a typo or a deliberate edit can aim a category at a drive root, at a
/// home directory, or at the tree the application itself lives in. Clearing
/// is recursive and immediate, so admitting such a root once is
/// unrecoverable. The rejected cases are those whose recursive removal
/// necessarily reaches data this program never wrote: a filesystem, drive or
/// UNC share root, which has no normal component of its own; the home
/// directory, an ancestor of it, or one of its standard top-level folders;
/// and any directory containing the running executable.
pub(super) fn authorize_root(root: &Path) -> Result<PathBuf, String> {
    let canonical = root
        .canonicalize()
        .map_err(|error| format!("cannot resolve {}: {error}", root.display()))?;
    if !canonical
        .components()
        .any(|component| matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "{} is a filesystem root, which is never cleared recursively",
            canonical.display()
        ));
    }
    if let Some(home) = home_directory() {
        if home.starts_with(&canonical) {
            return Err(format!(
                "{} is the home directory or an ancestor of it",
                canonical.display()
            ));
        }
        if STANDARD_USER_FOLDERS
            .iter()
            .any(|name| canonical == home.join(name))
        {
            return Err(format!(
                "{} is a standard user folder, not application storage",
                canonical.display()
            ));
        }
    }
    if let Some(executable) = executable_directory() {
        if executable.starts_with(&canonical) {
            return Err(format!(
                "{} contains the running application",
                canonical.display()
            ));
        }
    }
    Ok(canonical)
}

/// The path removal acts on, resolved without following a final symbolic
/// link.
///
/// Every component but the last is resolved, so a link in the middle of the
/// path cannot smuggle the target outside the root it was checked against.
/// The last component is kept verbatim, so removing a link removes the link
/// and never what it points at.
pub(super) fn removal_target(path: &Path) -> Result<PathBuf, String> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(format!("{} has no parent directory", path.display()));
    };
    let parent = parent
        .canonicalize()
        .map_err(|error| format!("cannot resolve {}: {error}", parent.display()))?;
    Ok(parent.join(name))
}
