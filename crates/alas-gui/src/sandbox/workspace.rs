// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The full-window sandbox workspace.
//!
//! The standard navigation, content pane, preview dock and run-log dock are
//! replaced by: a menu bar with the actions the sandbox needs and the
//! Advanced Settings action, the central 3D viewport with its floating
//! controls (camera presets, geometry-category buttons and the parameter
//! search), the estimates strip on the right, and a bottom bar with the
//! derived geometry metrics on the left and the Quick Analysis and Full
//! Analysis actions on the right. Floating windows carry the Discipline
//! Windows, Advanced Settings, the run log and the Full Analysis results.

use egui::{menu, Context, Frame as EguiFrame, RichText, SidePanel, TopBottomPanel, Ui};

use crate::layout;
use crate::state::AppState;
use crate::view_controls::render_view_options;
use crate::views::{overlays, tr};

use super::advanced::{show_advanced_settings_window, show_menu_action};
use super::estimates::show_estimates_strip;
use super::scene::geometry_metrics;
use super::viewport::show_viewport;
use super::windows::{
    show_discipline_windows, show_exit_prompt, show_log_window, show_results_window,
};

/// Launch the reduced Quick Analysis for the current revision.
pub fn start_quick_analysis(state: &mut AppState) {
    let (Some(config), Some(design)) = (state.typed_config(), state.current_design()) else {
        state.status_message =
            tr("The sandbox configuration is not valid; fix the highlighted fields first.");
        return;
    };
    let revision = state.sandbox.revision;
    state.sandbox.estimates.start(config, design, revision);
    state.sandbox.layout.estimates_open = true;
    state.status_message = tr("Quick Analysis running...");
}

/// Launch the Full Analysis of the drawn aircraft.
pub fn start_full_analysis(state: &mut AppState) {
    if state.is_running {
        return;
    }
    state.sandbox.full_analysis_revision = Some(state.sandbox.revision);
    state.start_pipeline(true);
}

fn handle_shortcuts(state: &mut AppState, ctx: &Context) {
    let (undo, redo) = ctx.input_mut(|input| {
        let undo = input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z);
        let redo = input.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
            || input.consume_key(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::Z,
            );
        (undo, redo)
    });
    if undo {
        state.sandbox_undo();
    }
    if redo {
        state.sandbox_redo();
    }
}

fn show_menu_bar(state: &mut AppState, ctx: &Context, ui: &mut Ui) {
    menu::bar(ui, |ui| {
        if let Some(image) = crate::branding::text_logo_image(ctx) {
            ui.add(image.max_width(132.0).max_height(20.0));
        }
        ui.separator();
        ui.menu_button(tr("File"), |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("Path:"));
                ui.text_edit_singleline(&mut state.config_path);
            });
            if ui.button(tr("Load configuration")).clicked() {
                state.load_config();
                ui.close_menu();
            }
            if ui.button(tr("Save configuration")).clicked() {
                state.save_config();
                ui.close_menu();
            }
            ui.separator();
            if ui
                .button(tr("New sandbox from AVE"))
                .on_hover_text(tr("Replace the sandbox aircraft with the AVE reference; the current sandbox design is not kept."))
                .clicked()
            {
                state.new_sandbox_from_reference();
                ui.close_menu();
            }
            if ui.button(tr("Leave sandbox...")).clicked() {
                state.request_leave_sandbox();
                ui.close_menu();
            }
            ui.separator();
            if ui.button(tr("Exit")).clicked() {
                std::process::exit(0);
            }
        });
        ui.menu_button(tr("View"), |ui| {
            ui.set_min_width(layout::MENU_MIN_WIDTH);
            ui.checkbox(
                &mut state.sandbox.layout.estimates_open,
                tr("Estimates strip"),
            );
            ui.checkbox(
                &mut state.sandbox.layout.log_window_open,
                tr("Run log window"),
            );
            ui.checkbox(
                &mut state.sandbox.layout.advanced_settings_open,
                tr("Advanced Settings window"),
            );
            ui.separator();
            render_view_options(state, ctx, ui, true);
        });
        ui.menu_button(tr("Help"), |ui| {
            if ui.button(tr("About ALAS")).clicked() {
                state.show_about = true;
                ui.close_menu();
            }
        });
        ui.separator();
        show_menu_action(state, ui);
    });
}

