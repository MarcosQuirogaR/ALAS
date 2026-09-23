// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native Athena Vortex Lattice batch execution with observable outcomes.
//!
//! AVL is interactive even in batch use, so this runner retains the exact
//! command script fed to standard input and requires one fresh `FT` file per
//! requested angle. A stale output can therefore never make a failed process
//! appear successful.

use std::fmt::Write as FmtWrite;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::process::{
    absolute_path, parent_directory, timeout_from_seconds, wait_with_timeout, DeadlineWait,
    NewProcessGroup, NoConsoleWindow,
};

/// Process-level outcome before aerodynamic parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvlProcessStatus {
    /// Geometry, alpha schedule, or another execution input was invalid.
    InputMissing,
    /// The configured timeout was not a finite number of seconds greater
    /// than zero; nothing was launched.
    InvalidTimeout,
    /// The executable or retained command streams could not be launched.
    LaunchFailed,
    /// The deadline expired and the complete process tree was killed.
    TimedOut,
    /// AVL returned a non-zero status.
    SolverFailed,
    /// AVL returned success without every requested fresh force file.
    OutputMissing,
    /// AVL returned success and every requested force file is fresh.
    Completed,
}

/// Additional AVL artifacts a caller may retain beside the total-force files.
///
/// Strip-force and stability-derivative output use AVL's native `FS` and `ST`
/// commands. Trim output is intentionally expressed as session commands: the
/// operating menu needs aircraft-specific constraints, mass, and density, so
/// the executor cannot invent a physically meaningful trim setup. The supplied
/// nonblank command lines are written in order, followed by `X` and a
/// total-force export.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AvlOutputChannels {
    /// Retain one machine-readable strip-force file per requested alpha.
    pub strip_forces: bool,
    /// Retain one machine-readable stability-derivative matrix for the final
    /// executed operating point.
    pub stability_derivatives: bool,
    /// Optional AVL operating-menu commands used to establish a trim case.
    pub trim_commands: Option<Vec<String>>,
}

/// Output paths for the optional channels requested in [`AvlOutputChannels`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AvlOutputPaths {
    /// One strip-force file per requested alpha, in schedule order.
    pub strip_forces: Vec<PathBuf>,
    /// Stability-derivative matrix for the final operating point.
    pub derivatives: Option<PathBuf>,
    /// Total-force file produced after the optional trim commands.
    pub trim: Option<PathBuf>,
}

/// Per-invocation execution choices that do not change the legacy file layout.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AvlRunOptions {
    /// Directory retaining this run's session, logs, and solver outputs.
    ///
    /// `None` preserves the historical layout beside `geometry_path`. A
    /// namespace is created before launch and is the AVL working directory, so
    /// two callers can use the same geometry stem without overwriting one
    /// another's retained evidence.
    pub result_namespace: Option<PathBuf>,
    /// Optional native AVL artifacts to request and validate.
    pub output_channels: AvlOutputChannels,
}

impl AvlRunOptions {
    /// Build options that retain all process artifacts under `namespace`.
    pub fn in_namespace(namespace: impl Into<PathBuf>) -> Self {
        Self {
            result_namespace: Some(namespace.into()),
            output_channels: AvlOutputChannels::default(),
        }
    }

    /// Add optional native output channels to this invocation.
    pub fn with_output_channels(mut self, output_channels: AvlOutputChannels) -> Self {
        self.output_channels = output_channels;
        self
    }
}

impl AvlProcessStatus {
    /// Stable text for runtime evidence and UI status messages.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InputMissing => "input_missing",
            Self::InvalidTimeout => "invalid_timeout",
            Self::LaunchFailed => "launch_failed",
            Self::TimedOut => "timed_out",
            Self::SolverFailed => "solver_failed",
            Self::OutputMissing => "output_missing",
            Self::Completed => "completed",
        }
    }
}

