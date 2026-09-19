// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::form_options::{optional_hint, parse_number_or_sentinel, sentinel_word};
use super::{
    display_option, display_unit, form_column_count, format_number, label_text_size,
    readonly_unless, resolved_options,
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
                if leaf.options.is_some() {
                    let options = resolved_options(field, values).expect("option source resolves");
                    assert!(!options.is_empty(), "{} has no options", field.name);
                    names.push(field.name.to_owned());
                }
            }
        }
    }
}

fn leaf_named<'a>(fields: &'a [alas_config::Field], name: &str) -> Option<&'a alas_config::Field> {
    for field in fields {
        match &field.entry {
            Entry::Node(node) => {
                if let Some(found) = leaf_named(&node.fields, name) {
                    return Some(found);
                }
            }
            Entry::Leaf(_) if field.name == name => return Some(field),
            Entry::Leaf(_) => {}
        }
    }
    None
}

#[test]
fn the_reset_affordance_is_a_renderable_glyph_and_not_an_ascii_arrow() {
    assert_ne!(super::RESET_GLYPH, "<-");
    let ctx = egui::Context::default();
    let _ = ctx.run(egui::RawInput::default(), |_| {});
    for style in [egui::TextStyle::Button, egui::TextStyle::Body] {
        let font = style.resolve(&ctx.style());
        assert!(
            ctx.fonts(|fonts| fonts.has_glyphs(&font, super::RESET_GLYPH)),
            "the bundled fonts must cover the reset glyph at {style:?}"
        );
    }
}

#[test]
fn a_declared_sentinel_reads_as_its_word_and_can_be_typed_back() {
    let config = AlasConfig::default();
    let schema = config.schema();
    for (name, word) in [
        ("n_nlg_wheels", "Auto"),
        ("n_mlg_struts", "Auto"),
        ("wheels_per_mlg_strut", "Auto"),
        ("design_range_nmi", "From route"),
        ("max_approach_speed_kt", "No limit"),
    ] {
        let field = leaf_named(&schema.fields, name)
            .unwrap_or_else(|| panic!("{name} is not in the schema"));
        assert_eq!(
            sentinel_word(field, 0.0),
            Some(word),
            "{name} declares a zero sentinel"
        );
        // A real measurement is still a number.
        assert_eq!(sentinel_word(field, 2.0), None);
        assert_eq!(parse_number_or_sentinel(word, field), Some(0.0));
        assert_eq!(parse_number_or_sentinel(" 4 ", field), Some(4.0));
    }
    let xtr = leaf_named(&schema.fields, "xtr_upper").expect("MSES transition field");
    assert_eq!(sentinel_word(xtr, 1.0), Some("Free"));
    assert_eq!(sentinel_word(xtr, 0.4), None);
    assert_eq!(parse_number_or_sentinel("Free", xtr), Some(1.0));
}

#[test]
fn a_value_the_schema_calls_real_is_never_dressed_up_as_a_sentinel() {
    let config = AlasConfig::default();
    let schema = config.schema();
    // "Enter zero explicitly when the aircraft carries none" and the FLOPS
    // mass margin describe genuine zeros, not sentinels.
    for name in ["galley_crew", "mass_margin_fraction"] {
        if let Some(field) = leaf_named(&schema.fields, name) {
            assert_eq!(sentinel_word(field, 0.0), None, "{name} holds a real zero");
        }
    }
}

#[test]
fn an_empty_optional_states_what_leaving_it_empty_means() {
    let config = AlasConfig::default();
    let schema = config.schema();
    for (name, hint) in [
        ("seed", "Random"),
        ("num_ribs_override", "Auto"),
        ("reference_wheelbase_m", "Not set"),
    ] {
        let field = leaf_named(&schema.fields, name)
            .unwrap_or_else(|| panic!("{name} is not in the schema"));
        assert_eq!(optional_hint(field), hint, "{name} placeholder");
    }
    // An optional length states its unit; the editor renders it beside the box.
    let wheelbase = leaf_named(&schema.fields, "reference_wheelbase_m").expect("wheelbase");
    assert_eq!(display_unit(wheelbase.unit), "m");
}

