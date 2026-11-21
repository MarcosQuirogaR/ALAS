// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The full-window sandbox workspace.
//!
//! The standard navigation, content pane, preview dock and run-log dock are
//! replaced by: a menu bar with the actions the sandbox needs and the
//! Advanced Settings action, the central 3D viewport with every other
//! control floating inside it (camera presets, geometry-category buttons,
//! the parameter search, the action row with Quick Analysis, Full
//! Analysis, Undo, Redo and Run log, and the Summary button that lists the
//! derived geometry metrics on demand), and the estimates strip on the
//! right once Quick Analysis has opened it. Native windows carry the Discipline
//! Windows, Advanced Settings, the run log and the Full Analysis results.

use egui::{menu, Context, Frame as EguiFrame, SidePanel, TopBottomPanel, Ui};

use crate::layout;
use crate::state::AppState;
use crate::view_controls::render_view_options;
use crate::views::{overlays, tr};

use super::advanced::{show_advanced_settings_window, show_menu_action};
use super::estimates::show_estimates_strip;
use super::viewport::show_viewport;
use super::windows::{
    show_discipline_windows, show_exit_prompt, show_hump_confirm, show_log_window,
    show_results_window,
};

/// Launch the Quick Analysis for the current revision.
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
    // A text box owns Ctrl+Z / Ctrl+Y (its own text undo), as in the guided shell.
    if ctx.wants_keyboard_input() {
        return;
    }
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
            crate::user_files_ui::show_config_file_buttons(state, ui);
            if ui.button(tr("Load configuration")).clicked() {
                state.guarded(crate::shell_confirm::Pending::ConfigFile);
                ui.close_menu();
            }
            if ui.button(tr("Save configuration")).clicked() {
                state.save_config_settling();
                ui.close_menu();
            }
            ui.separator();
            if ui
                .button(tr("New sandbox from AVE"))
                .on_hover_text(tr("Replace the sandbox aircraft with the AVE reference."))
                .clicked()
            {
                state.guarded(crate::shell_confirm::Pending::NewSandbox);
                ui.close_menu();
            }
            if ui.button(tr("Leave sandbox...")).clicked() {
                state.request_leave_sandbox();
                ui.close_menu();
            }
            ui.separator();
            if ui.button(tr("Exit")).clicked() {
                state.guarded(crate::shell_confirm::Pending::Exit);
                ui.close_menu();
            }
        });
        ui.menu_button(tr("View"), |ui| {
            ui.set_min_width(layout::MENU_MIN_WIDTH);
            crate::checkbox::checkbox(
                ui,
                &mut state.sandbox.layout.estimates_open,
                tr("Estimates strip"),
            );
            if crate::checkbox::checkbox(
                ui,
                &mut state.sandbox.layout.log_window_open,
                tr("Run log window"),
            )
            .changed()
                && state.sandbox.layout.log_window_open
            {
                crate::native_viewport::raise(ctx, "sandbox_run_log");
            }
            if crate::checkbox::checkbox(
                ui,
                &mut state.sandbox.layout.advanced_settings_open,
                tr("Advanced Settings window"),
            )
            .changed()
                && state.sandbox.layout.advanced_settings_open
            {
                crate::native_viewport::raise(ctx, "advanced_settings");
            }
            ui.separator();
            render_view_options(state, ctx, ui, true);
        });
        // One menu-bar order across modes: the guided workspace is
        // File - View - Analysis - Advanced Settings - Help. The standalone
        // analyses act on the loaded aircraft and need no pipeline run, so the
        // same entries are live here; their windows are dispatched by the
        // Sandbox fast path of the app loop.
        crate::app::menus::show_analysis_menu(state, ui);
        show_menu_action(state, ui);
        crate::views::tool_intro::show_menu_action(state, ui);
        ui.menu_button(tr("Help"), |ui| {
            if ui.button(tr("About ALAS")).clicked() {
                state.show_about = true;
                ui.close_menu();
            }
        });
        // Top-right: the one-click way out of Sandbox mode (the same dialog as
        // File > Leave sandbox).
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if exit_sandbox_button(ui)
                .on_hover_text(tr("Exit Sandbox mode"))
                .clicked()
            {
                state.request_leave_sandbox();
            }
        });
    });
}

/// The subtle red cross at the top-right of the Sandbox bar: a faint red
/// plate at rest, a stronger one under the pointer. The cross is painted, not
/// a glyph, so it never depends on a font.
fn exit_sandbox_button(ui: &mut Ui) -> egui::Response {
    const RED: (u8, u8, u8) = (229, 83, 75);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(24.0, 22.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered() || response.has_focus();
        let (fill_alpha, color) = if hovered {
            (56, egui::Color32::from_rgb(RED.0, RED.1, RED.2))
        } else {
            (26, egui::Color32::from_rgb(0xff, 0x9a, 0x93))
        };
        ui.painter().rect_filled(
            rect,
            crate::theme::SELECTABLE_CONTROL_RADIUS,
            egui::Color32::from_rgba_unmultiplied(RED.0, RED.1, RED.2, fill_alpha),
        );
        let stroke = egui::Stroke::new(1.6_f32, color);
        let arm = 4.5;
        let c = rect.center();
        ui.painter().line_segment(
            [c + egui::vec2(-arm, -arm), c + egui::vec2(arm, arm)],
            stroke,
        );
        ui.painter().line_segment(
            [c + egui::vec2(-arm, arm), c + egui::vec2(arm, -arm)],
            stroke,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Render the whole sandbox workspace for this frame.
pub fn show_sandbox_workspace(state: &mut AppState, ctx: &Context) {
    if state.sandbox.estimates.poll() || state.sandbox.estimates.running() {
        // Quick Analysis reports progress through an mpsc channel with no
        // egui::Context of its own, so a 10 Hz poll (the cadence already
        // used elsewhere for channel-backed background jobs) picks up
        // completion promptly without spinning the UI at display refresh
        // rate while the estimate is running.
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    handle_shortcuts(state, ctx);
    // The Ctrl+K palette and its jump highlight, which the guided frame
    // normally drives; the Sandbox returns before that code runs.
    crate::views::search_overlay::handle_shortcuts(state, ctx);
    crate::views::search_overlay::tick_focus(state, ctx);

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
    show_hump_confirm(state, ctx);
    overlays::show_about(state, ctx);
    crate::views::search_overlay::show_overlay(state, ctx);
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