/// Retained files and diagnostics from one native AVL sweep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvlProcessResult {
    /// Explicit process outcome.
    pub status: AvlProcessStatus,
    /// Geometry deck passed to AVL.
    pub geometry_path: PathBuf,
    /// Exact interactive command stream passed on standard input.
    pub session_path: PathBuf,
    /// One expected total-force file per requested alpha.
    pub force_paths: Vec<PathBuf>,
    /// Optional native output paths requested for this run.
    pub output_paths: AvlOutputPaths,
    /// Captured standard output.
    pub stdout_path: PathBuf,
    /// Captured standard error.
    pub stderr_path: PathBuf,
    /// Actionable failure detail, absent only after complete output.
    pub error: Option<String>,
}

/// Execute one native AVL alpha sweep.
pub fn run_avl(
    executable: &Path,
    geometry_path: &Path,
    alpha_deg: &[f64],
    timeout_seconds: f64,
) -> AvlProcessResult {
    run_avl_with_options(
        executable,
        geometry_path,
        alpha_deg,
        timeout_seconds,
        &AvlRunOptions::default(),
    )
}

/// Execute one AVL sweep with retained output under a dedicated namespace.
///
/// This is the reusable execution primitive for future VLM/AVL parallel runs;
/// orchestration decides how the namespace is named and whether the calls run
/// on separate threads. The geometry remains the caller's input deck, while
/// AVL's working directory and every generated artifact are isolated.
pub fn run_avl_in_namespace(
    executable: &Path,
    geometry_path: &Path,
    alpha_deg: &[f64],
    timeout_seconds: f64,
    namespace: &Path,
) -> AvlProcessResult {
    run_avl_with_options(
        executable,
        geometry_path,
        alpha_deg,
        timeout_seconds,
        &AvlRunOptions::in_namespace(namespace),
    )
}

