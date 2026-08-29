// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! External-tool discovery and the typed environment passed to a run.
//!
//! The executable, GUI and pipeline used to each make a different decision
//! about where an optional solver lived. Keeping discovery here means a
//! packaged executable and a development checkout resolve the same way, and a
//! run receives one named value rather than a list of unrelated `Option`s.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Resolved external tools for one pipeline run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunEnvironment {
    /// Directory containing `mset`, `mses`, and `mplot`.
    pub mses_dir: Option<PathBuf>,
    /// NASTRAN executable, when installed and configured.
    pub nastran_exe: Option<PathBuf>,
    /// Patran executable, when installed and configured.
    pub patran_exe: Option<PathBuf>,
    /// Headless OpenVSP AngelScript executable, when installed adjacent to ALAS.
    #[serde(default)]
    pub openvsp_exe: Option<PathBuf>,
    /// Native VSPAERO solver executable from the OpenVSP distribution.
    #[serde(default)]
    pub vspaero_exe: Option<PathBuf>,
    /// Native Athena Vortex Lattice executable.
    #[serde(default)]
    pub avl_exe: Option<PathBuf>,
    /// Explicit FLOWUnsteady adapter launcher (`ALAS_FLOWUNSTEADY_EXE`).
    #[serde(default)]
    pub flowunsteady_exe: Option<PathBuf>,
}

/// User-level persisted locations for optional tools.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolPreferences {
    /// Configured MSES directory.
    pub mses_dir: Option<String>,
    /// Configured NASTRAN executable.
    pub nastran_exe: Option<String>,
    /// Optional MSC solver binary passed through the NASTRAN launcher.
    ///
    /// Student Edition installations can split the visible launcher from the
    /// `analysis.exe` process that receives the deck. This must survive a
    /// desktop restart just like the launcher path, or Setup can appear ready
    /// while the next solve loses the required `a.solver` override.
    #[serde(default)]
    pub nastran_solver: Option<String>,
    /// Directory containing the locally built NASTRAN-95 tree.
    #[serde(default)]
    pub nastran95_dir: Option<String>,
    /// Optional directory containing the local NASTRAN-95 GNU Fortran runtime.
    #[serde(default)]
    pub nastran95_runtime: Option<String>,
    /// Optional short absolute directory for staged NASTRAN-95 rigid formats.
    #[serde(default)]
    pub nastran95_rf_stage: Option<String>,
    /// Optional open-core allocation for the local NASTRAN-95 solver.
    #[serde(default)]
    pub nastran95_open_core_words: Option<String>,
    /// Configured Patran executable.
    pub patran_exe: Option<String>,
    /// OpenVSP installation directory or its headless script runner.
    ///
    /// The script runner and VSPAERO are separate programs in one official
    /// distribution. Retaining their common location lets the pipeline report
    /// a missing runner instead of treating the solver alone as ready.
    #[serde(default)]
    pub openvsp_dir: Option<String>,
    /// Configured Athena Vortex Lattice executable.
    #[serde(default)]
    pub avl_exe: Option<String>,
    /// Directory containing the optional open navigation-data dataset.
    ///
    /// This remains a user preference rather than an installation-relative
    /// default: the data is sizeable, licensed separately, and may be shared
    /// by more than one ALAS installation on the same computer.
    #[serde(default)]
    pub navdata_dir: Option<String>,
    /// Directory containing user-imported and saved flight routes.
    #[serde(default)]
    pub routes_dir: Option<String>,
}

/// Result of inspecting one MSES installation directory.
///
/// MSES is a three-program installation. Reporting which requirement is
/// missing keeps an incomplete adjacent install distinct from a directory
/// that is not present at all, so the caller can show an actionable status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MsesDiscovery {
    /// No candidate directory exists.
    Absent,
    /// The directory exists but one or more required programs are absent.
    Incomplete {
        /// Directory that was inspected.
        directory: PathBuf,
        /// Required program names that were not regular files.
        missing: Vec<String>,
    },
    /// All required MSES programs are present.
    Ready(PathBuf),
}

