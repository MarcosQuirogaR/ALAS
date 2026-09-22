// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Execution boundary for a user-provided FLOWUnsteady adapter.
//!
//! The adapter command receives `--alas-request <file> --alas-result <file>`.
//! It may launch Julia and FLOWUnsteady, but neither is bundled, linked, nor
//! silently substituted by this Rust application.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::process::{kill_process_tree, NewProcessGroup, NoConsoleWindow};
use crate::supervise::SupervisedSpawn;

/// Process outcome before parsing adapter output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowUnsteadyProcessStatus {
    /// Request file was absent before launch.
    InputMissing,
    /// Adapter executable could not start.
    LaunchFailed,
    /// Adapter exceeded its deadline.
    TimedOut,
    /// Adapter returned a non-zero process status.
    SolverFailed,
    /// Adapter returned success without a fresh result.
    OutputMissing,
    /// Adapter completed and retained its result.
    Completed,
}
impl FlowUnsteadyProcessStatus {
    /// Stable retained-evidence spelling.
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

/// Retained adapter invocation evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowUnsteadyProcessResult {
    /// Process-level outcome.
    pub status: FlowUnsteadyProcessStatus,
    /// Request supplied to the adapter.
    pub request_path: PathBuf,
    /// Adapter result expected after completion.
    pub result_path: PathBuf,
    /// Captured standard output.
    pub stdout_path: PathBuf,
    /// Captured standard error.
    pub stderr_path: PathBuf,
    /// Failure detail, absent only after a completed call.
    pub error: Option<String>,
}

/// Run a configured adapter and require a fresh non-empty result file.
pub fn run_flowunsteady_adapter(
    executable: &Path,
    request_path: &Path,
    timeout_seconds: f64,
) -> FlowUnsteadyProcessResult {
    let result_path = request_path.with_extension("flowunsteady.result.txt");
    let stdout_path = request_path.with_extension("flowunsteady.stdout.txt");
    let stderr_path = request_path.with_extension("flowunsteady.stderr.txt");
    let mut result = FlowUnsteadyProcessResult {
        status: FlowUnsteadyProcessStatus::InputMissing,
        request_path: request_path.to_path_buf(),
        result_path: result_path.clone(),
        stdout_path: stdout_path.clone(),
        stderr_path: stderr_path.clone(),
        error: None,
    };
    if !request_path.is_file() {
        result.error = Some(format!(
            "adapter request is missing: {}",
            request_path.display()
        ));
        return result;
    }
    if result_path.exists() && fs::remove_file(&result_path).is_err() {
        result.status = FlowUnsteadyProcessStatus::LaunchFailed;
        result.error = Some(format!("cannot remove stale {}", result_path.display()));
        return result;
    }
    let stdout = match File::create(&stdout_path) {
        Ok(file) => file,
        Err(error) => {
            result.status = FlowUnsteadyProcessStatus::LaunchFailed;
            result.error = Some(error.to_string());
            return result;
        }
    };
    let stderr = match File::create(&stderr_path) {
        Ok(file) => file,
        Err(error) => {
            result.status = FlowUnsteadyProcessStatus::LaunchFailed;
            result.error = Some(error.to_string());
            return result;
        }
    };
    let mut command = Command::new(executable);
    command
        .args(["--alas-request"])
        .arg(request_path)
        .args(["--alas-result"])
        .arg(&result_path)
        .current_dir(request_path.parent().unwrap_or(Path::new(".")))
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .no_window()
        .new_process_group();
    let mut child = match command.spawn_supervised("FLOWUnsteady analysis") {
        Ok(child) => child,
        Err(error) => {
            result.status = FlowUnsteadyProcessStatus::LaunchFailed;
            result.error = Some(format!(
                "failed to launch FLOWUnsteady adapter {}: {error}",
                executable.display()
            ));
            return result;
        }
    };
    let deadline = Instant::now() + Duration::from_secs_f64(timeout_seconds.max(0.1));
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                kill_process_tree(child.id());
                let _ = child.wait();
                result.status = FlowUnsteadyProcessStatus::TimedOut;
                result.error = Some(format!(
                    "FLOWUnsteady adapter exceeded {timeout_seconds:.1} s"
                ));
                return result;
            }
            Err(error) => {
                result.status = FlowUnsteadyProcessStatus::SolverFailed;
                result.error = Some(error.to_string());
                return result;
            }
        }
    };
    if !status.success() {
        result.status = FlowUnsteadyProcessStatus::SolverFailed;
        result.error = Some(format!(
            "FLOWUnsteady adapter exited with {:?}",
            status.code()
        ));
        return result;
    }
    if !result_path
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 40)
    {
        result.status = FlowUnsteadyProcessStatus::OutputMissing;
        result.error = Some(format!(
            "adapter returned success without fresh {}",
            result_path.display()
        ));
        return result;
    }
    result.status = FlowUnsteadyProcessStatus::Completed;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn status_text_is_stable() {
        assert_eq!(
            FlowUnsteadyProcessStatus::OutputMissing.as_str(),
            "output_missing"
        );
    }
}
