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

pub(super) fn show_progressive_results(state: &mut AppState, ui: &mut Ui) {
    ui.heading(tr("Results"));
    ui.add_space(6.0);
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        if state.is_running {
            ui.label(
                RichText::new(tr(
                    "Figures will appear here as each analysis stage finalizes.",
                ))
                .weak(),
            );
        } else {
            // One line; the Run button of the bottom bar is the only one.
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(tr("No results yet")).strong());
                ui.label(
                    RichText::new(tr("Run the study to see its figures and tables here.")).weak(),
                );
            });
        }
    });
}
