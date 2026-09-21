// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

impl AlasApp {
    /// Top-bar entry point for standalone analyses that do not require a
    /// whole-aircraft pipeline run.
    fn render_analysis_menu(&mut self, ui: &mut Ui) {
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

    fn render_file_menu(&mut self, ui: &mut Ui) {
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

    fn render_view_menu(&mut self, ctx: &Context, ui: &mut Ui) {
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
    fn show_detached_view_panel(&mut self, ctx: &Context) {
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

    fn render_help_menu(&mut self, ui: &mut Ui) {
        ui.menu_button(tr("Help"), |ui| {
            if ui.button(tr("Replay Walkthrough")).clicked() {
                self.state.begin_walkthrough();
                ui.close_menu();
            }
            if ui.button(tr("Advanced Walkthrough...")).clicked() {
                self.state.show_advanced_guide = true;
                ui.close_menu();
            }
            ui.separator();
            if ui
                .selectable_label(self.state.help_verbose, tr("Learn-more help"))
                .clicked()
            {
                self.state.help_verbose = !self.state.help_verbose;
                ui.close_menu();
            }
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
const ALAS_DOCUMENTATION_URL: &str = "https://alas.uvigo.es/docs/";

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

#[cfg(test)]
mod tests {
    use super::{AlasApp, ALAS_DOCUMENTATION_URL};
    use crate::state::{AppState, Language};
    use crate::view_controls::{
        auto_zoom_factor, auto_zoom_for_physical_size, zoom_after_command, ZoomCommand,
    };
    use egui::vec2;

    #[test]
    fn shell_menu_and_log_strings_have_spanish_desktop_translations() {
        let catalog = alas_i18n::es::desktop_catalog();
        for key in [
            "Navigation",
            "Unpin",
            "Pin",
            "Collapse navigation to an 8 px hover rail",
            "Keep navigation open and reserve its column",
            "Open navigation",
            "Unknown page.",
            "File",
            "Path:",
            "Load configuration",
            "Save configuration",
            "Export figures (ZIP)",
            "Generate report (PDF)",
            "Manage storage...",
            "Load preset",
            "Exit",
            "View",
            "Dark",
            "Light",
            "Grey",
            "3D Live Preview",
            "Show Run Log",
            "Hide Run Log",
            "Close",
            "Reduced Animations",
            "Automatic zoom",
            "English",
            "Spanish",
            "Zoom in",
            "Zoom out",
            "Reset zoom (100%)",
            "Help",
            "Replay Walkthrough",
            "Advanced Walkthrough...",
            "Learn-more help",
            "Documentation",
            "About ALAS",
            "Figure archive with {count} SVG sources written to {path}.",
            "Figure archive failed: {error}",
            "Sectioned PDF report with {count} SVG sources written to {path}.",
            "PDF report failed: {error}",
        ] {
            assert!(catalog.contains_key(key), "missing shell text: {key}");
        }
    }

    #[test]
    fn explicit_state_constructor_activates_the_selected_walkthrough_language() {
        let mut state = AppState::default();
        state.finish_walkthrough();
        state.language = Language::Es;
        let _app = AlasApp::from_state(state);

        assert_eq!(alas_i18n::get_language(), "es");
        alas_i18n::set_language(Some("en"));
    }

    #[test]
    fn documentation_menu_target_is_the_official_alas_docs_url() {
        assert_eq!(ALAS_DOCUMENTATION_URL, "https://alas.uvigo.es/docs/");
    }

    #[test]
    fn fresh_state_enables_explanatory_help_by_default() {
        assert!(AppState::default().help_verbose);
    }

    #[test]
    fn explicit_help_preference_survives_explicit_state_constructor() {
        let mut state = AppState::default();
        state.help_verbose = false;
        let app = AlasApp::from_state(state);

        assert!(!app.state.help_verbose);
        alas_i18n::set_language(Some("en"));
    }

    #[test]
    fn zoom_commands_stay_inside_the_readable_interface_range() {
        assert_eq!(zoom_after_command(2.2, ZoomCommand::In), 2.2);
        assert_eq!(zoom_after_command(0.75, ZoomCommand::Out), 0.75);
        assert_eq!(zoom_after_command(1.4, ZoomCommand::Reset), 1.0);
    }

    #[test]
    fn automatic_zoom_scales_from_the_client_area_with_recoverable_bounds() {
        assert_eq!(auto_zoom_for_physical_size(vec2(1_240.0, 760.0)), 1.0);
        assert_eq!(auto_zoom_for_physical_size(vec2(620.0, 380.0)), 0.90);
        assert_eq!(auto_zoom_for_physical_size(vec2(3_840.0, 2_160.0)), 1.35);
        assert_eq!(auto_zoom_for_physical_size(vec2(1_400.0, 850.0)), 1.125);
    }

    #[test]
    fn automatic_zoom_stays_stable_when_egui_applies_a_pending_zoom() {
        let context = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                vec2(1_400.0, 850.0),
            )),
            ..egui::RawInput::default()
        };

        let _ = context.run(input.clone(), |_| {});
        let automatic = auto_zoom_factor(&context);
        context.set_zoom_factor(automatic);
        let _ = context.run(input, |_| {});

        assert_eq!(auto_zoom_factor(&context), automatic);
    }
}
