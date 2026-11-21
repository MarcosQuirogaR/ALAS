// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The global Basic / Advanced level over schema-driven forms.

use alas_config::{AlasConfig, ConfigNode, Entry, Field};
use egui::epaint::Shape;
use serde_json::json;

use crate::detail::{publish, Detail};

fn collect(shape: &Shape, out: &mut Vec<String>) {
    match shape {
        Shape::Text(text) => out.push(text.galley.text().to_owned()),
        Shape::Vec(shapes) => shapes.iter().for_each(|shape| collect(shape, out)),
        _ => {}
    }
}

/// A node of the default schema with at least one advanced and one routine leaf.
fn mixed_fields() -> Vec<Field> {
    let schema = AlasConfig::default().schema();
    for field in &schema.fields {
        if let Entry::Node(node) = &field.entry {
            let leaf = |advanced: bool| {
                node.fields
                    .iter()
                    .any(|f| f.advanced == advanced && matches!(f.entry, Entry::Leaf(_)))
            };
            if leaf(true) && leaf(false) {
                return node.fields.clone();
            }
        }
    }
    panic!("no schema group mixes routine and advanced leaves");
}

fn rendered_texts(detail: Option<Detail>) -> Vec<String> {
    let fields = mixed_fields();
    let mut values = json!({});
    let ctx = egui::Context::default();
    let mut texts = Vec::new();
    for _ in 0..3 {
        if let Some(detail) = detail {
            publish(&ctx, detail);
        }
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 2000.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    super::dynamic_form(
                        ui,
                        &fields,
                        &mut values,
                        &Default::default(),
                        Some("en"),
                        false,
                    );
                });
            },
        );
        texts.clear();
        for clipped in &output.shapes {
            collect(&clipped.shape, &mut texts);
        }
    }
    texts
}

fn has_advanced_header(texts: &[String]) -> bool {
    texts.iter().any(|text| text.starts_with("Advanced ("))
}

#[test]
fn without_a_published_level_the_advanced_group_stays_collapsed() {
    assert!(has_advanced_header(&rendered_texts(None)));
}

#[test]
fn basic_hides_the_advanced_rows_and_advanced_shows_them_open() {
    let basic = rendered_texts(Some(Detail::Basic));
    assert!(!has_advanced_header(&basic), "{basic:?}");
    let advanced = rendered_texts(Some(Detail::Advanced));
    assert!(has_advanced_header(&advanced), "{advanced:?}");
    // Open, the group paints its rows as well: more text than Basic.
    assert!(advanced.len() > basic.len());
}
