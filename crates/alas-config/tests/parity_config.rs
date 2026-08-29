// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares every configuration struct against the dataclass it was ported
//! from: what a freshly constructed one holds, and what it tells the settings
//! interface about each of its fields.
//!
//! Both halves are compared at `exact`. A default is copied, not computed, so
//! any difference at all is a transposed digit -- and a transposed digit here
//! produces a plausible aircraft rather than a failure, which is the worst
//! kind of defect this program can have. The form description is compared for
//! the same reason one step removed: a field offered with the wrong unit or
//! without its bound produces a wrong number by way of the person filling it
//! in.
//!
//! Two deliberate exceptions, both of which add prose and change no value:
//!
//! * `help` is compared only where the dataclass field declared one. This
//!   port documents every field, including those upstream left blank, which
//!   is what CONTRIBUTING.md requires of it.
//! * A string field whose accepted values live in a crate above this one
//!   carries the name of that list rather than the list. The fixture's
//!   resolved list is checked where this crate can resolve it, and the rest
//!   is checked where the resolution happens.
//! * `optimize_passenger_capacity` is a native load-case switch added after a
//!   physical audit found the translated optimizer replacing real-preset
//!   passenger targets. Unit and preset tests pin its two modes; there is no
//!   Python field to compare it with here.
//! * `run_sol_vibration_random` requests native force-PSD RMS integration.
//!   The frozen default and the product default both enable it, but the Rust
//!   calculation replaces the invalid acceleration-PSD/force-receptance path.
//! * The structures enable/station help names the native structural wing-mass
//!   centroid consumer. Both old and corrected prose are pinned below; values,
//!   field order, and exact comparison remain unchanged.
//! * The seven solver/mission help fields listed by
//!   [`solver_agnostic_help_correction`] remove retired implementation
//!   attribution from product-facing prose. Their corrected text and the
//!   frozen reference text are both compared exactly below; no tolerance is
//!   widened and no schema/value contract is relaxed.
//! * The active transport planform adds four explicit side-of-body/kink
//!   fields to `WingConfig`. The frozen Python schema has none of them, so the
//!   absent upstream fields and the source-corrected Rust values/schema are
//!   both checked explicitly below.
//!
//! `label` is compared always, including where it was derived from the field
//! name: a derived label that disagrees means the name-to-label rule was
//! ported wrong, and every one of those labels is a key into the translation
//! catalog.

// This file is itself a test binary, so an unwrap that fails is the
// assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use std::collections::BTreeMap;

use alas_config::{
    AlasConfig, AnalysisConfig, CabinConfig, ConfigNode, ControlSurfacesConfig, DesignRequirements,
    DragModelConfig, EngineConfig, Entry, Field, GeometryConfig, LandingGearConfig, LeafField,
    MassModelConfig, MissionConfig, MsesConfig, Node, OptimizerConfig, PerformanceConfig,
    PropulsionCycleConfig, StructuresConfig,
};
use alas_testkit::{Comparison, Tier};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Whether one field declared its own explanation, keyed by its dotted path
/// from the configuration's root. A path is the same on both sides of the
/// port; a type name is not, since each language names types its own way.
#[derive(Deserialize)]
struct Declared {
    help: bool,
}

#[derive(Deserialize)]
struct TypeFixture {
    defaults: Value,
    schema: Value,
    declared: BTreeMap<String, Declared>,
}

#[derive(Deserialize)]
struct Fixture {
    types: BTreeMap<String, TypeFixture>,
}

struct TransportPlanformField {
    name: &'static str,
    label: &'static str,
    unit: &'static str,
    help: &'static str,
    value: Option<f64>,
}

const TRANSPORT_PLANFORM_FIELDS: [TransportPlanformField; 4] = [
    TransportPlanformField {
        name: "side_of_body_span_fraction",
        label: "Wing side-of-body span location",
        unit: "0-1 of semispan",
        help: "Optional fuselage side-of-body station as a semispan fraction. Leave unset to retain the legacy centerline-root planform.",
        value: Some(0.10),
    },
    TransportPlanformField {
        name: "side_of_body_chord_ratio",
        label: "Wing side-of-body chord ratio",
        unit: "root chord ratio",
        help: "Optional side-of-body chord divided by root chord. A value near one retains wing-box and high-lift depth inboard.",
        value: None,
    },
    TransportPlanformField {
        name: "kink_span_fraction",
        label: "Wing kink span location",
        unit: "0-1 of semispan",
        help: "Optional Yehudi-kink station as a semispan fraction. When unset, the legacy wing break span location remains active.",
        value: Some(0.37),
    },
    TransportPlanformField {
        name: "outboard_le_sweep_deg",
        label: "Outboard leading-edge sweep",
        unit: "deg",
        help: "Legacy reference-replay override. Product transport wings use the design-vector sweep across both panels so this value cannot create an unintended leading-edge crank.",
        value: None,
    },
];

