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
use std::process::Command;

use crate::process::{NewProcessGroup, NoConsoleWindow};

/// One HTTPS file to download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadSpec {
    /// File name relative to the destination directory.
    pub name: String,
    /// HTTPS source URL.
    pub url: String,
    /// Minimum size accepted as a completed transfer.
    pub min_bytes: u64,
}

impl DownloadSpec {
    /// Construct a download specification from the caller's asset metadata.
    pub fn new(name: impl Into<String>, url: impl Into<String>, min_bytes: u64) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            min_bytes,
        }
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

/// Download all incomplete files into `target_dir`.
///
/// Each transfer is written to a private part file and validated before the
/// destination is replaced.  A cancelled or failed curl invocation therefore
/// cannot leave a short file that later looks like usable navdata.  Existing
/// valid files are skipped, which makes repeated setup actions cheap and safe.
pub fn download_files(
    specs: &[DownloadSpec],
    target_dir: &Path,
    timeout_seconds: f64,
) -> Result<DownloadReport, String> {
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

    for (index, spec) in specs.iter().enumerate() {
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
                "=https",
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
            .no_window()
            .new_process_group();
        let output = match command.output() {
            Ok(output) => output,
            Err(error) => {
                let _ = fs::remove_file(&temporary);
                return Err(format!("cannot launch {executable}: {error}"));
            }
        };
        if !output.status.success() {
            let body = fs::read_to_string(&temporary).unwrap_or_default();
            let _ = fs::remove_file(&temporary);
            return Err(format!(
                "download of {} failed ({}): {}",
                spec.name,
                output.status,
                diagnostic_text(&body, &String::from_utf8_lossy(&output.stderr))
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

        // Windows does not replace an existing file with rename, so remove an
        // invalid old destination only after the new file has passed its size
        // floor.  A valid old destination was skipped above.
        if destination.exists() {
            fs::remove_file(&destination).map_err(|error| {
                let _ = fs::remove_file(&temporary);
                format!("cannot replace {}: {error}", destination.display())
            })?;
        }
        if let Err(error) = fs::rename(&temporary, &destination) {
            let _ = fs::remove_file(&temporary);
            return Err(format!("cannot install {}: {error}", destination.display()));
        }
        downloaded.push(spec.name.clone());
    }

    Ok(DownloadReport {
        target_dir: target_dir.to_path_buf(),
        downloaded,
        skipped,
    })
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
    if !spec.url.starts_with("https://") {
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
        result.push_str(" …");
    }
    result.replace(['\r', '\n'], " ")
}

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
        let error = download_files(&[], &target, f64::NAN).expect_err("NaN timeout");
        assert!(error.contains("invalid download timeout"));
        assert!(!target.exists());
    }
}
