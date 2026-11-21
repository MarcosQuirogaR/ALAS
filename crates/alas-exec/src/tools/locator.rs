// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

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

    /// The per-user writable data folder: the one default location for saved
    /// configurations, user aircraft, custom airports, and exports.
    pub fn user_data_root(&self) -> &Path {
        &self.user_data_root
    }

    /// Location of the user-level preferences file.
    pub fn preferences_path(&self) -> PathBuf {
        self.user_data_root.join("tool-preferences.json")
    }

    /// The user-level file the custom-airport registry is kept in.
    ///
    /// The registry itself is in-memory, and a saved workspace carries a copy,
    /// but an airport entered once is expected to survive a restart on its
    /// own, in the desktop application and headless alike. It sits beside the
    /// tool preferences, so one user-data location holds both.
    pub fn custom_airports_path(&self) -> PathBuf {
        self.user_data_root.join("custom-airports.json")
    }

    /// The `library` folder of the user data root: one sub-folder per kind of
    /// user-saved object (airports, engines, cabins).
    pub fn library_root(&self) -> PathBuf {
        self.user_data_root.join("library")
    }

    /// Custom airports, one `<ICAO>.json` file per airport.
    pub fn my_airports_dir(&self) -> PathBuf {
        self.library_root().join("my-airports")
    }

    /// User-saved engines.
    pub fn my_engines_dir(&self) -> PathBuf {
        self.library_root().join("my-engines")
    }

    /// User-saved cabin layouts.
    pub fn my_cabins_dir(&self) -> PathBuf {
        self.library_root().join("my-cabins")
    }

    /// Resolve a relative writable/data path against the same roots used by
    /// the packaged application.  The pipeline cannot depend on `alas-app`
    /// without creating a crate cycle, so this small policy lives with the
    /// process/environment locator instead of being duplicated at each call
    /// site.
    pub fn resolve_data_path(&self, configured: &Path) -> PathBuf {
        if configured.is_absolute() {
            return configured.to_path_buf();
        }
        for root in [&self.app_root, &self.user_data_root] {
            let candidate = root.join(configured);
            if candidate.exists() {
                return candidate;
            }
        }
        self.user_data_root.join(configured)
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
    ///
    /// `ALAS_TOOL_DISCOVERY=disabled` supplies an empty environment for
    /// isolated runs, including tools with explicit configured paths.
    pub fn resolve_environment(
        &self,
        mses_dir: &Path,
        nastran_exe: &Path,
        patran_exe: &Path,
        openvsp_dir: &Path,
        avl_exe: &Path,
    ) -> RunEnvironment {
        self.resolve_environment_with_discovery(
            mses_dir,
            nastran_exe,
            patran_exe,
            openvsp_dir,
            avl_exe,
            tool_discovery_enabled(),
        )
    }

    pub(super) fn resolve_environment_with_discovery(
        &self,
        mses_dir: &Path,
        nastran_exe: &Path,
        patran_exe: &Path,
        openvsp_dir: &Path,
        avl_exe: &Path,
        discovery_enabled: bool,
    ) -> RunEnvironment {
        if !discovery_enabled {
            return RunEnvironment::default();
        }
        let nastran_exe = self.discover_nastran(nastran_exe).ready_path();
        let nastran_solver = nastran_exe
            .as_deref()
            .and_then(|launcher| solver_for_launcher(launcher).ready_path());
        RunEnvironment {
            mses_dir: self.resolve_mses_dir(mses_dir),
            nastran_exe,
            nastran_solver,
            patran_exe: self.discover_patran(patran_exe).ready_path(),
            openvsp_exe: self.discover_openvsp(openvsp_dir).ready_path(),
            vspaero_exe: self.discover_vspaero(openvsp_dir).ready_path(),
            avl_exe: self.discover_avl(avl_exe).ready_path(),
            // No caller threads a configured path through here: unlike every
            // other tool resolved above, FLOWUnsteady has no adjacent-tool
            // discovery convention (no bundled or well-known install layout
            // to search), so the persisted preference is read directly
            // rather than adding a parameter with nothing else to combine it
            // with. The GUI already persists this preference to disk on
            // every edit (`save_direct_tool_preferences`), so this reflects
            // the current value.
            flowunsteady_exe: self
                .discover_flowunsteady(Path::new(
                    self.load_preferences()
                        .flowunsteady_exe
                        .as_deref()
                        .unwrap_or(""),
                ))
                .ready_path(),
        }
    }

    /// Resolve an MSES directory, preferring an explicit configured path and
    /// then the conventional `external tools/MSES` beside the executable.
    pub fn resolve_mses_dir(&self, configured: &Path) -> Option<PathBuf> {
        self.discover_mses(configured)
            .ready_path()
            .map(Path::to_path_buf)
    }

    /// Inspect the configured MSES directory and then adjacent packaged
    /// locations, returning the most useful installation state.
    ///
    /// A complete configured directory wins. If no complete directory exists,
    /// an existing incomplete candidate is reported before the caller falls
    /// back to the ordinary absent state.
    pub fn discover_mses(&self, configured: &Path) -> MsesDiscovery {
        let mut incomplete = None;
        for (candidate, source) in self.mses_candidates(configured) {
            if is_mses_dir(&candidate) {
                return MsesDiscovery::Ready {
                    directory: candidate,
                    source,
                };
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
                if let Some(path) = names
                    .iter()
                    .map(|name| directory.join(name))
                    .find(|path| path.is_file())
                {
                    return ExecutableDiscovery::Ready(path);
                }
                candidates.push(directory);
            }
        }

        for root in self.adjacent_tool_roots() {
            let tools = root.join("external tools");
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

    pub(super) fn resolve_directory(&self, configured: &Path) -> Option<PathBuf> {
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

    pub(super) fn discover_msc_executable(
        &self,
        relative_directory: &str,
        names: &[&str],
        prefer_embedded_nastran: bool,
    ) -> Option<ExecutableDiscovery> {
        let mut incomplete = None;
        for root in &self.system_tool_roots {
            // App-data editions can require the inner launcher for their
            // side-by-side runtime; Program Files editions use the bin entry.
            let is_napa = root
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case("NaPa_SE"));
            if prefer_embedded_nastran && !is_napa {
                if let Some(path) = find_embedded_nastran(root) {
                    return Some(ExecutableDiscovery::Ready(path));
                }
            }
            let directory = root.join(relative_directory);
            if let Some(path) = names
                .iter()
                .map(|name| directory.join(name))
                .find(|path| path.is_file())
            {
                return Some(ExecutableDiscovery::Ready(path));
            }
            if directory.is_dir() && incomplete.is_none() {
                incomplete = Some(ExecutableDiscovery::Incomplete {
                    directory,
                    missing: names.iter().map(|name| (*name).to_owned()).collect(),
                });
            }
        }
        incomplete
    }

    fn mses_candidates(&self, configured: &Path) -> Vec<(PathBuf, MsesSource)> {
        let mut candidates = Vec::new();
        if let Some(path) = self.resolve_directory(configured) {
            candidates.push((path, MsesSource::Configured));
        }
        for root in self.adjacent_tool_roots() {
            let tools = root.join("external tools");
            for name in ["MSES", "mses"] {
                let path = tools.join(name);
                if !candidates.iter().any(|(candidate, _)| candidate == &path) {
                    candidates.push((path, MsesSource::Adjacent { root: root.clone() }));
                }
            }
            for path in versioned_directories(&tools, "mses") {
                if !candidates.iter().any(|(candidate, _)| candidate == &path) {
                    candidates.push((path, MsesSource::Adjacent { root: root.clone() }));
                }
            }
        }
        candidates
    }

    pub(super) fn resolve_file(&self, configured: &Path) -> Option<PathBuf> {
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

    /// Return bounded application roots for adjacent-tool discovery.
    ///
    /// A release binary normally lives below `target/release`, while a
    /// development checkout keeps its external tools beside the repository's
    /// `Cargo.toml`. Walking only to that marker recovers the checkout layout
    /// without searching arbitrary directories on the host. Packaged installs
    /// have no marker, so discovery stops at the application root and its
    /// direct parent (as `candidate_roots` does) and the per-user data root;
    /// it never climbs towards the drive root.
    pub(super) fn adjacent_tool_roots(&self) -> Vec<PathBuf> {
        let mut chain = Vec::new();
        let mut marker_found = false;
        let mut current = Some(self.app_root.as_path());
        while let Some(root) = current {
            chain.push(root);
            if root.join("Cargo.toml").is_file() {
                marker_found = true;
                break;
            }
            current = root.parent();
        }
        if !marker_found {
            chain.truncate(2);
        }
        let mut roots: Vec<PathBuf> = Vec::new();
        for root in chain {
            if !roots.iter().any(|candidate| candidate == root) {
                roots.push(root.to_path_buf());
            }
        }
        if !roots.contains(&self.user_data_root) {
            roots.push(self.user_data_root.clone());
        }
        roots
    }
}
