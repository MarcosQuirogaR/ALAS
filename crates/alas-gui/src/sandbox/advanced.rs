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

use egui::{vec2, Context, ScrollArea, ViewportBuilder, ViewportCommand};

use crate::native_viewport::{show_native_viewport, viewport_id};
use crate::nav::{self, PageKind};
use crate::state::AppState;
use crate::views::{form_page, tr};

#[cfg(test)]
#[path = "advanced_tests.rs"]
mod tests;

/// The pages the window offers, in tab order: every Advanced Settings tab
/// (discipline forms and External Tools) plus Run options. Airfoil Screening
/// is not a tab; it opens from the top bar's Analysis menu.
pub fn pages() -> Vec<&'static nav::Page> {
    nav::ADVANCED_SETTINGS_PAGES.iter().collect()
}

/// Render the window when it is open.
pub fn show_advanced_settings_window(state: &mut AppState, ctx: &Context) {
    if !state.sandbox.layout.advanced_settings_open {
        return;
    }
    let pages = pages();
    // A session saved while a since-removed tab (Airfoil Screening) was
    // active falls back to the first tab instead of an empty page.
    if state.sandbox.advanced_tab != "run_options"
        && !pages
            .iter()
            .any(|page| page.id == state.sandbox.advanced_tab)
    {
        state.sandbox.advanced_tab.clear();
    }
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
            show_advanced_settings_contents(state, ui);
        },
    );
    if response.close_requested {
        state.sandbox.layout.advanced_settings_open = false;
    }
}

/// Rows the wrapped tab strip may take before it becomes a single paged row.
const MAX_WRAPPED_TAB_ROWS: usize = 2;
/// The paged strip's arrows: single guillemets.
const PREVIOUS: &str = "\u{2039}";
const NEXT: &str = "\u{203a}";

/// Rows a left-to-right wrap of `widths` takes in `available` points.
pub(crate) fn wrapped_rows(widths: &[f32], spacing: f32, available: f32) -> usize {
    let mut rows = 1;
    let mut x = 0.0;
    for (index, width) in widths.iter().enumerate() {
        let start = if index == 0 || x == 0.0 {
            0.0
        } else {
            x + spacing
        };
        if start > 0.0 && start + width > available {
            rows += 1;
            x = *width;
        } else {
            x = start + width;
        }
    }
    rows
}

/// The first and one-past-last tab of the single-row strip: the selected tab
/// stays visible, the run starts at the remembered first tab when it can, and
/// as many whole tabs as fit follow it.
pub(crate) fn paged_window(
    widths: &[f32],
    spacing: f32,
    available: f32,
    first: usize,
    selected: usize,
) -> (usize, usize) {
    let count = widths.len();
    if count == 0 {
        return (0, 0);
    }
    let selected = selected.min(count - 1);
    let span = |start: usize, end: usize| {
        widths[start..end].iter().sum::<f32>() + spacing * (end - start).saturating_sub(1) as f32
    };
    let mut start = first.min(selected);
    while start < selected && span(start, selected + 1) > available {
        start += 1;
    }
    let mut end = start + 1;
    while end < count && span(start, end + 1) <= available {
        end += 1;
    }
    // Use free space at the end of the list to show earlier tabs.
    while start > 0 && span(start - 1, end) <= available {
        start -= 1;
    }
    (start, end)
}

