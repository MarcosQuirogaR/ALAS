// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Spawning the solver, feeding the deck and holding to the timeout.

use super::*;

/// Spawn, feed the deck, hold to the timeout, and return stdout as the print
/// file.
pub(super) fn supervise(mut command: Command, deck: &str, timeout: Duration) -> RunOutcome {
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .no_window()
        .new_process_group();
    let spawned = alas_exec::SupervisedSpawn::spawn_supervised(&mut command, "NASTRAN-95 solve");
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => return RunOutcome::Failed(format!("failed to launch nastran.exe: {error}")),
    };

    if let Some(mut stdin) = child.stdin.take() {
        let owned = deck.to_owned();
        // Write on a thread: a deck larger than the pipe buffer would otherwise
        // deadlock against a solver already reading and echoing it.
        thread::spawn(move || {
            let _ = stdin.write_all(owned.as_bytes());
        });
    }
    let stdout_reader = drain(child.stdout.take());
    let stderr_reader = drain(child.stderr.take());

    let status = match wait_with_timeout(&mut child, timeout) {
        DeadlineWait::Exited(status) => status,
        DeadlineWait::TimedOut => {
            return RunOutcome::Failed(format!(
                "nastran.exe timed out after {:.0}s (tree force-killed)",
                timeout.as_secs_f64()
            ));
        }
        DeadlineWait::PollFailed(error) => {
            return RunOutcome::Failed(format!("failed while waiting on nastran.exe: {error}"));
        }
    };

    let stdout = drained_text(stdout_reader);
    let stderr = drained_text(stderr_reader);
    if !status.success() {
        let code = status
            .code()
            .map_or_else(|| "a signal".to_owned(), |code| code.to_string());
        return RunOutcome::Failed(format!(
            "nastran.exe exited with code {code} (stdout tail: {}; stderr tail: {})",
            tail(&stdout),
            tail(&stderr)
        ));
    }
    let fatals = fatal_lines(&stdout);
    if !fatals.is_empty() {
        return RunOutcome::Failed(format!(
            "print file reports {} fatal message(s):\n{}",
            fatals.len(),
            fatals.join("\n")
        ));
    }
    if read_displacement_tables(&stdout).is_empty() && read_eigenvalues(&stdout).is_empty() {
        return RunOutcome::Failed(format!(
            "nastran.exe wrote no result table (stderr tail: {})",
            tail(&stderr)
        ));
    }
    RunOutcome::Print(stdout)
}

/// Every fatal message the print file carries, the way [`crate::nastran::run`]
/// scans for them: across form feeds, not only newlines.
pub(super) fn fatal_lines(print: &str) -> Vec<String> {
    text::splitlines(print)
        .into_iter()
        .filter(|line| line.contains("USER FATAL") || line.contains("SYSTEM FATAL"))
        .map(|line| text::strip(line).to_owned())
        .collect()
}

fn tail(text_input: &str) -> String {
    let lines = text::splitlines(text::strip(text_input));
    lines[lines.len().saturating_sub(6)..].join(" | ")
}
