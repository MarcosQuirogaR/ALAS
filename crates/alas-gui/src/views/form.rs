// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The generic form, rendered straight from a configuration group's schema.
//!
//! A port of the reference desktop app's `DynamicForm`: it walks the [`Field`]
//! list the derive macro emits and edits the matching `serde_json::Value` in
//! place, so every group's every field appears with zero per-field code and a
//! new field on the model side needs none here. Field prose is looked up in the
//! active language the way the schema documents.

use std::collections::HashSet;

use alas_config::{Entry, Field, Kind};
use egui::{ComboBox, DragValue, RichText, Slider, TextEdit, Ui};
use serde_json::Value;

use crate::views::{tr, tr_fields};

#[path = "form_feedback.rs"]
mod form_feedback;
#[path = "form_options.rs"]
mod form_options;

use form_feedback::{is_modified, paint_modified_indicator, record_modified};
use form_options::{
    bounds, display_option, is_editable, leaf_decimals, number_step, readonly_unless,
    resolved_options, value_as_str,
};

/// The log-slider mapping a `weight_slider` field uses, matching the
/// reference's 0.001..1000 logarithmic range.
const WEIGHT_MIN: f64 = 0.001;
const WEIGHT_MAX: f64 = 1000.0;

/// The narrowest readable form column and adaptive label bounds.
const MIN_FORM_COLUMN_WIDTH: f32 = 300.0;
const MIN_LABEL_TEXT_SIZE: f32 = 11.0;
const MAX_LABEL_TEXT_SIZE: f32 = 15.0;

/// One localized schema field whose displayed value changed this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormEdit {
    /// Human-readable, localized schema label.
    pub label: String,
    /// Final rendered value, including a declared unit where applicable.
    pub value: String,
}

/// Render a form for `fields`, editing `values` in place and returning each
/// value changed this frame.
pub fn dynamic_form(
    ui: &mut Ui,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
) -> Vec<FormEdit> {
    dynamic_form_with_open_root_nodes(ui, fields, values, error_fields, lang, show_help, false)
}

/// Render a dynamic form whose direct child groups start open. This is used
/// sparingly for pages whose two root nodes are the primary navigation (for
/// example, optimizer weights and solver settings); nested groups retain the
/// compact default so opening a page never reveals an entire tree at once.
pub fn dynamic_form_with_open_root_nodes(
    ui: &mut Ui,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    open_root_nodes: bool,
) -> Vec<FormEdit> {
    let mut edits = Vec::new();
    render_fields(
        ui,
        fields,
        values,
        error_fields,
        lang,
        show_help,
        open_root_nodes,
        "",
        &[],
        &mut edits,
    );
    edits
}

// The schema, inherited visibility context, and edit accumulator are separate
// mutable UI concerns; bundling them would obscure recursive node rendering.
#[allow(clippy::too_many_arguments)]
fn render_fields(
    ui: &mut Ui,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    open_root_nodes: bool,
    id_prefix: &str,
    ancestors: &[Value],
    edits: &mut Vec<FormEdit>,
) -> bool {
    if !values.is_object() {
        *values = Value::Object(serde_json::Map::new());
    }
    let mut changed = false;

    let primary: Vec<&Field> = fields.iter().filter(|f| !f.advanced).collect();
    let advanced: Vec<&Field> = fields.iter().filter(|f| f.advanced).collect();

    changed |= render_field_list(
        ui,
        &primary,
        values,
        error_fields,
        lang,
        show_help,
        open_root_nodes,
        id_prefix,
        ancestors,
        edits,
    );

    if !advanced.is_empty() {
        let header = tr_fields(
            "Advanced ({count})",
            &[("count", advanced.len().to_string())],
        );
        egui::CollapsingHeader::new(header)
            .id_salt(format!("{id_prefix}::advanced"))
            .default_open(false)
            .show(ui, |ui| {
                changed |= render_field_list(
                    ui,
                    &advanced,
                    values,
                    error_fields,
                    lang,
                    show_help,
                    open_root_nodes,
                    id_prefix,
                    ancestors,
                    edits,
                );
            });
    }

    changed
}

