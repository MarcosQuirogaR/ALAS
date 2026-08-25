// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native VSPAERO process execution with observable failure states.
//!
//! OpenVSP's result database is not the solver output contract: result keys
//! have changed across releases while VSPAERO's native `.polar` file has
//! remained the durable batch boundary. This module launches `vspaero`
//! directly and proves that a fresh polar was produced. Aerodynamic parsing
//! belongs to `alas-aero`, which already depends on this crate.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::process::{kill_process_tree, NewProcessGroup, NoConsoleWindow};

/// Native VSPAERO process outcome before aerodynamic parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VspaeroProcessStatus {
    /// The `.vspgeom` or `.vspaero` input is absent.
    InputMissing,
    /// The executable could not be launched.
    LaunchFailed,
    /// The process exceeded its deadline and its tree was killed.
    TimedOut,
    /// VSPAERO returned a non-zero process status.
    SolverFailed,
    /// VSPAERO returned success without producing a fresh non-empty polar.
    OutputMissing,
    /// VSPAERO returned success and produced a fresh native polar.
    Completed,
}

impl VspaeroProcessStatus {
    /// Stable text used by retained runtime summaries.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InputMissing => "input_missing",
            Self::LaunchFailed => "launch_failed",
            Self::TimedOut => "timed_out",
            Self::SolverFailed => "solver_failed",
            Self::OutputMissing => "output_missing",
            Self::Completed => "completed",
        }
    }
}

/// Files and diagnostics retained from one native VSPAERO invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VspaeroProcessResult {
    /// Explicit process-level state.
    pub status: VspaeroProcessStatus,
    /// Extensionless case path passed to VSPAERO.
    pub case_path: PathBuf,
    /// Native polar expected beside the case inputs.
    pub polar_path: PathBuf,
    /// Captured standard output.
    pub stdout_path: PathBuf,
    /// Captured standard error.
    pub stderr_path: PathBuf,
    /// Actionable failure detail, absent only for a completed run.
    pub error: Option<String>,
}

/// Execute native VSPAERO for an extensionless case path.
///
/// The case must already have adjacent `.vspgeom`, `.vkey`, and `.vspaero`
/// files. Stale `.polar` and `.history` files are removed before launch, so
/// prior results cannot turn a failed or non-converged invocation into
/// apparent success.
pub fn run_vspaero(
    executable: &Path,
    case_path: &Path,
    thread_count: usize,
    timeout_seconds: f64,
) -> VspaeroProcessResult {
    let polar_path = case_path.with_extension("polar");
    let history_path = case_path.with_extension("history");
    let stdout_path = case_path.with_extension("vspaero.stdout.txt");
    let stderr_path = case_path.with_extension("vspaero.stderr.txt");
    let mut result = VspaeroProcessResult {
        status: VspaeroProcessStatus::InputMissing,
        case_path: case_path.to_path_buf(),
        polar_path: polar_path.clone(),
        stdout_path: stdout_path.clone(),
        stderr_path: stderr_path.clone(),
        error: None,
    };

    let missing = ["vspgeom", "vkey", "vspaero"]
        .into_iter()
        .map(|extension| case_path.with_extension(extension))
        .filter(|path| !path.is_file())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        result.error = Some(format!(
            "VSPAERO input files are missing: {}",
            missing
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        return result;
    }
    if polar_path.exists() {
        if let Err(error) = fs::remove_file(&polar_path) {
            result.error = Some(format!(
                "cannot remove stale {}: {error}",
                polar_path.display()
            ));
            return result;
        }
    }
    if history_path.exists() {
        if let Err(error) = fs::remove_file(&history_path) {
            result.error = Some(format!(
                "cannot remove stale {}: {error}",
                history_path.display()
            ));
            return result;
        }
    }

    let stdout = match File::create(&stdout_path) {
        Ok(file) => file,
        Err(error) => {
            result.error = Some(format!("cannot create {}: {error}", stdout_path.display()));
            return result;
        }
    };
    let stderr = match File::create(&stderr_path) {
        Ok(file) => file,
        Err(error) => {
            result.error = Some(format!("cannot create {}: {error}", stderr_path.display()));
            return result;
        }
    };
    let working_directory = case_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // VSPAERO resolves its case argument after changing into the case
    // directory. Passing the original relative path would therefore prepend
    // that directory twice (for example `outputs/openvsp/outputs/openvsp`),
    // while an absolute path happens to work. The case file name is valid for
    // both relative and absolute callers once the working directory is set.
    let case_argument = vspaero_case_argument(case_path);
    let mut command = Command::new(executable);
    command
        .args(["-omp", &thread_count.max(1).to_string()])
        .arg(case_argument)
        .current_dir(working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .no_window()
        .new_process_group();
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            result.status = VspaeroProcessStatus::LaunchFailed;
            result.error = Some(format!(
                "failed to launch {}: {error}",
                executable.display()
            ));
            return result;
        }
    };

    let deadline = Instant::now() + Duration::from_secs_f64(timeout_seconds.max(0.1));
    let process_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                kill_process_tree(child.id());
                let _ = child.wait();
                result.status = VspaeroProcessStatus::TimedOut;
                result.error = Some(format!(
                    "VSPAERO exceeded the {timeout_seconds:.1} s deadline; process tree force-killed"
                ));
                return result;
            }
            Err(error) => {
                result.status = VspaeroProcessStatus::SolverFailed;
                result.error = Some(format!("cannot poll VSPAERO: {error}"));
                return result;
            }
        }
    };

    if !process_status.success() {
        let stdout_text = fs::read_to_string(&stdout_path).unwrap_or_default();
        let stderr_text = fs::read_to_string(&stderr_path).unwrap_or_default();
        result.status = VspaeroProcessStatus::SolverFailed;
        result.error = Some(format!(
            "VSPAERO exited with {}; case {}; inspect stdout {} and stderr {}; stdout tail: {}; stderr tail: {}",
            process_status
                .code()
                .map_or_else(|| "unknown code".to_owned(), |code| code.to_string()),
            case_path.display(),
            stdout_path.display(),
            stderr_path.display(),
            text_tail(&stdout_text),
            text_tail(&stderr_text),
        ));
        return result;
    }
    let polar_is_fresh = polar_path
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 100);
    if !polar_is_fresh {
        result.status = VspaeroProcessStatus::OutputMissing;
        result.error = Some(format!(
            "VSPAERO returned success without a fresh non-empty {}",
            polar_path.display()
        ));
        return result;
    }

    result.status = VspaeroProcessStatus::Completed;
    result.error = None;
    result
}

