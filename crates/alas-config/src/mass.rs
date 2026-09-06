// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/mass_config.py
// Reference: alas @ rust-port-baseline.

//! Empirical fractions and parameters of the mass buildup.
//!
//! Conceptual-design mass estimation is a set of statistical fits to aircraft
//! that were actually built, so almost every number here is a fraction of
//! maximum takeoff weight taken from a table rather than something derived.
//! The defaults follow Torenbeek (*Synthesis of Subsonic Airplane Design*,
//! Delft University Press, 1982) and Raymer (*Aircraft Design: A Conceptual
//! Approach*, 5th ed., table 15.2) for transports certified to CS-25 or
//! FAR-25.
//!
//! Fractions of maximum takeoff weight are circular by nature -- the weight
//! depends on the fractions and the fractions are applied to the weight --
//! which is why they are exposed: calibrating them against a known aircraft
//! is how the buildup is made to agree with reality, and doing that in the
//! settings beats doing it in the source.

use serde::{Deserialize, Serialize};

use crate::{ConfigNode, FlopsTransportConfig, SystemsMassMethod};

/// Tunable mass fractions and structural parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct MassModelConfig {
    /// Method used for systems, equipment, and operating-item mass.
    #[serde(
        default,
        skip_serializing_if = "SystemsMassMethod::is_reference_compatible"
    )]
    #[config(
        advanced,
        options = SystemsMassMethod,
        label = "Systems mass method",
        help = "Versioned systems-mass method: frozen Python-compatible MTOW fractions or the NASA FLOPS transport component buildup."
    )]
    pub systems_mass_method: SystemsMassMethod,

    /// Physical architecture required by the FLOPS transport method.
    #[serde(default, skip_serializing_if = "FlopsTransportConfig::is_unspecified")]
    #[config(
        nested,
        advanced,
        help = "Declared range, crew, cabin, hydraulic, engine-mounting, and fuel-system inputs required by FLOPS transport mass correlations."
    )]
    pub flops_transport: FlopsTransportConfig,

    /// Whether the product analysis places each mass group at its
    /// geometry-derived station.
    #[serde(default = "default_true", skip_serializing_if = "Clone::clone")]
    #[config(
        advanced,
        label = "Geometry-derived component stations",
        help = "Place every mass group at the station the built geometry gives it: the integrated wingbox centroid, the tails at 42 percent of their mean chord, the gear at its nose and main stations, the engines at their nacelles and the fuel in its tanks. Disable to keep the frozen point placement of the reference implementation."
    )]
    pub geometric_component_stations: bool,

    /// Share of maximum takeoff weight the wing structure must carry.
    #[config(
        label = "Wing suspended-mass fraction",
        help = "Fraction of MTOW treated as 'suspended' mass in the Torenbeek wing structural formula (everything the wing structure must carry other than itself). Typical commercial transport: 0.70-0.78."
    )]
    pub suspended_mass_fraction: f64,

    /// Design airspeed with the flaps out.
    #[config(
        label = "Max airspeed with flaps extended",
        unit = "m/s",
        help = "Design airspeed with flaps extended, fed into the Torenbeek wing-mass formula."
    )]
    pub max_airspeed_for_flaps_ms: f64,

    /// Maximum flap deflection.
    #[config(
        label = "Max take-off flap deflection",
        unit = "deg",
        help = "Maximum flap deflection angle, fed into the Torenbeek wing-mass formula."
    )]
    pub flap_deflection_angle_deg: f64,

    /// Landing gear mass as a share of maximum takeoff weight.
    #[config(
        label = "Landing-gear mass fraction",
        help = "Landing gear mass as a fraction of MTOW. Raymer Table 15.2: ~4% for commercial jet transports."
    )]
    pub landing_gear_mass_fraction: f64,

    /// Engine thrust-to-weight ratio used to size dry engine mass.
    #[config(
        label = "Engine thrust-to-weight factor",
        help = "Dry engine mass is estimated as thrust / (this factor * g). Historical engine thrust-to-weight ratios are ~5-7, so this factor is typically ~6."
    )]
    pub propulsion_twr_factor: f64,

    /// Multiplier on dry engine mass for everything installed around it.
    #[config(
        label = "Propulsion installation overhead",
        help = "Multiplier on dry engine mass accounting for pylon, cowling, fire suppression and other installed accessories."
    )]
    pub propulsion_installation_factor: f64,

    /// Propulsion mass fraction used when the engine is not in the database.
    #[config(
        label = "Propulsion mass fallback fraction",
        help = "Fallback propulsion mass as a fraction of MTOW, used only if the selected engine isn't found in the database."
    )]
    pub propulsion_mass_fallback_fraction: f64,

    /// Systems and equipment as a share of maximum takeoff weight.
    #[config(
        label = "Systems & equipment mass fraction",
        help = "Avionics, electrical, ECS, APU, etc. as a fraction of MTOW. Raymer Table 15.2: 9-13% for commercial transports."
    )]
    pub systems_mass_fraction: f64,

    /// Furnishings and operational items as a share of maximum takeoff
    /// weight.
    #[config(
        label = "Furnishings & operations mass fraction",
        help = "Passenger seats, galleys, lavatories, insulation, crew, paint, and operational empty items as a fraction of MTOW. Typically 10-14% for passenger transports."
    )]
    pub furnishings_mass_fraction: f64,

    /// How much payload mass one metre of cabin holds.
    #[config(
        label = "Payload linear density",
        unit = "kg/m",
        help = "How much payload mass occupies one metre of cabin length. Used only to derive the payload/systems CG position (the occupied cabin length), not the payload mass itself -- so stretching the fuselage beyond what the payload needs doesn't shift the CG aft 'for free'."
    )]
    pub cabin_payload_density_kg_m: f64,

    /// Where the nose gear sits along the fuselage.
    #[config(
        label = "Nose-gear X position",
        unit = "fraction of fuselage length",
        help = "Nose landing gear longitudinal position, as a fraction of total fuselage length from the nose."
    )]
    pub nlg_x_fraction: f64,

    /// Where the main gear sits along the mean aerodynamic chord.
    #[config(
        label = "Main-gear X position",
        unit = "fraction of MAC aft of MAC LE",
        help = "Main landing gear longitudinal position, as a fraction of the mean aerodynamic chord aft of the MAC leading edge."
    )]
    pub mlg_x_fraction_mac: f64,

    /// Most weight the nose gear is rated to carry.
    #[config(
        label = "Max nose-gear load fraction",
        help = "Maximum fraction of total aircraft weight the nose gear is rated to carry -- sets the 'NLG Max Strength' CG-envelope boundary."
    )]
    pub pct_load_nlg_max: f64,

    /// Most weight the main gear is rated to carry.
    #[config(
        label = "Max main-gear load fraction",
        help = "Maximum fraction of total aircraft weight the main gear is rated to carry -- sets the 'MLG Max Strength' CG-envelope boundary."
    )]
    pub pct_load_mlg_max: f64,

    /// Least weight the nose gear needs for steering authority.
    #[config(
        label = "Min nose-gear load fraction",
        help = "Minimum fraction of weight that must be on the nose gear for adequate steering authority -- sets the 'Min Nose Load' CG-envelope boundary (the aft-most safe CG at each weight)."
    )]
    pub pct_load_nlg_min: f64,

    /// Maximum landing weight as a share of maximum takeoff weight.
    #[config(
        label = "Max landing weight fraction of MTOW",
        help = "Maximum Landing Weight (MLW) as a fraction of MTOW, shown as a reference line on the CG envelope."
    )]
    pub mlw_fraction_mtow: f64,

    /// Density used to turn tank volume into a fuel mass.
    #[config(
        label = "Fuel density",
        unit = "kg/m^3",
        help = "Jet-A/Jet-A1 density at 15C (~804 kg/m^3). Converts wing tank volume to a fuel-mass capacity for the payload-range diagram and the wing fuel-volume check."
    )]
    pub fuel_density_kg_m3: f64,

    /// Share of the wing's geometric volume that is usable tankage.
    #[config(
        label = "Usable fuel-tank volume fraction",
        help = "Fraction of the wing's geometric (Torenbeek) fuel volume that's actually usable tank capacity, after structure, ribs, systems and unusable-fuel allowance. Typical preliminary-design value: 0.85-0.95."
    )]
    pub fuel_tank_usable_fraction: f64,
}