// See `render_fields`: this preserves explicit ownership at the recursive UI boundary.
#[allow(clippy::too_many_arguments)]
fn render_field_list(
    ui: &mut Ui,
    fields: &[&Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    open_root_nodes: bool,
    id_prefix: &str,
    ancestors: &[Value],
    edits: &mut Vec<FormEdit>,
) -> bool {
    if fields.is_empty() {
        return false;
    }
    let columns = form_column_count(ui.available_width()).min(fields.len());
    if columns == 1 {
        let mut changed = false;
        for field in fields {
            changed |= render_one(
                ui,
                field,
                values,
                error_fields,
                lang,
                show_help,
                open_root_nodes,
                id_prefix,
                ancestors,
                edits,
            );
        }
        return changed;
    }

    let mut changed = false;
    // Lay fields out in rows, rather than filling one long masonry column at
    // a time.  Every editor now begins beneath its matching label and the
    // next row starts only after the tallest cell has finished.
    for row in fields.chunks(columns) {
        ui.columns(columns, |column_uis| {
            for (index, field) in row.iter().enumerate() {
                changed |= render_one(
                    &mut column_uis[index],
                    field,
                    values,
                    error_fields,
                    lang,
                    show_help,
                    open_root_nodes,
                    id_prefix,
                    ancestors,
                    edits,
                );
            }
        });
        ui.add_space(4.0);
    }
    changed
}

fn form_column_count(available_width: f32) -> usize {
    ((available_width / MIN_FORM_COLUMN_WIDTH).floor() as usize).clamp(1, 3)
}

fn label_text_size(available_width: f32) -> f32 {
    let t = ((available_width - 320.0) / 480.0).clamp(0.0, 1.0);
    MIN_LABEL_TEXT_SIZE + (MAX_LABEL_TEXT_SIZE - MIN_LABEL_TEXT_SIZE) * t
}

// A field renderer needs each independently borrowed schema and JSON context.
#[allow(clippy::too_many_arguments)]
fn render_one(
    ui: &mut Ui,
    field: &Field,
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    open_by_default: bool,
    id_prefix: &str,
    ancestors: &[Value],
    edits: &mut Vec<FormEdit>,
) -> bool {
    let label = alas_i18n::t(Some(field.label), lang).into_owned();
    let help = alas_i18n::t(Some(field.help), lang).into_owned();

    match &field.entry {
        Entry::Node(node) => {
            let mut changed = false;
            let child_id = format!("{id_prefix}::{}", field.name);
            crate::theme::card_frame(ui).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                egui::CollapsingHeader::new(RichText::new(&label).strong())
                    .id_salt(&child_id)
                    // Large groups such as geometry stations and cabin
                    // classes start compact. Only a caller's direct root
                    // groups may opt into an expanded landing state.
                    .default_open(open_by_default)
                    .show(ui, |ui| {
                        if show_help && !help.is_empty() {
                            ui.label(RichText::new(&help).weak());
                        }
                        let mut child_ancestors = Vec::with_capacity(ancestors.len() + 1);
                        child_ancestors.push(values.clone());
                        child_ancestors.extend_from_slice(ancestors);
                        let child = values.as_object_mut().and_then(|m| {
                            m.entry(field.name.to_owned())
                                .or_insert_with(|| Value::Object(serde_json::Map::new()));
                            m.get_mut(field.name)
                        });
                        if let Some(child) = child {
                            changed = render_fields(
                                ui,
                                &node.fields,
                                child,
                                error_fields,
                                lang,
                                show_help,
                                false,
                                &child_id,
                                &child_ancestors,
                                edits,
                            );
                        }
                    });
            });
            changed
        }
        Entry::Leaf(leaf) => {
            // Resolve read-only gating against a sibling's current value before
            // borrowing the edited slot, so the two borrows never overlap.
            let readonly = leaf
                .readonly_unless
                .map(|ru| readonly_unless(values, ancestors, ru))
                .unwrap_or(false);

            let has_error = error_fields.contains(field.name);
            let options = resolved_options(field, values);
            let slot = match slot_mut(values, field.name, leaf.value.clone()) {
                Some(slot) => slot,
                None => return false,
            };
            let mut changed = false;
            let modified_id = egui::Id::new(("alas-form-modified", id_prefix, field.name));

            let modified = is_modified(ui, modified_id);
            let mut draw = |ui: &mut Ui| {
                let mut text = RichText::new(&label);
                if has_error {
                    text = text.color(ui.visuals().error_fg_color);
                }
                text = text.size(label_text_size(ui.available_width()));
                if leaf.kind == Kind::Bool {
                    ui.add_enabled_ui(!readonly, |ui| {
                        changed = edit_leaf(
                            ui,
                            field,
                            leaf.kind,
                            slot,
                            id_prefix,
                            options.as_deref(),
                            modified,
                            &label,
                        );
                    })
                    .response
                    .on_hover_text(&help);
                } else {
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(text).truncate())
                            .on_hover_text(&help);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !readonly
                                && *slot != leaf.value
                                && ui
                                    .add(egui::Button::new("<-").frame(false))
                                    .on_hover_text(tr("Reset this value"))
                                    .clicked()
                            {
                                *slot = leaf.value.clone();
                                changed = true;
                            }
                        });
                    });
                    ui.add_enabled_ui(!readonly, |ui| {
                        changed |= edit_leaf(
                            ui,
                            field,
                            leaf.kind,
                            slot,
                            id_prefix,
                            options.as_deref(),
                            modified,
                            &label,
                        );
                    });
                }
            };
            ui.vertical(&mut draw);
            record_modified(ui, modified_id, changed);
            if changed {
                edits.push(FormEdit {
                    label,
                    value: feedback_value(field, slot),
                });
            }
            changed
        }
    }
}

