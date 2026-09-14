// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared field editors for the Parameter Panel and the Discipline Windows.
//!
//! One editor per field kind, each committing through the same path: the
//! value is checked against the field's validity domain, recorded in the
//! session undo history (a pointer drag on a numeric control is one
//! transaction), written to the edit buffers, and followed by the sandbox
//! model-change bookkeeping. Every editor shows the unit and the valid range
//! and offers a reset to the AVE reference value.

use egui::{ComboBox, DragValue, RichText, Ui};
use serde_json::Value;

use crate::state::AppState;
use crate::views::{tr, tr_fields};

use super::fields::{self, FieldKind, SandboxField};

/// How an edit reached the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    /// A typed or clicked change.
    Committed,
    /// The first frame of a pointer drag.
    DragStarted,
    /// A drag frame.
    Dragging,
    /// The pointer was released.
    DragStopped,
}

/// Apply `value` to `field` through the undo, domain and revision path.
///
/// Returns whether the model changed.
pub fn commit_field(
    state: &mut AppState,
    field: &SandboxField,
    value: Value,
    gesture: Gesture,
) -> bool {
    if !fields::value_in_domain(field, &value) {
        state.sandbox.rejected_edit = Some(tr_fields(
            "{label} must stay between {min} and {max} {unit}; the edit was not applied.",
            &[
                ("label", tr(&field.label)),
                ("min", format!("{}", field.min)),
                ("max", format!("{}", field.max)),
                ("unit", field.unit.clone()),
            ],
        ));
        return false;
    }
    let before = state.edit_snapshot();
    match gesture {
        Gesture::DragStarted => state.sandbox.undo.begin_transaction(before),
        Gesture::Committed => state.sandbox.undo.record(before),
        Gesture::Dragging | Gesture::DragStopped => {}
    }
    let current = fields::read_value(field, &state.config_values, &state.design_values);
    let changed = current != value;
    if changed {
        fields::write_value(
            field,
            value.clone(),
            &mut state.config_values,
            &mut state.design_values,
        );
        state.on_sandbox_model_changed();
        state.note_parameter_modified(tr(&field.label), display_value(field, &value));
    }
    if gesture == Gesture::DragStopped {
        let now = state.edit_snapshot();
        let differs = state.sandbox.undo.transaction_differs(&now);
        state.sandbox.undo.commit_transaction(differs);
    }
    changed
}

/// Restore one field to its AVE reference value.
pub fn reset_field_to_reference(state: &mut AppState, field: &SandboxField) {
    let Some(value) = state.sandbox.reference.as_ref().map(|r| r.value_of(field)) else {
        return;
    };
    commit_field(state, field, value, Gesture::Committed);
}

/// Restore every field of `group` and the fields that depend on them, as
/// one undo step.
pub fn reset_group_to_reference(state: &mut AppState, group: &[SandboxField]) {
    let all = state.sandbox.fields.clone();
    let mut targets: Vec<SandboxField> = group.to_vec();
    for field in group {
        for dependent in field.dependents {
            if !targets.iter().any(|t| t.id == *dependent) {
                if let Some(found) = all.iter().find(|f| f.id == *dependent) {
                    targets.push(found.clone());
                }
            }
        }
    }
    let Some(reference) = state.sandbox.reference.as_ref() else {
        return;
    };
    let values: Vec<(SandboxField, Value)> = targets
        .into_iter()
        .map(|field| {
            let value = reference.value_of(&field);
            (field, value)
        })
        .collect();
    let before = state.edit_snapshot();
    let mut changed = false;
    for (field, value) in &values {
        let current = fields::read_value(field, &state.config_values, &state.design_values);
        if current != *value {
            fields::write_value(
                field,
                value.clone(),
                &mut state.config_values,
                &mut state.design_values,
            );
            changed = true;
        }
    }
    if changed {
        state.sandbox.undo.record(before);
        state.on_sandbox_model_changed();
        state.note_parameter_modified(tr("Group reset"), tr("AVE reference values"));
    }
}

fn display_value(field: &SandboxField, value: &Value) -> String {
    match value {
        Value::Number(n) => format!("{n} {}", field.unit),
        Value::String(s) => s.clone(),
        Value::Null => tr("unset"),
        other => other.to_string(),
    }
}

fn range_hint(field: &SandboxField) -> String {
    tr_fields(
        "Valid range: {min} to {max} {unit}",
        &[
            ("min", format!("{}", field.min)),
            ("max", format!("{}", field.max)),
            ("unit", field.unit.clone()),
        ],
    )
}

