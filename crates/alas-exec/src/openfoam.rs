// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! OpenFOAM discovery and process supervision.
//!
//! The GUI and the CFD case builder deliberately do not know whether a case
//! is being run by the native OpenCFD Windows distribution or by a Linux
//! installation exposed through WSL2.  This module keeps that distinction at
//! one boundary: all paths are converted once, commands are passed as
//! arguments (never through a shell), and the owned process tree can be
//! cancelled or timed out.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// A connection test covers several utilities. Keep the whole operation
/// bounded even when a broken installation makes each individual probe wait
/// for its timeout.
const PROBE_TOTAL_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_CAPTURE_BYTES: u64 = 16 * 1024 * 1024;
const OUTPUT_CHANNEL_CAPACITY: usize = 128;
const OUTPUT_CHUNK_BYTES: usize = 16 * 1024;
const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_millis(750);

/// Backend used to execute OpenFOAM.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenFoamBackend {
    /// Select a usable native installation first, then WSL2.
    Auto,
    /// OpenCFD's native Windows/MinGW distribution.
    Native,
    /// A Linux distribution launched through `wsl.exe`.
    Wsl2,
}

impl Default for OpenFoamBackend {
    fn default() -> Self {
        Self::Auto
    }
}

impl OpenFoamBackend {
    /// Stable preference/configuration spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Native => "native",
            Self::Wsl2 => "wsl2",
        }
    }

    /// Human-readable label for the desktop UI.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Auto => "Automatic",
            Self::Native => "Native Windows",
            Self::Wsl2 => "WSL2 Linux",
        }
    }
}

/// Persisted OpenFOAM environment settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenFoamPreferences {
    /// Backend selection.
    pub backend: OpenFoamBackend,
    /// Native directory containing `blockMesh.exe`, `simpleFoam.exe`, etc.
    pub native_bin_dir: Option<String>,
    /// Native OpenFOAM project directory used for `WM_PROJECT_DIR`.
    pub native_project_dir: Option<String>,
    /// WSL distribution name (empty uses the default distribution).
    pub wsl_distribution: Option<String>,
    /// Optional OpenFOAM `bin` directory inside WSL2.
    pub wsl_bin_dir: Option<String>,
    /// Optional native/WSL Gmsh executable used for the verified 2-D mesh
    /// route. A blank value falls back to `gmsh`/`gmsh.exe` on PATH.
    pub gmsh_executable: Option<String>,
    /// Per-command time limit in seconds.
    pub timeout_seconds: u64,
    /// Maximum simultaneous solver processes exposed to future sweeps.
    pub max_parallel: u32,
}

impl Default for OpenFoamPreferences {
    fn default() -> Self {
        Self {
            backend: OpenFoamBackend::Auto,
            native_bin_dir: None,
            native_project_dir: None,
            wsl_distribution: None,
            wsl_bin_dir: None,
            gmsh_executable: None,
            timeout_seconds: 1_800,
            max_parallel: 1,
        }
    }
}

/// Commands required by the native Gmsh to OpenFOAM steady 2-D workflow.
pub const REQUIRED_COMMANDS: &[&str] = &["gmshToFoam", "checkMesh", "simpleFoam", "postProcess"];

/// External mesher required by the current reusable airfoil template.
pub const GMSH_COMMAND: &str = "gmsh";

/// Utilities used when available but not required for a serial case.
pub const OPTIONAL_COMMANDS: &[&str] = &[
    "blockMesh",
    "snappyHexMesh",
    "surfaceFeatureExtract",
    "potentialFoam",
];

/// Result of probing one OpenFOAM environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenFoamCapabilities {
    /// Backend that answered the probe.
    pub backend: OpenFoamBackend,
    /// Reported version, when `foamVersion` or a utility banner exposed one.
    pub version: Option<String>,
    /// Availability by command name.
    pub commands: BTreeMap<String, bool>,
    /// Whether every required OpenFOAM utility and Gmsh were found.
    pub available: bool,
    /// Actionable probe detail suitable for the setup page and run log.
    pub detail: String,
}

