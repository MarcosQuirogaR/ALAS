// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reset only the leaves owned by the Optimizer settings page.

use alas_config::{Entry, Field};
use serde_json::Value;

use crate::state::{AppState, LogKind};
use crate::{
    nav::Page,
    views::{tr, tr_fields},
};

/// Restore the defaults of the leaves this page shows. The MTOW controls the
/// Inputs card owns, the Design Space bounds and the retired hidden settings
/// keep their current values.
pub(super) fn reset(state: &mut AppState, page: &Page) {
    let Some(Entry::Node(node)) = state.schema.field("optimizer").map(|field| &field.entry) else {
        return;
    };
    let fields = super::placement::visible_fields(page, "optimizer", &node.fields);
    let Ok(defaults) = serde_json::to_value(alas_config::AlasConfig::default()) else {
        return;
    };
    if let Some(values) = state.group_mut("optimizer") {
        reset_leaves(&fields, values, &defaults["optimizer"]);
    }
    state.log(
        tr_fields(
            "Reset {group} settings to defaults.",
            &[("group", tr(page.title))],
        ),
        LogKind::Info,
    );
    state.on_config_modified();
}

fn reset_leaves(fields: &[Field], values: &mut Value, defaults: &Value) {
    for field in fields {
        match &field.entry {
            Entry::Node(node) => {
                reset_leaves(&node.fields, &mut values[field.name], &defaults[field.name]);
            }
            Entry::Leaf(_) => {
                if let Some(default) = defaults.get(field.name) {
                    values[field.name] = default.clone();
                } else if let Some(object) = values.as_object_mut() {
                    // An optional field whose default is unset is omitted
                    // from the serialized defaults.
                    object.remove(field.name);
                }
            }
        }
    }
}