/// Execute one native AVL alpha sweep with explicit retention choices.
///
/// `run_avl` remains the compatibility wrapper for existing callers. New
/// callers should provide a unique `result_namespace` for every retained run;
/// this makes parallel execution a pipeline concern without allowing the
/// executor's files to collide.
pub fn run_avl_with_options(
    executable: &Path,
    geometry_path: &Path,
    alpha_deg: &[f64],
    timeout_seconds: f64,
    options: &AvlRunOptions,
) -> AvlProcessResult {
    let paths = render_output_paths(
        geometry_path,
        alpha_deg.len(),
        options.result_namespace.as_deref(),
        &options.output_channels,
    );
    let session_path = paths.session_path.clone();
    let stdout_path = paths.stdout_path.clone();
    let stderr_path = paths.stderr_path.clone();
    let force_paths = paths.force_paths.clone();
    let mut result = AvlProcessResult {
        status: AvlProcessStatus::InputMissing,
        geometry_path: geometry_path.to_path_buf(),
        session_path: session_path.clone(),
        force_paths: force_paths.clone(),
        output_paths: paths.output_paths.clone(),
        stdout_path: stdout_path.clone(),
        stderr_path: stderr_path.clone(),
        error: None,
    };
    if !geometry_path.is_file() {
        result.error = Some(format!(
            "AVL geometry is missing: {}",
            geometry_path.display()
        ));
        return result;
    }
    if alpha_deg.is_empty() || alpha_deg.iter().any(|alpha| !alpha.is_finite()) {
        result.error = Some("AVL alpha schedule must contain finite values".to_owned());
        return result;
    }

    let timeout = match timeout_from_seconds("AVL", timeout_seconds) {
        Ok(timeout) => timeout,
        Err(error) => {
            result.status = AvlProcessStatus::InvalidTimeout;
            result.error = Some(error.to_string());
            return result;
        }
    };
    if let Some(namespace) = options.result_namespace.as_deref() {
        if let Err(error) = fs::create_dir_all(namespace) {
            result.status = AvlProcessStatus::LaunchFailed;
            result.error = Some(format!(
                "cannot create AVL result namespace {}: {error}",
                namespace.display()
            ));
            return result;
        }
        if !namespace.is_dir() {
            result.status = AvlProcessStatus::LaunchFailed;
            result.error = Some(format!(
                "AVL result namespace is not a directory: {}",
                namespace.display()
            ));
            return result;
        }
    }

    for path in paths.all_artifacts() {
        if let Err(error) = remove_stale_artifact(path) {
            result.status = AvlProcessStatus::LaunchFailed;
            result.error = Some(format!("cannot remove stale {}: {error}", path.display()));
            return result;
        }
    }

    let session = render_session_with_outputs(
        alpha_deg,
        &force_paths,
        &paths.output_paths,
        &options.output_channels,
    );
    let mut session_file = match create_fresh_file(&session_path) {
        Ok(file) => file,
        Err(error) => {
            result.status = AvlProcessStatus::LaunchFailed;
            result.error = Some(format!("cannot create {}: {error}", session_path.display()));
            return result;
        }
    };
    if let Err(error) = std::io::Write::write_all(&mut session_file, session.as_bytes()) {
        result.status = AvlProcessStatus::LaunchFailed;
        result.error = Some(format!("cannot write {}: {error}", session_path.display()));
        return result;
    }
    drop(session_file);
    let stdin = match File::open(&session_path) {
        Ok(file) => file,
        Err(error) => {
            result.status = AvlProcessStatus::LaunchFailed;
            result.error = Some(format!("cannot open {}: {error}", session_path.display()));
            return result;
        }
    };
    let stdout = match create_fresh_file(&stdout_path) {
        Ok(file) => file,
        Err(error) => {
            result.status = AvlProcessStatus::LaunchFailed;
            result.error = Some(format!("cannot create {}: {error}", stdout_path.display()));
            return result;
        }
    };
    let stderr = match create_fresh_file(&stderr_path) {
        Ok(file) => file,
        Err(error) => {
            result.status = AvlProcessStatus::LaunchFailed;
            result.error = Some(format!("cannot create {}: {error}", stderr_path.display()));
            return result;
        }
    };
    let working_directory = options
        .result_namespace
        .as_deref()
        .unwrap_or_else(|| parent_directory(geometry_path));
    let geometry_argument = if options.result_namespace.is_some() {
        absolute_path(geometry_path)
    } else {
        geometry_path
            .file_name()
            .map(PathBuf::from)
            .unwrap_or_else(|| geometry_path.to_path_buf())
    };
    let mut command = Command::new(executable);
    command
        .arg(geometry_argument)
        .current_dir(working_directory)
        .stdin(Stdio::from(stdin))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .no_window()
        .new_process_group();
    let mut child = match crate::SupervisedSpawn::spawn_supervised(&mut command, "AVL sweep") {
        Ok(child) => child,
        Err(error) => {
            result.status = AvlProcessStatus::LaunchFailed;
            result.error = Some(format!(
                "failed to launch {}: {error}",
                executable.display()
            ));
            return result;
        }
    };

    let process_status = match wait_with_timeout(&mut child, timeout) {
        DeadlineWait::Exited(status) => status,
        DeadlineWait::TimedOut => {
            result.status = AvlProcessStatus::TimedOut;
            result.error = Some(format!(
                "AVL exceeded the {timeout_seconds:.1} s deadline; process tree force-killed"
            ));
            return result;
        }
        DeadlineWait::PollFailed(error) => {
            result.status = AvlProcessStatus::SolverFailed;
            result.error = Some(format!("cannot poll AVL: {error}"));
            return result;
        }
    };
    if !process_status.success() {
        result.status = AvlProcessStatus::SolverFailed;
        result.error = Some(format!(
            "AVL exited with {}",
            process_status
                .code()
                .map_or_else(|| "unknown code".to_owned(), |code| code.to_string())
        ));
        return result;
    }
    let missing = paths
        .required_outputs()
        .filter(|path| !is_fresh_output(path))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        result.status = AvlProcessStatus::OutputMissing;
        result.error = Some(format!(
            "AVL returned success without fresh requested output files: {}",
            missing
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        return result;
    }
    result.status = AvlProcessStatus::Completed;
    result.error = None;
    result
}

#[cfg(test)]
fn render_session(alpha_deg: &[f64], force_paths: &[PathBuf]) -> String {
    render_session_with_outputs(
        alpha_deg,
        force_paths,
        &AvlOutputPaths::default(),
        &AvlOutputChannels::default(),
    )
}

fn render_session_with_outputs(
    alpha_deg: &[f64],
    force_paths: &[PathBuf],
    output_paths: &AvlOutputPaths,
    channels: &AvlOutputChannels,
) -> String {
    // `MRF` switches AVL's FT writer to its full-precision machine-readable
    // layout.  The human FT layout rounds Sref/Cref/Bref to a few digits, so
    // feeding it to the strict reference/frame comparison would reject an
    // otherwise identical SI deck.
    let mut session = String::from("OPER\nMRF\n");
    for (index, (&alpha, path)) in alpha_deg.iter().zip(force_paths).enumerate() {
        writeln!(session, "A A {alpha:.12}").ok();
        writeln!(session, "X").ok();
        writeln!(session, "FT").ok();
        writeln!(session, "{}", output_filename(path)).ok();
        if channels.strip_forces {
            session.push_str("FS\n");
            if let Some(strip_path) = output_paths.strip_forces.get(index) {
                writeln!(session, "{}", output_filename(strip_path)).ok();
            }
        }
    }
    if let Some(trim_commands) = &channels.trim_commands {
        for command in trim_commands {
            for line in command.lines() {
                if !line.trim().is_empty() {
                    writeln!(session, "{line}").ok();
                }
            }
        }
        session.push_str("X\nFT\n");
        if let Some(trim_path) = &output_paths.trim {
            writeln!(session, "{}", output_filename(trim_path)).ok();
        }
    }
    if channels.stability_derivatives {
        session.push_str("ST\n");
        if let Some(derivatives_path) = &output_paths.derivatives {
            writeln!(session, "{}", output_filename(derivatives_path)).ok();
        }
    }
    session.push_str("\nQUIT\n");
    session
}

#[derive(Debug, Clone)]
struct AvlRunPaths {
    session_path: PathBuf,
    force_paths: Vec<PathBuf>,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
    output_paths: AvlOutputPaths,
}

impl AvlRunPaths {
    fn all_artifacts(&self) -> impl Iterator<Item = &Path> {
        self.force_paths
            .iter()
            .map(PathBuf::as_path)
            .chain(self.output_paths.strip_forces.iter().map(PathBuf::as_path))
            .chain(self.output_paths.derivatives.iter().map(PathBuf::as_path))
            .chain(self.output_paths.trim.iter().map(PathBuf::as_path))
            .chain([
                self.session_path.as_path(),
                self.stdout_path.as_path(),
                self.stderr_path.as_path(),
            ])
    }

    fn required_outputs(&self) -> impl Iterator<Item = &Path> {
        self.force_paths
            .iter()
            .map(PathBuf::as_path)
            .chain(self.output_paths.strip_forces.iter().map(PathBuf::as_path))
            .chain(self.output_paths.derivatives.iter().map(PathBuf::as_path))
            .chain(self.output_paths.trim.iter().map(PathBuf::as_path))
    }
}

fn render_output_paths(
    geometry_path: &Path,
    alpha_count: usize,
    namespace: Option<&Path>,
    channels: &AvlOutputChannels,
) -> AvlRunPaths {
    let source_base = geometry_path.with_extension("");
    let base = match namespace {
        Some(namespace) => namespace.join(
            source_base
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("avl_case")),
        ),
        None => source_base,
    };
    let force_paths = (0..alpha_count)
        .map(|index| base.with_extension(format!("avl.{index:03}.ft")))
        .collect::<Vec<_>>();
    let output_paths = AvlOutputPaths {
        strip_forces: if channels.strip_forces {
            (0..alpha_count)
                .map(|index| base.with_extension(format!("avl.{index:03}.fs")))
                .collect()
        } else {
            Vec::new()
        },
        derivatives: channels
            .stability_derivatives
            .then(|| base.with_extension("avl.derivatives.txt")),
        trim: channels
            .trim_commands
            .as_ref()
            .map(|_| base.with_extension("avl.trim.ft")),
    };
    AvlRunPaths {
        session_path: base.with_extension("avl.session.txt"),
        force_paths,
        stdout_path: base.with_extension("avl.stdout.txt"),
        stderr_path: base.with_extension("avl.stderr.txt"),
        output_paths,
    }
}

