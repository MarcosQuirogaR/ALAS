// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The global Ctrl+K palette, scoped to the Sandbox parameters.
//!
//! While the Sandbox is open the palette searches the Sandbox fields (label,
//! identifier, unit, help, group and discipline, in both languages) instead
//! of the guided pages' settings. Choosing a result opens the Discipline
//! Window that holds the field and starts the jump highlight on it: the
//! editor scrolls the field into view and pulses it for one second through
//! `search::focus_anchor`, and the group that holds it opens
//! (`search::focus_wants`).

use crate::search::{Focus, SearchEntry};
use crate::state::AppState;
use crate::views::tr;

use super::fields::{Discipline, SandboxField};
use super::panel::open_discipline_window;

/// The id prefix of an entry that names a Discipline Window itself, not a field.
const DISCIPLINE_PREFIX: &str = "discipline:";

/// Sandbox fields that set the wing's dihedral (the vertical offsets of its
/// stations), found by the word "dihedral" ahead of the other fields of the
/// "Twist and dihedral" group, as the main palette ranks the same offsets.
const DIHEDRAL_FIELDS: &[&str] = &[
    "geometry.wing.root_z_m",
    "geometry.wing.break_z_m",
    "geometry.wing.tip_z_m",
    "geometry.wing.flight_tip_rise_semispan_fraction",
];
/// Words a user thinks in for those fields.
const DIHEDRAL_SYNONYMS: &str = "dihedral anhedral vertical offset tip rise";

/// Glossary concepts (see `search_concepts`) that the Sandbox answers with a
/// field of its own: the wing's position against the centre of gravity is the
/// balance lever the Sandbox has.
const CONCEPT_TARGETS: &[(&str, &str)] = &[
    ("CG gate", "design.wing_x_shift_m"),
    ("Load & trim sheet", "design.wing_x_shift_m"),
];

/// One palette entry per Discipline Window, then one per Sandbox field, then
/// the glossary concepts the Sandbox can answer. The location column of a
/// field reads "Discipline > Group".
pub fn build_index(fields: &[SandboxField]) -> Vec<SearchEntry> {
    let windows = Discipline::ALL.into_iter().map(|discipline| {
        let title = discipline.title();
        SearchEntry::scoped(
            title,
            "",
            title,
            None,
            &format!("{DISCIPLINE_PREFIX}{}", discipline.id()),
            &format!("{title} {} {}", tr(title), discipline.id()),
        )
    });
    windows
        .chain(fields.iter().map(|field| {
            let haystack = format!(
                "{} {} {} {} {} {} {} {} {} {}",
                field.label,
                tr(&field.label),
                field.id,
                field.id.replace(['.', '_'], " "),
                field.unit,
                tr(&field.help),
                field.group,
                tr(field.group),
                field.discipline.title(),
                tr(field.discipline.title()),
            );
            let entry = SearchEntry::scoped(
                &field.label,
                &field.unit,
                field.discipline.title(),
                Some(field.group),
                &field.id,
                &haystack,
            );
            if DIHEDRAL_FIELDS.contains(&field.id.as_str()) {
                entry.with_summary(DIHEDRAL_SYNONYMS)
            } else {
                entry
            }
        }))
        .chain(concept_entries(fields))
        .collect()
}

/// The glossary concepts of `CONCEPT_TARGETS`, each pointing at its Sandbox
/// field and matched through the glossary's own synonyms.
fn concept_entries(fields: &[SandboxField]) -> Vec<SearchEntry> {
    CONCEPT_TARGETS
        .iter()
        .filter_map(|(title, id)| {
            let concept = crate::search_concepts::CONCEPTS
                .iter()
                .find(|concept| concept.title == *title)?;
            let field = fields.iter().find(|field| field.id == *id)?;
            let haystack = format!("{title} {} {}", tr(title), concept.synonyms);
            Some(SearchEntry::scoped(
                title,
                "",
                field.discipline.title(),
                Some(field.group),
                id,
                &haystack,
            ))
        })
        .collect()
}

