// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The detached Advanced Settings window.
//!
//! One window, organized by discipline tabs, shared by the guided workspace
//! and the sandbox. Every tab renders the same schema-driven form page the
//! navigation tree used, editing the one authoritative configuration, so an
//! edit here invalidates sandbox estimates exactly like a Parameter Panel
//! edit. A registered preset's geometry stays read-only here as everywhere
//! else in the guided workspace.

use egui::{vec2, Context, ScrollArea, ViewportBuilder};

use crate::native_viewport::show_native_viewport;
use crate::nav::{self, PageKind};
use crate::state::AppState;
use crate::views::{form_page, tr};

/// The pages the window offers, in tab order: every Advanced Settings tab
/// (discipline forms, Airfoil Screening and External Tools) plus Run options.
pub fn pages() -> Vec<&'static nav::Page> {
    nav::ADVANCED_SETTINGS_PAGES.iter().collect()
}

/// Render the window when it is open.
pub fn show_advanced_settings_window(state: &mut AppState, ctx: &Context) {
    if !state.sandbox.layout.advanced_settings_open {
        return;
    }
    let pages = pages();
    if state.sandbox.advanced_tab.is_empty() {
        if let Some(first) = pages.first() {
            state.sandbox.advanced_tab = first.id.to_owned();
        }
    }
    let response = show_native_viewport(
        ctx,
        "advanced_settings",
        tr("Advanced Settings"),
        ViewportBuilder::default()
            .with_title(tr("Advanced Settings"))
            .with_inner_size(vec2(760.0, 560.0))
            .with_min_inner_size(vec2(560.0, 380.0))
            .with_resizable(true),
        |_child_ctx, ui, _class| {
            ui.horizontal_wrapped(|ui| {
                for page in &pages {
                    let selected = state.sandbox.advanced_tab == page.id;
                    if ui
                        .add(crate::theme::selectable_button(tr(page.title), selected))
                        .clicked()
                    {
                        state.sandbox.advanced_tab = page.id.to_owned();
                    }
                }
                let run_selected = state.sandbox.advanced_tab == "run_options";
                if ui
                    .add(crate::theme::selectable_button(
                        tr("Run options"),
                        run_selected,
                    ))
                    .clicked()
                {
                    state.sandbox.advanced_tab = "run_options".to_owned();
                }
            });
            ui.separator();
            let active = state.sandbox.advanced_tab.clone();
            ScrollArea::vertical()
                .id_salt("advanced_settings_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if active == "run_options" {
                        crate::views::inputs_view::show_run_evaluation_options(state, ui);
                        return;
                    }
                    let Some(page) = pages.iter().find(|p| p.id == active).cloned() else {
                        return;
                    };
                    match page.kind {
                        PageKind::Setup => {
                            crate::views::show_tools_view(state, ui);
                            return;
                        }
                        PageKind::AirfoilScreening => {
                            if state.screening.window_open {
                                ui.label(tr("Airfoil Screening is open in its own window."));
                            } else {
                                crate::views::show_screening_view(state, ui);
                            }
                            return;
                        }
                        _ => {}
                    }
                    let locked = state.manual_geometry_locked()
                        && matches!(page.group, Some("geometry") | Some("control_surfaces"));
                    // The page draws its own lock notice under its title and
                    // disables only its editors, so a protected page keeps its
                    // heading, description and field labels readable.
                    form_page::show_form_page_locked(state, ui, page, locked);
                });
        },
    );
    if response.close_requested {
        state.sandbox.layout.advanced_settings_open = false;
    }
}

/// The top-bar action that opens the window.
pub fn show_menu_action(state: &mut AppState, ui: &mut egui::Ui) {
    if ui.button(tr("Advanced Settings")).clicked() {
        state.sandbox.layout.advanced_settings_open = true;
    }
}
