// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/physics_config.py

//! Coefficients of the semi-empirical drag buildup.
//!
//! The parasite-drag estimate is Raymer's component method and the transonic
//! rise is the Korn equation, and both are parameterized by a handful of
//! technology and interference factors. In the scripts this program grew out
//! of these were inline literals, `* 1.10`, `0.95`, `radians(32)`, which
//! made the fidelity assumptions invisible and unadjustable. Gathering them
//! here is what makes them either.
//!
//! References for the values themselves are in each field's explanation:
//! D. P. Raymer, *Aircraft Design: A Conceptual Approach*, for the form and
//! interference factors, and the Korn equation as it is usually given for the
//! wave-drag rise.
//!
//! The Korn technology factor is not one of these coefficients: it follows
//! from the section technology the wing is declared with,
//! [`crate::AirfoilClass`] on `geometry.wing.airfoil_class`.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Tunable coefficients for the semi-empirical drag buildup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct DragModelConfig {
    /// Exclude the main-wing center section shielded by the fuselage.
    #[serde(default = "exclude_buried_main_wing_area_default")]
    #[config(
        label = "Exclude buried main-wing area",
        help = "Subtract the tapered center section inside the local fuselage width at the root quarter chord from the parasite wetted-area estimate (NASA NDARC exposed-area convention). This is a local body approximation, not a surface intersection. Disable to replay the gross-wing-area convention."
    )]
    pub exclude_buried_main_wing_area: bool,
    /// Chordwise location of maximum airfoil thickness.
    #[config(
        label = "Max-thickness chordwise location",
        unit = "x/c",
        help = "Chordwise location of maximum airfoil thickness (as a fraction of chord), used in the wing form factor."
    )]
    pub max_thickness_chordwise_loc: f64,

    /// How much the wing-fuselage junction disturbs the local flow.
    #[config(
        label = "Wing interference factor (Q)",
        help = "How much the wing-fuselage junction disturbs the local flow, multiplying the wing's parasite drag. 1.0 = no interference."
    )]
    pub interference_factor_wing: f64,

    /// How much neighbouring components disturb the fuselage's flow.
    ///
    /// Read only by the frozen reference-compatibility drag buildup; the
    /// product path uses the constant `FUSELAGE_INTERFERENCE` = 1.0 (Raymer
    /// 12.5.5) and ignores this field, so changing it does not change a
    /// product result.
    #[config(
        label = "Fuselage interference factor (Q)",
        help = "How much neighbouring components (wing, tail) disturb the fuselage's flow, multiplying its parasite drag."
    )]
    pub interference_factor_fuselage: f64,

    /// Lumped multiplier covering what a component-by-component buildup
    /// cannot see.
    #[config(
        label = "Viscous drag margin",
        help = "Lumped multiplier applied to the total parasite drag, accounting for excrescences, gaps and roughness not captured component-by-component. Default 1.00: refitted with the lift-dependent viscous fraction (0.25) on the A320-200, A340-300 and B787-9 payload-range corners after the sourced engine anchors and the bleed and power-extraction loss (fit 0.98, band 0.94 to 1.02, so 1.00 is the physical lower bound); Jenkinson's secondary-drag allowance of up to 10 % is not supported by that set, so it is a calibrated level; the frozen 1.10 applies with the fraction at 0."
    )]
    pub viscous_margin: f64,

    /// Lift-dependent viscous (profile and interference) drag, as a
    /// fraction of the clean zero-lift drag per unit `CL^2`.
    #[serde(default, skip_serializing_if = "is_zero_fraction")]
    #[config(
        label = "Lift-dependent viscous drag fraction",
        unit = "1/CL^2",
        help = "Adds CD_v = fraction * CD0 * CL^2 to the drag polar: the lift-dependent profile and interference drag that an inviscid wake-energy induced drag cannot see, CDp = CD0 (1 + k_L CL^2) (total Oswald factor 1/e = 1/e_inviscid + fraction * CD0 * pi * A). Default 0.25 from Jenkinson, Civil Jet Aircraft Design (1999), Ch. 8, pp. 179-180: dCD/dCL^2 = 0.35 CD0 for older-technology designs (B737-300, B757, B767) and 0.15 CD0 for advanced-technology designs (A330/A340, B777), the A320 in between; the default is the midpoint of that printed range because the product carries no technology class (a rule on the printed range, not a fit). Applies to the clean (cruise and climb) polar only; take-off and landing configurations keep their own drag increments. 0 disables the term and keeps the lumped Viscous drag margin as the only viscous correction; when it is non-zero the margin must be recalibrated so the cruise point is not counted twice."
    )]
    pub lift_dependent_viscous_fraction: f64,

    /// How much the nacelle and pylon disturb the local flow.
    #[config(
        label = "Nacelle/pylon interference factor (Q)",
        help = "How much the engine nacelle and pylon disturb the local flow. Typical value 1.3 for podded under-wing installations (Raymer)."
    )]
    pub interference_factor_nacelle: f64,

    /// Below this Mach number, wave drag is taken as zero.
    #[config(
        label = "Wave-drag onset Mach",
        help = "Below this Mach number, transonic wave drag is assumed zero (not yet computed)."
    )]
    pub wave_drag_onset_mach: f64,

    /// Leading constant of the Korn wave-drag rise.
    #[config(
        label = "Wave-drag rise coefficient",
        help = "Leading constant in the Lock/Korn wave-drag rise: CD_wave = coefficient * max(M - M_critical, 0)^4. The critical Mach is M_drag_divergence - (0.1 / (4 * coefficient))^(1/3); coefficient must be positive."
    )]
    pub wave_drag_coefficient: f64,
}

