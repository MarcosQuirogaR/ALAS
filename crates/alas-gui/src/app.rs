// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Main `eframe::App` desktop application shell.
//!
//! Reproduces the reference desktop app's layout: a File/View/Help menu bar,
//! a left navigation tree, an optional right-hand 3D preview dock, a run log
//! and control bar along the bottom, and a central content pane that routes
//! to the active [`crate::nav::Page`].

use std::path::Path;

use eframe::{App, Frame};
use egui::{
    menu, pos2, vec2, Align, Area, CentralPanel, Context, Frame as EguiFrame, Layout, Order,
    RichText, ScrollArea, Sense, SidePanel, TopBottomPanel, Ui, Window,
};

use crate::layout;
use crate::nav::{self, PageKind};
use crate::state::{nav_overlay_open_with_bounds, AppState, LogKind};
use crate::theme::{card_frame, navigation_overlay_frame};
use crate::view_controls::{auto_zoom_factor, handle_zoom_shortcuts, render_view_options};
use crate::views::tour_data::TourTarget;
use crate::views::{form_page, overlays};

fn tr(text: &str) -> String {
    alas_i18n::t(Some(text), None).into_owned()
}

fn tr_fields(template: &str, fields: &[(&str, String)]) -> String {
    fields.iter().fold(tr(template), |text, (name, value)| {
        text.replace(&format!("{{{name}}}"), value)
    })
}

/// Main desktop application container for ALAS.
pub struct AlasApp {
    state: AppState,
    #[cfg(debug_assertions)]
    layout_debug: crate::layout_debug::LayoutDebug,
}

impl Default for AlasApp {
    fn default() -> Self {
        // Register the embedded catalog once so form labels and help text can
        // follow the language selected by the desktop shell.
        alas_i18n::es::install();
        let state = AppState::default();
        alas_i18n::set_language(Some(state.language.code()));
        Self {
            state,
            #[cfg(debug_assertions)]
            layout_debug: crate::layout_debug::LayoutDebug::default(),
        }
    }
}

impl AlasApp {
    /// Construct the shell around explicit state for examples and UI audits.
    pub fn from_state(state: AppState) -> Self {
        // `from_state` is also the embedding/testing constructor, so it must
        // establish the same catalog and thread-local language as `default`.
        // Otherwise a Spanish state renders the walkthrough in English until
        // the user opens View and toggles the language once.
        alas_i18n::es::install();
        alas_i18n::set_language(Some(state.language.code()));
        Self {
            state,
            #[cfg(debug_assertions)]
            layout_debug: crate::layout_debug::LayoutDebug::default(),
        }
    }
}

