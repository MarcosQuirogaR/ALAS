// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Standalone Airfoil CFD study window.
//!
//! The window deliberately keeps routine operating controls next to the
//! resolved database geometry. Mesh/solver choices live on the Advanced tab,
//! while result cards only display data supplied by OpenFOAM parsers.

#[path = "cfd_view_parts/mod.rs"]
mod cfd_view_parts;

use egui::{vec2, RichText, Ui, ViewportBuilder};

use crate::cfd::CfdTab;
use crate::native_viewport::show_native_viewport;
use crate::state::AppState;
use crate::views::tr;

use cfd_view_parts::log::show_log_tab;
use cfd_view_parts::results::show_results_tab;
use cfd_view_parts::study::{show_advanced_tab, show_study_tab};

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
            ui.add_space(6.0);
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

fn show_header(state: &mut AppState, ui: &mut Ui) {
    ui.horizontal_wrapped(|ui| {
        ui.heading(tr("Airfoil CFD"));
        ui.label(
            RichText::new(tr("Standalone 2-D OpenFOAM study"))
                .weak()
                .small(),
        );
        if state.cfd.running {
            ui.spinner();
            ui.colored_label(ui.visuals().warn_fg_color, tr("Running"));
        } else if state.cfd.probing {
            ui.spinner();
            ui.colored_label(ui.visuals().warn_fg_color, tr("Checking connection"));
        }
    });
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(tr("Airfoil")).strong());
        ui.label(state.cfd.selected_airfoil());
        ui.separator();
        ui.label(RichText::new(tr("Status")).strong());
        ui.label(RichText::new(tr(state.cfd.status.as_str())).weak());
    });
    if let Some(error) = &state.cfd.error {
        ui.colored_label(
            ui.visuals().error_fg_color,
            crate::views::tr_fields("CFD error: {error}", &[("error", error.clone())]),
        );
    }
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
