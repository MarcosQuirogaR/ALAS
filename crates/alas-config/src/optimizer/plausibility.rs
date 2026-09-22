// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Limits that keep an optimized design a recognisable transport aeroplane.
//!
//! These are not performance requirements and they are not preferences. They
//! are the shape of the model's own validity: the mass, drag and stability
//! correlations the disciplines are built from were fitted to conventional
//! tube-and-wing transports, and a search that walks outside that shape stops
//! being wrong in a way the residual table can see. A wing whose aspect ratio
//! doubles still gets a plausible induced-drag credit from the lattice and a
//! plausible-looking wingbox mass from the correlation, so the objective keeps
//! improving while the aeroplane stops existing. The same is true of a
//! fuselage stretched past any fineness ratio ever flown, of a tail placed at
//! an arm the fuselage cannot carry, and of a planform whose trailing-edge
//! break runs the wrong way.
//!
//! Each limit below is therefore a *validity domain* statement about the
//! model, expressed as a window that every registered aircraft in this
//! program sits comfortably inside. They are engineering choices, informed by
//! the span of conventional transports, not certification limits and not
//! measured boundaries of the correlations; the residuals they produce are
//! reported by name so a rejected candidate says which one it left.
//!
//! # Units and conventions
//!
//! Aspect ratio is `b_ref^2 / S_ref` on the projected XY reference
//! quantities, dimensionless. Fuselage fineness is overall length divided by
//! the largest cross-section equivalent diameter, both metres, dimensionless.
//! The tail-arm fraction is the distance from the main wing's quarter-chord
//! aerodynamic centre to the horizontal tail's, divided by the fuselage
//! length, dimensionless and positive aft. Chord ratios are dimensionless.
//! Root thickness is the section's maximum thickness as a fraction of the
//! root chord.
//!
//! # Where these are enforced
//!
//! `alas_opt::mdo::residuals_geometry` turns them into named residuals in the
//! Geometry family, so they follow that family's configured policy and the
//! constraint-relaxation rules with every other geometry requirement. They
//! are not a separate rejection path.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Validity-domain limits on the shape of an optimized design.
///
/// The group reaches Advanced Settings as a form, so every field carries a
/// label, a help sentence and, where the quantity has one, a unit. The
/// dimensionless ratios state "dimensionless" in their help rather than
/// carrying an empty unit chip, and both twist bounds are degrees.
///
/// # Serialization
///
/// `#[serde(default, deny_unknown_fields)]` on this group and
/// `skip_serializing_if = "PlausibilityLimits::is_default"` on the field that
/// holds it mean a saved document contains the group only when the user has
/// changed something, and a document written before the group existed loads
/// with the shipped defaults rather than with zeros. An unknown key inside
/// the group is an error, not a silently ignored typo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(default, deny_unknown_fields)]
pub struct PlausibilityLimits {
    /// Whether the limits are enforced at all.
    #[config(
        label = "Enforce the validity domain",
        help = "On by default. Off lets the search leave the shape window these correlations were fitted for; the objective keeps improving while the aeroplane stops existing, so the residual table can no longer be read as a statement about an aircraft."
    )]
    pub enabled: bool,

    /// Smallest wing aspect ratio, `b^2 / S`, accepted as a transport wing.
    #[config(
        label = "Minimum wing aspect ratio",
        help = "Smallest b^2/S accepted, on the projected reference span and area, dimensionless. Below this the lifting-line and wingbox correlations are extrapolating. Every registered aircraft sits well above it."
    )]
    pub min_aspect_ratio: f64,

    /// Largest wing aspect ratio accepted.
    ///
    /// Conventional cantilever transports run from about 7.5 (early
    /// narrowbodies) to about 11 (787, A350). The ceiling is set well above
    /// that because a higher aspect ratio is a legitimate design direction;
    /// what it excludes is the unbounded span the induced-drag credit would
    /// otherwise reward, since neither the wingbox correlation nor the
    /// aeroelastic model here remains meaningful there.
    #[config(
        label = "Maximum wing aspect ratio",
        help = "Largest b^2/S accepted, dimensionless. Conventional cantilever transports run from about 7.5 to about 11; the ceiling is set well above that because a higher aspect ratio is a legitimate design direction. What it excludes is the unbounded span the induced-drag credit would otherwise reward, where neither the wingbox correlation nor the aeroelastic model remains meaningful."
    )]
    pub max_aspect_ratio: f64,

    /// Smallest fuselage fineness ratio (length over equivalent diameter).
    #[config(
        label = "Minimum fuselage fineness ratio",
        help = "Smallest overall length divided by the largest cross-section equivalent diameter, both in metres, dimensionless. Below this the body is too stubby for the skin-friction and structure correlations fitted to transport fuselages."
    )]
    pub min_fuselage_fineness: f64,

    /// Largest fuselage fineness ratio.
    ///
    /// Transport fuselages sit near 8 to 12; the widest-body and
    /// longest-stretch variants reach about 14. Beyond the ceiling the
    /// skin-friction and structure correlations are extrapolating and the
    /// body would not meet the bending or ground-clearance cases this program
    /// does not model.
    #[config(
        label = "Maximum fuselage fineness ratio",
        help = "Largest length over equivalent diameter, dimensionless. Transport fuselages sit near 8 to 12 and the longest stretches reach about 14. Beyond the ceiling the skin-friction and structure correlations are extrapolating, and the body would not meet bending or ground-clearance cases this program does not model."
    )]
    pub max_fuselage_fineness: f64,

    /// Smallest horizontal-tail arm as a fraction of fuselage length.
    #[config(
        label = "Minimum horizontal-tail arm fraction",
        help = "Smallest distance from the main wing quarter-chord aerodynamic centre to the horizontal tail quarter-chord, divided by fuselage length, dimensionless and positive aft. Below this the tail has too little arm for the pitch authority the trim solution assumes."
    )]
    pub min_tail_arm_fraction: f64,

    /// Largest horizontal-tail arm as a fraction of fuselage length.
    ///
    /// The tail cannot be mounted beyond the body that carries it. A
    /// conventional transport puts the horizontal tail's quarter chord at
    /// roughly 0.40 to 0.55 of body length aft of the wing's; the window here
    /// is wider and exists to stop the search buying pitch authority with an
    /// arm the airframe does not have.
    #[config(
        label = "Maximum horizontal-tail arm fraction",
        help = "Largest tail arm as a fraction of fuselage length, dimensionless. The tail cannot be mounted beyond the body that carries it: a conventional transport sits near 0.40 to 0.55, and this wider window stops the search buying pitch authority with an arm the airframe does not have."
    )]
    pub max_tail_arm_fraction: f64,

    /// Smallest tip-to-root chord ratio of the main wing.
    ///
    /// A vanishing tip chord is where the lattice's local Reynolds number and
    /// the section data stop being defined, and it is a shape no transport
    /// wing has. The floor is below every registered aircraft's value.
    #[config(
        label = "Minimum tip-to-root chord ratio",
        help = "Smallest tip chord divided by root chord, dimensionless. A vanishing tip chord is where the lattice local Reynolds number and the section data stop being defined, and it is a shape no transport wing has. The floor is below every registered aircraft value."
    )]
    pub min_tip_root_chord_ratio: f64,

    /// Largest tip-to-root chord ratio of the main wing.
    #[config(
        label = "Maximum tip-to-root chord ratio",
        help = "Largest tip chord divided by root chord, dimensionless. Above this the wing is closer to untapered than to a transport planform, and the spanwise load the correlations assume no longer holds."
    )]
    pub max_tip_root_chord_ratio: f64,

    /// Whether the trailing-edge break chord must lie between the tip and
    /// root chords.
    ///
    /// The yehudi break is what joins the inboard box carrying the
    /// centre-section load to the outboard panel. A break chord outside the
    /// tip-to-root interval is not a blend: it is either a discontinuity that
    /// the built geometry renders as a notch, or an inversion where the wing
    /// grows outboard of the root. Neither is an aeroplane, and both are
    /// scored as ordinary wings by the correlations.
    #[config(
        label = "Require an ordered trailing-edge break",
        help = "On by default. The yehudi break joins the inboard box carrying the centre-section load to the outboard panel, so its chord must lie between the tip and root chords. A break chord outside that interval is either a notch in the built geometry or an inversion where the wing grows outboard of the root, and the correlations score both as ordinary wings."
    )]
    pub require_monotonic_planform_break: bool,

    /// Smallest main-wing root thickness-to-chord ratio.
    #[config(
        label = "Minimum root thickness-to-chord ratio",
        help = "Smallest maximum section thickness at the wing root as a fraction of root chord, dimensionless. Below this the section carries no spar box the structural model can size."
    )]
    pub min_root_thickness_ratio: f64,

    /// Largest main-wing root thickness-to-chord ratio.
    ///
    /// Transport roots run from about 0.12 to 0.16 on supercritical wings.
    /// The window is wider on both sides; it excludes the sections that carry
    /// no spar box at the thin end and that the compressible drag model
    /// cannot represent at the thick end.
    #[config(
        label = "Maximum root thickness-to-chord ratio",
        help = "Largest root thickness as a fraction of root chord, dimensionless. Transport roots run from about 0.12 to 0.16 on supercritical wings; the window is wider on both sides, and the ceiling excludes sections the compressible drag model cannot represent."
    )]
    pub max_root_thickness_ratio: f64,

    /// Most negative built geometric washout accepted, degrees.
    ///
    /// # What this measures, and what `tip_twist_deg` is not
    ///
    /// The quantity is the **built** wing's tip section incidence minus its
    /// root section incidence, negative for washout. It is deliberately not
    /// the `tip_twist_deg` design variable: despite that variable's label
    /// ("Geometric washout at tip"), the builder writes it as the tip
    /// section's *absolute* incidence, while the root takes
    /// `geometry.wing.root_twist_deg`, which every registered aircraft sets
    /// between +2.0 and +4.5 degrees. The washout is the difference.
    ///
    /// Measured on the eight registered design vectors and their own wing
    /// geometry (`tip_twist_deg - root_twist_deg`): AVE and ATR72-600 -4.0,
    /// A320-200 and A220-300 -4.5, DC-10 and B787-9 -5.5, A340-300 -6.0,
    /// A380-800 -7.0. The floor is set below the most twisted of them with
    /// margin, in keeping with every other window here.
    #[config(
        label = "Most negative built washout",
        unit = "deg",
        help = "Most negative built geometric washout accepted, in degrees: the built tip section incidence minus the root section incidence, negative for washout. This is not the tip twist design variable, which is the tip absolute incidence. Measured on the registered design vectors the built washout runs from -4.0 deg (AVE, ATR 72-600) to -7.0 deg (A380-800), and the floor sits below the most twisted of them with margin."
    )]
    pub min_tip_washout_deg: f64,

    /// Least negative built geometric washout accepted, degrees.
    ///
    /// Zero: a transport wing is not built with wash-in. The vortex-lattice
    /// model rewards loading the tip (it lowers induced drag at a fixed span
    /// and lets the trimmed attitude fall), and nothing in the aerodynamic or
    /// structural model here opposes it, because neither the stall
    /// progression nor the outboard gust/manoeuvre load case that washout
    /// exists to control is represented. Positive twist at the tip would
    /// therefore be bought from an omission in the model rather than earned.
    ///
    /// **This bound cannot be reached from the current design space**, and
    /// saying so is the point of writing it down. `tip_twist_deg` is bounded
    /// above at +1.0 degree and the smallest root incidence any registered
    /// aircraft or the default geometry carries is +2.0, so the most positive
    /// built washout reachable today is -1.0 degrees. The limit is a guard on
    /// the quantity, not a correction of an observed pathology: it exists so
    /// that widening the design variable, or a geometry document with no root
    /// incidence, cannot silently produce a wash-in wing that the objective
    /// would reward.
    ///
    /// This is an engineering choice about conventional transport practice,
    /// not a certification limit and not a measured boundary of the
    /// correlations.
    #[config(
        label = "Least negative built washout",
        unit = "deg",
        help = "Least negative built washout accepted, in degrees. Zero: a transport wing is not built with wash-in. The vortex-lattice model rewards loading the tip while neither the stall progression nor the outboard gust and manoeuvre case that washout exists to control is represented here, so positive tip twist would be bought from an omission in the model. The current design space cannot reach this bound; it guards the quantity rather than correcting an observed pathology."
    )]
    pub max_tip_washout_deg: f64,
}

