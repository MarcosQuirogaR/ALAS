// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Ctrl+K search overlay and the top-bar button that opens it.

use egui::{Align2, Area, Context, Id, Key, Modifiers, Order, Ui};

use crate::search::{build_index_with_presets, jump_to, search_in, SearchEntry, FOCUS_SECONDS};
use crate::state::AppState;
use crate::views::tr_label;

/// Results shown at most.
const MAX_RESULTS: usize = 12;

/// Open the overlay on Ctrl+K (Cmd+K on macOS) and close it on Escape.
pub fn handle_shortcuts(state: &mut AppState, ctx: &Context) {
    let toggle = ctx.input_mut(|input| input.consume_key(Modifiers::COMMAND, Key::K));
    if toggle {
        set_open(state, !state.shell.search.open);
    }
    // While the palette is open it owns the keys it uses. They are taken out
    // of the input before any page widget is drawn: a button that kept the
    // keyboard focus (Full Analysis, Run) would otherwise see the same Enter
    // that confirms an empty search and start a run.
    let search = &mut state.shell.search;
    search.enter = false;
    search.step = 0;
    if search.open {
        ctx.input_mut(|input| {
            search.enter = input.consume_key(Modifiers::NONE, Key::Enter);
            search.step = i32::from(input.consume_key(Modifiers::NONE, Key::ArrowDown))
                - i32::from(input.consume_key(Modifiers::NONE, Key::ArrowUp));
            input.consume_key(Modifiers::NONE, Key::Tab);
            input.consume_key(Modifiers::SHIFT, Key::Tab);
        });
    }
}

/// Open or close the palette. Opening starts from an empty query.
pub fn set_open(state: &mut AppState, open: bool) {
    let search = &mut state.shell.search;
    if open && !search.open {
        search.query.clear();
    }
    search.open = open;
    search.wants_focus = open;
    search.selected = 0;
}
/// Advance the highlight by wall-clock time and clear it after
/// [`FOCUS_SECONDS`]; the pulse does not depend on the frame rate.
pub fn tick_focus(state: &mut AppState, ctx: &Context) {
    let now = ctx.input(|input| input.time);
    if let Some(focus) = state.shell.search.focus.as_mut() {
        let started = *focus.started.get_or_insert(now);
        focus.elapsed = (now - started) as f32;
        if focus.elapsed >= FOCUS_SECONDS {
            state.shell.search.focus = None;
        } else {
            ctx.request_repaint();
        }
    }
    crate::search::publish_focus(ctx, state.shell.search.focus.as_ref());
}

/// The top-bar button that opens the overlay. The shortcut hint is greyed so
/// it reads as a hint, not as part of the label.
pub fn show_button(state: &mut AppState, ui: &mut Ui) {
    let mut label = egui::text::LayoutJob::default();
    let font = egui::TextStyle::Button.resolve(ui.style());
    label.append(
        &tr_label("Search"),
        0.0,
        egui::TextFormat::simple(font.clone(), ui.visuals().text_color()),
    );
    label.append(
        "Ctrl+K",
        10.0,
        egui::TextFormat::simple(font, ui.visuals().weak_text_color()),
    );
    if ui
        .add(
            crate::theme::selectable_button(label, state.shell.search.open)
                .min_size(egui::vec2(0.0, crate::theme::control_height(ui))),
        )
        .clicked()
    {
        set_open(state, !state.shell.search.open);
    }
}

fn ensure_index(state: &mut AppState) {
    // The Sandbox searches its own parameters, so its index is kept apart.
    let sandbox = state.sandbox.active();
    let language = format!(
        "{}{}",
        state.language.code(),
        if sandbox { ":sandbox" } else { "" }
    );
    let fresh = state
        .shell
        .search
        .index
        .as_ref()
        .is_some_and(|(built, _)| *built == language);
    if !fresh {
        let entries = if sandbox {
            crate::sandbox::search::build_index(&state.sandbox.fields)
        } else {
            build_index_with_presets(&state.schema, &state.preset_names)
        };
        state.shell.search.index = Some((language, entries));
    }
}

