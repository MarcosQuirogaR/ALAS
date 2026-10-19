// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Plot data of GUI charts that are not report figures, for "Export all plot data".
//!
//! Charts that live in their own window and hold their data there (Wing
//! Analysis, Airfoil CFD) export through their own right-click menu; the
//! charts below are derived from the shared application state.

use alas_report::PlotData;

use crate::state::AppState;

/// Plots drawn directly by the GUI from state that is always available.
pub(crate) fn standalone_plots(state: &AppState, run_label: &str) -> Vec<PlotData> {
    let mut plots = Vec::new();
    if let Some(config) = state.typed_config() {
        let points = super::mission_profile_preview::profile_points(&config);
        let mut plot = crate::export::line_plot(
            "mission_profile_preview",
            "Live mission profile (schematic)",
            "Mission progress (schematic fraction) [-]",
            "Altitude [m]",
            &[("profile".to_owned(), points.as_slice())],
        );
        plot.run_id = Some(run_label.to_owned());
        plot.notes.push(
            "Schematic: the x axis is the phase weight of the preview, not flown distance."
                .to_owned(),
        );
        plots.push(plot);
    }
    plots
}
