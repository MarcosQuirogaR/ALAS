// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Field policy of a registered aircraft preset.
//!
//! Preset mode (a registered `preset` with the `ReferenceAdaptation` or
//! `BaselineSandbox` design mode) protects the defining properties of the
//! aircraft from manual edits and gives the optimizer only the bounded
//! envelope of decision D09 around the originally loaded reference. Every
//! configuration field and design variable falls into one of five classes:
//!
//! | Class | Members | Manual edit | Optimizer |
//! |---|---|---|---|
//! | `LockedGeometry` | `preset` identity; `geometry/*` except the engine model fields below; the engine installation (`spanwise_positions_m`, `z_m`, `inlet_x_offset_m`) | no | no |
//! | `OptimizerEnvelope` | the 16 design variables (`span_m` ... `bump_lower_rear`); their initial point is the preset's design vector | no (initial point held at the preset) | yes, inside `x_ref +- 0.10 |x_ref|` for lengths and scales (angles, shifts and bumps use the configured absolute windows), intersected with the preset limits; variables listed in `reference_fixed_variables` stay at the reference |
//! | `Operational` | `requirements/*`, `departure_airport`, `arrival_airport`, `mission/*`, `cabin/*`, `fuel_policy/*` | yes | no |
//! | `Derived` | quantities computed from the classes above (reference area, span loading, mean aerodynamic chord, aspect ratio, masses, capacities); never stored as independent inputs | no | no |
//! | `AdvancedModel` | `geometry/engine` model fields (engine name and its catalogue-derived cycle values), `optimizer/*`, `analysis/*`, `drag_model/*`, `performance/*`, `mass_model/*`, `landing_gear/*`, `mses/*`, `control_surfaces/*`, `propulsion_cycle/*`, `structures/*`, `fuel_tanks/*` | yes, in Advanced Settings | no |
//!
//! The locked set is the one the guided workspace restores after every
//! mutation; this module makes the same rule available to configuration
//! loading and to analysis dispatch so a loaded file, a detached window or a
//! library caller cannot bypass it. Candidates not yet locked and recorded
//! for review: landing-gear topology and control-surface geometry of a
//! registered type.
//!
//! D09 anchoring: the envelope is always built around the registry's design
//! vector, never around the latest candidate, so repeated runs cannot
//! compound the allowance. A zero reference stays zero for fractional
//! windows; discrete or categorical inputs are not design variables here.

use serde::Serialize;
use serde_json::Value;

use crate::design_variables::{DesignVector, SPECS};
use crate::optimizer::DesignMode;
use crate::presets::{self, AircraftPreset, UnknownAircraftPreset};
use crate::{AlasConfig, GeometryConfig};

/// The policy class of a configuration field or design variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum PresetFieldClass {
    /// Defining geometry of the registered aircraft; restored on every edit
    /// and rejected at dispatch when changed.
    LockedGeometry,
    /// A design variable the optimizer may move inside the D09 envelope.
    OptimizerEnvelope,
    /// Mission, payload and route inputs the user edits freely.
    Operational,
    /// A quantity computed from other classes, not an independent input.
    Derived,
    /// A model assumption editable in Advanced Settings.
    AdvancedModel,
}

impl PresetFieldClass {
    /// Stable name for catalogs and serialization.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LockedGeometry => "locked_geometry",
            Self::OptimizerEnvelope => "optimizer_envelope",
            Self::Operational => "operational",
            Self::Derived => "derived",
            Self::AdvancedModel => "advanced_model",
        }
    }

    /// Whether a user may edit the field by hand in preset mode.
    pub fn manual_edit_allowed(self) -> bool {
        matches!(self, Self::Operational | Self::AdvancedModel)
    }
}

/// Engine fields that describe the installation rather than the engine
/// model, and are therefore locked with the rest of the geometry.
pub const LOCKED_ENGINE_FIELDS: [&str; 3] = ["spanwise_positions_m", "z_m", "inlet_x_offset_m"];

