// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Design-flow pages: Aircraft, Mission, Payload and Analyses share one
//! frame (heading, scrolling body, Advanced sections at the Advanced detail
//! level). Mission and Payload keep their current cards here; the stage and
//! payload editors extend [`show_mission_body`] and [`show_payload_body`].

use egui::{RichText, ScrollArea, Ui};

use crate::detail::Detail;
use crate::nav::{self, AdvancedSection, Page, PageKind, Surface};
use crate::state::AppState;
use crate::views::form_page::DRAG_MODEL_GROUP;
use crate::views::{form_page, tr_label};

/// Render a flow page: its body, then its Advanced sections.
pub fn show_flow_page(state: &mut AppState, ui: &mut Ui, page: &Page) {
    ScrollArea::vertical()
        .id_salt(format!("flow_page_scroll::{}", page.id))
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            match page.kind {
                PageKind::Inputs => crate::views::inputs_view::show_aircraft_body(state, ui),
                PageKind::Mission => show_mission_body(state, ui),
                PageKind::Payload => show_payload_body(state, ui),
                PageKind::Analyses => crate::views::analyses_view::show_analyses_body(state, ui),
                _ => {}
            }
            show_advanced_sections(state, ui, page);
            crate::search::flush_scroll(ui);
        });
    form_page::handle_airfoil_import_request(state, ui.ctx());
}

/// The Mission step's body. The mission stage editor
/// attaches here: it replaces the profile card with the ordered stage list.
fn show_mission_body(state: &mut AppState, ui: &mut Ui) {
    ui.heading(tr_label("Mission"));
    ui.add_space(4.0);
    crate::views::mission_stage_editor::show(state, ui);
}

/// The Payload step's body. The payload zone and point-mass editor on the
/// generated side view attaches here.
fn show_payload_body(state: &mut AppState, ui: &mut Ui) {
    ui.heading(tr_label("Payload"));
    ui.add_space(4.0);
    crate::views::payload_editor::show(state, ui);
}

/// Whether a hosted form page has rows at the Basic level: its surface is not
/// the Advanced one and its group is not marked advanced in the schema.
fn shown_in_basic(state: &AppState, form: &Page) -> bool {
    form_shown_at_basic(&state.schema, form)
}

/// [`shown_in_basic`] from the schema alone, for the search index.
pub(crate) fn form_shown_at_basic(schema: &alas_config::Node, form: &Page) -> bool {
    form.surface != Surface::Advanced
        && !form
            .group
            .and_then(|group| schema.field(group))
            .is_some_and(|field| field.advanced)
}

/// Whether the form fields of `form` (not the cards its page adds below them)
/// are drawn at Basic: the drag-model cards are Advanced only.
pub(crate) fn form_rows_drawn_at_basic(schema: &alas_config::Node, form: &Page) -> bool {
    form_shown_at_basic(schema, form) && form.group != Some(DRAG_MODEL_GROUP)
}

/// Leaves under `fields`, counting only those the schema marks advanced (or
/// all of them when `inherited`).
fn leaves(fields: &[alas_config::Field], only_advanced: bool, inherited: bool) -> usize {
    fields
        .iter()
        .map(|field| {
            let advanced = inherited || field.advanced;
            match &field.entry {
                alas_config::Entry::Node(node) => leaves(&node.fields, only_advanced, advanced),
                alas_config::Entry::Leaf(_) => usize::from(advanced || !only_advanced),
            }
        })
        .sum()
}

/// Rows of `section` that the Basic level hides.
fn hidden_in_basic(state: &AppState, section: &AdvancedSection) -> usize {
    section
        .pages
        .iter()
        .filter_map(|id| nav::page(id))
        .filter_map(|form| {
            let group = form.group?;
            let alas_config::Entry::Node(node) = &state.schema.field(group)?.entry else {
                return None;
            };
            let fields =
                crate::views::form_page::placement::cached_visible_fields(state, form, group)?;
            let _ = node;
            // The drag-model cards are Advanced only: at Basic all of them hide.
            let basic = shown_in_basic(state, form) && group != DRAG_MODEL_GROUP;
            Some(leaves(&fields, basic, false))
        })
        .sum()
}

