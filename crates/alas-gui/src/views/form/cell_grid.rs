// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one form grid: schema forms and hand-built editors (the take-off mass
//! card, the route, the payload zone editor, the wing-analysis cards, the
//! add-on inputs) all place their cells with it.
//!
//! - The column count comes from the width only (`form_column_count`), the
//!   columns are [`COLUMN_GUTTER`] apart, and a value box fills its cell.
//! - A row is as tall as its tallest cell; its captions share the height of
//!   the tallest caption (see [`super::caption`]), so every box of the row
//!   starts at one y. Rows are [`ROW_GAP`] apart.
//! - A cell taller than two ordinary rows (a list or table editor) spans
//!   rows: its column stays busy down to its bottom and the cells after it
//!   fill the other columns, so it never stretches the rows beside it.

use egui::{pos2, vec2, Align, Id, Layout, Rect, Ui, UiBuilder, UiStackInfo};

use super::caption::{caption, caption_line_height, caption_row, CaptionRow, CAPTION_ROW_TAG};
use super::layout::form_column_count;
use super::MAX_FORM_COLUMNS;
use crate::theme::space::{COLUMN_GUTTER, LABEL_TO_BOX, ROW_GAP};

/// Cells placed left to right, top to bottom, in equal columns.
pub(crate) struct CellGrid {
    left: f32,
    top: f32,
    width: f32,
    column_width: f32,
    columns: usize,
    placed: usize,
    /// Index, top and level bottom of the current row.
    row: usize,
    row_top: f32,
    row_bottom: f32,
    /// Whether the current row holds a cell that is not spanning.
    row_has_level_cell: bool,
    next_column: usize,
    /// Bottom of the spanning cell that keeps each column busy.
    busy_until: Vec<f32>,
    /// Height above which a cell spans rows.
    tall: f32,
    /// The row of an enclosing grid, when this grid is drawn in its cell.
    outer: Option<CaptionRow>,
    salt: Id,
}

impl CellGrid {
    /// A grid across the width `ui` has left; `id` keeps the widgets of two
    /// grids on one page apart.
    pub(crate) fn new(ui: &Ui, id: impl std::hash::Hash) -> Self {
        Self::with_columns(ui, form_column_count(ui.available_width()), id)
    }

    /// [`CellGrid::new`] with a narrower least column, for editors whose cells
    /// are short (a number and its unit).
    pub(crate) fn with_min_column(ui: &Ui, min_column: f32, id: impl std::hash::Hash) -> Self {
        let width = ui.available_width().max(1.0);
        let columns = ((width / min_column.max(1.0)).floor() as usize).clamp(1, MAX_FORM_COLUMNS);
        Self::with_columns(ui, columns, id)
    }

    /// A grid of `columns` columns, for a page whose own column rule (the
    /// cards beside each other) the grid follows.
    pub(crate) fn with_columns(ui: &Ui, columns: usize, id: impl std::hash::Hash) -> Self {
        let width = ui.available_width().max(1.0);
        let columns = columns.clamp(1, MAX_FORM_COLUMNS);
        let column_width =
            ((width - COLUMN_GUTTER * (columns - 1) as f32) / columns as f32).max(1.0);
        let ordinary = caption_line_height(ui) + LABEL_TO_BOX + crate::theme::control_height(ui);
        let top = ui.cursor().top();
        let grid = Self {
            left: ui.cursor().left(),
            top,
            width,
            column_width,
            columns,
            placed: 0,
            row: 0,
            row_top: top,
            row_bottom: top,
            row_has_level_cell: false,
            next_column: 0,
            busy_until: vec![f32::MIN; columns],
            tall: 2.0 * ordinary + ROW_GAP,
            outer: caption_row(ui),
            salt: ui.id().with(id),
        };
        grid.begin_row(ui.ctx());
        grid
    }

    /// Columns of the grid.
    pub(crate) fn columns(&self) -> usize {
        self.columns
    }

    /// Draw the next cell. `add` gets the cell's Ui and the width a value box
    /// takes in it (the whole cell).
    pub(crate) fn cell(&mut self, ui: &mut Ui, add: impl FnOnce(&mut Ui, f32)) {
        let column = self.free_column(ui.ctx());
        let x = self.left + column as f32 * (self.column_width + COLUMN_GUTTER);
        let rect = Rect::from_min_size(pos2(x, self.row_top), vec2(self.column_width, 10_000.0));
        let mut child = ui.new_child(
            UiBuilder::new()
                .id_salt((self.salt, self.placed))
                .max_rect(rect)
                .layout(Layout::top_down(Align::Min))
                .ui_stack_info(
                    UiStackInfo::default().with_tag_value(CAPTION_ROW_TAG, self.caption_row(ui)),
                ),
        );
        // Inside a cell everything belongs to one field: caption, box, note,
        // and the field's controls change only their ring on hover or focus.
        child.spacing_mut().item_spacing.y = LABEL_TO_BOX;
        crate::theme::field_states(child.visuals_mut());
        add(&mut child, self.column_width);
        let bottom = child.min_rect().bottom();
        if self.columns > 1 && bottom - self.row_top > self.tall {
            self.busy_until[column] = bottom;
        } else {
            self.row_bottom = self.row_bottom.max(bottom);
            self.row_has_level_cell = true;
        }
        self.next_column = column + 1;
        self.placed += 1;
    }