/// Derived quantities shown next to the inputs, with their definition.
pub const DERIVED_QUANTITIES: [(&str, &str); 6] = [
    (
        "reference_area_m2",
        "trapezoidal planform area of the wing from span and the root, break and tip chords",
    ),
    (
        "mean_aerodynamic_chord_m",
        "mean aerodynamic chord of the trapezoidal planform",
    ),
    ("aspect_ratio", "span squared over the reference area"),
    ("taper_ratio", "tip chord over root chord"),
    (
        "operating_empty_mass_kg",
        "mass-model output at the fixed design weights",
    ),
    (
        "usable_fuel_capacity_kg",
        "resolved tank layout or published preset capacity",
    ),
];

/// Classify a configuration path such as `geometry/wing/span_m` or
/// `/requirements/mtow_kg` (leading slash optional, `/`-separated keys).
pub fn classify_path(path: &str) -> PresetFieldClass {
    let mut parts = path
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty());
    match parts.next() {
        Some("preset") => PresetFieldClass::LockedGeometry,
        Some("geometry") => match parts.next() {
            Some("engine") => match parts.next() {
                Some(field) if LOCKED_ENGINE_FIELDS.contains(&field) => {
                    PresetFieldClass::LockedGeometry
                }
                Some(_) => PresetFieldClass::AdvancedModel,
                None => PresetFieldClass::LockedGeometry,
            },
            _ => PresetFieldClass::LockedGeometry,
        },
        Some(
            "requirements" | "departure_airport" | "arrival_airport" | "mission" | "cabin"
            | "fuel_policy",
        ) => PresetFieldClass::Operational,
        Some(_) | None => PresetFieldClass::AdvancedModel,
    }
}

/// Classify a design variable by name; `None` when the name is not one.
pub fn classify_design_variable(name: &str) -> Option<PresetFieldClass> {
    SPECS
        .iter()
        .any(|spec| spec.name == name)
        .then_some(PresetFieldClass::OptimizerEnvelope)
}

/// Whether `config` is in preset mode: a registered preset analysed or
/// optimized as the reference. A clean-sheet study that starts from a
/// preset's shape clears the preset identity, so it is not preset mode.
pub fn preset_mode_active(config: &AlasConfig) -> bool {
    !config.preset.is_empty()
        && matches!(
            config.optimizer.design_space.mode,
            DesignMode::ReferenceAdaptation | DesignMode::BaselineSandbox
        )
}

/// The registry geometry of `preset` with its declared engine's catalogue
/// values applied, which is what a loaded preset configuration carries.
pub fn reference_geometry(preset: &AircraftPreset) -> GeometryConfig {
    let mut geometry = preset.geometry.clone();
    geometry.engine.apply_engine_spec();
    geometry
}

/// The protected geometry: `reference` with `current`'s unprotected engine
/// model fields carried over, so changing the engine model is allowed while
/// the installation stays where the registry puts it.
pub fn protected_geometry(reference: &GeometryConfig, current: &GeometryConfig) -> GeometryConfig {
    let mut protected = reference.clone();
    let mut engine = current.engine.clone();
    engine.spanwise_positions_m = reference.engine.spanwise_positions_m.clone();
    engine.z_m = reference.engine.z_m;
    engine.inlet_x_offset_m = reference.engine.inlet_x_offset_m;
    protected.engine = engine;
    protected
}

/// One field whose value departs from the policy.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PresetPolicyViolation {
    /// `/`-separated configuration path, design-variable name or bound name.
    pub path: String,
    /// The class the field belongs to.
    pub class: PresetFieldClass,
    /// The value the policy requires, rendered as JSON.
    pub expected: String,
    /// The value found, rendered as JSON.
    pub actual: String,
}

impl PresetPolicyViolation {
    fn new(path: String, class: PresetFieldClass, expected: &Value, actual: &Value) -> Self {
        Self {
            path,
            class,
            expected: expected.to_string(),
            actual: actual.to_string(),
        }
    }
}