impl Default for PlausibilityLimits {
    fn default() -> Self {
        Self {
            enabled: true,
            min_aspect_ratio: 4.0,
            max_aspect_ratio: 16.0,
            min_fuselage_fineness: 5.0,
            max_fuselage_fineness: 18.0,
            min_tail_arm_fraction: 0.15,
            max_tail_arm_fraction: 0.75,
            min_tip_root_chord_ratio: 0.05,
            max_tip_root_chord_ratio: 0.80,
            require_monotonic_planform_break: true,
            min_root_thickness_ratio: 0.06,
            max_root_thickness_ratio: 0.24,
            min_tip_washout_deg: -10.0,
            max_tip_washout_deg: 0.0,
        }
    }
}

impl PlausibilityLimits {
    /// Whether the serialized group equals the defaults.
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Reject a window the residual builder cannot evaluate.
    ///
    /// # Errors
    ///
    /// A description of the first invalid value.
    pub fn validate(&self) -> Result<(), String> {
        for (name, lower, upper) in [
            ("aspect_ratio", self.min_aspect_ratio, self.max_aspect_ratio),
            (
                "fuselage_fineness",
                self.min_fuselage_fineness,
                self.max_fuselage_fineness,
            ),
            (
                "tail_arm_fraction",
                self.min_tail_arm_fraction,
                self.max_tail_arm_fraction,
            ),
            (
                "tip_root_chord_ratio",
                self.min_tip_root_chord_ratio,
                self.max_tip_root_chord_ratio,
            ),
            (
                "root_thickness_ratio",
                self.min_root_thickness_ratio,
                self.max_root_thickness_ratio,
            ),
        ] {
            if !lower.is_finite() || !upper.is_finite() {
                return Err(format!("plausibility {name} window must be finite"));
            }
            if lower <= 0.0 {
                return Err(format!("plausibility {name} lower bound must be positive"));
            }
            if upper <= lower {
                return Err(format!(
                    "plausibility {name} window must satisfy lower < upper, got [{lower}, {upper}]"
                ));
            }
        }
        // The twist window is signed: washout is negative by the design
        // variable's own convention, so it is validated on its own terms
        // rather than under the positive-quantity rule above.
        if !self.min_tip_washout_deg.is_finite() || !self.max_tip_washout_deg.is_finite() {
            return Err("plausibility tip_washout window must be finite".to_owned());
        }
        if self.max_tip_washout_deg <= self.min_tip_washout_deg {
            return Err(format!(
                "plausibility tip_washout window must satisfy lower < upper, got [{}, {}]",
                self.min_tip_washout_deg, self.max_tip_washout_deg
            ));
        }
        Ok(())
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_are_on_and_internally_consistent() {
        let limits = PlausibilityLimits::default();
        assert!(limits.enabled);
        assert!(limits.validate().is_ok());
        assert!(limits.require_monotonic_planform_break);
    }

    #[test]
    fn every_registered_aircraft_planform_sits_inside_the_windows() {
        // The limits describe the validity domain of the correlations, so a
        // real aeroplane this program ships must never be excluded by them.
        // The design vector carries the planform directly; the built-geometry
        // quantities are checked in the optimizer's own residual tests.
        let limits = PlausibilityLimits::default();
        for preset in crate::presets::registry() {
            let design = preset.design_vector;
            let tip_root = design.tip_chord_m / design.root_chord_m;
            assert!(
                tip_root >= limits.min_tip_root_chord_ratio
                    && tip_root <= limits.max_tip_root_chord_ratio,
                "{}: tip/root {tip_root}",
                preset.name
            );
            assert!(
                design.break_chord_m <= design.root_chord_m
                    && design.break_chord_m >= design.tip_chord_m,
                "{}: break chord {} outside [{}, {}]",
                preset.name,
                design.break_chord_m,
                design.tip_chord_m,
                design.root_chord_m
            );
            let fineness = design.fuselage_length_m / preset.geometry.fuselage.diameter_m;
            assert!(
                fineness >= limits.min_fuselage_fineness
                    && fineness <= limits.max_fuselage_fineness,
                "{}: fineness {fineness}",
                preset.name
            );
            // The built washout, which is what the residual measures: the
            // tip section's incidence less the root's. `tip_twist_deg` alone
            // is the tip incidence and is *not* the washout, so comparing it
            // against this window would test the wrong quantity and pass for
            // the wrong reason.
            let washout_deg = design.tip_twist_deg - preset.geometry.wing.root_twist_deg;
            assert!(
                washout_deg >= limits.min_tip_washout_deg
                    && washout_deg <= limits.max_tip_washout_deg,
                "{}: built washout {washout_deg} (tip {} - root {}) outside the window [{}, {}]",
                preset.name,
                design.tip_twist_deg,
                preset.geometry.wing.root_twist_deg,
                limits.min_tip_washout_deg,
                limits.max_tip_washout_deg
            );
        }
    }

    #[test]
    fn the_washout_window_excludes_wash_in_and_rejects_an_inverted_one() {
        // The window stops at the flat wing, and its floor clears the most
        // twisted registered aircraft (A380-800, -7.0 deg built washout).
        let limits = PlausibilityLimits::default();
        assert_eq!(limits.max_tip_washout_deg, 0.0);
        assert!(limits.min_tip_washout_deg <= -8.0);
        assert!(limits.min_tip_washout_deg < limits.max_tip_washout_deg);
        assert!(limits.validate().is_ok());

        let inverted = PlausibilityLimits {
            min_tip_washout_deg: 1.0,
            ..Default::default()
        };
        assert!(inverted.validate().is_err());
        let nonfinite = PlausibilityLimits {
            max_tip_washout_deg: f64::NAN,
            ..Default::default()
        };
        assert!(nonfinite.validate().is_err());
    }

    #[test]
    fn an_inverted_window_is_rejected_rather_than_silently_ignored() {
        let limits = PlausibilityLimits {
            max_aspect_ratio: 3.0,
            ..Default::default()
        };
        assert!(limits.validate().is_err());
        let limits = PlausibilityLimits {
            min_fuselage_fineness: 0.0,
            ..Default::default()
        };
        assert!(limits.validate().is_err());
    }

    #[test]
    fn the_group_round_trips_and_an_old_document_gets_the_defaults() {
        let limits = PlausibilityLimits {
            max_aspect_ratio: 12.5,
            ..Default::default()
        };
        let encoded = serde_json::to_value(&limits).unwrap();
        let decoded: PlausibilityLimits = serde_json::from_value(encoded).unwrap();
        assert_eq!(decoded, limits);
        let empty: PlausibilityLimits = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(empty, PlausibilityLimits::default());
    }
}