impl OpenFoamCapabilities {
    /// A compact status string for the GUI.
    pub fn summary(&self) -> String {
        let version = self.version.as_deref().unwrap_or("version unknown");
        if self.available {
            format!("{} ({version})", self.backend.display_name())
        } else {
            format!("{} unavailable ({version})", self.backend.display_name())
        }
    }
}

/// A fully resolved command invocation.
#[derive(Debug, Clone)]
pub struct OpenFoamCommand {
    /// Program passed to `Command::new`.
    pub program: PathBuf,
    /// Argument vector, including the case directory.
    pub args: Vec<OsString>,
    /// Working directory visible to the command.
    pub current_dir: Option<PathBuf>,
    /// Environment additions/replacements needed by the distribution.
    pub environment: Vec<(OsString, OsString)>,
    /// Human-readable command label.
    pub label: String,
}

/// Pipe that produced one incremental output callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenFoamOutputStream {
    /// Standard output from the utility.
    Stdout,
    /// Standard error from the utility.
    Stderr,
}

/// Process-level outcome before a CFD parser classifies the case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OpenFoamProcessStatus {
    /// Utility exited with code zero.
    Completed,
    /// Utility exited with a non-zero code.
    Failed,
    /// The configured timeout elapsed.
    TimedOut,
    /// The user requested cancellation.
    Cancelled,
    /// The executable could not be started.
    LaunchFailed,
}

/// Captured output from one supervised OpenFOAM utility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenFoamProcessResult {
    /// Process-level status.
    pub status: OpenFoamProcessStatus,
    /// Exit code, if the OS supplied one.
    pub exit_code: Option<i32>,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
    /// Wall-clock duration.
    pub elapsed_seconds: f64,
}

/// Adapter selected from persisted preferences and current host state.
#[derive(Debug, Clone)]
pub struct OpenFoamAdapter {
    preferences: OpenFoamPreferences,
    backend: OpenFoamBackend,
}

#[path = "openfoam_parts/adapter.rs"]
mod adapter;
#[path = "openfoam_parts/process.rs"]
mod process_runner;

/// Probe from preferences in one call for setup cards and tests.
pub fn probe_openfoam(preferences: OpenFoamPreferences) -> OpenFoamCapabilities {
    OpenFoamAdapter::resolve(preferences).probe()
}

fn valid_tool_name(tool: &str) -> bool {
    !tool.is_empty()
        && tool
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn native_candidate(preferences: &OpenFoamPreferences) -> Option<PathBuf> {
    if let Some(bin) = preferences.native_bin_dir.as_deref() {
        let path = PathBuf::from(bin);
        if path.is_dir()
            && REQUIRED_COMMANDS
                .iter()
                .any(|tool| native_tool(&path, tool).is_some())
        {
            return Some(path);
        }
    }
    REQUIRED_COMMANDS
        .iter()
        .find_map(|tool| command_in_path(tool))
}

fn native_program(preferences: &OpenFoamPreferences, tool: &str) -> Result<PathBuf, String> {
    if let Some(bin) = preferences.native_bin_dir.as_deref() {
        let path = PathBuf::from(bin);
        if let Some(executable) = native_tool(&path, tool) {
            return Ok(executable);
        }
        return Err(format!(
            "OpenFOAM utility {tool} was not found in {}",
            path.display()
        ));
    }
    Ok(PathBuf::from(if cfg!(windows) {
        format!("{tool}.exe")
    } else {
        tool.to_owned()
    }))
}

fn native_tool(bin: &Path, tool: &str) -> Option<PathBuf> {
    let direct = bin.join(tool);
    if direct.is_file() {
        return Some(direct);
    }
    let windows = bin.join(format!("{tool}.exe"));
    windows.is_file().then_some(windows)
}

fn command_in_path(tool: &str) -> Option<PathBuf> {
    let candidate = if cfg!(windows) {
        format!("{tool}.exe")
    } else {
        tool.to_owned()
    };
    let command = OpenFoamCommand {
        program: PathBuf::from(&candidate),
        args: vec![OsString::from("-help")],
        current_dir: None,
        environment: Vec::new(),
        label: format!("probe:{tool}"),
    };
    if matches!(
        probe_result(&command),
        Some(result) if result.status == OpenFoamProcessStatus::Completed
    ) {
        Some(PathBuf::from(candidate))
    } else {
        None
    }
}

fn wsl_candidate(preferences: &OpenFoamPreferences) -> bool {
    if !cfg!(windows) {
        return false;
    }
    let mut args = Vec::new();
    if let Some(distribution) = preferences
        .wsl_distribution
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        args.push(OsString::from("--distribution"));
        args.push(OsString::from(distribution));
    }
    args.extend(
        ["--exec", "sh", "-lc", "command -v blockMesh"]
            .into_iter()
            .map(OsString::from),
    );
    let command = OpenFoamCommand {
        program: PathBuf::from("wsl.exe"),
        args,
        current_dir: None,
        environment: Vec::new(),
        label: "probe:wsl".to_owned(),
    };
    matches!(probe_result(&command), Some(result) if result.status == OpenFoamProcessStatus::Completed)
}