/// Apply a chosen result: open the field's Discipline Window and start the
/// highlight. The Sandbox stays where it is; nothing navigates away.
pub fn jump(state: &mut AppState, entry: &SearchEntry) {
    // A discipline is the window itself: it opens at its top, with nothing to
    // scroll to or highlight; only a field target scrolls.
    let window = entry
        .field
        .as_deref()
        .and_then(|id| id.strip_prefix(DISCIPLINE_PREFIX))
        .and_then(|id| Discipline::ALL.into_iter().find(|d| d.id() == id));
    if let Some(discipline) = window {
        open_discipline_window(state, discipline);
        state.shell.search.focus = None;
        state.shell.search.open = false;
        state.shell.search.query.clear();
        state.shell.search.selected = 0;
        return;
    }
    if let Some(field) = entry
        .field
        .as_deref()
        .and_then(|path| state.sandbox.fields.iter().find(|f| f.id == path))
    {
        let discipline = field.discipline;
        open_discipline_window(state, discipline);
    }
    state.shell.search.focus = Some(Focus::new(entry.field.clone(), None));
    state.shell.search.open = false;
    state.shell.search.query.clear();
    state.shell.search.selected = 0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::search;

    fn sandbox_state() -> AppState {
        let mut state = AppState::default();
        assert!(state.enter_sandbox(true));
        state
    }

    #[test]
    fn the_index_covers_every_sandbox_field_by_label_id_group_and_discipline() {
        let state = sandbox_state();
        let index = build_index(&state.sandbox.fields);
        assert_eq!(
            index.len(),
            state.sandbox.fields.len() + Discipline::ALL.len() + CONCEPT_TARGETS.len()
        );
        for (query, id) in [
            ("span", "design.span_m"),
            ("span_m", "design.span_m"),
            ("planform", "design.span_m"),
        ] {
            let hits = search(&index, query, 50);
            assert!(
                hits.iter().any(|e| e.field.as_deref() == Some(id)),
                "{query} finds {id}"
            );
        }
        assert!(search(&index, "zzzz-no-such-parameter", 5).is_empty());
        let nacelle = search(&index, "nacelle", 200);
        assert!(nacelle
            .iter()
            .all(|e| e.field.as_deref() != Some("design.span_m")));
    }

    #[test]
    fn choosing_the_wing_panel_count_raises_the_wing_window_and_pulses_the_field() {
        let mut state = sandbox_state();
        let id = "geometry.wing.n_subdivisions";
        let index = build_index(&state.sandbox.fields);
        let entry = index
            .iter()
            .find(|e| e.field.as_deref() == Some(id))
            .expect("panel count entry")
            .clone();
        crate::search::jump_to(&mut state, &entry);
        assert_eq!(state.sandbox.raise_discipline.as_deref(), Some("wing"));
        // The window draws the field's group with the shared pulse.
        let fields = state.sandbox.fields.clone();
        let groups = super::super::fields::grouped(&fields, super::super::fields::Discipline::Wing);
        let (title, members) = groups
            .iter()
            .find(|(_, members)| members.iter().any(|f| f.id == id))
            .expect("a group holds the field");
        let members: Vec<_> = members.iter().map(|f| (*f).clone()).collect();
        let ctx = egui::Context::default();
        let focus = Focus::new(Some(id.to_owned()), None);
        crate::search::publish_focus(&ctx, Some(&focus));
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 900.0));
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    crate::sandbox::editors::show_group(
                        &mut state,
                        ui,
                        title,
                        &members,
                        "test_group",
                        false,
                    );
                });
            },
        );
        assert!(crate::search::focus_was_anchored(&ctx));
    }

    #[test]
    fn choosing_a_result_opens_its_window_and_keeps_the_highlight() {
        let mut state = sandbox_state();
        let index = build_index(&state.sandbox.fields);
        let entry = index
            .iter()
            .find(|e| e.field.as_deref() == Some("design.span_m"))
            .expect("span entry")
            .clone();
        state.shell.search.open = true;
        crate::search::jump_to(&mut state, &entry);
        assert!(state.sandbox.active(), "the sandbox stays open");
        assert!(state
            .sandbox
            .layout
            .open_disciplines
            .contains(&"wing".to_owned()));
        assert_eq!(
            state
                .shell
                .search
                .focus
                .as_ref()
                .and_then(|f| f.field.as_deref()),
            Some("design.span_m")
        );
        assert!(!state.shell.search.open);
    }
}

#[cfg(test)]
mod keyboard_tests {
    use super::*;
    use egui::{Event, Key, Modifiers, PointerButton, Pos2};

