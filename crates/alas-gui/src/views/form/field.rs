// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rendering of one schema field: label row, reset and library affordances, editor.

use std::collections::HashSet;

use alas_config::{Entry, Field, Kind};
use egui::Ui;
use serde_json::Value;

use super::display_scale::display_scale;
use super::editors::edit_leaf;
use super::feedback::{differs_from_default, modified_marker};
use super::help::help_on_hover;
use super::layout::render_fields;
use super::number_text::format_number;
use super::options::{leaf_decimals, number_step, readonly_unless, resolved_options};
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
    ancestors: &[&Value],
    edits: &mut Vec<FormEdit>,
    grid: bool,
) -> bool {
    let label = super::text_cache::label(field.label, lang);
    let help = super::text_cache::help(field.help, lang);
    let long_help = super::text_cache::help(field.long_help, lang);

    match &field.entry {
        Entry::Node(node) => {
            let mut changed = false;
            let child_id = format!("{id_prefix}::{}", field.name);
            let focus = crate::search::published_focus(ui.ctx());
            // The header itself, or any control below it, is the search
            // target: the card opens so the target is drawn.
            let on_focus_path = focus.as_ref().is_some_and(|focus| {
                focus.tail == child_id || focus.tail.starts_with(&format!("{child_id}::"))
            });
            let mut header_rect = None;
            let card = crate::theme::card_frame(ui).show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                // The title style follows the card's depth on the page.
                let title = crate::theme::card_title(ui, label.clone());
                let header = egui::CollapsingHeader::new(title)
                    .id_salt(&child_id)
                    // Large groups such as geometry stations and cabin
                    // classes start compact. Only a caller's direct root
                    // groups may opt into an expanded landing state.
                    .default_open(open_by_default)
                    .open(if on_focus_path {
                        Some(true)
                    } else {
                        crate::perf_tour::force_open()
                    });
                let header = crate::theme::show_header(ui, header, |ui| {
                    // The child is moved out of its parent for the call so
                    // the parent can be lent by reference as an ancestor
                    // (a deep copy of every section array per open node,
                    // per frame, was the Aircraft tab's dominant cost).
                    // Ancestors are only read for sibling-path lookups.
                    let taken = values.as_object_mut().map(|m| {
                        let slot = m
                            .entry(field.name.to_owned())
                            .or_insert_with(|| Value::Object(serde_json::Map::new()));
                        std::mem::take(slot)
                    });
                    if let Some(mut child) = taken {
                        let mut child_ancestors: Vec<&Value> =
                            Vec::with_capacity(ancestors.len() + 1);
                        child_ancestors.push(&*values);
                        child_ancestors.extend_from_slice(ancestors);
                        changed = render_fields(
                            ui,
                            &node.fields,
                            &mut child,
                            error_fields,
                            lang,
                            show_help,
                            false,
                            &child_id,
                            &child_ancestors,
                            edits,
                        );
                        drop(child_ancestors);
                        if let Some(m) = values.as_object_mut() {
                            m.insert(field.name.to_owned(), child);
                        }
                    }
                });
                header_rect = Some(header.header_response.rect);
                help_on_hover(header.header_response, &label, &help, &long_help);
                super::pack::equalise(ui);
            });
            if let Some(header_rect) = header_rect {
                // Same pulse as a field; a tall card centres on its header.
                crate::search::focus_anchor_tail(ui, &child_id, card.response.rect, header_rect);
            }
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
            // `readonly` is this field's own gate (it shows a lock icon); a
            // page locked as a whole (`with_locked_editors`) disables the
            // editor the same way but carries one lock row for all of them.
            let disabled = readonly || super::editors_locked(ui.ctx());
            // The value of the aircraft the session started from, else the
            // schema default (see `baseline`).
            let reference = super::baseline::value_of(id_prefix, field.name)
                .unwrap_or_else(|| leaf.value.clone());
            let modified = !disabled && differs_from_default(slot, &reference);
            let lock_reason = tr(leaf
                .readonly_unless
                .map_or("Custom Cabin Only", lock_reason_for));
            let mut draw = |ui: &mut Ui| {
                // Caption, box and note of one field stand close together.
                ui.spacing_mut().item_spacing.y = crate::theme::space::LABEL_TO_BOX;
                // The shared label style: normal colour while the field can
                // be edited, weak only when it cannot.
                let mut text = crate::theme::field_label(&label, !disabled);
                if has_error {
                    text = text.color(ui.visuals().error_fg_color);
                }
                if leaf.kind == Kind::Bool && leaf.optional_value_kind.is_none() {
                    if grid {
                        // In a grid a checkbox is a cell like the others: the
                        // same caption as a text field, its markers on the
                        // caption's last line, the tick in a box under it.
                        let styled = crate::notation::styled(ui, text.clone());
                        let room = crate::theme::space::MARKER_ROOM;
                        super::caption::caption(ui, styled, room, |ui| {
                            if readonly {
                                crate::theme::lock_icon(ui, &lock_reason);
                            }
                            modified_marker(ui, modified);
                        });
                    }
                    // In a grid the checkbox stands in a cell as tall and as
                    // wide as the value boxes of its row, so it lines up with
                    // them instead of floating in an empty cell.
                    let cell = grid.then(|| {
                        let height = super::editors::numeric_editor_height(ui);
                        let width = ui.available_width();
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click());
                        (rect, response)
                    });
                    let mut draw_checkbox = |ui: &mut Ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.add_enabled_ui(!disabled, |ui| {
                                changed = edit_leaf(
                                    ui,
                                    field,
                                    leaf.kind,
                                    slot,
                                    id_prefix,
                                    options.as_deref(),
                                    if grid { "" } else { &label },
                                );
                            });
                            // Beside the label of a checkbox outside a grid.
                            if !grid {
                                if readonly {
                                    crate::theme::lock_icon(ui, &lock_reason);
                                }
                                modified_marker(ui, modified);
                            }
                        });
                    };
                    let cell_click = match cell {
                        Some((rect, response)) => {
                            // Framed like a value box, the tick at its inner
                            // padding: the caption stands on a visible box,
                            // as close as on every other cell of the row.
                            value_box_frame(ui, rect, &response, !disabled);
                            let pad = ui.spacing().button_padding.x;
                            let inner = rect.shrink2(egui::vec2(pad, 0.0));
                            let mut child = ui.new_child(
                                egui::UiBuilder::new()
                                    .max_rect(inner)
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                            );
                            draw_checkbox(&mut child);
                            Some(response)
                        }
                        None => {
                            draw_checkbox(ui);
                            None
                        }
                    };
                    // A click on the cell outside the checkbox toggles it too.
                    if cell_click.is_some_and(|response| response.clicked())
                        && !disabled
                        && !changed
                    {
                        let on = slot.as_bool().unwrap_or(false);
                        *slot = Value::Bool(!on);
                        changed = true;
                    }
                } else {
                    // The caption is as wide as the value box under it: the
                    // marker and the reset glyph sit on its last line over
                    // the box's right edge, in the room the caption always
                    // keeps for them (so a modified field never re-wraps).
                    let styled = crate::notation::styled(ui, text);
                    let room = crate::theme::space::MARKER_ROOM;
                    super::caption::caption(ui, styled, room, |ui| {
                        // A framed reset glyph, not a bare ASCII arrow:
                        // `<-` rendered as dim body text beside the label
                        // with no frame, no colour coding and no legend,
                        // and could not be told from the label itself.
                        if !disabled
                            && differs_from_default(slot, &reference)
                            && ui
                                .add(egui::Button::new(RESET_GLYPH).small())
                                .on_hover_text(tr("Reset this value"))
                                .clicked()
                        {
                            *slot = reference.clone();
                            changed = true;
                        }
                        if readonly {
                            crate::theme::lock_icon(ui, &lock_reason);
                        }
                        modified_marker(ui, modified && !changed);
                    });
                    // The value box fills its cell.
                    ui.add_enabled_ui(!disabled, |ui| {
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
            // The grid levels the row: the caption band is the row's, the
            // cell as tall as what it holds.
            let cell = ui.vertical(&mut draw).response.rect;
            // The help is on the whole cell (label, gap and value box), so it shows wherever the
            // pointer rests in it and its "Docs" link stays reachable while the pointer moves onto
            // the tooltip.
            if !help.trim().is_empty() {
                let overlay = ui.interact(
                    cell,
                    ui.id().with(("field_help", id_prefix, field.name)),
                    egui::Sense::hover(),
                );
                help_on_hover(
                    super::help::hovered_by_pointer(overlay),
                    &label,
                    &help,
                    &long_help,
                );
            }
            crate::search::focus_anchor_tail(
                ui,
                &format!("{id_prefix}::{}", field.name),
                cell,
                cell,
            );
            if changed {
                edits.push(FormEdit {
                    label,
                    value: feedback_value(
                        field,
                        slot,
                        options.is_some() && !super::options::is_editable(field),
                    ),
                });
            }
            changed
        }
    }
}

