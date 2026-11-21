// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The menu bar and the figure and report exports it triggers.

use std::path::PathBuf;

use egui::{Context, Ui};

use super::{tr, tr_fields, tr_label, AlasApp};
use crate::layout;
use crate::state::{AppState, LogKind};
use crate::view_controls::render_view_options;

/// The Analysis menu: the standalone analyses, which act on the loaded aircraft and
/// need no pipeline run. The guided workspace and the Sandbox show the same entries.
pub(crate) fn show_analysis_menu(state: &mut AppState, ui: &mut Ui) {
    ui.menu_button(tr("Analysis"), |ui| {
        if ui.button(tr_label("Open Wing Analysis")).clicked() {
            crate::native_viewport::raise(ui.ctx(), "wing_analysis");
            crate::views::wing_analysis_view::open_wing_analysis(ui.ctx());
            ui.close_menu();
        }
        if ui.button(tr_label("Open Airfoil CFD")).clicked() {
            crate::native_viewport::raise(ui.ctx(), "airfoil_cfd");
            state.cfd.window_open = true;
            state.cfd.tab = crate::cfd::CfdTab::Study;
            ui.close_menu();
        }
        if ui.button(tr_label("Open Airfoil Screening")).clicked() {
            // Opening an already-open workspace means "show it to me": the
            // window may be behind the main one, where setting the flag
            // again would look like the action did nothing.
            if state.screening.window_open {
                crate::views::screening_window::focus_window(ui.ctx());
                crate::native_viewport::raise(ui.ctx(), "airfoil_screening");
            }
            state.screening.window_open = true;
            ui.close_menu();
        }
    });
}

/// Horizontal clearance [px] kept right of the top-bar controls: the width of the
/// native minimise / maximise / close column plus a gap.
const WINDOW_BUTTON_COLUMN_CLEARANCE: f32 = 150.0;

impl AlasApp {
    /// Top-bar entry point for standalone analyses that do not require a
    /// whole-aircraft pipeline run.
    pub(super) fn render_analysis_menu(&mut self, ui: &mut Ui) {
        // An immediate egui viewport lives only while its parent keeps
        // showing it, so the detached window is dispatched beside its entry.
        crate::views::wing_analysis_view::show_wing_analysis_window(&mut self.state, ui.ctx());
        show_analysis_menu(&mut self.state, ui);
    }

