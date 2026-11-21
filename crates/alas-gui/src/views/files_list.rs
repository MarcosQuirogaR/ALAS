// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The item list of the Files explorer in its details form: sortable column
//! headers and one painted row per item, drawn only for the rows in view
//! (`show_rows`), so a folder of thousands of entries scrolls smoothly.
//! Selection, keyboard navigation and the events the rows raise live here and
//! are shared with the large-icons view.

use std::sync::Arc;

use egui::{Align2, Color32, Modifiers, Rounding, Sense, Stroke, Ui};

use super::files_model::{format_modified, format_size, Action, Entry, SortKey};
use super::files_state::{LibraryUi, ViewMode};
use super::{tr, AppState};

pub(super) use super::files_interact::{background, interact, rename_box};

const ROW_HEIGHT: f32 = 26.0;
const HEADER_HEIGHT: f32 = 28.0;
const PAD: f32 = 8.0;

/// Actions of the menu on the empty part of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Background {
    NewFolder,
    Refresh,
    ShowFolder,
    SaveCurrent,
}

/// What the rows and headers reported this frame.
#[derive(Default)]
pub(super) struct Events {
    /// A click on the item at this position of the view order.
    pub click: Option<(usize, Modifiers)>,
    /// A double-click.
    pub open: Option<usize>,
    pub menu: Option<Action>,
    pub background: Option<Background>,
    pub sort: Option<SortKey>,
    pub clear: bool,
    /// The rename ended: `Some(text)` to apply it, `None` to drop it.
    pub rename: Option<Option<String>>,
    /// A column border was dragged: column index (0 Type, 1 Date, 2 Size)
    /// and the horizontal movement.
    pub resize: Option<(usize, f32)>,
}

struct Columns {
    name: f32,
    kind: f32,
    modified: f32,
    size: f32,
}

/// Narrowest a resizable column may be dragged.
const MIN_COLUMN: f32 = 56.0;
/// Narrowest the name column may become when the others are wide.
const MIN_NAME: f32 = 150.0;

/// The default widths of the Type, Date modified and Size columns.
pub(super) const DEFAULT_COLUMNS: [f32; 3] = [140.0, 150.0, 84.0];

/// Lay the columns out for a list `width` wide: the Name column takes what
/// the others leave, and Type and Date modified drop out when it would get
/// too narrow.
fn columns(width: f32, widths: [f32; 3]) -> Columns {
    let size = widths[2].max(MIN_COLUMN);
    let mut kind = widths[0].max(MIN_COLUMN);
    let mut modified = widths[1].max(MIN_COLUMN);
    if width - size - kind - modified < MIN_NAME {
        kind = 0.0;
    }
    if width - size - kind - modified < MIN_NAME {
        modified = 0.0;
    }
    Columns {
        name: (width - size - kind - modified).max(MIN_NAME.min(width)),
        kind,
        modified,
        size,
    }
}

/// The accent fill of selected rows and the lighter one of hovered rows.
pub(super) fn paint_highlight(
    ui: &Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
    selected: bool,
    hovered: bool,
    cursor: bool,
) {
    let accent = ui.visuals().selection.bg_fill;
    let fill = if selected && hovered {
        Some(accent.gamma_multiply(0.75))
    } else if selected {
        Some(accent.gamma_multiply(0.55))
    } else if hovered {
        Some(
            ui.visuals()
                .widgets
                .hovered
                .weak_bg_fill
                .gamma_multiply(0.8),
        )
    } else {
        None
    };
    let rounding = Rounding::same(4.0);
    let inner = rect.shrink2(egui::vec2(1.0, 0.5));
    if let Some(fill) = fill {
        painter.rect_filled(inner, rounding, fill);
    }
    if cursor && selected {
        painter.rect_stroke(inner, rounding, Stroke::new(1.0_f32, accent));
    }
}

fn header_cell(
    ui: &mut Ui,
    rect: egui::Rect,
    label: &str,
    key: SortKey,
    sort: super::files_model::Sort,
    events: &mut Events,
    right: bool,
) {
    let response = ui.interact(rect, ui.id().with(("files_header", label)), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            Rounding::ZERO,
            ui.visuals()
                .widgets
                .hovered
                .weak_bg_fill
                .gamma_multiply(0.6),
        );
    }
    let font = egui::TextStyle::Body.resolve(ui.style());
    let (anchor, x) = if right {
        (Align2::RIGHT_CENTER, rect.right() - PAD - 12.0)
    } else {
        (Align2::LEFT_CENTER, rect.left() + PAD)
    };
    ui.painter().with_clip_rect(rect).text(
        egui::pos2(x, rect.center().y),
        anchor,
        label,
        font,
        ui.visuals().text_color(),
    );
    if sort.key == key {
        let c = egui::pos2(rect.right() - 12.0, rect.center().y);
        let d = if sort.ascending { -2.5 } else { 2.5 };
        ui.painter().add(egui::Shape::line(
            vec![
                c + egui::vec2(-3.5, -d),
                c + egui::vec2(0.0, d),
                c + egui::vec2(3.5, -d),
            ],
            Stroke::new(1.3_f32, ui.visuals().weak_text_color()),
        ));
    }
    if response.clicked() {
        events.sort = Some(key);
    }
}