/// Paint the frame of a value box (fill, stroke and rounding of an egui
/// value widget in its interaction state, faded like one when disabled)
/// around `rect`, for a cell whose content is not itself framed, such as a
/// checkbox.
fn value_box_frame(ui: &mut Ui, rect: egui::Rect, response: &egui::Response, enabled: bool) {
    let visuals = if enabled {
        *ui.style().interact(response)
    } else {
        ui.visuals().widgets.inactive
    };
    let mut frame = ui.new_child(egui::UiBuilder::new().max_rect(rect));
    if !enabled {
        frame.disable();
    }
    frame.painter().rect(
        rect.expand(visuals.expansion),
        visuals.rounding,
        visuals.weak_bg_fill,
        visuals.bg_stroke,
    );
}

/// Why a field is read-only, from the condition that unlocks it.
fn lock_reason_for(condition: alas_config::ReadonlyUnless) -> &'static str {
    if condition.field.ends_with("cabin_preset") {
        "Custom Cabin Only"
    } else {
        "Fixed by Reference Aircraft"
    }
}

/// The value an edit reports (the status line, the change log) as the field
/// shows it: a choice by its display name, a switch as On or Off, a number
/// with the editor's decimals and unit.
fn feedback_value(field: &Field, value: &Value, choice: bool) -> String {
    let text = match value {
        Value::String(value) if choice => super::options::display_option(field, value),
        Value::String(value) => value.clone(),
        Value::Bool(value) => tr(if *value { "On" } else { "Off" }),
        Value::Null => tr("none"),
        Value::Number(number) => match number.as_f64() {
            // The echo quotes the value the editor shows, so it uses the same
            // decimal policy rather than serde's full float rendering.
            Some(v) => match display_scale(field) {
                Some(scale) => scale.text(v),
                None => format_number(v, number_step(v, field.unit, leaf_decimals(field)).0),
            },
            None => number.to_string(),
        },
        value => value.to_string(),
    };
    let unit = display_scale(field).map_or_else(|| display_unit(field.unit), |s| s.unit.to_owned());
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
