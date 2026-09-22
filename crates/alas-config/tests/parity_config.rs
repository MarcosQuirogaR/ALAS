// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Compares every configuration struct against the dataclass it was ported
//! from: what a freshly constructed one holds, and what it tells the settings
//! interface about each of its fields.
//!
//! Both halves are compared at `exact`. A default is copied, not computed, so
//! any difference at all is a transposed digit, and a transposed digit here
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
//! * `n_modes` uses the validated 16-mode product default for the live
//!   NASTRAN-95 path; the frozen Python reference remains at 30 so its
//!   historical parity fixture stays reproducible.
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
//! * `MassModelConfig` now carries the versioned `mass_architecture` and
//!   `flops_transport` product inputs. They are native additions with their
//!   own migration and source-evidence tests; the frozen comparison remains
//!   focused on the fields inherited from the Python configuration.
//! * The serialized Torenbeek/fraction fields are still visible for the
//!   explicit compatibility comparison. Their product help now says so; the
//!   frozen help remains checked as the historical text.
//! * `passenger_mass_kg`'s help now states the passenger-mass authority
//!   decision: every product path prices a seated passenger of any class at
//!   this combined mass, deriving the occupant share from the checked-bag
//!   mass. Both the corrected and frozen prose are pinned below; the value
//!   and every other field are unchanged.
//! * `cabin_preset`'s help now names each passenger preset's descriptive,
//!   airline-independent GUI label (`alas-gui/src/views/form_options.rs`)
//!   instead of the historical airline name. The serialized identifiers
//!   ('Ryanair', 'Iberia', 'Emirates') and every preset value are unchanged;
//!   both the corrected and frozen prose are pinned below.
//! * `PropulsionCycleConfig` now carries the two conceptual free-turbine
//!   design inputs `turboprop_overall_pressure_ratio` and
//!   `turboprop_turbine_inlet_temperature_k`. The frozen Python cycle is
//!   turbofan-only and has neither, so they are native additions whose
//!   defaults and migration are pinned by
//!   `propulsion::tests::older_cycle_configs_receive_explicit_turboprop_design_defaults`.
//! * The three structural-screening requirements (`ultimate_load_factor`,
//!   `dive_speed_m_s`, `limit_load_factor_neg`) keep their frozen values and
//!   names, while their help no longer states a certification result the
//!   project has not established. Both the corrected and the frozen prose are
//!   pinned below; no value, bound or schema entry changed.
//! * The three cargo-deck fields (`main_deck_uld`, `lower_deck_uld`,
//!   `loading_strategy`) offer an option list the frozen Python schema did
//!   not. The list membership is owned by `cabin::cargo`'s own schema test;
//!   here both the frozen absence and the product list are pinned.
//! * The four vortex-lattice mesh resolutions diverge in value and in help.
//!   The frozen Python loop mesh is one chordwise panel, which samples the
//!   mean camber line only where it is zero and so makes every section a
//!   flat plate; the measurements behind the product values, cross-checked
//!   against AeroSandbox on identical geometry, are in
//!   `.agent/reports/2026-09-11-vlm-resolution-sensitivity.html` and
//!   summarized in `alas_config::analysis`'s module doc. Both the frozen
//!   and the corrected value are pinned below.
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
        // airports; what this entry pins down is the composition, which
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