/// Width of the right-hand column that holds the "Advanced" tag. Fixed, so the
/// tag sits at the same x on every row whether or not the row has one.
const TAG_COLUMN_WIDTH: f32 = 78.0;
/// Horizontal padding inside a row and the gap between its columns.
const ROW_PADDING: f32 = 10.0;
/// Wide enough that a long title never runs into its breadcrumb column.
const COLUMN_GAP: f32 = 24.0;

/// The column widths of a row `width` wide: (title, location). The split is
/// a fixed share of the row, so the breadcrumb column starts at the same x on
/// every row of every result set; a text longer than its column is cut with an
/// ellipsis and shown in full on hover.
fn row_columns(width: f32) -> (f32, f32) {
    let inner = (width - 2.0 * ROW_PADDING - TAG_COLUMN_WIDTH - 2.0 * COLUMN_GAP).max(40.0);
    let title = (inner * 0.56).max(20.0);
    (title, (inner - title).max(20.0))
}

/// One line of `text` at most `width` wide, ending in an ellipsis when it
/// does not fit. Returns the galley and whether it was cut.
fn fitted(
    ui: &Ui,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    width: f32,
) -> (std::sync::Arc<egui::Galley>, bool) {
    let mut job = crate::notation::job(text, &font, color);
    job.wrap = egui::text::TextWrapping {
        max_width: width,
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('\u{2026}'),
    };
    let galley = ui.fonts(|fonts| fonts.layout_job(job));
    let cut = galley.elided;
    (galley, cut)
}

/// One result as three columns: the title (cut with an ellipsis, in full on
/// hover), where it lives, and a fixed-width column at the right that holds
/// the small "Advanced" tag when choosing the result switches the view to the
/// Advanced level.
fn result_row(
    ui: &mut Ui,
    title: &str,
    location: &str,
    advanced: bool,
    selected: bool,
) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::click());
    let mut cut = false;
    if ui.is_rect_visible(rect) {
        let visuals = ui.visuals();
        let (fill, color, weak) = if selected {
            (
                visuals.selection.bg_fill,
                visuals.selection.stroke.color,
                visuals.selection.stroke.color,
            )
        } else if response.hovered() {
            (
                visuals.widgets.hovered.weak_bg_fill,
                visuals.text_color(),
                visuals.weak_text_color(),
            )
        } else {
            (
                egui::Color32::TRANSPARENT,
                visuals.text_color(),
                visuals.weak_text_color(),
            )
        };
        ui.painter()
            .rect_filled(rect, crate::theme::SELECTABLE_CONTROL_RADIUS, fill);
        let font = egui::FontId::proportional(14.0);
        let (title_width, location_width) = row_columns(rect.width());
        let title_left = rect.left() + ROW_PADDING;
        let location_left = title_left + title_width + COLUMN_GAP;
        let tag_left = rect.right() - ROW_PADDING - TAG_COLUMN_WIDTH;
        let (title_galley, title_cut) = fitted(ui, title, font.clone(), color, title_width);
        let (location_galley, location_cut) = fitted(ui, location, font, weak, location_width);
        cut = title_cut || location_cut;
        for (galley, left, tint) in [
            (title_galley, title_left, color),
            (location_galley, location_left, weak),
        ] {
            let top = rect.center().y - galley.size().y / 2.0;
            ui.painter().galley(egui::pos2(left, top), galley, tint);
        }
        if advanced {
            let tag_rect = egui::Rect::from_min_size(
                egui::pos2(tag_left, rect.center().y - 9.0),
                egui::vec2(TAG_COLUMN_WIDTH, 18.0),
            );
            ui.painter().text(
                tag_rect.center(),
                egui::Align2::CENTER_CENTER,
                tr_label("Advanced"),
                egui::FontId::proportional(11.0),
                weak,
            );
            ui.painter().rect_stroke(
                tag_rect,
                crate::theme::SELECTABLE_CONTROL_RADIUS,
                egui::Stroke::new(1.0_f32, weak.gamma_multiply(0.6)),
            );
        }
    }
    if cut {
        crate::notation::hover(response, format!("{title}\n{location}"))
    } else {
        response
    }
}

