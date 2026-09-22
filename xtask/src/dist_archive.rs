// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Final archive assembly for a package directory: `.zip` on Windows,
//! `.tar.gz` everywhere else.
//!
//! Split out of `dist.rs` as its own module (not a textual `include!`
//! fragment) so this platform-conditional logic has its own size budget, per
//! `docs/source-size-budgets.tsv`'s note on `dist.rs`: frozen pending
//! decomposition into modules with interfaces.

use std::path::Path;
use std::process::Command;

/// The archive file name for this target: `.zip` on Windows, matching
/// Explorer's built-in extraction, and `.tar.gz` everywhere else, the native
/// archive format that round-trips a Unix executable bit and any symlink a
/// future bundled tool might use, neither of which a zip guarantees.
pub(crate) fn archive_file_name(package_name: &str) -> String {
    if cfg!(windows) {
        format!("{package_name}.zip")
    } else {
        format!("{package_name}.tar.gz")
    }
}

pub(crate) fn create_archive(
    dist_root: &Path,
    folder_name: &str,
    archive_name: &str,
) -> Result<(), String> {
    if cfg!(windows) {
        create_zip_archive(dist_root, folder_name, archive_name)
    } else {
        create_tar_gz_archive(dist_root, folder_name, archive_name)
    }
}

fn create_zip_archive(dist_root: &Path, folder_name: &str, zip_name: &str) -> Result<(), String> {
    let script = format!(
        "Compress-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
        powershell_quote(folder_name),
        powershell_quote(zip_name)
    );
    let status = Command::new("powershell")
        .current_dir(dist_root)
        .args(["-NoProfile", "-Command", &script])
        .status()
        .map_err(|e| format!("failed to run powershell Compress-Archive: {e}"))?;

    if !status.success() {
        return Err("Compress-Archive failed".to_owned());
    }
    Ok(())
}

/// `tar` ships on every mainstream Linux distribution and on macOS (as
/// bsdtar), so it needs no extra runtime dependency beyond what a build host
/// already has. `-z` selects gzip explicitly instead of relying on `-a`
/// suffix sniffing, so the chosen compressor does not silently change if this
/// function is ever called with a differently named archive.
fn create_tar_gz_archive(
    dist_root: &Path,
    folder_name: &str,
    archive_name: &str,
) -> Result<(), String> {
    let status = Command::new("tar")
        .current_dir(dist_root)
        .args(["-czf", archive_name, "--", folder_name])
        .status()
        .map_err(|e| format!("failed to run tar: {e}"))?;
    if !status.success() {
        return Err("tar packaging failed".to_owned());
    }
    Ok(())
}

fn powershell_quote(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::archive_file_name;

    #[test]
    fn archive_file_name_selects_zip_on_windows_and_tar_gz_elsewhere() {
        let name = archive_file_name("alas-v1.2.0-windows-x86_64");
        if cfg!(windows) {
            assert!(name.ends_with(".zip"), "{name}");
        } else {
            assert!(name.ends_with(".tar.gz"), "{name}");
        }
    }
}
