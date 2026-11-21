// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Load-and-trim adapter for report and desktop figure dispatch.

use super::load_trim::{
    data::load_trim_data_from_report, figure_load_trim_sheet, figure_load_trim_sheet_sized,
};
use crate::scene::Scene;
use crate::theme::get_palette;
use alas_config::AlasConfig;
use alas_pipeline::full_analysis::AnalysisReport;

/// Render the load-and-trim sheet for the selected report and configuration.
pub fn figure_cg_envelope(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
) -> Scene {
    figure_cg_envelope_sized(report, config, theme, None)
}

/// [`figure_cg_envelope`] drawn for a card `width` points wide (see
/// [`figure_load_trim_sheet_sized`]); the height follows from the sheet.
pub fn figure_cg_envelope_sized(
    report: &AnalysisReport,
    config: &AlasConfig,
    theme: Option<&str>,
    width: Option<f64>,
) -> Scene {
    match load_trim_data_from_report(report, config) {
        Some(data) => match width {
            Some(width) => figure_load_trim_sheet_sized(&data, get_palette(theme), width),
            None => figure_load_trim_sheet(&data, get_palette(theme)),
        },
        None => crate::families::aerodynamics::figure_status_message(
            "Load & trim sheet",
            "CG assessment unavailable",
            false,
            theme,
        ),
    }
}
