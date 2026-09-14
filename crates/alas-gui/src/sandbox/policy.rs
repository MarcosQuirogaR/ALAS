// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Manual-edit protection of a registered preset's geometry.
//!
//! In the guided workspace a registered aircraft keeps its defining geometry:
//! the wing, empennage and fuselage definitions, the wetted-area factors and
//! the engine installation (count, spanwise placement, height and inlet
//! offset). The engine model itself remains a supported modelling choice, so
//! the engine name and the fields its catalogue entry derives are not
//! protected. The design vector's initial point is likewise held at the
//! preset's own values; the optimizer's bounded envelope around it is a
//! separate allowance and is left to the design-space contract.
//!
//! The policy is enforced where a value can change, not only where it is
//! shown: every configuration mutation and every run launch re-checks the
//! protected fields and restores them, so a loaded file, a detached
//! settings window or a hidden path cannot bypass it. Manual geometry
//! experimentation belongs in the sandbox.

use alas_config::{presets, GeometryConfig, DESIGN_VARIABLE_SPECS};

use crate::state::{AppState, LogKind};
use crate::views::tr;

/// The protected geometry: `reference` with `current`'s unprotected engine
/// model fields carried over.
pub fn protected_geometry(reference: &GeometryConfig, current: &GeometryConfig) -> GeometryConfig {
    let mut protected = reference.clone();
    let mut engine = current.engine.clone();
    engine.spanwise_positions_m = reference.engine.spanwise_positions_m.clone();
    engine.z_m = reference.engine.z_m;
    engine.inlet_x_offset_m = reference.engine.inlet_x_offset_m;
    protected.engine = engine;
    protected
}

impl AppState {
    /// Whether manual geometry edits are locked for the active case.
    pub fn manual_geometry_locked(&self) -> bool {
        !self.sandbox.active() && !self.active_preset.is_empty()
    }

    /// Restore protected preset geometry and design values after a
    /// mutation. Returns whether anything had to be restored.
    pub fn enforce_preset_geometry(&mut self) -> bool {
        if !self.manual_geometry_locked() {
            return false;
        }
        let Ok(preset) = presets::get(&self.active_preset) else {
            return false;
        };
        let Some(mut config) = self.typed_config() else {
            return false;
        };
        let mut reference = preset.geometry.clone();
        reference.engine.apply_engine_spec();
        let protected = protected_geometry(&reference, &config.geometry);
        let mut restored = false;
        if config.geometry != protected {
            config.geometry = protected;
            self.config_values = crate::config_edit::full_config_values(&config);
            restored = true;
        }
        if let Ok(design) = serde_json::to_value(preset.design_vector) {
            if let Some(map) = design.as_object() {
                for spec in DESIGN_VARIABLE_SPECS {
                    if let Some(value) = map.get(spec.name).and_then(serde_json::Value::as_f64) {
                        if self.design_values.get(spec.name).copied() != Some(value) {
                            self.design_values.insert(spec.name.to_owned(), value);
                            restored = true;
                        }
                    }
                }
            }
        }
        if restored {
            self.log(
                tr("Preset geometry is protected from manual edits; the registered values were restored. Use the sandbox for geometry experiments."),
                LogKind::Warn,
            );
        }
        restored
    }
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use serde_json::Value;

    #[test]
    fn a_geometry_edit_on_a_registered_preset_is_reverted_on_mutation() {
        let mut state = AppState::default();
        assert_eq!(state.active_preset, "AVE");
        assert!(state.manual_geometry_locked());
        let before = state.config_values["geometry"]["wing"]["root_z_m"].clone();
        state.config_values["geometry"]["wing"]["root_z_m"] = Value::from(3.5);
        state.design_values.insert("span_m".to_owned(), 60.0);
        state.on_config_modified();
        assert_eq!(state.config_values["geometry"]["wing"]["root_z_m"], before);
        assert_eq!(state.design_values["span_m"], 71.75);
        assert!(state
            .logs
            .iter()
            .any(|line| line.kind == LogKind::Warn && line.text.contains("protected")));
    }

    #[test]
    fn the_engine_model_stays_editable_while_its_installation_is_protected() {
        let mut state = AppState::default();
        state.set_engine("PW127M");
        let config = state.typed_config().expect("config");
        assert_eq!(config.geometry.engine.engine_name, "PW127M");
        let reference = presets::get("AVE").expect("AVE").geometry.clone();
        assert_eq!(
            config.geometry.engine.spanwise_positions_m,
            reference.engine.spanwise_positions_m
        );
    }

    #[test]
    fn a_custom_design_is_not_locked() {
        let mut state = AppState::default();
        state.set_design_mode(alas_config::DesignMode::CleanSheet);
        assert!(state.active_preset.is_empty());
        assert!(!state.manual_geometry_locked());
        state.config_values["geometry"]["wing"]["root_z_m"] = Value::from(3.5);
        state.on_config_modified();
        assert_eq!(
            state.config_values["geometry"]["wing"]["root_z_m"],
            Value::from(3.5)
        );
    }
}
