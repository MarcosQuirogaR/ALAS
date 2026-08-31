// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use std::collections::HashSet;

use alas_config::{Entry, Field, Kind};
use egui::{ComboBox, DragValue, RichText, Slider, TextEdit, Ui};
use serde_json::Value;

use crate::views::{tr, tr_fields};

#[path = "../form_feedback.rs"]
mod form_feedback;
#[path = "../form_options.rs"]
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
