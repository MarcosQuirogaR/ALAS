// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The "Export data..." action every plot offers in its context menu.
//!
//! A plot either hands over its [`Scene`] (built under plot-data capture, so
//! the plotted numbers travel with it) or builds a [`PlotData`] from the arrays
//! it draws. The files are CSV (tidy long format) and JSON (full metadata).

use alas_report::{PlotData, Scene};
use egui::Response;

use crate::views::{tr, tr_fields};

/// Offer "Export data..." on the right-click menu of `response`.
///
/// The response must sense clicks. `make` runs only when the user chooses the
/// action, so building the data costs nothing per frame.
pub(crate) fn attach(response: &Response, make: impl FnOnce() -> Option<PlotData>) {
    let message_id = response.id.with("plot-data-export-message");
    response.context_menu(|ui| {
        if ui
            .button(tr("Export data..."))
            .on_hover_text(tr(
                "Write the plotted values of this graph as CSV and JSON files",
            ))
            .clicked()
        {
            let message = match make() {
                Some(plot) => write_one(&plot),
                None => tr("This figure has no plotted data to export."),
            };
            ui.data_mut(|data| data.insert_temp(message_id, message));
        }
        if let Some(message) = ui.data(|data| data.get_temp::<String>(message_id)) {
            ui.label(egui::RichText::new(message).small());
        }
    });
}

/// Offer "Export data..." for a figure drawn from `scene`.
pub(crate) fn attach_scene(
    response: &Response,
    scene: &Scene,
    plot_id: &str,
    run_label: Option<String>,
) {
    attach(response, || {
        let mut plot = PlotData::from_scene(scene, plot_id, run_label.as_deref())?;
        if let Some(descriptor) = alas_report::find_figure(plot_id) {
            plot.title = descriptor.title.to_owned();
        }
        Some(plot)
    });
}

fn write_one(plot: &PlotData) -> String {
    let spec = crate::file_dialog::spec(
        crate::file_dialog::DialogKind::PickFolder,
        &tr("Export data..."),
        "",
        &crate::export::default_plot_export_dir(),
        None,
        Vec::new(),
    );
    let Some(directory) = crate::file_dialog::run_blocking(&spec) else {
        return tr("Export cancelled.");
    };
    match crate::export::export_plot_data(plot, &directory) {
        Ok(receipt) => tr_fields(
            "Plot data written to {path}",
            &[("path", receipt.path.display().to_string())],
        ),
        Err(error) => tr_fields(
            "Plot data export failed: {error}",
            &[("error", error.to_string())],
        ),
    }
}

/// Offer "Export data..." for a line chart the GUI draws itself.
///
/// `series` yields `(name, [x, y] points)` in plotted units and runs only when
/// the action is chosen. Units are read from the axis labels (`[unit]` or
/// `(unit)` at the end).
pub(crate) fn attach_xy(
    response: &Response,
    plot_id: &str,
    title: &str,
    x_label: &str,
    y_label: &str,
    series: impl FnOnce() -> Vec<(String, Vec<[f64; 2]>)>,
) {
    attach(response, || {
        let series = series();
        let borrowed: Vec<(String, &[[f64; 2]])> = series
            .iter()
            .map(|(name, points)| (name.clone(), points.as_slice()))
            .collect();
        let plot = crate::export::line_plot(plot_id, title, x_label, y_label, &borrowed);
        (plot.row_count() > 0).then_some(plot)
    });
}

/// Convert `(x, y)` tuples to the `[x, y]` pairs [`attach_xy`] takes.
pub(crate) fn pairs(points: &[(f64, f64)]) -> Vec<[f64; 2]> {
    points.iter().map(|&(x, y)| [x, y]).collect()
}

/// The registry figure id inside a result view key (`...;figure=<id>;...`).
pub(crate) fn figure_id_from_view_key(view_key: &str) -> Option<&str> {
    view_key
        .split(';')
        .find_map(|part| part.strip_prefix("figure="))
}