/// How many sections of `page` have routine rows, and how many rows the Basic
/// level hides across all of them.
pub(crate) fn basic_summary(state: &AppState, page: &Page) -> (usize, usize) {
    let mut sections = 0;
    let mut hidden = 0;
    for section in page.advanced {
        let routine = section
            .pages
            .iter()
            .filter_map(|id| nav::page(id))
            .any(|form| shown_in_basic(state, form));
        sections += usize::from(routine);
        hidden += hidden_in_basic(state, section);
    }
    (sections, hidden)
}

/// The model sections of `page`: at the Basic level the pages that have
/// routine rows, at the Advanced level every page. Section headers carry no
/// counts; a Basic page with nothing routine shows one weak hint.
fn show_advanced_sections(state: &mut AppState, ui: &mut Ui, page: &Page) {
    if page.advanced.is_empty() {
        return;
    }
    let advanced_level = state.shell.detail == Detail::Advanced;
    let (basic_sections, hidden_total) = basic_summary(state, page);
    ui.add_space(10.0);
    if !advanced_level && basic_sections == 0 {
        // Nothing routine to show: one weak hint that Advanced adds more.
        if hidden_total > 0 {
            ui.label(RichText::new(tr_label("More settings in Advanced view")).weak());
        }
        return;
    }
    // The same group label as "Core (every run)" and the section names below.
    crate::views::focus_card::group_heading(ui, "Settings");
    for section in page.advanced {
        let visible_pages: Vec<&Page> = section
            .pages
            .iter()
            .filter_map(|id| nav::page(id))
            .filter(|form| advanced_level || shown_in_basic(state, form))
            .collect();
        if visible_pages.is_empty() {
            continue;
        }
        let title = tr_label(section.title);
        let forced = state.active_page == page.id
            && state
                .shell
                .search
                .focus
                .as_ref()
                .is_some_and(|focus| focus.section == Some(section.title));
        let id = egui::Id::new(("advanced_section", page.id, section.title));
        let _ = crate::views::focus_card::card_with(ui, id, &title, false, forced, |ui| {
            for form in visible_pages {
                let locked = state.manual_geometry_locked()
                    && matches!(form.group, Some("geometry") | Some("control_surfaces"));
                form_page::show_form_page_embedded(state, ui, form, locked);
                ui.add_space(8.0);
            }
            if page.id == "setup_analyses" && section.title == "Analysis Fidelity" {
                crate::views::inputs_view::show_run_evaluation_options(state, ui);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_counts_what_advanced_adds_and_never_shows_an_empty_heading() {
        let state = AppState::default();
        // Mission has only advanced-level sections: nothing routine, a count.
        let mission = nav::page("mission").unwrap();
        let (sections, hidden) = basic_summary(&state, mission);
        assert_eq!(sections, 0);
        assert!(hidden > 0, "the Advanced sections of Mission hold rows");
        // Aircraft has routine model sections and a larger hidden count.
        let aircraft = nav::page("inputs").unwrap();
        let (sections, hidden) = basic_summary(&state, aircraft);
        assert!(sections >= 1 && hidden > 0);
    }

    fn painted(detail: Detail, page: &str) -> Vec<String> {
        fn collect(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| collect(s, out)),
                egui::Shape::Text(text) => out.push(text.galley.text().to_owned()),
                _ => {}
            }
        }
        let mut state = AppState::default();
        state.shell.detail = detail;
        let page = nav::page(page).unwrap();
        let ctx = egui::Context::default();
        let mut texts = Vec::new();
        for _ in 0..2 {
            crate::detail::publish(&ctx, detail);
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1_000.0, 1_600.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .show(ctx, |ui| show_advanced_sections(&mut state, ui, page));
                },
            );
            texts.clear();
            for clipped in &output.shapes {
                collect(&clipped.shape, &mut texts);
            }
        }
        texts
    }

    #[test]
    fn the_basic_mission_page_has_a_hint_and_no_counts_or_bare_settings_heading() {
        let texts = painted(Detail::Basic, "mission");
        assert!(texts.iter().all(|t| t != "Settings"), "{texts:?}");
        assert!(
            texts
                .iter()
                .all(|t| !t.to_lowercase().contains("advanced hidden")),
            "{texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|t| t.starts_with("More settings in advanced")),
            "{texts:?}"
        );
        let advanced = painted(Detail::Advanced, "mission");
        assert!(advanced.iter().any(|t| t == "Settings"));
    }
}
