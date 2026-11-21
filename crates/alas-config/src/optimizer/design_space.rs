// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Which design variables the search may move, and how far.
//!
//! A clean-sheet study lets every geometry variable roam its global bounds
//! and sizes what the brief fixes (the fuselage from the cabin, the engine
//! rating from the thrust requirements). A reference-aircraft adaptation
//! starts from a registered type and keeps it recognisable: each variable is
//! either fixed at the reference value or free inside an explicit window
//! around it, and the window is recorded with the run so a "redesigned A320"
//! can be traced back to the envelope that produced it.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::design_variables::{DesignVariableSpec, DesignVector, SPECS};
use crate::presets::AircraftClass;
use crate::{ConfigNode, Kind, Leaf};

/// How the search treats the aircraft it starts from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesignMode {
    /// Every geometry variable is free inside its global bounds; the fuselage
    /// is sized by the cabin and the engine rating by the thrust requirements.
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
        help = "Clean sheet moves every geometry variable inside its global bounds, sizes the fuselage from the cabin and scales the engine rating.",
        long_help = "Clean sheet lets every geometry variable move inside its global bounds, sizes the fuselage from the cabin and, unless disabled below, scales the engine rating to the thrust requirements. Reference adaptation starts from the registered aircraft, keeps the variables listed as fixed at their reference values and moves the others only inside the windows below."
    )]
    pub mode: DesignMode,

    /// Whether the fuselage length is derived from the cabin in clean-sheet mode.
    #[config(
        label = "Size fuselage from cabin",
        help = "In clean-sheet mode, set the fuselage length to the shortest one whose cabin seats the brief, instead of searching it with an integer seat-count step.",
        long_help = "In clean-sheet mode, set the fuselage length to the shortest length whose cabin seats the brief, instead of treating it as a search variable with an integer seat-count step. Reference adaptation ignores this and keeps the fuselage as configured."
    )]
    pub fuselage_sized_by_cabin: bool,

    /// Whether the engine rating is sized from the thrust requirements in
    /// clean-sheet mode.
    #[config(
        label = "Size engine from requirements",
        help = "In clean-sheet mode, scale the catalogue engine to the smallest rating meeting every thrust requirement of the closed design.",
        long_help = "In clean-sheet mode, scale the selected catalogue engine to the smallest rating that meets every thrust requirement of the closed design (take-off field, engine-out second segment, final segment, approach and landing climb, and level flight at the design cruise condition at mid-cruise mass; a turboprop sizes on its planned cruise level), instead of keeping its catalogue rating. The catalogue engine is the baseline cycle; its dimensions, fuel flow and mass follow the rating (Raymer, Aircraft Design, 6th ed., Eqs. 10.1-10.3; Jenkinson, Civil Jet Aircraft Design, Sec. 9.10). A held engine (this option off, or a promoted sandbox) whose own requirement is more than the engine rating window below its rating (the rating exceeds the requirement by more than 1/(1 - window) - 1, 25 percent at the default 20 percent window) is only flagged, as a warning finding shown with the results; the design stays feasible. Reference adaptation and baseline analysis always keep the declared engine."
    )]
    pub engine_sized_by_requirements: bool,

    /// Largest departure of the sized engine rating from the catalogue's.
    #[config(
        label = "Engine rating window",
        unit = "fraction",
        help = "Largest departure of the sized engine rating from the catalogue engine's, as a fraction of that rating, up to 0.6.",
        long_help = "Largest departure of the sized engine rating from the selected catalogue engine's, as a fraction of that rating, up to 0.6 (the end of the sizing search). Jenkinson (Civil Jet Aircraft Design, Sec. 9.10) limits scaling an engine to about 20 percent; a design that needs a rating outside the window is infeasible instead of being extrapolated. Select a catalogue engine closer to the rating the brief needs, or widen the window, to study it anyway."
    )]
    pub engine_rating_window: f64,

    /// Design variables held at their reference value in reference mode.
    ///
    /// The default is [`DEFAULT_REFERENCE_FIXED_VARIABLES`].
    #[config(
        label = "Fixed variables (reference mode)",
        help = "Comma-separated design-variable names that reference adaptation holds at the registered aircraft's values.",
        long_help = "Comma-separated design-variable names that reference adaptation holds at the registered aircraft's values. The default holds the fuselage length, the tail position and every variable whose effect on block fuel is below 0.1 % per full window width on the A320-200, B787-9 and A380-800, which today is only the rear lower Hicks-Henne bump. Remove a name to let the search move it again. The tail scale is always derived from the wing in reference adaptation, so the tails keep the registered tail volume coefficients. Every other variable moves inside the windows below."
    )]
    pub reference_fixed_variables: String,

    /// Relative half-width of the window on lengths and scale factors.
    #[config(
        label = "Reference window, lengths and scales",
        unit = "fraction of reference value",
        help = "Half-width of the search window around the reference value for spans, chords, fuselage length and scale factors, as a fraction of it. 0.10 moves a 34 m span 30.6-37.4 m.",
        long_help = "Half-width of the search window around the reference value for spans, chords, the fuselage length and the scale factors (thickness, camber, tail), as a fraction of that value. 0.10 lets a 34 m span move between 30.6 and 37.4 m."
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
        unit = "fraction of chord",
        help = "Half-width of the search window around the reference Hicks-Henne bump amplitudes."
    )]
    pub reference_bump_half_width: f64,

    /// Explicit clean-sheet start values, by design-variable name.
    ///
    /// Entries replace the derived (or preset) start of a clean-sheet run
    /// variable by variable; other modes ignore them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[config(skip)]
    pub initial_design: BTreeMap<String, f64>,

    /// Explicit clean-sheet search bounds, by design-variable name, as
    /// `[lower, upper]` in the variable's own units.
    ///
    /// Entries replace the derived (or global) box of a clean-sheet run
    /// variable by variable; the fuselage-length entry of a cabin-sized run
    /// is the interval its length is solved in. Other modes ignore them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[config(skip)]
    pub bounds: BTreeMap<String, (f64, f64)>,

    /// Whether a preset-less configuration is a clean-sheet brief whose
    /// start, box, dependent geometry, cabin and exits are derived from it
    /// (`alas_config::clean_sheet`). Loading a file without a preset marks it
    /// true when the brief departs from the shipped reference aircraft's in
    /// clean-sheet mode, and the mark is saved, so a reload keeps the
    /// decision; a file may state it explicitly, an explicit false included,
    /// which is kept. Unset (`None`) means undecided: a registered preset, the
    /// reference brief, or a configuration built in code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[config(skip)]
    pub clean_sheet_brief: Option<bool>,

    /// The class of a clean-sheet brief, which selects its guardrails
    /// ([`DesignVariableSpec::guardrails_for`]); a registered preset's own
    /// class takes precedence ([`crate::AlasConfig::aircraft_class`]).
    #[serde(default, skip_serializing_if = "AircraftClass::is_transport")]
    #[config(
        options = AircraftClass,
        label = "Aircraft class",
        help = "Class of a clean-sheet brief: its design-variable guardrails. A registered aircraft keeps the class its preset declares.",
        long_help = "Class of a clean-sheet brief, which selects the guardrails its design variables are held to: a transport is bounded by the transport pair (a 20 m span floor), a business jet by the business-jet envelope of Jane's All the World's Aircraft 2008-2009 (span 11.40 to 28.65 m). A registered aircraft keeps the class its preset declares, whatever this says."
    )]
    pub aircraft_class: AircraftClass,
}

