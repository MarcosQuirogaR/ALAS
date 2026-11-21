// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compact, schema-backed placement for the Cabin & Cargo page.

use std::collections::HashSet;

use alas_config::{Entry, Field};
use egui::{RichText, Ui};
use serde_json::Value;

use super::cabin_cards::{match_row_height, sub_heading};
use crate::views::form::{dynamic_form, dynamic_form_at_help, FormEdit};
use crate::views::tr;

const CLASS_NAMES: [&str; 3] = ["first", "business", "economy"];

/// Page width from which the four cargo groups stand side by side.
const WIDE_CARGO_WIDTH: f32 = 1_100.0;
/// From this width each of two halves holds a group's editors in several
/// columns, so the groups pair up again.
const WIDEST_CARGO_WIDTH: f32 = 1_800.0;

const CARGO_GROUPS: [(&str, &[&str]); 4] = [
    (
        "Decks",
        &["use_main_deck", "main_deck_uld", "lower_deck_uld"],
    ),
    (
        "Loading & balance",
        &["loading_strategy", "target_cg_pct_mac"],
    ),
    (
        "Door locations",
        &["main_door_x_m", "fwd_door_x_m", "aft_door_x_m"],
    ),
    ("CG trim", &["cg_trim_step_kg", "cg_trim_max_iterations"]),
];

// Each argument is an independent editor input; a wrapper struct would only rename them.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_cabin_editor(
    ui: &mut Ui,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    custom_preset: bool,
) -> Vec<FormEdit> {
    let mut edits = Vec::new();
    for field in fields {
        let Entry::Node(node) = &field.entry else {
            edits.extend(dynamic_form(
                ui,
                std::slice::from_ref(field),
                values,
                error_fields,
                lang,
                show_help,
            ));
            continue;
        };
        let Some(child) = child_values(values, field.name) else {
            continue;
        };
        // The path form a search jump targets, below the `cabin` group.
        let prefix = format!("::{}", field.name);
        // A search jump to a field of this section opens it.
        let forced = crate::search::focus_wants(ui.ctx(), &format!("cabin.{}", field.name));
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let mut header = egui::CollapsingHeader::new(crate::theme::card_title(
                ui,
                crate::views::form_label(field.label, lang),
            ))
            .id_salt(format!("cabin::{}", field.name))
            .default_open(true)
            .open(crate::perf_tour::force_open());
            if forced {
                header = header.open(Some(true));
            }
            let section = crate::theme::show_header(ui, header, |ui| match field.name {
                "passenger" => render_passenger(
                    ui,
                    &prefix,
                    &node.fields,
                    child,
                    error_fields,
                    lang,
                    show_help,
                    custom_preset,
                    &mut edits,
                ),
                "cargo" => render_cargo(
                    ui,
                    &prefix,
                    &node.fields,
                    child,
                    error_fields,
                    lang,
                    show_help,
                    &mut edits,
                ),
                _ => edits.extend(dynamic_form_at_help(
                    ui,
                    &node.fields,
                    child,
                    error_fields,
                    lang,
                    show_help,
                    &prefix,
                )),
            });
            if !field.help.is_empty() {
                section
                    .header_response
                    .on_hover_text(alas_i18n::t(Some(field.help), lang));
            }
        });
        ui.add_space(6.0);
    }
    edits
}

// Each argument is an independent editor input; a wrapper struct would only rename them.
#[allow(clippy::too_many_arguments)]
fn render_passenger(
    ui: &mut Ui,
    prefix: &str,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    custom_preset: bool,
    edits: &mut Vec<FormEdit>,
) {
    let classes: Vec<&Field> = CLASS_NAMES
        .iter()
        .filter_map(|name| fields.iter().find(|field| field.name == *name))
        .collect();
    // All three class cards contain the same one-field editor, so they form
    // one aligned row on a wide page. At smaller widths they stack cleanly.
    if ui.available_width() >= 1_080.0 && classes.len() == 3 {
        ui.columns(3, |columns| {
            for (column, class) in columns.iter_mut().zip(&classes) {
                render_class(
                    column,
                    prefix,
                    class,
                    values,
                    error_fields,
                    lang,
                    show_help,
                    custom_preset,
                    edits,
                    true,
                );
            }
        });
    } else {
        for class in classes {
            render_class(
                ui,
                prefix,
                class,
                values,
                error_fields,
                lang,
                show_help,
                custom_preset,
                edits,
                false,
            );
            ui.add_space(4.0);
        }
    }

    let remaining: Vec<Field> = fields
        .iter()
        .filter(|field| !CLASS_NAMES.contains(&field.name))
        .cloned()
        .collect();
    if !remaining.is_empty() {
        ui.add_space(6.0);
        edits.extend(dynamic_form_at_help(
            ui,
            &remaining,
            values,
            error_fields,
            lang,
            show_help,
            prefix,
        ));
    }
}

