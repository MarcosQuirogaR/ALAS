// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Small, policy-free HTTPS asset downloader used by the application layer.
//!
//! The route crate owns asset URLs and validation floors, while this crate owns
//! the operating-system transport.  Keeping those concerns separate means
//! the geometry/routing libraries remain network-free without leaving the
//! desktop setup page with a dead download control.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::process::{NewProcessGroup, NoConsoleWindow};
use crate::supervise::SupervisedSpawn;

/// One HTTPS file to download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadSpec {
    /// File name relative to the destination directory.
    pub name: String,
    /// HTTPS source URL.
    pub url: String,
    /// Minimum size accepted as a completed transfer.
    pub min_bytes: u64,
    /// A SHA-256, lowercase hex, of the last content a maintainer reviewed
    /// and accepted for this asset.
    ///
    /// `None` means this transfer is validated by `min_bytes` alone, exactly
    /// as before this field existed. When present, a mismatch means the
    /// source now serves content different from what was last reviewed:
    /// for an unpinned mirror (one with no release tags to pin against) an
    /// upstream content edit produces the exact same symptom as tampering,
    /// so this alone must not be reported as proof of tampering. Either way
    /// the file is not installed until a maintainer reviews the new content
    /// and updates the pinned hash.
    pub expected_sha256: Option<String>,
}

impl DownloadSpec {
    /// Construct a download specification from the caller's asset metadata.
    pub fn new(name: impl Into<String>, url: impl Into<String>, min_bytes: u64) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            min_bytes,
            expected_sha256: None,
        }
    }

    /// Attach a pinned reviewed-content hash to an existing specification.
    pub fn with_reviewed_sha256(mut self, sha256: impl Into<String>) -> Self {
        self.expected_sha256 = Some(sha256.into().to_lowercase());
        self
    }
}

/// Summary of one asset download operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadReport {
    /// Directory into which the files were installed.
    pub target_dir: PathBuf,
    /// Files newly downloaded and atomically moved into place as far as the
    /// platform permits.
    pub downloaded: Vec<String>,
    /// Files already meeting their validation floors.
    pub skipped: Vec<String>,
}

/// How one `download_files` call ended.
///
/// Cancellation is deliberately not an `Err`: asking a background transfer to
/// stop is an ordinary, successful outcome from the caller's request, not a
/// network or verification failure, and a caller must be able to tell the two
/// apart without parsing an error string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadOutcome {
    /// Every spec was downloaded or already present.
    Completed(DownloadReport),
    /// The cancellation signal was observed before every spec finished.
    /// `DownloadReport` describes only the files that completed, atomically
    /// and verified, before the stop, never a partially written file.
    Cancelled(DownloadReport),
}

impl DownloadOutcome {
    /// The report of files installed or skipped before this outcome, whether
    /// the transfer ran to completion or was cancelled partway through.
    pub fn report(&self) -> &DownloadReport {
        match self {
            DownloadOutcome::Completed(report) | DownloadOutcome::Cancelled(report) => report,
        }
    }

    /// Whether cancellation, rather than full completion, produced this
    /// outcome.
    pub fn is_cancelled(&self) -> bool {
        matches!(self, DownloadOutcome::Cancelled(_))
    }
}

