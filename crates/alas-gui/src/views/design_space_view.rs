// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The Setup > Design Space page: the optimizer's search variables, each with
//! an editable initial value and lower/upper bound.
//!
//! A port of the reference desktop app's `DesignSpaceTable`.

use alas_config::{DesignMode, VariableEnvelope, DESIGN_VARIABLE_SPECS};
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
            tr("The starting design and the optimization choice on Inputs set the study. Each row shows the starting design and the limits handed to the optimizer."),
        )
        .weak(),
    );
    ui.add_space(6.0);
    let mode = state.design_mode();
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        show_design_mode_settings(state, ui, mode);
    });
    ui.add_space(8.0);

    // Fixed rows are enforced at the view boundary as well as immediately
    // before a run. This keeps the editor honest when a user returns from an
    // advanced page after changing a requirement or loading a file.
    state.enforce_design_space_fixed_variables();
    let config = state.typed_config().unwrap_or_default();
    let nominal = state.current_design().unwrap_or_default();
    let envelopes = config.optimizer.design_space.envelope(&nominal);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            show_design_constraints(state, ui);
            ui.add_space(8.0);
            let columns = design_space_column_count(ui.available_width());
            for row in DESIGN_VARIABLE_SPECS.chunks(columns) {
                ui.columns(columns, |column_uis| {
                    for (index, spec) in row.iter().enumerate() {
                        if let Some(envelope) = envelopes.iter().find(|e| e.name == spec.name) {
                            show_variable_editor(state, &mut column_uis[index], spec, envelope);
                        }
                    }
                });
                ui.add_space(8.0);
            }
        });
}

/// The names shown in the product-level mode selector. The serialized values
/// remain the `DesignMode` snake-case tokens owned by `alas-config`.
pub(crate) fn design_mode_display_name(mode: DesignMode) -> String {
    tr(match mode {
        DesignMode::CleanSheet => "New aircraft",
        DesignMode::ReferenceAdaptation => "Adapt reference",
        DesignMode::BaselineSandbox => "Analyze reference",
    })
}

