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

use egui::{DragValue, RichText, Ui};
use serde_json::Value;

use crate::state::AppState;
use crate::views::{tr, tr_fields};

use super::fields::{self, FieldKind, SandboxField};
use super::list_editors::{list_editor, pair_list_editor};

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
        state.sandbox.rejected_edit = Some(
            tr_fields(
                "{label} must stay between {min} and {max} {unit}; the edit was not applied.",
                &[
                    ("label", tr(&field.label)),
                    ("min", format!("{}", field.min)),
                    ("max", format!("{}", field.max)),
                    ("unit", shown_unit(&field.unit)),
                ],
            )
            .replace(" ;", ";"),
        );
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
        Value::Number(n) => format!("{n}{}", unit_suffix(&field.unit)),
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
            ("unit", shown_unit(&field.unit)),
        ],
    )
    .trim_end()
    .to_owned()
}

/// The unit as shown: a dimensionless field (unit `-`, `1` or none) shows
/// nothing, instead of a stray dash after the value.
pub(super) fn shown_unit(unit: &str) -> String {
    match unit.trim() {
        "" | "-" | "\u{2013}" | "\u{2014}" | "1" | "[-]" => String::new(),
        unit => unit.to_owned(),
    }
}

/// The unit with its leading space, or nothing for a dimensionless field.
pub(super) fn unit_suffix(unit: &str) -> String {
    let unit = shown_unit(unit);
    if unit.is_empty() {
        unit
    } else {
        format!(" {unit}")
    }
}

pub(super) fn gesture_of(response: &egui::Response) -> Option<Gesture> {
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
            .suffix(unit_suffix(&field.unit)),
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
            // The hump is only drawn as a complete ordered set: enabling its
            // height fills the whole set (and asks first on a lofted body).
            if set && field.id == super::hump_defaults::HEIGHT_ID {
                super::hump_defaults::request_enable(state);
                return;
            }
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

/// An on/off field; a missing value (defaults are not serialised) reads off.
fn bool_editor(state: &mut AppState, ui: &mut Ui, field: &SandboxField, current: &Value) {
    let mut on = current.as_bool().unwrap_or(false);
    if crate::checkbox::checkbox(ui, &mut on, "").changed() {
        commit_field(state, field, Value::Bool(on), Gesture::Committed);
    }
}

fn airfoil_editor(state: &mut AppState, ui: &mut Ui, field: &SandboxField, current: &Value) {
    let mut text = current.as_str().unwrap_or_default().to_owned();
    let search_id = ui.make_persistent_id(("sandbox_airfoil_search", &field.id));
    ui.horizontal(|ui| {
        let optional = field.kind == FieldKind::OptionalAirfoil;
        let mut edit = egui::TextEdit::singleline(&mut text).desired_width(140.0);
        if optional {
            edit = edit.hint_text(tr("tail section"));
        }
        let response = ui.add(edit);
        if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            // An optional section left empty is unset, not an empty name.
            let value = if optional && text.trim().is_empty() {
                Value::Null
            } else {
                Value::String(text.clone())
            };
            commit_field(state, field, value, Gesture::Committed);
        }
        ui.menu_button("v", |ui| {
            let mut search = ui
                .ctx()
                .data(|data| data.get_temp::<String>(search_id))
                .unwrap_or_default();
            if ui
                .add(
                    egui::TextEdit::singleline(&mut search)
                        .hint_text(tr("Search airfoils"))
                        .desired_width(220.0),
                )
                .changed()
            {
                ui.ctx()
                    .data_mut(|data| data.insert_temp(search_id, search.clone()));
            }
            let filter = search.trim().to_ascii_lowercase();
            let names: Vec<_> =
                alas_geom::airfoil_library::AirfoilLibrary::get_available_airfoils()
                    .into_iter()
                    .filter(|name| filter.is_empty() || name.to_ascii_lowercase().contains(&filter))
                    .collect();
            if names.is_empty() {
                ui.label(tr("No library sections match the name filter."));
                return;
            }
            let row_height = ui
                .text_style_height(&egui::TextStyle::Button)
                .max(ui.spacing().interact_size.y);
            egui::ScrollArea::vertical()
                .id_salt(("sandbox_airfoil_rows", &field.id))
                .max_height(260.0)
                .show_rows(ui, row_height, names.len(), |ui, range| {
                    for index in range {
                        let name = names[index];
                        if ui.selectable_label(text == name, name).clicked() {
                            commit_field(
                                state,
                                field,
                                Value::String(name.to_owned()),
                                Gesture::Committed,
                            );
                            ui.ctx().data_mut(|data| data.remove::<String>(search_id));
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
    let chosen = crate::selector_search::named_list_combo(
        ui,
        &format!("sandbox_engine::{}", field.id),
        Some(ui.available_width()),
        &current,
        &names,
        &current,
    );
    if let Some(name) = chosen {
        let before = state.edit_snapshot();
        state.sandbox.undo.record(before);
        state.set_engine(&name);
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
    let top_left = ui.cursor().min;
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
        FieldKind::Bool => bool_editor(state, ui, field, &current),
        FieldKind::Airfoil | FieldKind::OptionalAirfoil => {
            airfoil_editor(state, ui, field, &current);
        }
        FieldKind::Engine => engine_editor(state, ui, field, &current),
        FieldKind::Vec3 => list_editor(state, ui, field, &current, Some(["x", "y", "z"])),
        FieldKind::FloatList => list_editor(state, ui, field, &current, None),
        FieldKind::PairList => pair_list_editor(state, ui, field, &current),
    }
    // The palette's jump: scroll this field into view and pulse it.
    let bottom_right = egui::pos2(ui.max_rect().right(), ui.cursor().top());
    crate::search::focus_anchor(
        ui,
        &field.id,
        egui::Rect::from_two_pos(top_left, bottom_right),
    );
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
        // A group that holds the field the palette jumped to opens for it.
        let wanted = group
            .iter()
            .any(|field| crate::search::focus_wants(ui.ctx(), &field.id));
        let mut header = egui::CollapsingHeader::new(crate::theme::card_title(ui, tr(title)))
            .id_salt(id_salt)
            .default_open(default_open).open(crate::perf_tour::force_open());
        if wanted {
            header = header.open(Some(true));
        }
        crate::theme::show_header(ui, header, |ui| {
                if ui
                    .add(crate::theme::selectable_button(tr("Reset group to AVE"), false).min_size(egui::vec2(0.0, ui.spacing().interact_size.y)))
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

#[cfg(test)]
#[path = "editors_tests.rs"]
mod tests;
