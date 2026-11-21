// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Editors for the individual leaf kinds: numbers, strings, options, lists.

use alas_config::{Entry, Field, Kind, OptionalValueKind};
use egui::{ComboBox, DragValue, Id, RichText, TextEdit, Ui};

/// Option lists longer than this get a search box in their drop-down.
const LONG_LIST_SEARCH_THRESHOLD: usize = 12;
use serde_json::Value;

use super::display_scale::display_scale;
use super::display_unit;
use super::number_text::format_number;
use super::options::{
    bounds, display_option, is_editable, leaf_decimals, number_step, optional_hint,
    parse_number_or_sentinel, sentinel_text, value_as_str,
};
use super::table_editors::{edit_number_list, edit_tuple_list};
use super::unit_display::value_box;
use super::{WEIGHT_MAX, WEIGHT_MIN};

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
                crate::checkbox::checkbox(ui, &mut v, label)
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
            let suffix = value_suffix(field, v);
            let mut drag = DragValue::new(&mut v)
                .speed(1.0)
                .max_decimals(0)
                .custom_parser(move |text| parse_number_or_sentinel(text, field));
            if let (Some(lo), Some(hi)) = bounds(field) {
                drag = drag.range(lo..=hi);
            }
            if super::options::zero_means_all_threads(field) {
                // Zero is the automatic setting; below it is no thread count.
                drag = drag.range(0.0..=f64::INFINITY);
            }
            let format = move |value| match sentinel_text(field, value) {
                Some(word) => word,
                None => format_number(value, 0),
            };
            let response = value_box(ui, drag, format, suffix);
            if response.changed() {
                *slot = Value::from(v.round() as i64);
                return true;
            }
            false
        }
        Kind::Float => {
            let scale = display_scale(field);
            let factor = scale.map_or(1.0, |scale| scale.factor);
            let mut v = slot.as_f64().unwrap_or(0.0) * factor;
            let (decimals, step) = match scale {
                Some(scale) => (scale.decimals, 0.1),
                None => number_step(v, field.unit, leaf_decimals(field)),
            };
            let suffix = value_suffix(field, v / factor);
            let mut drag = DragValue::new(&mut v)
                .speed(step)
                .max_decimals(decimals)
                .custom_parser(move |text| parse_number_or_sentinel(text, field));
            if let (Some(lo), Some(hi)) = bounds(field) {
                drag = drag.range(lo * factor..=hi * factor);
            }
            let format = move |value| match sentinel_text(field, value) {
                Some(word) => word,
                None => format_number(value, decimals),
            };
            let response = value_box(ui, drag, format, suffix);
            if response.changed() {
                *slot = Value::from(v / factor);
                return true;
            }
            false
        }
        Kind::WeightSlider => {
            // A weight is a number like any other: the same value box, within
            // the weight range, rather than a thin slider beside a small box.
            let mut v = slot
                .as_f64()
                .unwrap_or(WEIGHT_MIN)
                .clamp(WEIGHT_MIN, WEIGHT_MAX);
            let (decimals, step) = number_step(v, field.unit, leaf_decimals(field));
            let drag = DragValue::new(&mut v)
                .speed(step)
                .range(WEIGHT_MIN..=WEIGHT_MAX)
                .max_decimals(decimals)
                .custom_parser(move |text| parse_number_or_sentinel(text, field));
            let format = move |value| format_number(value, decimals);
            let response = value_box(ui, drag, format, unit_suffix(field));
            if response.changed() {
                *slot = Value::from(v);
                return true;
            }
            false
        }
        Kind::Str => edit_str(ui, field, slot, id_prefix, options),
        Kind::Optional => edit_optional(ui, field, OptionalValueKind::Other, slot, id_prefix),
        Kind::NumberList => edit_number_list(ui, field, slot),
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
            .icon(crate::combo_icon::down_arrow)
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
            (OptionalValueKind::Float, Some(v)) => match display_scale(field) {
                Some(scale) => scale.text(v),
                None => format_number(v, number_step(v, field.unit, leaf_decimals(field)).0),
            },
            _ => value_as_str(slot).unwrap_or_default(),
        });
    let unit = display_scale(field).map_or_else(|| display_unit(field.unit), |s| s.unit.to_owned());
    // An empty optional limit the registered preset declares shows that value,
    // not "Not set": the checks use it (see `design_space_view`).
    let preset_value: Option<f64> = ui
        .ctx()
        .data(|data| data.get_temp(Id::new(("optional_preset_value", field.name))));
    let hint = match preset_value {
        Some(value) => format!(
            "{} {unit} ({})",
            format_number(value, number_step(value, field.unit, leaf_decimals(field)).0),
            crate::views::tr("from preset")
        ),
        None => crate::views::tr(optional_hint(field)),
    };
    let mut changed = false;
    {
        // The unit follows the value inside the box exactly as it does in the
        // numeric editors (`200000 kg`, centred). While the box is being
        // edited the bare number is shown, as a numeric editor does, and an
        // empty box shows the hint without a unit.
        let width = ui.available_width();
        let editing = ui.memory(|memory| memory.has_focus(id));
        let mut shown = if editing || text.is_empty() || unit.is_empty() {
            text.clone()
        } else {
            format!("{text} {unit}")
        };
        // The same outer height as the numeric editors beside it, with the
        // text centred, so a form row's boxes share top and bottom edges.
        let response = ui.add(
            TextEdit::singleline(&mut shown)
                .id(id)
                .margin(egui::Margin::symmetric(4.0, 2.0))
                .hint_text(&hint)
                .desired_width((width - 8.0).max(40.0))
                .horizontal_align(egui::Align::Center)
                .min_size(egui::vec2(width, numeric_editor_height(ui)))
                .vertical_align(egui::Align::Center),
        );
        if response.changed() && editing {
            text = shown;
            ui.ctx().data_mut(|data| data.insert_temp(id, text.clone()));
            if let Some(mut value) = optional_value_from_text(&text, kind, field) {
                if let (Some(scale), Some(number)) = (display_scale(field), value.as_f64()) {
                    value = Value::from(scale.to_stored(number));
                }
                if *slot != value {
                    *slot = value;
                    changed = true;
                }
            }
        }
        if response.lost_focus() {
            ui.ctx().data_mut(|data| data.remove::<String>(id));
        }
    }
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
    let unit = display_scale(field).map_or_else(|| display_unit(field.unit), |s| s.unit.to_owned());
    if unit.is_empty() {
        String::new()
    } else {
        format!(" {unit}")
    }
}