fn gesture_of(response: &egui::Response) -> Option<Gesture> {
    if response.drag_started() {
        Some(Gesture::DragStarted)
    } else if response.drag_stopped() {
        Some(Gesture::DragStopped)
    } else if response.dragged() {
        Some(Gesture::Dragging)
    } else if response.changed() {
        Some(Gesture::Committed)
    } else {
        None
    }
}

fn numeric_editor(state: &mut AppState, ui: &mut Ui, field: &SandboxField, mut value: f64) {
    let step = if field.decimals == 0 {
        1.0
    } else {
        10f64.powi(-(field.decimals as i32))
    };
    let response = ui.add_sized(
        [ui.available_width().max(90.0), ui.spacing().interact_size.y],
        DragValue::new(&mut value)
            .range(field.min..=field.max)
            .speed(step)
            .max_decimals(field.decimals)
            .suffix(format!(" {}", field.unit)),
    );
    let response = response.on_hover_text(range_hint(field));
    if let Some(gesture) = gesture_of(&response) {
        let value = if field.kind == FieldKind::Int {
            Value::from(value.round() as i64)
        } else {
            Value::from(value)
        };
        commit_field(state, field, value, gesture);
    }
}

fn optional_editor(state: &mut AppState, ui: &mut Ui, field: &SandboxField, current: &Value) {
    let mut set = !current.is_null();
    ui.horizontal(|ui| {
        if ui
            .checkbox(&mut set, tr("Set"))
            .on_hover_text(tr("Unset leaves the derived value in force."))
            .changed()
        {
            let value = if set {
                Value::from(field.min.max(0.0).min(field.max))
            } else {
                Value::Null
            };
            commit_field(state, field, value, Gesture::Committed);
        }
        if let Some(value) = current.as_f64() {
            numeric_editor(state, ui, field, value);
        }
    });
}

fn airfoil_editor(state: &mut AppState, ui: &mut Ui, field: &SandboxField, current: &Value) {
    let mut text = current.as_str().unwrap_or_default().to_owned();
    ui.horizontal(|ui| {
        let response = ui.add(egui::TextEdit::singleline(&mut text).desired_width(140.0));
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            commit_field(
                state,
                field,
                Value::String(text.clone()),
                Gesture::Committed,
            );
        }
        ui.menu_button("v", |ui| {
            let filter = text.to_ascii_lowercase();
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    for name in alas_geom::airfoil_library::AirfoilLibrary::get_available_airfoils()
                        .into_iter()
                        .filter(|n| filter.is_empty() || n.to_ascii_lowercase().contains(&filter))
                        .take(300)
                    {
                        if ui.selectable_label(text == name, name).clicked() {
                            commit_field(
                                state,
                                field,
                                Value::String(name.to_owned()),
                                Gesture::Committed,
                            );
                            ui.close_menu();
                        }
                    }
                });
        })
        .response
        .on_hover_text(tr(
            "Choose a library airfoil; dragging never deforms a profile.",
        ));
    });
}

fn engine_editor(state: &mut AppState, ui: &mut Ui, field: &SandboxField, current: &Value) {
    let current = current.as_str().unwrap_or_default().to_owned();
    let names = state.engine_names.clone();
    let mut chosen = None;
    ComboBox::from_id_salt(("sandbox_engine", &field.id))
        .width(ui.available_width())
        .selected_text(&current)
        .show_ui(ui, |ui| {
            for name in &names {
                if ui.selectable_label(*name == current, name).clicked() {
                    chosen = Some(name.clone());
                }
            }
        });
    if let Some(name) = chosen {
        let before = state.edit_snapshot();
        state.sandbox.undo.record(before);
        state.set_engine(&name);
    }
}

fn list_editor(
    state: &mut AppState,
    ui: &mut Ui,
    field: &SandboxField,
    current: &Value,
    labels: Option<[&str; 3]>,
) {
    let mut items: Vec<f64> = current
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_f64).collect())
        .unwrap_or_default();
    let mut gesture = None;
    ui.horizontal_wrapped(|ui| {
        for (index, item) in items.iter_mut().enumerate() {
            if let Some(labels) = labels {
                if let Some(label) = labels.get(index) {
                    ui.label(RichText::new(*label).weak().small());
                }
            }
            let response = ui.add(
                DragValue::new(item)
                    .range(field.min..=field.max)
                    .speed(10f64.powi(-(field.decimals as i32)))
                    .max_decimals(field.decimals),
            );
            if let Some(g) = gesture_of(&response) {
                gesture = Some(g);
            }
        }
        if field.kind == FieldKind::FloatList {
            if ui
                .small_button("+")
                .on_hover_text(tr("Add a mirrored engine pair"))
                .clicked()
            {
                let outermost = items.iter().fold(0.0f64, |m, v| m.max(v.abs()));
                items.push(outermost + 3.0);
                items.push(-(outermost + 3.0));
                gesture = Some(Gesture::Committed);
            }
            if items.len() >= 2
                && ui
                    .small_button("-")
                    .on_hover_text(tr("Remove the outermost engine pair"))
                    .clicked()
            {
                items.truncate(items.len() - 2);
                gesture = Some(Gesture::Committed);
            }
        }
    });
    if let Some(gesture) = gesture {
        commit_field(state, field, Value::from(items), gesture);
    }
}

