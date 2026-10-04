// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compact, schema-backed placement for the Cabin & Cargo page.

use std::collections::HashSet;

use alas_config::{Entry, Field};
use egui::{RichText, Ui};
use serde_json::Value;

use crate::views::form::{dynamic_form, FormEdit};
use crate::views::tr;

const CLASS_NAMES: [&str; 3] = ["first", "business", "economy"];
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
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let section = egui::CollapsingHeader::new(
                RichText::new(alas_i18n::t(Some(field.label), lang)).strong(),
            )
            .id_salt(format!("cabin::{}", field.name))
            .default_open(true)
            .show(ui, |ui| match field.name {
                "passenger" => render_passenger(
                    ui,
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
                    &node.fields,
                    child,
                    error_fields,
                    lang,
                    show_help,
                    &mut edits,
                ),
                _ => edits.extend(dynamic_form(
                    ui,
                    &node.fields,
                    child,
                    error_fields,
                    lang,
                    show_help,
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
                    class,
                    values,
                    error_fields,
                    lang,
                    show_help,
                    custom_preset,
                    edits,
                );
            }
        });
    } else {
        for class in classes {
            render_class(
                ui,
                class,
                values,
                error_fields,
                lang,
                show_help,
                custom_preset,
                edits,
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
        edits.extend(dynamic_form(
            ui,
            &remaining,
            values,
            error_fields,
            lang,
            show_help,
        ));
    }
}

// Each argument is an independent editor input; a wrapper struct would only rename them.
#[allow(clippy::too_many_arguments)]
fn render_class(
    ui: &mut Ui,
    field: &Field,
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    custom_preset: bool,
    edits: &mut Vec<FormEdit>,
) {
    let Entry::Node(node) = &field.entry else {
        return;
    };
    let Some(class_values) = child_values(values, field.name) else {
        return;
    };
    let mut fields = node.fields.clone();
    if custom_preset {
        // The condition names a root sibling (requirements.cabin_preset).
        // The class editor receives only the nested class JSON value, so the
        // page resolves this condition before handing the field to the form.
        if let Some(share) = fields.iter_mut().find(|field| field.name == "share_pct") {
            if let Entry::Leaf(leaf) = &mut share.entry {
                leaf.readonly_unless = None;
            }
        }
    }
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(alas_i18n::t(Some(field.label), lang)).strong())
            .on_hover_text(alas_i18n::t(Some(field.help), lang));
        edits.extend(dynamic_form(
            ui,
            &fields,
            class_values,
            error_fields,
            lang,
            show_help,
        ));
    });
}

// Each argument is an independent editor input; a wrapper struct would only rename them.
#[allow(clippy::too_many_arguments)]
fn render_cargo(
    ui: &mut Ui,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    edits: &mut Vec<FormEdit>,
) {
    if ui.available_width() >= 1_100.0 {
        // Two similarly sized blocks use the wide page without stretching
        // a single drop-down across the entire window.
        ui.columns(2, |columns| {
            for index in [0, 1] {
                render_cargo_group(
                    &mut columns[0],
                    index,
                    fields,
                    values,
                    error_fields,
                    lang,
                    show_help,
                    edits,
                );
            }
            for index in [2, 3] {
                render_cargo_group(
                    &mut columns[1],
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
    } else {
        for index in 0..CARGO_GROUPS.len() {
            render_cargo_group(
                ui,
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
        edits.extend(dynamic_form(
            ui,
            &remaining,
            values,
            error_fields,
            lang,
            show_help,
        ));
    }
}

// Each argument is an independent editor input; a wrapper struct would only rename them.
#[allow(clippy::too_many_arguments)]
fn render_cargo_group(
    ui: &mut Ui,
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
    ui.label(RichText::new(tr(title)).strong());
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
        edits.extend(dynamic_form(
            ui,
            &toggle,
            values,
            error_fields,
            lang,
            show_help,
        ));
        edits.extend(dynamic_form(
            ui,
            &selectors,
            values,
            error_fields,
            lang,
            show_help,
        ));
    } else {
        edits.extend(dynamic_form(
            ui,
            &group,
            values,
            error_fields,
            lang,
            show_help,
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
            .find(|(text, _)| text == name)
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
        let field = |name| schema.field(name).expect("cargo field").label;
        let [main_uld, lower_uld] =
            ["main_deck_uld", "lower_deck_uld"].map(|name| position(&labels, field(name)));
        assert!((main_uld.y - lower_uld.y).abs() < 5.0);
        assert!(main_uld.x + 200.0 < lower_uld.x);

        let [main_door, forward_door, aft_door] = ["main_door_x_m", "fwd_door_x_m", "aft_door_x_m"]
            .map(|name| position(&labels, field(name)));
        assert!((main_door.y - forward_door.y).abs() < 5.0);
        assert!((main_door.y - aft_door.y).abs() < 5.0);
        assert!(main_door.x + 200.0 < forward_door.x);
        assert!(forward_door.x + 200.0 < aft_door.x);
        assert!(main_door.x > main_uld.x + 600.0);

        let narrow = rendered_positions(600.0);
        let narrow_main_uld = position(&narrow, field("main_deck_uld"));
        let narrow_main_door = position(&narrow, field("main_door_x_m"));
        assert!((narrow_main_uld.x - narrow_main_door.x).abs() < 5.0);
        assert!(narrow_main_uld.y < narrow_main_door.y);
    }
}
