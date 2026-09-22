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

#[path = "storage_parts/ownership.rs"]
mod ownership;

use ownership::{admitted, authorize_root, owns_configured_root, removal_target};
pub use ownership::{mark_storage_root, OWNERSHIP_SENTINEL};

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
                    if owns_configured_root(id, &output_root) {
                        children_except(&output_root, &cfd_root)
                    } else {
                        Vec::new()
                    },
                ),
                StorageCategoryId::AirfoilCfdCases => (cfd_root.clone(), admitted(id, &cfd_root)),
                StorageCategoryId::SolverScratch => (temp.to_path_buf(), scratch_directories(temp)),
                StorageCategoryId::DownloadedNavdata => {
                    (navdata_root.clone(), admitted(id, &navdata_root))
                }
                StorageCategoryId::DownloadedTexture => (texture.clone(), admitted(id, &texture)),
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
/// keeps its own category when it lives inside the output directory) and
/// other than [`OWNERSHIP_SENTINEL`], which outlives a clear so the root
/// stays claimed.
fn children_except(root: &Path, except: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut children: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| !same_path(path, except))
        .filter(|path| {
            path.file_name()
                .is_none_or(|name| name != OWNERSHIP_SENTINEL)
        })
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

/// Remove every removable path of `entry` that its authorized root admits.
/// Paths that fail are reported, not retried; a solver still running in one
/// of them keeps its directory.
///
/// Removal is fail-closed and decided here rather than trusted from the
/// inventory that produced the entry. The category root must be a location
/// the application may reclaim ([`authorize_root`]), and each path must
/// resolve to the root itself or to something inside it, compared by path
/// component so a sibling whose name merely starts with the root's name is
/// outside it. Anything else -- a `..` segment, a link aimed away from the
/// root, an entry built against a different configuration -- is refused and
/// left untouched, with the reason recorded in [`ClearOutcome::failed`].
///
/// The root must also be one the application owns
/// ([`owns_configured_root`]): a claimed root, or one carrying evidence this
/// program wrote it. A directory the user merely named in a setting is
/// refused whole, so a mistyped or repointed output location cannot have its
/// contents removed. Clearing an output root re-claims it, because the
/// directory survives its own cleanup.
pub fn clear_storage(entry: &StorageEntry) -> ClearOutcome {
    let mut outcome = ClearOutcome::default();
    if entry.removable.is_empty() {
        return outcome;
    }
    let root = match authorize_root(&entry.root) {
        Ok(root) => root,
        Err(reason) => {
            for path in &entry.removable {
                outcome
                    .failed
                    .push((path.clone(), format!("refused: {reason}")));
            }
            return outcome;
        }
    };
    if !owns_configured_root(entry.id, &entry.root) {
        for path in &entry.removable {
            outcome.failed.push((
                path.clone(),
                format!(
                    "refused: {} is not a storage root this application owns",
                    entry.root.display()
                ),
            ));
        }
        return outcome;
    }
    for path in &entry.removable {
        let target = match removal_target(path) {
            Ok(target) if target.starts_with(&root) => target,
            Ok(target) => {
                outcome.failed.push((
                    path.clone(),
                    format!(
                        "refused: {} is outside {}",
                        target.display(),
                        root.display()
                    ),
                ));
                continue;
            }
            Err(reason) => {
                outcome
                    .failed
                    .push((path.clone(), format!("refused: {reason}")));
                continue;
            }
        };
        let result = match target.symlink_metadata() {
            Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(&target),
            Ok(_) => fs::remove_file(&target),
            Err(error) => Err(error),
        };
        match result {
            Ok(()) => outcome.removed.push(path.clone()),
            Err(error) => outcome.failed.push((path.clone(), error.to_string())),
        }
    }
    if entry.id == StorageCategoryId::GeneratedOutputs && !outcome.removed.is_empty() {
        let _ = mark_storage_root(entry.id, &root);
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
    use super::ownership::home_directory;
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

    fn entry_for(root: &Path, removable: Vec<PathBuf>) -> StorageEntry {
        let id = StorageCategoryId::GeneratedOutputs;
        StorageEntry {
            id,
            label: id.label(),
            description: id.description(),
            root: root.to_path_buf(),
            exists: !removable.is_empty(),
            removable,
            bytes: 0,
            files: 0,
        }
    }

    #[test]
    fn clearing_refuses_a_removable_path_that_resolves_outside_its_root() {
        let fixture = Fixture::new("escape");
        let inside = fixture.write("app/outputs/design_database.json", 10);
        let outside = fixture.write("precious/thesis.txt", 10);
        let root = fixture.root.join("app").join("outputs");
        let escape = root.join("..").join("..").join("precious");

        let outcome = clear_storage(&entry_for(&root, vec![inside.clone(), escape.clone()]));

        assert_eq!(outcome.removed, vec![inside.clone()]);
        assert!(!inside.exists(), "the in-root path was not cleared");
        assert!(outside.is_file(), "cleanup escaped its own root");
        assert_eq!(outcome.failed.len(), 1);
        assert_eq!(outcome.failed[0].0, escape);
        assert!(
            outcome.failed[0].1.starts_with("refused: "),
            "{:?}",
            outcome.failed[0]
        );
    }

    #[test]
    fn clearing_refuses_a_filesystem_root_without_touching_anything() {
        let fixture = Fixture::new("fsroot");
        let kept = fixture.write("app/outputs/design_database.json", 10);
        let filesystem_root = fixture
            .root
            .ancestors()
            .last()
            .expect("every absolute path has a root")
            .to_path_buf();

        let outcome = clear_storage(&entry_for(&filesystem_root, vec![kept.clone()]));

        assert!(outcome.removed.is_empty());
        assert!(kept.is_file(), "a filesystem root was cleared");
        assert_eq!(outcome.failed.len(), 1);
        assert!(
            outcome.failed[0].1.contains("filesystem root"),
            "{:?}",
            outcome.failed[0]
        );
    }

    #[test]
    fn clearing_refuses_the_home_directory_and_its_standard_folders() {
        let fixture = Fixture::new("home");
        let kept = fixture.write("app/outputs/design_database.json", 10);
        let Some(home) = home_directory() else {
            return;
        };
        for root in [home.clone(), home.join("Documents")] {
            if !root.is_dir() {
                continue;
            }
            let outcome = clear_storage(&entry_for(&root, vec![kept.clone()]));
            assert!(outcome.removed.is_empty(), "{} was cleared", root.display());
            assert!(kept.is_file());
            assert_eq!(outcome.failed.len(), 1);
            assert!(
                outcome.failed[0].1.starts_with("refused: "),
                "{:?}",
                outcome.failed[0]
            );
        }
    }

    #[test]
    fn an_output_root_without_evidence_of_ownership_is_not_inventoried() {
        let fixture = Fixture::new("unowned");
        let app = fixture.root.join("app");
        let thesis = fixture.write("app/outputs/thesis.docx", 40);
        let photo = fixture.write("app/outputs/photos/holiday.jpg", 60);
        let locator = ToolLocator::new(&app, fixture.root.join("user"));
        let temp = fixture.root.join("temp");

        let entries = storage_inventory_with_temp(&locator, &locations(), &temp);
        let outputs = entry(&entries, StorageCategoryId::GeneratedOutputs);
        assert_eq!(outputs.root, app.join("outputs"));
        assert!(!outputs.exists, "an unowned root was offered for clearing");
        assert!(outputs.removable.is_empty());
        assert_eq!((outputs.bytes, outputs.files), (0, 0));
        assert!(thesis.is_file() && photo.is_file());
    }

    #[test]
    fn clearing_refuses_a_root_the_application_does_not_own() {
        let fixture = Fixture::new("unowned-clear");
        let thesis = fixture.write("app/outputs/thesis.docx", 40);
        let root = fixture.root.join("app").join("outputs");

        let outcome = clear_storage(&entry_for(&root, vec![thesis.clone()]));

        assert!(outcome.removed.is_empty());
        assert!(thesis.is_file(), "an unowned root was cleared");
        assert_eq!(outcome.failed.len(), 1);
        assert!(
            outcome.failed[0].1.contains("not a storage root"),
            "{:?}",
            outcome.failed[0]
        );
    }

    #[test]
    fn a_claimed_root_is_owned_and_keeps_its_sentinel_through_a_clear() {
        let fixture = Fixture::new("claimed");
        let app = fixture.root.join("app");
        let data = fixture.write("app/outputs/notes.txt", 40);
        let root = app.join("outputs");
        let locator = ToolLocator::new(&app, fixture.root.join("user"));
        let temp = fixture.root.join("temp");
        assert!(
            !owns_configured_root(StorageCategoryId::GeneratedOutputs, &root),
            "unclaimed and unrecognized"
        );

        mark_storage_root(StorageCategoryId::GeneratedOutputs, &root).unwrap();
        assert!(owns_configured_root(
            StorageCategoryId::GeneratedOutputs,
            &root
        ));

        let entries = storage_inventory_with_temp(&locator, &locations(), &temp);
        let outputs = entry(&entries, StorageCategoryId::GeneratedOutputs);
        assert_eq!(outputs.removable, vec![data.clone()]);
        assert_eq!(
            (outputs.bytes, outputs.files),
            (40, 1),
            "the sentinel is not reclaimable storage"
        );

        let outcome = clear_storage(outputs);
        assert_eq!(outcome.removed, vec![data.clone()]);
        assert!(outcome.failed.is_empty());
        assert!(!data.exists());
        assert!(
            root.join(OWNERSHIP_SENTINEL).is_file(),
            "the claim did not survive its own cleanup"
        );
        assert!(owns_configured_root(
            StorageCategoryId::GeneratedOutputs,
            &root
        ));
    }
}
