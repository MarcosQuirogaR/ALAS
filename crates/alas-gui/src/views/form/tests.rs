// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::display_unit;
use super::editors::optional_value_from_text;
use super::layout::{form_column_count, label_text_size};
use super::options::{display_option, format_number, readonly_unless, resolved_options};
use super::options::{optional_hint, parse_number_or_sentinel, sentinel_word};
use alas_config::{AlasConfig, ConfigNode, Entry, Field, Kind, LeafField, Node, OptionalValueKind};
use serde_json::{json, Value};

#[test]
fn objective_options_reuse_result_labels_and_worker_zero_round_trips() {
    let objective_schema = alas_config::ObjectiveConfig::default().schema();
    let objective = objective_schema.field("kind").unwrap();
    let plain = plain_option_field();
    for token in [
        "block_fuel",
        "takeoff_mass",
        "operating_empty_mass",
        "fuel_per_seat_kilometre",
    ] {
        let kind = super::options::objective_kind(objective, token).expect("objective");
        assert_eq!(
            display_option(objective, token),
            crate::views::tr(alas_pipeline::optimizer_summary::objective::objective_label(kind))
        );
        // The objective labels follow the schema's option source, not any
        // value that happens to spell an objective token.
        assert_eq!(super::options::objective_kind(&plain, token), None);
    }
    let schema = alas_config::SolverSettings::default().schema();
    let field = schema.field("workers").unwrap();
    let text = super::options::sentinel_text(field, 0.0).unwrap();
    let threads = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    assert_eq!(
        text,
        crate::views::tr_fields(
            "Automatic (all {count} threads)",
            &[("count", threads.to_string())]
        )
    );
    assert_eq!(parse_number_or_sentinel(&text, field), Some(0.0));
    assert_eq!(parse_number_or_sentinel("3", field), Some(3.0));
    assert_eq!(super::options::sentinel_text(field, 3.0), None);
    // Another whole-number field, even one named `workers`, keeps zero a
    // number unless its schema declares the automatic meaning.
    let undeclared = layout_test_leaf("workers", "Workers", Kind::Int, json!(0));
    assert_eq!(super::options::sentinel_text(&undeclared, 0.0), None);
}

