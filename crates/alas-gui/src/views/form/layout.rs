// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Layout of a schema form: root nodes, columns and runs of leaf fields.

use std::collections::HashSet;

use alas_config::{Entry, Field};
use egui::Ui;
use serde_json::Value;

use super::field::render_one;
use super::FormEdit;
use super::{MAX_LABEL_TEXT_SIZE, MIN_FORM_COLUMN_WIDTH, MIN_LABEL_TEXT_SIZE};
use crate::views::tr_fields;

// The schema, inherited visibility context, and edit accumulator are separate
// mutable UI concerns; bundling them would obscure recursive node rendering.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_fields(
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
    let mut changed = false;
    let mut run_start = 0;
    for (index, field) in fields.iter().enumerate() {
        if !matches!(&field.entry, Entry::Node(_)) {
            continue;
        }
        // Collapsible groups can become arbitrarily taller than their peers.
        // Give each group the full width so expanding one never leaves an
        // empty cell below a collapsed group beside it.
        if run_start < index {
            changed |= render_leaf_run(
                ui,
                &fields[run_start..index],
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
            false,
        );
        run_start = index + 1;
    }
    if run_start < fields.len() {
        changed |= render_leaf_run(
            ui,
            &fields[run_start..],
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
    changed
}

// See `render_fields`: this preserves explicit ownership at the recursive UI boundary.
#[allow(clippy::too_many_arguments)]
fn render_leaf_run(
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
    let columns = form_column_count(ui.available_width()).min(fields.len());
    let mut changed = false;
    if columns == 1 {
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
                false,
            );
        }
        return changed;
    }

    // Keep column UIs alive for the whole run. A row-per-chunk layout made
    // every later field wait below the tallest editor in its previous row.
    ui.columns(columns, |column_uis| {
        for (index, field) in fields.iter().enumerate() {
            let column = if index < columns {
                index
            } else {
                column_uis
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| {
                        a.min_rect().bottom().total_cmp(&b.min_rect().bottom())
                    })
                    .map(|(index, _)| index)
                    .unwrap_or(0)
            };
            changed |= render_one(
                &mut column_uis[column],
                field,
                values,
                error_fields,
                lang,
                show_help,
                open_root_nodes,
                id_prefix,
                ancestors,
                edits,
                true,
            );
            column_uis[column].add_space(4.0);
        }
    });
    changed
}

pub(super) fn form_column_count(available_width: f32) -> usize {
    ((available_width / MIN_FORM_COLUMN_WIDTH).floor() as usize).clamp(1, 3)
}

pub(super) fn label_text_size(available_width: f32) -> f32 {
    let t = ((available_width - 320.0) / 480.0).clamp(0.0, 1.0);
    MIN_LABEL_TEXT_SIZE + (MAX_LABEL_TEXT_SIZE - MIN_LABEL_TEXT_SIZE) * t
}

// A field renderer needs each independently borrowed schema and JSON context.
