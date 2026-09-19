// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Single-column card layout helpers for the CFD window.
//!
//! Cards never share a row: unequal cards leave the shorter one trailing
//! empty space.  Instead each card lays its labelled control groups side by
//! side only when every group keeps a readable width, and stacks them
//! otherwise, so the card content adapts to the width it is given.

use crate::views::tr;
use egui::{RichText, Ui};

/// Narrowest width at which a label-plus-editor grid stays readable.  The
/// widest label in these forms ("Upwind startup iterations") plus the shared
/// field width has to fit without clipping the editor.
pub(crate) const GROUP_MIN_WIDTH: f32 = 320.0;
/// Horizontal gap egui inserts between side-by-side group columns.
pub(crate) const GROUP_GAP: f32 = 12.0;
/// Vertical gap between stacked groups.
const GROUP_STACK_GAP: f32 = 8.0;

/// Shared width of every numeric editor, so adjacent forms line up instead of
/// each field hugging its own value.
pub(crate) const FIELD_WIDTH: f32 = 92.0;

/// One labelled control group rendered into a card column: heading, the
/// hover-only explanation for that heading, and the control renderer.
pub(crate) type Group<T> = (&'static str, &'static str, fn(&mut Ui, &mut T));

/// Add a numeric editor at the shared field width.
pub(crate) fn field(ui: &mut Ui, value: egui::DragValue<'_>) -> egui::Response {
    let height = ui.spacing().interact_size.y;
    ui.add_sized([FIELD_WIDTH, height], value)
}

/// Add a numeric editor for a quantity whose magnitude is far from unity.
///
/// The default decimal rendering turns a dynamic viscosity of 1.789e-5 into a
/// box-widening `0.0000179`, and a residual tolerance of 1e-12 into a plain
/// `0.000000`, which reads as an exactly zero setting. Scientific notation
/// keeps the real value visible and the editor at the shared width; typed
/// input still accepts either form.
pub(crate) fn scientific_field(ui: &mut Ui, value: egui::DragValue<'_>) -> egui::Response {
    field(
        ui,
        value
            .custom_formatter(|number, _| format!("{number:.3e}"))
            .custom_parser(|text| text.trim().parse::<f64>().ok()),
    )
}

/// Add a numeric editor whose notation follows the value's own magnitude.
///
/// A setting whose useful range spans both `0.05` and `1e-6` reads badly under
/// either fixed rule: scientific notation makes a five-percent tolerance say
/// `5.000e-2`, and decimals make a micro-tolerance say `0.000001`. This picks
/// per value, so the pressure relative tolerance stays `0.0500` at its preset
/// defaults and still shows a deep override honestly.
pub(crate) fn adaptive_field(ui: &mut Ui, value: egui::DragValue<'_>) -> egui::Response {
    field(
        ui,
        value
            .custom_formatter(|number, _| {
                if number != 0.0 && !(1.0e-3..1.0e5).contains(&number.abs()) {
                    format!("{number:.3e}")
                } else {
                    format!("{number:.4}")
                }
            })
            .custom_parser(|text| text.trim().parse::<f64>().ok()),
    )
}

/// Add a numeric editor at the shared field width, enabled conditionally.
pub(crate) fn field_enabled(
    ui: &mut Ui,
    enabled: bool,
    value: egui::DragValue<'_>,
) -> egui::Response {
    let height = ui.spacing().interact_size.y;
    ui.add_enabled_ui(enabled, |ui| ui.add_sized([FIELD_WIDTH, height], value))
        .inner
}

/// Number of side-by-side columns for `groups` control groups.
///
/// When the widest fitting count would leave a ragged last row, one narrower
/// arrangement that divides the groups evenly is preferred: six groups in a
/// four-column card read better as two rows of three than as a full row of
/// four followed by a half-empty row of two.
pub(crate) fn group_column_count(available_width: f32, groups: usize) -> usize {
    let fitting = ((available_width.max(1.0) + GROUP_GAP) / (GROUP_MIN_WIDTH + GROUP_GAP)).floor();
    let fitting = fitting.clamp(1.0, groups.max(1) as f32) as usize;
    for candidate in [
        fitting,
        fitting.saturating_sub(1),
        fitting.saturating_sub(2),
    ] {
        if candidate >= 2 && groups % candidate == 0 {
            return candidate;
        }
    }
    fitting
}

/// Padding kept between the painted preview box and the drawn outline.
pub(crate) const PREVIEW_PADDING: f32 = 10.0;

/// Height of the airfoil preview strip for the given content width and the
/// section's own thickness-to-chord extent.
///
/// The box tracks the physical aspect ratio, so a 14 % section produces a
/// strip roughly 14 % as tall as it is wide and the outline fills it, instead
/// of a fixed tall rectangle that leaves most of the card empty.  The bounds
/// keep a very thin section legible and stop a very wide window from turning
/// the preview into the whole tab.
pub(crate) fn preview_height(width: f32, aspect: f32) -> f32 {
    let drawable = (width - 2.0 * PREVIEW_PADDING).max(1.0);
    (drawable * aspect.clamp(0.02, 1.0) + 2.0 * PREVIEW_PADDING).clamp(84.0, 200.0)
}

/// Render the groups inside the current card: side by side when each column
/// keeps [`GROUP_MIN_WIDTH`], stacked in order otherwise.  Overflowing groups
/// continue below the first row, column by column.
pub(crate) fn show_groups<T>(ui: &mut Ui, target: &mut T, groups: &[Group<T>]) {
    let columns = group_column_count(ui.available_width(), groups.len());
    if columns == 1 {
        for (index, (heading, help, show)) in groups.iter().enumerate() {
            if index > 0 {
                ui.add_space(GROUP_STACK_GAP);
            }
            group_heading(ui, heading, help);
            show(ui, target);
        }
        return;
    }
    // One `columns` call per row keeps every heading in a row aligned on the
    // same baseline: a single call would let a tall first-row group push the
    // group below it out of step with its neighbours.
    for (row, chunk) in groups.chunks(columns).enumerate() {
        if row > 0 {
            ui.add_space(GROUP_STACK_GAP);
        }
        ui.columns(columns, |columns_ui| {
            for (index, (heading, help, show)) in chunk.iter().enumerate() {
                let column = &mut columns_ui[index];
                group_heading(column, heading, help);
                show(column, target);
            }
        });
    }
}

fn group_heading(ui: &mut Ui, heading: &str, help: &str) {
    let response = ui.label(RichText::new(tr(heading)).strong());
    if !help.is_empty() {
        response.on_hover_text(tr(help));
    }
}