fn plain_option_field() -> Field {
    layout_test_leaf("plain", "Plain", Kind::Str, json!(""))
}

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
        assert!(
            ctx.fonts(|fonts| fonts.has_glyphs(&font, super::MENU_GLYPH)),
            "the bundled fonts must cover the option-menu glyph at {style:?}"
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
fn every_fraction_reference_in_the_schema_reads_in_spanish() {
    fn units(fields: &[alas_config::Field], out: &mut Vec<&'static str>) {
        for field in fields {
            out.push(field.unit);
            if let alas_config::Entry::Node(node) = &field.entry {
                units(&node.fields, out);
            }
        }
    }
    let mut all = Vec::new();
    units(
        &alas_config::AlasConfig::default().schema().fields,
        &mut all,
    );
    let references: Vec<_> = all
        .into_iter()
        .filter(|unit| unit.starts_with("fraction of ") || unit.starts_with("0-1 of "))
        .collect();
    assert!(!references.is_empty());
    alas_i18n::es::install();
    let previous = alas_i18n::get_language();
    for unit in references {
        alas_i18n::set_language(Some("en"));
        let english = display_unit(unit);
        alas_i18n::set_language(Some("es"));
        let spanish = display_unit(unit);
        assert!(
            english.starts_with("of ") && spanish.starts_with("de"),
            "{unit}: {english:?} / {spanish:?}"
        );
    }
    alas_i18n::set_language(Some(&previous));
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
        "aircraft_type",
        "cabin_preset",
        "main_deck_uld",
        "lower_deck_uld",
        "loading_strategy",
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
fn cargo_choice_lists_cover_the_loader_inputs() {
    let config = AlasConfig::default();
    let values = serde_json::to_value(&config).expect("default config serializes");
    let schema = config.schema();
    let cabin = schema.field("cabin").expect("cabin group");
    let Entry::Node(cabin) = &cabin.entry else {
        panic!("cabin is a group");
    };
    let cargo = cabin.field("cargo").expect("cargo group");
    let Entry::Node(cargo) = &cargo.entry else {
        panic!("cargo is a group");
    };
    let cargo_values = values
        .get("cabin")
        .and_then(|cabin| cabin.get("cargo"))
        .expect("cargo values");

    let main = cargo.field("main_deck_uld").expect("main-deck ULD field");
    let lower = cargo.field("lower_deck_uld").expect("lower-deck ULD field");
    let strategy = cargo
        .field("loading_strategy")
        .expect("loading strategy field");

    let main_options = resolved_options(main, cargo_values).expect("main-deck ULD options");
    assert_eq!(main_options.len(), alas_payload::cargo::ULD_DATABASE.len());
    assert!(main_options.contains(&"PMC".to_owned()));
    assert!(main_options.contains(&"M1".to_owned()));

    let lower_options = resolved_options(lower, cargo_values).expect("lower-deck ULD options");
    assert_eq!(lower_options.first(), Some(&"AUTO".to_owned()));
    assert!(lower_options.contains(&"LD3".to_owned()));
    assert!(lower_options.contains(&"BLK".to_owned()));

    assert_eq!(
        resolved_options(strategy, cargo_values).expect("loading strategy options"),
        vec!["target_cg", "min_pallets", "door_proximity", "uniform"]
    );
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

fn layout_test_leaf(name: &'static str, label: &'static str, kind: Kind, value: Value) -> Field {
    Field {
        name,
        label,
        unit: "",
        help: "",
        advanced: false,
        entry: Entry::Leaf(LeafField {
            kind,
            optional_value_kind: None,
            value,
            min: None,
            max: None,
            decimals: None,
            columns: None,
            readonly_unless: None,
            options: None,
            zero_means: None,
        }),
    }
}

#[test]
fn optional_numbers_and_text_keep_their_declared_json_types() {
    let schema = AlasConfig::default().schema();
    let dry_mass = leaf_named(&schema.fields, "engine_dry_mass_kg").expect("dry mass field");
    let seed = leaf_named(&schema.fields, "seed").expect("seed field");
    let leaf_kind = |field: &Field| match &field.entry {
        Entry::Leaf(leaf) => leaf.optional_value_kind.expect("declared optional type"),
        Entry::Node(_) => panic!("expected a leaf"),
    };

    assert_eq!(leaf_kind(dry_mass), OptionalValueKind::Float);
    assert_eq!(leaf_kind(seed), OptionalValueKind::I64);
    assert_eq!(
        alas_config::Leaf::optional_value_kind(&None::<String>),
        Some(OptionalValueKind::String)
    );
    assert_eq!(
        optional_value_from_text("481.7", leaf_kind(dry_mass), dry_mass),
        Some(json!(481.7))
    );
    assert_eq!(
        optional_value_from_text("-42", leaf_kind(seed), seed),
        Some(json!(-42))
    );
    assert_eq!(
        optional_value_from_text("fuselage", OptionalValueKind::String, dry_mass),
        Some(json!("fuselage"))
    );
    assert_eq!(
        optional_value_from_text(" ", OptionalValueKind::String, dry_mass),
        Some(json!(" "))
    );
    assert_eq!(
        optional_value_from_text("", leaf_kind(dry_mass), dry_mass),
        Some(Value::Null)
    );
    assert_eq!(
        optional_value_from_text("not a mass", leaf_kind(dry_mass), dry_mass),
        None
    );

    let mut values = serde_json::to_value(AlasConfig::default()).expect("default JSON");
    values["mass_model"]["flops_turboprop"]["engine_dry_mass_kg"] =
        optional_value_from_text("481.7", leaf_kind(dry_mass), dry_mass).expect("numeric edit");
    let decoded: AlasConfig = serde_json::from_value(values.clone()).expect("typed config");
    assert_eq!(
        decoded.mass_model.flops_turboprop.engine_dry_mass_kg,
        Some(481.7)
    );
    values["mass_model"]["flops_turboprop"]["engine_dry_mass_kg"] =
        optional_value_from_text("", leaf_kind(dry_mass), dry_mass).expect("cleared edit");
    let decoded: AlasConfig = serde_json::from_value(values).expect("typed config after clear");
    assert_eq!(decoded.mass_model.flops_turboprop.engine_dry_mass_kg, None);
}

fn rendered_label_positions(fields: &[Field], open_root_nodes: bool) -> Vec<(String, egui::Pos2)> {
    fn visit(shape: &egui::Shape, out: &mut Vec<(String, egui::Pos2)>) {
        match shape {
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    visit(shape, out);
                }
            }
            egui::Shape::Text(text) => {
                out.push((text.galley.text().to_owned(), text.pos));
            }
            _ => {}
        }
    }

    let ctx = egui::Context::default();
    let mut values = json!({});
    let mut output = None;
    for _ in 0..2 {
        output = Some(ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(780.0, 900.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    super::dynamic_form_with_open_root_nodes(
                        ui,
                        fields,
                        &mut values,
                        &Default::default(),
                        Some("en"),
                        false,
                        open_root_nodes,
                    );
                });
            },
        ));
    }
    let mut labels = Vec::new();
    for clipped in &output.expect("rendered form").shapes {
        visit(&clipped.shape, &mut labels);
    }
    labels
}