fn feedback_value(field: &Field, value: &Value) -> String {
    let text = match value {
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Null => tr("none"),
        value => value.to_string(),
    };
    if field.unit.is_empty() || !value.is_number() {
        text
    } else {
        format!("{text} {}", field.unit)
    }
}

fn slot_mut<'a>(values: &'a mut Value, name: &str, default: Value) -> Option<&'a mut Value> {
    let map = values.as_object_mut()?;
    Some(map.entry(name.to_owned()).or_insert(default))
}

// Each argument is an independent schema or presentation input used by the
// leaf editor; a bundle would make the dynamic-form boundary less explicit.
#[allow(clippy::too_many_arguments)]
fn edit_leaf(
    ui: &mut Ui,
    field: &Field,
    kind: Kind,
    slot: &mut Value,
    id_prefix: &str,
    options: Option<&[String]>,
    modified: bool,
    label: &str,
) -> bool {
    match kind {
        Kind::Bool => {
            let mut v = slot.as_bool().unwrap_or(false);
            if ui.checkbox(&mut v, label).changed() {
                *slot = Value::Bool(v);
                return true;
            }
            false
        }
        Kind::Int => {
            let mut v = slot.as_f64().unwrap_or(0.0);
            let mut drag = DragValue::new(&mut v)
                .speed(1.0)
                .suffix(unit_suffix(field, modified));
            if let (Some(lo), Some(hi)) = bounds(field) {
                drag = drag.range(lo..=hi);
            }
            let response = ui.add_sized([ui.available_width(), ui.spacing().interact_size.y], drag);
            paint_modified_indicator(ui, &response, modified || response.changed());
            if response.changed() {
                *slot = Value::from(v.round() as i64);
                return true;
            }
            false
        }
        Kind::Float => {
            let mut v = slot.as_f64().unwrap_or(0.0);
            let (decimals, step) = number_step(v, field.unit, leaf_decimals(field));
            let mut drag = DragValue::new(&mut v)
                .speed(step)
                .max_decimals(decimals)
                .suffix(unit_suffix(field, modified));
            if let (Some(lo), Some(hi)) = bounds(field) {
                drag = drag.range(lo..=hi);
            }
            let response = ui.add_sized([ui.available_width(), ui.spacing().interact_size.y], drag);
            paint_modified_indicator(ui, &response, modified || response.changed());
            if response.changed() {
                *slot = Value::from(v);
                return true;
            }
            false
        }
        Kind::WeightSlider => {
            let mut v = slot
                .as_f64()
                .unwrap_or(WEIGHT_MIN)
                .clamp(WEIGHT_MIN, WEIGHT_MAX);
            let slider = Slider::new(&mut v, WEIGHT_MIN..=WEIGHT_MAX)
                .logarithmic(true)
                .suffix(unit_suffix(field, modified));
            let response =
                ui.add_sized([ui.available_width(), ui.spacing().interact_size.y], slider);
            paint_modified_indicator(ui, &response, modified || response.changed());
            if response.changed() {
                *slot = Value::from(v);
                return true;
            }
            false
        }
        Kind::Str => edit_str(ui, field, slot, id_prefix, options, modified),
        Kind::Optional => {
            let mut text = value_as_str(slot).unwrap_or_default();
            let response = ui.add(
                TextEdit::singleline(&mut text)
                    .hint_text(unit_suffix_text("", modified))
                    .desired_width(ui.available_width()),
            );
            paint_modified_indicator(ui, &response, modified || response.changed());
            if response.changed() {
                *slot = if text.is_empty() {
                    Value::Null
                } else {
                    Value::String(text)
                };
                return true;
            }
            false
        }
        Kind::NumberList => edit_number_list(ui, slot),
        Kind::TupleList => edit_tuple_list(ui, field, slot),
        Kind::Nested | Kind::Unsupported => {
            ui.add_enabled(
                false,
                egui::Label::new(RichText::new(slot.to_string()).weak()),
            );
            false
        }
    }
}

