// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Paths and fresh-file checks for owned AVL execution artifacts.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use super::{AvlOutputChannels, AvlOutputPaths};

pub(super) fn output_filename(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.as_os_str().to_string_lossy().into_owned())
}

pub(super) fn create_fresh_file(path: &Path) -> std::io::Result<File> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .truncate(false)
        .open(path)
}

pub(super) fn remove_stale_artifact(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() || metadata.file_type().is_symlink() => {
            fs::remove_file(path)
        }
        Ok(_) => Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "expected an output file, found another filesystem object",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

pub(super) fn is_fresh_output(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .is_ok_and(|metadata| metadata.file_type().is_file() && metadata.len() > 100)
}

#[derive(Debug, Clone)]
pub(super) struct AvlRunPaths {
    pub(super) session_path: PathBuf,
    pub(super) force_paths: Vec<PathBuf>,
    pub(super) stdout_path: PathBuf,
    pub(super) stderr_path: PathBuf,
    pub(super) output_paths: AvlOutputPaths,
}

impl AvlRunPaths {
    pub(super) fn all_artifacts(&self) -> impl Iterator<Item = &Path> {
        self.force_paths
            .iter()
            .map(PathBuf::as_path)
            .chain(self.output_paths.strip_forces.iter().map(PathBuf::as_path))
            .chain(self.output_paths.derivatives.iter().map(PathBuf::as_path))
            .chain(self.output_paths.trim.iter().map(PathBuf::as_path))
            .chain([
                self.session_path.as_path(),
                self.stdout_path.as_path(),
                self.stderr_path.as_path(),
            ])
    }

    pub(super) fn required_outputs(&self) -> impl Iterator<Item = &Path> {
        self.force_paths
            .iter()
            .map(PathBuf::as_path)
            .chain(self.output_paths.strip_forces.iter().map(PathBuf::as_path))
            .chain(self.output_paths.derivatives.iter().map(PathBuf::as_path))
            .chain(self.output_paths.trim.iter().map(PathBuf::as_path))
    }
}

pub(super) fn render_output_paths(
    geometry_path: &Path,
    alpha_count: usize,
    namespace: Option<&Path>,
    channels: &AvlOutputChannels,
) -> AvlRunPaths {
    let source_base = geometry_path.with_extension("");
    let base = match namespace {
        Some(namespace) => namespace.join(
            source_base
                .file_name()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("avl_case")),
        ),
        None => source_base,
    };
    let force_paths = (0..alpha_count)
        .map(|index| base.with_extension(format!("avl.{index:03}.ft")))
        .collect::<Vec<_>>();
    let output_paths = AvlOutputPaths {
        strip_forces: if channels.strip_forces {
            (0..alpha_count)
                .map(|index| base.with_extension(format!("avl.{index:03}.fs")))
                .collect()
        } else {
            Vec::new()
        },
        derivatives: channels
            .stability_derivatives
            .then(|| base.with_extension("avl.derivatives.txt")),
        trim: channels
            .trim_commands
            .as_ref()
            .map(|_| base.with_extension("avl.trim.ft")),
    };
    AvlRunPaths {
        session_path: base.with_extension("avl.session.txt"),
        force_paths,
        stdout_path: base.with_extension("avl.stdout.txt"),
        stderr_path: base.with_extension("avl.stderr.txt"),
        output_paths,
    }
}