// Each argument is an independent editor input; a wrapper struct would only rename them.
#[allow(clippy::too_many_arguments)]
fn render_class(
    ui: &mut Ui,
    prefix: &str,
    field: &Field,
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    custom_preset: bool,
    edits: &mut Vec<FormEdit>,
    // The three cards stand in one row: pad each to the tallest, so a
    // disabled class does not leave a short card and an empty block.
    equal_height: bool,
) {
    let Entry::Node(node) = &field.entry else {
        return;
    };
    // A class switched back on without a remembered share takes the mean share of
    // the other present classes (the mix is normalised, only ratios matter).
    let others: Vec<f64> = CLASS_NAMES
        .iter()
        .filter(|name| **name != field.name)
        .filter_map(|name| values.get(*name)?.get("share_pct")?.as_f64())
        .filter(|share| *share > 0.0)
        .collect();
    let fallback_share = if others.is_empty() {
        100.0
    } else {
        others.iter().sum::<f64>() / others.len() as f64
    };
    let Some(class_values) = child_values(values, field.name) else {
        return;
    };
    let mut fields = node.fields.clone();
    if custom_preset {
        // The condition names a root sibling (requirements.cabin_preset).
        // The class editor receives only the nested class JSON value, so the
        // page resolves this condition before handing the field to the form.
        for editable in &mut fields {
            if let Entry::Leaf(leaf) = &mut editable.entry {
                leaf.readonly_unless = None;
            }
        }
    }
    let enabled = class_values
        .get("share_pct")
        .and_then(Value::as_f64)
        .is_some_and(|share| share > 0.0)
        || class_values
            .get("count")
            .and_then(Value::as_i64)
            .is_some_and(|count| count > 0);
    let memory_id = egui::Id::new(("cabin_class_share", field.name));
    let class_prefix = format!("{prefix}::{}", field.name);
    let card = crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        crate::theme::header_row(ui, |ui| {
            ui.horizontal(|ui| {
                let title =
                    crate::theme::card_title(ui, crate::views::form_label(field.label, lang));
                ui.label(title)
                    .on_hover_text(alas_i18n::t(Some(field.help), lang));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !custom_preset {
                        crate::theme::lock_icon(ui, &tr("Custom Cabin Only"));
                    }
                    let mut on = enabled;
                    let toggled = ui
                        .add_enabled_ui(custom_preset, |ui| {
                            crate::checkbox::checkbox(ui, &mut on, tr("Enabled")).changed()
                        })
                        .inner;
                    if toggled {
                        let share = if on {
                            ui.data(|data| data.get_temp::<f64>(memory_id))
                                .unwrap_or(fallback_share)
                        } else {
                            let current = class_values
                                .get("share_pct")
                                .and_then(Value::as_f64)
                                .unwrap_or(fallback_share);
                            ui.data_mut(|data| data.insert_temp(memory_id, current));
                            0.0
                        };
                        class_values["share_pct"] = Value::from(share);
                        if !on {
                            class_values["count"] = Value::from(0);
                        }
                        edits.push(FormEdit {
                            label: tr("Enabled"),
                            value: tr(if on { "On" } else { "Off" }),
                        });
                    }
                });
            })
        });
        if enabled {
            edits.extend(dynamic_form_at_help(
                ui,
                &fields,
                class_values,
                error_fields,
                lang,
                show_help,
                &class_prefix,
            ));
        } else {
            ui.label(RichText::new(crate::views::tr_label("Class Disabled")).weak());
        }
        if equal_height {
            let natural = ui.min_rect().height();
            match_row_height(ui, egui::Id::new("cabin_class_row"), natural);
        }
    });
    // A disabled class draws no fields: the search pulses its card instead.
    if !enabled {
        let class_path = format!("cabin{}", class_prefix.replace("::", "."));
        if let Some(focus) = crate::search::published_focus(ui.ctx()) {
            if focus.path.starts_with(&class_path) {
                crate::search::focus_anchor(ui, &focus.path, card.response.rect);
            }
        }
    }
}