fn label_position(labels: &[(String, egui::Pos2)], label: &str) -> egui::Pos2 {
    labels
        .iter()
        .find(|(text, _)| text == label)
        .map(|(_, position)| *position)
        .unwrap_or_else(|| panic!("{label} was not painted: {labels:?}"))
}

#[test]
fn a_tall_editor_does_not_hold_up_the_next_field_in_another_column() {
    let fields = [
        layout_test_leaf(
            "long_list",
            "Tall input list",
            Kind::NumberList,
            Value::from((0..70).map(f64::from).collect::<Vec<_>>()),
        ),
        layout_test_leaf("second", "Second input", Kind::Float, json!(1.0)),
        layout_test_leaf("third", "Third input", Kind::Float, json!(2.0)),
    ];
    let labels = rendered_label_positions(&fields, false);
    let second = label_position(&labels, "Second input");
    let third = label_position(&labels, "Third input");
    assert!(third.y > second.y, "the third field follows the second");
    assert!(
        (third.x - second.x).abs() < 5.0 && third.y - second.y < 90.0,
        "the third field should fill below the short second field: {second:?}, {third:?}"
    );
}

#[test]
fn expandable_groups_stack_across_the_page_instead_of_sharing_a_row() {
    let node = |name, label| Field {
        name,
        label,
        unit: "",
        help: "",
        advanced: false,
        entry: Entry::Node(Node {
            type_name: "Layout test group",
            fields: vec![layout_test_leaf("value", "Value", Kind::Float, json!(1.0))],
        }),
    };
    let fields = [node("first", "First group"), node("second", "Second group")];
    let labels = rendered_label_positions(&fields, true);
    let first = label_position(&labels, "First group");
    let second = label_position(&labels, "Second group");
    assert!(
        (second.x - first.x).abs() < 5.0 && second.y > first.y + 40.0,
        "groups should use full-width, consecutive rows: {first:?}, {second:?}"
    );
}

#[test]
fn fixed_option_values_are_capitalized_without_changing_their_data_value() {
    let plain = plain_option_field();
    assert_eq!(display_option(&plain, "passenger"), "Passenger");
    assert_eq!(display_option(&plain, "auto"), "Auto");
    assert_eq!(display_option(&plain, "target_cg"), "Target CG");
    assert_eq!(display_option(&plain, "block_fuel"), "Block fuel");
    assert_eq!(display_option(&plain, "long_haul"), "Long haul");
    assert_eq!(
        display_option(&plain, "lth_civil_transport_v1"),
        "LTH civil transport v1"
    );
    assert_eq!(
        display_option(&plain, "fuel_per_seat_kilometre"),
        "Fuel per seat-kilometre"
    );
    // Catalogue names are not identifiers and are never rewritten.
    assert_eq!(display_option(&plain, "LD3-45"), "LD3-45");
    assert_eq!(
        display_option(&plain, "7075-T6 aluminium"),
        "7075-T6 aluminium"
    );
}

