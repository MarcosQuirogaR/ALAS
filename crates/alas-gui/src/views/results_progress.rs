// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Empty Results state before the first report snapshot is available.
//!
//! Once the pipeline publishes an immutable report boundary, `results_view`
//! renders its normal tabs and figure gallery. This state intentionally has no
//! progress bars, event log, or diagnostic numbers: those belong to the Run
//! Log, while Results is reserved for figures that are part of the run's
//! eventual report.

use egui::{RichText, Ui};

use crate::state::AppState;
use crate::views::tr;

pub(super) fn show_progressive_results(state: &AppState, ui: &mut Ui) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.heading(tr("Results"));
            ui.add_space(8.0);
            let message = if state.is_running {
                tr("Figures will appear here as each analysis stage finalizes.")
            } else {
                tr("Run the pipeline from Inputs to populate the Results gallery.")
            };
            ui.label(RichText::new(message).weak());
        });
    });
}