fn remaining_probe_time(deadline: Instant) -> Option<Duration> {
    deadline.checked_duration_since(Instant::now())
}

fn probe_command_with_timeout(command: &OpenFoamCommand, timeout: Duration) -> bool {
    probe_output_with_timeout(command, timeout).is_some()
}

fn probe_output(command: &OpenFoamCommand) -> Option<String> {
    probe_output_with_timeout(command, PROBE_TIMEOUT)
}

fn probe_output_with_timeout(command: &OpenFoamCommand, timeout: Duration) -> Option<String> {
    let output = probe_result_with_timeout(command, timeout)?;
    if output.status != OpenFoamProcessStatus::Completed {
        return None;
    }
    let mut text = output.stdout;
    if text.trim().is_empty() {
        text = output.stderr;
    }
    Some(text)
}

fn probe_result(command: &OpenFoamCommand) -> Option<OpenFoamProcessResult> {
    probe_result_with_timeout(command, PROBE_TIMEOUT)
}

fn probe_result_with_timeout(
    command: &OpenFoamCommand,
    timeout: Duration,
) -> Option<OpenFoamProcessResult> {
    let cancel = Arc::new(AtomicBool::new(false));
    Some(run_command(command, &cancel, timeout))
}

fn run_command(
    command: &OpenFoamCommand,
    cancel: &Arc<AtomicBool>,
    timeout: Duration,
) -> OpenFoamProcessResult {
    process_runner::run_command_with_callback(command, cancel, timeout, |_stream, _chunk| {})
}
fn extract_version(output: &str) -> Option<String> {
    if let Some(index) = output.find("OpenFOAM-") {
        let token = output[index..]
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '-');
        if !token.is_empty() {
            return Some(token.to_owned());
        }
    }
    output.split_whitespace().find_map(|token| {
        let clean = token.trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '.');
        let has_digit = clean.chars().any(|ch| ch.is_ascii_digit());
        let has_separator = clean.contains('.') || clean.starts_with('v');
        (has_digit && has_separator).then_some(clean.to_owned())
    })
}

fn project_version(project: &str) -> String {
    Path::new(project)
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("OpenFOAM-"))
        .unwrap_or("unknown")
        .to_owned()
}

/// Convert a Windows host path to the conventional `/mnt/<drive>/...` WSL
/// spelling. POSIX paths and relative paths are retained with slash cleanup.
pub fn wsl_path(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let bytes = raw.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic() {
        return format!(
            "/mnt/{}/{}",
            (bytes[0] as char).to_ascii_lowercase(),
            raw[2..].trim_start_matches('/')
        );
    }
    raw
}

