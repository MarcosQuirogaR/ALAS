// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The menu bar and the figure and report exports it triggers.

use std::path::Path;

use egui::{Context, Ui};

use super::{tr, tr_fields, AlasApp};
use crate::layout;
use crate::state::{AppState, LogKind};
use crate::view_controls::render_view_options;

impl AlasApp {
    /// Top-bar entry point for standalone analyses that do not require a
    /// whole-aircraft pipeline run.
    pub(super) fn render_analysis_menu(&mut self, ui: &mut Ui) {
        // An immediate egui viewport lives only while its parent keeps
        // showing it, so the detached window is dispatched beside its entry.
        crate::views::wing_analysis_view::show_wing_analysis_window(&mut self.state, ui.ctx());
        ui.menu_button(tr("Analysis"), |ui| {
            if ui.button(tr("Open Wing Analysis")).clicked() {
                crate::views::wing_analysis_view::open_wing_analysis(ui.ctx());
                ui.close_menu();
            }
            if ui.button(tr("Open Airfoil CFD")).clicked() {
                self.state.cfd.window_open = true;
                self.state.cfd.tab = crate::cfd::CfdTab::Study;
                ui.close_menu();
            }
            if ui.button(tr("Open Airfoil Screening")).clicked() {
                // Opening an already-open workspace means "show it to me": the
                // window may be behind the main one, where setting the flag
                // again would look like the action did nothing.
                if self.state.screening.window_open {
                    crate::views::screening_window::focus_window(ui.ctx());
                }
                self.state.screening.window_open = true;
                ui.close_menu();
            }
        });
    }

    pub(super) fn render_file_menu(&mut self, ui: &mut Ui) {
        ui.menu_button(tr("File"), |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("Path:"));
                ui.text_edit_singleline(&mut self.state.config_path);
            });
            if ui.button(tr("Load configuration")).clicked() {
                self.state.load_config();
                ui.close_menu();
            }
            if ui.button(tr("Save configuration")).clicked() {
                self.state.save_config();
                ui.close_menu();
            }
            ui.separator();
            let exports_enabled = self.state.pipeline_result_complete;
            if ui
                .add_enabled(
                    exports_enabled,
                    egui::Button::new(tr("Export figures (ZIP)")),
                )
                .clicked()
            {
                export_figures(&mut self.state);
                ui.close_menu();
            }
            if ui
                .add_enabled(
                    exports_enabled,
                    egui::Button::new(tr("Generate report (PDF)")),
                )
                .clicked()
            {
                export_report(&mut self.state);
                ui.close_menu();
            }
            ui.separator();
            if ui.button(tr("Manage storage...")).clicked() {
                self.state.show_storage = true;
                ui.close_menu();
            }
            ui.separator();
            ui.menu_button(tr("Load preset"), |ui| {
                for (name, display) in self.state.preset_names.clone() {
                    if ui.button(display).clicked() {
                        self.state.load_preset(&name);
                        ui.close_menu();
                    }
                }
            });
            ui.separator();
            if ui.button(tr("Clean sheet design (sandbox)")).clicked() {
                self.state.enter_sandbox(false);
                ui.close_menu();
            }
            ui.separator();
            if ui.button(tr("Exit")).clicked() {
                std::process::exit(0);
            }
        });
    }

    pub(super) fn render_view_menu(&mut self, ctx: &Context, ui: &mut Ui) {
        ui.menu_button(tr("View"), |ui| {
            ui.set_min_width(layout::MENU_MIN_WIDTH);
            let run_log_label = if self.state.run_log_open {
                "Hide Run Log"
            } else {
                "Show Run Log"
            };
            if ui.button(tr(run_log_label)).clicked() {
                self.state.run_log_open = !self.state.run_log_open;
                ui.close_menu();
            }
            ui.separator();
            render_view_options(&mut self.state, ctx, ui, true);
            #[cfg(debug_assertions)]
            self.layout_debug.show_menu(ui);
        });
    }

    /// Expose the appearance and zoom controls in a normal movable egui
    /// window when users need them while inspecting a figure. It is intentionally
    /// separate from the View menu: moving or resizing it never consumes page
    /// space or forces a configuration page to grow.
    pub(super) fn show_detached_view_panel(&mut self, ctx: &Context) {
        if !self.state.show_view_panel {
            return;
        }
        let response = crate::native_viewport::show_native_viewport(
            ctx,
            "view_options",
            tr("View options"),
            egui::ViewportBuilder::default()
                .with_title(tr("View options"))
                .with_inner_size(egui::vec2(290.0, 360.0))
                .with_min_inner_size(egui::vec2(240.0, 260.0))
                .with_resizable(true),
            |child_ctx, ui, _class| {
                render_view_options(&mut self.state, child_ctx, ui, false);
            },
        );
        if response.close_requested {
            self.state.show_view_panel = false;
        }
    }

    pub(super) fn render_help_menu(&mut self, ui: &mut Ui) {
        ui.menu_button(tr("Help"), |ui| {
            if ui.button(tr("Replay Walkthrough")).clicked() {
                self.state.begin_walkthrough();
                ui.close_menu();
            }
            if ui.button(tr("Advanced Walkthrough...")).clicked() {
                self.state.show_advanced_guide = true;
                ui.close_menu();
            }
            if ui.button(tr("External Tools Overview...")).clicked() {
                self.state.tool_intro_selected_config = None;
                crate::views::tool_intro::open_tool_manager(&mut self.state, ui.ctx());
                ui.close_menu();
            }
            ui.separator();
            if ui.button(tr("Documentation")).clicked() {
                ui.ctx()
                    .open_url(egui::OpenUrl::new_tab(ALAS_DOCUMENTATION_URL));
                ui.close_menu();
            }
            ui.separator();
            if ui.button(tr("About ALAS")).clicked() {
                self.state.show_about = true;
                ui.close_menu();
            }
        });
    }
}

/// Official ALAS documentation opened from Help > Documentation.
pub(super) const ALAS_DOCUMENTATION_URL: &str = "https://alas.uvigo.es/docs/";

/// Export ordered SVG figure sources as a user-facing archive.
fn export_figures(state: &mut AppState) {
    match crate::export::export_figure_archive(state, Path::new("exports")) {
        Ok(receipt) => state.log(
            tr_fields(
                "Figure archive with {count} SVG sources written to {path}.",
                &[
                    ("count", receipt.figure_count.to_string()),
                    ("path", receipt.path.display().to_string()),
                ],
            ),
            LogKind::Info,
        ),
        Err(error) => state.log(
            tr_fields(
                "Figure archive failed: {error}",
                &[("error", error.to_string())],
            ),
            LogKind::Error,
        ),
    }
}

/// Write the ordered figure report as a sectioned vector PDF.
fn export_report(state: &mut AppState) {
    match crate::export::export_pdf_report(state, Path::new("exports")) {
        Ok(receipt) => state.log(
            tr_fields(
                "Sectioned PDF report with {count} SVG sources written to {path}.",
                &[
                    ("count", receipt.figure_count.to_string()),
                    ("path", receipt.path.display().to_string()),
                ],
            ),
            LogKind::Info,
        ),
        Err(error) => state.log(
            tr_fields(
                "PDF report failed: {error}",
                &[("error", error.to_string())],
            ),
            LogKind::Error,
        ),
    }
}