/// Default lift-dependent viscous fraction `k_L`, per unit `CL^2`.
///
/// Jenkinson, *Civil Jet Aircraft Design* (1999), Ch. 8, pp. 179-180 (PDF p.
/// 194-195 of the library copy): the viscous part of the lift-dependent drag
/// is proportional to the profile drag, `dCD/dCL^2 = 0.35 CD0` for older
/// technology designs (B737-300, B757, B767) and `0.15 CD0` for advanced
/// technology designs (A330/A340, B777), the A320 lying in between. The
/// product has no technology-class input, so one global value is used: the
/// midpoint of the printed range, 0.25 (owner decision 2026-10-13; a rule on
/// the printed range, not a fit). The 0.15 and 0.35 ends move the fitted
/// margin by +0.024 and -0.022. It replaces Torenbeek's 0.43 (2013, p. 96,
/// Eq. 4.21, a whole-airplane en-route value). Applied to the clean polar
/// only; take-off and landing configurations keep their own drag increments.
pub const DEFAULT_LIFT_DEPENDENT_VISCOUS_FRACTION: f64 = 0.25;

/// Default viscous margin, fitted with [`DEFAULT_LIFT_DEPENDENT_VISCOUS_FRACTION`]
/// on the calibration presets (A320-200, A340-300, B787-9) against their
/// payload-range corner ranges on the merged tip 65303100; fit 1.0051, band
/// 0.967 to 1.043 (Delta chi^2 = 1), plus the +0.024 / -0.022 shift over the
/// fraction range. The hold-out presets were not read while fitting. It
/// replaces the frozen 1.10, which is only valid with the fraction at 0.
///
/// PROVISIONAL refit (fix/fuel-chain-2026-10-14, kappa 0.25, same protocol and
/// CAL set) after the cruise TSFC anchors became the printed public values
/// (CFM56 0.545, CF6-80C2 0.564), the bleed and power-extraction loss entered
/// the engine deck, the LTO part-power dip left the cruise bucket and the
/// cruise Mach became the requirement's: m_hat 0.9805, band 0.936 to 1.023,
/// J_min 7.17 on 5 dof (final model, the internal fuel-chain calibration run `cal/v2`: 0.9814, band
/// 0.937 to 1.024). A value below 1 would be a negative excrescence
/// allowance, so the physical lower bound 1.00 (inside the band, J 7.3) is
/// adopted. Jenkinson's up-to-10 % allowance (m 1.10) gives J 15.5. The final
/// refit follows the merge of the parallel lanes (empty-mass changes).
pub const DEFAULT_VISCOUS_MARGIN: f64 = 1.00;

impl Default for DragModelConfig {
    fn default() -> Self {
        Self {
            exclude_buried_main_wing_area: true,
            max_thickness_chordwise_loc: 0.35,
            interference_factor_wing: 1.0,
            interference_factor_fuselage: 1.25,
            viscous_margin: DEFAULT_VISCOUS_MARGIN,
            lift_dependent_viscous_fraction: DEFAULT_LIFT_DEPENDENT_VISCOUS_FRACTION,
            interference_factor_nacelle: 1.3,
            wave_drag_onset_mach: 0.6,
            wave_drag_coefficient: 20.0,
        }
    }
}

fn exclude_buried_main_wing_area_default() -> bool {
    true
}

fn is_zero_fraction(value: &f64) -> bool {
    *value == 0.0
}

