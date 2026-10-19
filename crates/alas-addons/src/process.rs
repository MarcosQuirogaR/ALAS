// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Process add-ons: an external program speaking JSON over stdin and stdout.
//!
//! WARNING: this is not a sandbox. The child runs with the user's own
//! privileges and can do anything the user can. The caller must have the user
//! enable each such add-on explicitly (see [`crate::settings`]).
//!
//! Protocol version 1. The program reads one JSON object from stdin,
//! `{"protocol": 1, "inputs": {name: number}, "design": {...}}`, and writes one
//! JSON object to stdout, `{"outputs": {name: number}, "messages": [text]}`
//! (`messages` is optional). stderr is captured as the add-on log. A timeout kills
//! the whole process tree. A non-zero
//! exit status, a timeout, malformed JSON or output past 1 MiB is an error.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use alas_exec::process::{stop_process_tree, NewProcessGroup, NoConsoleWindow};
use alas_exec::SupervisedSpawn;
use serde_json::Value;

use crate::error::AddonError;

/// Cap on captured stdout and on captured stderr, bytes each.
pub const MAX_STREAM_BYTES: usize = 1024 * 1024;

/// Wire protocol version sent in the request.
pub const PROTOCOL_VERSION: u32 = 1;

/// What a process run produced.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessResult {
    /// Declared outputs as returned.
    pub outputs: BTreeMap<String, f64>,
    /// Free-text messages from the program.
    pub messages: Vec<String>,
    /// Captured stderr, the add-on log.
    pub stderr: String,
}

/// How to launch the program.
#[derive(Debug, Clone)]
pub struct ProcessSettings {
    /// Python interpreter for `.py` entries; `None` picks `python` on Windows
    /// and `python3` elsewhere from `PATH`.
    pub python: Option<PathBuf>,
}

fn default_python() -> PathBuf {
    PathBuf::from(if cfg!(windows) { "python" } else { "python3" })
}

fn drain<R: Read + Send + 'static>(mut stream: R) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut chunk = [0_u8; 8192];
        while let Ok(read) = stream.read(&mut chunk) {
            if read == 0 {
                break;
            }
            // Keep reading past the cap so the child never blocks on a full
            // pipe, but stop storing.
            let room = MAX_STREAM_BYTES.saturating_sub(kept.len());
            kept.extend_from_slice(&chunk[..read.min(room)]);
        }
        kept
    })
}

/// Build the command for `entry` inside `dir`.
fn command_for(dir: &Path, entry: &str, settings: &ProcessSettings) -> Command {
    let path = dir.join(entry);
    let is_python = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("py"));
    let mut command = if is_python {
        let mut command = Command::new(settings.python.clone().unwrap_or_else(default_python));
        command.args(["-X", "utf8"]).arg(&path);
        command
    } else {
        Command::new(&path)
    };
    command.current_dir(dir);
    // No console flash on Windows; on Unix a process group of its own, so a
    // timeout can signal the whole tree (`kill -s KILL -- -pgid`).
    command.no_window().new_process_group();
    command
}

/// Run the add-on's entry file as a child process.
pub fn run_process(
    dir: &Path,
    entry: &str,
    inputs: &BTreeMap<String, f64>,
    design: &Value,
    settings: &ProcessSettings,
    timeout: Duration,
) -> Result<ProcessResult, AddonError> {
    let request = serde_json::json!({
        "protocol": PROTOCOL_VERSION,
        "inputs": inputs,
        "design": design,
    });
    let mut child = command_for(dir, entry, settings)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Placed in the application's kill-on-close Windows job object, so
        // ending ALAS ends the add-on and anything it started.
        .spawn_supervised("addon")
        .map_err(|error| AddonError::Process(format!("cannot start `{entry}`: {error}")))?;

    let stdin = child.stdin.take();
    let writer = thread::spawn(move || {
        if let Some(mut stdin) = stdin {
            // A child that exits without reading closes the pipe; the exit
            // status below reports that, so a write error is not separate.
            let _ = stdin.write_all(request.to_string().as_bytes());
        }
    });
    let stdout = child.stdout.take().map(drain);
    let stderr = child.stderr.take().map(drain);

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= timeout => {
                // The whole tree: `taskkill /F /T` on Windows, a signal to the
                // process group on Unix.
                stop_process_tree(&mut child);
                break None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(error) => return Err(AddonError::Process(error.to_string())),
        }
    };
    let _ = writer.join();
    let stdout_bytes = stdout
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default();
    let stderr_text = String::from_utf8_lossy(
        &stderr
            .and_then(|handle| handle.join().ok())
            .unwrap_or_default(),
    )
    .into_owned();

    let Some(status) = status else {
        return Err(AddonError::Timeout(timeout.as_secs_f64()));
    };
    if !status.success() {
        let tail: String = stderr_text
            .lines()
            .rev()
            .take(3)
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(AddonError::Process(format!(
            "exit status {status}; stderr: {tail}"
        )));
    }
    if stdout_bytes.len() >= MAX_STREAM_BYTES {
        return Err(AddonError::Outputs("stdout exceeded 1 MiB".to_owned()));
    }
    let reply: Value = serde_json::from_slice(&stdout_bytes)
        .map_err(|error| AddonError::Outputs(format!("stdout is not valid JSON: {error}")))?;
    let object = reply
        .get("outputs")
        .and_then(Value::as_object)
        .ok_or_else(|| AddonError::Outputs("missing `outputs` object".to_owned()))?;
    let mut outputs = BTreeMap::new();
    for (name, value) in object {
        let number = value
            .as_f64()
            .ok_or_else(|| AddonError::Outputs(format!("output `{name}` is not a number")))?;
        outputs.insert(name.clone(), number);
    }
    let messages = reply
        .get("messages")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    Ok(ProcessResult {
        outputs,
        messages,
        stderr: stderr_text,
    })
}