    /// The right end of the top bar: the search button and the global
    /// Basic / Advanced switch.
    pub(super) fn render_top_bar_controls(&mut self, ui: &mut Ui) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // The Windows snap-layout flyout of the maximise button drops over
            // the right end of the bar, so the controls stay left of the
            // window-button column (three 46 px buttons) at every width.
            ui.add_space(WINDOW_BUTTON_COLUMN_CLEARANCE);
            // One segmented control: equal, padded segments with a tight gap.
            ui.scope(|ui| {
                crate::theme::segmented_style(ui);
                ui.spacing_mut().button_padding.x = 10.0;
                for detail in [
                    crate::detail::Detail::Advanced,
                    crate::detail::Detail::Basic,
                ] {
                    let selected = self.state.shell.detail == detail;
                    let size = egui::vec2(84.0, crate::theme::control_height(ui));
                    if ui
                        .add(
                            crate::theme::selectable_button(tr(detail.label()), selected)
                                .min_size(size),
                        )
                        .clicked()
                    {
                        self.state.shell.detail = detail;
                    }
                }
            });
            crate::views::search_overlay::show_button(&mut self.state, ui);
        });
    }

    /// The File menu: start, open, save, the library and exit. Exports sit
    /// in their own submenu.
    pub(super) fn render_file_menu(&mut self, ui: &mut Ui) {
        ui.menu_button(tr("File"), |ui| {
            ui.set_min_width(layout::MENU_MIN_WIDTH);
            ui.menu_button(tr_label("New from preset..."), |ui| {
                for (name, display) in self.state.preset_names.clone() {
                    if ui.button(display).clicked() {
                        self.state
                            .guarded(crate::shell_confirm::Pending::Preset(name));
                        ui.close_menu();
                    }
                }
            });
            if ui.button(tr_label("New blank design")).clicked() {
                self.state.shell.start_mode = crate::views::aircraft_start::StartMode::CleanSheet;
                crate::app::go_to_page(&mut self.state, "inputs");
                ui.close_menu();
            }
            ui.separator();
            if ui
                .add_enabled(
                    !self
                        .state
                        .file_dialog_pending(&crate::file_dialog::DialogTarget::LibraryOpenPath),
                    egui::Button::new(tr_label("Open...")),
                )
                .clicked()
            {
                let data = self.state.data_dir();
                let dialog = crate::file_dialog::spec(
                    crate::file_dialog::DialogKind::OpenFile,
                    "Open File",
                    "",
                    &data,
                    None,
                    crate::file_dialog::config_filters(),
                );
                self.state
                    .begin_file_dialog(crate::file_dialog::DialogTarget::LibraryOpenPath, dialog);
                ui.close_menu();
            }
            self.render_open_recent(ui);
            ui.separator();
            if ui.button(tr_label("Save")).clicked() {
                self.state.save_config_settling();
                ui.close_menu();
            }
            if ui.button(tr_label("Save as my aircraft...")).clicked() {
                // An open save window may be minimised or behind this one.
                crate::native_viewport::raise(ui.ctx(), "save_my_aircraft");
                self.state.files.save_name = self.state.default_user_aircraft_name();
                self.state.files.save_open = true;
                self.state.files.status = None;
                ui.close_menu();
            }
            if ui.button(tr_label("Library...")).clicked() {
                crate::library_window::open_raised(
                    &mut self.state,
                    ui.ctx(),
                    crate::library_window::LibraryTab::Aircraft,
                );
                ui.close_menu();
            }
            ui.separator();
            ui.menu_button(tr_label("Export"), |ui| {
                let exports_enabled = self.state.pipeline_result_complete;
                if ui
                    .add_enabled(
                        exports_enabled,
                        egui::Button::new(tr_label("Export figures (ZIP)")),
                    )
                    .clicked()
                {
                    export_figures(&mut self.state);
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        exports_enabled,
                        egui::Button::new(tr_label("Generate report (PDF)")),
                    )
                    .clicked()
                {
                    export_report(&mut self.state);
                    ui.close_menu();
                }
                if ui
                    .add_enabled(
                        self.state.pipeline_result.is_some(),
                        egui::Button::new(tr_label("Export plot data (CSV + JSON)")),
                    )
                    .clicked()
                {
                    export_plot_data(&mut self.state);
                    ui.close_menu();
                }
            });
            ui.separator();
            if ui.button(tr_label("Exit")).clicked() {
                self.state.guarded(crate::shell_confirm::Pending::Exit);
                ui.close_menu();
            }
        });
    }

    /// The recently used files, newest first; each opens like File > Open.
    fn render_open_recent(&mut self, ui: &mut Ui) {
        let recent = crate::file_library::recent(&self.state.data_dir());
        ui.add_enabled_ui(!recent.is_empty(), |ui| {
            ui.menu_button(tr_label("Open recent"), |ui| {
                for file in recent.iter().take(10) {
                    let name = file
                        .path
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if ui
                        .button(name)
                        .on_hover_text(file.path.display().to_string())
                        .clicked()
                    {
                        self.state
                            .guarded(crate::shell_confirm::Pending::Path(file.path.clone()));
                        self.state.shell.library.invalidate();
                        ui.close_menu();
                    }
                }
            });
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
            if ui.button(tr_label(run_log_label)).clicked() {
                self.state.run_log_open = !self.state.run_log_open;
                ui.close_menu();
            }
            if self.state.nav_pinned && ui.button(tr_label("Collapse Navigation")).clicked() {
                super::nav_panel::set_nav_pinned(&mut self.state, false);
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
            if ui.button(tr_label("Replay Walkthrough")).clicked() {
                self.state.begin_walkthrough();
                ui.close_menu();
            }
            if ui.button(tr_label("External Tools Overview...")).clicked() {
                crate::app::go_to_page(&mut self.state, "setup_tools");
                ui.close_menu();
            }
            ui.separator();
            if ui.button(tr_label("Documentation")).clicked() {
                ui.ctx().open_url(egui::OpenUrl::new_tab(
                    crate::views::form::DOCUMENTATION_URL,
                ));
                ui.close_menu();
            }
            ui.separator();
            if ui.button(tr_label("About ALAS")).clicked() {
                crate::native_viewport::raise(ui.ctx(), "about");
                self.state.show_about = true;
                ui.close_menu();
            }
        });
    }
}