fn fixture() -> Fixture {
    alas_testkit::load("config", "defaults")
}

/// Every configuration type the fixture covers, checked the same way.
///
/// One line per ported module: the fixture key, and a default instance. A
/// type in the fixture that no line names fails
/// [`the_fixture_has_no_type_this_test_forgot_to_check`], so this list cannot
/// silently fall behind the modules.
fn checked_types() -> Vec<(&'static str, Box<dyn Checkable>)> {
    vec![
        (
            "DragModelConfig",
            Box::new(DragModelConfig::default()) as Box<dyn Checkable>,
        ),
        ("MissionConfig", Box::new(MissionConfig::default())),
        ("MSESConfig", Box::new(MsesConfig::default())),
        ("AnalysisConfig", Box::new(AnalysisConfig::default())),
        ("MassModelConfig", Box::new(MassModelConfig::default())),
        ("LandingGearConfig", Box::new(LandingGearConfig::default())),
        (
            "ControlSurfacesConfig",
            Box::new(ControlSurfacesConfig::default()),
        ),
        ("PerformanceConfig", Box::new(PerformanceConfig::default())),
        ("StructuresConfig", Box::new(StructuresConfig::default())),
        ("CabinConfig", Box::new(CabinConfig::default())),
        (
            "DesignRequirements",
            Box::new(DesignRequirements::default()),
        ),
        (
            "PropulsionCycleConfig",
            Box::new(PropulsionCycleConfig::default()),
        ),
        ("OptimizerConfig", Box::new(OptimizerConfig::default())),
        ("GeometryConfig", Box::new(GeometryConfig::default())),
        // `GeometryConfig` hides its engine group, so nothing above describes
        // those fields. See the note in `golden/generators/gen_config.py`.
        ("EngineConfig", Box::new(EngineConfig::default())),
        // The aggregate. Its own fields are the preset name and the two
        // airports; what this entry pins down is the composition -- which
        // groups a run is made of, in which order the settings screen lists
        // them, and that each arrives at its own defaults.
        ("ALASConfig", Box::new(AlasConfig::default())),
    ]
}

/// A configuration value this test can compare without knowing its type.
trait Checkable {
    fn schema(&self) -> Node;
    fn as_value(&self) -> Value;
}

impl<T: ConfigNode + Serialize> Checkable for T {
    fn schema(&self) -> Node {
        ConfigNode::schema(self)
    }

    fn as_value(&self) -> Value {
        serde_json::to_value(self).expect("a configuration serializes")
    }
}

#[test]
fn every_default_matches_the_reference() {
    let fixture = fixture();
    let mut comparison = Comparison::new("alas-config defaults", Tier::Exact);

    for (key, config) in checked_types() {
        let expected = &fixture
            .types
            .get(key)
            .unwrap_or_else(|| panic!("the fixture has no type `{key}`"))
            .defaults;
        compare_values(&mut comparison, key, &config.as_value(), expected);
    }
    comparison.finish();
}

#[test]
fn every_field_description_matches_the_reference() {
    let fixture = fixture();
    let mut comparison = Comparison::new("alas-config field descriptions", Tier::Exact);

    for (key, config) in checked_types() {
        let entry = fixture
            .types
            .get(key)
            .unwrap_or_else(|| panic!("the fixture has no type `{key}`"));
        compare_node(
            &mut comparison,
            key,
            "",
            &config.schema(),
            &entry.schema,
            &entry.declared,
        );
    }
    comparison.finish();
}

#[test]
fn the_fixture_has_no_type_this_test_forgot_to_check() {
    let fixture = fixture();
    let checked: Vec<&str> = checked_types().into_iter().map(|(key, _)| key).collect();
    for key in fixture.types.keys() {
        assert!(
            checked.contains(&key.as_str()),
            "the fixture covers `{key}` but this test does not check it; \
             add it to `checked_types`"
        );
    }
}

