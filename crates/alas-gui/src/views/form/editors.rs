// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Editors for the individual leaf kinds: numbers, strings, options, lists.

use alas_config::{Entry, Field, Kind, OptionalValueKind};
use egui::{ComboBox, DragValue, RichText, Slider, TextEdit, Ui};
use serde_json::Value;

use super::display_unit;
use super::options::{
    bounds, display_option, format_number, is_editable, leaf_decimals, number_step, optional_hint,
    parse_number_or_sentinel, sentinel_text, value_as_str,
};
use super::{MENU_GLYPH, WEIGHT_MAX, WEIGHT_MIN};

// Each argument is an independent schema or presentation input used by the
// leaf editor; a bundle would make the dynamic-form boundary less explicit.
#[allow(clippy::too_many_arguments)]
pub(super) fn edit_leaf(
    ui: &mut Ui,
    field: &Field,
    kind: Kind,
    slot: &mut Value,
    id_prefix: &str,
    options: Option<&[String]>,
    label: &str,
) -> bool {
    // An optional field remains nullable even after it acquires a value.
    // `leaf.kind` describes that current value (and loses the inner type at
    // null), whereas this declared metadata tells us how to serialize edits.
    if let Entry::Leaf(leaf) = &field.entry {
        if let Some(optional_kind) = leaf
            .optional_value_kind
            .filter(|kind| *kind != OptionalValueKind::Other)
        {
            return edit_optional(ui, field, optional_kind, slot, id_prefix);
        }
    }
    match kind {
        Kind::Bool => {
            let mut v = slot.as_bool().unwrap_or(false);
            let response = ui.scope(|ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                ui.checkbox(&mut v, label)
            });
            if response.inner.changed() {
                *slot = Value::Bool(v);
                return true;
            }
            false
        }
        Kind::Int => {
            let mut v = slot.as_f64().unwrap_or(0.0);
            // A whole-number field renders as a whole number. egui would
            // otherwise derive one decimal place from the drag speed and the
            // display scale, so wheel and iteration counts read "0.0"/"30.0".
            // A declared sentinel reads as its word instead of as a count.
            let mut drag = DragValue::new(&mut v)
                .speed(1.0)
                .max_decimals(0)
                .custom_formatter(move |value, _| match sentinel_text(field, value) {
                    Some(word) => word,
                    None => format_number(value, 0),
                })
                .custom_parser(move |text| parse_number_or_sentinel(text, field))
                .suffix(unit_suffix(field));
            if let (Some(lo), Some(hi)) = bounds(field) {
                drag = drag.range(lo..=hi);
            }
            if super::options::zero_means_all_threads(field) {
                // Zero is the automatic setting; below it is no thread count.
                drag = drag.range(0.0..=f64::INFINITY);
            }
            let response = ui.add_sized([ui.available_width(), ui.spacing().interact_size.y], drag);
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
                .custom_formatter(move |value, _| match sentinel_text(field, value) {
                    Some(word) => word,
                    None => format_number(value, decimals),
                })
                .custom_parser(move |text| parse_number_or_sentinel(text, field))
                .suffix(unit_suffix(field));
            if let (Some(lo), Some(hi)) = bounds(field) {
                drag = drag.range(lo..=hi);
            }
            let response = ui.add_sized([ui.available_width(), ui.spacing().interact_size.y], drag);
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
                .suffix(unit_suffix(field));
            let response =
                ui.add_sized([ui.available_width(), ui.spacing().interact_size.y], slider);
            if response.changed() {
                *slot = Value::from(v);
                return true;
            }
            false
        }
        Kind::Str => edit_str(ui, field, slot, id_prefix, options),
        Kind::Optional => edit_optional(ui, field, OptionalValueKind::Other, slot, id_prefix),
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

pub(super) fn optional_value_from_text(
    text: &str,
    kind: OptionalValueKind,
    field: &Field,
) -> Option<Value> {
    if text.is_empty()
        || (!matches!(kind, OptionalValueKind::String | OptionalValueKind::Other)
            && text.trim().is_empty())
    {
        return Some(Value::Null);
    }
    match kind {
        OptionalValueKind::I64 => text.trim().parse::<i64>().ok().map(Value::from),
        OptionalValueKind::U32 => text.trim().parse::<u32>().ok().map(Value::from),
        OptionalValueKind::Usize => text.trim().parse::<usize>().ok().map(Value::from),
        OptionalValueKind::Float => {
            let number = parse_number_or_sentinel(text, field)?;
            serde_json::Number::from_f64(number).map(Value::Number)
        }
        OptionalValueKind::String | OptionalValueKind::Other => {
            Some(Value::String(text.to_owned()))
        }
        OptionalValueKind::Bool => match text.trim() {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
    }
}

/// Outer height of a numeric editor: a drag value sized to the interaction
/// height grows to its text plus button padding when that is taller.
pub(super) fn numeric_editor_height(ui: &Ui) -> f32 {
    let spacing = ui.spacing();
    spacing
        .interact_size
        .y
        .max(ui.text_style_height(&egui::TextStyle::Button) + 2.0 * spacing.button_padding.y)
}

pub(super) fn edit_optional(
    ui: &mut Ui,
    field: &Field,
    kind: OptionalValueKind,
    slot: &mut Value,
    id_prefix: &str,
) -> bool {
    if kind == OptionalValueKind::Bool {
        let previous = slot.clone();
        ComboBox::from_id_salt(format!("{id_prefix}::{}::optional", field.name))
            .width(ui.available_width())
            .selected_text(match slot {
                Value::Bool(true) => crate::views::tr("Yes"),
                Value::Bool(false) => crate::views::tr("No"),
                _ => crate::views::tr(optional_hint(field)),
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(slot, Value::Null, crate::views::tr(optional_hint(field)));
                ui.selectable_value(slot, Value::Bool(true), crate::views::tr("Yes"));
                ui.selectable_value(slot, Value::Bool(false), crate::views::tr("No"));
            });
        return *slot != previous;
    }

    // Keep a partially typed numeric value visible while the editor has
    // focus. A bare minus sign or exponent prefix is not a valid JSON number
    // yet; resetting it on every frame would make those values impossible to
    // type. Only complete, finite values are written to the configuration.
    let id = ui.make_persistent_id((id_prefix, field.name, "optional"));
    let mut text = ui
        .ctx()
        .data(|data| data.get_temp::<String>(id))
        .unwrap_or_else(|| match (kind, slot.as_f64()) {
            // Same decimal policy as the non-optional numeric editors.
            (OptionalValueKind::Float, Some(v)) => {
                format_number(v, number_step(v, field.unit, leaf_decimals(field)).0)
            }
            _ => value_as_str(slot).unwrap_or_default(),
        });
    let unit = display_unit(field.unit);
    let hint = crate::views::tr(optional_hint(field));
    let mut changed = false;
    ui.horizontal(|ui| {
        let unit_width = if unit.is_empty() {
            0.0
        } else {
            ui.fonts(|fonts| {
                fonts
                    .layout_no_wrap(
                        unit.clone(),
                        egui::TextStyle::Body.resolve(ui.style()),
                        ui.visuals().text_color(),
                    )
                    .size()
                    .x
            }) + ui.spacing().item_spacing.x
        };
        // `desired_width` sizes the text area only; the frame margin is added
        // outside it. Leaving it out made every optional field overflow its
        // column, which widened the column and pushed later editors into the
        // next one.
        let margin = egui::Margin::symmetric(4.0, 2.0);
        let text_width = ui.available_width() - unit_width - margin.sum().x;
        // The same outer height as the numeric editors beside it, with the
        // text centred, so a form row's boxes share top and bottom edges.
        let response = ui.add(
            TextEdit::singleline(&mut text)
                .id(id)
                .margin(margin)
                .hint_text(&hint)
                .desired_width(text_width.max(40.0))
                .min_size(egui::vec2(0.0, numeric_editor_height(ui)))
                .vertical_align(egui::Align::Center),
        );
        if !unit.is_empty() {
            ui.label(RichText::new(unit).weak());
        }
        if response.changed() {
            ui.ctx().data_mut(|data| data.insert_temp(id, text.clone()));
            if let Some(value) = optional_value_from_text(&text, kind, field) {
                if *slot != value {
                    *slot = value;
                    changed = true;
                }
            }
        }
        if response.lost_focus() {
            ui.ctx().data_mut(|data| data.remove::<String>(id));
        }
    });
    changed
}

/// The editor's value suffix: the field's displayed unit and nothing else.
///
/// A numeric editor renders its suffix inside its own value area, so nothing
/// but the unit belongs here: the modification marker is drawn once, in the
/// label row (`form_feedback::modified_marker`). The unit itself goes through
/// [`display_unit`], which is where the one dimensionless convention and the
/// unit typography live.
pub(super) fn unit_suffix(field: &Field) -> String {
    let unit = display_unit(field.unit);
    if unit.is_empty() {
        String::new()
    } else {
        format!(" {unit}")
    }
}

pub(super) fn edit_str(
    ui: &mut Ui,
    field: &Field,
    slot: &mut Value,
    id_prefix: &str,
    options: Option<&[String]>,
) -> bool {
    let current = value_as_str(slot).unwrap_or_default();

    match options {
        Some(opts) if !is_editable(field) => {
            let mut changed = false;
            let selected = if current.is_empty() {
                "-".to_owned()
            } else {
                display_option(field, &current)
            };
            // The closed box keeps one line so every cell in a form row has
            // the same height in either language; the full text is on hover
            // and the open list wraps.
            let response = ComboBox::from_id_salt(format!("{id_prefix}::{}", field.name))
                .width(ui.available_width())
                .truncate()
                .selected_text(selected.clone())
                .show_ui(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Wrap);
                    // Keep a value from an older save selectable even when it
                    // is no longer in the list.
                    if !current.is_empty()
                        && !opts.iter().any(|o| o == &current)
                        && ui
                            .selectable_label(true, display_option(field, &current))
                            .clicked()
                    {
                        changed = true;
                    }
                    for opt in opts {
                        let response =
                            ui.selectable_label(current == *opt, display_option(field, opt));
                        let response = if let Some(kind) =
                            super::options::objective_kind(field, opt)
                        {
                            response.on_hover_text(crate::views::tr(
                                alas_pipeline::optimizer_summary::objective::objective_help(kind),
                            ))
                        } else {
                            response
                        };
                        if response.clicked() {
                            *slot = Value::String(opt.clone());
                            changed = true;
                        }
                    }
                });
            let hover = match super::options::objective_kind(field, &current) {
                Some(kind) => format!(
                    "{selected}\n\n{}",
                    crate::views::tr(alas_pipeline::optimizer_summary::objective::objective_help(
                        kind
                    ))
                ),
                None => selected,
            };
            response.response.on_hover_text(hover);
            changed
        }
        Some(opts) => {
            // Editable option field (airfoils): a free-text box plus a menu of
            // the known names, since free text is still accepted downstream.
            let mut text = current.clone();
            let mut changed = false;
            ui.horizontal(|ui| {
                let margin = egui::Margin::symmetric(4.0, 2.0);
                let menu_width = ui.spacing().interact_size.y + ui.spacing().item_spacing.x;
                let text_width = ui.available_width() - menu_width - margin.sum().x - 8.0;
                changed = ui
                    .add(
                        TextEdit::singleline(&mut text)
                            .margin(margin)
                            .desired_width(text_width.max(80.0)),
                    )
                    .changed();
                if changed {
                    *slot = Value::String(text.clone());
                }
                ui.menu_button(MENU_GLYPH, |ui| {
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
                })
                .response
                .on_hover_text(crate::views::tr("Choose from the library"));
            });
            changed
        }
        None => {
            let mut text = current;
            let response =
                ui.add(TextEdit::singleline(&mut text).desired_width(ui.available_width()));
            if response.changed() {
                *slot = Value::String(text);
                return true;
            }
            false
        }
    }
}

pub(super) fn edit_number_list(ui: &mut Ui, slot: &mut Value) -> bool {
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

pub(super) fn edit_tuple_list(ui: &mut Ui, field: &Field, slot: &mut Value) -> bool {
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
