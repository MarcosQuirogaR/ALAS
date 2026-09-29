// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The storage dialog and the resets it offers.

use egui::{vec2, Color32, Context, Id, RichText, ScrollArea, ViewportBuilder};

use alas_exec::storage::{
    clear_storage, reset_tool_preferences, storage_inventory, StorageLocations,
};

use super::{tr, tr_fields};
use crate::native_viewport::show_native_viewport;
use crate::state::AppState;

/// Render the storage-management dialog, if it is open.
///
/// The inventory is built from the same resolved data roots as the pipeline,
/// so the dialog never clears a path merely because it happens to have a
/// familiar name. CFD cases remain a separate category when they live below
/// the run output directory, and the saved tool-path reset removes only the
/// preferences file and its in-memory path values.
pub fn show_storage_dialog(state: &mut AppState, ctx: &Context) {
    if !state.show_storage {
        return;
    }
    let config = state.typed_config().unwrap_or_default();
    let output_dir = state
        .pipeline_options
        .output_dir
        .clone()
        .unwrap_or_else(|| std::path::PathBuf::from("outputs"));
    let cfd_case_root = std::path::PathBuf::from("outputs/airfoil-cfd");
    let navdata_dir = std::path::PathBuf::from(config.mission.navdata_dir);
    let texture_path = std::path::PathBuf::from(config.mission.texture_path);
    let locations = StorageLocations {
        output_dir: &output_dir,
        cfd_case_root: &cfd_case_root,
        navdata_dir: &navdata_dir,
        texture_path: &texture_path,
    };
    let locator = state.tool_locator.clone();
    let cache_id = Id::new((
        "alas_storage_inventory",
        &output_dir,
        &cfd_case_root,
        &navdata_dir,
        &texture_path,
    ));
    let mut entries = ctx
        .data(|data| data.get_temp::<Vec<alas_exec::storage::StorageEntry>>(cache_id))
        .unwrap_or_else(|| {
            let entries = storage_inventory(&locator, &locations);
            ctx.data_mut(|data| data.insert_temp(cache_id, entries.clone()));
            entries
        });
    let response = show_native_viewport(
        ctx,
        "manage_storage",
        tr("Manage storage"),
        ViewportBuilder::default()
            .with_title(tr("Manage storage"))
            .with_inner_size(vec2(760.0, 620.0))
            .with_min_inner_size(vec2(560.0, 420.0))
            .with_resizable(true),
        |_child_ctx, ui, _class| {
            // Keep the complete body in one scroll area.  The previous fixed
            // 420-point child area left the saved-path controls below the
            // viewport on short windows, where the outer viewport itself did
            // not expose a second scroll bar.
            ScrollArea::vertical()
                .id_salt("manage_storage_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(tr(
                        "Only ALAS-owned generated data is listed here. Tool installations and saved aircraft documents are not removed.",
                    ));
                    if state.is_running {
                        ui.colored_label(
                            Color32::YELLOW,
                            tr("Storage clearing is disabled while an analysis is running."),
                        );
                    }
                    for index in 0..entries.len() {
                        let entry = entries[index].clone();
                        ui.group(|ui| {
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
                            });
                            ui.small(entry.root.display().to_string());
                            ui.add_enabled_ui(entry.exists && !state.is_running, |ui| {
                                if ui.button(tr("Clear"))
                                    .on_hover_text(tr(entry.description))
                                    .clicked() {
                                    let outcome = clear_storage(&entry);
                                    if outcome.failed.is_empty() {
                                        state.log(
                                            tr_fields(
                                                "Cleared {category}.",
                                                &[("category", tr(entry.label))],
                                            ),
                                            crate::state::LogKind::Info,
                                        );
                                    } else {
                                        state.log(
                                            tr_fields(
                                                "Cleared {removed} paths; {failed} could not be removed.",
                                                &[
                                                    ("removed", outcome.removed.len().to_string()),
                                                    ("failed", outcome.failed.len().to_string()),
                                                ],
                                            ),
                                            crate::state::LogKind::Warn,
                                        );
                                    }
                                    entries = storage_inventory(&locator, &locations);
                                    ctx.data_mut(|data| {
                                        data.insert_temp(cache_id, entries.clone())
                                    });
                                }
                            });
                        });
                        ui.add_space(4.0);
                    }
                    if ui.button(tr("Refresh inventory")).clicked() {
                        entries = storage_inventory(&locator, &locations);
                        ctx.data_mut(|data| data.insert_temp(cache_id, entries.clone()));
                    }
                    ui.separator();
                    ui.label(RichText::new(tr("Saved tool paths")).strong())
                        .on_hover_text(tr(
                            "Resetting saved paths leaves installed tools untouched; ALAS will discover them again on the next run or launch.",
                        ));
                    if ui.button(tr("Reset saved tool paths"))
                        .on_hover_text(tr(
                            "Resetting saved paths leaves installed tools untouched; ALAS will discover them again on the next run or launch.",
                        ))
                        .clicked() {
                        match reset_tool_preferences(&locator) {
                            Ok(_) => {
                                reset_session_tool_paths(state);
                                state.log(
                                    tr("Saved tool paths reset; installed tools were not removed."),
                                    crate::state::LogKind::Info,
                                );
                            }
                            Err(error) => state.log(
                                tr_fields(
                                    "Could not reset saved tool paths: {error}",
                                    &[("error", error)],
                                ),
                                crate::state::LogKind::Error,
                            ),
                        }
                    }
                });
        },
    );
    if response.close_requested {
        state.show_storage = false;
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