impl App for AlasApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        #[cfg(debug_assertions)]
        {
            self.layout_debug.handle_shortcuts(ctx);
            self.layout_debug.begin_frame(ctx);
        }
        self.state.poll_worker();
        self.state.screening.poll();
        self.state.uav.poll();
        if self.state.zoom_auto {
            let automatic_zoom = auto_zoom_factor(ctx);
            if (self.state.zoom - automatic_zoom).abs() > f32::EPSILON {
                self.state.zoom = automatic_zoom;
            }
        }
        handle_zoom_shortcuts(ctx, &mut self.state.zoom, &mut self.state.zoom_auto);
        if let Some(delay) = self.state.flush_parameter_feedback() {
            ctx.request_repaint_after(delay);
        }
        if self.state.is_running || self.state.screening.running || self.state.uav.is_running() {
            ctx.request_repaint();
        }
        // Automatic sizing follows the client area until a View > Zoom action
        // records an explicit user preference above the native display scale.
        // Reapplying an identical zoom asks egui-winit for another scale pass
        // even though the client rect has not changed. Avoiding that feedback
        // loop keeps a decorated, windowed application stable while Windows
        // rounds its client size by a pixel during a resize.
        if (ctx.zoom_factor() - self.state.zoom).abs() > f32::EPSILON {
            ctx.set_zoom_factor(self.state.zoom);
        }

        overlays::show_splash(&mut self.state, ctx);
        if self.state.boot_frames_remaining > 0 {
            ctx.request_repaint();
            return;
        }

        self.state.prepare_walkthrough_step();
        self.state.clear_walkthrough_targets();

        let menu_panel = TopBottomPanel::top("menu_bar").show(ctx, |ui| {
            menu::bar(ui, |ui| {
                self.render_file_menu(ui);
                self.render_view_menu(ctx, ui);
                self.render_help_menu(ui);
            });
        });
        self.state
            .record_walkthrough_target(TourTarget::MenuBar, menu_panel.response.rect);
        #[cfg(debug_assertions)]
        crate::layout_debug::record(
            ctx,
            "menu bar panel",
            menu_panel.response.rect,
            crate::layout_debug::RegionKind::Menu,
        );

        if self.state.nav_pinned {
            let nav_panel = SidePanel::left("nav_panel")
                .resizable(true)
                .default_width(layout::NAV_PANEL_WIDTH + layout::NAV_CONTENT_GAP)
                .width_range(
                    (layout::NAV_PANEL_MIN_WIDTH + layout::NAV_CONTENT_GAP)
                        ..=(layout::NAV_PANEL_MAX_WIDTH + layout::NAV_CONTENT_GAP),
                )
                .frame(
                    EguiFrame::side_top_panel(ctx.style().as_ref()).inner_margin(egui::Margin {
                        left: 10.0,
                        right: 10.0 + layout::NAV_CONTENT_GAP,
                        top: 12.0,
                        bottom: 12.0,
                    }),
                )
                .show(ctx, |ui| {
                    render_nav(&mut self.state, ui);
                });
            self.state
                .record_walkthrough_target(TourTarget::Navigation, nav_panel.response.rect);
            #[cfg(debug_assertions)]
            crate::layout_debug::record(
                ctx,
                "pinned navigation panel",
                nav_panel.response.rect,
                crate::layout_debug::RegionKind::Navigation,
            );
        }

        // Bottom panels are shown before the right-side preview so the run log
        // spans the full width. The control bar anchors to the very bottom
        // edge and the run log stacks above it.
        let control_panel = TopBottomPanel::bottom("control_bar").show(ctx, |ui| {
            crate::views::show_control_bar(&mut self.state, ui);
        });
        #[cfg(debug_assertions)]
        crate::layout_debug::record(
            ctx,
            "control bar panel",
            control_panel.response.rect,
            crate::layout_debug::RegionKind::Controls,
        );
        #[cfg(not(debug_assertions))]
        let _ = control_panel;
        let viewport_height = ctx.available_rect().height();
        let log_max_height = layout::run_log_max_height(viewport_height);
        let log_height = layout::run_log_height(viewport_height, self.state.run_log_height);
        let log_panel = TopBottomPanel::bottom("run_log")
            .resizable(true)
            .default_height(log_height)
            .height_range(layout::RUN_LOG_MIN_HEIGHT..=log_max_height)
            .frame(
                EguiFrame::side_top_panel(ctx.style().as_ref()).inner_margin(egui::Margin {
                    left: 10.0,
                    right: 10.0,
                    top: 12.0,
                    bottom: 12.0,
                }),
            )
            .show(ctx, |ui| {
                crate::views::show_run_log(&mut self.state, ui);
            });
        self.state.run_log_height = log_panel
            .response
            .rect
            .height()
            .clamp(layout::RUN_LOG_MIN_HEIGHT, log_max_height);
        self.state
            .record_walkthrough_target(TourTarget::RunLog, log_panel.response.rect);
        #[cfg(debug_assertions)]
        crate::layout_debug::record(
            ctx,
            "run log panel",
            log_panel.response.rect,
            crate::layout_debug::RegionKind::RunLog,
        );

        if self.state.preview_open {
            let preview_range = layout::preview_width_range(ctx.available_rect().width());
            let preview_panel = SidePanel::right("preview_panel")
                .resizable(true)
                .default_width(layout::PREVIEW_DOCK_DEFAULT_WIDTH)
                .width_range(preview_range)
                .show(ctx, |ui| {
                    crate::views::show_preview_dock(&mut self.state, ui);
                });
            self.state
                .record_walkthrough_target(TourTarget::PreviewDock, preview_panel.response.rect);
            #[cfg(debug_assertions)]
            crate::layout_debug::record(
                ctx,
                "preview dock panel",
                preview_panel.response.rect,
                crate::layout_debug::RegionKind::Preview,
            );
        }

        // The central panel is laid out before the unpinned rail so the rail
        // can float over it without reducing the content width. Pinned
        // navigation and the preview take the normal SidePanel path above and
        // therefore reserve real layout space for every form and result card.
        let body_rect = ctx.available_rect();
        #[cfg(debug_assertions)]
        crate::layout_debug::record(
            ctx,
            "central available rect",
            body_rect,
            crate::layout_debug::RegionKind::Available,
        );
        let content_panel = CentralPanel::default()
            .frame(
                EguiFrame::central_panel(ctx.style().as_ref()).inner_margin(egui::Margin {
                    left: 26.0,
                    right: 18.0,
                    top: 16.0,
                    bottom: 14.0,
                }),
            )
            .show(ctx, |ui| {
                route_page(&mut self.state, ui);
            });
        self.state
            .record_walkthrough_target(TourTarget::Content, content_panel.response.rect);

        if !self.state.nav_pinned {
            render_nav_rail(&mut self.state, ctx, body_rect);
        }

        overlays::show_walkthrough(&mut self.state, ctx);
        overlays::show_advanced_guide(&mut self.state, ctx);
        overlays::show_storage_dialog(&mut self.state, ctx);
        overlays::show_about(&mut self.state, ctx);
        self.show_detached_view_panel(ctx);
        #[cfg(debug_assertions)]
        self.layout_debug.finish_frame(ctx);
    }
}

