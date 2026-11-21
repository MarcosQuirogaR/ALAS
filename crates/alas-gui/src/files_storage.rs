// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Disk-usage summary of the Files tab (it replaces the former "Manage
//! storage" dialog). The inventory walks directories recursively, so it runs
//! on a worker thread; the page shows "Measuring..." until the result arrives.
//! Clearing a category also runs on the worker, then re-measures.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::Arc;
use std::time::Instant;

use egui::{RichText, Ui};

use alas_exec::storage::{
    clear_storage, reset_tool_preferences, storage_inventory_progress, StorageEntry,
    StorageLocations,
};

use crate::state::{AppState, LogKind};
use crate::views::{tr, tr_fields};

enum Message {
    Inventory(Vec<StorageEntry>),
    Cleared {
        label: &'static str,
        removed: usize,
        failed: usize,
        entries: Vec<StorageEntry>,
    },
}

/// The summary and the worker feeding it.
#[derive(Debug, Default)]
pub struct StorageSummary {
    entries: Option<Vec<StorageEntry>>,
    pending: Option<Receiver<Message>>,
    /// Whether the details window is open.
    open: bool,
    /// Bytes measured so far by the running worker.
    progress: Arc<AtomicU64>,
    /// When the running worker started.
    started: Option<Instant>,
    /// Set when the worker ended without a result.
    failed: bool,
}

impl StorageSummary {
    /// Measure again on the next frame.
    pub fn invalidate(&mut self) {
        self.entries = None;
    }
}

/// The resolved locations the inventory is built from, owned for the worker.
struct Locations {
    output_dir: PathBuf,
    cfd_case_root: PathBuf,
    navdata_dir: PathBuf,
    texture_path: PathBuf,
}

impl Locations {
    fn of(state: &AppState) -> Self {
        let config = state.typed_config_arc().unwrap_or_default();
        Self {
            output_dir: state
                .pipeline_options
                .output_dir
                .clone()
                .unwrap_or_else(|| PathBuf::from("outputs")),
            cfd_case_root: PathBuf::from("outputs/airfoil-cfd"),
            navdata_dir: PathBuf::from(config.mission.navdata_dir.clone()),
            texture_path: PathBuf::from(config.mission.texture_path.clone()),
        }
    }

    fn borrowed(&self) -> StorageLocations<'_> {
        StorageLocations {
            output_dir: &self.output_dir,
            cfd_case_root: &self.cfd_case_root,
            navdata_dir: &self.navdata_dir,
            texture_path: &self.texture_path,
        }
    }
}

fn spawn(state: &mut AppState, ctx: &egui::Context, clear: Option<StorageEntry>) {
    let (sender, receiver) = channel();
    let locator = state.tool_locator.clone();
    let locations = Locations::of(state);
    let ctx = ctx.clone();
    let progress = Arc::new(AtomicU64::new(0));
    state.shell.library.storage.progress = progress.clone();
    state.shell.library.storage.started = Some(Instant::now());
    state.shell.library.storage.failed = false;
    std::thread::spawn(move || {
        let temp = std::env::temp_dir();
        let inventory = |locations: &Locations| {
            storage_inventory_progress(&locator, &locations.borrowed(), &temp, Some(&progress))
        };
        let message = match clear {
            Some(entry) => {
                let outcome = clear_storage(&entry);
                Message::Cleared {
                    label: entry.label,
                    removed: outcome.removed.len(),
                    failed: outcome.failed.len(),
                    entries: inventory(&locations),
                }
            }
            None => Message::Inventory(inventory(&locations)),
        };
        let _ = sender.send(message);
        ctx.request_repaint();
    });
    state.shell.library.storage.pending = Some(receiver);
}

fn poll(state: &mut AppState) {
    let Some(receiver) = state.shell.library.storage.pending.as_ref() else {
        return;
    };
    let message = match receiver.try_recv() {
        Ok(message) => message,
        Err(TryRecvError::Empty) => return,
        Err(TryRecvError::Disconnected) => {
            // The worker ended without a result: show an empty inventory
            // instead of measuring again forever.
            let storage = &mut state.shell.library.storage;
            storage.pending = None;
            storage.failed = true;
            storage.entries = Some(Vec::new());
            return;
        }
    };
    state.shell.library.storage.pending = None;
    match message {
        Message::Inventory(entries) => state.shell.library.storage.entries = Some(entries),
        Message::Cleared {
            label,
            removed,
            failed,
            entries,
        } => {
            state.shell.library.storage.entries = Some(entries);
            if failed == 0 {
                state.log(
                    tr_fields("Cleared {category}.", &[("category", tr(label))]),
                    LogKind::Info,
                );
            } else {
                state.log(
                    tr_fields(
                        "Cleared {removed} paths; {failed} could not be removed.",
                        &[
                            ("removed", removed.to_string()),
                            ("failed", failed.to_string()),
                        ],
                    ),
                    LogKind::Warn,
                );
            }
        }
    }
}