/// Compare two default trees, reporting each disagreeing key by its path
/// rather than dumping both trees.
fn compare_values(comparison: &mut Comparison, path: &str, actual: &Value, expected: &Value) {
    if let Some((upstream, corrected)) = vibration_performance_default_correction(path) {
        comparison.exact(&format!("{path}: frozen Python value"), expected, &upstream);
        comparison.exact(
            &format!("{path}: source-corrected Rust value"),
            actual,
            &corrected,
        );
        return;
    }
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            for (key, expected_value) in expected {
                // The native mission intentionally drops the two Python
                // runtime-location keys. They remain in historical fixtures
                // solely to prove the migration path.
                if (path.ends_with("MissionConfig") || path.ends_with(".mission"))
                    && (key == "suave_venv_dir" || key == "suave_runner_dir")
                {
                    continue;
                }
                let child = format!("{path}.{key}");
                match actual.get(key) {
                    Some(actual_value) => {
                        compare_values(comparison, &child, actual_value, expected_value);
                    }
                    None => {
                        comparison.exact(&child, &Value::Null, expected_value);
                    }
                }
            }
            for key in actual.keys() {
                if (path.ends_with("MissionConfig") || path.ends_with(".mission"))
                    && (key == "suave_venv_dir" || key == "suave_runner_dir")
                {
                    continue;
                }
                if is_native_config_field(path, key) {
                    continue;
                }
                if !expected.contains_key(key) {
                    if let Some(field) = transport_planform_field(path, key) {
                        comparison.exact(
                            &format!("{path}.{key}: frozen Python field absent"),
                            &expected.contains_key(key),
                            &false,
                        );
                        comparison.exact(
                            &format!("{path}.{key}: source-corrected Rust default"),
                            actual.get(key).unwrap_or(&Value::Null),
                            &serde_json::to_value(field.value).unwrap(),
                        );
                    } else {
                        comparison.exact(
                            &format!("{path}.{key}"),
                            &"present".to_owned(),
                            &"absent upstream".to_owned(),
                        );
                    }
                }
            }
        }
        (actual, expected) => {
            comparison.exact(path, actual, expected);
        }
    }
}

/// The frozen Python configuration enabled an expensive 500 Hz harmonic
/// output by default. The product default keeps force-PSD RMS enabled while
/// making the extended sweep explicit and bounded to its useful RMS band.
fn vibration_performance_default_correction(path: &str) -> Option<(Value, Value)> {
    if path.ends_with(".run_sol_vibration_sine") {
        Some((Value::Bool(true), Value::Bool(false)))
    } else if path.ends_with(".freq_sweep_max_hz") {
        Some((serde_json::json!(500.0), serde_json::json!(60.0)))
    } else {
        None
    }
}

fn compare_node(
    comparison: &mut Comparison,
    label: &str,
    path: &str,
    node: &Node,
    expected: &Value,
    declared: &BTreeMap<String, Declared>,
) {
    let expected_fields = expected
        .get("fields")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("{label}: the fixture entry has no `fields`"));
    compare_transport_planform_schema(comparison, label, node, expected_fields);

    // A field the interface never shows is omitted by both sides, so the two
    // lists are directly comparable -- and a field one side hides and the
    // other does not shows up here as an ordering disagreement, which is
    // exactly what it is.
    let fields: Vec<&Field> = node
        .fields
        .iter()
        .filter(|field| {
            !is_native_config_field(node.type_name, field.name)
                && !is_transport_planform_field_name(node.type_name, field.name)
                && (node.type_name != "MassModelConfig"
                    || !matches!(field.name, "systems_mass_method" | "flops_transport"))
        })
        .collect();
    let names: Vec<&str> = fields.iter().map(|field| field.name).collect();
    let expected_fields: Vec<&Value> = expected_fields
        .iter()
        .filter(|field| {
            let name = field.get("name").and_then(Value::as_str);
            !matches!(name, Some("suave_venv_dir" | "suave_runner_dir"))
                && !name.is_some_and(|name| product_hidden_cabin_field(node.type_name, name))
        })
        .collect();
    let expected_names: Vec<&str> = expected_fields
        .iter()
        .filter_map(|field| field.get("name").and_then(Value::as_str))
        .collect();
    comparison.exact(&format!("{label}: field order"), &names, &expected_names);
    if names != expected_names {
        return;
    }

    for (field, expected_field) in fields.into_iter().zip(expected_fields) {
        compare_field(
            comparison,
            &format!("{label}.{}", field.name),
            &format!("{path}.{}", field.name),
            field,
            expected_field,
            declared,
        );
    }
}

