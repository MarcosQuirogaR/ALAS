// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Editors of the leaf kinds that hold a list of numbers or a table of them.

use alas_config::{Entry, Field};
use egui::{DragValue, RichText, Ui};
use serde_json::Value;

use super::number_text::format_number;

pub(super) fn edit_number_list(ui: &mut Ui, field: &Field, slot: &mut Value) -> bool {
    // Every box carries the field's unit, like a single-value field does; a
    // field shown in a scaled unit keeps its plain boxes.
    let suffix = if super::display_scale::display_scale(field).is_none() {
        super::editors::unit_suffix(field)
    } else {
        String::new()
    };
    let mut nums: Vec<f64> = slot
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_f64()).collect())
        .unwrap_or_default();
    let mut changed = false;
    // The spars are named by their order along the chord, whatever order
    // they are listed in.
    let spar_names: Option<Vec<&'static str>> = (field.name == "spar_chord_fractions").then(|| {
        let mut order: Vec<usize> = (0..nums.len()).collect();
        order.sort_by(|a, b| nums[*a].total_cmp(&nums[*b]));
        let mut names = vec!["Mid spar"; nums.len()];
        if let Some(first) = order.first() {
            names[*first] = "Front spar";
        }
        if let (Some(last), true) = (order.last(), nums.len() > 1) {
            names[*last] = "Rear spar";
        }
        names
    });
    // Top-aligned rows: a centred cross axis put a box a few pixels lower than
    // its neighbour once the first box had set the row height.
    let row = egui::Layout::left_to_right(egui::Align::Min).with_main_wrap(true);
    ui.with_layout(row, |ui| {
        for (index, v) in nums.iter_mut().enumerate() {
            // Equal-width boxes, so a list of positions reads as a row.
            let height = ui.spacing().interact_size.y;
            let mut show_box = |ui: &mut Ui| {
                let drag = DragValue::new(v).speed(0.1).suffix(suffix.clone());
                if ui.add_sized([112.0, height], drag).changed() {
                    changed = true;
                }
            };
            match &spar_names {
                // A caption over each box; the plain lists keep one wrapping row.
                Some(names) => {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(crate::views::tr(names[index])).small().weak());
                        show_box(ui);
                    });
                }
                None => show_box(ui),
            }
        }
    });
    if changed {
        *slot = Value::from(nums);
    }
    changed
}

pub(super) fn edit_tuple_list(ui: &mut Ui, field: &Field, slot: &mut Value) -> bool {
    let mut rows: Vec<Vec<f64>> = slot
        .as_array()
        .map(|a| {
            a.iter()
                .map(|row| {
                    row.as_array()
                        .map(|r| r.iter().filter_map(|v| v.as_f64()).collect())
                        .unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();
    let mut changed = false;
    let columns = rows
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0)
        .max(match &field.entry {
            Entry::Leaf(leaf) => leaf.columns.map_or(0, <[_]>::len),
            Entry::Node(_) => 0,
        });
    if columns > 0 {
        // One aligned grid: every column as wide as its share of the card
        // (tables end at the card's right edge, like the plot above), headings centred over their boxes and
        // every box on the same baseline.
        let spacing = ui.spacing().item_spacing;
        let cell_width = ((ui.available_width() - spacing.x * (columns as f32 - 1.0))
            / columns as f32)
            .max(70.0);
        let height = ui.spacing().interact_size.y;
        // Rows of equal-width cells laid out from this frame's width (a Grid
        // places its columns from the previous frame, and for the one frame in
        // which a scroll bar appears that overshoots the card).
        if let Entry::Leaf(leaf) = &field.entry {
            if let Some(cols) = leaf.columns {
                ui.horizontal(|ui| {
                    for c in cols {
                        ui.add_sized(
                            [cell_width, height],
                            egui::Label::new(RichText::new(crate::views::tr(c)).weak()),
                        );
                    }
                });
            }
        }
        for row in rows.iter_mut() {
            ui.horizontal(|ui| {
                for v in row.iter_mut() {
                    // Shortest rendering, as every other number box shows it.
                    let cell = DragValue::new(v)
                        .speed(0.1)
                        .max_decimals(5)
                        .custom_formatter(|value, _| format_number(value, 5));
                    if ui.add_sized([cell_width, height], cell).changed() {
                        changed = true;
                    }
                }
            });
        }
    }
    if changed {
        *slot = Value::from(rows.into_iter().map(Value::from).collect::<Vec<_>>());
    }
    changed
}
