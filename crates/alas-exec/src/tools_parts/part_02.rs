// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez


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
