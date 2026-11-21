// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The value a form field is compared with to decide whether it is "Modified".
//!
//! A field is modified when it differs from the aircraft the session started
//! from: the loaded preset's own value, not the schema's generic default. A
//! reference aircraft such as the AVE declares its own cabin, so comparing it
//! with the schema default marked every cabin field "Modified" right after the
//! preset was loaded. The host publishes the starting design once per frame
//! ([`publish`]) and the page that draws a configuration group names it
//! ([`with_group`]); a form drawn without both falls back to the schema default.

use std::cell::RefCell;

use serde_json::Value;

thread_local! {
    /// The key the published values were built for, and the values.
    static BASELINE: RefCell<Option<(String, Value)>> = const { RefCell::new(None) };
    /// The configuration group the form being drawn belongs to.
    static GROUP: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Publish the starting design's values for `key`; `build` runs only when the
/// key changed since the last frame. `None` from `build` (or no preset) clears
/// the baseline, so every form compares with the schema default.
pub(crate) fn publish(key: &str, build: impl FnOnce() -> Option<Value>) {
    BASELINE.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.as_ref().is_some_and(|(built, _)| built == key) {
            return;
        }
        *slot = build().map(|values| (key.to_owned(), values));
    });
}

/// Draw `add` with forms attributed to the configuration group `group`.
pub(crate) fn with_group<R>(group: &str, add: impl FnOnce() -> R) -> R {
    let previous = GROUP.with(|slot| slot.replace(Some(group.to_owned())));
    let result = add();
    GROUP.with(|slot| *slot.borrow_mut() = previous);
    result
}

/// The starting design's value of the field named `name` below the form path
/// `id_prefix` (the renderers' `::wing::root_airfoil` form without the group),
/// when one is published for the group being drawn.
pub(super) fn value_of(id_prefix: &str, name: &str) -> Option<Value> {
    let group = GROUP.with(|slot| slot.borrow().clone())?;
    let mut path = group;
    for segment in id_prefix.split("::").filter(|segment| !segment.is_empty()) {
        path.push('.');
        path.push_str(segment);
    }
    path.push('.');
    path.push_str(name);
    BASELINE.with(|slot| {
        let slot = slot.borrow();
        let (_, values) = slot.as_ref()?;
        path.split('.')
            .try_fold(values, |value, segment| value.get(segment))
            .cloned()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_published_value_is_found_by_group_and_form_path() {
        publish("test-key", || {
            Some(json!({"cabin": {"passenger": {"business": {"pitch_m": 1.9}}}}))
        });
        assert_eq!(
            value_of("::passenger::business", "pitch_m"),
            None,
            "no group yet"
        );
        with_group("cabin", || {
            assert_eq!(
                value_of("::passenger::business", "pitch_m"),
                Some(json!(1.9))
            );
            assert_eq!(value_of("::passenger::business", "missing"), None);
        });
        publish("test-key-2", || None);
        with_group("cabin", || {
            assert_eq!(value_of("::passenger::business", "pitch_m"), None);
        });
    }
}