/// The two free-turbine design inputs are absent from the frozen Python
/// cycle, so the comparisons above skip them rather than report every saved
/// file as a parity drift. Their defaults are pinned here instead, on both
/// sides, so "native addition" does not quietly become "unchecked value":
/// `propulsion`'s own migration test proves an older file receives them and
/// that they reach the form with a unit, but not what they are worth.
///
/// SI: the pressure ratio is dimensionless, the turbine inlet temperature is
/// a total temperature in kelvin. Both are conceptual design-cycle
/// assumptions, not certified engine data, which is what their help says.
#[test]
fn the_native_turboprop_design_inputs_are_absent_upstream_and_pinned_here() {
    let fixture = fixture();
    let mut comparison = Comparison::new("alas-config turboprop design cycle", Tier::Exact);

    let defaults = serde_json::to_value(PropulsionCycleConfig::default()).unwrap();
    for (key, expected) in [
        ("turboprop_overall_pressure_ratio", serde_json::json!(15.0)),
        (
            "turboprop_turbine_inlet_temperature_k",
            serde_json::json!(1400.0),
        ),
    ] {
        for type_key in ["PropulsionCycleConfig", "ALASConfig"] {
            let frozen = &fixture.types.get(type_key).unwrap().defaults;
            let frozen = if type_key == "ALASConfig" {
                frozen.get("propulsion_cycle").unwrap()
            } else {
                frozen
            };
            comparison.exact(
                &format!("{type_key}.{key}: frozen Python field absent"),
                &frozen.get(key).is_some(),
                &false,
            );
        }
        comparison.exact(
            &format!("PropulsionCycleConfig.{key}: product default"),
            defaults.get(key).unwrap_or(&Value::Null),
            &expected,
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
    if let Some((upstream, corrected)) = product_default_correction(path) {
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

/// Product defaults that intentionally differ from the frozen Python
/// configuration. The reference values remain checked explicitly so a product
/// optimization cannot silently become a parity drift.
fn product_default_correction(path: &str) -> Option<(Value, Value)> {
    if path.ends_with(".freq_sweep_max_hz") {
        Some((serde_json::json!(500.0), serde_json::json!(60.0)))
    } else if path.ends_with(".n_modes") {
        Some((serde_json::json!(30), serde_json::json!(16)))
    }
    // The vortex-lattice mesh. Upstream evaluates the optimizer loop at one
    // chordwise panel, which samples the mean camber line only at the leading
    // and trailing edges (where it is zero) so every section is a flat
    // plate and the search cannot see camber at all. Measured over four
    // presets and cross-checked against AeroSandbox 4.2.8 on identical
    // geometry (`.agent/reports/2026-09-11-vlm-resolution-sensitivity.html`):
    // that costs 1.1-4.1 deg of cruise attitude, -14.4 to +2.9 % of L/D, and
    // it mis-ranks neighbouring candidates (Spearman 0.77). Eight panels rank
    // them exactly. The spanwise fields move the other way: the builder has
    // already subdivided each surface and that is converged, so upstream's
    // fine value of two only doubles the panel count. The frozen values stay
    // pinned here so the divergence remains a recorded decision.
    else if path.ends_with(".chordwise_resolution") {
        Some((serde_json::json!(1), serde_json::json!(8)))
    } else if path.ends_with(".fine_chordwise_resolution") {
        Some((serde_json::json!(8), serde_json::json!(16)))
    } else if path.ends_with(".fine_spanwise_resolution") {
        Some((serde_json::json!(2), serde_json::json!(1)))
    }
    // The wing spanwise panel count, which changed meaning rather than
    // fidelity: the frozen value is a per-section multiplier, the product one
    // an absolute panel count across the semispan, and 24 is what the frozen
    // three-section planform already meshed to. See
    // `alas_geom::aircraft::spanwise`.
    else if path.ends_with(".wing.n_subdivisions") || path == "WingConfig.n_subdivisions" {
        Some((serde_json::json!(8), serde_json::json!(24)))
    }
    // The native worker count. The frozen value is a literal one; the product
    // default is `0`, meaning "resolve against this machine", which the staged
    // MADS search uses to evaluate a poll block in parallel (measured 2.24x on
    // the B787-9 and 2.76x on AVE at eight workers, with the evaluation count
    // and the winner unchanged). Differential evolution is deliberately *not*
    // covered by that: its generation loop batches only when a configuration
    // explicitly asks for more than one worker, because a batched generation
    // defers the population update and is a different algorithm from the
    // serial one. See `SolverSettings::resolved_workers` and the guard in
    // `alas_opt::DesignOptimizer::run_search`.
    else if path.ends_with(".optimizer.solver.workers")
        || path == "OptimizerConfig.solver.workers"
    {
        Some((serde_json::json!(1), serde_json::json!(0)))
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
    // lists are directly comparable, and a field one side hides and the
    // other does not shows up here as an ordering disagreement, which is
    // exactly what it is.
    let fields: Vec<&Field> = node
        .fields
        .iter()
        .filter(|field| {
            !is_native_config_field(node.type_name, field.name)
                && !is_transport_planform_field_name(node.type_name, field.name)
                && (node.type_name != "MassModelConfig"
                    || !matches!(
                        field.name,
                        "schema_version"
                            | "mass_architecture"
                            | "systems_mass_method"
                            | "flops_transport"
                            | "structural_mass_method"
                            | "propulsion_mass_method"
                            | "flops_structure"
                            | "flops_turboprop"
                            | "geometric_component_stations"
                    ))
        })
        .collect();
    let names: Vec<&str> = fields.iter().map(|field| field.name).collect();
    let expected_fields: Vec<&Value> = expected_fields
        .iter()
        .filter(|field| {
            let name = field.get("name").and_then(Value::as_str);
            !matches!(name, Some("suave_venv_dir" | "suave_runner_dir"))
                && !(node.type_name == "EngineConfig" && name == Some("thrust_kn"))
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
    (matches!(key, "fuel_policy" | "fuel_tanks" | "downstream")
        && (path.ends_with("AlasConfig") || path.ends_with("ALASConfig") || path.is_empty()))
        // Source-backed landing-gear references and heterogeneous bogie
        // counts are native additions; the frozen Python schema predates
        // them. Their values are checked by landing-gear unit/config tests.
        || (matches!(
            key,
            "reference_wheelbase_m"
                | "reference_station_frame"
                | "reference_station_fuselage_length_m"
                | "reference_nlg_x_fraction"
                | "reference_mlg_x_fractions"
                | "reference_body_wheelbase_m"
                | "reference_track_m"
                | "mlg_strut_bogie_wheels"
        ) && (path.ends_with("LandingGearConfig") || path.ends_with(".landing_gear")))
        // Condition-specific OEI evidence fields are native additions; the
        // frozen Python schema predates them. Their optional/default
        // semantics are covered by the OEI assessment tests.
        || (matches!(
            key,
            "oei_condition_to_sls_thrust_ratio"
                | "oei_asymmetric_trim_cd"
                | "oei_windmilling_cd"
        ) && (path.ends_with("PerformanceConfig") || path.ends_with(".performance")))
        // The mission-sized objective, the design-space boundary, the
        // correlation validity domain and the D01-D03 relaxation policy are
        // native product additions; the frozen Python optimizer schema
        // predates all four. Their values, bounds and review are checked by
        // the optimizer config tests and by `optimizer::policy_review`.
        || (matches!(
            key,
            "objective" | "design_space" | "plausibility" | "relaxation"
        ) && (path.ends_with("OptimizerConfig") || path.ends_with(".optimizer")))
        // Native speed-reference switch for the climb/descent legs. Its
        // serialization skips the `TrueAirspeed` default, so a legacy file
        // and the frozen default tree round-trip unchanged.
        || (key == "climb_descent_speed_reference"
            && (path.ends_with("MissionProfileConfig") || path.ends_with(".profile")))
        || (matches!(
            key,
            "turbofan"
                | "turboprop"
                | "propulsion_technology"
                | "part_power_fuel_flow_ratios"
                | "part_power_source"
        ) && (path.ends_with("EngineConfig") || path.ends_with(".engine")))
        || (key == "exclude_buried_main_wing_area"
            && (path.ends_with("DragModelConfig") || path.ends_with(".drag_model")))
        || (matches!(
            key,
            "use_airway_endpoint_coordinates" | "max_airway_stretch"
        ) && (path.ends_with("MissionConfig") || path.ends_with(".mission")))
        // The versioned pure-FLOPS architecture and its physical transport
        // inputs are native product additions. Their migration, schema and
        // source-evidence contracts are checked by mass-architecture and
        // preset-FLOPS tests rather than the frozen Python fixture.
        || (matches!(
            key,
            "schema_version" | "mass_architecture" | "flops_transport" | "flops_turboprop"
        )
            && (path.ends_with("MassModelConfig") || path.ends_with(".mass_model")))
        // The conceptual free-turbine design cycle. The frozen Python
        // propulsion configuration models a turbofan only and declares
        // neither field; their defaults and the migration that supplies them
        // to an older saved file are checked by
        // `older_cycle_configs_receive_explicit_turboprop_design_defaults`.
        || (matches!(
            key,
            "turboprop_overall_pressure_ratio" | "turboprop_turbine_inlet_temperature_k"
        ) && (path.ends_with("PropulsionCycleConfig")
            || path.ends_with(".propulsion_cycle")))
        || (matches!(
            key,
            "method" | "finite_difference_step" | "constraint_tolerance"
        ) && (path.ends_with("SolverSettings") || path.ends_with(".solver")))
        || (key == "random_force_psd_n2_per_hz"
            && (path.ends_with("StructuresConfig") || path.ends_with(".structures")))
        // The cargo capacity objective (clarified ledger App Features 2,
        // decision D10) is a native product addition; the frozen Python
        // requirements schema predates it. Its default, valid domain,
        // round-trip and schema entry are checked by the
        // `DesignRequirements` unit tests.
        || (matches!(key, "optimize_passenger_capacity" | "cargo_objective_kg")
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

    if label.ends_with(".fuel_volume_penalty_scale") {
        assert_eq!(
            expected["label"],
            "Insufficient wing fuel-volume penalty weight"
        );
        assert_eq!(field.label, "Legacy fuel-volume penalty weight (unused)");
    } else if label.ends_with(".share_pct") {
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
    } else if let Some((source_label, frozen_label, _, _)) = oei_documentation_correction(label) {
        comparison.exact(
            &format!("{label}.label: source-corrected Rust value"),
            &field.label,
            &source_label,
        );
        comparison.exact(
            &format!("{label}.label: frozen Python value"),
            &expected.get("label").and_then(Value::as_str).unwrap_or(""),
            &frozen_label,
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
    if label.ends_with(".fuel_volume_penalty_scale") {
        assert_eq!(field.help, "Deprecated compatibility field. MTOW minus zero-fuel mass is a mass allowance, not mission-required fuel, so it is no longer used by the optimizer. Tank capacity will be constrained against mission fuel plus the selected reserve policy.");
        assert_eq!(expected["help"], "Penalizes the wing's physical usable fuel-tank volume (physics.performance.wing_fuel_volume_m3, Torenbeek geometric estimate) being too small to hold the fuel mass the weight & balance analysis says this design actually needs: a wing that's too thin/small/tapered to carry its own required fuel is not a buildable aircraft, independent of whether the MTOW fuel-mass budget itself closes. Quadratic on the fractional shortfall (required_fuel - tank_capacity) / required_fuel.");
    } else if label.ends_with(".share_pct") {
        comparison.exact(
            &format!("{label}.help: product seat-share semantics"),
            &field.help,
            &"Target percentage of passengers in this class. The selected preset supplies the seat geometry; the layout converts the normalized mix into rows that fit the usable, regulation-compliant cabin. Only Custom exposes this value for editing.",
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
    } else if let Some((source_corrected, frozen_python)) = mass_legacy_help_correction(label) {
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
    } else if let Some((source_corrected, frozen_python)) =
        passenger_mass_authority_help_correction(label)
    {
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
    } else if let Some((source_corrected, frozen_python)) =
        structural_screening_help_correction(label)
    {
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
    } else if let Some((source_corrected, frozen_python)) = cabin_preset_help_correction(label) {
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
    } else if let Some((_, _, source_help, frozen_help)) = oei_documentation_correction(label) {
        comparison.exact(
            &format!("{label}.help: source-corrected Rust value"),
            &field.help,
            &source_help,
        );
        comparison.exact(
            &format!("{label}.help: frozen Python value"),
            &expected.get("help").and_then(Value::as_str).unwrap_or(""),
            &frozen_help,
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

/// The Torenbeek/fraction controls remain serialized for the explicit
/// compatibility comparison, but product help must not suggest that they own
/// pure FLOPS production mass. Keep both the corrected and frozen prose
/// visible in this parity test.
fn mass_legacy_help_correction(label: &str) -> Option<(&'static str, &'static str)> {
    match label {
        label if label.ends_with(".suspended_mass_fraction") => Some((
            "Legacy comparison only: fraction of MTOW treated as 'suspended' mass in the Torenbeek wing structural formula (everything the wing structure must carry other than itself). Typical commercial transport: 0.70-0.78.",
            "Fraction of MTOW treated as 'suspended' mass in the Torenbeek wing structural formula (everything the wing structure must carry other than itself). Typical commercial transport: 0.70-0.78.",
        )),
        label if label.ends_with(".max_airspeed_for_flaps_ms") => Some((
            "Legacy comparison only: design airspeed with flaps extended, fed into the Torenbeek wing-mass formula.",
            "Design airspeed with flaps extended, fed into the Torenbeek wing-mass formula.",
        )),
        label if label.ends_with(".flap_deflection_angle_deg") => Some((
            "Legacy comparison only: maximum flap deflection angle, fed into the Torenbeek wing-mass formula.",
            "Maximum flap deflection angle, fed into the Torenbeek wing-mass formula.",
        )),
        label if label.ends_with(".landing_gear_mass_fraction") => Some((
            "Legacy comparison only: landing gear mass as a fraction of MTOW. Raymer Table 15.2: ~4% for commercial jet transports.",
            "Landing gear mass as a fraction of MTOW. Raymer Table 15.2: ~4% for commercial jet transports.",
        )),
        label if label.ends_with(".propulsion_twr_factor") => Some((
            "Legacy comparison only: dry engine mass is estimated as thrust / (this factor * g). Historical engine thrust-to-weight ratios are ~5-7, so this factor is typically ~6.",
            "Dry engine mass is estimated as thrust / (this factor * g). Historical engine thrust-to-weight ratios are ~5-7, so this factor is typically ~6.",
        )),
        label if label.ends_with(".propulsion_installation_factor") => Some((
            "Legacy comparison only: multiplier on dry engine mass accounting for pylon, cowling, fire suppression and other installed accessories.",
            "Multiplier on dry engine mass accounting for pylon, cowling, fire suppression and other installed accessories.",
        )),
        label if label.ends_with(".propulsion_mass_fallback_fraction") => Some((
            "Legacy comparison only: fallback propulsion mass as a fraction of MTOW, used only if the selected engine isn't found in the database.",
            "Fallback propulsion mass as a fraction of MTOW, used only if the selected engine isn't found in the database.",
        )),
        label if label.ends_with(".systems_mass_fraction") => Some((
            "Legacy comparison only: avionics, electrical, ECS, APU, etc. as a fraction of MTOW. Raymer Table 15.2: 9-13% for commercial transports.",
            "Avionics, electrical, ECS, APU, etc. as a fraction of MTOW. Raymer Table 15.2: 9-13% for commercial transports.",
        )),
        label if label.ends_with(".furnishings_mass_fraction") => Some((
            "Legacy comparison only: passenger seats, galleys, lavatories, insulation, crew, paint, and operational empty items as a fraction of MTOW. Typically 10-14% for passenger transports.",
            "Passenger seats, galleys, lavatories, insulation, crew, paint, and operational empty items as a fraction of MTOW. Typically 10-14% for passenger transports.",
        )),
        _ => None,
    }
}

/// The native OEI fields retain the frozen names and defaults, while their
/// documentation was tightened to distinguish conceptual fallbacks and the
/// gear-up, high-lift configuration used by the evidence-aware helpers. Keep
/// both texts checked explicitly so this remains a documented migration rather
/// than silently dropping parity coverage.
fn oei_documentation_correction(
    label: &str,
) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    match label {
        "PerformanceConfig.oei_gradient" | "ALASConfig.performance.oei_gradient" => Some((
            "OEI 2nd-segment climb gradient (fallback)",
            "OEI 2nd-segment climb gradient (fallback)",
            "Conceptual fallback for an engine count outside the implemented 14 CFR 25.121(b) two/three/four-engine table. The matching chart auto-selects 0.024 (twin) / 0.027 (tri-jet) / 0.030 (quad) from the actual engine count; this fallback does not establish a Part 25 result for unsupported counts.",
            "FAR 25.121 minimum second-segment climb gradient with one engine inoperative (OEI). The matching chart auto-selects 0.024 (twin) / 0.027 (tri-jet) / 0.030 (quad) from the actual engine count; this value is only the fallback for any other engine count.",
        )),
        "PerformanceConfig.oei_climb_cl" | "ALASConfig.performance.oei_climb_cl" => Some((
            "OEI climb configuration CL",
            "OEI climb configuration CL",
            "Legacy constant lift coefficient for conceptual OEI second-segment climb L/D (Raymer Ch.17). A V2-based evaluation should derive CL from CLmax_TO and the selected V2/VSR or V2/VS ratio instead.",
            "Lift coefficient assumed in the take-off configuration when evaluating OEI second-segment climb L/D (Raymer Ch.17).",
        )),
        "PerformanceConfig.oei_climb_delta_cd"
        | "ALASConfig.performance.oei_climb_delta_cd" => Some((
            "OEI climb high-lift drag increment (gear up)",
            "OEI climb flap/gear drag increment",
            "Parasite-drag increment added to clean CD0 for the takeoff flap/slat configuration with landing gear retracted, as required by 14 CFR 25.121(b). Asymmetric trim/control and inoperative-engine or windmilling drag require separate source values; this field does not represent them.",
            "Parasite-drag increment added to the clean CD0 for the flap/gear-down OEI second-segment climb configuration.",
        )),
        _ => None,
    }
}

fn wing_centroid_help_correction(label: &str) -> Option<(&'static str, &'static str)> {
    if label == "StructuresConfig.enabled" || label.ends_with(".structures.enabled") {
        Some((
            "Size a generic wingbox (skin/spars/ribs) for the optimized design's main wing, write NASTRAN .bdf files, and compute theoretical (no-NASTRAN) deformations/stresses/frequencies as part of a normal Run, populating the Structural Analysis Results tab. This switch controls the downstream structural solve; the configured spars, materials, and gauges still define the main-wing mass centroid used by weight and balance, without replacing the Torenbeek total wing mass.",
            "Size a generic wingbox (skin/spars/ribs) for the optimized design's main wing, write NASTRAN .bdf files, and compute theoretical (no-NASTRAN) deformations/stresses/frequencies as part of a normal Run, populating the Structural Analysis Results tab. Does not affect the mass model, CG, or optimizer, purely a downstream analysis, like MSES/Propulsion Analysis.",
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
        "WingConfig.n_subdivisions"
        | "GeometryConfig.wing.n_subdivisions"
        | "ALASConfig.geometry.wing.n_subdivisions" => Some((
            "Spanwise panels across the whole wing semispan for the vortex-lattice solver. This is an absolute count, not a count per section: a planform with a side-of-body station and a kink gets the same mesh density as one without, and adding a station no longer changes the panel count underneath a search. Every planform station (root, side-of-body, kink, tip) is always kept as a panel edge whatever the count, so refining the mesh never averages a kink away. The default of 24 is converged: a twelve-fold refinement moves the trimmed cruise attitude by 0.01 deg.",
            "Spanwise panel refinement per wing section for the vortex-lattice solver. Higher = more accurate, slower.",
        )),
        "EmpennageConfig.n_subdivisions"
        | "GeometryConfig.empennage.n_subdivisions"
        | "ALASConfig.geometry.empennage.n_subdivisions" => Some((
            "Spanwise panels across each tail surface for the vortex-lattice solver, as an absolute count rather than a count per section. Both stabilizers are single-section surfaces, so this is the panel count they already had; it is stated absolutely so a cranked fin later gets the same density rather than twice it.",
            "Spanwise panel refinement per tail surface for the vortex-lattice solver.",
        )),
        "AnalysisConfig.spanwise_resolution"
        | "ALASConfig.analysis.spanwise_resolution" => Some((
            "Multiplier on each surface's built-in spanwise panel subdivision for the vortex-lattice solver. Leave at 1: the geometry builder has already subdivided every surface (24 strips per semispan on the main wing), and that is converged, refining it further moves the trimmed cruise attitude by 0.01 deg. Values above 2 are rejected, because this multiplier re-applies a cosine spacing inside each existing strip and the induced drag then stops converging. Part of the Fidelity preset.",
            "Multiplier on each surface's built-in spanwise panel subdivision for the vortex-lattice solver. Higher = finer mesh, slower. Part of the Fidelity preset.",
        )),
        "AnalysisConfig.chordwise_resolution"
        | "ALASConfig.analysis.chordwise_resolution" => Some((
            "Number of chordwise panels per strip for the vortex-lattice solver, used by the fast in-loop estimate the optimizer ranks candidates with; the final reported analysis uses fine_chordwise_resolution instead. This is a literal panel count, not a multiplier, and nothing else in the pipeline sets one. At 1 the mesh samples the camber line only at the leading and trailing edges, where it is zero, so the section becomes a flat plate: cruise attitude comes out 1-4 deg high, L/D wrong by -14 to +3 percent, and the four airfoil bump design variables have no effect at all. 8 ranks candidates identically to a converged mesh. Higher = finer mesh, slower. Part of the Fidelity preset.",
            "Multiplier on each surface's built-in chordwise panel subdivision for the vortex-lattice solver. Higher = finer mesh, slower. Part of the Fidelity preset. Used by the fast in-loop estimate; the final reported analysis uses fine_chordwise_resolution instead.",
        )),
        "AnalysisConfig.fine_spanwise_resolution"
        | "ALASConfig.analysis.fine_spanwise_resolution" => Some((
            "Spanwise panel resolution used ONLY for the once-per-run final/reported analysis (drag polar, trimmed cruise point, neutral point), not the optimizer loop. Leave at 1 for the same reason as the in-loop field: the span is already converged, so raising this doubles the panel count to change the answer by about 1 percent. Spend the panels on fine_chordwise_resolution instead.",
            "Spanwise panel resolution used ONLY for the once-per-run final/reported analysis (drag polar, trimmed cruise point, neutral point), not the optimizer loop. Higher fidelity where speed doesn't matter.",
        )),
        "AnalysisConfig.fine_chordwise_resolution"
        | "ALASConfig.analysis.fine_chordwise_resolution" => Some((
            "Chordwise panel resolution for the once-per-run final/reported analysis. A supercritical section needs roughly 8 chordwise panels before the VLM resolves its camber line at all, and the convergence is first-order in panel count: 8 still leaves the reported cruise attitude about 1 deg high on a supercritical wing, 16 about 0.5 deg. Kept above the in-loop value so the REPORTED cruise alpha and L/D are the more trustworthy of the two, at a cost paid once per run.",
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
        // The corrected prose no longer says parallel evaluation is free,
        // because it is not. Differential evolution's generation loop batches
        // only when a configuration explicitly asks for more than one worker:
        // a batched generation defers the population update, so an accepted
        // trial stops influencing later trial vectors in the same generation,
        // which is a different algorithm with a different winner. Resolving
        // the automatic setting against the machine moved the frozen replay
        // off the reference interleaving and made its result core-count
        // dependent, which `seeded_example_replays_the_python_winner` catches.
        // The staged MADS search is the case where the count really does
        // change only the wall time, and the text now says so of that search
        // alone.
        "OptimizerConfig.solver.workers" | "ALASConfig.optimizer.solver.workers" => Some((
            "Number of native worker threads for candidate batches. 0 (the default) picks a count from the machine's available parallelism; a positive value is used exactly as written; negative values are treated as 1. For the staged MADS search this changes only how long a poll block takes, not which points are evaluated or the winner. Differential evolution is different: asking for more than one worker builds a whole generation before evaluating it, so an accepted trial no longer influences later trial vectors in the same generation, which is a different algorithm and a different result. External evaluator adapters remain serial because they own mutable process/session state.",
            "Number of worker processes for parallel evaluation (>1 uses multiprocessing). Requires a picklable objective, already the case for ALAS's optimizer.",
        )),
        _ => None,
    }
}

/// `passenger_mass_kg` documents the product passenger-mass authority
/// decision: every seated passenger, of any class, is priced at this
/// combined (occupant + checked bag) mass, and the per-class occupant slot
/// is the derived remainder. The prose also no longer presents the shipped
/// 100 kg as a standard: AC 120-27F is operator weight-and-balance guidance,
/// so the text names the number as a project load-case default and says what
/// must be recorded before another value is used operationally. Only the
/// prose changed; the value and every other field are unchanged, and both
/// texts stay pinned here.
fn passenger_mass_authority_help_correction(label: &str) -> Option<(&'static str, &'static str)> {
    match label {
        "DesignRequirements.passenger_mass_kg" | "ALASConfig.requirements.passenger_mass_kg" => {
            Some((
                "Combined average mass per occupant (body + baggage). The shipped 100 kg is a transparent project load-case default; FAA AC 120-27F is operator weight-and-balance guidance and does not establish a universal passenger mass. Record the operator, population, baggage method and date before using another value operationally. This remains the single load-case authority for report, GUI preview, pipeline, export and optimizer paths: cabin.passenger.checked_bag_mass_kg supplies the baggage share and the occupant slot the remainder.",
                "Combined average mass per occupant (body + baggage). FAA AC 120-27E standard is 100 kg; airlines may use 90-105 kg.",
            ))
        }
        _ => None,
    }
}

/// The three structural-screening requirements keep their frozen values,
/// names, bounds and schema entries. Only their prose changed: the frozen
/// text presented a shipped default as a certification result (an amendment
/// clause, a CS-25 paragraph, a derived VC relation), and the product text
/// names the same number as a screening input whose certification basis the
/// reader must establish. Both texts stay pinned so this remains a recorded
/// documentation decision rather than dropped parity coverage.
fn structural_screening_help_correction(label: &str) -> Option<(&'static str, &'static str)> {
    match label {
        "DesignRequirements.ultimate_load_factor"
        | "ALASConfig.requirements.ultimate_load_factor" => Some((
            "Structural screening input fed into the Torenbeek mass formulas. The shipped 3.75 is 1.5 x 2.5; verify the selected certification basis, amendment, aircraft category and load case before treating it as an airworthiness value.",
            "Limit load factor times the 1.5 safety margin, fed into the Torenbeek structural mass formulas.",
        )),
        "DesignRequirements.dive_speed_m_s" | "ALASConfig.requirements.dive_speed_m_s" => Some((
            "Structural screening dive speed, fed into the Torenbeek mass formulas and the V-n diagram. The project may derive VC as VD/1.25 for this study; verify speed type, altitude/Mach envelope, certification basis and amendment before treating that relation as an airworthiness result.",
            "Structural design dive speed, fed into the Torenbeek structural mass formulas. Also VD on the V-n diagram; design cruise speed VC is derived as VD/1.25 (CS-25.335(b) minimum margin) rather than a separate field.",
        )),
        "DesignRequirements.limit_load_factor_neg"
        | "ALASConfig.requirements.limit_load_factor_neg" => Some((
            "Negative V-n screening input. The shipped -1.0 follows the large-aeroplane CS-25 reference case up to VC; verify the selected certification basis, amendment, speed range and category before using it for qualification. The positive limit value is derived as ultimate_load_factor / 1.5.",
            "CS-25.337(c) negative limit load factor for the V-n diagram. The positive limit load factor is derived as ultimate_load_factor / 1.5 (CS-25.303) rather than a separate field.",
        )),
        _ => None,
    }
}

/// `cabin_preset`'s help now names each passenger preset's descriptive,
/// airline-independent GUI label (`alas-gui/src/views/form_options.rs::
/// cabin_preset_display_name`) instead of the historical airline name. The
/// serialized identifiers ('Ryanair', 'Iberia', 'Emirates') and every preset
/// value are unchanged; only this prose and the combo-box display text did.
fn cabin_preset_help_correction(label: &str) -> Option<(&'static str, &'static str)> {
    match label {
        "DesignRequirements.cabin_preset" | "ALASConfig.requirements.cabin_preset" => Some((
            "Named seating/payload layout preset ('High-density single-class', 'Two-class (Business/Economy)', 'Three-class (First/Business/Economy)' for passenger; 'Max payload', 'Dense payload' for cargo). 'Custom' lets you hand-edit the Cabin & Payload tab.",
            "Named seating/payload layout preset ('Ryanair', 'Iberia', 'Emirates' for passenger; 'Max payload', 'Dense payload' for cargo). 'Custom' lets you hand-edit the Cabin & Payload tab.",
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
    if let Some((upstream, corrected)) = product_default_correction(path) {
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
            // Reaching this arm is itself the statement that the product
            // field offers a list, so for the cargo-deck fields what is left
            // to check is the frozen side: upstream offered none.
            if product_cargo_option_field(path).is_some() {
                comparison.exact(
                    &format!("{path}: frozen Python field has no option list"),
                    &expected_options.is_some(),
                    &false,
                );
            } else {
                comparison.exact(
                    &format!("{path}: names an option list"),
                    &true,
                    &expected_options.is_some(),
                );
            }
            if let Some(resolved) = source.options() {
                if let Some(Some(product_list)) = product_cargo_option_field(path) {
                    comparison.exact(
                        &format!("{path}.options: product list"),
                        &serde_json::to_value(resolved).unwrap(),
                        &serde_json::to_value(product_list).unwrap(),
                    );
                } else {
                    comparison.exact(
                        &format!("{path}.options"),
                        &serde_json::to_value(resolved).unwrap(),
                        expected_options.unwrap_or(&Value::Null),
                    );
                }
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
            // A cargo-deck field that stopped offering its list would
            // otherwise agree with a frozen schema that never had one, so it
            // is required here rather than merely permitted.
            comparison.exact(
                &format!("{path}: names an option list"),
                &false,
                &(expected_options.is_some() || product_cargo_option_field(path).is_some()),
            );
        }
    }
}

/// The three cargo-deck fields that offer an option list the frozen Python
/// schema did not. The outer `Some` marks the field as one of them; the inner
/// option is the list where this crate can resolve it, and `None` where the
/// membership is owned by a crate above this one and checked there.
///
/// `cabin::cargo`'s own schema test pins which `OptionSource` each field
/// names; what is added here is that upstream offered nothing to compare it
/// with, so the divergence stays recorded instead of unchecked.
fn product_cargo_option_field(path: &str) -> Option<Option<&'static [&'static str]>> {
    if path.ends_with(".cargo.main_deck_uld") || path.ends_with(".cargo.lower_deck_uld") {
        Some(None)
    } else if path.ends_with(".cargo.loading_strategy") {
        Some(Some(&[
            "target_cg",
            "min_pallets",
            "door_proximity",
            "uniform",
        ]))
    } else {
        None
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