// Each argument is an independent editor input; a wrapper struct would only rename them.
#[allow(clippy::too_many_arguments)]
fn render_cargo(
    ui: &mut Ui,
    prefix: &str,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    edits: &mut Vec<FormEdit>,
) {
    let width = ui.available_width();
    if (WIDE_CARGO_WIDTH..WIDEST_CARGO_WIDTH).contains(&width) {
        // One column per group: each group's editors take the width of one
        // value box, so the row has no empty area beside any of them.
        ui.columns(CARGO_GROUPS.len(), |columns| {
            for (index, column) in columns.iter_mut().enumerate() {
                render_cargo_group(
                    column,
                    prefix,
                    index,
                    fields,
                    values,
                    error_fields,
                    lang,
                    show_help,
                    edits,
                );
            }
        });
    } else if width >= 640.0 {
        // Two rows of two blocks, so the sub-headings of a row start on one line: decks beside
        // the door locations, then loading beside the CG trim.
        for pair in [[0, 2], [1, 3]] {
            ui.columns(2, |columns| {
                for (column, index) in columns.iter_mut().zip(pair) {
                    render_cargo_group(
                        column,
                        prefix,
                        index,
                        fields,
                        values,
                        error_fields,
                        lang,
                        show_help,
                        edits,
                    );
                }
            });
        }
    } else {
        for index in 0..CARGO_GROUPS.len() {
            render_cargo_group(
                ui,
                prefix,
                index,
                fields,
                values,
                error_fields,
                lang,
                show_help,
                edits,
            );
        }
    }
    let remaining: Vec<Field> = fields
        .iter()
        .filter(|field| {
            !CARGO_GROUPS
                .iter()
                .any(|(_, names)| names.contains(&field.name))
        })
        .cloned()
        .collect();
    if !remaining.is_empty() {
        edits.extend(dynamic_form_at_help(
            ui,
            &remaining,
            values,
            error_fields,
            lang,
            show_help,
            prefix,
        ));
    }
}

// Each argument is an independent editor input; a wrapper struct would only rename them.
#[allow(clippy::too_many_arguments)]
fn render_cargo_group(
    ui: &mut Ui,
    prefix: &str,
    index: usize,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    edits: &mut Vec<FormEdit>,
) {
    let (title, names) = CARGO_GROUPS[index];
    let group: Vec<Field> = fields
        .iter()
        .filter(|field| names.contains(&field.name))
        .cloned()
        .collect();
    if group.is_empty() {
        return;
    }
    sub_heading(ui, &tr(title));
    if index == 0 {
        // The checkbox is one line tall; put it above the paired ULD
        // selectors so their labels and editors line up with each other.
        let toggle: Vec<Field> = group
            .iter()
            .filter(|field| field.name == "use_main_deck")
            .cloned()
            .collect();
        let selectors: Vec<Field> = group
            .iter()
            .filter(|field| field.name != "use_main_deck")
            .cloned()
            .collect();
        edits.extend(dynamic_form_at_help(
            ui,
            &toggle,
            values,
            error_fields,
            lang,
            show_help,
            prefix,
        ));
        edits.extend(dynamic_form_at_help(
            ui,
            &selectors,
            values,
            error_fields,
            lang,
            show_help,
            prefix,
        ));
    } else {
        edits.extend(dynamic_form_at_help(
            ui,
            &group,
            values,
            error_fields,
            lang,
            show_help,
            prefix,
        ));
    }
    ui.add_space(10.0);
}

fn child_values<'a>(values: &'a mut Value, name: &str) -> Option<&'a mut Value> {
    let map = values.as_object_mut()?;
    Some(
        map.entry(name.to_owned())
            .or_insert_with(|| Value::Object(serde_json::Map::new())),
    )
}

#[cfg(test)]
mod tests {
    use super::{render_cabin_editor, CARGO_GROUPS};
    use alas_config::{AlasConfig, CargoDeckConfig, ConfigNode, Entry};

    #[test]
    fn cargo_groups_cover_every_current_editor_once() {
        let actual: Vec<&str> = CargoDeckConfig::default()
            .schema()
            .fields
            .iter()
            .map(|field| field.name)
            .collect();
        let grouped: Vec<&str> = CARGO_GROUPS
            .iter()
            .flat_map(|(_, names)| names.iter().copied())
            .collect();
        let mut actual_sorted = actual.clone();
        let mut grouped_sorted = grouped;
        actual_sorted.sort_unstable();
        grouped_sorted.sort_unstable();
        assert_eq!(grouped_sorted, actual_sorted);
    }