/// The labels of the groups above the field a dotted `path` names, nearest
/// group first, in the interface language. Empty when the path is not in the
/// schema (a concept, a Sandbox parameter) or sits at the top level.
fn owner_labels(schema: &alas_config::Node, path: &str) -> Vec<String> {
    let mut node = schema;
    let mut labels = Vec::new();
    let segments: Vec<&str> = path.split('.').collect();
    for segment in &segments[..segments.len().saturating_sub(1)] {
        let Some(field) = node.field(segment) else {
            return Vec::new();
        };
        labels.push(tr_label(field.label));
        match &field.entry {
            alas_config::Entry::Node(child) => node = child,
            alas_config::Entry::Leaf(_) => return Vec::new(),
        }
    }
    labels.reverse();
    labels
}

/// The title of every result, with the owning group appended ("Source
/// document - Wing mass") wherever two results would otherwise read the same
/// title under the same location. The nearest group is tried first, then the
/// ones above it, until the rows differ.
pub(crate) fn distinguishing_titles(
    results: &[SearchEntry],
    schema: &alas_config::Node,
) -> Vec<String> {
    let base: Vec<String> = results.iter().map(|entry| tr_label(&entry.title)).collect();
    let owners: Vec<Vec<String>> = results
        .iter()
        .map(|entry| {
            entry
                .field
                .as_deref()
                .map(|path| owner_labels(schema, path))
                .unwrap_or_default()
        })
        .collect();
    let key = |index: usize, depth: usize| -> (String, String) {
        let mut title = base[index].clone();
        for owner in owners[index].iter().take(depth) {
            title = format!("{title} \u{2014} {owner}");
        }
        (title, results[index].location())
    };
    let mut depth = vec![0_usize; results.len()];
    for _ in 0..4 {
        let keys: Vec<(String, String)> = (0..results.len()).map(|i| key(i, depth[i])).collect();
        let mut changed = false;
        for i in 0..results.len() {
            let clash = (0..results.len()).any(|j| j != i && keys[j] == keys[i]);
            if clash && depth[i] < owners[i].len() {
                depth[i] += 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    (0..results.len()).map(|i| key(i, depth[i]).0).collect()
}

/// Draw the overlay when it is open.
pub fn show_overlay(state: &mut AppState, ctx: &Context) {
    if !state.shell.search.open {
        return;
    }
    ensure_index(state);
    if ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape)) {
        state.shell.search.open = false;
        return;
    }
    let mut chosen: Option<SearchEntry> = None;
    let mut close = false;
    // Dim the app behind the palette so the eye stays on it. The layer takes
    // the pointer, so the page below neither shows its hover tooltips over the
    // palette nor reacts to a click; a click on it closes the palette.
    Area::new(Id::new("alas_search_backdrop"))
        .order(Order::Foreground)
        .fixed_pos(ctx.screen_rect().min)
        .show(ctx, |ui| {
            let (rect, response) =
                ui.allocate_exact_size(ctx.screen_rect().size(), egui::Sense::click());
            ui.painter()
                .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(160));
            if response.clicked() {
                close = true;
            }
        });
    Area::new(Id::new("alas_search_overlay"))
        .order(Order::Tooltip)
        .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 72.0))
        .show(ctx, |ui| {
            crate::theme::card_frame(ui)
                // Opaque, with a shadow: the page behind never shows through
                // the palette's border.
                .fill(ui.visuals().panel_fill.to_opaque())
                .shadow(egui::Shadow {
                    offset: egui::vec2(0.0, 6.0),
                    blur: 24.0,
                    spread: 0.0,
                    color: egui::Color32::from_black_alpha(120),
                })
                .show(ui, |ui| {
                    ui.set_width(760.0_f32.min(ctx.screen_rect().width() - 40.0));
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut state.shell.search.query)
                            .hint_text(tr_label(if state.sandbox.active() {
                                "Search Sandbox parameters"
                            } else {
                                "Search Settings and Concepts"
                            }))
                            .desired_width(f32::INFINITY),
                    );
                    // The palette keeps the keyboard while it is open: a click
                    // on a result row or elsewhere never leaves focus on a
                    // page widget that would then see the next key.
                    if state.shell.search.wants_focus || !ctx.memory(|m| m.focused().is_some()) {
                        response.request_focus();
                        state.shell.search.wants_focus = false;
                    }
                    if response.changed() {
                        state.shell.search.selected = 0;
                    }
                    let results: Vec<SearchEntry> = state
                        .shell
                        .search
                        .index
                        .as_ref()
                        .map(|(_, index)| {
                            // Only targets the live configuration draws.
                            search_in(
                                index,
                                &state.shell.search.query,
                                MAX_RESULTS,
                                Some(&state.config_values),
                            )
                            .into_iter()
                            .cloned()
                            .collect()
                        })
                        .unwrap_or_default();
                    if !results.is_empty() {
                        let last = results.len() - 1;
                        let moved =
                            state.shell.search.selected as i64 + i64::from(state.shell.search.step);
                        state.shell.search.selected = moved.clamp(0, last as i64) as usize;
                    }
                    ui.add_space(6.0);
                    let titles = distinguishing_titles(&results, &state.schema);
                    for (index, entry) in results.iter().enumerate() {
                        let selected = index == state.shell.search.selected;
                        let row = result_row(
                            ui,
                            &titles[index],
                            &entry.location(),
                            entry.advanced,
                            selected,
                        );
                        if row.clicked() {
                            chosen = Some(entry.clone());
                        }
                    }
                    if results.is_empty() && !state.shell.search.query.trim().is_empty() {
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 28.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().text(
                            rect.left_center() + egui::vec2(ROW_PADDING, 0.0),
                            egui::Align2::LEFT_CENTER,
                            tr_label("No matches"),
                            egui::FontId::proportional(14.0),
                            ui.visuals().weak_text_color(),
                        );
                    }
                    // Enter was taken out of the input at the start of the
                    // frame (`handle_shortcuts`); with no result it does nothing.
                    if state.shell.search.enter {
                        chosen = results.get(state.shell.search.selected).cloned();
                        state.shell.search.enter = false;
                    }
                    if ui.input(|input| input.pointer.any_click())
                        && !ui.rect_contains_pointer(ui.min_rect())
                    {
                        close = true;
                    }
                });
        });
    if let Some(entry) = chosen {
        jump_to(state, &entry);
    } else if close {
        state.shell.search.open = false;
    }
}

