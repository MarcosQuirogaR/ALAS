// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Resumable, hash-verified transfer of one archive through `curl`.

use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::process::{NewProcessGroup, NoConsoleWindow};
use crate::sha256::Sha256;
use crate::supervise::SupervisedSpawn;

use super::InstallError;

/// Transport switches. The default is the production policy: HTTPS only.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransferOptions {
    /// Also accept plain `http://` URLs whose host is a loopback address.
    /// Exists so the test suite can serve archives from a local mock server;
    /// the application never sets it.
    pub allow_loopback_http: bool,
}

/// SHA-256 of a file, streamed.
pub(super) fn file_sha256(path: &Path) -> Result<String, InstallError> {
    let mut file = File::open(path)
        .map_err(|error| InstallError::Io(format!("cannot open {}: {error}", path.display())))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            InstallError::Io(format!("cannot read {}: {error}", path.display()))
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize_hex())
}

fn is_loopback_http(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit_once(':').map_or(host, |(name, port)| {
        if port.bytes().all(|b| b.is_ascii_digit()) {
            name
        } else {
            host
        }
    });
    matches!(host, "127.0.0.1" | "localhost" | "[::1]")
}

pub(super) fn validate_url(url: &str, options: TransferOptions) -> Result<(), InstallError> {
    if url.starts_with("https://") || (options.allow_loopback_http && is_loopback_http(url)) {
        return Ok(());
    }
    Err(InstallError::Download(format!(
        "refusing non-HTTPS source {url}"
    )))
}

/// Download `url` into `part`, resuming a previous partial file, then verify
/// size floor and SHA-256 and return. `on_bytes` receives the current size of
/// the part file about every 200 ms.
///
/// A part file that already has the pinned hash is accepted without touching
/// the network. A finished transfer with the wrong hash is deleted: a resumed
/// prefix from a different upstream build must not poison later attempts.
pub(super) struct Job<'a> {
    pub name: &'a str,
    pub url: &'a str,
    pub sha256: &'a str,
    pub min_bytes: u64,
    pub part: &'a Path,
    pub options: TransferOptions,
}

pub(super) fn fetch_verified(
    job: &Job<'_>,
    cancel: &AtomicBool,
    on_bytes: &mut dyn FnMut(u64),
) -> Result<(), InstallError> {
    fetch_attempt(job, cancel, on_bytes, true)
}

/// One fetch; `may_restart` allows a single restart from zero when the server
/// turns out not to honour `Range`.
fn fetch_attempt(
    job: &Job<'_>,
    cancel: &AtomicBool,
    on_bytes: &mut dyn FnMut(u64),
    may_restart: bool,
) -> Result<(), InstallError> {
    let Job {
        name,
        url,
        sha256,
        min_bytes,
        part,
        options,
    } = *job;
    validate_url(url, options)?;
    let had_partial = fs::metadata(part).is_ok_and(|meta| meta.len() > 0);
    if part.is_file() && file_sha256(part)? == sha256 {
        return Ok(());
    }
    let executable = if cfg!(windows) { "curl.exe" } else { "curl" };
    let protocols = if options.allow_loopback_http {
        "=https,http"
    } else {
        "=https"
    };
    let mut command = Command::new(executable);
    command
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--location",
            "--proto",
            protocols,
            "--proto-redir",
            "=https",
            "--continue-at",
            "-",
            "--connect-timeout",
            "15",
            // Abort a stalled transfer rather than cap the whole one: some
            // archives are several hundred megabytes.
            "--speed-limit",
            "1000",
            "--speed-time",
            "60",
            "--retry",
            "3",
            "--retry-delay",
            "2",
            "--user-agent",
            "ALAS/1.5 (tool installer)",
        ])
        .arg("--output")
        .arg(part)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .no_window()
        .new_process_group();
    let mut child = command
        .spawn_supervised("tool installer download")
        .map_err(|error| InstallError::Download(format!("cannot launch {executable}: {error}")))?;
    let stderr_reader = child.stderr.take().map(|mut stderr| {
        thread::spawn(move || {
            let mut text = String::new();
            let _ = stderr.read_to_string(&mut text);
            text
        })
    });
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            // The part file is kept so the next attempt resumes.
            return Err(InstallError::Cancelled);
        }
        on_bytes(fs::metadata(part).map_or(0, |meta| meta.len()));
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(200)),
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(InstallError::Download(format!(
                    "cannot poll {executable}: {error}"
                )));
            }
        }
    };
    let stderr_text = stderr_reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default();
    if !status.success() {
        // A complete part file makes curl report 416 on resume; accept it only
        // if the hash proves it complete.
        if part.is_file() && file_sha256(part)? == sha256 {
            return Ok(());
        }
        // curl exit 33: the server ignored the `Range` request and sent the whole
        // body (GitHub's codeload archives are generated on the fly and do this).
        // The kept part file could then never be resumed, so start again from zero,
        // once.
        if status.code() == Some(33) && had_partial && may_restart {
            fs::remove_file(part).map_err(|error| {
                InstallError::Download(format!(
                    "{name}: the server cannot resume and {} cannot be removed to restart: {error}",
                    part.display()
                ))
            })?;
            return fetch_attempt(job, cancel, on_bytes, false);
        }
        return Err(InstallError::Download(format!(
            "{name}: {} ({status}); the partial file is kept and the next attempt resumes",
            stderr_text.trim()
        )));
    }
    let size = fs::metadata(part).map_or(0, |meta| meta.len());
    on_bytes(size);
    if size < min_bytes {
        let _ = fs::remove_file(part);
        return Err(InstallError::Download(format!(
            "{name}: incomplete transfer, {size} bytes, expected at least {min_bytes}"
        )));
    }
    let actual = file_sha256(part)?;
    if actual != sha256 {
        let _ = fs::remove_file(part);
        return Err(InstallError::HashMismatch {
            name: name.to_owned(),
            expected: sha256.to_owned(),
            actual,
        });
    }
    Ok(())
}