/// The suffix for the value shown now: a sentinel word ("No limit", "Auto")
/// is not a quantity, so it carries no unit.
fn value_suffix(field: &Field, stored: f64) -> String {
    if sentinel_text(field, stored).is_some() {
        String::new()
    } else {
        // An altitude in metres also reads as the flight level it stands for.
        if field.name == "cruise_altitude_m" && field.unit == "m" {
            return format!(
                "{} \u{b7} FL{:.0}",
                unit_suffix(field),
                (stored / 0.3048 / 100.0).max(0.0)
            );
        }
        unit_suffix(field)
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
                .icon(crate::combo_icon::down_arrow)
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
                    // A long list (more than a screenful) gets a search box.
                    let search_salt = format!("{id_prefix}::{}", field.name);
                    let query = if opts.len() > LONG_LIST_SEARCH_THRESHOLD {
                        crate::selector_search::search_field(ui, &search_salt)
                    } else {
                        String::new()
                    };
                    for opt in opts {
                        if !crate::selector_search::matches(
                            &query,
                            &[opt.as_str(), display_option(field, opt).as_str()],
                        ) {
                            continue;
                        }
                        let response =
                            ui.selectable_label(current == *opt, display_option(field, opt));
                        let response =
                            if let Some(kind) = super::options::objective_kind(field, opt) {
                                crate::notation::hover(
                                    response,
                                    crate::views::tr(
                                        alas_pipeline::optimizer_summary::objective::objective_help(
                                            kind,
                                        ),
                                    ),
                                )
                            } else {
                                response
                            };
                        if response.clicked() {
                            *slot = Value::String(opt.clone());
                            changed = true;
                        }
                    }
                });
            if opts.len() > LONG_LIST_SEARCH_THRESHOLD {
                crate::selector_search::reset_when_closed(
                    ui,
                    &format!("{id_prefix}::{}", field.name),
                    &response.inner,
                );
            }
            let hover = match super::options::objective_kind(field, &current) {
                Some(kind) => format!(
                    "{selected}\n\n{}",
                    crate::views::tr(alas_pipeline::optimizer_summary::objective::objective_help(
                        kind
                    ))
                ),
                None => {
                    // The box already shows a value that fits; repeating it in
                    // a tooltip would only cover the field below.
                    let font = egui::TextStyle::Button.resolve(ui.style());
                    let needed = ui.fonts(|fonts| {
                        fonts
                            .layout_no_wrap(selected.clone(), font, ui.visuals().text_color())
                            .size()
                            .x
                    });
                    if needed + 40.0 <= response.response.rect.width() {
                        return changed;
                    }
                    selected
                }
            };
            crate::notation::hover(response.response, hover);
            changed
        }
        // Editable option field (airfoils): one searchable drop-down of the
        // built-in airfoils, then the user's own, then an import entry.
        Some(opts) => {
            super::airfoil_combo::edit_airfoil(ui, field, slot, id_prefix, opts, &current)
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