fn show_header(ui: &mut Ui, cols: &Columns, sort: super::files_model::Sort, events: &mut Events) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), HEADER_HEIGHT),
        Sense::hover(),
    );
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color),
    );
    let mut x = rect.left();
    let columns = [
        (cols.name, tr("Name"), SortKey::Name, false, None),
        (cols.kind, tr("Type"), SortKey::Type, false, Some(0)),
        (
            cols.modified,
            tr("Date modified"),
            SortKey::Modified,
            false,
            Some(1),
        ),
        (cols.size, tr("Size"), SortKey::Size, true, Some(2)),
    ];
    for (width, label, key, right, grip) in columns {
        if width <= 0.0 {
            continue;
        }
        let area =
            egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(width, HEADER_HEIGHT));
        header_cell(ui, area, &label, key, sort, events, right);
        if let Some(column) = grip {
            resize_handle(ui, area, column, events);
        }
        x += width;
    }
}

/// The draggable border on the left edge of a column (Explorer style): the
/// column grows when dragged left, and the Name column gives way.
fn resize_handle(ui: &mut Ui, column_rect: egui::Rect, column: usize, events: &mut Events) {
    let grip = egui::Rect::from_x_y_ranges(
        (column_rect.left() - 3.0)..=(column_rect.left() + 3.0),
        column_rect.y_range(),
    );
    let response = ui.interact(
        grip,
        ui.id().with(("files_column_grip", column)),
        Sense::drag(),
    );
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeColumn);
    }
    let line = if response.hovered() || response.dragged() {
        ui.visuals().widgets.hovered.bg_stroke.color
    } else {
        ui.visuals()
            .widgets
            .noninteractive
            .bg_stroke
            .color
            .gamma_multiply(0.6)
    };
    ui.painter().line_segment(
        [
            egui::pos2(column_rect.left(), column_rect.top() + 6.0),
            egui::pos2(column_rect.left(), column_rect.bottom() - 6.0),
        ],
        Stroke::new(1.0_f32, line),
    );
    if response.dragged() {
        events.resize = Some((column, response.drag_delta().x));
    }
}

// reason: the arguments are the independent inputs of one draw call; a struct would only rename them.
#[allow(clippy::too_many_arguments)]
fn row(
    state: &mut AppState,
    ui: &mut Ui,
    entries: &[Entry],
    order: &[usize],
    position: usize,
    cols: &Columns,
    events: &mut Events,
    data: &std::path::Path,
) {
    let entry = &entries[order[position]];
    let width = ui.available_width();
    let (rect, mut response) =
        ui.allocate_exact_size(egui::vec2(width, ROW_HEIGHT), Sense::click());
    let lib = &mut state.shell.library;
    let selected = lib.sel.items.contains(&entry.path);
    if response.secondary_clicked() && !selected {
        lib.sel.only(&entry.path);
    }
    let cursor = lib.sel.cursor.as_ref() == Some(&entry.path);
    let painter = ui.painter_at(rect);
    paint_highlight(ui, &painter, rect, selected, response.hovered(), cursor);
    let icon = egui::Rect::from_center_size(
        rect.left_center() + egui::vec2(PAD + 9.0, 0.0),
        egui::vec2(18.0, 18.0),
    );
    super::files_icons::paint(&painter, icon, entry.kind);
    let font = egui::TextStyle::Body.resolve(ui.style());
    let color = ui.visuals().text_color();
    // On the accent fill of a selected row every cell takes the selection's
    // text colour; grey would be unreadable on blue.
    let weak = if selected {
        ui.visuals().selection.stroke.color
    } else {
        ui.visuals().weak_text_color()
    };
    let name_left = rect.left() + PAD + 26.0;
    let renaming = lib
        .rename
        .as_mut()
        .filter(|rename| rename.path == entry.path);
    if let Some(rename) = renaming {
        let area = egui::Rect::from_min_max(
            egui::pos2(name_left - 2.0, rect.top() + 1.0),
            egui::pos2(rect.left() + cols.name - 6.0, rect.bottom() - 1.0),
        );
        rename_box(ui, area, rename, events);
    } else {
        let (galley, cut) = super::files_icons::elided(
            &painter,
            &entry.name,
            font.clone(),
            color,
            rect.left() + cols.name - 8.0 - name_left,
        );
        painter.galley(
            egui::pos2(name_left, rect.center().y - galley.size().y * 0.5),
            galley,
            color,
        );
        if cut {
            response = response.on_hover_text(entry.name.clone());
        }
    }
    let mut x = rect.left() + cols.name;
    let mut cell = |width: f32, text: String, color: Color32, right: bool| {
        if width > 0.0 {
            let clip =
                egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(width, ROW_HEIGHT));
            let (anchor, at) = if right {
                (Align2::RIGHT_CENTER, clip.right() - PAD - 12.0)
            } else {
                (Align2::LEFT_CENTER, clip.left() + PAD)
            };
            painter.with_clip_rect(clip).text(
                egui::pos2(at, rect.center().y),
                anchor,
                text,
                font.clone(),
                color,
            );
            x += width;
        }
    };
    cell(cols.kind, tr(entry.kind.label()), weak, false);
    cell(cols.modified, format_modified(entry.modified), weak, false);
    let size = if entry.is_folder() {
        String::new()
    } else {
        format_size(entry.size)
    };
    cell(cols.size, size, weak, true);
    interact(state, &response, entries, order, position, data, events);
}

