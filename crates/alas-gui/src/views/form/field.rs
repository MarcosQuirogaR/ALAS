// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rendering of one schema field: label row, reset and library affordances, editor.

use std::collections::HashSet;

use alas_config::{Entry, Field, Kind};
use egui::{RichText, Ui};
use serde_json::Value;

use super::cell_height::{uniform_height_kind, uniform_leaf_height};
use super::editors::edit_leaf;
use super::feedback::{differs_from_default, modified_marker};
use super::layout::{label_text_size, render_fields};
use super::options::{
    format_number, leaf_decimals, number_step, readonly_unless, resolved_options,
};
use super::RESET_GLYPH;
use super::{display_unit, FormEdit};
use crate::views::tr;

// A field renderer needs each independently borrowed schema and JSON context.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_one(
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
    grid: bool,
) -> bool {
    let label = alas_i18n::t(Some(field.label), lang).into_owned();
    let help = alas_i18n::t(Some(field.help), lang).into_owned();

    match &field.entry {
        Entry::Node(node) => {
            let mut changed = false;
            let child_id = format!("{id_prefix}::{}", field.name);
            crate::theme::card_frame(ui).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                let header = egui::CollapsingHeader::new(RichText::new(&label).strong())
                    .id_salt(&child_id)
                    // Large groups such as geometry stations and cabin
                    // classes start compact. Only a caller's direct root
                    // groups may opt into an expanded landing state.
                    .default_open(open_by_default)
                    .show(ui, |ui| {
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
                if !help.is_empty() {
                    header.header_response.on_hover_text(&help);
                }
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
            // One source for the marker and the reset arrow: the value the
            // field holds against the default the schema declares. A field
            // stays marked for as long as it differs, whatever the focus is
            // doing, and the editor below never repeats the word inside its
            // own value text.
            let modified = !readonly && differs_from_default(slot, &leaf.value);
            let mut draw = |ui: &mut Ui| {
                let mut text = RichText::new(&label);
                if has_error {
                    text = text.color(ui.visuals().error_fg_color);
                }
                text = text.size(label_text_size(ui.available_width()));
                let label_row_height = ui.spacing().interact_size.y;
                if leaf.kind == Kind::Bool && leaf.optional_value_kind.is_none() {
                    if grid {
                        // A checkbox carries its own label. In a grid it sits
                        // on the editor line of its row, not on the label line.
                        ui.add_space(label_row_height + ui.spacing().item_spacing.y);
                    }
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(!readonly, |ui| {
                            changed = edit_leaf(
                                ui,
                                field,
                                leaf.kind,
                                slot,
                                id_prefix,
                                options.as_deref(),
                                &label,
                            );
                        })
                        .response
                        .on_hover_text(&help);
                        modified_marker(ui, modified);
                    });
                } else {
                    ui.horizontal(|ui| {
                        // A fixed label-row height: the reset button appears
                        // only on modified fields and must not shift the row.
                        ui.set_min_height(label_row_height);
                        ui.add(egui::Label::new(text).truncate())
                            .on_hover_text(&help);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // A framed reset glyph, not a bare ASCII arrow:
                            // `<-` rendered as dim body text beside the label
                            // with no frame, no colour coding and no legend,
                            // and could not be told from the label itself.
                            if !readonly
                                && *slot != leaf.value
                                && ui
                                    .add(egui::Button::new(RESET_GLYPH).small())
                                    .on_hover_text(tr("Reset this value"))
                                    .clicked()
                            {
                                *slot = leaf.value.clone();
                                changed = true;
                            }
                            modified_marker(ui, modified && !changed);
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
                            &label,
                        );
                    });
                }
            };
            if grid && uniform_height_kind(leaf.kind) {
                // Equal cell heights keep the shortest-column placement in
                // `render_leaf_run` on a row grid, so labels line up across
                // columns instead of drifting by a few pixels per field.
                let target = uniform_leaf_height(ui);
                ui.vertical(|ui| {
                    // egui reserves this height at the current cursor. Set
                    // it before drawing; afterwards it adds a blank cell
                    // below the editor instead of setting the total height.
                    ui.set_min_height(target);
                    draw(ui);
                });
            } else {
                ui.vertical(&mut draw);
            }
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
        Value::Number(number) => match number.as_f64() {
            // The echo quotes the value the editor shows, so it uses the same
            // decimal policy rather than serde's full float rendering.
            Some(v) => format_number(v, number_step(v, field.unit, leaf_decimals(field)).0),
            None => number.to_string(),
        },
        value => value.to_string(),
    };
    let unit = display_unit(field.unit);
    if unit.is_empty() || !value.is_number() {
        text
    } else {
        format!("{text} {unit}")
    }
}

fn slot_mut<'a>(values: &'a mut Value, name: &str, default: Value) -> Option<&'a mut Value> {
    let map = values.as_object_mut()?;
    Some(map.entry(name.to_owned()).or_insert(default))
}
