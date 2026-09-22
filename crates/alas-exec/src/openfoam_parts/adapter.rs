// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Backend resolution, command construction, and capability probing.

use super::*;
impl OpenFoamAdapter {
    /// Resolve an adapter. `Auto` probes native utilities before WSL2.
    pub fn resolve(preferences: OpenFoamPreferences) -> Self {
        let backend = match preferences.backend {
            OpenFoamBackend::Auto => {
                if native_candidate(&preferences).is_some() {
                    OpenFoamBackend::Native
                } else if wsl_candidate(&preferences) {
                    OpenFoamBackend::Wsl2
                } else {
                    OpenFoamBackend::Native
                }
            }
            selected => selected,
        };
        Self {
            preferences,
            backend,
        }
    }

    /// The persisted preferences used by this adapter.
    pub fn preferences(&self) -> &OpenFoamPreferences {
        &self.preferences
    }

    /// Backend selected after `Auto` resolution.
    pub fn backend(&self) -> OpenFoamBackend {
        self.backend
    }

    /// Probe required and optional utilities without launching a case.
    pub fn probe(&self) -> OpenFoamCapabilities {
        let deadline = Instant::now() + PROBE_TOTAL_TIMEOUT;
        let mut commands = BTreeMap::new();
        let mut version = None;
        let mut detail = String::new();

        if self.backend == OpenFoamBackend::Wsl2 && !wsl_candidate(&self.preferences) {
            for tool in REQUIRED_COMMANDS.iter().chain(OPTIONAL_COMMANDS) {
                commands.insert((*tool).to_owned(), false);
            }
            commands.insert(GMSH_COMMAND.to_owned(), false);
            let parsed_version = configured_directory_version(&self.preferences);
            return OpenFoamCapabilities {
                backend: self.backend,
                version: parsed_version.as_ref().map(OpenFoamVersion::qualified_name),
                commands,
                available: false,
                detail: "wsl.exe is not available on this host".to_owned(),
                version_support: OpenFoamVersionAssessment::from_version(parsed_version.as_ref()),
                parsed_version,
            };
        }

        for tool in REQUIRED_COMMANDS.iter().chain(OPTIONAL_COMMANDS) {
            let available = remaining_probe_time(deadline).is_some_and(|timeout| {
                self.command(tool, None, &[OsString::from("-help")])
                    .ok()
                    .is_some_and(|command| probe_command_with_timeout(&command, timeout))
            });
            commands.insert((*tool).to_owned(), available);
        }

        let gmsh_available = remaining_probe_time(deadline).is_some_and(|timeout| {
            self.gmsh_command(Path::new("."), &[OsString::from("-version")])
                .ok()
                .is_some_and(|command| probe_output_with_timeout(&command, timeout).is_some())
        });
        commands.insert(GMSH_COMMAND.to_owned(), gmsh_available);

        if let Some(timeout) = remaining_probe_time(deadline) {
            if let Ok(command) = self.command("foamVersion", None, &[]) {
                if let Some(output) = probe_output_with_timeout(&command, timeout) {
                    version = extract_version(&output);
                    detail = output.lines().take(2).collect::<Vec<_>>().join(" ");
                }
            }
        }
        if version.is_none() {
            let banner_tool = REQUIRED_COMMANDS
                .iter()
                .find(|tool| commands.get(**tool).copied().unwrap_or(false));
            if let (Some(tool), Some(timeout)) = (banner_tool, remaining_probe_time(deadline)) {
                if let Ok(command) = self.command(tool, None, &[OsString::from("-help")]) {
                    if let Some(output) = probe_output_with_timeout(&command, timeout) {
                        version = extract_version(&output);
                        if detail.is_empty() {
                            detail = output.lines().take(2).collect::<Vec<_>>().join(" ");
                        }
                    }
                }
            }
        }

        let available = REQUIRED_COMMANDS
            .iter()
            .all(|tool| commands.get(*tool).copied().unwrap_or(false))
            && gmsh_available;
        if detail.is_empty() {
            let mut missing = REQUIRED_COMMANDS
                .iter()
                .filter(|tool| !commands.get(**tool).copied().unwrap_or(false))
                .copied()
                .collect::<Vec<_>>();
            if !gmsh_available {
                missing.push(GMSH_COMMAND);
            }
            detail = if missing.is_empty() {
                "Required OpenFOAM utilities and Gmsh are available.".to_owned()
            } else {
                let suffix = if remaining_probe_time(deadline).is_none() {
                    "; connection probe reached its time limit"
                } else {
                    ""
                };
                format!("Missing required utilities: {}{suffix}", missing.join(", "))
            };
        }

        // A banner is authoritative; the configured directory name is only a
        // fallback so a Foundation 11 `OpenFOAM-11` install is still reported
        // as unsupported when no utility answered the probe.
        let parsed_version = version.or_else(|| configured_directory_version(&self.preferences));
        let version_support = OpenFoamVersionAssessment::from_version(parsed_version.as_ref());
        if version_support.level == OpenFoamSupportLevel::Unsupported {
            detail = format!("{detail} {}", version_support.reason);
        }
        OpenFoamCapabilities {
            backend: self.backend,
            version: parsed_version.as_ref().map(OpenFoamVersion::qualified_name),
            commands,
            available,
            detail,
            parsed_version,
            version_support,
        }
    }

