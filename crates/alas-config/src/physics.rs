// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/physics_config.py
// Reference: alas @ rust-port-baseline.

//! Coefficients of the semi-empirical drag buildup.
//!
//! The parasite-drag estimate is Raymer's component method and the transonic
//! rise is the Korn equation, and both are parameterized by a handful of
//! technology and interference factors. In the scripts this program grew out
//! of these were inline literals -- `* 1.10`, `0.95`, `radians(32)` -- which
//! made the fidelity assumptions invisible and unadjustable. Gathering them
//! here is what makes them either.
//!
//! References for the values themselves are in each field's explanation:
//! D. P. Raymer, *Aircraft Design: A Conceptual Approach*, for the form and
//! interference factors, and the Korn equation as it is usually given for the
//! wave-drag rise.

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
    #[config(
        label = "Fuselage interference factor (Q)",
        help = "How much neighbouring components (wing, tail) disturb the fuselage's flow, multiplying its parasite drag."
    )]
    pub interference_factor_fuselage: f64,

    /// Lumped multiplier covering what a component-by-component buildup
    /// cannot see.
    #[config(
        label = "Viscous drag margin",
        help = "Lumped multiplier applied to the total parasite drag, accounting for excrescences, gaps and roughness not captured component-by-component. 1.10 = +10% margin."
    )]
    pub viscous_margin: f64,

    /// How much the nacelle and pylon disturb the local flow.
    #[config(
        label = "Nacelle/pylon interference factor (Q)",
        help = "How much the engine nacelle and pylon disturb the local flow. Typical value 1.3 for podded under-wing installations (Raymer)."
    )]
    pub interference_factor_nacelle: f64,

    /// Airfoil-technology factor in the Korn wave-drag equation.
    #[config(
        label = "Korn technology factor (kappa)",
        help = "Airfoil-technology factor in the Korn wave-drag equation; ~0.95 for modern supercritical sections, lower for older/less efficient sections."
    )]
    pub korn_technology_factor: f64,

    /// Below this Mach number, wave drag is taken as zero.
    #[config(
        label = "Wave-drag onset Mach",
        help = "Below this Mach number, transonic wave drag is assumed zero (not yet computed)."
    )]
    pub wave_drag_onset_mach: f64,

    /// Leading constant of the Korn wave-drag rise.
    #[config(
        label = "Wave-drag rise coefficient",
        help = "Leading constant in the Korn wave-drag rise: CD_wave = coefficient * (M - M_drag_divergence)^4."
    )]
    pub wave_drag_coefficient: f64,
}

impl Default for DragModelConfig {
    fn default() -> Self {
        Self {
            exclude_buried_main_wing_area: true,
            max_thickness_chordwise_loc: 0.35,
            interference_factor_wing: 1.0,
            interference_factor_fuselage: 1.25,
            viscous_margin: 1.10,
            interference_factor_nacelle: 1.3,
            korn_technology_factor: 0.95,
            wave_drag_onset_mach: 0.6,
            wave_drag_coefficient: 20.0,
        }
    }
}

fn exclude_buried_main_wing_area_default() -> bool {
    true
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
                "interference_factor_nacelle",
                "korn_technology_factor",
                "wave_drag_onset_mach",
                "wave_drag_coefficient",
            ]
        );
    }

    #[test]
    fn a_field_with_no_declared_unit_takes_one_from_its_name() {
        // None of these names end in a unit suffix, so all of them are
        // dimensionless -- which is the point: the derived unit is the empty
        // string rather than a guess.
        let schema = DragModelConfig::default().schema();
        let viscous = schema.field("viscous_margin").unwrap();
        assert_eq!(viscous.unit, "");
        assert_eq!(viscous.label, "Viscous drag margin");
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

    #[test]
    fn a_configuration_round_trips_through_serialization() {
        let config = DragModelConfig::default();
        let text = serde_json::to_string(&config).unwrap();
        assert_eq!(
            serde_json::from_str::<DragModelConfig>(&text).unwrap(),
            config
        );
    }
}