    /// Keep the next two cells on one row: when the current row has room for
    /// only one more, start a new row first. A switch stays beside the field
    /// it shows.
    pub(crate) fn keep_with_next(&mut self, ctx: &egui::Context) {
        if self.columns < 2 || self.next_column == 0 {
            return;
        }
        let free = (self.next_column..self.columns)
            .filter(|&column| self.free_at(column, self.row_top))
            .count();
        if free < 2 {
            self.next_row(ctx);
        }
    }

    /// Reserve the room of the grid; call once after the last cell.
    pub(crate) fn finish(self, ui: &mut Ui) {
        if self.placed == 0 {
            return;
        }
        self.settle_band(ui.ctx());
        let level = if self.row_has_level_cell {
            self.row_bottom
        } else {
            self.top
        };
        let bottom = self.busy_until.iter().copied().fold(level, f32::max);
        ui.advance_cursor_after_rect(Rect::from_min_max(
            pos2(self.left, self.top),
            pos2(self.left + self.width, bottom.max(self.top)),
        ));
    }

    /// The column the next cell takes, starting a new row when the current
    /// one has no free column left.
    fn free_column(&mut self, ctx: &egui::Context) -> usize {
        loop {
            if let Some(column) =
                (self.next_column..self.columns).find(|&column| self.free_at(column, self.row_top))
            {
                return column;
            }
            self.next_row(ctx);
        }
    }

    fn free_at(&self, column: usize, top: f32) -> bool {
        self.busy_until[column] + ROW_GAP <= top + 0.5
    }

    fn next_row(&mut self, ctx: &egui::Context) {
        self.settle_band(ctx);
        let mut top = if self.row_has_level_cell {
            self.row_bottom + ROW_GAP
        } else {
            self.row_top
        };
        if !(0..self.columns).any(|column| self.free_at(column, top)) {
            // Every column is under a spanning cell: start below the first to end.
            top = self
                .busy_until
                .iter()
                .map(|bottom| bottom + ROW_GAP)
                .fold(f32::INFINITY, f32::min)
                .max(top);
        }
        self.row += 1;
        self.row_top = top;
        self.row_bottom = top;
        self.row_has_level_cell = false;
        self.next_column = 0;
        self.begin_row(ctx);
    }

    fn band_key(&self, row: usize) -> Id {
        self.salt.with(("caption_band", row))
    }

    fn measured_key(&self, row: usize) -> Id {
        self.salt.with(("caption_measured", row))
    }

    fn begin_row(&self, ctx: &egui::Context) {
        let key = self.measured_key(self.row);
        ctx.data_mut(|data| data.insert_temp(key, 0.0_f32));
    }

    /// The band the captions of the current row stand in.
    fn caption_row(&self, ui: &Ui) -> CaptionRow {
        let band = ui
            .ctx()
            .data(|data| data.get_temp::<f32>(self.band_key(self.row)))
            .unwrap_or(0.0);
        let mut row = CaptionRow {
            band,
            report: vec![self.measured_key(self.row)],
        };
        // The first row of a grid in a cell is part of the enclosing row.
        if let (0, Some(outer)) = (self.row, &self.outer) {
            row.band = row.band.max(outer.band);
            row.report.extend(outer.report.iter().copied());
        }
        row
    }

    /// Keep the tallest caption of the row just drawn for the next frame, and
    /// draw again when it changed, so the boxes of the row line up.
    fn settle_band(&self, ctx: &egui::Context) {
        let (band_key, measured_key) = (self.band_key(self.row), self.measured_key(self.row));
        let changed = ctx.data_mut(|data| {
            let measured = data.get_temp::<f32>(measured_key).unwrap_or(0.0);
            let band = data.get_temp::<f32>(band_key).unwrap_or(0.0);
            data.insert_temp(band_key, measured);
            (measured - band).abs() > 0.5
        });
        if changed {
            ctx.request_repaint();
        }
    }
}

/// A drag value as wide as its cell, centred like the schema form's boxes.
pub(crate) fn drag_box(ui: &mut Ui, width: f32, drag: egui::DragValue<'_>) -> egui::Response {
    ui.add_sized([width, crate::theme::control_height(ui)], drag)
}

/// A caption over the box of a hand-built cell, with `extra` (a note, a lock)
/// right-aligned on its last line, over the box's right edge; the text wraps
/// short of what `extra` takes. The same band as a schema field's caption.
pub(crate) fn label_row(
    ui: &mut Ui,
    text: impl Into<egui::WidgetText>,
    extra: impl FnOnce(&mut Ui),
) -> egui::Response {
    caption(ui, text, 0.0, extra)
}

/// A numeric field as a cell: `label` over a value box as wide as the cell,
/// like the schema form's fields. True when the value changed.
pub(crate) fn drag_cell(ui: &mut Ui, label: &str, hover: &str, drag: egui::DragValue<'_>) -> bool {
    let width = ui.available_width();
    label_row(ui, label.to_owned(), |_| {}).on_hover_text(hover);
    drag_box(ui, width, drag).changed()
}

/// A hand-built editor on the form grid: `contents` adds its fields as cells
/// of one [`CellGrid`], so they take the columns, gutters and row gaps of the
/// schema forms.
pub(crate) fn grid_form(
    ui: &mut Ui,
    id: impl std::hash::Hash,
    contents: impl FnOnce(&mut CellGrid, &mut Ui),
) {
    let mut grid = CellGrid::new(ui, id);
    contents(&mut grid, ui);
    grid.finish(ui);
}