fn leaf_differences(
    prefix: &str,
    expected: &Value,
    actual: &Value,
    class: PresetFieldClass,
    out: &mut Vec<PresetPolicyViolation>,
) {
    match (expected, actual) {
        (Value::Object(expected_map), Value::Object(actual_map)) => {
            for (key, expected_value) in expected_map {
                let path = format!("{prefix}/{key}");
                match actual_map.get(key) {
                    Some(actual_value) => {
                        leaf_differences(&path, expected_value, actual_value, class, out)
                    }
                    None => out.push(PresetPolicyViolation::new(
                        path,
                        class,
                        expected_value,
                        &Value::Null,
                    )),
                }
            }
            for (key, actual_value) in actual_map {
                if !expected_map.contains_key(key) {
                    out.push(PresetPolicyViolation::new(
                        format!("{prefix}/{key}"),
                        class,
                        &Value::Null,
                        actual_value,
                    ));
                }
            }
        }
        (Value::Array(expected_items), Value::Array(actual_items))
            if expected_items.len() == actual_items.len() =>
        {
            for (index, (expected_item, actual_item)) in
                expected_items.iter().zip(actual_items).enumerate()
            {
                leaf_differences(
                    &format!("{prefix}/{index}"),
                    expected_item,
                    actual_item,
                    class,
                    out,
                );
            }
        }
        _ if expected != actual => out.push(PresetPolicyViolation::new(
            prefix.to_owned(),
            class,
            expected,
            actual,
        )),
        _ => {}
    }
}

/// Locked geometry fields of `current` that differ from `preset`'s.
pub fn geometry_violations(
    preset: &AircraftPreset,
    current: &GeometryConfig,
) -> Vec<PresetPolicyViolation> {
    let protected = protected_geometry(&reference_geometry(preset), current);
    let mut out = Vec::new();
    match (
        serde_json::to_value(&protected),
        serde_json::to_value(current),
    ) {
        (Ok(expected), Ok(actual)) => leaf_differences(
            "geometry",
            &expected,
            &actual,
            PresetFieldClass::LockedGeometry,
            &mut out,
        ),
        _ if protected != *current => out.push(PresetPolicyViolation {
            path: "geometry".to_owned(),
            class: PresetFieldClass::LockedGeometry,
            expected: "the registered preset geometry".to_owned(),
            actual: "a geometry that does not serialize".to_owned(),
        }),
        _ => {}
    }
    out
}

/// Design variables of `design` that differ from the preset's design vector,
/// which is the only admissible initial point in preset mode.
pub fn design_violations(
    preset: &AircraftPreset,
    design: &DesignVector,
) -> Vec<PresetPolicyViolation> {
    SPECS
        .iter()
        .zip(preset.design_vector.to_array())
        .zip(design.to_array())
        .filter(|((_, expected), actual)| expected != actual)
        .map(|((spec, expected), actual)| PresetPolicyViolation {
            path: spec.name.to_owned(),
            class: PresetFieldClass::OptimizerEnvelope,
            expected: expected.to_string(),
            actual: actual.to_string(),
        })
        .collect()
}

/// Search bounds that reach outside the D09 envelope anchored at the
/// preset's own design vector. `bounds` follows the design-vector order; a
/// wrong length is reported as one violation.
pub fn bounds_violations(
    config: &AlasConfig,
    preset: &AircraftPreset,
    bounds: &[(f64, f64)],
) -> Vec<PresetPolicyViolation> {
    let envelope = config
        .optimizer
        .design_space
        .envelope(&preset.design_vector);
    if bounds.len() != envelope.len() {
        return vec![PresetPolicyViolation {
            path: "bounds".to_owned(),
            class: PresetFieldClass::OptimizerEnvelope,
            expected: format!("{} bound pairs", envelope.len()),
            actual: format!("{} bound pairs", bounds.len()),
        }];
    }
    envelope
        .iter()
        .zip(bounds)
        .filter(|(variable, &(lower, upper))| {
            let slack = 1e-9 * variable.nominal.abs().max(1.0);
            lower < variable.lower - slack || upper > variable.upper + slack || lower > upper
        })
        .map(|(variable, &(lower, upper))| PresetPolicyViolation {
            path: variable.name.to_owned(),
            class: PresetFieldClass::OptimizerEnvelope,
            expected: format!("[{}, {}]", variable.lower, variable.upper),
            actual: format!("[{lower}, {upper}]"),
        })
        .collect()
}