/// The footer line of the Files page: the total size of the generated data
/// and a button that opens the details window.
pub(crate) fn show_footer(state: &mut AppState, ui: &mut Ui) {
    poll(state);
    let busy = state.shell.library.storage.pending.is_some();
    if state.shell.library.storage.entries.is_none() && !busy {
        spawn(state, ui.ctx(), None);
    }
    if busy {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(200));
    }
    let storage = &state.shell.library.storage;
    let text = match (&storage.entries, busy) {
        (Some(_), false) if storage.failed => tr("Could not be measured"),
        (Some(entries), false) => format_bytes(entries.iter().map(|entry| entry.bytes).sum()),
        _ => tr_fields(
            "Measuring... {size} so far ({seconds} s)",
            &[
                (
                    "size",
                    format_bytes(storage.progress.load(Ordering::Relaxed)),
                ),
                (
                    "seconds",
                    storage
                        .started
                        .map_or(0, |started| started.elapsed().as_secs())
                        .to_string(),
                ),
            ],
        ),
    };
    // Laid out right to left by the caller: the button first, then the text,
    // which takes the room left and is cut with an ellipsis.
    if ui.small_button(tr("Details\u{2026}")).clicked() {
        state.shell.library.storage.open = !state.shell.library.storage.open;
    }
    // The total covers every place ALAS writes generated data, subfolders
    // included, so it is named for that and not read as the shown folder's.
    ui.add(
        egui::Label::new(
            RichText::new(format!("{}: {text}", tr("Generated data, all locations")))
                .weak()
                .small(),
        )
        .truncate(),
    )
    .on_hover_text(tr(
        "Everything ALAS generated, in every location it writes to (outputs, airfoil CFD cases, solver scratch files, navigation data and the globe texture), subfolders included. A folder's \"Size of files\" counts only the files directly in that folder.",
    ));
}

/// The storage details: one row per generated-data category with its size and
/// a Clear button, and the reset of the saved tool paths.
pub(crate) fn show_window(state: &mut AppState, ctx: &egui::Context) {
    if !state.shell.library.storage.open {
        return;
    }
    let response = crate::native_viewport::show_native_viewport(
        ctx,
        "files_storage_window",
        tr("Storage"),
        egui::ViewportBuilder::default()
            .with_title(tr("Storage"))
            .with_inner_size(egui::vec2(640.0, 460.0))
            .with_min_inner_size(egui::vec2(420.0, 280.0))
            .with_resizable(true),
        |_ctx, ui, _class| {
            let busy = state.shell.library.storage.pending.is_some();
            let entries = state.shell.library.storage.entries.clone();
            ui.label(RichText::new(tr(
                "Only ALAS-owned generated data is listed here. Tool installations and saved aircraft documents are not removed.",
            ))
            .weak()
            .small());
            let mut clear = None;
            for entry in entries.iter().flatten() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(tr(entry.label)).strong())
                        .on_hover_text(tr(entry.description));
                    ui.label(if entry.exists {
                        tr_fields(
                            "{size} * {files} files",
                            &[
                                ("size", format_bytes(entry.bytes)),
                                ("files", entry.files.to_string()),
                            ],
                        )
                    } else {
                        tr("Not created yet.")
                    });
                    if ui
                        .add_enabled_ui(entry.exists && !state.is_running && !busy, |ui| {
                            crate::theme::danger_button(ui, tr("Clear"))
                        })
                        .inner
                        .on_hover_text(tr(entry.description))
                        .clicked()
                    {
                        clear = Some(entry.clone());
                    }
                });
                ui.label(
                    RichText::new(entry.root.display().to_string())
                        .weak()
                        .small(),
                );
            }
            if state.is_running {
                ui.label(tr(
                    "Storage clearing is disabled while an analysis is running.",
                ));
            }
            if let Some(entry) = clear {
                spawn(state, ui.ctx(), Some(entry));
            }
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!busy, egui::Button::new(tr("Refresh inventory")))
                    .clicked()
                {
                    state.shell.library.storage.invalidate();
                }
                if crate::theme::danger_button(ui, tr("Reset saved tool paths"))
                    .on_hover_text(tr(
                        "Resetting saved paths leaves installed tools untouched; ALAS will discover them again on the next run or launch.",
                    ))
                    .clicked()
                {
                    reset_paths(state);
                }
            });
        },
    );
    if response.close_requested {
        state.shell.library.storage.open = false;
    }
}

fn reset_paths(state: &mut AppState) {
    match reset_tool_preferences(&state.tool_locator.clone()) {
        Ok(_) => {
            reset_session_tool_paths(state);
            state.log(
                tr("Saved tool paths reset; installed tools were not removed."),
                LogKind::Info,
            );
        }
        Err(error) => state.log(
            tr_fields(
                "Could not reset saved tool paths: {error}",
                &[("error", error)],
            ),
            LogKind::Error,
        ),
    }
}

/// Clear the path fields that are persisted as tool preferences in the live
/// session as well as on disk. Aircraft, mission and solver behaviour remain
/// otherwise unchanged; defaults are the discovery starting points.
fn reset_session_tool_paths(state: &mut AppState) {
    let Some(mut config) = state.typed_config() else {
        state.tool_preferences = alas_exec::ToolPreferences::default();
        return;
    };
    let defaults = alas_config::AlasConfig::default();
    config.mses.mses_dir = defaults.mses.mses_dir;
    config.structures.nastran_exe_path = defaults.structures.nastran_exe_path;
    config.structures.nastran_solver_path = defaults.structures.nastran_solver_path;
    config.structures.nastran95_dir_path = defaults.structures.nastran95_dir_path;
    config.structures.nastran95_runtime_path = defaults.structures.nastran95_runtime_path;
    config.structures.nastran95_rf_stage_path = defaults.structures.nastran95_rf_stage_path;
    config.structures.nastran95_open_core_words = defaults.structures.nastran95_open_core_words;
    config.structures.patran_exe_path = defaults.structures.patran_exe_path;
    config.mission.navdata_dir = defaults.mission.navdata_dir;
    config.mission.routes_dir = defaults.mission.routes_dir;
    state.config_values = crate::config_edit::full_config_values(&config);
    state.tool_preferences = alas_exec::ToolPreferences::default();
    state.on_config_modified();
}

fn format_bytes(n: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}
