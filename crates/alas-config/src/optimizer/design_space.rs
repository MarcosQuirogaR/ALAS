// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Which design variables the search may move, and how far.
//!
//! A clean-sheet study lets every geometry variable roam its global bounds
//! and sizes what the brief fixes (the fuselage from the cabin, the engine
//! from the thrust the mission needs). A reference-aircraft adaptation
//! starts from a registered type and keeps it recognisable: each variable is
//! either fixed at the reference value or free inside an explicit window
//! around it, and the window is recorded with the run so a "redesigned A320"
//! can be traced back to the envelope that produced it.

use serde::{Deserialize, Serialize};

use crate::design_variables::{DesignVariableSpec, DesignVector, SPECS};
use crate::{ConfigNode, Kind, Leaf};

/// How the search treats the aircraft it starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignMode {
    /// Every geometry variable is free inside its global bounds; the fuselage
    /// is sized by the cabin. The selected catalogue engine remains fixed.
    #[default]
    CleanSheet,
    /// The registered aircraft is the reference; only the variables listed as
    /// mutable move, inside the configured windows, and the engine is fixed.
    ReferenceAdaptation,
    /// Replay the registered physical aircraft and load case without any
    /// optimizer-driven geometry, engine, or structural resizing.
    BaselineSandbox,
}

impl DesignMode {
    /// Stable serialized name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CleanSheet => "clean_sheet",
            Self::ReferenceAdaptation => "reference_adaptation",
            Self::BaselineSandbox => "baseline_sandbox",
        }
    }
}

impl Leaf for DesignMode {
    fn kind(&self, _name: &str) -> Kind {
        Kind::Str
    }
}

/// What one design variable may do in the search.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VariableEnvelope {
    /// The variable, by its design-vector name.
    pub name: &'static str,
    /// The value the search starts from.
    pub nominal: f64,
    /// Lower bound handed to the search; equals `nominal` when fixed.
    pub lower: f64,
    /// Upper bound handed to the search; equals `nominal` when fixed.
    pub upper: f64,
    /// Whether the variable is held at its reference value.
    pub fixed: bool,
}

/// The design space the search runs over.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(default, deny_unknown_fields)]
pub struct DesignSpaceConfig {
    /// Clean sheet or reference adaptation.
    #[config(
        options = DesignMode,
        label = "Design mode",
        help = "Clean sheet lets every geometry variable move inside its global bounds, sizes the fuselage from the cabin and may scale the engine. Reference adaptation starts from the registered aircraft, keeps the variables listed as fixed at their reference values and moves the others only inside the windows below."
    )]
    pub mode: DesignMode,

    /// Whether the fuselage length is derived from the cabin in clean-sheet mode.
    #[config(
        label = "Size fuselage from cabin",
        help = "In clean-sheet mode, set the fuselage length to the shortest length whose cabin seats the brief, instead of treating it as a search variable with an integer seat-count step. Reference adaptation ignores this and keeps the fuselage as configured."
    )]
    pub fuselage_sized_by_cabin: bool,

    /// Deprecated compatibility field. Engine scaling is not a supported
    /// design freedom: the selected catalogue engine, including its off-design
    /// deck, is bound once and used by every discipline.
    #[config(skip)]
    pub engine_scale_enabled: bool,

    /// Lowest engine thrust scale the search may select.
    #[config(
        label = "Engine scale, lower bound",
        help = "Smallest multiplier on the selected engine's rated thrust the clean-sheet search may choose."
    )]
    pub engine_scale_lower: f64,

    /// Highest engine thrust scale the search may select.
    #[config(
        label = "Engine scale, upper bound",
        help = "Largest multiplier on the selected engine's rated thrust the clean-sheet search may choose."
    )]
    pub engine_scale_upper: f64,

    /// Design variables held at their reference value in reference mode.
    #[config(
        label = "Fixed variables (reference mode)",
        help = "Comma-separated design-variable names that reference adaptation holds at the registered aircraft's values, for example 'fuselage_length_m, tail_scale, tail_x_shift_m'. Every other variable moves inside the windows below."
    )]
    pub reference_fixed_variables: String,

    /// Relative half-width of the window on lengths and scale factors.
    #[config(
        label = "Reference window, lengths and scales",
        unit = "fraction",
        help = "Half-width of the search window around the reference value for spans, chords, the fuselage length and the scale factors (thickness, camber, tail), as a fraction of that value. 0.10 lets a 34 m span move between 30.6 and 37.4 m."
    )]
    pub reference_fraction_half_width: f64,

    /// Half-width of the window on angles.
    #[config(
        label = "Reference window, angles",
        unit = "deg",
        help = "Half-width of the search window around the reference sweep and tip twist."
    )]
    pub reference_angle_half_width_deg: f64,

    /// Half-width of the window on longitudinal shifts.
    #[config(
        label = "Reference window, shifts",
        unit = "m",
        help = "Half-width of the search window around the reference wing and tail longitudinal positions."
    )]
    pub reference_shift_half_width_m: f64,

    /// Half-width of the window on airfoil bump amplitudes.
    #[config(
        label = "Reference window, airfoil bumps",
        unit = "chord fraction",
        help = "Half-width of the search window around the reference Hicks-Henne bump amplitudes."
    )]
    pub reference_bump_half_width: f64,
}