fn show_metrics(state: &AppState, ui: &mut Ui) {
    let Some(plane) = &state.sandbox.airplane else {
        return;
    };
    let Some(design) = state.current_design() else {
        return;
    };
    let m = geometry_metrics(plane, &design);
    let text = format!(
        "S_ref {:.1} m2   b {:.2} m   MAC {:.2} m   LE sweep {:.1} deg   c/4 sweep {:.1} deg   AR {:.2}   taper {:.3}   L_fus {:.2} m",
        m.reference_area_m2,
        m.span_m,
        m.mean_aerodynamic_chord_m,
        m.leading_edge_sweep_deg,
        m.quarter_chord_sweep_deg,
        m.aspect_ratio,
        m.taper_ratio,
        m.fuselage_length_m
    );
    ui.label(RichText::new(text).monospace().small())
        .on_hover_text(tr(
            "S_ref: projected planform area of the main wing including the carry-through, m2. b: projected tip-to-tip span, m. MAC: mean aerodynamic chord, m. LE sweep: inboard leading-edge sweep design variable, deg, positive aft. c/4 sweep: area-weighted mean quarter-chord sweep of the lofted sections, deg. AR: b^2 / S_ref. taper: tip chord over root chord. L_fus: overall fuselage length, m. Axes: x aft, y right, z up.",
        ));
}

fn show_bottom_bar(state: &mut AppState, ui: &mut Ui) {
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        show_metrics(state, ui);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let running = state.is_running;
            let blocked = state.blocked();
            if ui
                .add_enabled(!running && !blocked, egui::Button::new(tr("Full Analysis")))
                .on_hover_text(tr("Run the complete pipeline on the drawn aircraft as a fixed design; results open in their own window."))
                .clicked()
            {
                start_full_analysis(state);
            }
            let quick = egui::Button::new(RichText::new(tr("Quick Analysis")).strong());
            if ui
                .add_enabled(!blocked && !state.sandbox.estimates.running(), quick)
                .on_hover_text(tr("Reduced in-process estimates for the drawn aircraft; first results within seconds, labelled as initial estimates."))
                .clicked()
            {
                start_quick_analysis(state);
            }
            if running {
                if ui
                    .add_enabled(!state.cancellation_requested, egui::Button::new(tr("Cancel")))
                    .clicked()
                {
                    state.request_pipeline_cancel();
                }
                ui.spinner();
            }
            if ui.add(egui::Button::new(tr("Run log")).small()).clicked() {
                state.sandbox.layout.log_window_open = !state.sandbox.layout.log_window_open;
            }
            if state.pipeline_result.is_some()
                && ui.add(egui::Button::new(tr("Results")).small()).clicked()
            {
                state.sandbox.results_window_open = true;
            }
            ui.separator();
            if ui
                .add_enabled(state.sandbox.undo.can_redo(), egui::Button::new(tr("Redo")).small())
                .clicked()
            {
                state.sandbox_redo();
            }
            if ui
                .add_enabled(state.sandbox.undo.can_undo(), egui::Button::new(tr("Undo")).small())
                .clicked()
            {
                state.sandbox_undo();
            }
            ui.separator();
            let status = tr(&state.status_message);
            ui.label(RichText::new(status).small());
        });
    });
    ui.add_space(4.0);
}

/// Render the whole sandbox workspace for this frame.
pub fn show_sandbox_workspace(state: &mut AppState, ctx: &Context) {
    if state.sandbox.estimates.poll() || state.sandbox.estimates.running() {
        ctx.request_repaint();
    }
    handle_shortcuts(state, ctx);

    TopBottomPanel::top("sandbox_menu_bar")
        .frame(
            EguiFrame::side_top_panel(ctx.style().as_ref()).inner_margin(egui::Margin {
                left: 8.0,
                right: 8.0,
                top: layout::MENU_BAR_VERTICAL_INSET,
                bottom: layout::MENU_BAR_VERTICAL_INSET,
            }),
        )
        .show(ctx, |ui| show_menu_bar(state, ctx, ui));

    TopBottomPanel::bottom("sandbox_bottom_bar").show(ctx, |ui| show_bottom_bar(state, ui));

    if state.sandbox.layout.estimates_open {
        let estimates = SidePanel::right("sandbox_estimates_strip")
            .resizable(true)
            .default_width(state.sandbox.layout.estimates_width)
            .width_range(240.0..=460.0)
            .show(ctx, |ui| show_estimates_strip(state, ui));
        state.sandbox.layout.estimates_width = estimates.response.rect.width();
    }

    egui::CentralPanel::default()
        .frame(EguiFrame::central_panel(ctx.style().as_ref()).inner_margin(egui::Margin::same(6.0)))
        .show(ctx, |ui| show_viewport(state, ui));

    show_discipline_windows(state, ctx);
    show_advanced_settings_window(state, ctx);
    show_log_window(state, ctx);
    show_results_window(state, ctx);
    show_exit_prompt(state, ctx);
    overlays::show_about(state, ctx);
}

/// Render the sandbox when it is active; returns whether it was rendered so
/// the shell can skip its own panels for this frame.
pub fn show_if_active(state: &mut AppState, ctx: &Context) -> bool {
    if !state.sandbox.active() {
        return false;
    }
    show_sandbox_workspace(state, ctx);
    true
}
