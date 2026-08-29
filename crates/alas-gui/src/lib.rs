// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native desktop graphical user interface application for ALAS.
//!
//! Provides the primary interactive user environment for conceptual aircraft
//! design, multi-stage engineering analysis, optimization, and visualization.

pub mod app;
pub mod config_edit;
pub mod export;
pub mod feedback;
pub mod layout;
#[cfg(debug_assertions)]
mod layout_debug;
pub mod nav;
mod nav_overlay;
pub mod path_picker;
pub mod run;
pub mod scene;
pub mod screening;
pub mod state;
pub mod theme;
pub mod uav;
mod view_controls;
pub mod viewport;
pub mod views;

pub use app::AlasApp;
pub use state::AppState;
pub use theme::{apply_theme, AppTheme};

/// Launch the native ALAS desktop graphical user interface.
pub fn run() -> Result<(), eframe::Error> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("ALAS - Aircraft Layout and Analysis Suite")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([880.0, 560.0]),
        ..Default::default()
    };

    eframe::run_native(
        "ALAS",
        native_options,
        Box::new(|cc| {
            theme::apply_theme(AppTheme::Dark, &cc.egui_ctx);
            Ok(Box::new(AlasApp::default()))
        }),
    )
}
