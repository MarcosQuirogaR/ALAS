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
#[path = "../form_tests.rs"]
mod tests;

