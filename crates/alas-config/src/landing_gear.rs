// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/landing_gear_config.py
// Reference: alas @ rust-port-baseline.

//! Wheel, tire and strut sizing assumptions.
//!
//! These drive a real wheel-and-tire sizing pass rather than a fraction of
//! takeoff weight: the number of wheels and their rated load are what produce
//! the gear load limits, which in turn produce the strength boundaries of the
//! centre-of-gravity envelope. The distinction matters because the envelope
//! then reflects gear the aircraft could actually be built with, rather than
//! an assumption about gear nobody sized.
//!
//! Every count here accepts zero, meaning "size it": a wheel count is a
//! discrete choice made from the load, and asking a user to pick one before
//! the load is known has the causality backwards. A non-zero value overrides
//! the sizing, which is what makes an existing aircraft's gear reproducible.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Tunable landing-gear sizing assumptions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct LandingGearConfig {
    /// Margin left in the rated tire load after the static reaction.
    #[config(
        label = "Tire load safety factor",
        help = "Margin applied to the static reaction load when selecting/verifying tire count -- real gear is sized so the rated tire load is never fully consumed by static load alone, leaving margin for dynamic (braking, turning, rough-field) loads. Raymer: ~1.07 typical for a preliminary sizing pass."
    )]
    pub tire_safety_factor: f64,

    /// Wheels on the nose gear, or zero to size it.
    #[config(
        label = "Nose-gear wheel count (0 = auto)",
        help = "Wheels on the nose gear strut. 0 = auto: 1 for light aircraft, 2 (the near-universal choice for CS-25/FAR-25 transports) once MTOW exceeds nlg_dual_wheel_mtow_kg."
    )]
    pub n_nlg_wheels: i64,

    /// Where auto-sizing switches to a twin nose wheel.
    #[config(
        label = "MTOW threshold for dual nose wheels",
        unit = "kg",
        help = "Auto-sizing switches from a single to a dual (twin) nose wheel above this MTOW -- below it, transport-category aircraft still commonly fly single nose wheels."
    )]
    pub nlg_dual_wheel_mtow_kg: f64,

    /// Main-gear legs, left and right combined, or zero to size them.
    #[config(
        label = "Main-gear strut count (0 = auto)",
        help = "Number of main-gear legs (each with its own wheel bogie), left+right combined. 0 = auto: 2 (one per side) below mlg_body_gear_mtow_kg, 4 (adds centreline body gear, e.g. A380/747-class) above it -- real widebodies above roughly 300 t add body gear because a two-leg bogie would need an impractically large tire count/track width to carry the load within tire-pressure limits."
    )]
    pub n_mlg_struts: i64,

    /// Where auto-sizing adds centreline body gear.
    #[config(
        label = "MTOW threshold for body (centreline) main gear",
        unit = "kg",
        help = "Auto-sizing adds two centreline body-gear legs (4 main legs total) above this MTOW."
    )]
    pub mlg_body_gear_mtow_kg: f64,

    /// Wheels on each main-gear leg, or zero to size them.
    #[config(
        label = "Wheels per main-gear strut (0 = auto)",
        help = "0 = auto: the smallest of {2, 4, 6} standard bogie sizes whose rated capacity (tire_safety_factor-derated) covers this strut's static reaction load at the aft CG limit."
    )]
    pub wheels_per_mlg_strut: i64,

    /// Main-gear track as a multiple of fuselage diameter.
    #[config(
        label = "Main-gear track / fuselage-diameter factor",
        help = "Main-gear lateral track width, as a multiple of fuselage diameter. Real transports with wing-root-mounted main gear run track/diameter ~1.75-2.0 (777-300ER 2.03, 787-9 1.90, A340-300 1.91, A380-800 2.00, A320-200 1.92, DC-10-30 1.77) -- 1.85 is the fleet-average calibration. An earlier default (1.15) understated real track width by roughly a factor of 1.6, which fed directly into the lateral-turnover check (physics.landing_gear) reading artificially safe."
    )]
    pub track_diameter_factor: f64,

    /// Which reference tire the sizing works from.
    #[config(
        options = TireClass,
        label = "Tire class",
        help = "Which reference tire (see physics.landing_gear.TIRE_DATABASE) to size with -- 'auto' picks the smallest class whose rated load, combined with a realistic wheel count (<=6/strut), covers the aircraft's static gear loads. Options: auto, light, narrowbody, widebody, heavy."
    )]
    pub tire_class: String,

    /// What the strut is made of.
    #[config(
        options = StrutMaterial,
        label = "Strut material",
        help = "Landing-gear strut/piston material, shown on the planform diagram and in the design report. 'auto' selects by MTOW class (see physics.landing_gear.STRUT_MATERIALS): high-strength steel (300M-class) for larger transports, an aluminium/steel combination for light aircraft. Informational/labelling only -- this preliminary-design tool does not run a structural (FEA) stress analysis of the strut itself."
    )]
    pub strut_material: String,

    /// The lateral tip-over criterion.
    #[config(
        label = "Max lateral turnover angle",
        unit = "deg",
        help = "Lateral tip-over (overturn) criterion, Raymer Ch.11 / Currey convention: the angle from the vertical whose tangent is CG height over the CG's perpendicular distance to the nose-gear-to-main-gear ground line must not exceed this (evaluated at the forward CG limit, the worst case), or the aircraft risks tipping over in a tight turn. 63 deg is the standard transport-category limit; a higher CG, narrower track, or more forward CG all push the angle up toward it."
    )]
    pub turnover_angle_limit_deg: f64,
}

impl Default for LandingGearConfig {
    fn default() -> Self {
        Self {
            tire_safety_factor: 1.07,
            n_nlg_wheels: 0,
            nlg_dual_wheel_mtow_kg: 15_000.0,
            n_mlg_struts: 0,
            mlg_body_gear_mtow_kg: 300_000.0,
            wheels_per_mlg_strut: 0,
            track_diameter_factor: 1.85,
            tire_class: "auto".to_owned(),
            strut_material: "auto".to_owned(),
            turnover_angle_limit_deg: 63.0,
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Entry, OptionSource};

    #[test]
    fn every_count_defaults_to_being_sized_rather_than_asserted() {
        // A wheel count follows from the load, so the shipped default has to
        // be the one that lets the sizing decide.
        let config = LandingGearConfig::default();
        assert_eq!(config.n_nlg_wheels, 0);
        assert_eq!(config.n_mlg_struts, 0);
        assert_eq!(config.wheels_per_mlg_strut, 0);
        assert_eq!(config.tire_class, "auto");
        assert_eq!(config.strut_material, "auto");
    }

    #[test]
    fn body_gear_is_added_well_above_the_twin_nose_wheel_threshold() {
        let config = LandingGearConfig::default();
        assert!(config.mlg_body_gear_mtow_kg > config.nlg_dual_wheel_mtow_kg);
    }

    #[test]
    fn the_strut_material_has_its_own_option_list_and_not_the_general_one() {
        // Its accepted values are the strut materials, which are a different
        // set from the structural material database, and offering the wrong
        // list would let a value through that nothing downstream can resolve.
        let schema = LandingGearConfig::default().schema();
        match &schema.field("strut_material").unwrap().entry {
            Entry::Leaf(leaf) => {
                assert_eq!(leaf.options, Some(OptionSource::StrutMaterial));
                assert!(!OptionSource::StrutMaterial.editable());
            }
            Entry::Node(_) => panic!("a material name is not a group"),
        }
    }
}
