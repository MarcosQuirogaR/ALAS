// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Inventory of the storage the application owns and may reclaim.
//!
//! Manage Storage (clarified GUI section 1) shows each category with its
//! location and removable size and clears one category at a time. The
//! categories are the locations the code actually writes:
//!
//! * generated outputs: the resolved output directory of a run
//!   (`design_database.json`, `structures/` with the NASTRAN scratch beside
//!   its deck, `mses/`, `openvsp/`, `avl/`, `cpacs/`, ...), excluding the
//!   Airfoil CFD case root when it lives inside it;
//! * Airfoil CFD cases: the CFD study root (OpenFOAM cases included);
//! * solver scratch: directories in the system temporary directory whose
//!   names carry an ALAS prefix (MSES work directories, pipeline runs
//!   without an output directory); they are removed when a run finishes
//!   and are left behind only when the process dies;
//! * downloaded navigation data and the downloaded globe texture, which
//!   the application fetches again on demand.
//!
//! Tool installations, saved routes, saved sandbox documents and the
//! saved tool paths are not temporary data; resetting the saved tool paths
//! is a separate configuration action ([`reset_tool_preferences`]).

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::tools::ToolLocator;

/// A reclaimable storage category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum StorageCategoryId {
    /// The run output directory.
    GeneratedOutputs,
    /// The Airfoil CFD case root.
    AirfoilCfdCases,
    /// ALAS-prefixed directories in the system temporary directory.
    SolverScratch,
    /// Downloaded navigation data.
    DownloadedNavdata,
    /// The downloaded globe texture.
    DownloadedTexture,
}

impl StorageCategoryId {
    /// Every category, in display order.
    pub const ALL: [Self; 5] = [
        Self::GeneratedOutputs,
        Self::AirfoilCfdCases,
        Self::SolverScratch,
        Self::DownloadedNavdata,
        Self::DownloadedTexture,
    ];

    /// Stable identifier.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GeneratedOutputs => "generated_outputs",
            Self::AirfoilCfdCases => "airfoil_cfd_cases",
            Self::SolverScratch => "solver_scratch",
            Self::DownloadedNavdata => "downloaded_navdata",
            Self::DownloadedTexture => "downloaded_texture",
        }
    }

    /// Short English label (translated by the front end).
    pub fn label(self) -> &'static str {
        match self {
            Self::GeneratedOutputs => "Generated outputs",
            Self::AirfoilCfdCases => "Airfoil CFD cases",
            Self::SolverScratch => "Solver scratch files",
            Self::DownloadedNavdata => "Downloaded navigation data",
            Self::DownloadedTexture => "Downloaded globe texture",
        }
    }

    /// One sentence on what the category holds and what clearing it costs.
    pub fn description(self) -> &'static str {
        match self {
            Self::GeneratedOutputs => "Results, exports and solver files written by runs; the next run recreates what it needs.",
            Self::AirfoilCfdCases => "Airfoil CFD studies with their meshes, solver logs and results; clearing removes every saved study.",
            Self::SolverScratch => "Work directories external solvers left in the system temporary folder after an interrupted run.",
            Self::DownloadedNavdata => "Navigation data for airway routing; downloaded again on demand.",
            Self::DownloadedTexture => "Earth image for the route globe; downloaded again on demand.",
        }
    }
}

/// Prefixes of the scratch directories the application creates in the
/// system temporary directory.
pub const SCRATCH_PREFIXES: [&str; 2] = ["alas_mses_", "alas-analysis-"];

/// The configured locations the inventory resolves.
#[derive(Debug, Clone, Copy)]
pub struct StorageLocations<'a> {
    /// The run output directory (relative paths resolve against the data roots).
    pub output_dir: &'a Path,
    /// The Airfoil CFD case root.
    pub cfd_case_root: &'a Path,
    /// `mission.navdata_dir`.
    pub navdata_dir: &'a Path,
    /// `mission.texture_path`.
    pub texture_path: &'a Path,
}

/// One category with its location and measured size.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StorageEntry {
    /// Which category.
    pub id: StorageCategoryId,
    /// [`StorageCategoryId::label`].
    pub label: &'static str,
    /// [`StorageCategoryId::description`].
    pub description: &'static str,
    /// Where the category lives.
    pub root: PathBuf,
    /// The paths clearing removes: the root, its children, or the matching
    /// scratch directories.
    pub removable: Vec<PathBuf>,
    /// Whether anything removable exists.
    pub exists: bool,
    /// Removable bytes on disk.
    pub bytes: u64,
    /// Removable files.
    pub files: u64,
}

/// Measure every category at its resolved location.
pub fn storage_inventory(locator: &ToolLocator, locations: &StorageLocations) -> Vec<StorageEntry> {
    storage_inventory_with_temp(locator, locations, &std::env::temp_dir())
}