    /// Resolve one utility invocation for an isolated case directory.
    pub fn command(
        &self,
        tool: &str,
        case_dir: Option<&Path>,
        extra_args: &[OsString],
    ) -> Result<OpenFoamCommand, String> {
        if !valid_tool_name(tool) {
            return Err(format!("invalid OpenFOAM utility name: {tool}"));
        }
        match self.backend {
            OpenFoamBackend::Native | OpenFoamBackend::Auto => {
                let program = native_program(&self.preferences, tool)?;
                let mut args = Vec::with_capacity(extra_args.len() + 2);
                if let Some(case_dir) = case_dir {
                    args.push(OsString::from("-case"));
                    // The process is launched in the isolated case directory;
                    // passing the same relative path again would make
                    // OpenFOAM resolve `case/case` for caller-supplied
                    // relative paths. `.` also handles spaces and Unicode
                    // without shell quoting.
                    let _ = case_dir;
                    args.push(OsString::from("."));
                }
                args.extend(extra_args.iter().cloned());
                let mut environment = Vec::new();
                if let Some(project) = self.preferences.native_project_dir.as_deref() {
                    environment.push((OsString::from("WM_PROJECT_DIR"), OsString::from(project)));
                    environment.push((
                        OsString::from("WM_PROJECT_VERSION"),
                        project_version(project).into(),
                    ));
                }
                if let Some(bin) = self.preferences.native_bin_dir.as_deref() {
                    let old_path = std::env::var_os("PATH").unwrap_or_default();
                    let mut value = OsString::from(bin);
                    value.push(if cfg!(windows) { ";" } else { ":" });
                    value.push(old_path);
                    environment.push((OsString::from("PATH"), value));
                }
                Ok(OpenFoamCommand {
                    program,
                    args,
                    current_dir: case_dir.map(Path::to_path_buf),
                    environment,
                    label: tool.to_owned(),
                })
            }
            OpenFoamBackend::Wsl2 => {
                if !wsl_candidate(&self.preferences) {
                    return Err("wsl.exe is not available on this host".to_owned());
                }
                let args = wsl_tool_args(&self.preferences, tool, case_dir, extra_args);
                Ok(OpenFoamCommand {
                    program: PathBuf::from("wsl.exe"),
                    args,
                    current_dir: None,
                    environment: Vec::new(),
                    label: format!("wsl2:{tool}"),
                })
            }
        }
    }

    /// Execute a command while capturing both streams and supervising the
    /// owned process tree.
    pub fn run(
        &self,
        command: &OpenFoamCommand,
        cancel: &Arc<AtomicBool>,
        timeout: Duration,
    ) -> OpenFoamProcessResult {
        self.run_with_callback(command, cancel, timeout, |_stream, _chunk| {})
    }

