// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Bounded output capture and process-tree supervision.

use super::*;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{sync_channel, SyncSender, TryRecvError};
use std::thread;
use std::time::Instant;

use crate::process::{kill_process_tree, NewProcessGroup, NoConsoleWindow};
enum PipeMessage {
    Chunk(OpenFoamOutputStream, Vec<u8>),
    Closed(OpenFoamOutputStream),
}

pub(super) struct CapturedPipe {
    pub(super) bytes: Vec<u8>,
    closed: bool,
}

impl CapturedPipe {
    pub(super) fn new() -> Self {
        Self {
            bytes: Vec::new(),
            closed: false,
        }
    }

    pub(super) fn append(&mut self, bytes: &[u8]) -> usize {
        let remaining = (MAX_CAPTURE_BYTES as usize).saturating_sub(self.bytes.len());
        let accepted = remaining.min(bytes.len());
        self.bytes.extend_from_slice(&bytes[..accepted]);
        accepted
    }
}

pub(super) fn run_command_with_callback<F>(
    command: &OpenFoamCommand,
    cancel: &Arc<AtomicBool>,
    timeout: Duration,
    mut callback: F,
) -> OpenFoamProcessResult
where
    F: FnMut(OpenFoamOutputStream, String),
{
    let started = Instant::now();
    let mut process = Command::new(&command.program);
    process.args(&command.args);
    if let Some(current_dir) = &command.current_dir {
        process.current_dir(current_dir);
    }
    for (key, value) in &command.environment {
        process.env(key, value);
    }
    let spawn = process
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .no_window()
        .new_process_group()
        .spawn();
    let Ok(mut child) = spawn else {
        return OpenFoamProcessResult {
            status: OpenFoamProcessStatus::LaunchFailed,
            exit_code: None,
            stdout: String::new(),
            stderr: format!("could not launch {}", command.program.display()),
            elapsed_seconds: started.elapsed().as_secs_f64(),
        };
    };

    // Reader threads continuously drain both pipes, including bytes beyond
    // the retained cap. The main worker receives bounded chunks through a
    // bounded channel, so a verbose solver cannot deadlock on a full pipe.
    // Reader threads are deliberately detached: after a forced tree kill, a
    // descendant may still hold an inherited pipe handle, and joining here
    // would make cancellation/timeout wait on an unrelated process.
    let (output_tx, output_rx) = sync_channel(OUTPUT_CHANNEL_CAPACITY);
    if let Some(stdout) = child.stdout.take() {
        spawn_pipe_reader(stdout, OpenFoamOutputStream::Stdout, output_tx.clone());
    }
    if let Some(stderr) = child.stderr.take() {
        spawn_pipe_reader(stderr, OpenFoamOutputStream::Stderr, output_tx.clone());
    }
    drop(output_tx);
    let mut stdout = CapturedPipe::new();
    let mut stderr = CapturedPipe::new();
    let mut drain = |blocking: bool| loop {
        let message = if blocking {
            match output_rx.recv_timeout(Duration::from_millis(50)) {
                Ok(message) => message,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => break,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match output_rx.try_recv() {
                Ok(message) => message,
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            }
        };
        consume_pipe_message(message, &mut stdout, &mut stderr, &mut callback);
    };
    let (status, exit_code) = loop {
        drain(false);
        if cancel.load(Ordering::Relaxed) {
            terminate_child(&mut child);
            break (OpenFoamProcessStatus::Cancelled, None);
        }
        if started.elapsed() >= timeout {
            terminate_child(&mut child);
            break (OpenFoamProcessStatus::TimedOut, None);
        }
        match child.try_wait() {
            Ok(Some(result)) => {
                let status = if result.success() {
                    OpenFoamProcessStatus::Completed
                } else {
                    OpenFoamProcessStatus::Failed
                };
                break (status, result.code());
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => {
                terminate_child(&mut child);
                break (OpenFoamProcessStatus::Failed, None);
            }
        }
    };
    // Drain for a bounded period after process exit/kill. In the ordinary
    // case both reader threads close promptly and all output is retained; if
    // a descendant inherited a handle, return the already captured prefix
    // instead of hanging the worker forever.
    let drain_deadline = Instant::now() + OUTPUT_DRAIN_TIMEOUT;
    while !(stdout.closed && stderr.closed) {
        let Some(remaining) = drain_deadline.checked_duration_since(Instant::now()) else {
            break;
        };
        if remaining.is_zero() {
            break;
        }
        let message = match output_rx.recv_timeout(remaining.min(Duration::from_millis(50))) {
            Ok(message) => message,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        consume_pipe_message(message, &mut stdout, &mut stderr, &mut callback);
    }
    OpenFoamProcessResult {
        status,
        exit_code,
        stdout: String::from_utf8_lossy(&stdout.bytes).into_owned(),
        stderr: String::from_utf8_lossy(&stderr.bytes).into_owned(),
        elapsed_seconds: started.elapsed().as_secs_f64(),
    }
}

/// Stop the owned process and reap it without waiting for descendants that
/// may have inherited one of the captured pipe handles.  The tree kill is the
/// normal path; the direct child kill closes the small race where the tree
/// helper cannot see a process between creation and group registration.
fn terminate_child(child: &mut Child) {
    kill_process_tree(child.id());
    if matches!(child.try_wait(), Ok(None)) {
        let _ = child.kill();
    }
    let _ = child.wait();
}

fn spawn_pipe_reader(
    mut pipe: impl Read + Send + 'static,
    stream: OpenFoamOutputStream,
    sender: SyncSender<PipeMessage>,
) {
    let _ = thread::spawn(move || {
        let mut buffer = [0_u8; OUTPUT_CHUNK_BYTES];
        loop {
            match pipe.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => {
                    if sender
                        .send(PipeMessage::Chunk(stream, buffer[..count].to_vec()))
                        .is_err()
                    {
                        break;
                    }
                }
            }
        }
        let _ = sender.send(PipeMessage::Closed(stream));
    });
}

fn consume_pipe_message<F>(
    message: PipeMessage,
    stdout: &mut CapturedPipe,
    stderr: &mut CapturedPipe,
    callback: &mut F,
) where
    F: FnMut(OpenFoamOutputStream, String),
{
    match message {
        PipeMessage::Chunk(stream, bytes) => {
            let target = match stream {
                OpenFoamOutputStream::Stdout => stdout,
                OpenFoamOutputStream::Stderr => stderr,
            };
            let accepted = target.append(&bytes);
            if accepted > 0 {
                callback(
                    stream,
                    String::from_utf8_lossy(&bytes[..accepted]).into_owned(),
                );
            }
        }
        PipeMessage::Closed(stream) => match stream {
            OpenFoamOutputStream::Stdout => stdout.closed = true,
            OpenFoamOutputStream::Stderr => stderr.closed = true,
        },
    }
}