impl DragModelConfig {
    /// The lumped margin that keeps the drag at lift coefficient `cl_ref`
    /// unchanged when `fraction` of lift-dependent viscous drag is switched
    /// on: `margin_old / (1 + fraction * cl_ref^2)`, from
    /// `m_new CD0 (1 + fraction CL^2) = m_old CD0` at `CL = cl_ref`.
    ///
    /// This is the analytic neutral point, not a calibration: the calibrated
    /// margin comes from the calibration presets alone (FIX-PLAN section 3;
    /// procedure in the lane report). Returns `None` for a non-finite or
    /// negative input.
    pub fn neutral_margin(margin_old: f64, fraction: f64, cl_ref: f64) -> Option<f64> {
        let denominator = 1.0 + fraction * cl_ref * cl_ref;
        (margin_old.is_finite() && fraction.is_finite() && fraction >= 0.0 && cl_ref.is_finite())
            .then(|| margin_old / denominator)
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entry, Kind};

    #[test]
    fn every_field_reaches_the_schema_in_declaration_order() {
        let schema = DragModelConfig::default().schema();
        let names: Vec<&str> = schema.fields.iter().map(|field| field.name).collect();
        assert_eq!(
            names,
            vec![
                "exclude_buried_main_wing_area",
                "max_thickness_chordwise_loc",
                "interference_factor_wing",
                "interference_factor_fuselage",
                "viscous_margin",
                "lift_dependent_viscous_fraction",
                "interference_factor_nacelle",
                "wave_drag_onset_mach",
                "wave_drag_coefficient",
            ]
        );
    }

    #[test]
    fn a_field_with_no_declared_unit_takes_one_from_its_name() {
        // None of these names end in a unit suffix, so all of them are
        // dimensionless, which is the point: the derived unit is the empty
        // string rather than a guess.
        let schema = DragModelConfig::default().schema();
        let viscous = schema.field("viscous_margin").unwrap();
        assert_eq!(viscous.unit, "");
        assert_eq!(viscous.label, "Viscous drag margin");
    }

    #[test]
    fn the_neutral_margin_keeps_the_reference_lift_drag_unchanged() {
        let (m_old, kappa, cl) = (1.10, 0.30, 0.5);
        let m_new = DragModelConfig::neutral_margin(m_old, kappa, cl).unwrap();
        assert!((m_new * (1.0 + kappa * cl * cl) - m_old).abs() < 1e-15);
        // No lift-dependent term: the margin is untouched.
        assert_eq!(DragModelConfig::neutral_margin(m_old, 0.0, cl), Some(m_old));
        assert_eq!(DragModelConfig::neutral_margin(m_old, f64::NAN, cl), None);
        assert_eq!(DragModelConfig::neutral_margin(m_old, -0.1, cl), None);
    }

    #[test]
    fn the_default_fraction_is_the_midpoint_of_jenkinsons_range_and_zero_is_not_serialized() {
        // Jenkinson (1999) p. 180: 0.15 CD0 (advanced) to 0.35 CD0 (older).
        assert_eq!(DEFAULT_LIFT_DEPENDENT_VISCOUS_FRACTION, (0.15 + 0.35) / 2.0);
        let defaults = DragModelConfig::default();
        assert_eq!(defaults.lift_dependent_viscous_fraction, 0.25);
        assert_eq!(defaults.viscous_margin, DEFAULT_VISCOUS_MARGIN);
        let value = serde_json::to_value(&defaults).unwrap();
        assert_eq!(value["lift_dependent_viscous_fraction"], 0.25);
        // A saved configuration that carries no fraction keeps the frozen
        // lumped-margin convention (fraction 0), and zero is not written back.
        let off = DragModelConfig {
            lift_dependent_viscous_fraction: 0.0,
            ..Default::default()
        };
        let value = serde_json::to_value(off).unwrap();
        assert!(value.get("lift_dependent_viscous_fraction").is_none());
        let mut legacy = serde_json::to_value(DragModelConfig::default()).unwrap();
        legacy
            .as_object_mut()
            .unwrap()
            .remove("lift_dependent_viscous_fraction");
        let loaded: DragModelConfig = serde_json::from_value(legacy).unwrap();
        assert_eq!(loaded.lift_dependent_viscous_fraction, 0.0);
    }

    #[test]
    fn a_declared_unit_survives_into_the_schema() {
        let schema = DragModelConfig::default().schema();
        assert_eq!(
            schema.field("max_thickness_chordwise_loc").unwrap().unit,
            "x/c"
        );
    }

    #[test]
    fn the_schema_carries_the_current_values_not_the_defaults() {
        let config = DragModelConfig {
            viscous_margin: 1.5,
            ..Default::default()
        };
        let schema = config.schema();
        match &schema.field("viscous_margin").unwrap().entry {
            Entry::Leaf(leaf) => {
                assert_eq!(leaf.kind, Kind::Float);
                assert_eq!(leaf.value, serde_json::json!(1.5));
            }
            Entry::Node(_) => panic!("a real number is not a group"),
        }
    }
}
