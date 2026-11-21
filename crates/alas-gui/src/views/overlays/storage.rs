// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The former "Manage storage" dialog. Its content now lives in the Files tab
//! (`crate::files_storage`, measured on a worker thread); opening the dialog
//! flag takes the user there instead.

use crate::state::AppState;

/// Redirect the retired dialog flag to the Files tab.
pub fn show_storage_dialog(state: &mut AppState, _ctx: &egui::Context) {
    if state.show_storage {
        state.show_storage = false;
        state.active_page = "files".to_owned();
    }
}
