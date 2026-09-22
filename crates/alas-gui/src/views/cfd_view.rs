// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Standalone Airfoil CFD study window.
//!
//! The window deliberately keeps routine operating controls next to the
//! resolved database geometry. Mesh/solver choices live on the Advanced tab,
//! while result cards only display data supplied by OpenFOAM parsers.

#[path = "cfd_view_parts/mod.rs"]
mod cfd_view_parts;

use egui::{vec2, Layout, RichText, Ui, ViewportBuilder};

use crate::cfd::CfdTab;
use crate::native_viewport::show_native_viewport;
use crate::state::{AppState, LogKind};
use crate::views::tr;
use alas_cfd::CfdOutcome;

use cfd_view_parts::advanced::show_advanced_tab;
use cfd_view_parts::log::show_log_tab;
use cfd_view_parts::results::show_results_tab;
use cfd_view_parts::study::show_study_tab;

/// Render the detached Airfoil CFD study window when it is open.
pub fn show_cfd_window(state: &mut AppState, ctx: &egui::Context) {
    if !state.cfd.window_open {
        return;
    }
    let response = show_native_viewport(
        ctx,
        "airfoil_cfd",
        tr("Airfoil CFD"),
        ViewportBuilder::default()
            .with_title(tr("Airfoil CFD"))
            .with_inner_size(vec2(1040.0, 760.0))
            .with_min_inner_size(vec2(620.0, 420.0))
            .with_resizable(true),
        |_child_ctx, ui, _class| {
            show_header(state, ui);
            show_tabs(state, ui);
            ui.separator();
            match state.cfd.tab {
                CfdTab::Study => show_study_tab(state, ui),
                CfdTab::Advanced => show_advanced_tab(state, ui),
                CfdTab::Results => show_results_tab(state, ui),
                CfdTab::Log => show_log_tab(state, ui),
            }
        },
    );
    if response.close_requested {
        state.cfd.window_open = false;
    }
}

/// Width the run actions need before they can share the identity row.
const HEADER_ACTION_WIDTH: f32 = 250.0;

/// Two compact header rows: identity with the run and cancel actions, which
/// stay reachable from every tab, then the honest run status.
fn show_header(state: &mut AppState, ui: &mut Ui) {
    let mut actions_wrapped = false;
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Airfoil CFD")).strong().size(17.0))
            .on_hover_text(tr("Standalone 2-D OpenFOAM study"));
        ui.separator();
        ui.label(RichText::new(state.cfd.selected_airfoil()).strong());
        if ui.available_width() >= HEADER_ACTION_WIDTH {
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                show_run_actions(state, ui);
            });
        } else {
            actions_wrapped = true;
        }
    });
    if actions_wrapped {
        ui.horizontal_wrapped(|ui| show_run_actions(state, ui));
    }
    // The status keeps its own row: it is the longest piece of text here and
    // must be able to wrap without being covered by the run actions.
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Status")).weak().small());
        show_status(state, ui);
    });
    if let Some(error) = &state.cfd.error {
        ui.colored_label(
            ui.visuals().error_fg_color,
            crate::views::tr_fields("CFD error: {error}", &[("error", error.clone())]),
        );
    }
}

/// Run and cancel, reachable from every tab.  In a right-to-left header row
/// the cancel button is added first so run ends up leftmost on screen.
fn show_run_actions(state: &mut AppState, ui: &mut Ui) {
    let running = state.cfd.running;
    if ui
        .add_enabled(running, egui::Button::new(tr("Cancel CFD")))
        .clicked()
    {
        state.cfd.cancel_run();
        state.log("Airfoil CFD cancellation requested.", LogKind::Warn);
    }
    if ui
        .add_enabled(!running, egui::Button::new(tr("Run CFD study")))
        .on_hover_text(tr("Prepare an isolated case, generate/check the mesh, solve, and collect actual OpenFOAM results in the background."))
        .clicked()
    {
        match state.cfd.start_run() {
            Ok(run_id) => state.log(format!("Airfoil CFD run #{run_id} started."), LogKind::Info),
            Err(error) => state.log(error, LogKind::Error),
        }
    }
}

/// The status reported here is never softened: a finished run keeps its
/// recorded numerical outcome and its colour, and an active stage shows the
/// stage text the worker actually emitted.
fn show_status(state: &AppState, ui: &mut Ui) {
    if state.cfd.running || state.cfd.probing {
        ui.spinner();
        ui.colored_label(ui.visuals().warn_fg_color, tr(state.cfd.status.as_str()))
            .on_hover_text(tr(state.cfd.status.as_str()));
        return;
    }
    if let Some(result) = state.cfd.result.as_ref() {
        let (color, label) = match result.outcome {
            CfdOutcome::NumericallyConverged => (
                crate::theme::success_color(ui.visuals()),
                "Numerically converged",
            ),
            CfdOutcome::Unconverged => (ui.visuals().warn_fg_color, "Unconverged"),
            CfdOutcome::Cancelled => (ui.visuals().warn_fg_color, "Cancelled"),
            CfdOutcome::Failed => (ui.visuals().error_fg_color, "Failed"),
        };
        ui.colored_label(color, tr(label))
            .on_hover_text(result.status_detail.as_str());
        return;
    }
    ui.label(RichText::new(tr(state.cfd.status.as_str())).weak())
        .on_hover_text(tr(state.cfd.status.as_str()));
}

fn show_tabs(state: &mut AppState, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        for (tab, label) in [
            (CfdTab::Study, "Study"),
            (CfdTab::Advanced, "Advanced"),
            (CfdTab::Results, "Results"),
            (CfdTab::Log, "Run log"),
        ] {
            if ui
                .selectable_label(state.cfd.tab == tab, tr(label))
                .clicked()
            {
                state.cfd.tab = tab;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfd::AirfoilCfdState;
    use cfd_view_parts::drawing::{bounds, paint_airfoil_outline};

    #[test]
    fn airfoil_outline_projection_preserves_positive_y_up() {
        let ctx = egui::Context::default();
        let mut pixels = 0;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    vec2(420.0, 240.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    paint_airfoil_outline(
                        ui,
                        egui::Rect::from_min_size(egui::pos2(10.0, 10.0), vec2(400.0, 200.0)),
                        Some(&[(0.0, 0.0), (1.0, 0.0), (0.0, 0.2)]),
                    );
                    pixels += 1;
                });
            },
        );
        assert_eq!(pixels, 1);
    }

    #[test]
    fn line_plot_bounds_expand_constant_series() {
        assert_eq!(bounds([1.0, 1.0].into_iter()), (0.0, 2.0));
        assert_eq!(bounds(std::iter::empty()), (0.0, 1.0));
    }

    #[test]
    fn cfd_default_state_has_an_actual_preview_before_running() {
        let state = AirfoilCfdState::default();
        assert!(state.preview_coordinates.is_some());
        assert!(!state.running);
    }
}
