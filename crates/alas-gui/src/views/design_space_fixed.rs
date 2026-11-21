// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The multi-select of design variables held at the reference value in
//! reference-adaptation mode. It edits the same comma-separated identifier
//! list the configuration stores.

use alas_config::DESIGN_VARIABLE_SPECS;
use egui::{RichText, Ui};

use super::display_name;
use crate::views::{tr, tr_label};

/// The identifiers of the comma-separated fixed list, trimmed, in order.
fn fixed_variable_names(list: &str) -> Vec<&str> {
    list.split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect()
}

/// Rewrite the fixed list with `name` added or removed. Known variables are
/// stored in the order of [`DESIGN_VARIABLE_SPECS`]; an identifier the specs do
/// not know (typed in a configuration file) is kept after them.
pub(super) fn toggle_fixed_variable(list: &str, name: &str, fixed: bool) -> String {
    let current = fixed_variable_names(list);
    let mut known: Vec<&str> = DESIGN_VARIABLE_SPECS
        .iter()
        .map(|spec| spec.name)
        .filter(|candidate| {
            if *candidate == name {
                fixed
            } else {
                current.contains(candidate)
            }
        })
        .collect();
    known.extend(
        current
            .iter()
            .copied()
            .filter(|item| DESIGN_VARIABLE_SPECS.iter().all(|spec| spec.name != *item)),
    );
    known.join(", ")
}

/// Smallest width of one checklist column: the longest variable label plus its
/// check box.
const FIXED_COLUMN_WIDTH: f32 = 200.0;

/// The multi-select of design variables held at the reference value. Returns
/// the new list when a box was toggled. The stored value is the same
/// comma-separated identifier list as before.
pub(super) fn show_fixed_variable_picker(ui: &mut Ui, list: &str) -> Option<String> {
    ui.label(RichText::new(tr_label("Fixed variables (reference mode)")).small());
    ui.add_space(2.0);
    let total = DESIGN_VARIABLE_SPECS.len();
    let wide = ((ui.available_width() / FIXED_COLUMN_WIDTH).floor() as usize).clamp(1, 8);
    // Balanced columns, filled down: 17 items in five columns are 4, 4, 3, 3, 3, not 5, 5, 5, 2.
    let rows = total.div_ceil(wide).max(1);
    let columns = total.div_ceil(rows).max(1);
    let (base, extra) = (total / columns, total % columns);
    let current = fixed_variable_names(list);
    let mut result = None;
    egui::Grid::new("design_space::fixed_variables")
        .num_columns(columns)
        .spacing(crate::theme::space::GRID_SPACING)
        .min_col_width(FIXED_COLUMN_WIDTH - 20.0)
        .show(ui, |ui| {
            for row in 0..rows {
                for column in 0..columns {
                    let height = base + usize::from(column < extra);
                    if row >= height {
                        ui.label("");
                        continue;
                    }
                    let index = column * base + column.min(extra) + row;
                    let spec = &DESIGN_VARIABLE_SPECS[index];
                    let mut fixed = current.contains(&spec.name);
                    if crate::checkbox::checkbox(ui, &mut fixed, tr(&display_name(spec)))
                        .on_hover_text(tr(spec.description))
                        .changed()
                    {
                        result = Some(toggle_fixed_variable(list, spec.name, fixed));
                    }
                }
                ui.end_row();
            }
        });
    result
}

#[cfg(test)]
mod tests {
    use super::toggle_fixed_variable;

    #[test]
    fn toggling_a_fixed_variable_keeps_spec_order_and_unknown_names() {
        assert_eq!(
            toggle_fixed_variable("tail_x_shift_m, fuselage_length_m", "span_m", true),
            "span_m, fuselage_length_m, tail_x_shift_m"
        );
        assert_eq!(
            toggle_fixed_variable("fuselage_length_m, legacy_name", "fuselage_length_m", false),
            "legacy_name"
        );
        assert_eq!(toggle_fixed_variable("", "tail_scale", true), "tail_scale");
    }
}
