// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::{
    display_option, form_column_count, label_text_size, readonly_unless, resolved_options,
};
use alas_config::{AlasConfig, ConfigNode, Entry};
use serde_json::{json, Value};

fn visit_fields(fields: &[alas_config::Field], values: &Value, names: &mut Vec<String>) {
    for field in fields {
        match &field.entry {
            Entry::Node(node) => {
                if let Some(child) = values.get(field.name) {
                    visit_fields(&node.fields, child, names);
                }
            }
            Entry::Leaf(leaf) => {
                if leaf.options.is_some() || field.name == "class_mix_mode" {
                    let options = resolved_options(field, values).expect("option source resolves");
                    assert!(!options.is_empty(), "{} has no options", field.name);
                    names.push(field.name.to_owned());
                }
            }
        }
    }
}

#[test]
fn every_declared_option_source_resolves_to_a_nonempty_form_list() {
    let config = AlasConfig::default();
    let values = serde_json::to_value(&config).expect("default config serializes");
    let mut names = Vec::new();
    visit_fields(&config.schema().fields, &values, &mut names);

    for expected in [
        "tail_airfoil",
        "tire_class",
        "strut_material",
        "skin_material",
        "strategy",
        "aircraft_type",
        "cabin_preset",
        "class_mix_mode",
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "missing {expected}"
        );
    }
}

#[test]
fn tire_and_class_mix_lists_include_the_declared_values() {
    let config = AlasConfig::default();
    let values = serde_json::to_value(&config).expect("default config serializes");
    let schema = config.schema();
    let gear = schema.field("landing_gear").expect("gear group");
    let Entry::Node(gear) = &gear.entry else {
        panic!("gear is a group");
    };
    let gear_values = values.get("landing_gear").expect("gear values");
    let tire = gear.field("tire_class").expect("tire field");
    assert_eq!(
        resolved_options(tire, gear_values).expect("tire options"),
        vec!["auto", "light", "narrowbody", "widebody", "heavy"]
    );

    let cabin = schema.field("requirements").expect("requirements group");
    let Entry::Node(requirements) = &cabin.entry else {
        panic!("requirements is a group");
    };
    let requirement_values = values.get("requirements").expect("requirement values");
    let preset = requirements.field("cabin_preset").expect("cabin preset");
    assert_eq!(
        resolved_options(preset, requirement_values).expect("passenger presets"),
        vec!["Ryanair", "Iberia", "Emirates", "Custom"]
    );
    assert_eq!(
        resolved_options(preset, &json!({"aircraft_type": "cargo"})).expect("cargo presets"),
        vec!["Max payload", "Dense payload", "Custom"]
    );

    let class_mix = alas_config::PassengerCabinConfig::default().schema();
    let class_mix = class_mix.field("class_mix_mode").expect("class mix mode");
    assert_eq!(
        resolved_options(class_mix, &Value::Null).expect("class mix options"),
        vec!["percent", "count"]
    );

    let engine = alas_config::EngineConfig::default().schema();
    let engine = engine.field("engine_name").expect("engine field");
    assert!(!resolved_options(engine, &Value::Null)
        .expect("engine options")
        .is_empty());
}

#[test]
fn nested_readonly_conditions_can_use_the_parent_class_mix_mode() {
    let condition = alas_config::ReadonlyUnless {
        field: "class_mix_mode",
        value: "count",
    };
    let parent = json!({"class_mix_mode": "count"});
    assert!(!readonly_unless(
        &json!({"count": 42}),
        &[parent],
        condition
    ));
    assert!(readonly_unless(
        &json!({"count": 42}),
        &[json!({"class_mix_mode": "percent"})],
        condition
    ));
}

#[test]
fn form_columns_and_label_text_stay_within_readability_bounds() {
    assert_eq!(form_column_count(299.0), 1);
    assert_eq!(form_column_count(600.0), 2);
    assert_eq!(form_column_count(1_200.0), 3);
    assert_eq!(label_text_size(200.0), 11.0);
    assert_eq!(label_text_size(900.0), 15.0);
    assert!(label_text_size(500.0) > 11.0);
    assert!(label_text_size(500.0) < 15.0);
}

#[test]
fn fixed_option_values_are_capitalized_without_changing_their_data_value() {
    assert_eq!(display_option("passenger"), "Passenger");
    assert_eq!(display_option("auto"), "Auto");
}