#[test]
fn cabin_presets_display_a_descriptive_airline_independent_name() {
    let plain = plain_option_field();
    // The combo box shows a descriptive name, but `resolved_options` above
    // still returns the serialized identifier a saved config stores and
    // `apply_cabin_preset` matches on, only the label changes.
    assert_eq!(
        display_option(&plain, "Ryanair"),
        "High-density single-class"
    );
    assert_eq!(
        display_option(&plain, "Iberia"),
        "Two-class (Business/Economy)"
    );
    assert_eq!(
        display_option(&plain, "Emirates"),
        "Three-class (First/Business/Economy)"
    );
    // Unaffected cabin/cargo identifiers still pass through unchanged.
    assert_eq!(display_option(&plain, "Custom"), "Custom");
    assert_eq!(display_option(&plain, "Max payload"), "Max payload");
    assert_eq!(display_option(&plain, "Dense payload"), "Dense payload");
}

fn node_named<'a>(fields: &'a [Field], name: &str) -> Option<&'a Node> {
    fields.iter().find_map(|field| match &field.entry {
        Entry::Node(node) if field.name == name => Some(node),
        Entry::Node(node) => node_named(&node.fields, name),
        Entry::Leaf(_) => None,
    })
}

fn collect_shapes(
    shape: &egui::Shape,
    texts: &mut Vec<(String, egui::Rect)>,
    frames: &mut Vec<egui::Rect>,
) {
    match shape {
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_shapes(shape, texts, frames);
            }
        }
        egui::Shape::Text(text) => texts.push((
            text.galley.text().to_owned(),
            egui::Rect::from_min_size(text.pos, text.galley.size()),
        )),
        egui::Shape::Rect(rect) => frames.push(rect.rect),
        _ => {}
    }
}

#[test]
fn themed_requirements_remain_compact_across_frames() {
    let config = AlasConfig::default();
    let schema = config.schema();
    let requirements = node_named(&schema.fields, "requirements").expect("requirements");
    let fields: Vec<_> = requirements
        .fields
        .iter()
        .filter(|field| !field.advanced)
        .cloned()
        .collect();
    for width in [480.0, 780.0, 1400.0] {
        let ctx = egui::Context::default();
        crate::theme::apply_theme(crate::theme::AppTheme::Dark, &ctx);
        let mut values = serde_json::to_value(&config.requirements).unwrap();
        for frame in 0..12 {
            let mut height = 0.0;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 1800.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        height = ui
                            .vertical(|ui| {
                                super::dynamic_form(
                                    ui,
                                    &fields,
                                    &mut values,
                                    &Default::default(),
                                    Some("en"),
                                    false,
                                );
                            })
                            .response
                            .rect
                            .height();
                        // A locally taller form must not set the cell height of
                        // unrelated single-line forms on the next frame.
                        let other_fields = [
                            layout_test_leaf(
                                "another_checkbox",
                                "Another checkbox",
                                Kind::Bool,
                                json!(false),
                            ),
                            layout_test_leaf(
                                "another_field",
                                "Another field",
                                Kind::Float,
                                json!(1.0),
                            ),
                        ];
                        ui.scope(|ui| {
                            ui.set_width(620.0);
                            ui.spacing_mut().interact_size.y = 80.0;
                            super::dynamic_form(
                                ui,
                                &other_fields,
                                &mut values,
                                &Default::default(),
                                Some("en"),
                                false,
                            );
                        });
                    });
                },
            );
            let rows = fields.len().div_ceil(form_column_count(width - 16.0));
            assert!(
                height <= rows as f32 * 100.0,
                "width={width}, frame={frame}: {height} points for {rows} rows"
            );
            if frame == 11 {
                eprintln!("requirements width={width}: {height} points for {rows} rows");
            }
        }
    }
}

