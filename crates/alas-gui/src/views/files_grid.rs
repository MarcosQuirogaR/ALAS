// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The large-icons form of the item list. Aircraft files show their top view
//! as the icon (built on a worker thread, a few at a time); every other item
//! shows its painted icon. Only the rows of cells in view are drawn.

use std::path::Path;
use std::sync::Arc;

use egui::{Color32, Sense, Ui};

use super::files_list::{cursor_offset, interact, paint_highlight, rename_box, Events};
use super::files_model::{Entry, ItemKind};
use super::files_state::Thumb;
use super::AppState;

pub(super) const CELL_WIDTH: f32 = 116.0;
pub(super) const CELL_HEIGHT: f32 = 118.0;
const ICON_SIZE: f32 = 64.0;
/// Free space under the last row of cells, points.
const LAST_ROW_ROOM: f32 = 10.0;

// reason: a cell needs the state, the ui, its rectangle, the listing, the order, its
// position, the data folder and the event sink; a struct would only rename them.
#[allow(clippy::too_many_arguments)]
fn cell(
    state: &mut AppState,
    ui: &mut Ui,
    rect: egui::Rect,
    entries: &[Entry],
    order: &[usize],
    position: usize,
    data: &Path,
    events: &mut Events,
) {
    let entry = &entries[order[position]];
    let response = ui.interact(rect, ui.id().with(("files_cell", position)), Sense::click());
    let lib = &mut state.shell.library;
    let selected = lib.sel.items.contains(&entry.path);
    if response.secondary_clicked() && !selected {
        lib.sel.only(&entry.path);
    }
    let cursor = lib.sel.cursor.as_ref() == Some(&entry.path);
    let painter = ui.painter_at(rect);
    paint_highlight(ui, &painter, rect, selected, response.hovered(), cursor);
    let icon = egui::Rect::from_center_size(
        egui::pos2(rect.center().x, rect.top() + 8.0 + ICON_SIZE * 0.5),
        egui::vec2(ICON_SIZE, ICON_SIZE),
    );
    let mut drawn = false;
    if entry.kind == ItemKind::Aircraft {
        let thumb = lib.thumbs.get(&entry.path).cloned();
        match thumb {
            None => {
                let theme = state.theme.figure_theme_name().to_owned();
                let started = state
                    .shell
                    .library
                    .jobs
                    .thumb(ui.ctx(), entry.path.clone(), theme);
                if started {
                    state
                        .shell
                        .library
                        .thumbs
                        .insert(entry.path.clone(), Thumb::Loading);
                }
            }
            Some(Thumb::Scene(scene)) => {
                let revision = state.shell.library.thumb_revision;
                paint_thumbnail(state, ui, icon, entry, &scene, revision);
                drawn = true;
            }
            Some(Thumb::Loading | Thumb::None) => {}
        }
    }
    if !drawn {
        super::files_icons::paint(&painter, icon, entry.kind);
    }
    let label_top = icon.bottom() + 6.0;
    let label_rect = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 4.0, label_top),
        egui::pos2(rect.right() - 4.0, rect.bottom() - 2.0),
    );
    let renaming = state
        .shell
        .library
        .rename
        .as_mut()
        .filter(|rename| rename.path == entry.path);
    if let Some(rename) = renaming {
        rename_box(ui, label_rect.with_max_y(label_top + 22.0), rename, events);
    } else {
        let font = egui::TextStyle::Body.resolve(ui.style());
        let width = label_rect.width();
        let colour = ui.visuals().text_color();
        let measure = |text: &str| {
            painter
                .layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE)
                .size()
                .x
        };
        // Lines break only after a separator; a word wider than the cell is
        // elided, and the full name is the tooltip.
        let (lines, cut) = super::files_label::lines(&measure, &entry.name, width);
        let line_height = font.size * 1.25;
        for (index, line) in lines.iter().enumerate() {
            painter.text(
                egui::pos2(
                    label_rect.center().x,
                    label_top + index as f32 * line_height,
                ),
                egui::Align2::CENTER_TOP,
                line,
                font.clone(),
                colour,
            );
        }
        if cut {
            response.clone().on_hover_text(&entry.name);
        }
    }
    interact(state, &response, entries, order, position, data, events);
}

fn paint_thumbnail(
    state: &mut AppState,
    ui: &mut Ui,
    icon: egui::Rect,
    entry: &Entry,
    scene: &Arc<alas_report::scene::Scene>,
    revision: u64,
) {
    let builder =
        egui::UiBuilder::new()
            .max_rect(icon)
            .layout(egui::Layout::centered_and_justified(
                egui::Direction::TopDown,
            ));
    ui.allocate_new_ui(builder, |ui| {
        crate::views::files_thumbnail::show_fitted(
            state,
            ui,
            &entry.path,
            scene,
            icon.size(),
            revision,
        );
    });
}

/// Draw the cells in view and return through `events` what happened.
pub(super) fn show_grid(
    state: &mut AppState,
    ui: &mut Ui,
    entries: &Arc<Vec<Entry>>,
    order: &Arc<Vec<usize>>,
    data: &Path,
    events: &mut Events,
) {
    let width = ui.available_width() - 12.0;
    let columns = ((width / CELL_WIDTH).floor() as usize).max(1);
    let rows = order.len().div_ceil(columns);
    state.shell.library.grid_columns = columns;
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt("files_grid_scroll")
        .auto_shrink([false, false]);
    let lib = &mut state.shell.library;
    if let Some(offset) = lib.scroll_to.take() {
        scroll = scroll.vertical_scroll_offset(offset);
    } else if lib.follow_cursor {
        lib.follow_cursor = false;
        if let Some(offset) = cursor_offset(lib, entries, order, CELL_HEIGHT, columns, 0.0) {
            scroll = scroll.vertical_scroll_offset(offset);
        }
    }
    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
    let output = scroll.show_rows(ui, CELL_HEIGHT, rows, |ui, range| {
        let last = range.end >= rows;
        for row in range {
            let (strip, _) = ui.allocate_exact_size(
                egui::vec2(columns as f32 * CELL_WIDTH, CELL_HEIGHT),
                Sense::hover(),
            );
            for column in 0..columns {
                let position = row * columns + column;
                if position >= order.len() {
                    break;
                }
                let rect = egui::Rect::from_min_size(
                    egui::pos2(strip.left() + column as f32 * CELL_WIDTH, strip.top()),
                    egui::vec2(CELL_WIDTH, CELL_HEIGHT),
                );
                cell(state, ui, rect, entries, order, position, data, events);
            }
        }
        // Room under the last row, so it can be scrolled fully into view.
        if last {
            ui.allocate_space(egui::vec2(1.0, LAST_ROW_ROOM));
        }
    });
    state.shell.library.list_offset = output.state.offset.y;
}