#[cfg(test)]
mod row_tests {
    use super::*;
    use crate::search::build_index;

    #[test]
    fn results_with_one_title_under_one_location_are_told_apart_by_their_owner() {
        let state = AppState::default();
        let index = build_index(&state.schema);
        // Every query a user could type; any title shared by several fields.
        for query in ["source", "revision", "efficiency", "ratio", "mass"] {
            let results: Vec<SearchEntry> = search_in(&index, query, 40, None)
                .into_iter()
                .cloned()
                .collect();
            let titles = distinguishing_titles(&results, &state.schema);
            let mut seen = std::collections::BTreeSet::new();
            for (entry, title) in results.iter().zip(&titles) {
                let same_label = results
                    .iter()
                    .filter(|other| {
                        other.title == entry.title && other.location() == entry.location()
                    })
                    .count();
                if same_label > 1 {
                    assert!(
                        seen.insert((title.clone(), entry.location())),
                        "{query}: {title} in {} appears twice",
                        entry.location()
                    );
                }
            }
        }
    }

    #[test]
    fn the_columns_never_overlap_and_the_tag_column_is_fixed() {
        for width in [300.0_f32, 400.0, 520.0, 560.0, 900.0] {
            let (title, location) = row_columns(width);
            let used = 2.0 * ROW_PADDING + title + location + 2.0 * COLUMN_GAP + TAG_COLUMN_WIDTH;
            assert!(used <= width + 0.5, "{width}: {used}");
            assert!(title > 0.0 && location > 0.0);
        }
        // The tag starts at the same distance from the right edge on every row.
        let (title, location) = row_columns(560.0);
        let tag_left = ROW_PADDING + title + COLUMN_GAP + location + COLUMN_GAP;
        assert!((560.0 - ROW_PADDING - TAG_COLUMN_WIDTH - tag_left).abs() < 0.5);
    }
}