fn render_nav(state: &mut AppState, ui: &mut Ui) {
    #[cfg(debug_assertions)]
    crate::layout_debug::record_ui(
        ui.ctx(),
        "navigation content",
        ui,
        crate::layout_debug::RegionKind::Navigation,
    );
    ui.set_min_width(ui.available_width());
    render_nav_contents(state, ui, true);
}

fn render_nav_rail(state: &mut AppState, ctx: &Context, body_rect: egui::Rect) {
    let pointer = ctx.pointer_hover_pos();
    let width = layout::expanded_navigation_width(
        (body_rect.width() - layout::NAV_CONTENT_GAP).max(layout::NAV_RAIL_WIDTH),
    );
    let pointer_x = pointer.filter(|p| body_rect.contains(*p)).map(|p| p.x);
    let hovered = nav_overlay_open_with_bounds(
        state.nav_hover_open,
        pointer_x,
        body_rect.left(),
        layout::NAV_RAIL_WIDTH,
        width,
    );
    state.nav_hover_open = hovered;
    // Hovering a navigation rail is a navigational affordance, not a content
    // transition. Opening it immediately removes the distracting resize
    // animation while retaining the compact rail when it is not needed.
    let expansion = if hovered { 1.0 } else { 0.0 };
    let panel_width = layout::NAV_RAIL_WIDTH + (width - layout::NAV_RAIL_WIDTH) * expansion;
    let panel_height = (body_rect.height() - 2.0 * layout::NAV_OVERLAY_MARGIN).max(1.0);
    let panel_pos = pos2(
        body_rect.left(),
        body_rect.top() + layout::NAV_OVERLAY_MARGIN,
    );

    let rail = Area::new(egui::Id::new("nav_rail"))
        .order(Order::Foreground)
        .fixed_pos(panel_pos)
        .show(ctx, |ui| {
            ui.allocate_ui_with_layout(
                vec2(panel_width, panel_height),
                Layout::top_down(Align::Min),
                |ui| {
                    let frame = navigation_overlay_frame(ui, expansion);
                    frame.show(ui, |ui| {
                        if expansion > 0.08 {
                            render_nav_contents(state, ui, false);
                        } else {
                            let response = ui
                                .allocate_rect(ui.max_rect(), Sense::click())
                                .on_hover_text(tr("Open navigation"));
                            if response.clicked() {
                                state.nav_pinned = true;
                            }
                        }
                    });
                },
            );
        });
    state.record_walkthrough_target(TourTarget::Navigation, rail.response.rect);
    #[cfg(debug_assertions)]
    crate::layout_debug::record(
        ctx,
        "navigation hover rail",
        rail.response.rect,
        crate::layout_debug::RegionKind::Rail,
    );
}