    fn key(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    fn pointer(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    /// One Sandbox frame with `events`, at a fixed time step.
    fn frame(state: &mut AppState, ctx: &egui::Context, time: &mut f64, events: Vec<Event>) {
        *time += 0.05;
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    Pos2::ZERO,
                    egui::vec2(1400.0, 900.0),
                )),
                time: Some(*time),
                events,
                ..Default::default()
            },
            |ctx| crate::sandbox::workspace::show_sandbox_workspace(state, ctx),
        );
    }

    fn run_started(state: &AppState) -> bool {
        state.sandbox.full_analysis_revision.is_some()
            || state.is_running
            || state.sandbox.estimates.running()
    }

    /// Enter in the palette with no result does nothing, even when an
    /// analysis button owned the keyboard focus before the palette opened.
    #[test]
    fn enter_in_the_palette_with_no_result_starts_no_analysis() {
        for button in 0..2 {
            let mut state = AppState::default();
            assert!(state.enter_sandbox(true));
            let ctx = egui::Context::default();
            let mut time = 0.0;
            frame(&mut state, &ctx, &mut time, vec![]);
            frame(&mut state, &ctx, &mut time, vec![]);
            // Press an analysis button and release elsewhere: no click, but
            // the button keeps the keyboard focus.
            let rects = crate::sandbox::viewport::overlay_rects_tagged(&ctx, "action");
            let target = rects[button].center();
            frame(
                &mut state,
                &ctx,
                &mut time,
                vec![Event::PointerMoved(target)],
            );
            frame(&mut state, &ctx, &mut time, vec![pointer(target, true)]);
            let away = Pos2::new(5.0, 5.0);
            frame(&mut state, &ctx, &mut time, vec![Event::PointerMoved(away)]);
            frame(&mut state, &ctx, &mut time, vec![pointer(away, false)]);
            assert!(!run_started(&state), "setup: no click happened");
            frame(
                &mut state,
                &ctx,
                &mut time,
                vec![key(Key::K, Modifiers::COMMAND)],
            );
            assert!(state.shell.search.open);
            frame(&mut state, &ctx, &mut time, vec![]);
            frame(
                &mut state,
                &ctx,
                &mut time,
                vec![Event::Text("zzzz-no-such-parameter".into())],
            );
            frame(
                &mut state,
                &ctx,
                &mut time,
                vec![key(Key::Enter, Modifiers::NONE)],
            );
            for _ in 0..3 {
                frame(&mut state, &ctx, &mut time, vec![]);
            }
            assert!(!run_started(&state), "button {button}");
            assert!(state.shell.search.open, "the palette stays open");
            assert_eq!(state.shell.search.query, "zzzz-no-such-parameter");
        }
    }

    /// Reopening starts from an empty query.
    #[test]
    fn reopening_the_palette_clears_the_previous_query() {
        let mut state = AppState::default();
        assert!(state.enter_sandbox(true));
        crate::views::search_overlay::set_open(&mut state, true);
        state.shell.search.query = "propeller".to_owned();
        crate::views::search_overlay::set_open(&mut state, false);
        crate::views::search_overlay::set_open(&mut state, true);
        assert!(state.shell.search.query.is_empty());
    }
}

#[cfg(test)]
mod ranking_tests {
    use super::*;
    use crate::search::search;

    fn index() -> Vec<SearchEntry> {
        let mut state = AppState::default();
        assert!(state.enter_sandbox(true));
        build_index(&state.sandbox.fields)
    }

    /// "dihedral" finds the wing's vertical offsets, and no tail twist ranks
    /// above them (the main palette's order).
    #[test]
    fn dihedral_ranks_the_wing_offsets_first() {
        let index = index();
        let hits = search(&index, "dihedral", 9);
        assert!(!hits.is_empty());
        for hit in &hits {
            assert!(
                hit.field
                    .as_deref()
                    .is_some_and(|id| id.starts_with("geometry.wing.")),
                "{:?} is not a wing dihedral item",
                hit.title
            );
        }
        assert!(hits
            .iter()
            .any(|hit| hit.field.as_deref() == Some("geometry.wing.tip_z_m")));
    }

    /// The glossary's synonyms reach the Sandbox palette: "CG gate" finds the
    /// wing-position field instead of "No matches".
    #[test]
    fn the_cg_gate_concept_is_found_in_the_sandbox_palette() {
        let index = index();
        let hits = search(&index, "cg gate", 5);
        assert_eq!(
            hits.first().and_then(|hit| hit.field.as_deref()),
            Some("design.wing_x_shift_m")
        );
        let by_synonym = search(&index, "centre of gravity", 5);
        assert!(!by_synonym.is_empty());
    }
}
