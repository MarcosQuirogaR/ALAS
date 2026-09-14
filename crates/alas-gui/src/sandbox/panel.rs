// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parameter access floating over the design space.
//!
//! There is no Parameter Panel: the geometry categories are separate
//! buttons floating down the left edge of the viewport, each of which opens
//! the Discipline Window of that category and isolates its component in the
//! preview, exactly as the window's own Open editor action does. The stack
//! (Search box and buttons) is centred about the viewport's horizontal
//! centreline, within the space left free between the camera row and the
//! bottom action block, so it never overlaps either. Above the buttons
//! sits the Search box; while it holds text, the matching fields are listed
//! in a transient results card with the shared editors, so a parameter can
//! be edited from a search hit without a permanent duplicate of every
//! editor. Clearing the search removes the card.

use egui::{pos2, vec2, Rect, RichText, ScrollArea, TextEdit, Ui};

use crate::state::AppState;
use crate::views::tr;

use super::editors::show_field;
use super::fields::{grouped, Discipline, SandboxField};
use super::viewport::{
    floating_button, register_overlay_rect, was_lit, OVERLAY_INSET, REST_OPACITY,
};

/// Width of the floating column of category buttons, in points.
pub const CATEGORY_COLUMN_WIDTH: f32 = 168.0;
/// Vertical spacing between category buttons, in points.
pub const CATEGORY_SPACING: f32 = 10.0;
/// Width of the search results card, in points.
const RESULTS_WIDTH: f32 = 400.0;