    /// Execute a command while forwarding bounded stdout/stderr chunks as
    /// they arrive. The callback runs on the caller's worker thread, never on
    /// the GUI thread; `run` above remains the compatibility wrapper for
    /// callers that only need the final captured result.
    pub fn run_with_callback<F>(
        &self,
        command: &OpenFoamCommand,
        cancel: &Arc<AtomicBool>,
        timeout: Duration,
        callback: F,
    ) -> OpenFoamProcessResult
    where
        F: FnMut(OpenFoamOutputStream, String),
    {
        super::process_runner::run_command_with_callback(command, cancel, timeout, callback)
    }

    /// Default timeout derived from the persisted settings.
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.preferences.timeout_seconds.clamp(1, 86_400))
    }

    /// Resolve the external Gmsh invocation used to create the 2-D mesh.
    /// Gmsh is intentionally separate from the OpenFOAM utility list because
    /// the official native distribution does not bundle it.
    pub fn gmsh_command(
        &self,
        case_dir: &Path,
        extra_args: &[OsString],
    ) -> Result<OpenFoamCommand, String> {
        let configured = self
            .preferences
            .gmsh_executable
            .as_deref()
            .filter(|value| !value.trim().is_empty());
        match self.backend {
            OpenFoamBackend::Native | OpenFoamBackend::Auto => {
                let program = configured.map(PathBuf::from).unwrap_or_else(|| {
                    PathBuf::from(if cfg!(windows) { "gmsh.exe" } else { "gmsh" })
                });
                let mut environment = Vec::new();
                if let Some(project) = self.preferences.native_project_dir.as_deref() {
                    environment.push((OsString::from("WM_PROJECT_DIR"), OsString::from(project)));
                    environment.push((
                        OsString::from("WM_PROJECT_VERSION"),
                        project_version(project).into(),
                    ));
                }
                if let Some(bin) = self.preferences.native_bin_dir.as_deref() {
                    let old_path = std::env::var_os("PATH").unwrap_or_default();
                    let mut value = OsString::from(bin);
                    value.push(if cfg!(windows) { ";" } else { ":" });
                    value.push(old_path);
                    environment.push((OsString::from("PATH"), value));
                }
                Ok(OpenFoamCommand {
                    program,
                    args: extra_args.to_vec(),
                    current_dir: Some(case_dir.to_path_buf()),
                    environment,
                    label: "gmsh".to_owned(),
                })
            }
            OpenFoamBackend::Wsl2 => {
                if !wsl_candidate(&self.preferences) {
                    return Err("wsl.exe is not available on this host".to_owned());
                }
                let mut args = Vec::with_capacity(extra_args.len() + 8);
                if let Some(distribution) = self
                    .preferences
                    .wsl_distribution
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                {
                    args.push(OsString::from("--distribution"));
                    args.push(OsString::from(distribution));
                }
                args.push(OsString::from("--cd"));
                args.push(OsString::from(wsl_path(case_dir)));
                args.push(OsString::from("--exec"));
                let wsl_tool = wsl_executable_path(
                    configured,
                    self.preferences.wsl_bin_dir.as_deref(),
                    "gmsh",
                );
                args.push(OsString::from(wsl_tool));
                args.extend(extra_args.iter().cloned());
                Ok(OpenFoamCommand {
                    program: PathBuf::from("wsl.exe"),
                    args,
                    current_dir: None,
                    environment: Vec::new(),
                    label: "wsl2:gmsh".to_owned(),
                })
            }
        }
    }

    /// Probe Gmsh separately from the OpenFOAM capability contract.
    pub fn probe_gmsh(&self) -> bool {
        self.gmsh_command(Path::new("."), &[OsString::from("-version")])
            .ok()
            .and_then(|command| probe_output(&command))
            .is_some()
    }
}