fn transport_planform_field(path: &str, key: &str) -> Option<&'static TransportPlanformField> {
    if path.ends_with("GeometryConfig.wing") || path.ends_with(".geometry.wing") {
        TRANSPORT_PLANFORM_FIELDS
            .iter()
            .find(|field| field.name == key)
    } else {
        None
    }
}

fn is_transport_planform_field_name(type_name: &str, name: &str) -> bool {
    type_name == "WingConfig"
        && TRANSPORT_PLANFORM_FIELDS
            .iter()
            .any(|field| field.name == name)
}

/// Product cabin configuration deliberately exposes only the three target
/// seat shares. The frozen fields remain serialized for compatibility, while
/// this parity comparison continues to check every field that is still part
/// of the user-facing schema.
fn product_hidden_cabin_field(type_name: &str, name: &str) -> bool {
    match type_name {
        "DesignRequirements" => name == "num_passengers",
        "PassengerCabinConfig" => matches!(
            name,
            "class_mix_mode"
                | "premium"
                | "aisle_width_m"
                | "galley_count"
                | "lavatory_count"
                | "checked_bag_mass_kg"
                | "belly_cargo_kg"
                | "wall_thickness_m"
                | "min_exit_pair_spacing_m"
                | "exit_capacity_realism_factor"
        ),
        "SeatClassConfig" => matches!(
            name,
            "count" | "abreast" | "pitch_m" | "width_m" | "mass_per_pax_kg"
        ),
        _ => false,
    }
}

fn compare_transport_planform_schema(
    comparison: &mut Comparison,
    label: &str,
    node: &Node,
    frozen_fields: &[Value],
) {
    if node.type_name != "WingConfig" {
        return;
    }

    for expected_field in &TRANSPORT_PLANFORM_FIELDS {
        let frozen_has_field = frozen_fields
            .iter()
            .any(|field| field.get("name").and_then(Value::as_str) == Some(expected_field.name));
        comparison.exact(
            &format!(
                "{label}.{}: frozen Python schema field absent",
                expected_field.name
            ),
            &frozen_has_field,
            &false,
        );

        let field = node
            .fields
            .iter()
            .find(|field| field.name == expected_field.name);
        comparison.exact(
            &format!(
                "{label}.{}: source-corrected Rust schema field present",
                expected_field.name
            ),
            &field.is_some(),
            &true,
        );
        let Some(field) = field else {
            continue;
        };

        comparison.exact(
            &format!("{label}.{}.label", expected_field.name),
            &field.label,
            &expected_field.label,
        );
        comparison.exact(
            &format!("{label}.{}.unit", expected_field.name),
            &field.unit,
            &expected_field.unit,
        );
        comparison.exact(
            &format!("{label}.{}.advanced", expected_field.name),
            &field.advanced,
            &false,
        );
        comparison.exact(
            &format!("{label}.{}.help", expected_field.name),
            &field.help,
            &expected_field.help,
        );

        let Entry::Leaf(leaf) = &field.entry else {
            comparison.exact(
                &format!("{label}.{}.kind", expected_field.name),
                &"node",
                &"float",
            );
            continue;
        };
        let expected_kind = if expected_field.value.is_some() {
            "float"
        } else {
            "optional"
        };
        comparison.exact(
            &format!("{label}.{}.kind", expected_field.name),
            &serde_json::to_value(leaf.kind).unwrap(),
            &Value::String(expected_kind.to_owned()),
        );
        comparison.exact(
            &format!("{label}.{}.value", expected_field.name),
            &leaf.value,
            &serde_json::to_value(expected_field.value).unwrap(),
        );
        comparison.exact(
            &format!("{label}.{}.min", expected_field.name),
            &to_value(leaf.min),
            &Option::<Value>::None,
        );
        comparison.exact(
            &format!("{label}.{}.max", expected_field.name),
            &to_value(leaf.max),
            &Option::<Value>::None,
        );
        comparison.exact(
            &format!("{label}.{}.decimals", expected_field.name),
            &to_value(leaf.decimals),
            &Option::<Value>::None,
        );
        comparison.exact(
            &format!("{label}.{}.columns", expected_field.name),
            &to_value(leaf.columns),
            &Option::<Value>::None,
        );
        if expected_field.name == "share_pct" {
            comparison.exact(
                &format!("{label}.{}.readonly_unless", expected_field.name),
                &to_value(leaf.readonly_unless),
                &Some(serde_json::json!({
                    "field": "requirements.cabin_preset",
                    "value": "Custom"
                })),
            );
        } else {
            comparison.exact(
                &format!("{label}.{}.readonly_unless", expected_field.name),
                &to_value(leaf.readonly_unless),
                &Option::<Value>::None,
            );
        }
        comparison.exact(
            &format!("{label}.{}: has no option list", expected_field.name),
            &leaf.options.is_none(),
            &true,
        );
    }
}

