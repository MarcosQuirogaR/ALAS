// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Setup > Design Space page: the optimizer's search variables, each with
//! an editable initial value and lower/upper bound.
//!
//! A port of the reference desktop app's `DesignSpaceTable`.

use alas_config::DESIGN_VARIABLE_SPECS;
use egui::{DragValue, RichText, ScrollArea, Ui};

use crate::state::AppState;
use crate::views::form::dynamic_form;
use crate::views::tr;

/// A search variable needs room for its three numeric values. Two columns are
/// useful on a desktop, but a third makes the labels and bounds too narrow to
/// scan as one unit.
const MIN_DESIGN_COLUMN_WIDTH: f32 = 480.0;

fn display_name(spec: &alas_config::DesignVariableSpec) -> String {
    let name = if spec.unit == "m" {
        spec.name.strip_suffix("_m").unwrap_or(spec.name)
    } else if spec.unit == "deg" {
        spec.name.strip_suffix("_deg").unwrap_or(spec.name)
    } else {
        spec.name
    };
    name.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            let first = chars
                .next()
                .map(|character| character.to_ascii_uppercase())
                .unwrap_or_default();
            format!("{first}{}", chars.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Render the Design Space page.
pub fn show_design_space_view(state: &mut AppState, ui: &mut Ui) {
    ui.heading(tr("Design Space"));
    ui.label(
        RichText::new(
            tr("The optimizer's search variables. Edit the Initial Value (nominal/starting design) \
             and the Lower/Upper bounds; loading a preset recenters these around its design vector.",
            ),
        )
        .weak(),
    );
    ui.add_space(6.0);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_design_constraints(state, ui);
            ui.add_space(8.0);
            let columns = design_space_column_count(ui.available_width());
            for row in DESIGN_VARIABLE_SPECS.chunks(columns) {
                ui.columns(columns, |column_uis| {
                    for (index, spec) in row.iter().enumerate() {
                        show_variable_editor(state, &mut column_uis[index], spec);
                    }
                });
                ui.add_space(8.0);
            }
        });
}

/// Render the optimizer-facing limits next to the variables they constrain.
///
/// These values remain in the requirements configuration group so the solver
/// receives exactly the same numbers. Only their presentation moves here:
/// keeping one editor for each field avoids two pages disagreeing about a
/// limit while making the relationship to the design vector explicit.
fn show_design_constraints(state: &mut AppState, ui: &mut Ui) {
    let Some(field) = state.schema.field("requirements") else {
        return;
    };
    let alas_config::Entry::Node(node) = &field.entry else {
        return;
    };
    let fields: Vec<alas_config::Field> = node
        .fields
        .iter()
        .filter(|field| field.advanced)
        .cloned()
        .map(|mut field| {
            // This card is already the advanced design-constraint section; a
            // second nested disclosure would put the moved fields out of view.
            field.advanced = false;
            field
        })
        .collect();
    if fields.is_empty() {
        return;
    }

    let error_fields: std::collections::HashSet<String> = state
        .validation_findings
        .iter()
        .filter(|finding| finding.field_path.starts_with("requirements"))
        .filter_map(|finding| finding.field_path.rsplit('.').next())
        .map(str::to_owned)
        .collect();
    let lang = Some(state.language.code());
    let show_help = state.help_verbose;
    let mut edits = Vec::new();
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        egui::CollapsingHeader::new(RichText::new(tr("Design constraints")).strong())
            .id_salt("design_space::constraints")
            .default_open(true)
            .show(ui, |ui| {
                ui.label(
                    RichText::new(tr(
                        "Limits and stability targets used to score candidates. Their values are kept in the Input configuration and are not optimizer variables.",
                    ))
                    .weak()
                    .small(),
                );
                if let Some(values) = state.group_mut("requirements") {
                    edits.extend(dynamic_form(
                        ui,
                        &fields,
                        values,
                        &error_fields,
                        lang,
                        show_help,
                    ));
                }
            });
    });
    if !edits.is_empty() {
        state.on_config_modified();
        for edit in edits {
            state.note_parameter_modified(edit.label, edit.value);
        }
    }
}

fn show_variable_editor(state: &mut AppState, ui: &mut Ui, spec: &alas_config::DesignVariableSpec) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr(&display_name(spec))).strong())
                .on_hover_text(tr(spec.description));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(if spec.unit.is_empty() { "-" } else { spec.unit })
                        .weak()
                        .small(),
                );
            });
        });
        ui.add_space(2.0);

        let decimals = spec.decimals.max(0) as usize;
        let mut values = [
            state
                .design_values
                .get(spec.name)
                .copied()
                .unwrap_or(spec.default),
            state
                .bounds
                .get(spec.name)
                .map(|bounds| bounds.0)
                .unwrap_or(spec.lower),
            state
                .bounds
                .get(spec.name)
                .map(|bounds| bounds.1)
                .unwrap_or(spec.upper),
        ];
        let mut changed = false;
        let labels = ["Initial value", "Lower bound", "Upper bound"];
        ui.columns(3, |columns| {
            for (index, label) in labels.into_iter().enumerate() {
                let column = &mut columns[index];
                column.label(RichText::new(tr(label)).weak().small());
                let width = column.available_width().max(72.0);
                changed |= column
                    .add_sized(
                        [width, column.spacing().interact_size.y],
                        DragValue::new(&mut values[index])
                            .speed(0.01)
                            .max_decimals(decimals),
                    )
                    .changed();
            }
        });
        if changed {
            state.design_values.insert(spec.name.to_owned(), values[0]);
            state
                .bounds
                .insert(spec.name.to_owned(), (values[1], values[2]));
            state.on_config_modified();
            state.note_parameter_modified(tr(&display_name(spec)), tr("design-space bounds"));
        }
    });
}

fn design_space_column_count(available_width: f32) -> usize {
    if available_width >= MIN_DESIGN_COLUMN_WIDTH * 2.0 {
        2
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::{design_space_column_count, display_name, MIN_DESIGN_COLUMN_WIDTH};
    use alas_config::DESIGN_VARIABLE_SPECS;

    #[test]
    fn design_space_labels_use_human_names_and_keep_units_separate() {
        let sweep = DESIGN_VARIABLE_SPECS
            .iter()
            .find(|spec| spec.name == "sweep_deg")
            .expect("sweep variable");
        let tail = DESIGN_VARIABLE_SPECS
            .iter()
            .find(|spec| spec.name == "tail_scale")
            .expect("tail scale variable");

        assert_eq!(display_name(sweep), "Sweep");
        assert_eq!(sweep.unit, "deg");
        assert_eq!(display_name(tail), "Tail Scale");
        assert_eq!(tail.unit, "-");
    }

    #[test]
    fn design_space_uses_two_columns_only_when_each_editor_remains_readable() {
        assert_eq!(
            design_space_column_count(MIN_DESIGN_COLUMN_WIDTH * 2.0 - 1.0),
            1
        );
        assert_eq!(design_space_column_count(MIN_DESIGN_COLUMN_WIDTH * 2.0), 2);
    }
}