fn show_design_mode_settings(state: &mut AppState, ui: &mut Ui, mode: DesignMode) {
    let fields = state
        .schema
        .field("optimizer")
        .and_then(|field| match &field.entry {
            alas_config::Entry::Node(node) => node
                .fields
                .iter()
                .find(|field| field.name == "design_space"),
            alas_config::Entry::Leaf(_) => None,
        })
        .and_then(|field| match &field.entry {
            alas_config::Entry::Node(node) => Some(
                node.fields
                    .iter()
                    .filter(|field| match mode {
                        DesignMode::CleanSheet => field.name == "fuselage_sized_by_cabin",
                        DesignMode::ReferenceAdaptation => matches!(
                            field.name,
                            "reference_fixed_variables"
                                | "reference_fraction_half_width"
                                | "reference_angle_half_width_deg"
                                | "reference_shift_half_width_m"
                                | "reference_bump_half_width"
                        ),
                        DesignMode::BaselineSandbox => false,
                    })
                    .cloned()
                    .map(|mut field| {
                        // The mode card already provides the hierarchy; these
                        // controls should not acquire another Advanced fold.
                        field.advanced = false;
                        field
                    })
                    .collect::<Vec<_>>(),
            ),
            alas_config::Entry::Leaf(_) => None,
        })
        .unwrap_or_default();

    if fields.is_empty() {
        if mode == DesignMode::BaselineSandbox {
            ui.label(
                RichText::new(tr(
                    "All design variables are fixed at the selected reference values for this run.",
                ))
                .weak()
                .small(),
            );
        }
        return;
    }

    ui.add_space(6.0);
    let title = match mode {
        DesignMode::CleanSheet => "Clean-sheet options",
        DesignMode::ReferenceAdaptation => "Reference envelope controls",
        DesignMode::BaselineSandbox => "Baseline options",
    };
    ui.label(RichText::new(tr(title)).strong());
    let lang = Some(state.language.code());
    let show_help = state.help_verbose;
    let error_fields: std::collections::HashSet<String> = state
        .validation_findings
        .iter()
        .filter(|finding| finding.field_path.starts_with("optimizer.design_space"))
        .filter_map(|finding| finding.field_path.rsplit('.').next())
        .map(str::to_owned)
        .collect();
    let Some(optimizer) = state.group_mut("optimizer") else {
        return;
    };
    let Some(object) = optimizer.as_object_mut() else {
        return;
    };
    let values = object
        .entry("design_space".to_owned())
        .or_insert_with(|| serde_json::json!({}));
    let edits = dynamic_form(ui, &fields, values, &error_fields, lang, show_help);
    if !edits.is_empty() {
        // Changing a window or the clean-sheet cabin-sizing policy changes the
        // declared envelope. Rebuild run bounds from that same source.
        state.reset_design_space_bounds_to_mode();
        state.on_config_modified();
        for edit in edits {
            state.note_parameter_modified(edit.label, edit.value);
        }
    }
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

fn show_variable_editor(
    state: &mut AppState,
    ui: &mut Ui,
    spec: &alas_config::DesignVariableSpec,
    envelope: &VariableEnvelope,
) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(RichText::new(tr(&display_name(spec))).strong())
                .on_hover_text(tr(spec.description));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(tr(if envelope.fixed { "Fixed" } else { "Mutable" }))
                        .weak()
                        .small(),
                );
                ui.label(
                    RichText::new(if spec.unit.is_empty() { "-" } else { spec.unit })
                        .weak()
                        .small(),
                );
            });
        });
        ui.add_space(2.0);

        let decimals = spec.decimals.max(0) as usize;
        let requested_bounds = state
            .bounds
            .get(spec.name)
            .copied()
            .unwrap_or((envelope.lower, envelope.upper));
        let lower = if envelope.fixed {
            envelope.lower
        } else {
            requested_bounds.0.max(envelope.lower).min(envelope.upper)
        };
        let upper = if envelope.fixed {
            envelope.upper
        } else {
            requested_bounds.1.min(envelope.upper).max(lower)
        };
        let mut values = [
            state
                .design_values
                .get(spec.name)
                .copied()
                .unwrap_or(envelope.nominal)
                .clamp(lower, upper),
            lower,
            upper,
        ];
        let mut changed = false;
        let labels = ["Initial value", "Lower bound", "Upper bound"];
        ui.columns(3, |columns| {
            for (index, label) in labels.into_iter().enumerate() {
                let column = &mut columns[index];
                column.label(RichText::new(tr(label)).weak().small());
                let width = column.available_width().max(72.0);
                let height = column.spacing().interact_size.y;
                let response = column.add_enabled_ui(!envelope.fixed, |ui| {
                    ui.add_sized(
                        [width, height],
                        DragValue::new(&mut values[index])
                            .speed(0.01)
                            .max_decimals(decimals),
                    )
                });
                changed |= response.inner.changed();
            }
        });
        if changed && !envelope.fixed {
            if values[1] > values[2] {
                values.swap(1, 2);
            }
            values[0] = values[0].clamp(values[1], values[2]);
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
    use super::{
        design_mode_display_name, design_space_column_count, display_name, MIN_DESIGN_COLUMN_WIDTH,
    };
    use crate::state::AppState;
    use alas_config::{DesignMode, DESIGN_VARIABLE_SPECS};

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

    #[test]
    fn product_mode_labels_are_stable_and_baseline_is_distinct() {
        assert_eq!(
            design_mode_display_name(DesignMode::CleanSheet),
            "New aircraft"
        );
        assert_eq!(
            design_mode_display_name(DesignMode::ReferenceAdaptation),
            "Adapt reference"
        );
        assert_eq!(
            design_mode_display_name(DesignMode::BaselineSandbox),
            "Analyze reference"
        );
    }

    #[test]
    fn selecting_reference_mode_writes_the_typed_config_and_fixes_reference_rows() {
        let mut state = AppState::default();
        state.set_design_mode(DesignMode::ReferenceAdaptation);

        assert_eq!(state.design_mode(), DesignMode::ReferenceAdaptation);
        assert_eq!(
            state
                .config_values
                .pointer("/optimizer/design_space/mode")
                .and_then(serde_json::Value::as_str),
            Some("reference_adaptation")
        );
        let config = state.typed_config().expect("typed config");
        let fixed = config.optimizer.design_space.fixed_variable_names();
        for name in fixed {
            let (lower, upper) = state.bounds[name];
            assert_eq!(lower, upper, "{name} must stay fixed in reference mode");
        }
    }
}