fn is_native_config_field(path: &str, key: &str) -> bool {
    (key == "method" && (path.ends_with("SolverSettings") || path.ends_with(".solver")))
        || (key == "random_force_psd_n2_per_hz"
            && (path.ends_with("StructuresConfig") || path.ends_with(".structures")))
        || (key == "optimize_passenger_capacity"
            && (path.ends_with("DesignRequirements") || path.ends_with(".requirements")))
        || (matches!(
            key,
            "transport_planform_constraints_enabled"
                | "transport_shape_priors_enabled"
                | "geometric_body_alpha_min_deg"
                | "geometric_body_alpha_max_deg"
                | "geometric_body_alpha_penalty_scale"
                | "min_root_wingbox_depth_m"
                | "min_break_wingbox_depth_m"
                | "min_break_wingbox_width_m"
                | "wingbox_packaging_penalty_scale"
                | "min_flap_area_fraction"
                | "flap_area_penalty_scale"
                | "max_root_bending_box_slenderness"
                | "bending_slenderness_penalty_scale"
                | "tankable_span_start_fraction"
                | "tankable_span_end_fraction"
                | "min_break_root_chord_ratio"
                | "min_tip_root_chord_ratio"
                | "min_inboard_te_sweep_deg"
                | "max_inboard_te_sweep_deg"
        ) && (path.ends_with("ObjectiveWeights") || path.ends_with(".weights")))
}