impl Default for DesignSpaceConfig {
    fn default() -> Self {
        Self {
            mode: DesignMode::CleanSheet,
            fuselage_sized_by_cabin: true,
            engine_scale_enabled: false,
            engine_scale_lower: 0.7,
            engine_scale_upper: 1.4,
            reference_fixed_variables: "fuselage_length_m, tail_scale, tail_x_shift_m".to_owned(),
            reference_fraction_half_width: 0.10,
            reference_angle_half_width_deg: 3.0,
            reference_shift_half_width_m: 1.0,
            reference_bump_half_width: 0.002,
        }
    }
}

/// How a variable's reference window is measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowKind {
    Fraction,
    Angle,
    Shift,
    Bump,
}

fn window_kind(name: &str) -> WindowKind {
    match name {
        "sweep_deg" | "tip_twist_deg" => WindowKind::Angle,
        "wing_x_shift_m" | "tail_x_shift_m" => WindowKind::Shift,
        name if name.starts_with("bump_") => WindowKind::Bump,
        _ => WindowKind::Fraction,
    }
}

impl DesignSpaceConfig {
    /// Whether the serialized group equals the defaults.
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// The variable names listed as fixed, trimmed and de-duplicated.
    pub fn fixed_variable_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self
            .reference_fixed_variables
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect();
        names.dedup();
        names
    }

    /// Whether the fuselage length is derived from the cabin for this run.
    pub fn sizes_fuselage_from_cabin(&self) -> bool {
        self.mode == DesignMode::CleanSheet && self.fuselage_sized_by_cabin
    }

    /// Whether the engine thrust scale is a search variable for this run.
    pub fn scales_engine(&self) -> bool {
        // No design-vector coordinate represents engine thrust. Keep this
        // method for saved-config/source compatibility, but never expose a
        // rubber-engine degree of freedom that would bypass the catalogue
        // off-design deck and propulsion-mass binding.
        false
    }

    /// Reject windows and scale bounds the search cannot run with.
    ///
    /// # Errors
    ///
    /// A description of the first invalid value.
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("engine_scale_lower", self.engine_scale_lower),
            ("engine_scale_upper", self.engine_scale_upper),
            (
                "reference_fraction_half_width",
                self.reference_fraction_half_width,
            ),
            (
                "reference_angle_half_width_deg",
                self.reference_angle_half_width_deg,
            ),
            (
                "reference_shift_half_width_m",
                self.reference_shift_half_width_m,
            ),
            ("reference_bump_half_width", self.reference_bump_half_width),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(format!(
                    "design space {name} must be finite and nonnegative"
                ));
            }
        }
        if self.engine_scale_lower <= 0.0 || self.engine_scale_upper < self.engine_scale_lower {
            return Err("engine scale bounds must satisfy 0 < lower <= upper".to_owned());
        }
        for name in self.fixed_variable_names() {
            if !SPECS.iter().any(|spec| spec.name == name) {
                return Err(format!("fixed variable '{name}' is not a design variable"));
            }
        }
        Ok(())
    }

    /// The per-variable envelope the search runs over, from `nominal`.
    ///
    /// Clean sheet: the global bounds of every variable, with the fuselage
    /// length fixed at the nominal when the cabin sizes it. Reference
    /// adaptation: the configured windows around `nominal`, intersected with
    /// the global bounds, and the listed variables fixed. A nominal outside
    /// its global bounds keeps a window around itself rather than being
    /// snapped: the reference aircraft is the truth the study starts from.
    pub fn envelope(&self, nominal: &DesignVector) -> Vec<VariableEnvelope> {
        let values = nominal.to_array();
        let fixed = self.fixed_variable_names();
        SPECS
            .iter()
            .zip(values)
            .map(|(spec, value)| match self.mode {
                DesignMode::CleanSheet => {
                    let fixed = spec.name == "fuselage_length_m" && self.fuselage_sized_by_cabin;
                    self.clean_sheet_envelope(spec, value, fixed)
                }
                DesignMode::ReferenceAdaptation => {
                    self.reference_envelope(spec, value, fixed.contains(&spec.name))
                }
                DesignMode::BaselineSandbox => VariableEnvelope {
                    name: spec.name,
                    nominal: value,
                    lower: value,
                    upper: value,
                    fixed: true,
                },
            })
            .collect()
    }

    /// `(lower, upper)` per variable, in design-vector order.
    pub fn bounds(&self, nominal: &DesignVector) -> Vec<(f64, f64)> {
        self.envelope(nominal)
            .into_iter()
            .map(|variable| (variable.lower, variable.upper))
            .collect()
    }

    fn clean_sheet_envelope(
        &self,
        spec: &DesignVariableSpec,
        value: f64,
        fixed: bool,
    ) -> VariableEnvelope {
        if fixed {
            return VariableEnvelope {
                name: spec.name,
                nominal: value,
                lower: value,
                upper: value,
                fixed: true,
            };
        }
        // A registered aircraft may sit outside AVE's global box (an A320
        // fuselage is shorter than the box's floor); the box is then widened
        // to include the start so the search can leave it in every direction.
        VariableEnvelope {
            name: spec.name,
            nominal: value,
            lower: spec.lower.min(value),
            upper: spec.upper.max(value),
            fixed: false,
        }
    }

    fn reference_envelope(
        &self,
        spec: &DesignVariableSpec,
        value: f64,
        fixed: bool,
    ) -> VariableEnvelope {
        if fixed {
            return VariableEnvelope {
                name: spec.name,
                nominal: value,
                lower: value,
                upper: value,
                fixed: true,
            };
        }
        let half_width = match window_kind(spec.name) {
            WindowKind::Fraction => self.reference_fraction_half_width * value.abs(),
            WindowKind::Angle => self.reference_angle_half_width_deg,
            WindowKind::Shift => self.reference_shift_half_width_m,
            WindowKind::Bump => self.reference_bump_half_width,
        };
        VariableEnvelope {
            name: spec.name,
            nominal: value,
            lower: (value - half_width).max(spec.preset_lower),
            upper: (value + half_width).min(spec.preset_upper),
            fixed: half_width <= 0.0,
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_is_a_clean_sheet_with_a_cabin_sized_fuselage() {
        let config = DesignSpaceConfig::default();
        assert_eq!(config.mode, DesignMode::CleanSheet);
        assert!(config.sizes_fuselage_from_cabin());
        assert!(!config.scales_engine());
        assert!(config.validate().is_ok());
        let envelope = config.envelope(&DesignVector::default());
        let fuselage = envelope
            .iter()
            .find(|variable| variable.name == "fuselage_length_m")
            .unwrap();
        assert!(fuselage.fixed);
        let span = envelope
            .iter()
            .find(|variable| variable.name == "span_m")
            .unwrap();
        assert_eq!((span.lower, span.upper), (60.0, 80.0));
    }

    #[test]
    fn reference_mode_windows_the_planform_and_fixes_the_listed_variables() {
        let config = DesignSpaceConfig {
            mode: DesignMode::ReferenceAdaptation,
            ..Default::default()
        };
        let nominal = DesignVector {
            span_m: 34.1,
            fuselage_length_m: 37.57,
            ..DesignVector::default()
        };
        let envelope = config.envelope(&nominal);
        let span = envelope.iter().find(|v| v.name == "span_m").unwrap();
        assert!((span.lower - 30.69).abs() < 1e-9 && (span.upper - 37.51).abs() < 1e-9);
        assert!(!span.fixed);
        let fuselage = envelope
            .iter()
            .find(|v| v.name == "fuselage_length_m")
            .unwrap();
        assert!(fuselage.fixed && fuselage.lower == 37.57 && fuselage.upper == 37.57);
        let sweep = envelope.iter().find(|v| v.name == "sweep_deg").unwrap();
        assert!((sweep.lower - 31.0).abs() < 1e-9 && (sweep.upper - 37.0).abs() < 1e-9);
        assert!(!config.scales_engine());
        assert!(!config.sizes_fuselage_from_cabin());
    }

    #[test]
    fn a_reference_outside_the_global_box_keeps_its_own_window() {
        let config = DesignSpaceConfig {
            mode: DesignMode::ReferenceAdaptation,
            reference_fixed_variables: String::new(),
            ..Default::default()
        };
        let nominal = DesignVector {
            fuselage_length_m: 37.57,
            ..DesignVector::default()
        };
        let fuselage = config
            .envelope(&nominal)
            .into_iter()
            .find(|v| v.name == "fuselage_length_m")
            .unwrap();
        assert!(fuselage.lower < 37.57 && fuselage.upper > 37.57);
        assert!(fuselage.lower >= 20.0);
    }

    #[test]
    fn an_unknown_fixed_variable_and_a_bad_scale_window_are_rejected() {
        let config = DesignSpaceConfig {
            reference_fixed_variables: "span, wing_x_shift_m".to_owned(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
        let config = DesignSpaceConfig {
            engine_scale_lower: 1.5,
            engine_scale_upper: 1.2,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn mode_names_are_stable_in_saved_configuration() {
        assert_eq!(
            serde_json::to_value(DesignMode::ReferenceAdaptation).ok(),
            Some(serde_json::json!("reference_adaptation"))
        );
        assert_eq!(DesignMode::CleanSheet.as_str(), "clean_sheet");
    }
}