/// The matching fields of a search, grouped by discipline and group.
type Hits = Vec<(Discipline, Vec<(&'static str, Vec<SandboxField>)>)>;

/// Whether a field matches the search text by label, identifier, group or
/// discipline, in the current language.
pub fn matches(field: &SandboxField, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let needle = needle.to_ascii_lowercase();
    tr(&field.label).to_ascii_lowercase().contains(&needle)
        || field.id.to_ascii_lowercase().contains(&needle)
        || tr(field.group).to_ascii_lowercase().contains(&needle)
        || tr(field.discipline.title())
            .to_ascii_lowercase()
            .contains(&needle)
}

/// Open a discipline window and focus its component.
pub fn open_discipline_window(state: &mut AppState, discipline: Discipline) {
    let id = discipline.id().to_owned();
    if !state.sandbox.layout.open_disciplines.contains(&id) {
        state.sandbox.layout.open_disciplines.push(id);
    }
    state.set_sandbox_focus(Some(discipline));
}

/// The tag a category button registers its rectangle under.
fn category_tag(discipline: Discipline) -> &'static str {
    match discipline {
        Discipline::Wing => "category:wing",
        Discipline::HorizontalTail => "category:horizontal_tail",
        Discipline::VerticalTail => "category:vertical_tail",
        Discipline::Fuselage => "category:fuselage",
        Discipline::Propulsion => "category:propulsion",
    }
}

/// The top of the category stack: centred about the viewport's horizontal
/// centreline, clamped to the free band between `free_top` and
/// `free_bottom`; when the band is shorter than the stack, the stack starts
/// at the top of the band.
pub fn stack_top(viewport: Rect, free_top: f32, free_bottom: f32, height: f32) -> f32 {
    let centred = viewport.center().y - height * 0.5;
    centred.clamp(free_top, (free_bottom - height).max(free_top))
}

fn stack_height_id() -> egui::Id {
    egui::Id::new("sandbox_category_stack_height")
}

/// Render the Search box, the category buttons and, while a search is
/// active, the results card, all floating over `viewport` within the
/// vertical band `free_top..free_bottom` the other overlays leave free.
pub fn show_parameter_access(
    state: &mut AppState,
    ui: &mut Ui,
    viewport: Rect,
    free_top: f32,
    free_bottom: f32,
) {
    let width = CATEGORY_COLUMN_WIDTH.min((viewport.width() - 2.0 * OVERLAY_INSET).max(1.0));
    let ctx = ui.ctx().clone();
    let measured = ctx
        .data(|d| d.get_temp::<f32>(stack_height_id()))
        .unwrap_or(196.0);
    let top = stack_top(viewport, free_top, free_bottom, measured);
    let column = Rect::from_min_max(
        pos2(viewport.left() + OVERLAY_INSET, top),
        pos2(
            viewport.left() + OVERLAY_INSET + width,
            viewport.bottom() - OVERLAY_INSET,
        ),
    );
    let mut results_top = free_top;
    let stack = ui.allocate_new_ui(egui::UiBuilder::new().max_rect(column), |ui| {
        ui.spacing_mut().item_spacing.y = CATEGORY_SPACING;
        show_search_box(state, ui, width);
        results_top = results_top.min(ui.cursor().top());
        for discipline in Discipline::ALL {
            let button = egui::Button::new(tr(discipline.title())).min_size(vec2(width, 0.0));
            if floating_button(ui, category_tag(discipline), button)
                .on_hover_text(tr("Open this discipline in its own window."))
                .clicked()
            {
                open_discipline_window(state, discipline);
            }
        }
    });
    let used = stack.response.rect;
    register_overlay_rect(&ctx, "stack", used);
    let height = used.height().max(1.0);
    if (height - measured).abs() > 0.5 {
        ctx.data_mut(|d| d.insert_temp(stack_height_id(), height));
        ctx.request_repaint();
    }
    if !state.sandbox.search.trim().is_empty() {
        show_search_results(
            state,
            ui,
            viewport,
            column.right() + OVERLAY_INSET,
            results_top,
            free_bottom,
        );
    }
}

/// The Search box: its own box only, dimmed like the buttons until it is
/// hovered, focused or holds text.
fn show_search_box(state: &mut AppState, ui: &mut Ui, width: f32) {
    let id = egui::Id::new("sandbox_parameter_search");
    let lit = was_lit(ui.ctx(), id) || !state.sandbox.search.is_empty();
    let previous = ui.opacity();
    ui.set_opacity(if lit { 1.0 } else { REST_OPACITY });
    let response = ui.add(
        TextEdit::singleline(&mut state.sandbox.search)
            .id(id)
            .hint_text(tr("Search"))
            .desired_width(width - 8.0),
    );
    ui.set_opacity(previous);
    register_overlay_rect(ui.ctx(), "search", response.rect);
}

/// The matching fields, grouped by discipline and group, with the shared
/// editors, in a transient card beside the category buttons that spans the
/// free band, so the buttons stay usable while a search is active.
fn show_search_results(
    state: &mut AppState,
    ui: &mut Ui,
    viewport: Rect,
    left: f32,
    top: f32,
    bottom: f32,
) {
    let needle = state.sandbox.search.trim().to_owned();
    let fields = state.sandbox.fields.clone();
    let width = RESULTS_WIDTH.min((viewport.right() - OVERLAY_INSET - left).max(1.0));
    let card = Rect::from_min_max(pos2(left, top), pos2(left + width, bottom.max(top + 40.0)));
    let hits: Hits = Discipline::ALL
        .into_iter()
        .map(|discipline| {
            let groups: Vec<(&'static str, Vec<SandboxField>)> = grouped(&fields, discipline)
                .into_iter()
                .map(|(title, members)| {
                    (
                        title,
                        members
                            .into_iter()
                            .filter(|f| matches(f, &needle))
                            .cloned()
                            .collect::<Vec<_>>(),
                    )
                })
                .filter(|(_, members)| !members.is_empty())
                .collect();
            (discipline, groups)
        })
        .filter(|(_, groups)| !groups.is_empty())
        .collect();
    let response = ui.allocate_new_ui(egui::UiBuilder::new().max_rect(card), |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.set_width(width - 2.0 * ui.spacing().window_margin.left);
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Matching parameters")).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(tr("Clear")).small())
                        .on_hover_text(tr("Clear the search and hide the matches."))
                        .clicked()
                    {
                        state.sandbox.search.clear();
                    }
                });
            });
            if hits.is_empty() {
                ui.label(RichText::new(tr("No parameter matches the search.")).weak());
                return;
            }
            ScrollArea::vertical()
                .id_salt("sandbox_search_results")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    for (discipline, groups) in &hits {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(tr(discipline.title()))
                                    .strong()
                                    .color(ui.visuals().hyperlink_color),
                            );
                            if ui
                                .add(egui::Button::new(tr("Open editor")).small())
                                .on_hover_text(tr("Open this discipline in its own window."))
                                .clicked()
                            {
                                open_discipline_window(state, *discipline);
                            }
                        });
                        for (title, members) in groups {
                            ui.label(RichText::new(tr(title)).weak().small());
                            for field in members {
                                show_field(state, ui, field);
                                ui.add_space(3.0);
                            }
                        }
                        ui.add_space(4.0);
                    }
                });
        });
    });
    register_overlay_rect(ui.ctx(), "results", response.response.rect);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_matches_label_identifier_group_and_discipline_case_insensitively() {
        let state = {
            let mut state = AppState::default();
            assert!(state.enter_sandbox(true));
            state
        };
        let span = state
            .sandbox
            .fields
            .iter()
            .find(|f| f.id == "design.span_m")
            .expect("span field");
        assert!(matches(span, ""));
        assert!(matches(span, "SPAN"));
        assert!(matches(span, "span_m"));
        assert!(matches(span, "planform"));
        assert!(matches(span, "wing"));
        assert!(!matches(span, "nacelle"));
    }

    #[test]
    fn the_stack_centres_on_the_viewport_and_stays_inside_the_free_band() {
        let viewport = Rect::from_min_max(pos2(0.0, 100.0), pos2(1000.0, 700.0));
        // Tall viewport: centred about y = 400.
        assert!((stack_top(viewport, 150.0, 600.0, 200.0) - 300.0).abs() < 1e-6);
        // The bottom block pushes it up when centring would overlap it.
        assert!((stack_top(viewport, 150.0, 450.0, 200.0) - 250.0).abs() < 1e-6);
        // The camera row pushes it down on a short viewport.
        let short = Rect::from_min_max(pos2(0.0, 100.0), pos2(1000.0, 360.0));
        assert!((stack_top(short, 150.0, 300.0, 200.0) - 150.0).abs() < 1e-6);
    }

    #[test]
    fn every_category_button_has_a_distinct_tag() {
        let tags: std::collections::BTreeSet<&str> =
            Discipline::ALL.into_iter().map(category_tag).collect();
        assert_eq!(tags.len(), Discipline::ALL.len());
    }
}