/// Download all incomplete files into `target_dir`.
///
/// Each transfer is written to a private part file and validated (size floor,
/// and a pinned hash when the spec carries one) before the destination is
/// replaced.  A cancelled or failed curl invocation therefore cannot leave a
/// short or mismatched file that later looks like usable navdata.  Existing
/// valid files are skipped, which makes repeated setup actions cheap and safe.
///
/// `cancel` is polled between specs and, since the transfer itself runs as a
/// child process rather than through an in-process streaming client, between
/// short waits on the file currently in flight: a cancellation request does
/// not wait for the largest single file to finish downloading first.
pub fn download_files(
    specs: &[DownloadSpec],
    target_dir: &Path,
    timeout_seconds: f64,
    cancel: &AtomicBool,
) -> Result<DownloadOutcome, String> {
    if !timeout_seconds.is_finite() || timeout_seconds <= 0.0 {
        return Err(format!(
            "invalid download timeout {timeout_seconds:?} s; expected a finite positive value"
        ));
    }
    fs::create_dir_all(target_dir)
        .map_err(|error| format!("cannot create {}: {error}", target_dir.display()))?;
    if !target_dir.is_dir() {
        return Err(format!(
            "download target is not a directory: {}",
            target_dir.display()
        ));
    }

    let executable = if cfg!(windows) { "curl.exe" } else { "curl" };
    let timeout = timeout_seconds.clamp(1.0, 600.0).to_string();
    let connect_timeout = timeout_seconds.clamp(1.0, 10.0).to_string();
    let mut downloaded = Vec::new();
    let mut skipped = Vec::new();

    // Test builds only: local-fixture transfers use a `file://` URL served
    // from a temporary directory rather than any live network endpoint.
    // `--proto` is a curl-level allow-list independent of the URL itself, so
    // both this and `validate_spec` below must admit the scheme, and only in
    // a test build, for a fixture test to exercise the real transfer path
    // without a live download.
    let allowed_protocols = if cfg!(test) { "=https,file" } else { "=https" };

    for (index, spec) in specs.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Ok(DownloadOutcome::Cancelled(DownloadReport {
                target_dir: target_dir.to_path_buf(),
                downloaded,
                skipped,
            }));
        }
        validate_spec(spec)?;
        let destination = target_dir.join(&spec.name);
        if is_usable_file(&destination, spec.min_bytes) {
            skipped.push(spec.name.clone());
            continue;
        }

        let temporary = target_dir.join(format!(
            ".{}.alas-download-{}-{}.part",
            spec.name,
            std::process::id(),
            index
        ));
        let _ = fs::remove_file(&temporary);
        let mut command = Command::new(executable);
        command
            .args([
                "--silent",
                "--show-error",
                "--fail-with-body",
                "--location",
                "--proto",
                allowed_protocols,
                "--connect-timeout",
                &connect_timeout,
                "--max-time",
                &timeout,
                "--retry",
                "2",
                "--retry-delay",
                "1",
                "--retry-max-time",
                &timeout,
                "--user-agent",
                "ALAS/1.0 (optional asset download)",
            ])
            .arg("--output")
            .arg(&temporary)
            .arg(&spec.url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .no_window()
            .new_process_group();
        let mut child = match command.spawn_supervised("curl download") {
            Ok(child) => child,
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                return Err(format!("cannot launch {executable}: {error}"));
            }
        };
        // Drain stderr while curl runs: a retrying transfer can write more
        // than the pipe buffer holds, and a child blocked on a full pipe
        // never exits for `try_wait` to see.
        let stderr_reader = child.stderr.take().map(|mut stderr| {
            thread::spawn(move || {
                use std::io::Read;
                let mut text = String::new();
                let _ = stderr.read_to_string(&mut text);
                text
            })
        });

        // Poll rather than block on `wait()` so a cancellation request can
        // kill the transfer in flight instead of waiting for it to finish:
        // curl is a subprocess here, not an in-process streaming client, so
        // this poll loop is the mechanism available for sub-file
        // responsiveness. Once `try_wait` reports an exit, the child has
        // already been reaped; the exit status from that call is used
        // directly rather than reaped a second time via `wait`.
        let status = loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                let _ = child.wait();
                let _ = fs::remove_file(&temporary);
                return Ok(DownloadOutcome::Cancelled(DownloadReport {
                    target_dir: target_dir.to_path_buf(),
                    downloaded,
                    skipped,
                }));
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => thread::sleep(Duration::from_millis(50)),
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = fs::remove_file(&temporary);
                    return Err(format!("cannot poll {executable}: {error}"));
                }
            }
        };
        let stderr_text = stderr_reader
            .and_then(|reader| reader.join().ok())
            .unwrap_or_default();

        if !status.success() {
            let body = fs::read_to_string(&temporary).unwrap_or_default();
            let _ = fs::remove_file(&temporary);
            return Err(format!(
                "download of {} failed ({}): {}",
                spec.name,
                status,
                diagnostic_text(&body, &stderr_text)
            ));
        }
        let Some(bytes) = file_size(&temporary) else {
            let _ = fs::remove_file(&temporary);
            return Err(format!(
                "download of {} produced no regular file",
                spec.name
            ));
        };
        if bytes < spec.min_bytes {
            let _ = fs::remove_file(&temporary);
            return Err(format!(
                "download of {} is incomplete: {bytes} bytes, expected at least {}",
                spec.name, spec.min_bytes
            ));
        }
        if let Some(expected) = &spec.expected_sha256 {
            let contents = match fs::read(&temporary) {
                Ok(contents) => contents,
                Err(error) => {
                    let _ = fs::remove_file(&temporary);
                    return Err(format!(
                        "cannot read {} to verify its content hash: {error}",
                        temporary.display()
                    ));
                }
            };
            let actual = sha256::hex_digest(&contents);
            if !actual.eq_ignore_ascii_case(expected) {
                let _ = fs::remove_file(&temporary);
                return Err(format!(
                    "content hash for {} does not match the last reviewed value (expected {expected}, got {actual}); the source may simply have changed since it was last reviewed, not necessarily tampering. A maintainer must look at the new content and update the pinned hash before it is trusted",
                    spec.name
                ));
            }
        }

        // `fs::rename` replaces an existing file on every supported platform
        // (on Windows it asks for `MOVEFILE_REPLACE_EXISTING` semantics), so
        // an invalid old destination is swapped for the verified file in one
        // step, with no moment at which neither exists. A valid old
        // destination was skipped above.
        if let Err(error) = fs::rename(&temporary, &destination) {
            let _ = fs::remove_file(&temporary);
            return Err(format!("cannot install {}: {error}", destination.display()));
        }
        downloaded.push(spec.name.clone());
    }

    Ok(DownloadOutcome::Completed(DownloadReport {
        target_dir: target_dir.to_path_buf(),
        downloaded,
        skipped,
    }))
}

