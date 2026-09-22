// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

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
        Kind::Optional => {
            // An empty optional used to render as a blank box with no
            // placeholder and no unit, so "derived", "random" and "never set"
            // all looked alike, and three optional lengths stated their metre
            // unit nowhere in the interface.
            let mut text = value_as_str(slot).unwrap_or_default();
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
                let response = ui.add(
                    TextEdit::singleline(&mut text)
                        .hint_text(&hint)
                        .desired_width((ui.available_width() - unit_width).max(40.0)),
                );
                if !unit.is_empty() {
                    ui.label(RichText::new(unit).weak());
                }
                if response.changed() {
                    *slot = if text.is_empty() {
                        Value::Null
                    } else {
                        Value::String(text)
                    };
                    changed = true;
                }
            });
            changed
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

/// The editor's value suffix: the field's displayed unit and nothing else.
///
/// A numeric editor renders its suffix inside its own value area, so nothing
/// but the unit belongs here: the modification marker is drawn once, in the
/// label row (`form_feedback::modified_marker`). The unit itself goes through
/// [`display_unit`], which is where the one dimensionless convention and the
/// unit typography live.
fn unit_suffix(field: &Field) -> String {
    let unit = display_unit(field.unit);
    if unit.is_empty() {
        String::new()
    } else {
        format!(" {unit}")
    }
}

fn edit_str(
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
            ComboBox::from_id_salt(format!("{id_prefix}::{}", field.name))
                .width(ui.available_width())
                .selected_text(if current.is_empty() {
                    "-".to_owned()
                } else {
                    display_option(&current)
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
#[path = "../form_tests.rs"]
mod tests;
