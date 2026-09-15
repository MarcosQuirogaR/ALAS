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

/// Narrowest width at which a label-plus-editor grid stays readable.
pub(crate) const GROUP_MIN_WIDTH: f32 = 300.0;
/// Horizontal gap egui inserts between side-by-side group columns.
pub(crate) const GROUP_GAP: f32 = 12.0;
/// Vertical gap between stacked groups.
const GROUP_STACK_GAP: f32 = 8.0;

/// One labelled control group rendered into a card column.
pub(crate) type Group<T> = (&'static str, fn(&mut Ui, &mut T));

/// Number of side-by-side columns for `groups` control groups.
pub(crate) fn group_column_count(available_width: f32, groups: usize) -> usize {
    let fitting = ((available_width.max(1.0) + GROUP_GAP) / (GROUP_MIN_WIDTH + GROUP_GAP)).floor();
    fitting.clamp(1.0, groups.max(1) as f32) as usize
}

/// Height of the airfoil preview strip for the given content width.
pub(crate) fn preview_height(width: f32) -> f32 {
    (width * 0.24).clamp(120.0, 220.0)
}

/// Render the groups inside the current card: side by side when each column
/// keeps [`GROUP_MIN_WIDTH`], stacked in order otherwise.  Overflowing groups
/// continue below the first row, column by column.
pub(crate) fn show_groups<T>(ui: &mut Ui, target: &mut T, groups: &[Group<T>]) {
    let columns = group_column_count(ui.available_width(), groups.len());
    if columns == 1 {
        for (index, (heading, show)) in groups.iter().enumerate() {
            if index > 0 {
                ui.add_space(GROUP_STACK_GAP);
            }
            group_heading(ui, heading);
            show(ui, target);
        }
        return;
    }
    ui.columns(columns, |columns_ui| {
        for (index, (heading, show)) in groups.iter().enumerate() {
            let column = &mut columns_ui[index % columns];
            if index >= columns {
                column.add_space(GROUP_STACK_GAP);
            }
            group_heading(column, heading);
            show(column, target);
        }
    });
}

fn group_heading(ui: &mut Ui, heading: &str) {
    ui.label(RichText::new(tr(heading)).strong());
}
