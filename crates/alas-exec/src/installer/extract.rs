// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Safe extraction through the platform `tar`.
//!
//! Windows 10+ ships `tar.exe` (bsdtar), which reads ZIP as well as tar, so no
//! archive library is needed. The archive is listed and every name checked
//! before anything is written; the result is walked afterwards for symbolic
//! links that point outside the staging directory.

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use std::time::Duration;

use crate::captured::run_captured;

/// Backstop for listing or extracting one tool archive; the largest bundles
/// are a few GB, so this is generous and only catches a hung `tar`.
const TAR_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);

use super::manifest::ArchiveFormat;
use super::InstallError;

fn tar_program() -> PathBuf {
    if cfg!(windows) {
        // Not the bare name: Git for Windows puts GNU tar, which cannot read
        // ZIP, ahead of the system one on PATH.
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
        PathBuf::from(root).join("System32").join("tar.exe")
    } else {
        PathBuf::from("tar")
    }
}

fn run_tar(args: &[&std::ffi::OsStr]) -> Result<String, InstallError> {
    let program = tar_program();
    let mut command = Command::new(&program);
    command.args(args).stdin(Stdio::null());
    let output =
        run_captured(&mut command, "tool installer archive", TAR_TIMEOUT).map_err(|error| {
            InstallError::Archive(format!("cannot run {}: {error}", program.display()))
        })?;
    if !output.status.success() {
        return Err(InstallError::Archive(format!(
            "{} failed: {}",
            program.display(),
            output.stderr.trim()
        )));
    }
    Ok(output.stdout)
}

/// Whether an archive entry name stays below the extraction root.
pub(super) fn entry_name_is_safe(name: &str) -> bool {
    let normalised = name.replace('\\', "/");
    if normalised.starts_with('/') || normalised.contains(':') {
        return false;
    }
    !normalised.split('/').any(|part| part == "..")
}

/// List `archive`, reject unsafe names, extract into `staging`, and verify the
/// extracted tree contains no link leaving `staging`.
pub(super) fn extract_archive(
    archive: &Path,
    format: ArchiveFormat,
    staging: &Path,
) -> Result<(), InstallError> {
    let _ = format; // bsdtar and GNU tar both detect the compression themselves.
    let listing = run_tar(&["-tf".as_ref(), archive.as_os_str()])?;
    let mut entries = 0usize;
    for name in listing.lines().filter(|line| !line.trim().is_empty()) {
        entries += 1;
        if !entry_name_is_safe(name) {
            return Err(InstallError::Archive(format!("unsafe entry name {name:?}")));
        }
    }
    if entries == 0 {
        return Err(InstallError::Archive("archive is empty".into()));
    }
    fs::create_dir_all(staging).map_err(|error| {
        InstallError::Io(format!("cannot create {}: {error}", staging.display()))
    })?;
    run_tar(&[
        "-xf".as_ref(),
        archive.as_os_str(),
        "-C".as_ref(),
        staging.as_os_str(),
    ])?;
    check_tree(staging, staging)
}

fn check_tree(root: &Path, directory: &Path) -> Result<(), InstallError> {
    let entries = fs::read_dir(directory).map_err(|error| {
        InstallError::Io(format!("cannot read {}: {error}", directory.display()))
    })?;
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            let target = fs::read_link(&path)
                .map_err(|error| InstallError::Io(format!("cannot read link: {error}")))?;
            if !link_stays_inside(root, &path, &target) {
                return Err(InstallError::Archive(format!(
                    "link {} points outside the install directory",
                    path.display()
                )));
            }
            // The name check reads one link at a time; following a chain of links
            // (`a -> ../b`, `b -> a/../..`) can still leave the tree.
            let base = fs::canonicalize(root).map_err(|error| {
                InstallError::Io(format!("cannot resolve the install root: {error}"))
            })?;
            if !link_resolves_inside(&base, &path, &target) {
                return Err(InstallError::Archive(format!(
                    "link {} resolves outside the install directory or cannot be resolved",
                    path.display()
                )));
            }
        } else if meta.is_dir() {
            check_tree(root, &path)?;
        }
    }
    Ok(())
}

/// Follow `link` to where it really ends up: a chain that exists is resolved with
/// `canonicalize`; a dangling one is resolved from its real parent directory, so a
/// later build step cannot write through it to somewhere outside `base`.
pub(super) fn link_resolves_inside(base: &Path, link: &Path, target: &Path) -> bool {
    if let Ok(real) = fs::canonicalize(link) {
        return real.starts_with(base);
    }
    // Dangling: let the operating system resolve the longest part of the path that
    // exists (so a `..` after an in-tree link goes where the link really points),
    // and allow only plain names in what is left.
    let Some(parent) = link.parent() else {
        return false;
    };
    let joined = parent.join(target);
    let full: Vec<Component<'_>> = joined.components().collect();
    for cut in (1..=full.len()).rev() {
        let prefix: PathBuf = full[..cut].iter().collect();
        if let Ok(mut real) = fs::canonicalize(&prefix) {
            for part in &full[cut..] {
                match part {
                    Component::Normal(name) => real.push(name),
                    _ => return false,
                }
            }
            return real.starts_with(base);
        }
    }
    false
}

pub(super) fn link_stays_inside(root: &Path, link: &Path, target: &Path) -> bool {
    if target.is_absolute() {
        return false;
    }
    let mut resolved: Vec<_> = link
        .parent()
        .unwrap_or(root)
        .strip_prefix(root)
        .map(|relative| relative.components().collect())
        .unwrap_or_default();
    for part in target.components() {
        match part {
            Component::Normal(name) => resolved.push(Component::Normal(name)),
            Component::CurDir => {}
            Component::ParentDir => {
                if resolved.pop().is_none() {
                    return false;
                }
            }
            _ => return false,
        }
    }
    true
}

/// The directory that should become the install root: `staging` itself, or its
/// single child directory when `strip_single_root` is set.
pub(super) fn install_root(
    staging: &Path,
    strip_single_root: bool,
) -> Result<PathBuf, InstallError> {
    if !strip_single_root {
        return Ok(staging.to_path_buf());
    }
    let children: Vec<_> = fs::read_dir(staging)
        .map_err(|error| InstallError::Io(format!("cannot read {}: {error}", staging.display())))?
        .filter_map(Result::ok)
        .collect();
    match children.as_slice() {
        [only] if only.path().is_dir() => Ok(only.path()),
        _ => Err(InstallError::Archive(
            "expected exactly one top-level directory in the archive".into(),
        )),
    }
}