/// The tab strip and scrollable active page shared by the native window.
///
/// The tabs wrap while they fit in [`MAX_WRAPPED_TAB_ROWS`] rows. A narrower
/// window shows one row of whole tabs between previous/next arrows, so the
/// strip never takes more than one row of a short window and no tab label is
/// cut.
#[doc(hidden)]
pub fn show_advanced_settings_contents(state: &mut AppState, ui: &mut egui::Ui) {
    let pages = pages();
    let mut tabs: Vec<(&str, String)> =
        pages.iter().map(|page| (page.id, tr(page.title))).collect();
    tabs.push(("run_options", tr("Run options")));
    let font = egui::TextStyle::Button.resolve(ui.style());
    let padding = ui.spacing().button_padding.x;
    let widths: Vec<f32> = tabs
        .iter()
        .map(|(_, title)| {
            ui.fonts(|fonts| {
                fonts
                    .layout_no_wrap(title.clone(), font.clone(), egui::Color32::WHITE)
                    .size()
                    .x
            }) + 2.0 * padding
        })
        .collect();
    let spacing = ui.spacing().item_spacing.x;
    let available = ui.available_width();
    // An unknown tab highlights nothing; the paged strip then starts at its
    // remembered first tab and "next" selects the first tab.
    let selected = tabs
        .iter()
        .position(|(id, _)| *id == state.sandbox.advanced_tab);
    let mut chosen = None;
    let tab_button = |ui: &mut egui::Ui, index: usize, chosen: &mut Option<usize>| {
        if ui
            .add(crate::theme::selectable_button(
                tabs[index].1.clone(),
                Some(index) == selected,
            ))
            .clicked()
        {
            *chosen = Some(index);
        }
    };
    if wrapped_rows(&widths, spacing, available) <= MAX_WRAPPED_TAB_ROWS {
        ui.horizontal_wrapped(|ui| {
            for index in 0..tabs.len() {
                tab_button(ui, index, &mut chosen);
            }
        });
    } else {
        // The arrows step the selection; the row follows it, starting where
        // it started last frame so a click does not reshuffle the tabs.
        let first_id = egui::Id::new("advanced_settings_first_tab");
        let first = ui
            .ctx()
            .data(|data| data.get_temp::<usize>(first_id))
            .unwrap_or(0);
        let arrow_width = ui.fonts(|fonts| {
            fonts
                .layout_no_wrap(PREVIOUS.to_owned(), font.clone(), egui::Color32::WHITE)
                .size()
                .x
        }) + 2.0 * padding;
        let room = available - 2.0 * (arrow_width + spacing);
        let anchor = selected.unwrap_or(first);
        let (start, end) = paged_window(&widths, spacing, room, first, anchor);
        ui.ctx().data_mut(|data| data.insert_temp(first_id, start));
        ui.horizontal(|ui| {
            if ui
                .add_enabled(selected.is_some_and(|s| s > 0), egui::Button::new(PREVIOUS))
                .on_hover_text(tr("Previous tab"))
                .clicked()
            {
                chosen = selected.map(|s| s.saturating_sub(1));
            }
            for index in start..end {
                tab_button(ui, index, &mut chosen);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let next = selected.map_or(0, |s| s + 1);
                if ui
                    .add_enabled(next < tabs.len(), egui::Button::new(NEXT))
                    .on_hover_text(tr("Next tab"))
                    .clicked()
                {
                    chosen = Some(next);
                }
            });
        });
    }
    if let Some(index) = chosen {
        state.sandbox.advanced_tab = tabs[index].0.to_owned();
    }
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
            if page.kind == PageKind::Setup {
                crate::views::show_tools_view(state, ui);
                return;
            }
            let locked = state.manual_geometry_locked()
                && matches!(page.group, Some("geometry") | Some("control_surfaces"));
            // The page draws its own lock notice under its title and
            // disables only its editors, so a protected page keeps its
            // heading, description and field labels readable.
            form_page::show_form_page_locked(state, ui, page, locked);
        });
}

/// The top-bar action that opens the window.
pub fn show_menu_action(state: &mut AppState, ui: &mut egui::Ui) {
    if ui.button(tr("Advanced Settings")).clicked() {
        if state.sandbox.layout.advanced_settings_open {
            // The action is also a raise/focus command when the native window
            // already exists behind the main ALAS window.
            ui.ctx()
                .send_viewport_cmd_to(viewport_id("advanced_settings"), ViewportCommand::Focus);
        }
        state.sandbox.layout.advanced_settings_open = true;
    }
}