fn text_tail(text: &str) -> String {
    const MAX_LINES: usize = 20;
    const EMPTY_LOG: &str = "<empty>";

    let tail = text.lines().rev().take(MAX_LINES).collect::<Vec<_>>();
    if tail.is_empty() {
        return EMPTY_LOG.to_owned();
    }
    tail.into_iter().rev().collect::<Vec<_>>().join(" | ")
}

fn vspaero_case_argument(case_path: &Path) -> &std::ffi::OsStr {
    case_path.file_name().unwrap_or(case_path.as_os_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_inputs_are_reported_before_launch() {
        let root = std::env::temp_dir().join(format!(
            "alas-vspaero-missing-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let result = run_vspaero(Path::new("absent-vspaero"), &root.join("case"), 1, 1.0);
        assert_eq!(result.status, VspaeroProcessStatus::InputMissing);
        assert!(result.error.as_deref().is_some_and(|error| {
            error.contains("case.vspgeom")
                && error.contains("case.vkey")
                && error.contains("case.vspaero")
        }));
    }

    #[test]
    fn process_status_text_is_stable() {
        assert_eq!(VspaeroProcessStatus::InputMissing.as_str(), "input_missing");
        assert_eq!(VspaeroProcessStatus::LaunchFailed.as_str(), "launch_failed");
        assert_eq!(VspaeroProcessStatus::TimedOut.as_str(), "timed_out");
        assert_eq!(VspaeroProcessStatus::SolverFailed.as_str(), "solver_failed");
        assert_eq!(
            VspaeroProcessStatus::OutputMissing.as_str(),
            "output_missing"
        );
        assert_eq!(VspaeroProcessStatus::Completed.as_str(), "completed");
    }

    #[test]
    fn diagnostic_tail_keeps_the_last_solver_lines_in_reading_order() {
        assert_eq!(
            text_tail("first\nsecond\nthird\n"),
            "first | second | third"
        );
        assert_eq!(text_tail(""), "<empty>");
    }

    #[test]
    fn relative_case_arguments_are_reduced_to_the_name_inside_the_workdir() {
        let case = Path::new("outputs/openvsp/optimized_aircraft");
        assert_eq!(
            vspaero_case_argument(case),
            std::ffi::OsStr::new("optimized_aircraft")
        );
        let absolute = Path::new(r"C:\tmp\optimized_aircraft");
        assert_eq!(
            vspaero_case_argument(absolute),
            std::ffi::OsStr::new("optimized_aircraft")
        );
    }
}