fn render_nav_contents(state: &mut AppState, ui: &mut Ui, pinned: bool) {
    ui.horizontal(|ui| {
        ui.label(
            RichText::new(tr("Navigation"))
                .strong()
                .color(ui.visuals().hyperlink_color),
        );
        let label = if pinned { tr("Unpin") } else { tr("Pin") };
        let hint = if pinned {
            tr("Collapse navigation to an 8 px hover rail")
        } else {
            tr("Keep navigation open and reserve its column")
        };
        if ui.small_button(label).on_hover_text(hint).clicked() {
            state.nav_pinned = !state.nav_pinned;
        }
    });
    ui.add_space(6.0);
    ScrollArea::vertical()
        .id_salt("navigation_scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for group in nav::NAV {
                card_frame(ui).show(ui, |ui| {
                    let group_width = ui.available_width();
                    ui.set_min_width(group_width);
                    ui.set_max_width(group_width);
                    ui.label(
                        RichText::new(tr(group.title))
                            .strong()
                            .color(ui.visuals().hyperlink_color),
                    );
                    for sub in group.subgroups {
                        if let Some(title) = sub.title {
                            ui.add_space(2.0);
                            ui.label(RichText::new(tr(title)).weak().small());
                        }
                        for page in sub.pages {
                            let selected = state.active_page == page.id;
                            let response = ui.selectable_label(
                                selected,
                                RichText::new(tr(page.title)).size(14.0),
                            );
                            if response.clicked() {
                                state.active_page = page.id.to_owned();
                                match page.kind {
                                    PageKind::Inputs | PageKind::Form => {
                                        state.update_preview_scene()
                                    }
                                    PageKind::Results => state.update_result_scene(),
                                    _ => {}
                                }
                            }
                        }
                    }
                });
                ui.add_space(4.0);
            }
        });
}

fn route_page(state: &mut AppState, ui: &mut Ui) {
    let Some(page) = nav::page(&state.active_page).cloned() else {
        ui.label(tr("Unknown page."));
        return;
    };
    match page.kind {
        PageKind::Inputs => crate::views::show_inputs_view(state, ui),
        PageKind::DesignSpace => crate::views::design_space_view::show_design_space_view(state, ui),
        PageKind::Results => crate::views::show_results_view(state, ui),
        PageKind::Setup => crate::views::show_tools_view(state, ui),
        PageKind::Analyses => crate::views::show_analyses_view(state, ui),
        PageKind::AirfoilScreening => crate::views::show_screening_view(state, ui),
        PageKind::Uav => crate::views::show_uav_view(state, ui),
        PageKind::Form => {
            ScrollArea::vertical()
                .id_salt(format!("advanced_page_scroll::{}", page.id))
                .auto_shrink([false, false])
                .show(ui, |ui| form_page::show_form_page(state, ui, &page));
        }
    }
}

impl AlasApp {
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
            let exports_enabled = self.state.pipeline_result.is_some();
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
            if ui.button(tr("Exit")).clicked() {
                std::process::exit(0);
            }
        });
    }

    fn render_view_menu(&mut self, ctx: &Context, ui: &mut Ui) {
        ui.menu_button(tr("View"), |ui| {
            ui.set_min_width(layout::MENU_MIN_WIDTH);
            let label = if self.state.show_view_panel {
                "Hide View panel"
            } else {
                "Detach View options"
            };
            if ui.button(tr(label)).clicked() {
                self.state.show_view_panel = !self.state.show_view_panel;
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
        let mut open = self.state.show_view_panel;
        Window::new(tr("View options"))
            .open(&mut open)
            .default_width(290.0)
            .resizable(true)
            .show(ctx, |ui| {
                render_view_options(&mut self.state, ctx, ui, false)
            });
        self.state.show_view_panel = open;
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
            if ui.button(tr("About ALAS")).clicked() {
                self.state.show_about = true;
                ui.close_menu();
            }
        });
    }
}

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
    use super::AlasApp;
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
            "Learn-more help",
            "Automatic zoom",
            "English",
            "Spanish",
            "Zoom in",
            "Zoom out",
            "Reset zoom (100%)",
            "Help",
            "Replay Walkthrough",
            "Advanced Walkthrough...",
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
