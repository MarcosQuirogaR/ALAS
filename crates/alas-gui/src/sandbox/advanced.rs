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

use egui::{Context, Id, RichText, ScrollArea, Window};

use crate::nav::{self, PageKind};
use crate::state::AppState;
use crate::views::{form_page, tr};

/// The pages the window offers, in tab order.
fn pages() -> Vec<&'static nav::Page> {
    nav::NAV
        .iter()
        .filter(|group| group.title == "Advanced Settings")
        .flat_map(|group| group.subgroups.iter())
        .flat_map(|subgroup| subgroup.pages.iter())
        .filter(|page| page.kind == PageKind::Form)
        .collect()
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
    let mut open = true;
    Window::new(tr("Advanced Settings"))
        .id(Id::new("advanced_settings_window"))
        .open(&mut open)
        .resizable(true)
        .default_size(egui::vec2(760.0, 560.0))
        .show(ctx, |ui| {
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
                    .add(crate::theme::selectable_button(tr("Run options"), run_selected))
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
                    let locked = state.manual_geometry_locked()
                        && matches!(page.group, Some("geometry") | Some("control_surfaces"));
                    if locked {
                        ui.label(
                            RichText::new(tr(
                                "Preset geometry is protected from manual edits here as well; open the sandbox for geometry experiments.",
                            ))
                            .color(ui.visuals().warn_fg_color)
                            .small(),
                        );
                    }
                    ui.add_enabled_ui(!locked, |ui| {
                        form_page::show_form_page(state, ui, page);
                    });
                });
        });
    state.sandbox.layout.advanced_settings_open = open;
}

/// The top-bar action that opens the window.
pub fn show_menu_action(state: &mut AppState, ui: &mut egui::Ui) {
    if ui.button(tr("Advanced Settings...")).clicked() {
        state.sandbox.layout.advanced_settings_open = true;
    }
}