/// Result of inspecting one executable-based installation.
///
/// An executable path can point at an unpacked but incomplete installation
/// directory. Keeping that state separate from an absent candidate lets the
/// setup page tell the user whether a path is wrong or a bundle is missing a
/// required launcher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutableDiscovery {
    /// No configured or adjacent installation candidate exists.
    Absent,
    /// A candidate installation directory exists but has no usable launcher.
    Incomplete {
        /// Directory that was inspected.
        directory: PathBuf,
        /// Launcher names that were not regular files.
        missing: Vec<String>,
    },
    /// A usable launcher was found.
    Ready(PathBuf),
}

impl ExecutableDiscovery {
    fn ready_path(self) -> Option<PathBuf> {
        match self {
            Self::Ready(path) => Some(path),
            Self::Absent | Self::Incomplete { .. } => None,
        }
    }
}

/// Roots and preference location used to resolve optional tools.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolLocator {
    app_root: PathBuf,
    user_data_root: PathBuf,
    system_tool_roots: Vec<PathBuf>,
}

impl ToolLocator {
    /// Create a locator with explicit roots. This constructor also makes
    /// packaged-path behavior testable without changing process globals.
    pub fn new(app_root: impl Into<PathBuf>, user_data_root: impl Into<PathBuf>) -> Self {
        Self {
            app_root: app_root.into(),
            user_data_root: user_data_root.into(),
            system_tool_roots: Vec::new(),
        }
    }