#[test]
fn one_dimensionless_convention_replaces_the_four_that_reached_the_screen() {
    // A hyphen, the word "fraction", a phrase and nothing at all were all in
    // use at once for the same kind of quantity.
    for unit in [
        "-",
        "0-1",
        "fraction",
        "x/c",
        "chord fraction",
        "root chord ratio",
        "",
    ] {
        assert_eq!(display_unit(unit), "", "{unit} should render bare");
    }
    // A fraction of a named reference keeps the reference, in one spelling.
    assert_eq!(display_unit("fraction of semi-span"), "of semi-span");
    assert_eq!(display_unit("0-1 of semispan"), "of semi-span");
    assert_eq!(display_unit("fraction of critical"), "of critical");
    assert_eq!(display_unit("fraction of MAC"), "of MAC");
    assert_eq!(
        display_unit("fraction of fuselage length"),
        "of fuselage length"
    );
}

#[test]
fn physical_units_keep_their_value_and_gain_conventional_typography() {
    for unit in ["m", "kg", "deg", "s", "Pa", "nmi", "kt", "K", "% MAC"] {
        assert_eq!(display_unit(unit), unit, "{unit} must not be rewritten");
    }
    assert_eq!(display_unit("m^2"), "m\u{b2}");
    assert_eq!(display_unit("m2"), "m\u{b2}");
    assert_eq!(display_unit("kg/m^3"), "kg/m\u{b3}");
    assert_eq!(display_unit("N^2/Hz"), "N\u{b2}/Hz");
    assert_eq!(display_unit("g^2/Hz"), "g\u{b2}/Hz");
    assert_eq!(display_unit("J/(kg.K)"), "J/(kg\u{b7}K)");
    // Mass-specific fuel consumption against kilogram-force: the separator
    // becomes a middle dot and the hour keeps its SI symbol. The number is
    // unchanged; only the symbol is typeset.
    assert_eq!(display_unit("kg/(kgf.hr)"), "kg/(kgf\u{b7}h)");
}

#[test]
fn a_rendered_number_does_not_depend_on_the_display_scale() {
    // egui derives minimum decimals from drag speed and the pointer aim
    // radius, which is scaled by points per pixel: whole-number counts showed
    // as "16.0"/"0.0" and one quantity rendered "-4.00" beside "10.0".
    assert_eq!(format_number(16.0, 0), "16");
    assert_eq!(format_number(0.0, 0), "0");
    assert_eq!(format_number(30.0, 2), "30");
    assert_eq!(format_number(-4.0, 3), "-4");
    assert_eq!(format_number(10.0, 2), "10");
    // Precision is never lost: the shortest exact rendering wins.
    assert_eq!(format_number(0.84, 3), "0.84");
    assert_eq!(format_number(0.027, 3), "0.027");
    assert_eq!(format_number(1.45, 3), "1.45");
    assert_eq!(format_number(358_670.0, 0), "358670");
    // Beyond the field's declared places the value is shown rounded, not cut.
    assert_eq!(format_number(0.123_456, 3), "0.123");
    assert!(format_number(f64::NAN, 3).contains("NaN"));
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
    ] {
        assert!(
            names.iter().any(|name| name == expected),
            "missing {expected}"
        );
    }
}

#[test]
fn tire_and_cabin_preset_lists_include_the_declared_values() {
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

    let engine = alas_config::EngineConfig::default().schema();
    let engine = engine.field("engine_name").expect("engine field");
    assert!(!resolved_options(engine, &Value::Null)
        .expect("engine options")
        .is_empty());
}

#[test]
fn nested_readonly_conditions_can_use_a_dotted_path_from_an_ancestor() {
    let class = alas_config::SeatClassConfig::default().schema();
    let Entry::Leaf(share) = &class.field("share_pct").expect("class share").entry else {
        panic!("class share is a leaf");
    };
    let condition = share.readonly_unless.expect("custom preset condition");
    let root = json!({"requirements": {"cabin_preset": "Custom"}});
    assert!(!readonly_unless(
        &json!({"share_pct": 20.0}),
        &[root],
        condition
    ));
    assert!(readonly_unless(
        &json!({"share_pct": 20.0}),
        &[json!({"requirements": {"cabin_preset": "Iberia"}})],
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

#[test]
fn cabin_presets_display_a_descriptive_airline_independent_name() {
    // The combo box shows a descriptive name, but `resolved_options` above
    // still returns the serialized identifier a saved config stores and
    // `apply_cabin_preset` matches on, only the label changes.
    assert_eq!(display_option("Ryanair"), "High-density single-class");
    assert_eq!(display_option("Iberia"), "Two-class (Business/Economy)");
    assert_eq!(
        display_option("Emirates"),
        "Three-class (First/Business/Economy)"
    );
    // Unaffected cabin/cargo identifiers still pass through unchanged.
    assert_eq!(display_option("Custom"), "Custom");
    assert_eq!(display_option("Max payload"), "Max payload");
    assert_eq!(display_option("Dense payload"), "Dense payload");
}