/// The Advanced Settings > Mass FLOPS transport inputs, rendered at the width
/// of a maximized Advanced Settings window: optional fields with a unit used
/// to overflow their column by the text-box margin, which widened the column
/// and pushed later editors and units into the neighbouring column's labels.
#[test]
fn a_three_column_form_has_no_overlapping_editors_and_keeps_its_rows_aligned() {
    let schema = AlasConfig::default().schema();
    let transport = node_named(&schema.fields, "flops_transport").expect("FLOPS transport node");
    let fields: Vec<Field> = transport
        .fields
        .iter()
        .filter(|field| matches!(field.entry, Entry::Leaf(_)))
        .cloned()
        .map(|mut field| {
            field.advanced = false;
            field
        })
        .collect();
    assert!(fields.len() >= 9, "enough leaves for three rows");
    let mut values = json!({});
    let ctx = egui::Context::default();
    let mut output = None;
    for _ in 0..4 {
        output = Some(ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1400.0, 2400.0),
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
        ));
    }
    let mut texts = Vec::new();
    let mut frames = Vec::new();
    for clipped in &output.expect("rendered form").shapes {
        collect_shapes(&clipped.shape, &mut texts, &mut frames);
    }

    // Editor frames (single-line widgets) never intersect one another.
    let editors: Vec<egui::Rect> = frames
        .into_iter()
        .filter(|rect| rect.height() > 8.0 && rect.height() < 60.0 && rect.width() < 1000.0)
        .collect();
    for (i, a) in editors.iter().enumerate() {
        for b in &editors[i + 1..] {
            let overlap = a.intersect(*b);
            let nested = a.contains_rect(*b) || b.contains_rect(*a);
            assert!(
                nested || overlap.width() <= 0.5 || overlap.height() <= 0.5,
                "editor frames overlap: {a:?} and {b:?}"
            );
        }
    }
    // No painted text runs into another (units against the next labels).
    for (i, (text_a, a)) in texts.iter().enumerate() {
        for (text_b, b) in &texts[i + 1..] {
            let overlap = a.intersect(*b);
            assert!(
                overlap.width() <= 0.5 || overlap.height() <= 0.5,
                "{text_a:?} {a:?} overlaps {text_b:?} {b:?}"
            );
        }
    }
    // Leaf labels share one baseline per row of three.
    let label_top = |label: &str| {
        texts
            .iter()
            .find(|(text, _)| text == label)
            .map(|(_, rect)| rect.top())
    };
    let tops: Vec<f32> = fields
        .iter()
        .filter(|field| {
            !matches!(&field.entry, Entry::Leaf(leaf) if leaf.kind == Kind::Bool && leaf.optional_value_kind.is_none())
        })
        .filter_map(|field| label_top(field.label))
        .collect();
    assert!(tops.len() >= 6, "labels found: {tops:?}");
    let mut rows: Vec<f32> = Vec::new();
    for top in &tops {
        if !rows.iter().any(|row| (row - top).abs() < 0.5) {
            rows.push(*top);
        }
    }
    assert!(
        rows.len() <= fields.len().div_ceil(3),
        "labels drift off a three-column row grid: {tops:?}"
    );
}

#[test]
fn an_empty_optional_box_is_as_tall_as_the_numeric_box_beside_it() {
    let schema = AlasConfig::default().schema();
    let ribs = leaf_named(&schema.fields, "num_ribs_override").expect("rib count override");
    let stations = layout_test_leaf("stations", "Stations", Kind::Int, json!(200));
    let ctx = egui::Context::default();
    crate::theme::apply_theme(crate::theme::AppTheme::Light, &ctx);
    let mut heights = (0.0, 0.0);
    for _ in 0..2 {
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let mut empty = Value::Null;
                let optional = ui.scope(|ui| {
                    super::editors::edit_leaf(
                        ui,
                        ribs,
                        Kind::Optional,
                        &mut empty,
                        "test",
                        None,
                        "",
                    )
                });
                let mut count = json!(200);
                let numeric = ui.scope(|ui| {
                    super::editors::edit_leaf(
                        ui,
                        &stations,
                        Kind::Int,
                        &mut count,
                        "test",
                        None,
                        "",
                    )
                });
                heights = (
                    optional.response.rect.height(),
                    numeric.response.rect.height(),
                );
            });
        });
    }
    assert!(
        (heights.0 - heights.1).abs() < 0.5,
        "optional {} vs numeric {}",
        heights.0,
        heights.1
    );
}
