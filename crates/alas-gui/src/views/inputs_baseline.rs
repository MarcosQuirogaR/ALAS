// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The value a reference aircraft ships with, for the fields whose schema
//! default is not what its presets set.
//!
//! The "Modified" marker and the reset arrow compare a field with its schema
//! default. A reference aircraft whose cabin scheme is a named airline cabin
//! differs from the schema's `Custom` the moment it loads, which marked a fresh
//! preset as modified. The cabin scheme is therefore compared with the loaded
//! preset's own value.

use alas_config::{Entry, Field};
use serde_json::Value;

use crate::state::AppState;

/// Point the `cabin_preset` field's default at the loaded preset's own cabin
/// scheme, so a freshly loaded preset is not marked modified and reset returns
/// to the preset's cabin.
pub(super) fn use_loaded_cabin_preset(state: &AppState, ctx: &egui::Context, fields: &mut [Field]) {
    let Some(field) = fields.iter_mut().find(|field| field.name == "cabin_preset") else {
        return;
    };
    let Entry::Leaf(leaf) = &mut field.entry else {
        return;
    };
    if let Some(scheme) = preset_cabin_scheme(ctx, &state.active_preset) {
        leaf.value = Value::String(scheme);
    }
}

/// The cabin scheme `preset` sets, built once per preset.
fn preset_cabin_scheme(ctx: &egui::Context, preset: &str) -> Option<String> {
    let id = egui::Id::new(("loaded_preset_cabin_scheme", preset));
    if let Some(cached) = ctx.data(|data| data.get_temp::<Option<String>>(id)) {
        return cached;
    }
    let scheme = alas_config::AlasConfig::from_value(&serde_json::json!({ "preset": preset }))
        .ok()
        .map(|config| config.requirements.cabin_preset);
    ctx.data_mut(|data| data.insert_temp(id, scheme.clone()));
    scheme
}

#[cfg(test)]
mod tests {
    use super::use_loaded_cabin_preset;
    use crate::state::AppState;

    /// A reference aircraft is not marked modified because its cabin scheme
    /// differs from the schema default.
    #[test]
    fn the_cabin_default_is_the_loaded_presets_scheme() {
        let mut state = AppState::default();
        state.load_preset("A340-300");
        let ctx = egui::Context::default();
        let mut fields = state
            .schema
            .field("requirements")
            .and_then(|field| match &field.entry {
                alas_config::Entry::Node(node) => Some(node.fields.clone()),
                _ => None,
            })
            .expect("requirements group");
        use_loaded_cabin_preset(&state, &ctx, &mut fields);
        let field = fields
            .iter()
            .find(|field| field.name == "cabin_preset")
            .expect("cabin preset field");
        let alas_config::Entry::Leaf(leaf) = &field.entry else {
            panic!("a leaf");
        };
        let loaded = state
            .config_values
            .pointer("/requirements/cabin_preset")
            .cloned()
            .expect("loaded cabin preset");
        assert_eq!(leaf.value, loaded);
    }
}