fn validate_spec(spec: &DownloadSpec) -> Result<(), String> {
    let path = Path::new(&spec.name);
    let safe_name = path.components().count() == 1
        && matches!(path.components().next(), Some(Component::Normal(_)));
    if !safe_name {
        return Err(format!(
            "refusing asset name outside the download directory: {}",
            spec.name
        ));
    }
    // Only a test build may serve a spec from a local `file://` fixture, so
    // an automated test can exercise the real transfer path (staging,
    // verification, atomic install) without a live network call. This is
    // additive to, never a replacement for, the HTTPS requirement below.
    let scheme_ok =
        spec.url.starts_with("https://") || (cfg!(test) && spec.url.starts_with("file://"));
    if !scheme_ok {
        return Err(format!("refusing non-HTTPS asset URL for {}", spec.name));
    }
    if spec.min_bytes == 0 {
        return Err(format!("asset {} has no positive size floor", spec.name));
    }
    Ok(())
}

fn file_size(path: &Path) -> Option<u64> {
    path.metadata()
        .ok()
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
}

fn is_usable_file(path: &Path, min_bytes: u64) -> bool {
    file_size(path).is_some_and(|bytes| bytes >= min_bytes)
}

fn diagnostic_text(body: &str, stderr: &str) -> String {
    const MAX_CHARS: usize = 2_000;
    let body = body.trim();
    let stderr = stderr.trim();
    let combined = match (body.is_empty(), stderr.is_empty()) {
        (true, true) => "no response body or stderr".to_owned(),
        (false, true) => format!("response: {body}"),
        (true, false) => format!("curl: {stderr}"),
        (false, false) => format!("response: {body}; curl: {stderr}"),
    };
    let mut result = combined.chars().take(MAX_CHARS).collect::<String>();
    if result.chars().count() < combined.chars().count() {
        result.push_str(" \u{2026}");
    }
    result.replace(['\r', '\n'], " ")
}

/// SHA-256 for verifying a completed download against a pinned reviewed-content
/// hash, using the shared FIPS 180-4 implementation in [`crate::sha256`] (also
/// the result cache's key hash). A dedicated hashing crate was deliberately not
/// added: the dependency-graph and lockfile churn is disproportionate.
mod sha256 {
    /// The lowercase hex SHA-256 digest of `data`.
    pub(super) fn hex_digest(data: &[u8]) -> String {
        let mut hasher = crate::sha256::Sha256::new();
        hasher.update(data);
        hasher.finalize_hex()
    }

    // A test asserts on values it constructed here directly, so a failed
    // unwrap or expect is the assertion failing, not a library invariant
    // being broken.
    #[allow(clippy::unwrap_used, clippy::expect_used)]
    #[cfg(test)]
    mod tests {
        use super::hex_digest;

        #[test]
        fn empty_input_matches_the_published_nist_test_vector() {
            assert_eq!(
                hex_digest(b""),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            );
        }

        #[test]
        fn abc_matches_the_published_nist_test_vector() {
            assert_eq!(
                hex_digest(b"abc"),
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
            );
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_specs_require_https_and_a_single_filename() {
        let base = DownloadSpec::new("earth_fix.dat", "https://example.test/fix", 10);
        assert!(validate_spec(&base).is_ok());
        assert!(validate_spec(&DownloadSpec::new("../fix", &base.url, 10)).is_err());
        assert!(validate_spec(&DownloadSpec::new("fix", "http://example.test/fix", 10)).is_err());
    }

    #[test]
    fn invalid_timeout_is_rejected_before_target_creation() {
        let target =
            std::env::temp_dir().join(format!("alas-download-invalid-{}", std::process::id()));
        let error = download_files(&[], &target, f64::NAN, &AtomicBool::new(false))
            .expect_err("NaN timeout");
        assert!(error.contains("invalid download timeout"));
        assert!(!target.exists());
    }

    /// A `file://` URL to a local fixture, used only under `cfg(test)` so a
    /// unit test can exercise the real curl transfer path without any live
    /// network call.
    fn fixture_url(path: &Path) -> String {
        let forward_slashes = path.to_string_lossy().replace('\\', "/");
        if cfg!(windows) {
            format!("file:///{forward_slashes}")
        } else if forward_slashes.starts_with('/') {
            format!("file://{forward_slashes}")
        } else {
            format!("file:///{forward_slashes}")
        }
    }

    fn unique_dir(label: &str) -> PathBuf {
        use std::sync::atomic::AtomicU64;
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "alas-download-{label}-{}-{sequence}",
            std::process::id(),
        ))
    }

