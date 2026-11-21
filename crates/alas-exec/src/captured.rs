// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Run a short helper program to completion with its output captured.
//!
//! `Command::output` has no deadline and, when ALAS is closed mid-call, leaves
//! the child behind. The helpers ALAS shells out to (`curl`, `tar`, a script
//! host) take the same route as the solvers instead: spawned under the
//! supervision job ([`SupervisedSpawn`]), in their own process group, with
//! both pipes drained while the deadline runs and the whole tree killed when
//! it passes.

use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

use crate::process::{
    drain, drained_text, wait_with_timeout, DeadlineWait, NewProcessGroup, NoConsoleWindow,
};
use crate::supervise::SupervisedSpawn;

/// What a helper wrote and how it ended.
#[derive(Debug)]
pub struct CapturedOutput {
    /// The exit status.
    pub status: ExitStatus,
    /// Standard output, decoded lossily (bounded read, see [`drained_text`]).
    pub stdout: String,
    /// Standard error, decoded lossily.
    pub stderr: String,
}

/// Why a helper produced no [`CapturedOutput`].
#[derive(Debug)]
pub enum CapturedError {
    /// The program could not be started.
    Launch(std::io::Error),
    /// The deadline passed; the process tree was killed.
    TimedOut(Duration),
    /// The process state could not be polled; the process tree was killed.
    Wait(std::io::Error),
}

impl std::fmt::Display for CapturedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Launch(error) => write!(f, "{error}"),
            Self::TimedOut(timeout) => {
                write!(
                    f,
                    "timed out after {:.0}s (tree force-killed)",
                    timeout.as_secs_f64()
                )
            }
            Self::Wait(error) => write!(f, "failed while waiting: {error}"),
        }
    }
}

impl std::error::Error for CapturedError {}

/// Spawn `command` under `role`, capture both output streams and hold it to
/// `timeout` (see [`crate::process::timeout_from_seconds`]).
///
/// Standard input is always null (what `Command::output` gave these callers
/// before) and output is always piped. Output is read with the bounded
/// [`drained_text`], so a descendant that outlives the helper and keeps a pipe
/// open delays this call by at most a short grace period.
pub fn run_captured(
    command: &mut Command,
    role: &str,
    timeout: Duration,
) -> Result<CapturedOutput, CapturedError> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .no_window()
        .new_process_group();
    let mut child = command
        .spawn_supervised(role)
        .map_err(CapturedError::Launch)?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let status = match wait_with_timeout(&mut child, timeout) {
        DeadlineWait::Exited(status) => status,
        DeadlineWait::TimedOut => return Err(CapturedError::TimedOut(timeout)),
        DeadlineWait::PollFailed(error) => return Err(CapturedError::Wait(error)),
    };
    Ok(CapturedOutput {
        status,
        stdout: drained_text(stdout),
        stderr: drained_text(stderr),
    })
}

// A failed expect in a test is the helper failing in the test environment, not a
// library invariant being broken.
#[allow(clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn shell(script: &str) -> Command {
        use std::os::windows::process::CommandExt;
        let mut command = Command::new("cmd");
        command.args(["/d", "/c"]).raw_arg(script);
        command
    }

    #[cfg(not(windows))]
    fn shell(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.args(["-c", script]);
        command
    }

    #[test]
    fn output_and_status_are_captured() {
        let mut command = shell(if cfg!(windows) {
            "echo out & echo err 1>&2 & exit 3"
        } else {
            "echo out; echo err >&2; exit 3"
        });
        let output = run_captured(&mut command, "test helper", Duration::from_secs(20))
            .expect("the helper runs");
        assert_eq!(output.status.code(), Some(3));
        assert!(output.stdout.contains("out"));
        assert!(output.stderr.contains("err"));
    }

    #[test]
    fn a_missing_program_is_a_launch_error() {
        let mut command = Command::new("alas-definitely-not-a-program");
        let error = run_captured(&mut command, "test helper", Duration::from_secs(5))
            .expect_err("nothing to launch");
        assert!(matches!(error, CapturedError::Launch(_)), "{error}");
    }

    #[test]
    fn a_hung_helper_is_killed_at_the_deadline() {
        let mut command = if cfg!(windows) {
            let mut command = Command::new("ping");
            command.args(["-n", "30", "127.0.0.1"]);
            command
        } else {
            shell("sleep 30")
        };
        let started = std::time::Instant::now();
        let error = run_captured(&mut command, "test helper", Duration::from_millis(400))
            .expect_err("the deadline passes");
        assert!(matches!(error, CapturedError::TimedOut(_)), "{error}");
        assert!(started.elapsed() < Duration::from_secs(15));
    }
}
