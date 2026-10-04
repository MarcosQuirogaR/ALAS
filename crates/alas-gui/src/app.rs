// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Main `eframe::App` desktop application shell.
//!
//! The shell layout: a File/View/Help menu bar, a left navigation tree, an
//! optional right-hand 3D preview dock, a run log and control bar along the
//! bottom, and a central content pane that routes to the active
//! [`crate::nav::Page`].

mod menus;
mod nav_panel;
#[cfg(test)]
mod tests;

use eframe::{App, Frame};
use egui::{menu, CentralPanel, Context, Frame as EguiFrame, SidePanel, TopBottomPanel};

use self::nav_panel::{render_nav, render_nav_rail, route_page};
use crate::layout;
use crate::state::{AppState, LogKind};
use crate::view_controls::{auto_zoom_factor, handle_zoom_shortcuts};
use crate::views::tour_data::TourTarget;
use crate::views::{notices, overlays};

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
        let mut state = AppState::default();
        // The desktop shell owns the user-data side effects. `AppState`
        // itself stays hermetic so tests and embedders are not affected by
        // whatever this machine's installation has stored.
        state.load_persisted_custom_airports();
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

    /// The shell's state, for examples and UI audits that drive navigation
    /// between frames.
    #[doc(hidden)]
    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }
}

impl App for AlasApp {
    fn update(&mut self, ctx: &Context, _frame: &mut Frame) {
        #[cfg(debug_assertions)]
        {
            self.layout_debug.handle_shortcuts(ctx);
            self.layout_debug.begin_frame(ctx);
        }
        self.state.poll_navdata_download();
        self.state.poll_openvsp_runtime_setup();
        self.state.poll_worker();
        self.state.screening.poll();
        for event in self.state.cfd.poll() {
            let level = match event.severity {
                alas_cfd::CfdEventSeverity::Info => LogKind::Info,
                alas_cfd::CfdEventSeverity::Warning => LogKind::Warn,
                alas_cfd::CfdEventSeverity::Error => LogKind::Error,
            };
            self.state.log(
                format!("CFD {}: {}", event.stage.as_str(), event.message),
                level,
            );
        }
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
        if self.state.cfd.running || self.state.cfd.probing {
            // CFD events and provisional solver output arrive through a
            // detached worker. Ten updates per second keep the log/results
            // responsive during a multi-minute solve without spinning the UI
            // at display refresh rate.
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.state.is_running
            || self.state.screening.running
            || self.state.uav.is_running()
            || self.state.navdata_download_in_progress
        {
            // No egui::Context reaches these mpsc-backed workers; poll at
            // the same 10 Hz cadence as the CFD/installer paths below.
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.state.openvsp_runtime_setup.running {
            // The installer streams stage lines from a subprocess; a modest
            // repaint cadence keeps the stage text current without spinning
            // the UI at display refresh rate for a multi-minute download.
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
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

        if crate::sandbox::workspace::show_if_active(&mut self.state, ctx) {
            // Standalone analyses remain owned by the shared AppState even
            // while the full-window Sandbox is active.  Keep their native
            // viewport dispatch on this fast path so a CFD study opened from
            // the guided workspace does not disappear when the user enters
            // Sandbox (and reappears when they leave it).
            crate::views::cfd_view::show_cfd_window(&mut self.state, ctx);
            crate::views::screening_window::show_screening_window(&mut self.state, ctx);
            crate::views::screening_window::show_custom_airfoil_import_window(&mut self.state, ctx);
            crate::views::wing_analysis_view::show_wing_analysis_window(&mut self.state, ctx);
            crate::views::airport_window::show_custom_airport_window(&mut self.state, ctx);
            crate::views::mission_profile_inputs::show_mission_profile_window(&mut self.state, ctx);
            crate::views::tool_intro::show_tool_intro(&mut self.state, ctx);
            return self.show_detached_view_panel(ctx);
        }
        self.state.prepare_walkthrough_step();
        self.state.clear_walkthrough_targets();

        let menu_panel = TopBottomPanel::top("menu_bar")
            .frame(
                EguiFrame::side_top_panel(ctx.style().as_ref()).inner_margin(egui::Margin {
                    left: 8.0,
                    right: 8.0,
                    top: layout::MENU_BAR_VERTICAL_INSET,
                    bottom: layout::MENU_BAR_VERTICAL_INSET,
                }),
            )
            .show(ctx, |ui| {
                menu::bar(ui, |ui| {
                    if let Some(image) = crate::branding::text_logo_image(ctx) {
                        ui.add(image.max_width(132.0).max_height(20.0));
                    }
                    ui.separator();
                    self.render_file_menu(ui);
                    self.render_view_menu(ctx, ui);
                    self.render_analysis_menu(ui);
                    crate::sandbox::advanced::show_menu_action(&mut self.state, ui);
                    crate::views::tool_intro::show_menu_action(&mut self.state, ui);
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
        if self.state.run_log_open {
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
        }

        let dock = layout::preview_width_range(ctx.available_rect().width());
        let dock_hidden = self.state.preview_open && dock.is_none();
        if let Some(preview_range) = dock.filter(|_| self.state.preview_open) {
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
                notices::show_preview_suppressed(ui, dock_hidden);
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
        crate::views::tool_intro::show_tool_intro(&mut self.state, ctx);
        crate::sandbox::advanced::show_advanced_settings_window(&mut self.state, ctx);
        crate::views::cfd_view::show_cfd_window(&mut self.state, ctx);
        crate::views::screening_window::show_screening_window(&mut self.state, ctx);
        crate::views::screening_window::show_custom_airfoil_import_window(&mut self.state, ctx);
        crate::views::airport_window::show_custom_airport_window(&mut self.state, ctx);
        crate::views::mission_profile_inputs::show_mission_profile_window(&mut self.state, ctx);
        self.show_detached_view_panel(ctx);
        #[cfg(debug_assertions)]
        self.layout_debug.finish_frame(ctx);
    }
}