fn output_filename(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.as_os_str().to_string_lossy().into_owned())
}

fn create_fresh_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .truncate(false)
        .open(path)
}

fn remove_stale_artifact(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() || metadata.file_type().is_symlink() => {
            fs::remove_file(path)
        }
        Ok(_) => Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "expected an output file, found another filesystem object",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn is_fresh_output(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.file_type().is_file() && metadata.len() > 100)
}

#[cfg(test)]
#[path = "avl_tests.rs"]
mod avl_tests;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_has_one_execute_and_force_export_per_alpha() {
        let paths = [PathBuf::from("case.000.ft"), PathBuf::from("case.001.ft")];
        let session = render_session(&[-2.0, 3.5], &paths);
        assert_eq!(session.matches("\nX\n").count(), 2);
        assert_eq!(session.matches("\nFT\n").count(), 2);
        assert!(session.contains("A A -2.000000000000\nX\nFT\ncase.000.ft"));
        assert!(session.ends_with("\nQUIT\n"));
    }
    #[test]
    fn missing_geometry_is_reported_before_launch() {
        let root = std::env::temp_dir().join(format!(
            "alas-avl-missing-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let result = run_avl(Path::new("absent-avl"), &root.join("case.avl"), &[0.0], 1.0);
        assert_eq!(result.status, AvlProcessStatus::InputMissing);
        assert!(result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("case.avl")));
    }

    #[test]
    fn missing_executable_is_a_launch_failure_after_inputs_are_retained() {
        let root = std::env::temp_dir().join(format!(
            "alas-avl-launch-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::create_dir_all(&root)
            .unwrap_or_else(|error| panic!("create temporary AVL directory: {error}"));
        let geometry = root.join("case.avl");
        fs::write(&geometry, b"test geometry")
            .unwrap_or_else(|error| panic!("write geometry input: {error}"));
        let result = run_avl(&root.join("absent-avl"), &geometry, &[0.0], 1.0);
        assert_eq!(result.status, AvlProcessStatus::LaunchFailed);
        assert!(result.session_path.is_file());
        assert!(result
            .error
            .as_deref()
            .is_some_and(|error| error.contains("failed to launch")));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn process_status_text_keeps_timeout_and_output_failure_distinct() {
        assert_eq!(AvlProcessStatus::InputMissing.as_str(), "input_missing");
        assert_eq!(AvlProcessStatus::InvalidTimeout.as_str(), "invalid_timeout");
        assert_eq!(AvlProcessStatus::LaunchFailed.as_str(), "launch_failed");
        assert_eq!(AvlProcessStatus::TimedOut.as_str(), "timed_out");
        assert_eq!(AvlProcessStatus::SolverFailed.as_str(), "solver_failed");
        assert_eq!(AvlProcessStatus::OutputMissing.as_str(), "output_missing");
        assert_eq!(AvlProcessStatus::Completed.as_str(), "completed");
    }
}