/// The message that replaces the list when it has nothing to show.
fn placeholder(state: &AppState, ui: &mut Ui, entries: &[Entry], empty_view: bool) -> bool {
    let lib = &state.shell.library;
    let listing = lib.listings.get(&lib.current());
    let text = if let Some(error) = listing.and_then(|listing| listing.error.as_ref()) {
        crate::views::tr_fields(
            "This folder cannot be read: {error}",
            &[("error", error.clone())],
        )
    } else if lib.searching_subfolders() && lib.search.results.is_none() {
        tr("Searching\u{2026}")
    } else if !listing.is_some_and(|listing| listing.loaded)
        && entries.is_empty()
        && !lib.searching_subfolders()
    {
        tr("Loading\u{2026}")
    } else if empty_view && !lib.query.trim().is_empty() {
        tr("No items match the search.")
    } else if empty_view {
        tr("This folder is empty.")
    } else {
        return false;
    };
    ui.add_space(16.0);
    ui.label(egui::RichText::new(text).weak());
    true
}

/// Draw the list (details or icons) and return what the user did.
pub(super) fn show_list(state: &mut AppState, ui: &mut Ui) -> Events {
    let mut events = Events::default();
    let (entries, order) = state.shell.library.view();
    let data = state.data_dir();
    let area = ui.available_rect_before_wrap();
    background(ui, area, &mut events);
    let mode = state.shell.library.mode;
    let sort = state.shell.library.sort;
    let width = ui.available_width();
    let widths = state.shell.library.col_widths;
    // The rows lose the scrollbar's width when the list overflows; the header
    // is laid out with the same columns plus that reserve, so the Size
    // column is never hidden under the bar.
    let overflow = order.len() as f32 * ROW_HEIGHT > ui.available_height() - HEADER_HEIGHT;
    let bar = if overflow {
        ui.spacing().scroll.allocated_width()
    } else {
        0.0
    };
    let cols = columns(width - bar, widths);
    if mode == ViewMode::Details {
        show_header(ui, &cols, sort, &mut events);
        if let Some((column, delta)) = events.resize.take() {
            let lib = &mut state.shell.library;
            let slot = &mut lib.col_widths[column];
            *slot = (*slot - delta).clamp(MIN_COLUMN, 420.0);
        }
    }
    if placeholder(state, ui, &entries, order.is_empty()) && order.is_empty() {
        return events;
    }
    state.shell.library.list_height = ui.available_height();
    if mode == ViewMode::Icons {
        super::files_grid::show_grid(state, ui, &entries, &order, &data, &mut events);
        return events;
    }
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt("files_list_scroll")
        .auto_shrink([false, false]);
    let lib = &mut state.shell.library;
    if let Some(offset) = lib.scroll_to.take() {
        scroll = scroll.vertical_scroll_offset(offset);
    } else if lib.follow_cursor {
        lib.follow_cursor = false;
        if let Some(offset) = cursor_offset(lib, &entries, &order, ROW_HEIGHT, 1, 0.0) {
            scroll = scroll.vertical_scroll_offset(offset);
        }
    }
    ui.spacing_mut().item_spacing.y = 0.0;
    let output = scroll.show_rows(ui, ROW_HEIGHT, order.len(), |ui, range| {
        for position in range {
            row(
                state,
                ui,
                &entries,
                &order,
                position,
                &cols,
                &mut events,
                &data,
            );
        }
    });
    state.shell.library.list_offset = output.state.offset.y;
    events
}

/// The scroll offset that brings the cursor row into view, when it is not.
pub(super) fn cursor_offset(
    lib: &LibraryUi,
    entries: &[Entry],
    order: &Arc<Vec<usize>>,
    row_height: f32,
    columns: usize,
    top_pad: f32,
) -> Option<f32> {
    let cursor = lib.sel.cursor.as_ref()?;
    let position = order
        .iter()
        .position(|&index| entries[index].path == *cursor)?;
    let top = (position / columns.max(1)) as f32 * row_height + top_pad;
    let bottom = top + row_height;
    let view = lib.list_height.max(row_height);
    if top < lib.list_offset {
        Some(top)
    } else if bottom > lib.list_offset + view {
        Some(bottom - view)
    } else {
        None
    }
}
