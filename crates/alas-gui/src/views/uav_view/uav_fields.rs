// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reusable editing and compact-value rows for the UAV workflow.

use alas_uav::optimizer::VariableBounds;
use egui::{DragValue, RichText, TextStyle, Ui};

use super::super::tr;
use crate::views::form::{drag_box, label_row, CellGrid};

/// One field of a UAV form: a cell of the form grid, its caption over a box
/// that fills the cell.
fn field(grid: &mut CellGrid, ui: &mut Ui, label: &str, drag: DragValue<'_>) {
    grid.cell(ui, |ui, width| {
        label_row(ui, RichText::new(tr(label)).strong(), |_| {});
        drag_box(ui, width, drag);
    });
}

pub(super) fn value(grid: &mut CellGrid, ui: &mut Ui, label: &str, value: &mut f64, suffix: &str) {
    field(grid, ui, label, numeric_drag(value, suffix));
}

/// A search range: the lower and the upper bound side by side, filling the
/// cell together.
pub(super) fn bounds_row(
    grid: &mut CellGrid,
    ui: &mut Ui,
    label: &str,
    bounds: &mut VariableBounds,
    suffix: &str,
) {
    grid.cell(ui, |ui, width| {
        label_row(ui, RichText::new(tr(label)).strong(), |_| {});
        ui.horizontal(|ui| {
            let to = tr("to");
            let word = ui
                .painter()
                .layout_no_wrap(
                    to.clone(),
                    TextStyle::Body.resolve(ui.style()),
                    egui::Color32::WHITE,
                )
                .size()
                .x;
            let each = ((width - word - 2.0 * ui.spacing().item_spacing.x) * 0.5).max(1.0);
            drag_box(ui, each, numeric_drag(&mut bounds.minimum, suffix));
            ui.label(to);
            drag_box(ui, each, numeric_drag(&mut bounds.maximum, suffix));
        });
    });
}

pub(super) fn integer(grid: &mut CellGrid, ui: &mut Ui, label: &str, value: &mut u16) {
    field(grid, ui, label, DragValue::new(value).range(1..=u16::MAX));
}

pub(super) fn usize_value(grid: &mut CellGrid, ui: &mut Ui, label: &str, value: &mut usize) {
    field(grid, ui, label, DragValue::new(value).range(1..=100_000));
}

/// The integer seed of the deterministic search.
pub(super) fn seed_value(grid: &mut CellGrid, ui: &mut Ui, label: &str, seed: &mut u64) {
    field(grid, ui, label, DragValue::new(seed).speed(1));
}

pub(super) fn result_value(ui: &mut Ui, label: &str, value: f64) {
    result_measure(ui, label, value, "");
}

/// Render a measured result without trailing precision that the model does not support.
pub(super) fn result_measure(ui: &mut Ui, label: &str, value: f64, suffix: &str) {
    ui.label(RichText::new(tr(label)).strong());
    let text = if suffix.is_empty() {
        compact_number(value)
    } else {
        format!("{} {suffix}", compact_number(value))
    };
    ui.monospace(text);
    ui.end_row();
}

/// Compact significant displayed precision while retaining small physical values.
pub(super) fn compact_number(value: f64) -> String {
    if !value.is_finite() {
        return "-".to_owned();
    }
    let magnitude = value.abs();
    let decimals = if magnitude >= 1_000.0 {
        0
    } else if magnitude >= 100.0 {
        1
    } else if magnitude >= 1.0 {
        3
    } else if magnitude >= 0.01 {
        4
    } else {
        6
    };
    let rendered = format!("{value:.decimals$}");
    if decimals == 0 {
        return rendered;
    }
    rendered
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}

fn numeric_drag<'a>(value: &'a mut f64, suffix: &str) -> DragValue<'a> {
    let step = number_step(*value);
    DragValue::new(value)
        .speed(step)
        .max_decimals(input_decimals(suffix))
        .suffix(unit_suffix(suffix))
}

fn input_decimals(suffix: &str) -> usize {
    match suffix {
        "s" | "kg" | "N" | "A" | "W" | "V" => 2,
        "m" | "m2" | "m/s" | "deg" | "kg/m3" => 3,
        _ => 4,
    }
}

fn number_step(value: f64) -> f64 {
    match value.abs() {
        magnitude if magnitude >= 1_000.0 => 1.0,
        magnitude if magnitude >= 100.0 => 0.1,
        _ => 0.01,
    }
}

/// The same display-unit policy the schema forms use, so `m2` and `kg/m3`
/// read as `m<superscript 2>` and `kg/m<superscript 3>` here too.
fn unit_suffix(suffix: &str) -> String {
    let unit = crate::views::form::display_unit(suffix);
    if unit.is_empty() {
        String::new()
    } else {
        format!(" {unit}")
    }
}

#[cfg(test)]
mod tests {
    use super::compact_number;

    #[test]
    fn compact_numbers_remove_unhelpful_decimal_overflow() {
        assert_eq!(compact_number(12.345678), "12.346");
        assert_eq!(compact_number(1_234.567), "1235");
        assert_eq!(compact_number(1_000.0), "1000");
        assert_eq!(compact_number(0.0001234), "0.000123");
    }
}
