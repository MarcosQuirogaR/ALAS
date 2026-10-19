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
pub mod app;
mod branding;
pub mod cfd;
pub mod config_edit;
mod config_edit_cruise;
pub mod export;
pub mod feedback;
pub mod file_dialog;
mod fs_atomic;
pub mod layout;
#[cfg(debug_assertions)]
mod layout_debug;
pub(crate) mod native_viewport;
pub mod nav;
mod nav_overlay;
pub mod openvsp_runtime_setup;
pub mod path_picker;
mod result_refresh;
pub mod run;
pub mod sandbox;
pub mod scene;
pub mod screening;
mod selector_search;
pub mod state;
pub mod theme;
pub mod uav;
pub mod user_files;
mod user_files_ops;
mod user_files_ui;
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
mod test_svg;
