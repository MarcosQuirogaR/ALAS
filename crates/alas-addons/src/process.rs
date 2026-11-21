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
//! (`messages` is optional). stderr is captured as the add-on log, line by line
//! as it arrives. A timeout or a cancel kills the whole process tree. A non-zero
//! exit status, a timeout, malformed JSON or output past 1 MiB is an error.
//!
//! Additions that keep version 1 programs working unchanged:
//!
//! - stdout lines beginning `ALAS-PROGRESS [percent] text` are streamed to the
//!   panel as progress and are not part of the reply;
//! - the reply may also carry `"texts": {name: string}`, `"files": [{"label",
//!   "path"}]` (links shown in the panel) and `"config_overlay": "file.json"`
//!   (a `{target: number}` file the user can preview and apply);
//! - the environment carries `ALAS_EXE`, `ALAS_MSES_DIR` and `ALAS_OSMAP`
//!   when those paths are known.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use alas_exec::process::{stop_process_tree, NewProcessGroup, NoConsoleWindow};
use alas_exec::SupervisedSpawn;
use serde_json::Value;

use crate::control::{parse_progress, RunControl, RunEvent, PROGRESS_PREFIX};
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
    /// Named text results.
    pub texts: BTreeMap<String, String>,
    /// Result files the program points at.
    pub files: Vec<ResultFile>,
    /// A `{target: number}` configuration overlay file, if returned.
    pub config_overlay: Option<PathBuf>,
}

/// A file a process add-on produced, shown as a link in the panel.
#[derive(Debug, Clone, PartialEq)]
pub struct ResultFile {
    /// Text of the link.
    pub label: String,
    /// Location; a relative path is relative to the add-on folder.
    pub path: PathBuf,
}

/// How to launch the program.
#[derive(Debug, Clone)]
pub struct ProcessSettings {
    /// Python interpreter for `.py` entries; `None` picks `python` on Windows
    /// and `python3` elsewhere from `PATH`.
    pub python: Option<PathBuf>,
    /// Extra environment variables for the child (`ALAS_EXE`, ...).
    pub env: Vec<(String, String)>,
}

fn default_python() -> PathBuf {
    PathBuf::from(if cfg!(windows) { "python" } else { "python3" })
}

/// Longest line sent as one event; a longer one arrives in pieces.
const MAX_LINE_BYTES: u64 = 16 * 1024;

/// Read `stream` line by line: every line goes to `lines` as it arrives, and
/// the first [`MAX_STREAM_BYTES`] are also kept.
fn drain<R: Read + Send + 'static>(
    stream: R,
    is_stderr: bool,
    lines: Sender<(bool, String)>,
) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut kept = Vec::new();
        let mut line = Vec::new();
        loop {
            line.clear();
            match reader
                .by_ref()
                .take(MAX_LINE_BYTES)
                .read_until(b'\n', &mut line)
            {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
            // Keep reading past the cap so the child never blocks on a full
            // pipe, but stop storing.
            let room = MAX_STREAM_BYTES.saturating_sub(kept.len());
            kept.extend_from_slice(&line[..line.len().min(room)]);
            let text = String::from_utf8_lossy(&line).trim_end().to_owned();
            let _ = lines.send((is_stderr, text));
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
        // `-I` isolates the run: no PYTHON* variables, no user site-packages
        // and no script directory on `sys.path`; an add-on that needs a
        // sibling module inserts its own folder.
        command.args(["-I", "-X", "utf8"]).arg(&path);
        command
    } else {
        Command::new(&path)
    };
    command.current_dir(dir);
    for (key, value) in &settings.env {
        command.env(key, value);
    }
    // No console flash on Windows; on Unix a process group of its own, so a
    // timeout can signal the whole tree (`kill -s KILL -- -pgid`).
    command.no_window().new_process_group();
    command
}

/// Run the add-on's entry file as a child process, without live feedback.
pub fn run_process(
    dir: &Path,
    entry: &str,
    inputs: &BTreeMap<String, f64>,
    design: &Value,
    settings: &ProcessSettings,
    timeout: Duration,
) -> Result<ProcessResult, AddonError> {
    run_process_with(
        dir,
        entry,
        inputs,
        design,
        settings,
        timeout,
        &RunControl::none(),
    )
}

/// Run the add-on's entry file as a child process, streaming progress and
/// stderr lines to `control` and honouring its cancel flag.
pub fn run_process_with(
    dir: &Path,
    entry: &str,
    inputs: &BTreeMap<String, f64>,
    design: &Value,
    settings: &ProcessSettings,
    timeout: Duration,
    control: &RunControl,
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
    let (line_tx, line_rx) = channel();
    let stdout = child
        .stdout
        .take()
        .map(|s| drain(s, false, line_tx.clone()));
    let stderr = child.stderr.take().map(|s| drain(s, true, line_tx.clone()));
    drop(line_tx);
    let relay = |(is_stderr, line): (bool, String)| {
        if is_stderr {
            control.emit(RunEvent::Log(line));
        } else if line.starts_with(PROGRESS_PREFIX) {
            if let Some(event) = parse_progress(&line) {
                control.emit(event);
            }
        }
    };

    let started = Instant::now();
    let mut cancelled = false;
    let status = loop {
        match line_rx.recv_timeout(Duration::from_millis(20)) {
            Ok(line) => relay(line),
            Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => {}
        }
        if control.cancelled() {
            stop_process_tree(&mut child);
            cancelled = true;
            break None;
        }
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= timeout => {
                // The whole tree: `taskkill /F /T` on Windows, a signal to the
                // process group on Unix.
                stop_process_tree(&mut child);
                break None;
            }
            Ok(None) => {}
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

    while let Ok(line) = line_rx.try_recv() {
        relay(line);
    }
    if cancelled {
        return Err(AddonError::Cancelled);
    }
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
    // The reply is stdout without the progress lines.
    let reply_text = String::from_utf8_lossy(&stdout_bytes)
        .lines()
        .filter(|line| !line.starts_with(PROGRESS_PREFIX))
        .collect::<Vec<_>>()
        .join("\n");
    let reply: Value = serde_json::from_str(&reply_text)
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
    let texts = reply
        .get("texts")
        .and_then(Value::as_object)
        .map(|items| {
            items
                .iter()
                .filter_map(|(name, item)| Some((name.clone(), item.as_str()?.to_owned())))
                .collect()
        })
        .unwrap_or_default();
    let files = reply
        .get("files")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(result_file).collect())
        .unwrap_or_default();
    let config_overlay = reply
        .get("config_overlay")
        .and_then(Value::as_str)
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from);
    Ok(ProcessResult {
        outputs,
        messages,
        stderr: stderr_text,
        texts,
        files,
        config_overlay,
    })
}

/// `{"label": .., "path": ..}` or a bare path string.
fn result_file(item: &Value) -> Option<ResultFile> {
    let (label, path) = match item {
        Value::String(path) => (path.as_str(), path.as_str()),
        Value::Object(map) => {
            let path = map.get("path")?.as_str()?;
            (
                map.get("label").and_then(Value::as_str).unwrap_or(path),
                path,
            )
        }
        _ => return None,
    };
    (!path.trim().is_empty()).then(|| ResultFile {
        label: label.to_owned(),
        path: PathBuf::from(path),
    })
}
