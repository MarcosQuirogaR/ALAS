// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The side view's seat rows as the engine's seat layer wants them, and the
//! class legend above the canvas (outside the drawing, so no text sits over
//! the figure).

use std::hash::{Hash, Hasher};

use alas_payload::layout::ItemKind;
use alas_report::families::geometry::{seat_class_color, SeatProfile};
use egui::{vec2, Color32, RichText, Sense, Ui};

use super::side_view::{SideView, ViewItem};
use crate::views::{tr, tr_fields};

/// Whether `item` is a built seat row (declared point masses are not).
fn is_row(item: &ViewItem) -> bool {
    item.kind == ItemKind::SeatRow
        && item.declared.is_empty()
        && item.mass > 0.0
        && item.seat.is_some()
}

/// The built seat rows of `view`.
pub(super) fn profiles(view: &SideView) -> Vec<SeatProfile> {
    view.items
        .iter()
        .filter(|item| is_row(item))
        .filter_map(|item| {
            let seat = item.seat.as_ref()?;
            Some(SeatProfile {
                x: item.x,
                floor_z: seat.floor_z,
                pan_z: item.z - item.height * 0.5,
                top_z: item.z + item.height * 0.5,
                length: item.length,
                cls: seat.cls,
            })
        })
        .collect()
}

/// A fingerprint of the rows the layer would draw: equal while they are.
pub(super) fn fingerprint(view: &SideView) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for item in view.items.iter().filter(|item| is_row(item)) {
        let floor = item.seat.as_ref().map_or(0.0, |seat| seat.floor_z);
        for value in [item.x, floor, item.z, item.length, item.height] {
            value.to_bits().hash(&mut hasher);
        }
        item.seat.as_ref().map(|seat| seat.cls).hash(&mut hasher);
    }
    hasher.finish()
}

fn colour(cls: &str) -> Color32 {
    let colour = seat_class_color(cls);
    Color32::from_rgb(colour.r, colour.g, colour.b)
}

/// The class name a row's `cls` stands for.
fn class_name(cls: &str) -> String {
    tr(match cls {
        "First" => "First",
        "Business" => "Business",
        "Premium" => "Premium",
        _ => "Economy",
    })
}

/// One legend entry per class that has rows: swatch, name and row count.
pub(super) fn legend(ui: &mut Ui, view: &SideView) {
    let mut counts: Vec<(&'static str, usize)> = Vec::new();
    for cls in ["First", "Business", "Premium", "Economy"] {
        let count = view
            .items
            .iter()
            .filter(|item| is_row(item))
            .filter(|item| {
                item.seat.as_ref().is_some_and(|seat| match cls {
                    "Economy" => !["First", "Business", "Premium"].contains(&seat.cls),
                    other => seat.cls == other,
                })
            })
            .count();
        if count > 0 {
            counts.push((cls, count));
        }
    }
    for (cls, count) in counts {
        let (swatch, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
        ui.painter().rect_filled(swatch, 2.0, colour(cls));
        ui.label(
            RichText::new(format!(
                "{} \u{b7} {}",
                class_name(cls),
                tr_fields("{n} rows", &[("n", count.to_string())])
            ))
            .weak(),
        );
        ui.add_space(6.0);
    }
}