/// Ask for the output folder of an export. The dialog opens in the per-user data folder's
/// `exports` directory, never the process working directory; cancelling writes nothing.
fn pick_export_folder(state: &AppState, title: &str) -> Option<PathBuf> {
    let spec = crate::file_dialog::spec(
        crate::file_dialog::DialogKind::PickFolder,
        title,
        "",
        &state.data_dir().join("exports"),
        None,
        Vec::new(),
    );
    crate::file_dialog::run_blocking(&spec)
}

/// Export ordered SVG figure sources as a user-facing archive.
fn export_figures(state: &mut AppState) {
    let Some(folder) = pick_export_folder(state, &tr("Export figures (ZIP)")) else {
        return;
    };
    match crate::export::export_figure_archive(state, &folder) {
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
    let Some(folder) = pick_export_folder(state, &tr("Generate report (PDF)")) else {
        return;
    };
    match crate::export::export_pdf_report(state, &folder) {
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

/// Write the raw data behind every graph of the run as CSV and JSON files.
///
/// The folder dialog opens in the per-user data folder's `exports` directory;
/// cancelling it writes nothing.
fn export_plot_data(state: &mut AppState) {
    let spec = crate::file_dialog::spec(
        crate::file_dialog::DialogKind::PickFolder,
        &tr("Export plot data (CSV + JSON)"),
        "",
        &state.data_dir().join("exports"),
        None,
        Vec::new(),
    );
    let Some(folder) = crate::file_dialog::run_blocking(&spec) else {
        return;
    };
    match crate::export::export_all_plot_data(state, &folder) {
        Ok(receipt) => state.log(
            tr_fields(
                "Plot data for {count} graphs written to {path}.",
                &[
                    ("count", receipt.figure_count.to_string()),
                    ("path", receipt.path.display().to_string()),
                ],
            ),
            LogKind::Info,
        ),
        Err(error) => state.log(
            tr_fields(
                "Plot data export failed: {error}",
                &[("error", error.to_string())],
            ),
            LogKind::Error,
        ),
    }
}

impl AlasApp {
    /// The Edit menu: undo and redo of the case.
    pub(super) fn render_edit_menu(&mut self, ui: &mut Ui) {
        ui.menu_button(tr_label("Edit"), |ui| {
            if ui
                .add_enabled(
                    self.state.can_shell_undo(),
                    egui::Button::new(tr_label("Undo")).shortcut_text("Ctrl+Z"),
                )
                .clicked()
            {
                self.state.shell_undo();
                ui.close_menu();
            }
            if ui
                .add_enabled(
                    self.state.can_shell_redo(),
                    egui::Button::new(tr_label("Redo")).shortcut_text("Ctrl+Y"),
                )
                .clicked()
            {
                self.state.shell_redo();
                ui.close_menu();
            }
        });
    }

    /// Ctrl+Z and Ctrl+Y (or Ctrl+Shift+Z), unless a text box has the keyboard.
    pub(super) fn handle_undo_shortcuts(&mut self, ctx: &Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let undo = ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
        let redo = ctx.input_mut(|input| {
            input.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
                || input.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Z,
                )
        });
        if undo {
            self.state.shell_undo();
        } else if redo {
            self.state.shell_redo();
        }
    }
}