    #[test]
    fn an_undersized_existing_file_is_replaced_by_the_verified_transfer() {
        let source_dir = unique_dir("replace-src");
        fs::create_dir_all(&source_dir).expect("create fixture source directory");
        let source_file = source_dir.join("payload.dat");
        let content = b"complete navdata fixture content".repeat(64);
        fs::write(&source_file, &content).expect("write fixture payload");

        let target = unique_dir("replace-target");
        fs::create_dir_all(&target).expect("create target directory");
        fs::write(target.join("payload.dat"), b"short").expect("write undersized file");
        let spec = DownloadSpec::new("payload.dat", fixture_url(&source_file), 100);
        let outcome = download_files(&[spec], &target, 30.0, &AtomicBool::new(false))
            .expect("an undersized destination is replaced, not an error");
        assert_eq!(outcome.report().downloaded, vec!["payload.dat".to_owned()]);
        assert_eq!(
            fs::read(target.join("payload.dat")).expect("read installed file"),
            content
        );

        let _ = fs::remove_dir_all(&target);
        let _ = fs::remove_dir_all(&source_dir);
    }

    #[test]
    fn a_correct_pinned_hash_installs_the_file_and_a_wrong_one_leaves_none() {
        let source_dir = unique_dir("hash-src");
        fs::create_dir_all(&source_dir).expect("create fixture source directory");
        let source_file = source_dir.join("payload.dat");
        let content = b"reviewed navdata fixture content".repeat(64);
        fs::write(&source_file, &content).expect("write fixture payload");
        let digest = sha256::hex_digest(&content);
        let url = fixture_url(&source_file);

        let target = unique_dir("hash-good");
        let good_spec =
            DownloadSpec::new("payload.dat", url.clone(), 10).with_reviewed_sha256(digest);
        let outcome = download_files(&[good_spec], &target, 30.0, &AtomicBool::new(false))
            .expect("a correctly hashed transfer must succeed");
        assert!(!outcome.is_cancelled());
        assert_eq!(outcome.report().downloaded, vec!["payload.dat".to_owned()]);
        assert!(target.join("payload.dat").is_file());
        let _ = fs::remove_dir_all(&target);

        let target = unique_dir("hash-bad");
        let bad_spec =
            DownloadSpec::new("payload.dat", url, 10).with_reviewed_sha256("0".repeat(64));
        let error = download_files(&[bad_spec], &target, 30.0, &AtomicBool::new(false))
            .expect_err("a mismatched hash must not install the file");
        assert!(error.contains("does not match"), "{error}");
        assert!(
            error.contains("not necessarily tampering"),
            "message must not assert mismatch is proven tampering: {error}"
        );
        assert!(!target.join("payload.dat").exists());

        let _ = fs::remove_dir_all(&source_dir);
        let _ = fs::remove_dir_all(&target);
    }

    #[test]
    fn a_cancellation_requested_before_the_call_stops_before_the_first_file_and_is_distinct_from_an_error(
    ) {
        let source_dir = unique_dir("cancel-src");
        fs::create_dir_all(&source_dir).expect("create fixture source directory");
        let first = source_dir.join("first.dat");
        let second = source_dir.join("second.dat");
        fs::write(&first, b"first fixture file".repeat(8)).expect("write first fixture");
        fs::write(&second, b"second fixture file".repeat(8)).expect("write second fixture");

        let target = unique_dir("cancel-target");
        let specs = vec![
            DownloadSpec::new("first.dat", fixture_url(&first), 10),
            DownloadSpec::new("second.dat", fixture_url(&second), 10),
        ];
        let cancel = AtomicBool::new(true);

        let outcome = download_files(&specs, &target, 30.0, &cancel)
            .expect("a cancellation must be a successful, distinct outcome");

        assert!(outcome.is_cancelled());
        assert!(outcome.report().downloaded.is_empty());
        assert!(!target.join("first.dat").exists());
        assert!(!target.join("second.dat").exists());

        let _ = fs::remove_dir_all(&source_dir);
        let _ = fs::remove_dir_all(&target);
    }
}