fn wsl_executable_path(configured: Option<&str>, bin_dir: Option<&str>, fallback: &str) -> String {
    let configured = configured.filter(|value| !value.trim().is_empty());
    let bin_dir = bin_dir
        .filter(|value| !value.trim().is_empty())
        .map(|value| wsl_path(Path::new(value)));
    let Some(configured) = configured else {
        return bin_dir
            .map(|bin| format!("{bin}/{fallback}"))
            .unwrap_or_else(|| fallback.to_owned());
    };

    // An absolute configured executable is already a complete path. A bare
    // executable name is resolved relative to the configured WSL bin dir,
    // which keeps PATH lookup out of a shell and handles Windows paths such as
    // `C:\\tools\\gmsh.exe` through the same conversion as case directories.
    let normalized = wsl_path(Path::new(configured));
    let absolute = normalized.starts_with('/') || is_windows_drive_path(configured);
    if absolute || bin_dir.is_none() {
        normalized
    } else {
        format!("{}/{}", bin_dir.unwrap_or_default(), normalized)
    }
}

fn is_windows_drive_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use std::thread;
    use std::time::Instant;

    fn shell_command(script: &str) -> OpenFoamCommand {
        #[cfg(windows)]
        let (program, args) = ("cmd", vec!["/C", script]);
        #[cfg(not(windows))]
        let (program, args) = ("sh", vec!["-c", script]);
        OpenFoamCommand {
            program: PathBuf::from(program),
            args: args.into_iter().map(OsString::from).collect(),
            current_dir: None,
            environment: Vec::new(),
            label: "test-shell".to_owned(),
        }
    }

    fn test_adapter() -> OpenFoamAdapter {
        OpenFoamAdapter::resolve(OpenFoamPreferences {
            backend: OpenFoamBackend::Native,
            ..OpenFoamPreferences::default()
        })
    }

    #[test]
    fn windows_path_translation_handles_drive_and_spaces() {
        assert_eq!(
            wsl_path(Path::new(r"C:\Users\Name With Space\case")),
            "/mnt/c/Users/Name With Space/case"
        );
        assert_eq!(wsl_path(Path::new("/tmp/case")), "/tmp/case");
    }

    #[test]
    fn wsl_executable_paths_do_not_leave_windows_separators_or_duplicate_bins() {
        assert_eq!(
            wsl_executable_path(Some(r"C:\tools\gmsh.exe"), Some(r"C:\OpenFOAM\bin"), "gmsh"),
            "/mnt/c/tools/gmsh.exe"
        );
        assert_eq!(
            wsl_executable_path(Some("gmsh"), Some("/opt/openfoam/bin"), "gmsh"),
            "/opt/openfoam/bin/gmsh"
        );
        assert_eq!(
            wsl_executable_path(None, Some(r"C:\OpenFOAM\bin"), "simpleFoam"),
            "/mnt/c/OpenFOAM/bin/simpleFoam"
        );
    }

    #[test]
    fn preferences_round_trip_with_legacy_defaults() {
        let parsed: OpenFoamPreferences = serde_json::from_str("{}").unwrap_or_default();
        assert_eq!(parsed.backend, OpenFoamBackend::Auto);
        assert_eq!(parsed.max_parallel, 1);
    }

    #[test]
    fn unsafe_utility_names_are_rejected_before_process_creation() {
        let adapter = OpenFoamAdapter::resolve(OpenFoamPreferences::default());
        assert!(adapter.command("simpleFoam;touch", None, &[]).is_err());
    }

    #[test]
    fn output_callback_receives_stdout_and_stderr_while_result_is_retained() {
        #[cfg(windows)]
        let script = "echo stdout & echo stderr 1>&2";
        #[cfg(not(windows))]
        let script = "printf stdout; printf stderr >&2";
        let command = shell_command(script);
        let cancel = Arc::new(AtomicBool::new(false));
        let mut chunks = Vec::new();
        let result = test_adapter().run_with_callback(
            &command,
            &cancel,
            Duration::from_secs(5),
            |stream, chunk| chunks.push((stream, chunk)),
        );

        assert_eq!(result.status, OpenFoamProcessStatus::Completed);
        assert!(
            result.stdout.contains("stdout"),
            "stdout={:?}",
            result.stdout
        );
        assert!(
            result.stderr.contains("stderr"),
            "stderr={:?}",
            result.stderr
        );
        assert!(chunks.iter().any(|(stream, chunk)| {
            *stream == OpenFoamOutputStream::Stdout && chunk.contains("stdout")
        }));
        assert!(chunks.iter().any(|(stream, chunk)| {
            *stream == OpenFoamOutputStream::Stderr && chunk.contains("stderr")
        }));
    }

    #[test]
    fn cancellation_stops_a_long_running_process_promptly() {
        #[cfg(windows)]
        let script = "ping -n 60 127.0.0.1 > NUL";
        #[cfg(not(windows))]
        let script = "sleep 60";
        let command = shell_command(script);
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_for_thread = Arc::clone(&cancel);
        let trigger = thread::spawn(move || {
            thread::sleep(Duration::from_millis(120));
            cancel_for_thread.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        let result = test_adapter().run(&command, &cancel, Duration::from_secs(5));
        trigger.join().expect("cancellation trigger should finish");

        assert_eq!(result.status, OpenFoamProcessStatus::Cancelled);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn timeout_stops_a_long_running_process() {
        #[cfg(windows)]
        let script = "ping -n 60 127.0.0.1 > NUL";
        #[cfg(not(windows))]
        let script = "sleep 60";
        let command = shell_command(script);
        let cancel = Arc::new(AtomicBool::new(false));
        let started = Instant::now();
        let result = test_adapter().run(&command, &cancel, Duration::from_millis(120));

        assert_eq!(result.status, OpenFoamProcessStatus::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn captured_output_is_bounded_without_disabling_pipe_drain() {
        let mut captured = process_runner::CapturedPipe::new();
        let input = vec![b'x'; MAX_CAPTURE_BYTES as usize + 1_024];
        let accepted = captured.append(&input);

        assert_eq!(accepted, MAX_CAPTURE_BYTES as usize);
        assert_eq!(captured.bytes.len(), MAX_CAPTURE_BYTES as usize);
    }

    #[test]
    fn connection_probe_reports_gmsh_as_a_distinct_capability() {
        let capabilities = test_adapter().probe();

        assert!(capabilities.commands.contains_key(GMSH_COMMAND));
        assert_eq!(
            capabilities.available,
            REQUIRED_COMMANDS.iter().all(|tool| capabilities
                .commands
                .get(*tool)
                .copied()
                .unwrap_or(false))
                && capabilities
                    .commands
                    .get(GMSH_COMMAND)
                    .copied()
                    .unwrap_or(false)
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn unavailable_wsl_probe_reports_the_complete_command_contract() {
        let adapter = OpenFoamAdapter::resolve(OpenFoamPreferences {
            backend: OpenFoamBackend::Wsl2,
            ..OpenFoamPreferences::default()
        });
        let capabilities = adapter.probe();

        for tool in REQUIRED_COMMANDS.iter().chain(OPTIONAL_COMMANDS) {
            assert_eq!(capabilities.commands.get(*tool), Some(&false));
        }
        assert_eq!(capabilities.commands.get(GMSH_COMMAND), Some(&false));
        assert!(!capabilities.available);
    }

    #[cfg(unix)]
    #[test]
    fn inherited_pipe_handles_do_not_make_a_completed_run_wait_for_descendants() {
        let command = shell_command("sleep 2 & exit 0");
        let cancel = Arc::new(AtomicBool::new(false));
        let started = Instant::now();
        let result = test_adapter().run(&command, &cancel, Duration::from_secs(5));

        assert_eq!(result.status, OpenFoamProcessStatus::Completed);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "run waited for inherited pipe handle: {:?}",
            started.elapsed()
        );
    }
}
