// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A failed unwrap in this test target is the assertion reporting malformed
// evidence, not a panic escaping from library code.
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Schema contract: the inventory table, the typed rows and the committed
//! Markdown reference must describe the same members.

use std::collections::BTreeSet;

use alas_uav_db::inventory::{fields, render_markdown};
use alas_uav_db::units::quantity_of;
use alas_uav_db::{Body, Category};

fn empty_body(category: Category) -> Body {
    serde_json::from_value(serde_json::json!({"category": category.key(), "spec": {}})).unwrap()
}

#[test]
fn inventory_names_equal_struct_members_for_every_category() {
    for category in Category::ALL {
        let value = serde_json::to_value(empty_body(category)).unwrap();
        let mut members: BTreeSet<String> =
            value["spec"].as_object().unwrap().keys().cloned().collect();
        if category == Category::Kit {
            members.remove("parts");
        }
        let inventory: BTreeSet<String> =
            fields(category).iter().map(|f| f.name.to_owned()).collect();
        assert_eq!(members, inventory, "{}", category.key());
    }
}

#[test]
fn inventory_units_are_known_and_stems_are_unique() {
    for category in Category::ALL {
        let mut stems = BTreeSet::new();
        for info in fields(category) {
            assert!(
                quantity_of(info.unit_si).is_some(),
                "{} {}",
                category.key(),
                info.name
            );
            assert!(
                stems.insert(info.stem),
                "duplicate stem {} in {}",
                info.stem,
                category.key()
            );
            assert!(info.min <= info.max);
        }
    }
}

#[test]
fn committed_schema_reference_matches_the_inventory() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/uav-db-schema.md");
    let begin = "<!-- BEGIN GENERATED INVENTORY -->";
    let end = "<!-- END GENERATED INVENTORY -->";
    let generated = format!("{begin}\n{}{end}", render_markdown());
    if std::env::var_os("UPDATE_SCHEMA_DOC").is_some() {
        let text = std::fs::read_to_string(path).unwrap();
        let (head, rest) = text.split_once(begin).unwrap();
        let (_, tail) = rest.split_once(end).unwrap();
        std::fs::write(path, format!("{head}{generated}{tail}")).unwrap();
    }
    let text = std::fs::read_to_string(path).unwrap().replace("\r\n", "\n");
    assert!(
        text.contains(&generated),
        "docs/uav-db-schema.md is stale; rerun with UPDATE_SCHEMA_DOC=1"
    );
}