fn unit_suffix(field: &Field, modified: bool) -> String {
    unit_suffix_text(field.unit, modified)
}

fn unit_suffix_text(unit: &str, modified: bool) -> String {
    match (unit.is_empty(), modified) {
        (true, false) => String::new(),
        (false, false) => format!(" {unit}"),
        (true, true) => format!("  {}", tr("Modified")),
        (false, true) => format!(" {unit}  {}", tr("Modified")),
    }
}

fn edit_str(
    ui: &mut Ui,
    field: &Field,
    slot: &mut Value,
    id_prefix: &str,
    options: Option<&[String]>,
    modified: bool,
) -> bool {
    let current = value_as_str(slot).unwrap_or_default();

    match options {
        Some(opts) if !is_editable(field) => {
            let mut changed = false;
            ComboBox::from_id_salt(format!("{id_prefix}::{}", field.name))
                .width(ui.available_width())
                .selected_text(if current.is_empty() {
                    "-".to_owned()
                } else {
                    format!(
                        "{}{}",
                        display_option(&current),
                        unit_suffix_text("", modified)
                    )
                })
                .show_ui(ui, |ui| {
                    // Keep a value from an older save selectable even when it
                    // is no longer in the list.
                    if !current.is_empty()
                        && !opts.iter().any(|o| o == &current)
                        && ui
                            .selectable_label(true, display_option(&current))
                            .clicked()
                    {
                        changed = true;
                    }
                    for opt in opts {
                        if ui
                            .selectable_label(current == *opt, display_option(opt))
                            .clicked()
                        {
                            *slot = Value::String(opt.clone());
                            changed = true;
                        }
                    }
                });
            changed
        }
        Some(opts) => {
            // Editable option field (airfoils): a free-text box plus a menu of
            // the known names, since free text is still accepted downstream.
            let mut text = current.clone();
            let mut changed = ui
                .add(TextEdit::singleline(&mut text).desired_width(160.0))
                .changed();
            if changed {
                *slot = Value::String(text.clone());
            }
            ui.menu_button("v", |ui| {
                egui::ScrollArea::vertical()
                    .max_height(240.0)
                    .show(ui, |ui| {
                        for opt in opts.iter().take(400) {
                            if ui.selectable_label(current == *opt, opt).clicked() {
                                *slot = Value::String(opt.clone());
                                changed = true;
                                ui.close_menu();
                            }
                        }
                    });
            });
            changed
        }
        None => {
            let mut text = current;
            let response = ui.add(
                TextEdit::singleline(&mut text)
                    .desired_width(ui.available_width())
                    .hint_text(unit_suffix_text("", modified)),
            );
            paint_modified_indicator(ui, &response, modified || response.changed());
            if response.changed() {
                *slot = Value::String(text);
                return true;
            }
            false
        }
    }
}

fn edit_number_list(ui: &mut Ui, slot: &mut Value) -> bool {
    let mut nums: Vec<f64> = slot
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_f64()).collect())
        .unwrap_or_default();
    let mut changed = false;
    ui.horizontal_wrapped(|ui| {
        for v in nums.iter_mut() {
            if ui.add(DragValue::new(v).speed(0.1)).changed() {
                changed = true;
            }
        }
    });
    if changed {
        *slot = Value::from(nums);
    }
    changed
}

fn edit_tuple_list(ui: &mut Ui, field: &Field, slot: &mut Value) -> bool {
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
    ui.vertical(|ui| {
        if let Entry::Leaf(leaf) = &field.entry {
            if let Some(cols) = leaf.columns {
                ui.horizontal_wrapped(|ui| {
                    for c in cols {
                        ui.add_sized([70.0, 16.0], egui::Label::new(RichText::new(*c).weak()));
                    }
                });
            }
        }
        for row in rows.iter_mut() {
            ui.horizontal_wrapped(|ui| {
                for v in row.iter_mut() {
                    if ui
                        .add_sized([70.0, 18.0], DragValue::new(v).speed(0.1))
                        .changed()
                    {
                        changed = true;
                    }
                }
            });
        }
    });
    if changed {
        *slot = Value::from(rows.into_iter().map(Value::from).collect::<Vec<_>>());
    }
    changed
}

#[cfg(test)]
#[path = "form_tests.rs"]
mod tests;