fn compare_field(
    comparison: &mut Comparison,
    label: &str,
    path: &str,
    field: &Field,
    expected: &Value,
    declared: &BTreeMap<String, Declared>,
) {
    let declaration = declared.get(path);

    if label.ends_with(".share_pct") {
        comparison.exact(
            &format!("{label}.label: product seat-share semantics"),
            &field.label,
            &"Target passenger share [%]",
        );
        comparison.exact(
            &format!("{label}.label: frozen floor-share semantics"),
            &expected.get("label").and_then(Value::as_str).unwrap_or(""),
            &"Share of cabin length [%]",
        );
    } else if is_random_response_correction(label) {
        comparison.exact(
            &format!("{label}.label: source-corrected Rust value"),
            &field.label,
            &"Calculate random-vibration RMS from SOL 111",
        );
        comparison.exact(
            &format!("{label}.label: frozen Python value"),
            &expected.get("label").and_then(Value::as_str).unwrap_or(""),
            &"Run random vibration (SOL 111)",
        );
    } else if is_legacy_acceleration_psd_correction(label) {
        comparison.exact(
            &format!("{label}.label: source-corrected Rust value"),
            &field.label,
            &"Legacy random vibration base PSD (not used)",
        );
        comparison.exact(
            &format!("{label}.label: frozen Python value"),
            &expected.get("label").and_then(Value::as_str).unwrap_or(""),
            &"Random vibration base PSD",
        );
    } else {
        compare_string(
            comparison,
            &format!("{label}.label"),
            field.label,
            expected,
            "label",
        );
    }
    compare_string(
        comparison,
        &format!("{label}.unit"),
        field.unit,
        expected,
        "unit",
    );
    comparison.exact(
        &format!("{label}.advanced"),
        &field.advanced,
        &expected
            .get("advanced")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    );

    // Where upstream declared no explanation it emits an empty string, and
    // this port supplies one. Comparing those would fail on prose the port is
    // required to add, so what is checked instead is that it was added.
    if label.ends_with(".share_pct") {
        comparison.exact(
            &format!("{label}.help: product seat-share semantics"),
            &field.help,
            &"Target percentage of passenger seats assigned to this class. The layout solver converts the target mix into floor-length allocations using each class's configured seat geometry, then fills the available cabin. Shares are normalised, so they need not add up to exactly 100. Set the class to 0 to remove it. Only editable with the Custom cabin preset.",
        );
        comparison.exact(
            &format!("{label}.help: frozen floor-share semantics"),
            &expected.get("help").and_then(Value::as_str).unwrap_or(""),
            &"Percentage of usable cabin floor length allocated to this class. Seat count is derived from it using this class's pitch/abreast and the real fuselage geometry. Shares are normalised, so they need not add up to exactly 100. Set the class to 0 to remove it. Only used when Class mix mode is 'percent'.",
        );
    } else if is_random_response_correction(label) {
        comparison.exact(
            &format!("{label}.help: source-corrected Rust value"),
            &field.help,
            &"Integrate the solved unit-force SOL 111 response against the one-sided force PSD below. This produces displacement RMS in metres over the configured frequency sweep; it is not a base-acceleration calculation.",
        );
        comparison.exact(
            &format!("{label}.help: frozen Python value"),
            &expected.get("help").and_then(Value::as_str).unwrap_or(""),
            &"Modal random-vibration response (PSD) to a white-noise engine-mounted excitation.",
        );
    } else if is_legacy_acceleration_psd_correction(label) {
        comparison.exact(
            &format!("{label}.help: source-corrected Rust value"),
            &field.help,
            &"Frozen-reference acceleration PSD retained for saved-file compatibility. It is not used by the product RMS path, which requires the force PSD above.",
        );
        comparison.exact(
            &format!("{label}.help: frozen Python value"),
            &expected.get("help").and_then(Value::as_str).unwrap_or(""),
            &"Flat white-noise acceleration power spectral density applied at the excitation point for the random-vibration case.",
        );
    } else if let Some((source_corrected, frozen_python)) = wing_centroid_help_correction(label) {
        comparison.exact(
            &format!("{label}.help: source-corrected Rust value"),
            &field.help,
            &source_corrected,
        );
        comparison.exact(
            &format!("{label}.help: frozen Python value"),
            &expected.get("help").and_then(Value::as_str).unwrap_or(""),
            &frozen_python,
        );
    } else if let Some((source_corrected, frozen_python)) = solver_agnostic_help_correction(label) {
        comparison.exact(
            &format!("{label}.help: source-corrected Rust value"),
            &field.help,
            &source_corrected,
        );
        comparison.exact(
            &format!("{label}.help: frozen Python value"),
            &expected.get("help").and_then(Value::as_str).unwrap_or(""),
            &frozen_python,
        );
    } else if declaration.is_some_and(|entry| entry.help) {
        compare_string(
            comparison,
            &format!("{label}.help"),
            field.help,
            expected,
            "help",
        );
    } else {
        comparison.exact(
            &format!("{label}.help: is documented"),
            &!field.help.is_empty(),
            &true,
        );
    }

    match &field.entry {
        Entry::Node(child) => {
            comparison.exact(
                &format!("{label}.kind"),
                &"dataclass".to_owned(),
                &expected
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
            );
            compare_node(comparison, label, path, child, expected, declared);
        }
        Entry::Leaf(leaf) => compare_leaf(comparison, label, leaf, expected),
    }
}

fn is_random_response_correction(path: &str) -> bool {
    path.ends_with(".run_sol_vibration_random")
}

fn is_legacy_acceleration_psd_correction(path: &str) -> bool {
    path.ends_with(".psd_base_g2_per_hz")
}

fn wing_centroid_help_correction(label: &str) -> Option<(&'static str, &'static str)> {
    if label == "StructuresConfig.enabled" || label.ends_with(".structures.enabled") {
        Some((
            "Size a generic wingbox (skin/spars/ribs) for the optimized design's main wing, write NASTRAN .bdf files, and compute theoretical (no-NASTRAN) deformations/stresses/frequencies as part of a normal Run, populating the Structural Analysis Results tab. This switch controls the downstream structural solve; the configured spars, materials, and gauges still define the main-wing mass centroid used by weight and balance, without replacing the Torenbeek total wing mass.",
            "Size a generic wingbox (skin/spars/ribs) for the optimized design's main wing, write NASTRAN .bdf files, and compute theoretical (no-NASTRAN) deformations/stresses/frequencies as part of a normal Run, populating the Structural Analysis Results tab. Does not affect the mass model, CG, or optimizer -- purely a downstream analysis, like MSES/Propulsion Analysis.",
        ))
    } else if label == "StructuresConfig.spanwise_stations"
        || label.ends_with(".structures.spanwise_stations")
    {
        Some((
            "Number of spanwise points used for load/moment/deflection integration, structural wing-mass centroid integration, and the analytical deformation/stress solver. Higher = smoother curves, slower.",
            "Number of spanwise points used for load/moment/deflection integration (sizing and the analytical deformation/stress solver). Higher = smoother curves, slower.",
        ))
    } else {
        None
    }
}

