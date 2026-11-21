// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one read-only table of the forms: a header row over rows of values,
//! across the whole width of the grid it stands in.
//!
//! A column needs the width of its widest text, header or cell; a header may
//! wrap to a second line (never a third) when that leaves the room the cells
//! need. The columns stand [`COLUMN_GUTTER`] apart, like the form grid's, and
//! share the grid width in proportion to the width they need, so the table
//! spans the card and no header runs over the column beside it. A table that
//! needs more than the card (long translated headers in a narrow window)
//! scrolls sideways instead of widening the page. Headers and cells are drawn
//! through the engineering notation (`S_ref`, `m^2`) like the form captions.
//! A text column is left-aligned; a number column is right-aligned at one
//! fixed number of decimals, so its decimals line up down the column and no
//! row reads at a different precision from the others.

use std::sync::Arc;

use egui::{pos2, vec2, Align, Color32, FontId, Galley, Rect, Sense, TextStyle, Ui};

use crate::theme::space::COLUMN_GUTTER;

/// Gap between two rows, pt; a row's stripe reaches half of it.
const ROW_SPACING: f32 = 3.0;

/// One column of a [`value_table`]: its (translated) header, and the decimals
/// its numbers are shown with, `None` for a text column.
pub(crate) struct Column {
    pub(crate) header: String,
    pub(crate) decimals: Option<usize>,
}

/// One cell of a [`value_table`] row.
pub(crate) enum Value {
    Text(String),
    Number(f64),
}

/// `value` at exactly `decimals` decimals; a value that rounds to zero reads
/// without a minus sign.
fn fixed(value: f64, decimals: usize) -> String {
    if !value.is_finite() {
        return "-".to_owned();
    }
    let text = format!("{value:.decimals$}");
    if text.starts_with('-') && text.trim_start_matches(['-', '0', '.']).is_empty() {
        text[1..].to_owned()
    } else {
        text
    }
}

/// `text` laid out in `font` with its notation drawn, wrapped at `wrap`
/// points and its lines aligned to `halign` (the galley's origin is the
/// left edge for left alignment, the right edge for right alignment).
fn galley(
    ui: &Ui,
    text: &str,
    font: &FontId,
    color: Color32,
    wrap: f32,
    halign: Align,
) -> Arc<Galley> {
    let mut job = crate::notation::job(text, font, color);
    job.wrap.max_width = wrap;
    job.halign = halign;
    ui.fonts(|fonts| fonts.layout_job(job))
}

/// Width of `text` on one line, pt.
fn width_of(ui: &Ui, text: &str, font: &FontId) -> f32 {
    galley(ui, text, font, Color32::WHITE, f32::INFINITY, Align::LEFT)
        .size()
        .x
}

/// The narrowest width that shows `text` on at most two lines broken at a
/// space, pt, with room for the space the first line keeps.
fn two_line_width(ui: &Ui, text: &str, font: &FontId) -> f32 {
    let whole = width_of(ui, text, font);
    let split = text
        .match_indices(' ')
        .map(|(at, _)| width_of(ui, &text[..at], font).max(width_of(ui, &text[at + 1..], font)))
        .fold(whole, f32::min);
    (split + 0.5 * font.size).min(whole)
}

/// Column widths across `room`: in proportion to the `natural` widths when
/// these fit, else shrunk from them toward the `least` widths, and never
/// below those (the table then scrolls).
fn spread(natural: &[f32], least: &[f32], room: f32) -> Vec<f32> {
    let total: f32 = natural.iter().sum();
    if total <= room {
        let stretch = room / total.max(1.0);
        return natural
            .iter()
            .map(|width| (width * stretch).floor())
            .collect();
    }
    let floor: f32 = least.iter().sum();
    let share = ((room - floor) / (total - floor).max(1e-3)).clamp(0.0, 1.0);
    natural
        .iter()
        .zip(least)
        .map(|(natural, least)| (least + (natural - least) * share).ceil())
        .collect()
}

