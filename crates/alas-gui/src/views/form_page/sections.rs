// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Shared hierarchy for schema-driven advanced-settings pages.
//!
//! The configuration schema owns values, bounds, labels, and help. This map
//! only groups related controls into scan-friendly cards so dense pages do not
//! present an undifferentiated wall of inputs.

mod table;

use std::collections::HashSet;

use alas_config::{Entry, Field};
use egui::{RichText, Ui};
use serde_json::Value;

use crate::views::form::{dynamic_form_with_open_root_nodes, FormEdit};
use crate::views::tr;

pub(super) use table::page_sections;

/// A visual subsection of a configuration page.
pub(super) struct PageSection {
    title: &'static str,
    names: &'static [&'static str],
    default_open: bool,
}

/// Render an explicitly grouped form while leaving the schema authoritative.
#[allow(clippy::too_many_arguments)]
pub(super) fn render_sectioned_form(
    ui: &mut Ui,
    group: &str,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    sections: &[PageSection],
) -> Vec<FormEdit> {
    let mut edits = Vec::new();
    for section in sections {
        let section_fields: Vec<Field> = fields
            .iter()
            .filter(|field| section.names.contains(&field.name))
            .cloned()
            .map(|mut field| {
                // The outer section is the page's explicit hierarchy. Do not
                // put an "Advanced" accordion immediately inside it, which
                // would split one concept across two unrelated menus.
                field.advanced = false;
                field
            })
            .collect();
        if section_fields.is_empty() {
            continue;
        }
        let flatten_turboprop = group == "mass_model"
            && section_fields.len() == 1
            && section_fields[0].name == "flops_turboprop";
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let header = egui::CollapsingHeader::new(RichText::new(tr(section.title)).strong())
                .id_salt(format!("{group}::{}", section.title))
                .default_open(section.default_open)
                .show(ui, |ui| {
                    if flatten_turboprop {
                        // This node already has a dedicated Advanced Settings
                        // card. Edit its children at the existing JSON path,
                        // without a second "Flops turboprop" card and the
                        // schema's inner "Advanced (17)" accordion.
                        let Entry::Node(node) = &section_fields[0].entry else {
                            return;
                        };
                        let mut child_fields = node.fields.clone();
                        for field in &mut child_fields {
                            field.advanced = false;
                        }
                        if !values.is_object() {
                            *values = Value::Object(serde_json::Map::new());
                        }
                        let child_values = values
                            .as_object_mut()
                            .expect("mass model object")
                            .entry("flops_turboprop")
                            .or_insert_with(|| Value::Object(serde_json::Map::new()));
                        edits.extend(dynamic_form_with_open_root_nodes(
                            ui,
                            &child_fields,
                            child_values,
                            error_fields,
                            lang,
                            show_help,
                            true,
                        ));
                    } else {
                        edits.extend(dynamic_form_with_open_root_nodes(
                            ui,
                            &section_fields,
                            values,
                            error_fields,
                            lang,
                            show_help,
                            true,
                        ));
                    }
                });
            if flatten_turboprop {
                header
                    .header_response
                    .on_hover_text(alas_i18n::t(Some(section_fields[0].help), lang).into_owned());
            }
        });
        ui.add_space(6.0);
    }

    let remaining: Vec<Field> = fields
        .iter()
        .filter(|field| {
            !sections
                .iter()
                .any(|section| section.names.contains(&field.name))
        })
        .cloned()
        .collect();
    if !remaining.is_empty() {
        crate::theme::card_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            egui::CollapsingHeader::new(RichText::new(tr("Additional settings")).strong())
                .id_salt(format!("{group}::additional"))
                .default_open(true)
                .show(ui, |ui| {
                    edits.extend(dynamic_form_with_open_root_nodes(
                        ui,
                        &remaining,
                        values,
                        error_fields,
                        lang,
                        show_help,
                        true,
                    ));
                });
        });
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::{page_sections, render_sectioned_form};
    use crate::nav::{page, Surface};
    use alas_config::{AlasConfig, ConfigNode};

    #[test]
    fn flops_turboprop_has_one_open_card_on_mass_advanced() {
        fn painted_text(shape: &egui::Shape, labels: &mut Vec<String>) {
            match shape {
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        painted_text(shape, labels);
                    }
                }
                egui::Shape::Text(text) => labels.push(text.galley.text().to_owned()),
                _ => {}
            }
        }

        let schema = AlasConfig::default().schema();
        let fields = super::super::placement::node_fields_at(&schema, "/mass_model")
            .expect("mass model schema");
        let fields = super::super::placement::visible_fields(
            page("mass_advanced").expect("Mass Advanced page"),
            "mass_model",
            &fields,
        )
        .into_iter()
        .filter(|field| field.name == "flops_turboprop")
        .collect::<Vec<_>>();
        assert_eq!(fields.len(), 1);
        let section = page_sections("mass_model", Surface::Advanced)
            .expect("Mass Advanced sections")
            .iter()
            .find(|section| section.names == ["flops_turboprop"])
            .expect("turboprop section");
        let mut values = serde_json::json!({});
        let ctx = egui::Context::default();
        let mut output = None;
        for _ in 0..2 {
            output = Some(ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1500.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        render_sectioned_form(
                            ui,
                            "mass_model",
                            &fields,
                            &mut values,
                            &Default::default(),
                            Some("en"),
                            false,
                            std::slice::from_ref(section),
                        );
                    });
                },
            ));
        }
        let mut labels = Vec::new();
        for clipped in &output.expect("rendered Mass Advanced card").shapes {
            painted_text(&clipped.shape, &mut labels);
        }
        assert_eq!(
            labels
                .iter()
                .filter(|label| *label == "Flops turboprop")
                .count(),
            1
        );
        assert!(labels.iter().any(|label| label == "Engine dry mass"));
        assert!(!labels.iter().any(|label| label.starts_with("Advanced (")));
        assert!(values
            .pointer("/flops_turboprop/engine_dry_mass_kg")
            .is_some());
    }
}