fn solver_agnostic_help_correction(label: &str) -> Option<(&'static str, &'static str)> {
    match label {
        "MSESConfig.alpha_sweep_n_points" | "ALASConfig.mses.alpha_sweep_n_points" => Some((
            "Number of alpha points in the MSES polar sweep. Kept small relative to the native VLM sweep (analysis.sweep_n_points) since each MSES point is a real viscous-compressible solve (~1-2s) rather than a linear-algebra VLM solve.",
            "Number of alpha points in the MSES polar sweep. Kept small relative to AeroSandbox's own VLM sweep (analysis.sweep_n_points) since each MSES point is a real viscous-compressible solve (~1-2s) rather than a linear-algebra VLM solve.",
        )),
        "AnalysisConfig.fine_chordwise_resolution"
        | "ALASConfig.analysis.fine_chordwise_resolution" => Some((
            "Chordwise panel resolution for the once-per-run final/reported analysis. A supercritical/cambered section needs ~8 chordwise panels for the VLM to resolve its camber line; at the coarse in-loop resolution the camber (and hence the zero-lift alpha) is under-captured, which inflates the reported cruise alpha by several degrees and under-predicts L/D by ~7%. Kept high here so the REPORTED cruise alpha (~1-4 deg) and L/D are physically accurate.",
            "Chordwise panel resolution for the once-per-run final/reported analysis. A supercritical/cambered section needs ~8 chordwise panels for the VLM to resolve its camber line; at the coarse in-loop resolution the camber (and hence the zero-lift alpha) is under-captured, which inflates the reported cruise alpha by several degrees and under-predicts L/D by ~7%. Kept high here so the REPORTED cruise alpha (~1-4 deg, matching SUAVE) and L/D are physically accurate.",
        )),
        "PropulsionCycleConfig.inlet_pressure_recovery"
        | "ALASConfig.propulsion_cycle.inlet_pressure_recovery" => Some((
            "Total-pressure recovery through the inlet (ram + duct losses). Matches the mission inlet_nozzle.pressure_ratio convention.",
            "Total-pressure recovery through the inlet (ram + duct losses). Matches SUAVE's inlet_nozzle.pressure_ratio.",
        )),
        "PropulsionCycleConfig.lpc_pressure_ratio_split"
        | "ALASConfig.propulsion_cycle.lpc_pressure_ratio_split" => Some((
            "Fixed low-pressure-compressor (booster) pressure ratio; the high-pressure compressor makes up the rest of the overall (core) pressure ratio (HPC = OPR / this value). Matches the fixed mission LPC split.",
            "Fixed low-pressure-compressor (booster) pressure ratio; the high-pressure compressor makes up the rest of the overall (core) pressure ratio (HPC = OPR / this value). Matches SUAVE's fixed LPC split.",
        )),
        "EngineConfig.thrust_kn" => Some((
            "Maximum rated sea-level-static take-off thrust, per engine. Drives propulsion mass, the Matching Chart T/W lookup, and the mission turbofan sizing target.",
            "Maximum rated sea-level-static take-off thrust, per engine. Drives propulsion mass, the Matching Chart T/W lookup, and the SUAVE turbofan sizing target.",
        )),
        "EngineConfig.bypass_ratio" => Some((
            "Ratio of bypass (fan duct) to core mass flow. Feeds the mission turbofan network and the Propulsion Analysis on-design cycle.",
            "Ratio of bypass (fan duct) to core mass flow. Feeds the SUAVE turbofan network and the Propulsion Analysis on-design cycle.",
        )),
        "EngineConfig.overall_pressure_ratio" => Some((
            "Total pressure ratio through the core compressors (LPC x HPC combined, NOT including the fan). Feeds mission compressor sizing (split into a fixed LPC ratio + a solved HPC ratio) and the Propulsion Analysis cycle's compressor_pressure_ratio.",
            "Total pressure ratio through the core compressors (LPC x HPC combined, NOT including the fan). Feeds SUAVE's compressor sizing (split into a fixed LPC ratio + a solved HPC ratio) and the Propulsion Analysis cycle's compressor_pressure_ratio.",
        )),
        "OptimizerConfig.solver.workers" | "ALASConfig.optimizer.solver.workers" => Some((
            "Number of native worker threads for differential-evolution candidate batches (>1 enables parallel evaluation; non-positive values are treated as 1). External evaluator adapters remain serial because they own mutable process/session state.",
            "Number of worker processes for parallel evaluation (>1 uses multiprocessing). Requires a picklable objective -- already the case for ALAS's optimizer.",
        )),
        _ => None,
    }
}

