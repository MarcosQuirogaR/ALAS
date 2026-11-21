// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Schema visibility conditions: a field declared
//! `visible_if(path = "...", equals = "...")` is drawn only while the value at
//! that dotted path of the configuration is one of the listed strings (a
//! switch compares as `"true"` or `"false"`).
//!
//! A form is handed the values of one configuration group, while a condition
//! names a path from the configuration root, so the host publishes the current
//! value of every controlling path once per frame ([`publish`]) and the field
//! renderer asks [`is_visible`]. A form that never had a context published,
//! such as a stand-alone one in a test, shows every field.

use std::cell::RefCell;
use std::collections::HashMap;

use alas_config::{Entry, Field, Node};
use serde_json::Value;

thread_local! {
    /// Controlling path to its current string value, `None` when the path
    /// holds no string. Absent entries mean no context was published.
    static CONTEXT: RefCell<HashMap<&'static str, Option<String>>> =
        RefCell::new(HashMap::new());
}

/// Publish the current value of every path a `visible_if` in `schema` names,
/// read from the configuration values `config`.
pub(crate) fn publish(schema: &Node, config: &Value) {
    CONTEXT.with(|context| {
        let mut context = context.borrow_mut();
        context.clear();
        collect(&schema.fields, config, &mut context);
    });
}

fn collect(fields: &[Field], config: &Value, out: &mut HashMap<&'static str, Option<String>>) {
    for field in fields {
        if let Some(condition) = field.visible_if {
            out.entry(condition.path)
                .or_insert_with(|| value_at_path(config, condition.path).and_then(condition_text));
        }
        if let Entry::Node(node) = &field.entry {
            collect(&node.fields, config, out);
        }
    }
}

/// The published string value at the dotted `path`, when a context was
/// published and the path holds a string.
pub(crate) fn published_value(path: &str) -> Option<String> {
    CONTEXT.with(|context| context.borrow().get(path).cloned().flatten())
}

/// Whether `field` is shown. Its condition is looked up in the published
/// context, then in the form's own values and their ancestors; with no value
/// source at all the field stays visible.
pub(super) fn is_visible(field: &Field, values: &Value, ancestors: &[&Value]) -> bool {
    let Some(condition) = field.visible_if else {
        return true;
    };
    let published = CONTEXT.with(|context| context.borrow().get(condition.path).cloned());
    let current = match published {
        Some(current) => current,
        None => {
            let found = std::iter::once(values)
                .chain(ancestors.iter().copied())
                .find_map(|level| value_at_path(level, condition.path));
            match found {
                Some(found) => condition_text(found),
                None => return true,
            }
        }
    };
    current.is_some_and(|current| condition.any_of.contains(&current.as_str()))
}

/// The text a condition compares: a string as is, a switch as `"true"` or
/// `"false"`; other values match no condition.
pub(crate) fn condition_text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_bool().map(|on| on.to_string()))
}

fn value_at_path<'a>(values: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(values, |current, segment| current.get(segment))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{AlasConfig, ConfigNode};
    use serde_json::json;

    fn propeller_field() -> Field {
        let schema = AlasConfig::default().schema();
        let Some(Entry::Node(performance)) = schema.field("performance").map(|f| f.entry.clone())
        else {
            panic!("performance group");
        };
        performance
            .fields
            .iter()
            .find(|field| field.visible_if.is_some())
            .expect("a propeller field declares visible_if")
            .clone()
    }

    #[test]
    fn propeller_settings_follow_the_propulsion_technology() {
        let schema = AlasConfig::default().schema();
        let field = propeller_field();
        let mut config = json!({"geometry": {"engine": {"propulsion_technology": "turboprop"}}});
        publish(&schema, &config);
        assert!(is_visible(&field, &json!({}), &[]));
        config["geometry"]["engine"]["propulsion_technology"] = json!("turbofan");
        publish(&schema, &config);
        assert!(!is_visible(&field, &json!({}), &[]));
        config["geometry"]["engine"] = json!({});
        publish(&schema, &config);
        assert!(!is_visible(&field, &json!({}), &[]));
    }

    /// The center spar's chord position shows only while that spar is on.
    #[test]
    fn a_field_can_follow_a_switch() {
        let schema = AlasConfig::default().schema();
        let Some(Entry::Node(structures)) = schema.field("structures").map(|f| f.entry.clone())
        else {
            panic!("structures group");
        };
        let field = structures
            .fields
            .iter()
            .find(|field| field.name == "center_spar_chord_fraction")
            .expect("center spar chord position")
            .clone();
        let mut config = json!({"structures": {"center_spar_enabled": true}});
        publish(&schema, &config);
        assert!(is_visible(&field, &json!({}), &[]));
        config["structures"]["center_spar_enabled"] = json!(false);
        publish(&schema, &config);
        assert!(!is_visible(&field, &json!({}), &[]));
        // Without a published context the form's own values decide.
        CONTEXT.with(|context| context.borrow_mut().clear());
        let root = json!({"structures": {"center_spar_enabled": true}});
        assert!(is_visible(&field, &root["structures"], &[&root]));
    }

    #[test]
    fn a_form_with_no_value_source_shows_every_field() {
        CONTEXT.with(|context| context.borrow_mut().clear());
        assert!(is_visible(&propeller_field(), &json!({}), &[]));
    }
}
