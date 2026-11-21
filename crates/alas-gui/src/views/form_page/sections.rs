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
use egui::Ui;
use serde_json::Value;

use crate::views::form::{dynamic_form_at_help, dynamic_form_with_open_root_nodes, FormEdit};
use crate::views::{tr, tr_label};

pub(super) use table::page_sections;

use crate::detail::Detail;

/// The configuration group of the Aerodynamics page, whose form cards are
/// Advanced only.
pub(crate) const DRAG_MODEL_GROUP: &str = "drag_model";

/// Nested nodes of the Mass page that are drawn as one flat card each.
const FLATTENED_NODES: &[&str] = &["flops_transport", "flops_structure", "flops_turboprop"];

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
    // The drag model's parasite build-up and wave-drag cards are Advanced
    // only; Basic keeps the airfoil pickers the page adds below the form.
    if group == DRAG_MODEL_GROUP && crate::detail::published(ui.ctx()) == Some(Detail::Basic) {
        return edits;
    }
    let identity = (fields.as_ptr() as usize, fields.len());
    let mut prepared = Vec::new();
    for section in sections {
        let section_fields =
            super::placement::cached_section_fields(group, section.title, identity, || {
                fields
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
                    .collect()
            });
        if section_fields.is_empty() {
            continue;
        }
        // The shaft-power inputs mean nothing for a jet engine: the card is
        // drawn only while the installed engine is a turboprop.
        if section_fields.iter().any(|f| f.name == "flops_turboprop")
            && crate::views::form::published_value("geometry.engine.propulsion_technology")
                .is_some_and(|technology| technology != "turboprop")
        {
            continue;
        }
        // A section that is one nested node (the FLOPS inputs) shows the
        // node's own children under the card header: no second header for the
        // node and no inner "Advanced (n)" accordion.
        // Any section that is one node is flattened the same way: the card
        // header is the heading, so the node never adds a second, same-named
        // level (Optimizer > Objective and requirements > Objective).
        let flatten = section_fields.len() == 1
            && (group != "mass_model" || FLATTENED_NODES.contains(&section_fields[0].name))
            && matches!(section_fields[0].entry, Entry::Node(_));
        prepared.push((section, section_fields, flatten));
    }
    let remaining =
        super::placement::cached_section_fields(group, "dditional settings", identity, || {
            fields
                .iter()
                .filter(|field| {
                    !sections
                        .iter()
                        .any(|section| section.names.contains(&field.name))
                })
                .cloned()
                .collect()
        });
    // Short cards share a row; a lone short card spreads over its own width.
    let nested = |list: &[Field]| list.iter().any(|f| matches!(f.entry, Entry::Node(_)));
    let mut cols: Vec<usize> = prepared
        .iter()
        .map(|(_, list, flatten)| {
            if *flatten || !nested(list) {
                crate::views::form::leaf_columns(list, values)
            } else {
                0
            }
        })
        .collect();
    let extra = !remaining.is_empty();
    if extra {
        cols.push(if nested(&remaining) {
            0
        } else {
            crate::views::form::leaf_columns(&remaining, values)
        });
    }
    crate::views::form::flow_cards(ui, ("sections", group), &cols, |ui, k| {
        let Some((section, section_fields, flatten_turboprop)) = prepared.get(k) else {
            if extra {
                additional_card(
                    ui,
                    group,
                    &remaining,
                    values,
                    error_fields,
                    lang,
                    show_help,
                    &mut edits,
                );
            }
            return;
        };
        let flatten_turboprop = *flatten_turboprop;
        let node_name = section_fields[0].name;
        // A search jump to a field of this card opens it.
        let forced = section
            .names
            .iter()
            .any(|name| crate::search::focus_wants(ui.ctx(), &format!("{group}.{name}")));
        let card = crate::theme::card_frame(ui).show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let mut header =
                egui::CollapsingHeader::new(crate::theme::card_title(ui, tr_label(section.title)))
                    .id_salt(format!("{group}::{}", section.title))
                    .default_open(section.default_open)
                    .open(crate::perf_tour::force_open());
            if forced {
                header = header.open(Some(true));
            }
            let header = crate::theme::show_header(ui, header, |ui| {
                if flatten_turboprop {
                    // This node already has a dedicated Advanced Settings
                    // card. Edit its children at the existing JSON path,
                    // without a second "FLOPS turboprop" card and the
                    // schema's inner "Advanced (17)" accordion.
                    let Entry::Node(node) = &section_fields[0].entry else {
                        return;
                    };
                    let mut child_fields = node.fields.clone();
                    // Advanced detail shows every child in the card itself; Basic
                    // keeps the "more settings in Advanced view" hint.
                    if crate::detail::published(ui.ctx()) != Some(Detail::Basic) {
                        for field in &mut child_fields {
                            field.advanced = false;
                        }
                    }
                    if !values.is_object() {
                        *values = Value::Object(serde_json::Map::new());
                    }
                    // The guard above made `values` an object, so this
                    // else branch is unreachable; skip the card if not.
                    let Some(values_map) = values.as_object_mut() else {
                        return;
                    };
                    let child_values = values_map
                        .entry(node_name)
                        .or_insert_with(|| Value::Object(serde_json::Map::new()));
                    // The rows keep the node's path, so a search jump
                    // to one of them finds it below `::<node>`.
                    edits.extend(dynamic_form_at_help(
                        ui,
                        &child_fields,
                        child_values,
                        error_fields,
                        lang,
                        show_help,
                        &format!("::{node_name}"),
                    ));
                } else {
                    edits.extend(dynamic_form_with_open_root_nodes(
                        ui,
                        section_fields,
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
            crate::views::form::equalise(ui);
        });
        if flatten_turboprop {
            // The card is the node's own header: it takes the search pulse.
            let path = format!("{group}.{}", section_fields[0].name);
            crate::search::focus_anchor(ui, &path, card.response.rect);
        }
        ui.add_space(6.0);
    });
    edits
}

/// The "Additional settings" card: the fields no section claimed.
#[allow(clippy::too_many_arguments)]
fn additional_card(
    ui: &mut Ui,
    group: &str,
    remaining: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    edits: &mut Vec<FormEdit>,
) {
    let forced = remaining
        .iter()
        .any(|field| crate::search::focus_wants(ui.ctx(), &format!("{group}.{}", field.name)));
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let mut header =
            egui::CollapsingHeader::new(crate::theme::card_title(ui, tr("Additional settings")))
                .id_salt(format!("{group}::additional"))
                .default_open(true)
                .open(crate::perf_tour::force_open());
        if forced {
            header = header.open(Some(true));
        }
        crate::theme::show_header(ui, header, |ui| {
            edits.extend(dynamic_form_with_open_root_nodes(
                ui,
                remaining,
                values,
                error_fields,
                lang,
                show_help,
                true,
            ));
        });
        crate::views::form::equalise(ui);
    });
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
                .filter(|label| **label == crate::views::form_label("FLOPS Turboprop", Some("en")))
                .count(),
            1
        );
        let engine_dry_mass = crate::views::form_label("Engine Dry Mass", Some("en"));
        assert!(labels.contains(&engine_dry_mass));
        assert!(!labels.iter().any(|label| label.starts_with("Advanced (")));
        assert!(values
            .pointer("/flops_turboprop/engine_dry_mass_kg")
            .is_some());
    }
}