const fn default_true() -> bool {
    true
}

impl Default for MassModelConfig {
    fn default() -> Self {
        Self {
            systems_mass_method: SystemsMassMethod::ReferenceCompatibleFractions,
            flops_transport: FlopsTransportConfig::default(),
            geometric_component_stations: true,
            suspended_mass_fraction: 0.75,
            max_airspeed_for_flaps_ms: 90.0,
            flap_deflection_angle_deg: 40.0,
            landing_gear_mass_fraction: 0.04,
            propulsion_twr_factor: 6.0,
            propulsion_installation_factor: 1.30,
            propulsion_mass_fallback_fraction: 0.07,
            systems_mass_fraction: 0.11,
            furnishings_mass_fraction: 0.10,
            cabin_payload_density_kg_m: 800.0,
            nlg_x_fraction: 0.10,
            mlg_x_fraction_mac: 0.50,
            pct_load_nlg_max: 0.10,
            pct_load_mlg_max: 0.93,
            pct_load_nlg_min: 0.02,
            mlw_fraction_mtow: 0.92,
            fuel_density_kg_m3: 804.0,
            fuel_tank_usable_fraction: 0.85,
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
    fn the_gear_load_bounds_leave_a_usable_cg_envelope() {
        // The three gear-load fractions are the envelope's boundaries, and an
        // ordering mistake among them describes an envelope with no interior:
        // every CG position would violate something.
        let config = MassModelConfig::default();
        assert!(config.pct_load_nlg_min < config.pct_load_nlg_max);
        assert!(config.pct_load_nlg_max + config.pct_load_mlg_max > 1.0);
    }

    #[test]
    fn the_mass_fractions_of_mtow_leave_room_for_structure_fuel_and_payload() {
        // Landing gear, systems and furnishings are each a share of the same
        // takeoff weight, and the wing, fuselage, engines, fuel and payload
        // have to fit in what is left.
        let config = MassModelConfig::default();
        let accounted = config.landing_gear_mass_fraction
            + config.systems_mass_fraction
            + config.furnishings_mass_fraction
            + config.propulsion_mass_fallback_fraction;
        assert!(
            accounted < 0.5,
            "the fixed fractions already take {accounted}"
        );
    }

    #[test]
    fn a_landing_weight_does_not_exceed_a_takeoff_weight() {
        assert!(MassModelConfig::default().mlw_fraction_mtow <= 1.0);
    }

    #[test]
    fn a_unit_the_field_name_cannot_express_is_stated_explicitly() {
        // `_kg_m3` is not one of the recognized suffixes and `_ms` is not
        // `_m_s`, so both of these would derive nothing without the explicit
        // unit -- and a density shown without one is a number nobody can
        // check.
        let schema = MassModelConfig::default().schema();
        assert_eq!(schema.field("fuel_density_kg_m3").unwrap().unit, "kg/m^3");
        assert_eq!(
            schema.field("max_airspeed_for_flaps_ms").unwrap().unit,
            "m/s"
        );
    }
}
