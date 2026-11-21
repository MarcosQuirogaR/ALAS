// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native desktop graphical user interface application for ALAS.
//!
//! Provides the primary interactive user environment for conceptual aircraft
//! design, multi-stage engineering analysis, optimization, and visualization.

// Unit tests assert that their constructed fixture inputs are valid.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod addons;
pub mod airport_editor;
mod airport_store;
pub mod app;
mod branding;
pub mod cfd;
pub mod checkbox;
mod combo_icon;
pub mod config_edit;
mod config_edit_cruise;
pub mod detail;
pub mod export;
pub mod feedback;
pub(crate) mod figure_jobs;
pub mod file_dialog;
pub mod file_library;
mod file_library_autosave;
mod file_library_ops;
mod file_library_recent;
mod files_storage;
mod flops_cabin_mirror;
mod fs_atomic;
pub mod layout;
#[cfg(debug_assertions)]
mod layout_debug;
pub mod library_window;
mod live_dispatch;
mod lock_icon;
pub(crate) mod native_viewport;
pub mod nav;
pub(crate) mod nav_input;
mod nav_overlay;
pub(crate) mod notation;
pub mod openvsp_runtime_setup;
pub mod path_picker;
pub mod perf_tour;
pub(crate) mod preview_jobs;
mod rail_row;
mod result_refresh;
pub mod run;
pub mod sandbox;
pub mod scene;
pub mod screening;
pub mod search;
mod search_concepts;
mod selector_search;
mod shell_confirm;
pub mod shell_ui;
mod shell_undo;
pub mod state;
mod status_text;
pub mod theme;
pub mod tool_installer;
pub mod uav;
pub mod user_files;
mod user_files_ops;
mod user_files_ui;
mod user_number;
mod view_controls;
pub mod viewport;
pub mod views;
mod walkthrough_state;
pub mod worker_stack;

pub use app::AlasApp;
pub use branding::native_options;
pub use state::AppState;
pub use theme::{apply_theme, AppTheme};

/// Launch the native ALAS desktop graphical user interface.
pub fn run() -> Result<(), eframe::Error> {
    let native_options = native_options(
        "ALAS - Aircraft Layout and Analysis Suite",
        [1280.0, 820.0],
        [880.0, 560.0],
    );

    eframe::run_native(
        "ALAS",
        native_options,
        Box::new(|cc| {
            theme::apply_theme(AppTheme::Dark, &cc.egui_ctx);
            Ok(Box::new(AlasApp::default()))
        }),
    )
}

#[cfg(test)]
mod detached_tests;
#[cfg(test)]
mod field_tooltip_tests;
#[cfg(test)]
mod gui_export_probe_tests;
#[cfg(test)]
mod preset_baseline_tests;
#[cfg(test)]
mod replacing_entry_tests;
#[cfg(test)]
mod spawn_guard_tests;
#[cfg(test)]
mod test_raster;
#[cfg(test)]
mod test_svg;
