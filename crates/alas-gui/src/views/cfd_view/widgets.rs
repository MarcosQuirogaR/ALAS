// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compact, width-filling building blocks shared by the CFD window.
//!
//! Every card here spans the full content width so no card is left stranded
//! against the left edge, long explanations live in hover text or in a named
//! expandable block instead of a permanent paragraph, and numeric tables pack
//! as many label/value pairs per row as the window can hold without stretching
//! the gap between a label and its value.

use crate::views::tr;
use egui::{Grid, RichText, Ui};

/// Width at or above which two plots or two figures may share a row.
pub(crate) const WIDE_ROW_WIDTH: f32 = 720.0;
/// Target width of one label/value pair inside a read-only table.
const PAIR_TARGET_WIDTH: f32 = 330.0;
/// Highest number of label/value pairs packed into one table row.
const MAX_PAIRS: usize = 3;

/// A full-width card with a compact title; `help` is only shown on hover.
pub(crate) fn card<R>(ui: &mut Ui, title: &str, help: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    crate::theme::card_frame(ui)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            card_title(ui, title, help);
            add(ui)
        })
        .inner
}

/// Card heading plus the hover-only explanation marker.
pub(crate) fn card_title(ui: &mut Ui, title: &str, help: &str) {
    ui.horizontal_wrapped(|ui| {
        let heading = ui.label(RichText::new(tr(title)).strong().size(15.0));
        if !help.is_empty() {
            heading.on_hover_text(tr(help));
            // Plain ASCII: the bundled font has no CIRCLED LATIN SMALL LETTER I,
            // so U+24D8 painted a tofu box next to every card title.
            ui.label(RichText::new("(i)").weak().small())
                .on_hover_text(tr(help));
        }
    });
}

/// A named expandable block for provenance, schema and derivation detail that
/// must stay reachable without occupying the screen by default.
pub(crate) fn details<R>(
    ui: &mut Ui,
    id: &str,
    summary: &str,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    egui::CollapsingHeader::new(RichText::new(tr(summary)).small())
        .id_salt(id)
        .default_open(false)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            add(ui)
        })
        .body_returned
}

/// Number of label/value pairs a read-only table packs into one row.
pub(crate) fn pair_columns(available_width: f32) -> usize {
    ((available_width.max(1.0) / PAIR_TARGET_WIDTH).floor() as usize).clamp(1, MAX_PAIRS)
}

/// A read-only table of label/value rows that uses the full card width by
/// packing several label/value pairs per row instead of leaving the right
/// half of the card empty.
pub(crate) fn value_table(ui: &mut Ui, id: &str, rows: &[(String, String)]) {
    value_table_with(ui, id, rows, |ui, value| {
        ui.monospace(value);
    });
}

/// Same packing as [`value_table`], with a caller-supplied value renderer.
pub(crate) fn value_table_with(
    ui: &mut Ui,
    id: &str,
    rows: &[(String, String)],
    mut show_value: impl FnMut(&mut Ui, &str),
) {
    let available = ui.available_width();
    let pairs = pair_columns(available);
    if pairs == 1 {
        pair_column(ui, id, rows, &mut show_value, available);
        return;
    }
    // One grid per pair column, rather than a single 2*pairs grid.  A single
    // grid has to stretch its label columns to spread the pairs across the
    // card, which opens a gap between a label and its own value; separate
    // grids let each label hug its value while the columns themselves are what
    // spread across the width.  Entries stay in row-major reading order.
    let column_width = available / pairs as f32;
    ui.columns(pairs, |columns| {
        for (index, column) in columns.iter_mut().enumerate() {
            let slice = rows
                .iter()
                .skip(index)
                .step_by(pairs)
                .cloned()
                .collect::<Vec<_>>();
            pair_column(
                column,
                &format!("{id}_{index}"),
                &slice,
                &mut show_value,
                column_width,
            );
        }
    });
}

/// One label/value grid: the label hugs its value, and a long value wraps
/// inside the column instead of widening the card.
fn pair_column(
    ui: &mut Ui,
    id: &str,
    rows: &[(String, String)],
    show_value: &mut impl FnMut(&mut Ui, &str),
    width: f32,
) {
    Grid::new(id)
        .num_columns(2)
        .striped(true)
        .max_col_width((width * 0.62).max(80.0))
        .spacing([10.0, 3.0])
        .show(ui, |ui| {
            for (label, value) in rows {
                ui.label(RichText::new(tr(label)).weak());
                show_value(ui, value);
                ui.end_row();
            }
        });
}

/// Physical scalars whose magnitude can be far from unity: small and large
/// values stay readable in scientific notation instead of collapsing to
/// `0.000000`, which would read as an exactly zero measurement.
pub(crate) fn physical_value(value: f64) -> String {
    if !value.is_finite() {
        return format!("{value}");
    }
    let magnitude = value.abs();
    if magnitude == 0.0 {
        "0".to_owned()
    } else if !(1.0e-3..1.0e5).contains(&magnitude) {
        format!("{value:.4e}")
    } else {
        format!("{value:.4}")
    }
}

/// Dimensionless coefficients: five decimals, and scientific notation once a
/// nonzero contribution would otherwise be shown as zero.
pub(crate) fn coefficient_value(value: f64) -> String {
    if !value.is_finite() {
        return format!("{value}");
    }
    let magnitude = value.abs();
    if magnitude != 0.0 && magnitude < 1.0e-3 {
        format!("{value:.3e}")
    } else {
        format!("{value:.5}")
    }
}

/// Counted quantities are integers; a cell or sample count never carries a
/// fractional part.
pub(crate) fn count_value(value: u64) -> String {
    let digits = value.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index) % 3 == 0 {
            grouped.push('\u{202f}');
        }
        grouped.push(digit);
    }
    grouped
}

/// Value string for an optional measurement, keeping "unavailable" honest.
pub(crate) fn optional_value(value: Option<f64>, format: impl Fn(f64) -> String) -> String {
    value.map_or_else(|| tr("Unavailable"), format)
}