    #[test]
    fn a_class_that_is_off_shows_one_disabled_note_and_never_a_format_placeholder() {
        let labels = rendered_positions(1_400.0);
        assert!(
            labels
                .iter()
                .any(|(text, _)| *text == crate::views::tr_label("Class Disabled")),
            "{labels:?}"
        );
        assert!(labels.iter().all(|(text, _)| !text.contains("{2014}")));
    }

    fn rendered_positions(width: f32) -> Vec<(String, egui::Pos2)> {
        fn collect(shape: &egui::Shape, labels: &mut Vec<(String, egui::Pos2)>) {
            match shape {
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        collect(shape, labels);
                    }
                }
                egui::Shape::Text(text) => {
                    labels.push((text.galley.text().to_owned(), text.pos));
                }
                _ => {}
            }
        }

        let config = AlasConfig::default();
        let schema = config.schema();
        let Entry::Node(cabin) = &schema.field("cabin").expect("cabin schema").entry else {
            panic!("cabin is a group");
        };
        let mut values = serde_json::to_value(config).expect("default config serializes");
        let ctx = egui::Context::default();
        let mut output = None;
        for _ in 0..2 {
            output = Some(ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 1_200.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        render_cabin_editor(
                            ui,
                            &cabin.fields,
                            values.pointer_mut("/cabin").expect("cabin values"),
                            &Default::default(),
                            Some("en"),
                            false,
                            false,
                        );
                    });
                },
            ));
        }
        let mut labels = Vec::new();
        for clipped in &output.expect("rendered cabin").shapes {
            collect(&clipped.shape, &mut labels);
        }
        labels
    }

    fn position(labels: &[(String, egui::Pos2)], name: &str) -> egui::Pos2 {
        labels
            .iter()
            .find(|(text, _)| text == name || *text == crate::views::label_case(name))
            .map(|(_, position)| *position)
            .unwrap_or_else(|| panic!("{name} was not painted: {labels:?}"))
    }

    #[test]
    fn passenger_classes_form_one_complete_row_or_stack_at_narrow_width() {
        let wide = rendered_positions(2_000.0);
        let [first, business, economy] =
            ["First", "Business", "Economy"].map(|name| position(&wide, name));
        assert!((first.y - business.y).abs() < 5.0);
        assert!((first.y - economy.y).abs() < 5.0);
        assert!(first.x + 200.0 < business.x && business.x + 200.0 < economy.x);

        let narrow = rendered_positions(600.0);
        let [first, business, economy] =
            ["First", "Business", "Economy"].map(|name| position(&narrow, name));
        assert!((first.x - business.x).abs() < 5.0);
        assert!((first.x - economy.x).abs() < 5.0);
        assert!(first.y + 40.0 < business.y && business.y + 40.0 < economy.y);
    }

    #[test]
    fn cargo_inputs_align_with_their_related_controls() {
        let labels = rendered_positions(2_000.0);
        let schema = CargoDeckConfig::default().schema();
        // The label as the English form shows it (acronyms restored).
        let field = |name| {
            crate::views::form_label(schema.field(name).expect("cargo field").label, Some("en"))
        };
        let [main_uld, lower_uld] =
            ["main_deck_uld", "lower_deck_uld"].map(|name| position(&labels, &field(name)));
        assert!((main_uld.y - lower_uld.y).abs() < 5.0);
        assert!(main_uld.x + 200.0 < lower_uld.x);

        let [main_door, forward_door, aft_door] = ["main_door_x_m", "fwd_door_x_m", "aft_door_x_m"]
            .map(|name| position(&labels, &field(name)));
        assert!((main_door.y - forward_door.y).abs() < 5.0);
        assert!((main_door.y - aft_door.y).abs() < 5.0);
        assert!(main_door.x + 200.0 < forward_door.x);
        assert!(forward_door.x + 200.0 < aft_door.x);
        assert!(main_door.x > main_uld.x + 600.0);

        let narrow = rendered_positions(600.0);
        let narrow_main_uld = position(&narrow, &field("main_deck_uld"));
        let narrow_main_door = position(&narrow, &field("main_door_x_m"));
        assert!((narrow_main_uld.x - narrow_main_door.x).abs() < 5.0);
        assert!(narrow_main_uld.y < narrow_main_door.y);
    }
}