fn compare_leaf(comparison: &mut Comparison, path: &str, leaf: &LeafField, expected: &Value) {
    comparison.exact(
        &format!("{path}.kind"),
        &serde_json::to_value(leaf.kind).unwrap(),
        expected.get("kind").unwrap_or(&Value::Null),
    );
    let expected_value = expected.get("value").unwrap_or(&Value::Null);
    if let Some((upstream, corrected)) = vibration_performance_default_correction(path) {
        comparison.exact(
            &format!("{path}.value: frozen Python value"),
            expected_value,
            &upstream,
        );
        comparison.exact(
            &format!("{path}.value: source-corrected Rust value"),
            &leaf.value,
            &corrected,
        );
    } else {
        comparison.exact(&format!("{path}.value"), &leaf.value, expected_value);
    }

    compare_optional(comparison, path, "min", to_value(leaf.min), expected);
    compare_optional(comparison, path, "max", to_value(leaf.max), expected);
    compare_optional(
        comparison,
        path,
        "decimals",
        to_value(leaf.decimals),
        expected,
    );
    compare_optional(
        comparison,
        path,
        "columns",
        to_value(leaf.columns),
        expected,
    );
    if path.ends_with(".share_pct") {
        comparison.exact(
            &format!("{path}.readonly_unless: product Custom-only semantics"),
            &to_value(leaf.readonly_unless),
            &Some(serde_json::json!({
                "field": "requirements.cabin_preset",
                "value": "Custom"
            })),
        );
        let frozen_readonly = expected
            .get("readonly_unless")
            .cloned()
            .unwrap_or(Value::Null);
        comparison.exact(
            &format!("{path}.readonly_unless: frozen Python value"),
            &frozen_readonly,
            &Value::Null,
        );
    } else {
        compare_optional(
            comparison,
            path,
            "readonly_unless",
            to_value(leaf.readonly_unless),
            expected,
        );
    }

    // A string field whose accepted values are owned by a crate above this
    // one names the list instead of holding it. Where this crate can resolve
    // the list, the resolved values are compared; where it cannot, what is
    // checked is that the field claims a list at all wherever upstream
    // produced one.
    let expected_options = expected.get("options");
    match leaf.options {
        Some(source) => {
            comparison.exact(
                &format!("{path}: names an option list"),
                &true,
                &expected_options.is_some(),
            );
            if let Some(resolved) = source.options() {
                comparison.exact(
                    &format!("{path}.options"),
                    &serde_json::to_value(resolved).unwrap(),
                    expected_options.unwrap_or(&Value::Null),
                );
            }
            comparison.exact(
                &format!("{path}.editable"),
                &source.editable(),
                &expected
                    .get("editable")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            );
        }
        None => {
            comparison.exact(
                &format!("{path}: names an option list"),
                &false,
                &expected_options.is_some(),
            );
        }
    }
}

fn to_value<T: Serialize>(value: Option<T>) -> Option<Value> {
    value.map(|value| serde_json::to_value(value).expect("a schema constraint serializes"))
}

/// Compare a constraint that is present only when the field declared it. An
/// absent constraint is an absent key upstream, not a null, and the two are
/// different statements about the field.
fn compare_optional(
    comparison: &mut Comparison,
    path: &str,
    key: &str,
    actual: Option<Value>,
    expected: &Value,
) {
    comparison.exact(
        &format!("{path}.{key}"),
        &actual,
        &expected.get(key).cloned(),
    );
}

fn compare_string(
    comparison: &mut Comparison,
    path: &str,
    actual: &str,
    expected: &Value,
    key: &str,
) {
    comparison.exact(
        path,
        &actual.to_owned(),
        &expected
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned(),
    );
}
