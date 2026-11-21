// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Health check: run an installed tool on a tiny case.

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::process::{
    drain, drained_text, wait_with_timeout, DeadlineWait, NewProcessGroup, NoConsoleWindow,
};
use crate::supervise::SupervisedSpawn;

use super::manifest::HealthSpec;
use super::InstallError;

/// Outcome of one health check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HealthReport {
    /// Whether the check passed.
    pub passed: bool,
    /// What happened, for display.
    pub detail: String,
}

/// Run `executable` with the spec's arguments in a scratch directory below
/// `scratch_parent`, and require exit status 0 plus the expected text.
pub fn run_health_check(
    executable: &Path,
    spec: &HealthSpec,
    scratch_parent: &Path,
) -> Result<HealthReport, InstallError> {
    let scratch = scratch_parent.join(format!(".health-{}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch);
    fs::create_dir_all(&scratch).map_err(|error| {
        InstallError::Io(format!("cannot create {}: {error}", scratch.display()))
    })?;
    let result = run_in(executable, spec, &scratch);
    let _ = fs::remove_dir_all(&scratch);
    result
}

fn run_in(
    executable: &Path,
    spec: &HealthSpec,
    scratch: &Path,
) -> Result<HealthReport, InstallError> {
    for (name, text) in &spec.case_files {
        let plain = Path::new(name).components().count() == 1;
        if !plain {
            return Err(InstallError::Manifest(format!(
                "case file name {name:?} is not plain"
            )));
        }
        fs::write(scratch.join(name), text)
            .map_err(|error| InstallError::Io(format!("cannot write case file: {error}")))?;
    }
    let directory = scratch.to_string_lossy().replace('\\', "/");
    let args: Vec<String> = spec
        .args
        .iter()
        .map(|arg| arg.replace("{dir}", &directory))
        .collect();
    let mut command = Command::new(executable);
    command
        .args(&args)
        .current_dir(scratch)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .no_window()
        .new_process_group();
    let mut child = command
        .spawn_supervised("tool installer health check")
        .map_err(|error| {
            InstallError::Health(format!("cannot start {}: {error}", executable.display()))
        })?;
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let status =
        match wait_with_timeout(&mut child, Duration::from_secs(spec.timeout_seconds.max(1))) {
            DeadlineWait::Exited(status) => status,
            DeadlineWait::TimedOut => {
                return Ok(HealthReport {
                    passed: false,
                    detail: format!("timed out after {} s", spec.timeout_seconds),
                });
            }
            DeadlineWait::PollFailed(error) => return Err(InstallError::Health(error.to_string())),
        };
    let text = format!("{}{}", drained_text(out), drained_text(err));
    if !status.success() {
        return Ok(HealthReport {
            passed: false,
            detail: format!("exit status {status}: {}", text.trim()),
        });
    }
    if let Some(expected) = &spec.expect_stdout_contains {
        if !text.contains(expected.as_str()) {
            return Ok(HealthReport {
                passed: false,
                detail: format!("output does not contain {expected:?}: {}", text.trim()),
            });
        }
    }
    Ok(HealthReport {
        passed: true,
        detail: "tiny case ran and produced the expected output".into(),
    })
}