impl Default for DesignSpaceConfig {
    fn default() -> Self {
        Self {
            mode: DesignMode::CleanSheet,
            fuselage_sized_by_cabin: true,
            engine_sized_by_requirements: true,
            engine_rating_window: crate::ENGINE_SCALE_VALIDITY_FRACTION,
            reference_fixed_variables: DEFAULT_REFERENCE_FIXED_VARIABLES.to_owned(),
            reference_fraction_half_width: 0.10,
            reference_angle_half_width_deg: 3.0,
            reference_shift_half_width_m: 1.0,
            reference_bump_half_width: 0.002,
            initial_design: BTreeMap::new(),
            bounds: BTreeMap::new(),
            clean_sheet_brief: None,
            aircraft_class: AircraftClass::Transport,
        }
    }
}

/// The variables a reference adaptation holds at the registered aircraft's
/// values unless the user lists otherwise.
///
/// The fuselage length and tail position belong to the registered fuselage
/// and empennage layout. Every other variable is held only if its
/// first-order effect on block fuel at the registered design is below 0.1 %
/// of block fuel per full window width on all three of the A320-200, B787-9
/// and A380-800, measured by central differences at +-5 % of the window.
/// Only `bump_lower_rear` qualifies (0.06, 0.03, 0.09). The other three
/// bumps read 0.19-0.83 on the wide-bodies, `sweep_deg` 0.08 on the A320-200
/// but 1.7-8.7 on the wide-bodies and `break_chord_m` 0.12 on the B787-9 but
/// 1.5-2.2 elsewhere, so they stay free. The full table is in
/// `docs/optimizer-design-vector.md`. Removing a name re-enables it, except
/// the section shape, which every search holds ([`SECTION_SHAPE_VARIABLES`]).
pub const DEFAULT_REFERENCE_FIXED_VARIABLES: &str =
    "fuselage_length_m, tail_x_shift_m, bump_lower_rear";