fn pair_list_editor(state: &mut AppState, ui: &mut Ui, field: &SandboxField, current: &Value) {
    let mut rows: Vec<[f64; 2]> = current
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    let row = row.as_array()?;
                    Some([row.first()?.as_f64()?, row.get(1)?.as_f64()?])
                })
                .collect()
        })
        .unwrap_or_default();
    let mut gesture = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr("x-station [m]")).weak().small());
        ui.label(RichText::new(tr("radius fraction [0-1]")).weak().small());
    });
    for row in rows.iter_mut() {
        ui.horizontal(|ui| {
            for (index, value) in row.iter_mut().enumerate() {
                let (lo, hi) = if index == 0 {
                    (0.0, field.max)
                } else {
                    (0.0, 1.0)
                };
                let response = ui.add(
                    DragValue::new(value)
                        .range(lo..=hi)
                        .speed(0.01)
                        .max_decimals(3),
                );
                if let Some(g) = gesture_of(&response) {
                    gesture = Some(g);
                }
            }
        });
    }
    if let Some(gesture) = gesture {
        let value = Value::from(
            rows.into_iter()
                .map(|r| Value::from(r.to_vec()))
                .collect::<Vec<_>>(),
        );
        commit_field(state, field, value, gesture);
    }
}

/// Render one field: label with help and range on hover, the editor, and
/// the reset to the AVE reference.
pub fn show_field(state: &mut AppState, ui: &mut Ui, field: &SandboxField) {
    let current = fields::read_value(field, &state.config_values, &state.design_values);
    let reference = state.sandbox.reference.as_ref().map(|r| r.value_of(field));
    let leaf = field.id.rsplit('.').next().unwrap_or("");
    let has_error = state
        .validation_findings
        .iter()
        .any(|f| f.field_path.rsplit('.').next() == Some(leaf));
    ui.horizontal(|ui| {
        let mut text = RichText::new(tr(&field.label)).size(12.5);
        if has_error {
            text = text.color(ui.visuals().error_fg_color);
        }
        let help = if field.help.is_empty() {
            range_hint(field)
        } else {
            format!("{}\n{}", tr(&field.help), range_hint(field))
        };
        ui.add(egui::Label::new(text).truncate())
            .on_hover_text(help);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let differs = reference.as_ref().is_some_and(|r| *r != current);
            if differs
                && ui
                    .add(egui::Button::new("AVE").small().frame(false))
                    .on_hover_text(tr("Reset this value to the AVE reference"))
                    .clicked()
            {
                reset_field_to_reference(state, field);
            }
        });
    });
    match field.kind {
        FieldKind::Float | FieldKind::Int => {
            numeric_editor(state, ui, field, current.as_f64().unwrap_or(0.0));
        }
        FieldKind::OptionalFloat => optional_editor(state, ui, field, &current),
        FieldKind::Airfoil => airfoil_editor(state, ui, field, &current),
        FieldKind::Engine => engine_editor(state, ui, field, &current),
        FieldKind::Vec3 => list_editor(state, ui, field, &current, Some(["x", "y", "z"])),
        FieldKind::FloatList => list_editor(state, ui, field, &current, None),
        FieldKind::PairList => pair_list_editor(state, ui, field, &current),
    }
}

/// Render one titled group of fields with its group reset.
pub fn show_group(
    state: &mut AppState,
    ui: &mut Ui,
    title: &str,
    group: &[SandboxField],
    id_salt: &str,
    default_open: bool,
) {
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        egui::CollapsingHeader::new(RichText::new(tr(title)).strong())
            .id_salt(id_salt)
            .default_open(default_open)
            .show(ui, |ui| {
                if ui
                    .add(egui::Button::new(tr("Reset group to AVE")).small().frame(false))
                    .on_hover_text(tr("Restore every value in this group, and the values that depend on them, to the AVE reference."))
                    .clicked()
                {
                    reset_group_to_reference(state, group);
                }
                for field in group {
                    show_field(state, ui, field);
                    ui.add_space(3.0);
                }
            });
    });
    ui.add_space(4.0);
}
