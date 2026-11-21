// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The configuration as the generic form editor reads it, split out of
//! `config_edit.rs` to keep that module inside its size budget.

use alas_config::{presets, AlasConfig, FuelPolicyConfig, FuelTankLayoutConfig};

use crate::sandbox::StartingDesign;
use crate::state::AppState;
use serde_json::Value;

/// Serialize a configuration for the generic form editor, which edits this
/// JSON directly and needs every schema field present to find it back.
///
/// `fuel_policy` and `fuel_tanks` skip serialization when they equal their
/// default, so a fresh or reloaded configuration, which starts at that
/// default, would otherwise be missing both keys entirely. The stated
/// take-off and landing field lengths (`toda_override_m`, `lda_override_m`) skip it
/// while unset, and the form needs the keys too.
pub(crate) fn full_config_values(config: &AlasConfig) -> Value {
    let mut value = serde_json::to_value(config).unwrap_or(Value::Null);
    if let Some(map) = value.as_object_mut() {
        map.entry("fuel_policy").or_insert_with(|| {
            serde_json::to_value(FuelPolicyConfig::default()).unwrap_or(Value::Null)
        });
        map.entry("fuel_tanks").or_insert_with(|| {
            serde_json::to_value(FuelTankLayoutConfig::default()).unwrap_or(Value::Null)
        });
        map.entry("toda_override_m").or_insert(Value::Null);
        map.entry("lda_override_m").or_insert(Value::Null);
    }
    value
}

impl AppState {
    /// The configuration a page reset restores: the active preset's for a
    /// preset aircraft, the clean-sheet defaults otherwise. It reads
    /// [`Self::active_preset`], the same state that decides
    /// [`Self::starting_design`], not the `preset` key of `config_values`,
    /// which can still name the aircraft loaded before a clean sheet.
    pub fn starting_design_defaults(&self) -> AlasConfig {
        if self.starting_design() == StartingDesign::PresetAircraft
            && presets::get(&self.active_preset).is_ok()
        {
            if let Ok(config) = alas_mission::preset_load::load_document(
                &serde_json::json!({"preset": self.active_preset}),
            )
            .map(|(config, _)| config)
            {
                return config;
            }
        }
        AlasConfig::default()
    }
}

impl AppState {
    /// Publish the values forms compare with for the "Modified" marker: the
    /// design as the active preset was loaded, else a regenerated copy of the
    /// preset, else nothing (clean sheet: the schema default).
    pub(crate) fn publish_form_baseline(&self) {
        let loaded = self
            .loaded_baseline
            .as_ref()
            .filter(|(name, _, _)| *name == self.active_preset);
        let key = format!(
            "{:?}|{}|{}",
            self.starting_design(),
            self.active_preset,
            loaded.map_or(u64::MAX, |(_, serial, _)| *serial)
        );
        crate::views::form::publish_baseline(&key, || {
            if self.starting_design() != StartingDesign::PresetAircraft {
                return None;
            }
            Some(loaded.map_or_else(
                || full_config_values(&self.starting_design_defaults()),
                |(_, _, values)| values.clone(),
            ))
        });
    }
}