/// Every violation a dispatch of `config` would carry: none unless preset
/// mode is active; otherwise the locked geometry, the initial design point
/// when supplied and the search bounds when supplied.
///
/// # Errors
///
/// The preset identity is not registered.
pub fn dispatch_violations(
    config: &AlasConfig,
    initial_design: Option<&DesignVector>,
    bounds: Option<&[(f64, f64)]>,
) -> Result<Vec<PresetPolicyViolation>, UnknownAircraftPreset> {
    if !preset_mode_active(config) {
        return Ok(Vec::new());
    }
    let preset = presets::get(&config.preset)?;
    let mut out = geometry_violations(preset, &config.geometry);
    if let Some(design) = initial_design {
        out.extend(design_violations(preset, design));
    }
    if let Some(bounds) = bounds {
        out.extend(bounds_violations(config, preset, bounds));
    }
    Ok(out)
}

/// Restore the locked geometry and the initial design point of `config`
/// to the registered preset's, returning what had to change. Nothing is
/// touched outside preset mode or for an unregistered preset.
pub fn restore_locked_fields(
    config: &mut AlasConfig,
    design: &mut DesignVector,
) -> Vec<PresetPolicyViolation> {
    if !preset_mode_active(config) {
        return Vec::new();
    }
    let Ok(preset) = presets::get(&config.preset) else {
        return Vec::new();
    };
    let mut restored = geometry_violations(preset, &config.geometry);
    if !restored.is_empty() {
        config.geometry = protected_geometry(&reference_geometry(preset), &config.geometry);
    }
    let design_drift = design_violations(preset, design);
    if !design_drift.is_empty() {
        *design = preset.design_vector;
        restored.extend(design_drift);
    }
    restored
}