/// [`storage_inventory`] with an explicit temporary directory for the
/// solver-scratch category.
pub fn storage_inventory_with_temp(
    locator: &ToolLocator,
    locations: &StorageLocations,
    temp: &Path,
) -> Vec<StorageEntry> {
    let output_root = locator.resolve_data_path(locations.output_dir);
    let cfd_root = locator.resolve_data_path(locations.cfd_case_root);
    let navdata_root = locator.resolve_data_path(locations.navdata_dir);
    let texture = locator.resolve_data_path(locations.texture_path);
    StorageCategoryId::ALL
        .iter()
        .map(|&id| {
            let (root, removable) = match id {
                StorageCategoryId::GeneratedOutputs => (
                    output_root.clone(),
                    children_except(&output_root, &cfd_root),
                ),
                StorageCategoryId::AirfoilCfdCases => (cfd_root.clone(), vec![cfd_root.clone()]),
                StorageCategoryId::SolverScratch => (temp.to_path_buf(), scratch_directories(temp)),
                StorageCategoryId::DownloadedNavdata => {
                    (navdata_root.clone(), vec![navdata_root.clone()])
                }
                StorageCategoryId::DownloadedTexture => (texture.clone(), vec![texture.clone()]),
            };
            let removable: Vec<PathBuf> = removable
                .into_iter()
                .filter(|path| path.symlink_metadata().is_ok())
                .collect();
            let (bytes, files) = removable
                .iter()
                .map(|path| measure(path))
                .fold((0, 0), |acc, item| (acc.0 + item.0, acc.1 + item.1));
            StorageEntry {
                id,
                label: id.label(),
                description: id.description(),
                root,
                exists: !removable.is_empty(),
                removable,
                bytes,
                files,
            }
        })
        .collect()
}

/// The ALAS scratch directories inside `temp`.
pub fn scratch_directories(temp: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(temp) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            SCRATCH_PREFIXES
                .iter()
                .any(|prefix| name.starts_with(prefix))
        })
        .map(|entry| entry.path())
        .collect();
    found.sort();
    found
}

/// The direct children of `root` other than `except` (the CFD case root
/// keeps its own category when it lives inside the output directory).
fn children_except(root: &Path, except: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut children: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| !same_path(path, except))
        .collect();
    children.sort();
    children
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// `(bytes, files)` under `path`, without following symbolic links.
pub fn measure(path: &Path) -> (u64, u64) {
    let Ok(metadata) = path.symlink_metadata() else {
        return (0, 0);
    };
    if metadata.is_file() {
        return (metadata.len(), 1);
    }
    if !metadata.is_dir() {
        return (0, 0);
    }
    let Ok(entries) = fs::read_dir(path) else {
        return (0, 0);
    };
    entries
        .flatten()
        .map(|entry| measure(&entry.path()))
        .fold((0, 0), |acc, item| (acc.0 + item.0, acc.1 + item.1))
}

/// What clearing a category did.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct ClearOutcome {
    /// Paths removed.
    pub removed: Vec<PathBuf>,
    /// Paths that could not be removed, with the error.
    pub failed: Vec<(PathBuf, String)>,
}

/// Remove every removable path of `entry`. Paths that fail are reported,
/// not retried; a solver still running in one of them keeps its directory.
pub fn clear_storage(entry: &StorageEntry) -> ClearOutcome {
    let mut outcome = ClearOutcome::default();
    for path in &entry.removable {
        let result = match path.symlink_metadata() {
            Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path),
            Ok(_) => fs::remove_file(path),
            Err(error) => Err(error),
        };
        match result {
            Ok(()) => outcome.removed.push(path.clone()),
            Err(error) => outcome.failed.push((path.clone(), error.to_string())),
        }
    }
    outcome
}

