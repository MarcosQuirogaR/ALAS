// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Standalone Wing Analysis window.
//!
//! The window analyses the wing of the current configuration, optionally with
//! the empennage as lofted, without a mission, a payload or a whole-aircraft
//! run. It is opened from the Analysis menu exactly like Airfoil Screening and
//! Airfoil CFD, and keeps its own detached native viewport.
//!
//! Every number shown here belongs to the modelled surfaces only; the header
//! states that configuration and the Setup tab lists what it omits.

#[path = "../wing_analysis.rs"]
pub mod wing_analysis;

#[path = "wing_analysis_view_draw.rs"]
mod draw;
#[path = "wing_analysis_view_results.rs"]
mod results;
#[path = "wing_analysis_view_setup.rs"]
mod setup;

#[cfg(test)]
#[path = "wing_analysis_view_tests.rs"]
mod tests;

use egui::{vec2, RichText, Ui, ViewportBuilder, ViewportCommand};

use crate::native_viewport::{show_native_viewport, viewport_id};
use crate::state::{AppState, LogKind};
use crate::views::{tr, tr_fields};

use wing_analysis::{with_window, RunPhase, WingAnalysisState, WingAnalysisTab};

/// The key both the viewport and its focus command are built from.
const WING_ANALYSIS_VIEWPORT_KEY: &str = "wing_analysis";

/// Open the Wing Analysis window, raising it when it is already open.
pub fn open_wing_analysis(ctx: &egui::Context) {
    with_window(|window| {
        if window.window_open {
            ctx.send_viewport_cmd_to(
                viewport_id(WING_ANALYSIS_VIEWPORT_KEY),
                ViewportCommand::Focus,
            );
        }
        window.window_open = true;
        window.tab = WingAnalysisTab::Setup;
    });
}

/// Whether the detached Wing Analysis window is open.
pub fn is_open() -> bool {
    with_window(|window| window.window_open)
}

/// Render the detached Wing Analysis window when it is open.
///
/// Called once per frame from the shell so the window keeps its own native
/// viewport, its worker messages and its live preview while the main window
/// does anything else.
pub fn show_wing_analysis_window(state: &mut AppState, ctx: &egui::Context) {
    let open = with_window(|window| {
        if let Some(status) = window.poll() {
            window.status = status;
        }
        window.window_open
    });
    if !open {
        return;
    }
    let mut log_lines = Vec::new();
    let response = show_native_viewport(
        ctx,
        WING_ANALYSIS_VIEWPORT_KEY,
        tr("Wing Analysis"),
        ViewportBuilder::default()
            .with_title(tr("Wing Analysis"))
            .with_inner_size(vec2(1040.0, 720.0))
            .with_min_inner_size(vec2(620.0, 420.0))
            .with_resizable(true),
        |_child_ctx, ui, _class| {
            show_window_contents(state, ui, &mut log_lines);
        },
    );
    for (text, kind) in log_lines {
        state.log(text, kind);
    }
    if response.close_requested {
        with_window(|window| window.window_open = false);
    }
    if with_window(|window| window.running()) {
        // The worker reports once, at the end of a solve that takes seconds on
        // a fine lattice. Ten updates a second keep the status honest without
        // spinning the interface at display rate.
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// Header, tabs and the active tab, with the window state borrowed once.
fn show_window_contents(state: &mut AppState, ui: &mut Ui, log: &mut Vec<(String, LogKind)>) {
    let theme = state.theme.figure_theme_name().to_owned();
    let source = state.active_preset.clone();
    let typed = state.typed_config();
    let design = state.current_design().unwrap_or_default();
    with_window(|window| {
        if let Some(config) = &typed {
            window.refresh_geometry(config, &design, &source, &theme);
        }
        show_header(window, ui, log);
        ui.separator();
        show_tabs(window, ui);
        ui.add_space(6.0);
    });
    match with_window(|window| window.tab) {
        WingAnalysisTab::Setup => setup::show_setup_tab(state, ui),
        WingAnalysisTab::Results => results::show_results_tab(ui),
    }
}

/// Identity, the configuration being analysed, the run actions and the status.
fn show_header(window: &mut WingAnalysisState, ui: &mut Ui, log: &mut Vec<(String, LogKind)>) {
    // The identity, the actions and the status keep separate wrapping rows.
    // A right-aligned action group anchors to the layout width, which in a
    // detached viewport can exceed the painted clip rectangle and push the
    // buttons out of sight on a narrow window.
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Wing Analysis")).strong().size(17.0))
            .on_hover_text(tr(
                "Aerodynamics of the selected wing alone, with no mission, payload or whole-aircraft run.",
            ));
        ui.separator();
        ui.label(RichText::new(configuration_label(window)).strong());
    });
    ui.horizontal_wrapped(|ui| show_run_actions(window, ui, log));
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Status")).weak().small());
        show_status(window, ui);
    });
    if let Some(error) = &window.error {
        ui.colored_label(
            ui.visuals().error_fg_color,
            tr_fields("Wing analysis error: {error}", &[("error", error.clone())]),
        );
    }
    if let Some(error) = &window.geometry_error {
        ui.colored_label(
            ui.visuals().error_fg_color,
            tr_fields(
                "The wing geometry could not be built: {error}",
                &[("error", error.clone())],
            ),
        );
    }
}