/// One line naming the violations, for logs and error messages.
pub fn describe_violations(violations: &[PresetPolicyViolation]) -> String {
    let mut listed: Vec<String> = violations
        .iter()
        .take(6)
        .map(|violation| {
            format!(
                "{} (expected {}, found {})",
                violation.path, violation.expected, violation.actual
            )
        })
        .collect();
    if violations.len() > listed.len() {
        listed.push(format!("and {} more", violations.len() - listed.len()));
    }
    listed.join("; ")
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn preset_config(name: &str, mode: DesignMode) -> AlasConfig {
        let mut config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
            .expect("registered preset loads");
        config.optimizer.design_space.mode = mode;
        config
    }

    #[test]
    fn a_freshly_loaded_preset_carries_no_violation_in_either_reference_mode() {
        for name in ["AVE", "A320-200", "ATR72-600", "A380-800"] {
            for mode in [DesignMode::ReferenceAdaptation, DesignMode::BaselineSandbox] {
                let config = preset_config(name, mode);
                let preset = presets::get(name).unwrap();
                let bounds = config.optimizer.design_space.bounds(&preset.design_vector);
                let violations =
                    dispatch_violations(&config, Some(&preset.design_vector), Some(&bounds))
                        .unwrap();
                assert!(violations.is_empty(), "{name} {mode:?}: {violations:?}");
            }
        }
    }

    #[test]
    fn locked_geometry_drift_is_named_by_path_while_the_engine_model_stays_free() {
        let mut config = preset_config("A320-200", DesignMode::BaselineSandbox);
        config.geometry.wing.root_z_m += 0.5;
        config.geometry.engine.spanwise_positions_m.push(3.0);
        let violations = dispatch_violations(&config, None, None).unwrap();
        let paths: Vec<&str> = violations.iter().map(|v| v.path.as_str()).collect();
        assert!(paths.contains(&"geometry/wing/root_z_m"), "{paths:?}");
        assert!(
            paths.contains(&"geometry/engine/spanwise_positions_m"),
            "{paths:?}"
        );
        assert!(violations
            .iter()
            .all(|v| v.class == PresetFieldClass::LockedGeometry));

        let mut config = preset_config("A320-200", DesignMode::BaselineSandbox);
        config.geometry.engine.engine_name = "PW127M".to_owned();
        config.geometry.engine.apply_engine_spec();
        assert!(dispatch_violations(&config, None, None).unwrap().is_empty());
    }

    #[test]
    fn a_drifted_initial_design_and_widened_bounds_are_rejected_in_preset_mode() {
        let config = preset_config("A320-200", DesignMode::ReferenceAdaptation);
        let preset = presets::get("A320-200").unwrap();
        let mut design = preset.design_vector;
        design.span_m *= 1.05;
        let violations = dispatch_violations(&config, Some(&design), None).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].path, "span_m");
        assert_eq!(violations[0].class, PresetFieldClass::OptimizerEnvelope);

        // Windows around the drifted candidate compound the allowance.
        let compounded = config.optimizer.design_space.bounds(&design);
        let violations =
            dispatch_violations(&config, Some(&preset.design_vector), Some(&compounded)).unwrap();
        assert!(
            violations.iter().any(|v| v.path == "span_m"),
            "{violations:?}"
        );
        let anchored = config.optimizer.design_space.bounds(&preset.design_vector);
        let violations =
            dispatch_violations(&config, Some(&preset.design_vector), Some(&anchored)).unwrap();
        assert!(violations.is_empty(), "{violations:?}");
        let short = bounds_violations(&config, preset, &anchored[..3]);
        assert_eq!(short.len(), 1);
        assert_eq!(short[0].path, "bounds");
    }

    #[test]
    fn outside_preset_mode_nothing_is_enforced() {
        let mut config = preset_config("A320-200", DesignMode::CleanSheet);
        config.geometry.wing.root_z_m += 0.5;
        assert!(dispatch_violations(&config, None, None).unwrap().is_empty());
        let mut custom = preset_config("A320-200", DesignMode::BaselineSandbox);
        custom.preset.clear();
        custom.geometry.wing.root_z_m += 0.5;
        assert!(dispatch_violations(&custom, None, None).unwrap().is_empty());
        let mut design = DesignVector::default();
        assert!(restore_locked_fields(&mut custom, &mut design).is_empty());
    }

    #[test]
    fn restoring_returns_what_changed_and_leaves_a_clean_configuration() {
        let mut config = preset_config("A380-800", DesignMode::ReferenceAdaptation);
        let preset = presets::get("A380-800").unwrap();
        config.geometry.wing.root_z_m += 0.5;
        let mut design = preset.design_vector;
        design.sweep_deg += 2.0;
        let restored = restore_locked_fields(&mut config, &mut design);
        assert_eq!(restored.len(), 2, "{restored:?}");
        assert!(dispatch_violations(&config, Some(&design), None)
            .unwrap()
            .is_empty());
        assert_eq!(design, preset.design_vector);
        let text = describe_violations(&restored);
        assert!(text.contains("geometry/wing/root_z_m") && text.contains("sweep_deg"));
    }

    #[test]
    fn the_class_table_follows_the_documented_groups() {
        assert_eq!(
            classify_path("geometry/wing/span_m"),
            PresetFieldClass::LockedGeometry
        );
        assert_eq!(
            classify_path("/geometry/engine/spanwise_positions_m"),
            PresetFieldClass::LockedGeometry
        );
        assert_eq!(
            classify_path("geometry/engine/engine_name"),
            PresetFieldClass::AdvancedModel
        );
        assert_eq!(
            classify_path("requirements/mtow_kg"),
            PresetFieldClass::Operational
        );
        assert_eq!(
            classify_path("departure_airport"),
            PresetFieldClass::Operational
        );
        assert_eq!(
            classify_path("mass_model/x"),
            PresetFieldClass::AdvancedModel
        );
        assert_eq!(classify_path("preset"), PresetFieldClass::LockedGeometry);
        assert_eq!(
            classify_design_variable("span_m"),
            Some(PresetFieldClass::OptimizerEnvelope)
        );
        assert_eq!(classify_design_variable("not_a_variable"), None);
        assert!(!PresetFieldClass::LockedGeometry.manual_edit_allowed());
        assert!(PresetFieldClass::Operational.manual_edit_allowed());
    }
}