/// Forget the saved external-tool paths: remove the preferences file so the
/// next launch starts from discovery. This is a configuration reset, not a
/// storage cleanup; the tools themselves are untouched. Returns whether a
/// file existed.
///
/// # Errors
///
/// The file exists and cannot be removed.
pub fn reset_tool_preferences(locator: &ToolLocator) -> Result<bool, String> {
    let path = locator.preferences_path();
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("cannot remove {}: {error}", path.display())),
    }
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn new(label: &str) -> Self {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let root = std::env::temp_dir().join(format!(
                "alas-storage-test-{label}-{}-{stamp}",
                std::process::id()
            ));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn write(&self, relative: &str, bytes: usize) -> PathBuf {
            let path = self.root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, vec![b'x'; bytes]).unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn entry(entries: &[StorageEntry], id: StorageCategoryId) -> &StorageEntry {
        entries.iter().find(|entry| entry.id == id).unwrap()
    }

    fn locations() -> StorageLocations<'static> {
        StorageLocations {
            output_dir: Path::new("outputs"),
            cfd_case_root: Path::new("outputs/airfoil-cfd"),
            navdata_dir: Path::new("alas/data/navdata"),
            texture_path: Path::new("alas/data/textures/earth.jpg"),
        }
    }

    #[test]
    fn every_category_reports_its_own_location_and_removable_size() {
        let fixture = Fixture::new("inventory");
        let app = fixture.root.join("app");
        fixture.write("app/outputs/design_database.json", 10);
        fixture.write("app/outputs/structures/case.bdf", 20);
        fixture.write("app/outputs/airfoil-cfd/study-1/study.json", 40);
        fixture.write("app/alas/data/navdata/airways.dat", 7);
        fixture.write("app/alas/data/textures/earth.jpg", 3);
        let temp = fixture.root.join("temp");
        fixture.write("temp/alas_mses_123_1_0/mses.dat", 100);
        fixture.write("temp/alas-analysis-9-1/log.txt", 5);
        fixture.write("temp/other-app-scratch/file", 1000);
        fixture.write("temp/alas_mses_plain_file", 1000);
        let locator = ToolLocator::new(&app, fixture.root.join("user"));
        let entries = storage_inventory_with_temp(&locator, &locations(), &temp);
        assert_eq!(entries.len(), StorageCategoryId::ALL.len());

        let outputs = entry(&entries, StorageCategoryId::GeneratedOutputs);
        assert_eq!(outputs.root, app.join("outputs"));
        assert!(outputs.exists);
        assert_eq!((outputs.bytes, outputs.files), (30, 2), "{outputs:?}");
        assert!(!outputs.removable.iter().any(|p| p.ends_with("airfoil-cfd")));

        let cfd = entry(&entries, StorageCategoryId::AirfoilCfdCases);
        assert_eq!((cfd.bytes, cfd.files), (40, 1));
        assert_eq!(cfd.removable, vec![app.join("outputs").join("airfoil-cfd")]);

        let scratch = entry(&entries, StorageCategoryId::SolverScratch);
        assert_eq!(scratch.root, temp);
        assert_eq!((scratch.bytes, scratch.files), (105, 2), "{scratch:?}");
        assert_eq!(scratch.removable.len(), 2);

        let navdata = entry(&entries, StorageCategoryId::DownloadedNavdata);
        assert_eq!((navdata.bytes, navdata.files), (7, 1));
        let texture = entry(&entries, StorageCategoryId::DownloadedTexture);
        assert_eq!((texture.bytes, texture.files), (3, 1));
        assert_eq!(
            texture.removable,
            vec![app.join("alas/data/textures/earth.jpg")]
        );
        for entry in &entries {
            assert!(!entry.label.is_empty() && !entry.description.is_empty());
            assert!(!entry.id.as_str().is_empty());
        }
    }

    #[test]
    fn clearing_removes_only_the_category_and_missing_locations_are_reported_absent() {
        let fixture = Fixture::new("clear");
        let app = fixture.root.join("app");
        fixture.write("app/outputs/design_database.json", 10);
        fixture.write("app/outputs/airfoil-cfd/study-1/study.json", 40);
        let temp = fixture.root.join("temp");
        fixture.write("temp/alas_mses_1_1_0/mses.dat", 100);
        fixture.write("temp/keep-me/file", 1);
        let locator = ToolLocator::new(&app, fixture.root.join("user"));
        let entries = storage_inventory_with_temp(&locator, &locations(), &temp);
        let navdata = entry(&entries, StorageCategoryId::DownloadedNavdata);
        assert!(!navdata.exists && navdata.removable.is_empty() && navdata.bytes == 0);

        let outcome = clear_storage(entry(&entries, StorageCategoryId::GeneratedOutputs));
        assert_eq!(outcome.removed.len(), 1);
        assert!(outcome.failed.is_empty());
        assert!(!app.join("outputs/design_database.json").exists());
        assert!(app.join("outputs/airfoil-cfd/study-1/study.json").exists());

        let outcome = clear_storage(entry(&entries, StorageCategoryId::SolverScratch));
        assert_eq!(outcome.removed.len(), 1);
        assert!(!temp.join("alas_mses_1_1_0").exists());
        assert!(temp.join("keep-me/file").exists());

        let again = storage_inventory_with_temp(&locator, &locations(), &temp);
        assert!(!entry(&again, StorageCategoryId::GeneratedOutputs).exists);
        assert!(entry(&again, StorageCategoryId::AirfoilCfdCases).exists);
    }

    #[test]
    fn resetting_tool_preferences_removes_the_file_and_nothing_else() {
        let fixture = Fixture::new("reset");
        let user = fixture.root.join("user");
        let locator = ToolLocator::new(fixture.root.join("app"), &user);
        assert_eq!(reset_tool_preferences(&locator), Ok(false));
        locator
            .save_preferences(&crate::ToolPreferences::default())
            .unwrap();
        fixture.write("user/keep.json", 2);
        assert!(locator.preferences_path().is_file());
        assert_eq!(reset_tool_preferences(&locator), Ok(true));
        assert!(!locator.preferences_path().exists());
        assert!(user.join("keep.json").is_file());
        assert_eq!(reset_tool_preferences(&locator), Ok(false));
    }
}