/// The modelled configuration, named the way every output must be read.
pub(crate) fn configuration_label(window: &WingAnalysisState) -> String {
    if window.includes_empennage() {
        tr("Wing and empennage")
    } else {
        tr("Wing only")
    }
}

/// Run and cancel, reachable from both tabs.
fn show_run_actions(window: &mut WingAnalysisState, ui: &mut Ui, log: &mut Vec<(String, LogKind)>) {
    let running = window.running();
    if ui
        .add_enabled(running, egui::Button::new(tr("Cancel analysis")))
        .clicked()
    {
        window.cancel();
        log.push((tr("Wing analysis cancellation requested."), LogKind::Warn));
    }
    if ui
        .add_enabled(!running, egui::Button::new(tr("Run wing analysis")))
        .on_hover_text(tr(
            "Solve the modelled surfaces at the stated condition on a background thread.",
        ))
        .clicked()
    {
        match window.start() {
            Ok(run_id) => log.push((
                tr_fields(
                    "Wing analysis run #{id} started.",
                    &[("id", run_id.to_string())],
                ),
                LogKind::Info,
            )),
            Err(error) => log.push((error, LogKind::Error)),
        }
    }
}

/// The run phase, never softened: a stale or cancelled run says so.
fn show_status(window: &WingAnalysisState, ui: &mut Ui) {
    let (text, color) = match window.phase {
        RunPhase::Idle => (tr("Ready."), ui.visuals().weak_text_color()),
        RunPhase::Running => (tr("Running."), ui.visuals().strong_text_color()),
        RunPhase::Cancelling => (tr("Cancelling."), ui.visuals().warn_fg_color),
        RunPhase::Finished if window.result_is_current() => {
            (tr("Finished."), crate::theme::success_color(ui.visuals()))
        }
        RunPhase::Finished => (
            tr("Superseded by an input change."),
            ui.visuals().warn_fg_color,
        ),
        RunPhase::Failed => (tr("Failed."), ui.visuals().error_fg_color),
    };
    ui.colored_label(color, text);
    if !window.status.is_empty() {
        ui.label(RichText::new(&window.status).weak());
    }
}

/// The two window tabs.
fn show_tabs(window: &mut WingAnalysisState, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        for (tab, label) in [
            (WingAnalysisTab::Setup, "Setup"),
            (WingAnalysisTab::Results, "Results"),
        ] {
            if ui
                .add(crate::theme::selectable_button(
                    tr(label),
                    window.tab == tab,
                ))
                .clicked()
            {
                window.tab = tab;
            }
        }
    });
}
