// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/settings.py (`_overlay_dataclass`).
// Reference: alas @ rust-port-baseline.

//! Laying a partial set of values over a complete one.
//!
//! A saved configuration file states what the user changed, not everything
//! there is; loading one means starting from the defaults and overlaying it.
//! The overlay is recursive, so a file naming one field of one nested group
//! leaves that group's other fields alone rather than resetting them to their
//! defaults, which is what a shallow merge would do and what would make a
//! saved file's meaning depend on the version that wrote it.
//!
//! An unrecognized key is an error rather than something quietly dropped.
//! A configuration file with a misspelled key that loads successfully is a
//! run whose settings are not the ones its author wrote down, and nothing
//! later can detect that.

use serde::de::DeserializeOwned;
use serde::Serialize;

/// Why a partial set of values could not be laid over a configuration.
#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    /// The configuration could not be represented as a tree of values, or the
    /// overlaid result could not be read back into it. The latter carries the
    /// unrecognized key, or the type mismatch, that caused it.
    #[error("cannot overlay onto {type_name}: {source}")]
    Rejected {
        /// The configuration type being overlaid.
        type_name: &'static str,
        /// What serde said.
        source: serde_json::Error,
    },
    /// The overlay was not a mapping of field names to values.
    #[error("expected a mapping of {type_name} field names to values, got {found}")]
    NotAMapping {
        /// The configuration type being overlaid.
        type_name: &'static str,
        /// What was supplied instead.
        found: &'static str,
    },
}

/// `base` with `data` laid over it.
///
/// `data` names a subset of `base`'s fields, to any depth. Every field it
/// does not name keeps the value it had.
///
/// # Errors
///
/// Returns [`OverlayError`] when `data` is not a mapping, names a field the
/// configuration does not have, or gives one a value of the wrong type.
pub fn overlay<T>(base: &T, data: &serde_json::Value) -> Result<T, OverlayError>
where
    T: Serialize + DeserializeOwned,
{
    let type_name = std::any::type_name::<T>();
    if !data.is_object() {
        return Err(OverlayError::NotAMapping {
            type_name,
            found: describe(data),
        });
    }

    let mut merged = serde_json::to_value(base)
        .map_err(|source| OverlayError::Rejected { type_name, source })?;
    merge(&mut merged, data);
    serde_json::from_value(merged).map_err(|source| OverlayError::Rejected { type_name, source })
}

/// Recursively lay `patch` over `base`.
///
/// Two mappings merge key by key; anything else replaces outright. A list is
/// replaced rather than merged because a list's meaning is positional: an
/// element-wise merge of a shorter list would leave a tail from the defaults
/// that the file's author never wrote.
fn merge(base: &mut serde_json::Value, patch: &serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(base), serde_json::Value::Object(patch)) => {
            for (key, value) in patch {
                match base.get_mut(key) {
                    Some(existing) => merge(existing, value),
                    None => {
                        // Left for the deserializer to reject by name, which
                        // reports the field it did not recognize and what it
                        // expected instead.
                        base.insert(key.clone(), value.clone());
                    }
                }
            }
        }
        (base, patch) => *base = patch.clone(),
    }
}

fn describe(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "a list",
        serde_json::Value::Object(_) => "a mapping",
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Inner {
        a: f64,
        b: f64,
    }

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Outer {
        name: String,
        inner: Inner,
        list: Vec<f64>,
    }

    fn base() -> Outer {
        Outer {
            name: "default".to_owned(),
            inner: Inner { a: 1.0, b: 2.0 },
            list: vec![1.0, 2.0, 3.0],
        }
    }

    #[test]
    fn a_named_field_changes_and_nothing_else_does() {
        let result = overlay(&base(), &json!({"name": "changed"})).unwrap();
        assert_eq!(result.name, "changed");
        assert_eq!(result.inner, base().inner);
        assert_eq!(result.list, base().list);
    }

    #[test]
    fn one_field_of_a_nested_group_leaves_its_siblings_alone() {
        // A shallow merge would reset `b` to whatever the partial file did
        // not say, which is the whole reason the overlay recurses.
        let result = overlay(&base(), &json!({"inner": {"a": 9.0}})).unwrap();
        assert_eq!(result.inner, Inner { a: 9.0, b: 2.0 });
    }

    #[test]
    fn a_list_is_replaced_rather_than_merged_element_by_element() {
        let result = overlay(&base(), &json!({"list": [7.0]})).unwrap();
        assert_eq!(result.list, vec![7.0]);
    }

    #[test]
    fn an_unrecognized_key_is_an_error_not_a_silent_drop() {
        let error = overlay(&base(), &json!({"nmae": "typo"})).unwrap_err();
        assert!(
            format!("{error}").contains("nmae"),
            "the error should name the key it did not recognize: {error}"
        );
    }

    #[test]
    fn an_unrecognized_key_inside_a_nested_group_is_also_an_error() {
        let error = overlay(&base(), &json!({"inner": {"c": 1.0}})).unwrap_err();
        assert!(format!("{error}").contains('c'), "{error}");
    }

    #[test]
    fn a_value_of_the_wrong_type_is_an_error() {
        assert!(overlay(&base(), &json!({"inner": {"a": "text"}})).is_err());
    }

    #[test]
    fn an_overlay_that_is_not_a_mapping_says_so() {
        let error = overlay(&base(), &json!([1, 2, 3])).unwrap_err();
        assert!(format!("{error}").contains("a list"), "{error}");
    }

    #[test]
    fn an_empty_overlay_changes_nothing() {
        assert_eq!(overlay(&base(), &json!({})).unwrap(), base());
    }
}
