// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Open the production desktop shell directly on the Spanish UAV workflow.

use alas_gui::state::{AppState, Language};
use alas_gui::theme::{apply_theme, AppTheme};
use alas_gui::AlasApp;

fn main() -> Result<(), eframe::Error> {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));
    let mut state = AppState::default();
    state.language = Language::Es;
    state.active_page = "uav".to_owned();
    state.nav_pinned = true;
    state.preview_open = false;
    state.boot_frames_remaining = 0;

    eframe::run_native(
        "ALAS - UAV Workflow Audit",
        alas_gui::native_options(
            "ALAS - UAV Workflow Audit",
            [1440.0, 920.0],
            [1000.0, 680.0],
        ),
        Box::new(move |creation_context| {
            apply_theme(AppTheme::Dark, &creation_context.egui_ctx);
            Ok(Box::new(AlasApp::from_state(state)))
        }),
    )
}
