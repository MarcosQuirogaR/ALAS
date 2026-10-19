// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Raw plot-data export: one plot, or every plot of the current run.
//!
//! The data comes from the same scenes the gallery shows, built under
//! [`alas_report::capture_plot_data`], so the files hold the plotted numbers.

use std::path::{Path, PathBuf};

use alas_report::{PlotData, PlotPanel, PlotSeries};

use super::{io_error, write_output, write_stored_zip, ExportError, ExportReceipt};
use crate::scene::{build_result_figure, build_screening_figure};
use crate::state::AppState;

/// Folder written next to the other exports by "Export all plot data".
pub const PLOT_DATA_DIR_NAME: &str = "alas-plot-data";
/// ZIP copy of that folder.
pub const PLOT_DATA_ARCHIVE_NAME: &str = "alas-plot-data.zip";

/// Default destination for plot data.
///
/// Installed builds write below the per-user data folder; a development
/// checkout writes to `exports/` like the figure archive and PDF report.
/// It is the folder the "Export data..." dialog opens in; the File menu uses
/// `AppState::data_dir()/exports`.
pub fn default_plot_export_dir() -> PathBuf {
    let frozen = std::env::var("ALAS_FROZEN")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if frozen {
        #[cfg(target_os = "windows")]
        if let Some(root) = std::env::var_os("LOCALAPPDATA") {
            return PathBuf::from(root).join("ALAS").join("exports");
        }
        #[cfg(not(target_os = "windows"))]
        if let Some(root) = std::env::var_os("HOME") {
            return PathBuf::from(root).join(".local/share/ALAS/exports");
        }
    }
    PathBuf::from("exports")
}

/// Identifier written into the files of the current run.
pub fn run_label(state: &AppState) -> String {
    format!("run-{:016x}", state.run_identity)
}

/// Plot data of every figure the current run can show.
pub fn collect_plot_data(state: &AppState) -> Result<Vec<PlotData>, ExportError> {
    let result = state
        .pipeline_result
        .as_ref()
        .ok_or(ExportError::NoPipelineRun)?;
    let theme = state.theme.figure_theme_name().to_owned();
    let run = run_label(state);
    let mut plots = Vec::new();
    for descriptor in alas_report::RESULT_FIGURES {
        if let Some(Some(scene)) = build_result_figure(state, descriptor.id, &result.config, &theme)
        {
            if let Some(mut plot) = PlotData::from_scene(&scene, descriptor.id, Some(&run)) {
                plot.title = descriptor.title.to_owned();
                plots.push(plot);
            }
        }
    }
    for descriptor in alas_report::SCREENING_FIGURES {
        if let Some(scene) = build_screening_figure(state, descriptor.id, &theme) {
            if let Some(mut plot) = PlotData::from_scene(&scene, descriptor.id, Some(&run)) {
                plot.title = descriptor.title.to_owned();
                plots.push(plot);
            }
        }
    }
    plots.extend(crate::views::plot_data_sources::standalone_plots(
        state, &run,
    ));
    if plots.is_empty() {
        return Err(ExportError::NoFigures);
    }
    Ok(plots)
}

/// Write every plot of the run to `<output_dir>/alas-plot-data/` and as a ZIP.
pub fn export_all_plot_data(
    state: &AppState,
    output_dir: &Path,
) -> Result<ExportReceipt, ExportError> {
    let plots = collect_plot_data(state)?;
    write_plot_bundle(&plots, output_dir)
}

/// Write a set of plots as a folder with an index, plus a ZIP of the same files.
pub fn write_plot_bundle(
    plots: &[PlotData],
    output_dir: &Path,
) -> Result<ExportReceipt, ExportError> {
    let files = alas_report::plot_data::bundle_files(plots);
    let folder = output_dir.join(PLOT_DATA_DIR_NAME);
    for (name, bytes) in &files {
        write_output(&folder.join(name), bytes, "write plot data")?;
    }
    write_stored_zip(&output_dir.join(PLOT_DATA_ARCHIVE_NAME), &files)?;
    Ok(ExportReceipt {
        path: folder,
        figure_count: plots.len(),
    })
}

/// Write one plot as `<stem>.csv` and `<stem>.json` in `output_dir`.
pub fn export_plot_data(plot: &PlotData, output_dir: &Path) -> Result<ExportReceipt, ExportError> {
    let stem = alas_report::plot_data::safe_file_stem(alas_report::export_file_stem(&plot.plot_id));
    std::fs::create_dir_all(output_dir)
        .map_err(|source| io_error("create export directory", output_dir, source))?;
    write_output(
        &output_dir.join(format!("{stem}.csv")),
        plot.to_csv().as_bytes(),
        "write plot data",
    )?;
    let json = output_dir.join(format!("{stem}.json"));
    write_output(&json, plot.to_json().as_bytes(), "write plot data")?;
    Ok(ExportReceipt {
        path: output_dir.join(format!("{stem}.csv")),
        figure_count: 1,
    })
}

/// One-panel line plot from named `[x, y]` series, for charts the GUI draws itself.
pub fn line_plot(
    plot_id: &str,
    title: &str,
    x_label: &str,
    y_label: &str,
    series: &[(String, &[[f64; 2]])],
) -> PlotData {
    let mut plot = PlotData::new(plot_id, title, None);
    let mut panel = PlotPanel::new(x_label, y_label);
    for (name, points) in series {
        panel.series.push(PlotSeries::line(name.clone(), points));
    }
    panel.fit_ranges();
    plot.panels.push(panel);
    plot
}

#[cfg(test)]
#[path = "plot_data_tests.rs"]
mod tests;
