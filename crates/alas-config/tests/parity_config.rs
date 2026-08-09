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
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            for (key, expected_value) in expected {
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
                if !expected.contains_key(key) {
                    comparison.exact(
                        &format!("{path}.{key}"),
                        &"present".to_owned(),
                        &"absent upstream".to_owned(),
                    );
                }
            }
        }
        (actual, expected) => {
            comparison.exact(path, actual, expected);
        }
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

    // A field the interface never shows is omitted by both sides, so the two
    // lists are directly comparable -- and a field one side hides and the
    // other does not shows up here as an ordering disagreement, which is
    // exactly what it is.
    let names: Vec<&str> = node.fields.iter().map(|field| field.name).collect();
    let expected_names: Vec<&str> = expected_fields
        .iter()
        .filter_map(|field| field.get("name").and_then(Value::as_str))
        .collect();
    comparison.exact(&format!("{label}: field order"), &names, &expected_names);
    if names != expected_names {
        return;
    }

    for (field, expected_field) in node.fields.iter().zip(expected_fields) {
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

fn compare_field(
    comparison: &mut Comparison,
    label: &str,
    path: &str,
    field: &Field,
    expected: &Value,
    declared: &BTreeMap<String, Declared>,
) {
    let declaration = declared.get(path);

    compare_string(
        comparison,
        &format!("{label}.label"),
        field.label,
        expected,
        "label",
    );
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
    if declaration.is_some_and(|entry| entry.help) {
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

fn compare_leaf(comparison: &mut Comparison, path: &str, leaf: &LeafField, expected: &Value) {
    comparison.exact(
        &format!("{path}.kind"),
        &serde_json::to_value(leaf.kind).unwrap(),
        expected.get("kind").unwrap_or(&Value::Null),
    );
    comparison.exact(
        &format!("{path}.value"),
        &leaf.value,
        expected.get("value").unwrap_or(&Value::Null),
    );

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
    compare_optional(
        comparison,
        path,
        "readonly_unless",
        to_value(leaf.readonly_unless),
        expected,
    );

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
