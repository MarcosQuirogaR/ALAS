// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Worker threads of the Files explorer. Every job that reads a folder, builds
//! a preview or changes many files runs here; the page only sends a request
//! and polls the one channel the answers come back on, so a frame never waits
//! for the disk.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

use super::files_model::{list_dir, search_tree, Entry};
use super::files_preview::{make_preview, make_thumbnail, Preview};

/// Results of subfolder searches stop at this many items.
pub(super) const SEARCH_LIMIT: usize = 2000;
/// Thumbnails built at the same time.
const MAX_THUMBS_IN_FLIGHT: usize = 3;

/// The outcome of a file operation.
pub(super) struct OpDone {
    pub message: String,
    pub ok: bool,
    /// An item to select once the listing shows it.
    pub select: Option<PathBuf>,
}

/// An answer from a worker.
pub(super) enum Msg {
    Listing(PathBuf, Result<Vec<Entry>, String>),
    Search(u64, Vec<Entry>, bool),
    Preview(PathBuf, Box<Preview>),
    Thumb(PathBuf, Option<Arc<alas_report::scene::Scene>>),
    Done(OpDone),
}

/// The request side of the worker channel and what is in flight.
#[derive(Debug)]
pub(super) struct Jobs {
    pub tx: Sender<Msg>,
    pub rx: Receiver<Msg>,
    /// File operations running.
    pub busy_ops: usize,
    pub thumbs_in_flight: usize,
}

impl Default for Jobs {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            tx,
            rx,
            busy_ops: 0,
            thumbs_in_flight: 0,
        }
    }
}

impl Jobs {
    fn run(&self, ctx: &egui::Context, job: impl FnOnce() -> Msg + Send + 'static) {
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(job());
            ctx.request_repaint();
        });
    }

    pub(super) fn list(&self, ctx: &egui::Context, dir: PathBuf) {
        self.run(ctx, move || {
            let listing = list_dir(&dir, true);
            Msg::Listing(dir, listing)
        });
    }

    pub(super) fn search(&self, ctx: &egui::Context, id: u64, dir: PathBuf, query: String) {
        self.run(ctx, move || {
            let (found, truncated) = search_tree(&dir, &query, SEARCH_LIMIT);
            Msg::Search(id, found, truncated)
        });
    }

    pub(super) fn preview(&self, ctx: &egui::Context, path: PathBuf, theme: String) {
        self.run(ctx, move || {
            let preview = make_preview(&path, &theme);
            Msg::Preview(path, Box::new(preview))
        });
    }

    /// Start a thumbnail unless enough are already being built.
    pub(super) fn thumb(&mut self, ctx: &egui::Context, path: PathBuf, theme: String) -> bool {
        if self.thumbs_in_flight >= MAX_THUMBS_IN_FLIGHT {
            return false;
        }
        self.thumbs_in_flight += 1;
        self.run(ctx, move || {
            let scene = make_thumbnail(&path, &theme);
            Msg::Thumb(path, scene)
        });
        true
    }

    /// Send items to the Recycle Bin (or delete them where there is none).
    pub(super) fn delete(&mut self, ctx: &egui::Context, paths: Vec<PathBuf>) {
        self.busy_ops += 1;
        self.run(ctx, move || {
            let outcome = delete_paths(&paths);
            Msg::Done(OpDone {
                ok: outcome.is_ok(),
                message: outcome.err().unwrap_or_default(),
                select: None,
            })
        });
    }

    pub(super) fn duplicate(&mut self, ctx: &egui::Context, data: PathBuf, path: PathBuf) {
        self.busy_ops += 1;
        self.run(ctx, move || {
            let outcome = duplicate_path(&data, &path);
            Msg::Done(OpDone {
                ok: outcome.is_ok(),
                message: outcome.as_ref().err().cloned().unwrap_or_default(),
                select: outcome.ok(),
            })
        });
    }
}

/// Whether deleting goes through a Recycle Bin on this system.
pub(super) const fn recycle_bin_available() -> bool {
    cfg!(target_os = "windows")
}

#[cfg(target_os = "windows")]
fn delete_paths(paths: &[PathBuf]) -> Result<(), String> {
    use alas_exec::process::NoConsoleWindow;
    let joined = paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("|");
    let script = "Add-Type -AssemblyName Microsoft.VisualBasic; \
        foreach ($p in $env:ALAS_DELETE_PATHS -split '\\|') { \
        if (Test-Path -LiteralPath $p -PathType Container) { \
        [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($p, 'OnlyErrorDialogs', 'SendToRecycleBin') } \
        else { [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile($p, 'OnlyErrorDialogs', 'SendToRecycleBin') } }";
    let mut command = std::process::Command::new("powershell.exe");
    command.no_window();
    let output = command
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("ALAS_DELETE_PATHS", joined)
        .output()
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_owned())
    }
}

#[cfg(not(target_os = "windows"))]
fn delete_paths(paths: &[PathBuf]) -> Result<(), String> {
    for path in paths {
        let result = if path.is_dir() {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        };
        result.map_err(|error| format!("{}: {error}", path.display()))?;
    }
    Ok(())
}

/// Copy a file next to itself as "name copy.ext"; a library file goes through
/// the library so its header name changes too.
pub(super) fn duplicate_path(data: &Path, path: &Path) -> Result<PathBuf, String> {
    let small_json = path.extension().is_some_and(|ext| ext == "json")
        && std::fs::metadata(path).is_ok_and(|meta| meta.len() < 8 * 1024 * 1024);
    if small_json {
        if let Some(kind) = crate::file_library::file_kind(path) {
            return crate::file_library::duplicate(data, kind, path);
        }
    }
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|ext| format!(".{}", ext.to_string_lossy()))
        .unwrap_or_default();
    let mut target = path.with_file_name(format!("{stem} copy{ext}"));
    let mut index = 2;
    while target.exists() {
        target = path.with_file_name(format!("{stem} copy {index}{ext}"));
        index += 1;
    }
    std::fs::copy(path, &target)
        .map(|_| target)
        .map_err(|error| error.to_string())
}