    /// Build a locator from the running executable and the platform user-data
    /// directory.
    pub fn for_current_process() -> Self {
        let app_root = env::var_os("ALAS_APP_DIR")
            .map(PathBuf::from)
            .filter(|path| path.exists())
            .or_else(|| {
                let cwd = env::current_dir().ok()?;
                (!is_frozen() && cwd.join("Cargo.toml").is_file()).then_some(cwd)
            })
            .or_else(|| {
                env::current_exe()
                    .ok()
                    .and_then(|path| path.parent().map(Path::to_path_buf))
            })
            .or_else(|| env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        let mut locator = Self::new(app_root, platform_user_data_root());
        locator.system_tool_roots = installed_msc_roots();
        locator
    }

    /// Application installation directory used by discovery.
    pub fn app_root(&self) -> &Path {
        &self.app_root
    }

    /// Location of the user-level preferences file.
    pub fn preferences_path(&self) -> PathBuf {
        self.user_data_root.join("tool-preferences.json")
    }

    /// Load persisted locations. A missing or malformed file is equivalent to
    /// no preferences; an optional tool must never prevent the app starting.
    pub fn load_preferences(&self) -> ToolPreferences {
        let path = self.preferences_path();
        let Ok(text) = fs::read_to_string(path) else {
            return ToolPreferences::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Persist locations selected by the user without exposing a partly
    /// written preference file to the next desktop launch.
    pub fn save_preferences(&self, preferences: &ToolPreferences) -> Result<(), String> {
        fs::create_dir_all(&self.user_data_root)
            .map_err(|error| format!("cannot create {}: {error}", self.user_data_root.display()))?;
        let text = serde_json::to_string_pretty(preferences)
            .map_err(|error| format!("cannot encode tool preferences: {error}"))?;
        let destination = self.preferences_path();
        let temporary =
            destination.with_file_name(format!(".tool-preferences-{}.tmp", std::process::id()));
        fs::write(&temporary, text)
            .map_err(|error| format!("cannot write {}: {error}", temporary.display()))?;
        fs::rename(&temporary, &destination).map_err(|error| {
            let _ = fs::remove_file(&temporary);
            format!("cannot replace {}: {error}", destination.display())
        })
    }

    /// Resolve configured locations and discover adjacent bundled tools.
    pub fn resolve_environment(
        &self,
        mses_dir: &Path,
        nastran_exe: &Path,
        patran_exe: &Path,
        openvsp_dir: &Path,
        avl_exe: &Path,
    ) -> RunEnvironment {
        RunEnvironment {
            mses_dir: self.resolve_mses_dir(mses_dir),
            nastran_exe: self.discover_nastran(nastran_exe).ready_path(),
            patran_exe: self.discover_patran(patran_exe).ready_path(),
            openvsp_exe: self.discover_openvsp(openvsp_dir).ready_path(),
            vspaero_exe: self.discover_vspaero(openvsp_dir).ready_path(),
            avl_exe: self.discover_avl(avl_exe).ready_path(),
            flowunsteady_exe: env::var_os("ALAS_FLOWUNSTEADY_EXE")
                .map(PathBuf::from)
                .filter(|path| path.is_file()),
        }
    }

    /// Inspect the configured and adjacent MSC Nastran installation.
    pub fn discover_nastran(&self, configured: &Path) -> ExecutableDiscovery {
        let adjacent = self.discover_executable(
            configured,
            &["nastran.exe", "nastran"],
            &["NASTRAN", "Nastran", "nastran"],
        );
        if matches!(adjacent, ExecutableDiscovery::Ready(_)) {
            return adjacent;
        }
        self.discover_msc_executable("Nastran/bin", &["nastran.exe", "nastranw.exe"])
            .unwrap_or(adjacent)
    }

    /// Inspect the configured and adjacent MSC Patran installation.
    pub fn discover_patran(&self, configured: &Path) -> ExecutableDiscovery {
        let adjacent = self.discover_executable(
            configured,
            &["patran.exe", "patran"],
            &["Patran", "PATRAN", "patran"],
        );
        if matches!(adjacent, ExecutableDiscovery::Ready(_)) {
            return adjacent;
        }
        self.discover_msc_executable("Patran/bin", &["patran.exe"])
            .unwrap_or(adjacent)
    }

    /// Inspect a configured or adjacent headless OpenVSP installation.
    ///
    /// The official Windows distribution calls the batch runner
    /// `vspscript.exe`. The interactive `vsp.exe` also accepts `-script`, but
    /// is deliberately not selected automatically because it loads the GUI
    /// and graphics stack for a pipeline operation that does not need either.
    pub fn discover_openvsp(&self, configured: &Path) -> ExecutableDiscovery {
        self.discover_executable(
            configured,
            &["vspscript.exe", "vspscript"],
            &["OpenVSP", "openvsp"],
        )
    }

    /// Inspect a configured or adjacent native VSPAERO installation.
    pub fn discover_vspaero(&self, configured: &Path) -> ExecutableDiscovery {
        self.discover_executable(
            configured,
            &["vspaero.exe", "vspaero"],
            &["OpenVSP", "openvsp"],
        )
    }

    /// Inspect a configured or adjacent native Athena Vortex Lattice install.
    pub fn discover_avl(&self, configured: &Path) -> ExecutableDiscovery {
        // The official Windows executable is commonly versioned (`avl352.exe`)
        // and the ALAS external-tools bundle uses that name.  Keep the generic
        // names for user installations, but recognize the bundled binary
        // without requiring a manual path entry.
        self.discover_executable(
            configured,
            &["avl352.exe", "avl.exe", "avl352", "avl"],
            &["AVL", "avl"],
        )
    }

    /// Resolve an MSES directory, preferring an explicit configured path and
    /// then the conventional `external tools/MSES` beside the executable.
    pub fn resolve_mses_dir(&self, configured: &Path) -> Option<PathBuf> {
        match self.discover_mses(configured) {
            MsesDiscovery::Ready(path) => Some(path),
            MsesDiscovery::Absent | MsesDiscovery::Incomplete { .. } => None,
        }
    }

    /// Inspect the configured MSES directory and then adjacent packaged
    /// locations, returning the most useful installation state.
    ///
    /// A complete configured directory wins. If no complete directory exists,
    /// an existing incomplete candidate is reported before the caller falls
    /// back to the ordinary absent state.
    pub fn discover_mses(&self, configured: &Path) -> MsesDiscovery {
        let mut incomplete = None;
        for candidate in self.mses_candidates(configured) {
            if is_mses_dir(&candidate) {
                return MsesDiscovery::Ready(candidate);
            }
            if candidate.is_dir() && incomplete.is_none() {
                let missing = missing_mses_programs(&candidate);
                incomplete = Some(MsesDiscovery::Incomplete {
                    directory: candidate,
                    missing,
                });
            }
        }
        incomplete.unwrap_or(MsesDiscovery::Absent)
    }

    /// Resolve a configured executable, then search the adjacent tools folder.
    pub fn resolve_executable(
        &self,
        configured: &Path,
        names: &[&str],
        tool_directories: &[&str],
    ) -> Option<PathBuf> {
        match self.discover_executable(configured, names, tool_directories) {
            ExecutableDiscovery::Ready(path) => Some(path),
            ExecutableDiscovery::Absent | ExecutableDiscovery::Incomplete { .. } => None,
        }
    }

    /// Inspect a configured executable and the conventional adjacent tool
    /// directories without collapsing an incomplete bundle into `Absent`.
    pub fn discover_executable(
        &self,
        configured: &Path,
        names: &[&str],
        tool_directories: &[&str],
    ) -> ExecutableDiscovery {
        let mut incomplete = None;
        let mut candidates = Vec::new();
        if !configured.as_os_str().is_empty() {
            if let Some(path) = self.resolve_file(configured) {
                return ExecutableDiscovery::Ready(path);
            }
            if let Some(directory) = self.resolve_directory(configured) {
                candidates.push(directory);
            }
        }

        let tools = self.app_root.join("external tools");
        if let Some(path) = names
            .iter()
            .map(|name| tools.join(name))
            .find(|path| path.is_file())
        {
            return ExecutableDiscovery::Ready(path);
        }
        for directory in tool_directories {
            let candidate = tools.join(directory);
            if !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
            for versioned in versioned_directories(&tools, directory) {
                if !candidates.contains(&versioned) {
                    candidates.push(versioned);
                }
            }
        }
        for candidate in candidates {
            let found = names
                .iter()
                .map(|name| candidate.join(name))
                .find(|path| path.is_file());
            if let Some(path) = found {
                return ExecutableDiscovery::Ready(path);
            }
            if candidate.is_dir() && incomplete.is_none() {
                let missing = names
                    .iter()
                    .filter(|name| !candidate.join(name).is_file())
                    .map(|name| (*name).to_owned())
                    .collect();
                incomplete = Some(ExecutableDiscovery::Incomplete {
                    directory: candidate,
                    missing,
                });
            }
        }
        incomplete.unwrap_or(ExecutableDiscovery::Absent)
    }

    fn resolve_directory(&self, configured: &Path) -> Option<PathBuf> {
        if configured.as_os_str().is_empty() {
            return None;
        }
        if configured.is_absolute() {
            return configured.is_dir().then(|| configured.to_path_buf());
        }
        self.candidate_roots()
            .into_iter()
            .map(|root| root.join(configured))
            .find(|path| path.is_dir())
    }

    fn discover_msc_executable(
        &self,
        relative_directory: &str,
        names: &[&str],
    ) -> Option<ExecutableDiscovery> {
        for root in &self.system_tool_roots {
            let directory = root.join(relative_directory);
            if let Some(path) = names
                .iter()
                .map(|name| directory.join(name))
                .find(|path| path.is_file())
            {
                return Some(ExecutableDiscovery::Ready(path));
            }
            if directory.is_dir() {
                return Some(ExecutableDiscovery::Incomplete {
                    directory,
                    missing: names.iter().map(|name| (*name).to_owned()).collect(),
                });
            }
        }
        None
    }

    fn mses_candidates(&self, configured: &Path) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(path) = self.resolve_directory(configured) {
            candidates.push(path);
        }
        for name in ["MSES", "mses"] {
            let path = self.app_root.join("external tools").join(name);
            if !candidates.contains(&path) {
                candidates.push(path);
            }
        }
        for path in versioned_directories(&self.app_root.join("external tools"), "mses") {
            if !candidates.contains(&path) {
                candidates.push(path);
            }
        }
        candidates
    }

    fn resolve_file(&self, configured: &Path) -> Option<PathBuf> {
        if configured.as_os_str().is_empty() {
            return None;
        }
        if configured.is_absolute() {
            return configured.is_file().then(|| configured.to_path_buf());
        }
        self.candidate_roots()
            .into_iter()
            .map(|root| root.join(configured))
            .find(|path| path.is_file())
    }

    fn candidate_roots(&self) -> Vec<PathBuf> {
        let mut roots = vec![self.app_root.clone(), self.app_root.join("bin")];
        if let Some(parent) = self.app_root.parent() {
            roots.push(parent.to_path_buf());
        }
        if !roots.contains(&self.user_data_root) {
            roots.push(self.user_data_root.clone());
        }
        roots
    }
}

fn versioned_directories(root: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let prefix = prefix.to_ascii_lowercase();
    let mut matches = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.to_ascii_lowercase().starts_with(&prefix))
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    matches
}

fn is_mses_dir(path: &Path) -> bool {
    ["mset.exe", "mses.exe", "mplot.exe"]
        .iter()
        .all(|name| path.join(name).is_file())
}

fn missing_mses_programs(path: &Path) -> Vec<String> {
    ["mset.exe", "mses.exe", "mplot.exe"]
        .iter()
        .filter(|name| !path.join(name).is_file())
        .map(|name| (*name).to_owned())
        .collect()
}

fn is_frozen() -> bool {
    env::var("ALAS_FROZEN")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn platform_user_data_root() -> PathBuf {
    #[cfg(target_os = "windows")]
    if let Some(root) = env::var_os("LOCALAPPDATA") {
        return PathBuf::from(root).join("ALAS");
    }
    #[cfg(target_os = "macos")]
    if let Some(root) = env::var_os("HOME") {
        return PathBuf::from(root)
            .join("Library")
            .join("Application Support")
            .join("ALAS");
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    if let Some(root) = env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(root).join("ALAS");
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    if let Some(root) = env::var_os("HOME") {
        return PathBuf::from(root)
            .join(".local")
            .join("share")
            .join("ALAS");
    }
    PathBuf::from(".")
}

fn installed_msc_roots() -> Vec<PathBuf> {
    #[cfg(target_os = "windows")]
    let Some(app_data) = env::var_os("APPDATA") else {
        return Vec::new();
    };
    #[cfg(not(target_os = "windows"))]
    return Vec::new();
    #[cfg(target_os = "windows")]
    let editions = PathBuf::from(app_data)
        .join("MSC.Software")
        .join("MSC Nastran and Patran Student Editions");
    #[cfg(target_os = "windows")]
    let Ok(entries) = fs::read_dir(editions) else {
        return Vec::new();
    };
    #[cfg(target_os = "windows")]
    {
        let mut roots = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>();
        roots.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
        roots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_external_tools_are_discovered_without_configuration() {
        let root = std::env::temp_dir().join(format!("alas-tools-{}", std::process::id()));
        let tools = root.join("external tools/MSES");
        let _ = fs::create_dir_all(&tools);
        let _ = fs::write(tools.join("mset.exe"), b"test");
        let _ = fs::write(tools.join("mses.exe"), b"test");
        let _ = fs::write(tools.join("mplot.exe"), b"test");
        let locator = ToolLocator::new(&root, root.join("prefs"));
        assert_eq!(locator.resolve_mses_dir(Path::new("")), Some(tools));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn incomplete_mses_directory_is_not_reported_as_an_installation() {
        let root =
            std::env::temp_dir().join(format!("alas-incomplete-mses-{}", std::process::id()));
        let tools = root.join("external tools/MSES");
        let _ = fs::create_dir_all(&tools);
        let _ = fs::write(tools.join("mplot.exe"), b"test");
        let locator = ToolLocator::new(&root, root.join("prefs"));

        assert_eq!(locator.resolve_mses_dir(Path::new("")), None);
        assert_eq!(locator.resolve_mses_dir(&tools), None);
        assert_eq!(
            locator.discover_mses(&tools),
            MsesDiscovery::Incomplete {
                directory: tools.clone(),
                missing: vec!["mset.exe".to_owned(), "mses.exe".to_owned()],
            }
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn missing_mses_directory_is_reported_as_absent() {
        let root = std::env::temp_dir().join(format!("alas-absent-mses-{}", std::process::id()));
        let locator = ToolLocator::new(&root, root.join("prefs"));
        assert_eq!(
            locator.discover_mses(Path::new("missing/MSES")),
            MsesDiscovery::Absent
        );
    }

    #[test]
    fn executables_are_discovered_in_conventional_tool_subdirectories() {
        let root = std::env::temp_dir().join(format!("alas-executables-{}", std::process::id()));
        let mses = root.join("external tools/MSES");
        let nastran = root.join("external tools/NASTRAN/nastran.exe");
        let patran = root.join("external tools/Patran/patran.exe");
        let openvsp = root.join("external tools/OpenVSP/vspscript.exe");
        let vspaero = root.join("external tools/OpenVSP/vspaero.exe");
        let avl = root.join("external tools/AVL/avl.exe");
        let _ = fs::create_dir_all(&mses);
        let _ = fs::create_dir_all(nastran.parent().unwrap_or(Path::new(".")));
        let _ = fs::create_dir_all(patran.parent().unwrap_or(Path::new(".")));
        let _ = fs::create_dir_all(openvsp.parent().unwrap_or(Path::new(".")));
        let _ = fs::write(mses.join("mset.exe"), b"test");
        let _ = fs::write(mses.join("mses.exe"), b"test");
        let _ = fs::write(mses.join("mplot.exe"), b"test");
        let _ = fs::write(&nastran, b"test");
        let _ = fs::write(&patran, b"test");
        let _ = fs::write(&openvsp, b"test");
        let _ = fs::write(&vspaero, b"test");
        let _ = fs::create_dir_all(avl.parent().unwrap_or(Path::new(".")));
        let _ = fs::write(&avl, b"test");

        let locator = ToolLocator::new(&root, root.join("prefs"));
        let environment = locator.resolve_environment(
            Path::new(""),
            Path::new(""),
            Path::new(""),
            Path::new(""),
            Path::new(""),
        );

        assert_eq!(environment.mses_dir, Some(mses));
        assert_eq!(environment.nastran_exe, Some(nastran));
        assert_eq!(environment.patran_exe, Some(patran));
        assert_eq!(environment.openvsp_exe, Some(openvsp));
        assert_eq!(environment.vspaero_exe, Some(vspaero));
        assert_eq!(environment.avl_exe, Some(avl));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn incomplete_msc_installations_are_distinct_from_absent_tools() {
        let root = std::env::temp_dir().join(format!("alas-incomplete-msc-{}", std::process::id()));
        let nastran_dir = root.join("external tools/NASTRAN");
        let patran_dir = root.join("external tools/Patran");
        let _ = fs::create_dir_all(&nastran_dir);
        let _ = fs::create_dir_all(&patran_dir);
        let locator = ToolLocator::new(&root, root.join("prefs"));

        assert_eq!(
            locator.discover_executable(Path::new(""), &["nastran.exe", "nastran"], &["NASTRAN"]),
            ExecutableDiscovery::Incomplete {
                directory: nastran_dir,
                missing: vec!["nastran.exe".to_owned(), "nastran".to_owned()],
            }
        );
        assert_eq!(
            locator.discover_executable(
                Path::new("missing/patran.exe"),
                &["patran.exe", "patran"],
                &["Patran"]
            ),
            ExecutableDiscovery::Incomplete {
                directory: patran_dir,
                missing: vec!["patran.exe".to_owned(), "patran".to_owned()],
            }
        );
        let _ = fs::remove_dir_all(root);
    }
}

#[cfg(test)]
#[path = "tools_external_tests.rs"]
mod external_tests;