/// Draw a read-only table with `columns` over `rows`.
pub(crate) fn value_table(ui: &mut Ui, id: &str, columns: &[Column], rows: &[Vec<Value>]) {
    if columns.is_empty() {
        return;
    }
    let head_font = TextStyle::Small.resolve(ui.style());
    let body_font = TextStyle::Body.resolve(ui.style());
    let cells: Vec<Vec<String>> = rows
        .iter()
        .map(|row| {
            columns
                .iter()
                .zip(row)
                .map(|(column, value)| match (value, column.decimals) {
                    (Value::Text(text), _) => text.clone(),
                    (Value::Number(number), decimals) => fixed(*number, decimals.unwrap_or(0)),
                })
                .collect()
        })
        .collect();
    let (natural, least): (Vec<f32>, Vec<f32>) = columns
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let widest = cells
                .iter()
                .filter_map(|row| row.get(index))
                .map(|text| width_of(ui, text, &body_font))
                .fold(0.0_f32, f32::max);
            let header = width_of(ui, &column.header, &head_font);
            let wrapped = two_line_width(ui, &column.header, &head_font);
            (widest.max(header).ceil(), widest.max(wrapped).ceil())
        })
        .unzip();
    let gutters = COLUMN_GUTTER * (columns.len() - 1) as f32;
    let widths = spread(&natural, &least, ui.available_width() - gutters);
    let lefts: Vec<f32> = widths
        .iter()
        .scan(0.0_f32, |left, width| {
            let at = *left;
            *left += width + COLUMN_GUTTER;
            Some(at)
        })
        .collect();
    let align = |column: &Column| {
        if column.decimals.is_some() {
            Align::RIGHT
        } else {
            Align::LEFT
        }
    };
    let strong = ui.visuals().strong_text_color();
    let text_color = ui.visuals().text_color();
    let headers: Vec<Arc<Galley>> = columns
        .iter()
        .zip(&widths)
        .map(|(column, width)| {
            galley(
                ui,
                &column.header,
                &head_font,
                strong,
                width + 0.5,
                align(column),
            )
        })
        .collect();
    let header_height = headers
        .iter()
        .map(|header| header.size().y)
        .fold(0.0_f32, f32::max);
    let row_height = ui.spacing().interact_size.y;
    let size = vec2(
        widths.iter().sum::<f32>() + gutters,
        header_height + rows.len() as f32 * (row_height + ROW_SPACING),
    );
    // The x of a cell's text origin: its column's left edge, or its right
    // edge for a right-aligned column.
    let origin = |left: f32, index: usize| match align(&columns[index]) {
        Align::RIGHT => left + lefts[index] + widths[index],
        _ => left + lefts[index],
    };
    egui::ScrollArea::horizontal()
        .id_salt((id, "scroll"))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            if !ui.is_rect_visible(rect) {
                return;
            }
            let painter = ui.painter();
            // Headers stand on one line at the bottom of the header band.
            for (index, header) in headers.into_iter().enumerate() {
                let top = rect.top() + header_height - header.size().y;
                painter.galley(pos2(origin(rect.left(), index), top), header, strong);
            }
            let mut top = rect.top() + header_height + ROW_SPACING;
            for (row_index, row) in cells.iter().enumerate() {
                // Every other row striped, from the first, as egui stripes a grid.
                if row_index % 2 == 0 {
                    let stripe =
                        Rect::from_min_size(pos2(rect.left(), top), vec2(size.x, row_height));
                    painter.rect_filled(
                        stripe.expand2(vec2(2.0, 0.5 * ROW_SPACING)),
                        2.0,
                        ui.visuals().faint_bg_color,
                    );
                }
                for (index, text) in row.iter().enumerate() {
                    let cell = galley(
                        ui,
                        text,
                        &body_font,
                        text_color,
                        f32::INFINITY,
                        align(&columns[index]),
                    );
                    let y = top + 0.5 * (row_height - cell.size().y);
                    painter.galley(pos2(origin(rect.left(), index), y), cell, text_color);
                }
                top += row_height + ROW_SPACING;
            }
        });
}
