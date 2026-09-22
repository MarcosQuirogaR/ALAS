// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reusable editing and compact-value rows for the UAV workflow.

use alas_uav::optimizer::VariableBounds;
use egui::{DragValue, RichText, TextEdit, Ui};

use super::super::tr;

pub(super) fn value(ui: &mut Ui, label: &str, value: &mut f64, suffix: &str) {
    ui.label(RichText::new(tr(label)).strong());
    ui.add_sized(
        [numeric_width(ui), ui.spacing().interact_size.y],
        numeric_drag(value, suffix),
    );
    ui.end_row();
}

pub(super) fn bounds_row(ui: &mut Ui, label: &str, bounds: &mut VariableBounds, suffix: &str) {
    ui.label(RichText::new(tr(label)).strong());
    ui.horizontal(|ui| {
        let width = ((numeric_width(ui) - 28.0) * 0.5).clamp(74.0, 126.0);
        ui.add_sized(
            [width, ui.spacing().interact_size.y],
            numeric_drag(&mut bounds.minimum, suffix),
        );
        ui.label(tr("to"));
        ui.add_sized(
            [width, ui.spacing().interact_size.y],
            numeric_drag(&mut bounds.maximum, suffix),
        );
    });
    ui.end_row();
}

pub(super) fn integer(ui: &mut Ui, label: &str, value: &mut u16) {
    ui.label(RichText::new(tr(label)).strong());
    ui.add_sized(
        [numeric_width(ui), ui.spacing().interact_size.y],
        DragValue::new(value).range(1..=u16::MAX),
    );
    ui.end_row();
}

pub(super) fn usize_value(ui: &mut Ui, label: &str, value: &mut usize) {
    ui.label(RichText::new(tr(label)).strong());
    ui.add_sized(
        [numeric_width(ui), ui.spacing().interact_size.y],
        DragValue::new(value).range(1..=100_000),
    );
    ui.end_row();
}

pub(super) fn text_value(ui: &mut Ui, label: &str, value: &mut String) {
    ui.label(RichText::new(tr(label)).strong());
    ui.add_sized(
        [numeric_width(ui), ui.spacing().interact_size.y],
        TextEdit::singleline(value),
    );
    ui.end_row();
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

fn numeric_width(ui: &Ui) -> f32 {
    ui.available_width().clamp(112.0, 220.0)
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