/// The variable a reference adaptation derives from the wing instead of
/// searching: the optimizer's tail auto-sizing sets it so the tails keep the
/// registered aircraft's volume coefficients.
const TAIL_SCALE_VARIABLE: &str = "tail_scale";

/// The wing twist variable, held at its nominal value in every search.
///
/// The trimmed drag table takes the induced drag of the wing twist as the
/// handbook allowance for a non-optimum twist (`alas_aero::analysis`
/// `NON_OPTIMUM_TWIST_DRAG_FACTOR`: Jenkinson, *Civil Jet Aircraft Design*,
/// 1999, pp. 179-180) in place of the wake energy of the lattice's zero-lift
/// loading, which the flight-test drag polars do not show. No published form
/// gives that allowance as a function of the departure of a twist distribution
/// from its optimum, so a search over the twist would be credited the
/// interaction of the twist loading with the lift loading without the cost of
/// the twist loading itself. The twist still sets the trim, the buffet onset
/// and the loads at its declared value.
const TWIST_VARIABLE: &str = "tip_twist_deg";

/// The wing-section shape variables, held at their nominal values in every
/// search.
///
/// The in-loop analysis carries a section's thickness into the wing mass
/// (FLOPS bending term), the tank volume, the form factor and the
/// compressibility drag, and its camber into the lattice and the trim, but
/// not into the section maximum lift or a section drag polar: the clean and
/// flapped CLmax scale the declared coefficients with planform terms only
/// (`alas_opt::mdo::sizing::clmax_geometry`), and the profile drag is skin
/// friction times a thickness form factor. A thinner or more cambered
/// section, or a Hicks-Henne bump, would therefore be credited its drag and
/// lift without the loss of maximum lift with leading-edge sharpness
/// (USAF DATCOM Sec. 4.1.3.4) or the profile drag away from the section's
/// design lift (Abbott and von Doenhoff, Theory of Wing Sections, 1959,
/// Ch. 7). An ATR 72-600 search found exactly that: camber
/// +10 % and thickness -10 % on the window edges. Until a section model
/// prices them the section stays as declared.
const SECTION_SHAPE_VARIABLES: [&str; 6] = [
    "airfoil_thickness_scale",
    "airfoil_camber_scale",
    "bump_upper_front",
    "bump_upper_rear",
    "bump_lower_mid",
    "bump_lower_rear",
];

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

    /// Hold the aircraft as drawn: no cabin-derived fuselage and no sized
    /// engine, as a promoted sandbox baseline needs.
    pub fn hold_as_drawn(&mut self) {
        self.fuselage_sized_by_cabin = false;
        self.engine_sized_by_requirements = false;
    }

    /// Whether the engine rating is sized from the thrust requirements for
    /// this run.
    pub fn sizes_engine(&self) -> bool {
        self.mode == DesignMode::CleanSheet && self.engine_sized_by_requirements
    }

    /// Reject windows the search cannot run with.
    ///
    /// # Errors
    ///
    /// A description of the first invalid value.
    pub fn validate(&self) -> Result<(), String> {
        self.validate_for(AircraftClass::Transport)
    }

    /// [`Self::validate`] with the explicit bounds held to the guardrails of
    /// an aircraft of `class`.
    ///
    /// # Errors
    ///
    /// The first invalid entry, as [`Self::validate`] reports it.
    pub fn validate_for(&self, class: AircraftClass) -> Result<(), String> {
        for (name, value) in [
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
        let widest = 1.0 - crate::ENGINE_SCALE_SEARCH_BRACKET.0;
        if !(self.engine_rating_window.is_finite()
            && self.engine_rating_window > 0.0
            && self.engine_rating_window <= widest)
        {
            return Err(format!(
                "design space engine_rating_window must be a fraction in (0, {widest}], the end of the sizing search"
            ));
        }
        for name in self.fixed_variable_names() {
            if !SPECS.iter().any(|spec| spec.name == name) {
                return Err(format!("fixed variable '{name}' is not a design variable"));
            }
        }
        self.validate_explicit_space(class)
    }

    /// Reject explicit start values and bounds that name no design variable,
    /// are not finite, are inverted, or put a start outside its own bound.
    fn validate_explicit_space(&self, class: AircraftClass) -> Result<(), String> {
        let known = |name: &str| SPECS.iter().any(|spec| spec.name == name);
        for (name, value) in &self.initial_design {
            if !known(name) || !value.is_finite() {
                return Err(format!(
                    "design space initial_design entry '{name}' must name a design variable and be finite"
                ));
            }
        }
        for (name, &(lower, upper)) in &self.bounds {
            if !known(name) || !lower.is_finite() || !upper.is_finite() || lower > upper {
                return Err(format!(
                    "design space bounds entry '{name}' must name a design variable with finite lower <= upper"
                ));
            }
            // The guardrails of the aircraft's class bound an explicit box too.
            if let Some(spec) = SPECS.iter().find(|spec| spec.name == name) {
                let (guard_lower, guard_upper) = spec.guardrails_for(class);
                if lower < guard_lower || upper > guard_upper {
                    return Err(format!(
                        "design space bounds entry '{name}' [{lower}, {upper}] leaves the design-variable guardrails [{guard_lower}, {guard_upper}]"
                    ));
                }
            }
            if let Some(value) = self.initial_design.get(name) {
                if *value < lower || *value > upper {
                    return Err(format!(
                        "design space initial_design entry '{name}' lies outside its explicit bounds"
                    ));
                }
            }
        }
        Ok(())
    }

    /// `design` with the explicit start values applied, and every variable
    /// with an explicit bound clamped into it.
    pub fn apply_explicit_start(&self, design: DesignVector) -> DesignVector {
        let mut values = design.to_array();
        for (index, spec) in SPECS.iter().enumerate() {
            if let Some(value) = self.initial_design.get(spec.name) {
                values[index] = *value;
            }
            if let Some(&(lower, upper)) = self.bounds.get(spec.name) {
                if lower <= upper {
                    values[index] = values[index].clamp(lower, upper);
                }
            }
        }
        DesignVector::from_array(&values).unwrap_or(design)
    }

    /// The per-variable envelope the search runs over, from `nominal`.
    ///
    /// Clean sheet: the global bounds of every variable, with the fuselage
    /// length fixed at the nominal when the cabin sizes it. Reference
    /// adaptation: the configured windows around `nominal`, intersected with
    /// the global bounds, and the listed variables fixed. A nominal outside
    /// its global bounds keeps a window around itself rather than being
    /// snapped: the reference aircraft is the truth the study starts from.
    ///
    /// The windows are bounded by the transport guardrails;
    /// [`Self::envelope_for`] bounds them by another class's.
    pub fn envelope(&self, nominal: &DesignVector) -> Vec<VariableEnvelope> {
        self.envelope_for(nominal, AircraftClass::Transport)
    }

    /// [`Self::envelope`] for an aircraft of `class`, whose guardrails bound
    /// the reference windows.
    pub fn envelope_for(
        &self,
        nominal: &DesignVector,
        class: AircraftClass,
    ) -> Vec<VariableEnvelope> {
        let values = nominal.to_array();
        let fixed = self.fixed_variable_names();
        SPECS
            .iter()
            .zip(values)
            .map(|(spec, value)| match self.mode {
                DesignMode::CleanSheet => {
                    let fixed = spec.name == TWIST_VARIABLE
                        || SECTION_SHAPE_VARIABLES.contains(&spec.name)
                        || (spec.name == "fuselage_length_m" && self.fuselage_sized_by_cabin);
                    self.clean_sheet_envelope(spec, value, fixed)
                }
                DesignMode::ReferenceAdaptation => {
                    let derived = spec.name == TAIL_SCALE_VARIABLE;
                    self.reference_envelope(
                        spec,
                        value,
                        derived
                            || spec.name == TWIST_VARIABLE
                            || SECTION_SHAPE_VARIABLES.contains(&spec.name)
                            || fixed.contains(&spec.name),
                        class,
                    )
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
        class: AircraftClass,
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
        let half_width = reference_half_width(self, spec, value);
        let (guard_lower, guard_upper) = spec.guardrails_for(class);
        VariableEnvelope {
            name: spec.name,
            nominal: value,
            lower: (value - half_width).max(guard_lower),
            upper: (value + half_width).min(guard_upper),
            fixed: half_width <= 0.0,
        }
    }
}

/// Half-width of the reference-adaptation window `config` searches around
/// `value` of the variable `spec`, in its native units.
pub(super) fn reference_half_width(
    config: &DesignSpaceConfig,
    spec: &DesignVariableSpec,
    value: f64,
) -> f64 {
    match window_kind(spec.name) {
        WindowKind::Fraction => config.reference_fraction_half_width * value.abs(),
        WindowKind::Angle => config.reference_angle_half_width_deg,
        WindowKind::Shift => config.reference_shift_half_width_m,
        WindowKind::Bump => config.reference_bump_half_width,
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
    fn every_search_holds_the_section_shape_and_a_reference_adaptation_the_listed_variables() {
        let nominal = DesignVector {
            bump_upper_rear: 0.001,
            bump_lower_rear: 0.001,
            ..DesignVector::default()
        };
        let variable = |config: &DesignSpaceConfig, name: &str| {
            config
                .envelope(&nominal)
                .into_iter()
                .find(|v| v.name == name)
                .unwrap()
        };
        let reference = DesignSpaceConfig {
            mode: DesignMode::ReferenceAdaptation,
            ..Default::default()
        };
        let freed = DesignSpaceConfig {
            mode: DesignMode::ReferenceAdaptation,
            reference_fixed_variables: "fuselage_length_m, tail_x_shift_m, sweep_deg".to_owned(),
            ..Default::default()
        };
        // The section shape is held in every mode, listed or not.
        for config in [&reference, &freed, &DesignSpaceConfig::default()] {
            for name in SECTION_SHAPE_VARIABLES {
                let held = variable(config, name);
                assert!(
                    held.fixed && held.lower == held.upper,
                    "{:?} {name}",
                    config.mode
                );
            }
        }
        assert_eq!(variable(&freed, "bump_upper_rear").lower, 0.001);
        // A listed planform variable is held; unlisted, it moves.
        assert!(!variable(&reference, "sweep_deg").fixed);
        assert!(variable(&freed, "sweep_deg").fixed);
        assert!(DesignSpaceConfig::default().validate().is_ok());
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
    fn reference_mode_derives_the_tail_scale_and_clean_sheet_searches_it() {
        let reference = DesignSpaceConfig {
            mode: DesignMode::ReferenceAdaptation,
            reference_fixed_variables: String::new(),
            ..Default::default()
        };
        let tail = |config: &DesignSpaceConfig| {
            config
                .envelope(&DesignVector::default())
                .into_iter()
                .find(|v| v.name == "tail_scale")
                .unwrap()
        };
        let fixed = tail(&reference);
        assert!(fixed.fixed && fixed.lower == fixed.upper);
        let clean = tail(&DesignSpaceConfig::default());
        assert!(!clean.fixed && clean.lower < clean.upper);
    }

    #[test]
    fn an_unknown_fixed_variable_is_rejected() {
        let config = DesignSpaceConfig {
            reference_fixed_variables: "span, wing_x_shift_m".to_owned(),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn only_a_clean_sheet_run_sizes_its_engine_and_the_default_window_is_jenkinsons() {
        let mut config = DesignSpaceConfig::default();
        assert!(config.engine_sized_by_requirements);
        assert!(config.sizes_engine());
        assert_eq!(config.engine_rating_window, 0.20);
        for mode in [DesignMode::ReferenceAdaptation, DesignMode::BaselineSandbox] {
            config.mode = mode;
            assert!(!config.sizes_engine(), "{mode:?}");
        }
        config.mode = DesignMode::CleanSheet;
        config.engine_sized_by_requirements = false;
        assert!(!config.sizes_engine());
    }

    #[test]
    fn the_engine_window_must_be_a_fraction_up_to_the_end_of_the_search() {
        for window in [0.0, -0.1, 0.65, 1.0, 1.5, f64::NAN, f64::INFINITY] {
            let config = DesignSpaceConfig {
                engine_rating_window: window,
                ..Default::default()
            };
            assert!(config.validate().is_err(), "window {window}");
        }
        let wide = DesignSpaceConfig {
            engine_rating_window: 0.35,
            ..Default::default()
        };
        assert!(wide.validate().is_ok());
    }

    #[test]
    fn a_saved_group_without_the_engine_fields_loads_with_their_defaults() {
        let config: DesignSpaceConfig =
            serde_json::from_value(serde_json::json!({"mode": "clean_sheet"})).unwrap();
        assert!(config.engine_sized_by_requirements);
        assert_eq!(config.engine_rating_window, 0.20);
        assert!(config.is_default());
    }

    #[test]
    fn holding_the_aircraft_as_drawn_stops_both_derivations() {
        let mut config = DesignSpaceConfig::default();
        config.hold_as_drawn();
        assert!(!config.sizes_fuselage_from_cabin());
        assert!(!config.sizes_engine());
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
