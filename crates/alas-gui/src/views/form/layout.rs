// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Layout of a schema form: root nodes, columns and runs of leaf fields.

use std::collections::HashSet;

use alas_config::{Entry, Field};
use egui::Ui;
use serde_json::Value;

use super::cell_grid::CellGrid;
use super::field::render_one;
use super::visibility::is_visible;
use super::FormEdit;
use super::MIN_FORM_COLUMN_WIDTH;
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
    ancestors: &[&Value],
    edits: &mut Vec<FormEdit>,
) -> bool {
    if !values.is_object() {
        *values = Value::Object(serde_json::Map::new());
    }
    let mut changed = false;

    // A field whose `visible_if` condition is false is not drawn at all.
    let shown = |f: &&Field| is_visible(f, values, ancestors);
    let primary: Vec<&Field> = fields
        .iter()
        .filter(|f| !f.advanced)
        .filter(shown)
        .collect();
    let advanced: Vec<&Field> = fields.iter().filter(|f| f.advanced).filter(shown).collect();

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

    let detail = crate::detail::published(ui.ctx());
    if !advanced.is_empty() && detail == Some(crate::detail::Detail::Basic) {
        ui.label(
            egui::RichText::new(crate::views::tr("More settings in Advanced view"))
                .weak()
                .small(),
        );
    }
    if !advanced.is_empty() && detail != Some(crate::detail::Detail::Basic) {
        let header = tr_fields(
            "Advanced ({count})",
            &[("count", advanced.len().to_string())],
        );
        // A search jump to a field inside opens the group.
        let focus_inside = crate::search::published_focus(ui.ctx()).is_some_and(|focus| {
            advanced.iter().any(|field| {
                let child = format!("{id_prefix}::{}", field.name);
                focus.tail == child || focus.tail.starts_with(&format!("{child}::"))
            })
        });
        let mut group = egui::CollapsingHeader::new(crate::theme::section_title(ui, header))
            .id_salt(format!("{id_prefix}::advanced"))
            .default_open(detail == Some(crate::detail::Detail::Advanced))
            .open(crate::perf_tour::force_open());
        if focus_inside {
            group = group.open(Some(true));
        }
        crate::theme::show_header(ui, group, |ui| {
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
    ancestors: &[&Value],
    edits: &mut Vec<FormEdit>,
) -> bool {
    if fields.is_empty() {
        return false;
    }
    render_field_run(
        ui,
        fields,
        values,
        error_fields,
        lang,
        show_help,
        open_root_nodes,
        id_prefix,
        ancestors,
        edits,
    )
}

// See `render_fields`: this preserves explicit ownership at the recursive UI boundary.
#[allow(clippy::too_many_arguments)]
fn render_field_run(
    ui: &mut Ui,
    fields: &[&Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    open_root_nodes: bool,
    id_prefix: &str,
    ancestors: &[&Value],
    edits: &mut Vec<FormEdit>,
) -> bool {
    let mut changed = false;
    let mut run_start = 0;
    let mut skip_until = 0;
    // Loose fields of a page whose other fields are cards get a card too.
    let boxed = id_prefix.is_empty()
        && ancestors.is_empty()
        && fields.iter().any(|f| matches!(f.entry, Entry::Node(_)));
    for (index, field) in fields.iter().enumerate() {
        if index < skip_until || !matches!(&field.entry, Entry::Node(_)) {
            continue;
        }
        // Collapsible groups can become arbitrarily taller than their peers.
        // Give each group the full width so expanding one never leaves an
        // empty cell below a collapsed group beside it.
        if run_start < index {
            changed |= super::pack::boxed_run(ui, boxed, |ui| {
                render_leaf_run(
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
                )
            });
        }
        // Consecutive groups are flowed into rows: short cards side by side.
        let end = (index..fields.len())
            .find(|at| !matches!(&fields[*at].entry, Entry::Node(_)))
            .unwrap_or(fields.len());
        let cols: Vec<usize> = fields[index..end]
            .iter()
            .map(|field| super::pack::node_columns(field, values))
            .collect();
        super::pack::flow_cards(ui, ("cards", index), &cols, |ui, k| {
            changed |= render_one(
                ui,
                fields[index + k],
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
        });
        skip_until = end;
        run_start = end;
    }
    if run_start < fields.len() {
        changed |= super::pack::boxed_run(ui, boxed, |ui| {
            render_leaf_run(
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
            )
        });
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
    ancestors: &[&Value],
    edits: &mut Vec<FormEdit>,
) -> bool {
    // Two or more checkboxes in a row form their own grid: a checkbox cell in
    // a mixed grid leaves a blank label line above it so it sits on the
    // editor line, which for a row of checkboxes only is a blank band.
    let mut changed = false;
    let mut start = 0;
    while start < fields.len() {
        let bools = fields[start..]
            .iter()
            .take_while(|field| is_plain_bool(field))
            .count();
        let (end, checkbox_grid) = if bools >= 2 {
            (start + bools, true)
        } else {
            let plain = fields[start..]
                .iter()
                .position(is_plain_bool)
                .map_or(fields.len(), |at| start + at);
            // A run of fields up to the next checkbox pair.
            let mut end = plain.max(start + 1);
            while end < fields.len() {
                if is_plain_bool(&fields[end]) && fields.get(end + 1).is_some_and(is_plain_bool) {
                    break;
                }
                end += 1;
            }
            (end, false)
        };
        changed |= render_leaf_chunk(
            ui,
            &fields[start..end],
            values,
            error_fields,
            lang,
            show_help,
            open_root_nodes,
            id_prefix,
            ancestors,
            edits,
            !checkbox_grid,
        );
        start = end;
    }
    changed
}

pub(super) fn is_plain_bool(field: &&Field) -> bool {
    matches!(
        &field.entry,
        Entry::Leaf(leaf) if leaf.kind == alas_config::Kind::Bool && leaf.optional_value_kind.is_none()
    )
}

// See `render_fields`: this preserves explicit ownership at the recursive UI boundary.
#[allow(clippy::too_many_arguments)]
fn render_leaf_chunk(
    ui: &mut Ui,
    fields: &[&Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    open_root_nodes: bool,
    id_prefix: &str,
    ancestors: &[&Value],
    edits: &mut Vec<FormEdit>,
    aligned_cells: bool,
) -> bool {
    // The one form grid (see `cell_grid`): columns from the width, level rows,
    // a tall list or table editor spanning rows beside the scalars.
    let first = fields.first().map_or("", |field| field.name);
    let mut grid = CellGrid::new(ui, ("form_grid", id_prefix, first));
    // A checkbox stands in a captioned cell when it shares rows with other
    // fields; in one column it keeps its label beside the tick.
    let captioned = aligned_cells && grid.columns() > 1;
    let mut changed = false;
    for (index, field) in fields.iter().enumerate() {
        if fields
            .get(index + 1)
            .is_some_and(|next| shown_by_switch(field, next))
        {
            grid.keep_with_next(ui.ctx());
        }
        grid.cell(ui, |ui, _| {
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
                captioned,
            );
        });
    }
    grid.finish(ui);
    changed
}

/// Whether `next` is shown by the switch `field` (its `visible_if` names it).
fn shown_by_switch(field: &Field, next: &Field) -> bool {
    is_plain_bool(&field)
        && next
            .visible_if
            .is_some_and(|condition| condition.path.rsplit('.').next() == Some(field.name))
}

pub(crate) fn form_column_count(available_width: f32) -> usize {
    ((available_width / MIN_FORM_COLUMN_WIDTH).floor() as usize).clamp(1, super::MAX_FORM_COLUMNS)
}
