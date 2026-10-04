// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Pressurized fuselage structure for the regional turboprop class.
//!
//! FLOPS equation 56 (NASA/TM-2017-219627 Vol. I) prices the transport
//! fuselage from `(length x mean diameter)^1.28` alone. It carries no cabin
//! pressure differential, so it implicitly carries the differential of the
//! jet transports it was fitted on, about 8.6 psi. A regional turboprop
//! certified to 25,000 ft needs about 5.5 psi, and its shell is priced too
//! heavy by the jet differential.
//!
//! This module evaluates the pressure-bending fuselage index method instead:
//!
//! ```text
//! I_p = 1.5e-3 P B                      pressure index
//! I_b = 1.91e-4 N W L / H^2             bending index
//! I_f = I_p                      if I_p > I_b
//!     = (I_p^2 + I_b^2) / (2 I_b)  otherwise
//! W_fuse = (1.051 + 0.102 I_f) S_fuse   [lb]
//! ```
//!
//! with `P` the maximum pressure differential (lb/ft^2), `B` the fuselage
//! width, `H` its height and `L` its length less half the wing root chord
//! (ft), `N` the limit load factor at zero-fuel weight, `W` the maximum
//! zero-fuel weight less the wing and the wing-mounted engines, nacelles and
//! pylons (lb), and `S_fuse` the gross fuselage wetted area (ft^2). Source:
//! I. Kroo, *Aircraft Design: Synthesis and Analysis*, Stanford University
//! AA 241 course notes (Desktop Aeronautics, 2001), "Component Weights",
//! section 4 "Fuselage" (adg.stanford.edu/aa241/structures/componentweight.html).
//! The same correlation is the fuselage relation of the SUAVE conceptual-design
//! code (`SUAVE/Methods/Weights/Correlations/Transport/tube.py`).
//!
//! The method is applied at class level only: the regional turboprop class
//! that [`alas_config::CabinEquipmentMethod::is_regional_turboprop_class`]
//! declares. Every other aircraft keeps equation 56 unchanged.
//!
//! The design differential is the one CS 25.841(a) requires at the certified
//! maximum operating altitude: a cabin pressure altitude of no more than
//! 8,000 ft, evaluated in the ISA. A real cabin controller may run a higher
//! differential than that minimum, so this is a lower bound of the shell's
//! actual design pressure.

use alas_atmo::Atmosphere;
use alas_config::{DesignRequirements, MassModelConfig};
use alas_units::{FOOT, POUND_FORCE, POUND_MASS};

use super::super::airframe_geometry::BuiltFuselage;
use super::super::turboprop::TurbopropPropulsionBreakdown;

/// Cabin pressure altitude CS 25.841(a) allows at the maximum operating
/// altitude under normal operating conditions: 8,000 ft, m.
pub const MAXIMUM_CABIN_PRESSURE_ALTITUDE_M: f64 = 8_000.0 * FOOT;

/// Factor of safety between limit and ultimate load, CS 25.303.
const ULTIMATE_FACTOR_OF_SAFETY: f64 = 1.5;

/// SI inputs to the pressure-bending fuselage method.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PressurizedFuselageInputs {
    /// Gross fuselage wetted area `S_fuse`, m^2.
    pub wetted_area_m2: f64,
    /// Maximum fuselage width `B`, m.
    pub width_m: f64,
    /// Maximum fuselage height `H`, m.
    pub height_m: f64,
    /// Fuselage length less half the wing root chord `L`, m.
    pub effective_length_m: f64,
    /// Maximum cabin pressure differential `P`, Pa.
    pub pressure_differential_pa: f64,
    /// Limit load factor at zero-fuel weight `N`.
    pub limit_load_factor: f64,
    /// Maximum zero-fuel mass, kg.
    pub design_zero_fuel_mass_kg: f64,
    /// Wing-mounted engines, propellers, nacelles and pylons, kg.
    pub wing_mounted_propulsion_kg: f64,
}

/// The ISA cabin pressure differential at `maximum_operating_altitude_m`
/// with the cabin at the CS 25.841(a) 8,000 ft limit, Pa. Zero when the
/// aircraft never climbs above that cabin altitude.
pub fn cabin_pressure_differential_pa(maximum_operating_altitude_m: f64) -> f64 {
    let cabin_pa = Atmosphere::isa(MAXIMUM_CABIN_PRESSURE_ALTITUDE_M).pressure();
    let ambient_pa = Atmosphere::isa(maximum_operating_altitude_m).pressure();
    (cabin_pa - ambient_pa).max(0.0)
}

/// The pressure-bending fuselage structural mass, kg, for a wing of
/// `wing_kg`.
pub fn pressurized_fuselage_kg(inputs: &PressurizedFuselageInputs, wing_kg: f64) -> f64 {
    let pressure_psf = inputs.pressure_differential_pa / (POUND_FORCE / (FOOT * FOOT));
    let width_ft = inputs.width_m / FOOT;
    let height_ft = inputs.height_m / FOOT;
    let length_ft = inputs.effective_length_m / FOOT;
    let bending_weight_lb =
        (inputs.design_zero_fuel_mass_kg - wing_kg - inputs.wing_mounted_propulsion_kg)
            / POUND_MASS;
    let pressure_index = 1.5e-3 * pressure_psf * width_ft;
    let bending_index = 1.91e-4 * inputs.limit_load_factor * bending_weight_lb * length_ft
        / (height_ft * height_ft);
    let fuselage_index = if pressure_index > bending_index || bending_index <= 0.0 {
        pressure_index
    } else {
        (pressure_index * pressure_index + bending_index * bending_index) / (2.0 * bending_index)
    };
    let wetted_ft2 = inputs.wetted_area_m2 / (FOOT * FOOT);
    (1.051 + 0.102 * fuselage_index) * wetted_ft2 * POUND_MASS
}

/// The wing-mounted share of a shaft-power propulsion group, kg: engines,
/// gearboxes, propellers, nacelles and pylons, for `wing_engines` of
/// `engine_count` engines. Zero without a turboprop group.
pub(in crate::flops_transport) fn wing_mounted_turboprop_kg(
    group: Option<&TurbopropPropulsionBreakdown>,
    wing_engines: usize,
    engine_count: usize,
) -> f64 {
    match group {
        Some(group) if engine_count > 0 => {
            (group.engines_kg
                + group.gearboxes_kg
                + group.propellers_kg
                + group.nacelles_kg
                + group.pylons_kg)
                * wing_engines as f64
                / engine_count as f64
        }
        _ => 0.0,
    }
}

/// The method's inputs when the aircraft is of the regional turboprop class,
/// else `None` (equation 56 then applies).
///
/// The maximum operating altitude is the declared one or, absent, the cruise
/// altitude requirement; the zero-fuel mass is the declared one or, absent,
/// `design_gross_mass_kg`, an upper bound. The limit load factor is the
/// ultimate one divided by the CS 25.303 factor of safety.
pub(in crate::flops_transport) fn regional_turboprop_inputs(
    mass_model: &MassModelConfig,
    requirements: &DesignRequirements,
    built: &BuiltFuselage<'_>,
    design_gross_mass_kg: f64,
    wing_mounted_propulsion_kg: f64,
) -> Option<PressurizedFuselageInputs> {
    if !mass_model
        .flops_transport
        .cabin_equipment_method
        .is_regional_turboprop_class()
    {
        return None;
    }
    let technology = &mass_model.flops_structure;
    let altitude_m = technology
        .maximum_operating_altitude_m
        .unwrap_or(requirements.cruise_altitude_m);
    let root_chord_m = built.wing.xsecs.first().map_or(0.0, |root| root.chord);
    Some(PressurizedFuselageInputs {
        wetted_area_m2: built.fuselage.area_wetted(),
        width_m: built.width_m,
        height_m: built.depth_m,
        effective_length_m: built.length_m - root_chord_m / 2.0,
        pressure_differential_pa: cabin_pressure_differential_pa(altitude_m),
        limit_load_factor: requirements.ultimate_load_factor / ULTIMATE_FACTOR_OF_SAFETY,
        design_zero_fuel_mass_kg: technology
            .design_zero_fuel_mass_kg
            .unwrap_or(design_gross_mass_kg),
        wing_mounted_propulsion_kg,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_units::PSI as PSI_PA;

    #[test]
    fn the_cs_25_841_differential_at_25000_ft_is_about_5_5_psi() {
        // ISA: 75.26 kPa at 8,000 ft, 37.60 kPa at 25,000 ft (ICAO Doc 7488
        // tables), a 37.66 kPa = 5.46 psi differential; at 41,000 ft the
        // ambient is 17.87 kPa and the differential 8.32 psi, the jet value
        // FLOPS equation 56 implicitly carries.
        let atr = cabin_pressure_differential_pa(25_000.0 * FOOT) / PSI_PA;
        assert!((atr - 5.46).abs() < 0.01, "{atr} psi");
        let jet = cabin_pressure_differential_pa(41_000.0 * FOOT) / PSI_PA;
        assert!((jet - 8.32).abs() < 0.01, "{jet} psi");
        assert_eq!(cabin_pressure_differential_pa(2_000.0), 0.0);
    }

    #[test]
    fn the_fuselage_index_reproduces_the_published_relation_by_hand() {
        let inputs = PressurizedFuselageInputs {
            wetted_area_m2: 190.0,
            width_m: 2.77,
            height_m: 2.77,
            effective_length_m: 25.87,
            pressure_differential_pa: 5.46 * PSI_PA,
            limit_load_factor: 2.5,
            design_zero_fuel_mass_kg: 21_000.0,
            wing_mounted_propulsion_kg: 2_000.0,
        };
        let wing_kg = 2_500.0;
        let p = 5.46 * PSI_PA / (POUND_FORCE / (FOOT * FOOT));
        let ip = 1.5e-3 * p * (2.77 / FOOT);
        let w = (21_000.0 - 2_500.0 - 2_000.0) / POUND_MASS;
        let ib = 1.91e-4 * 2.5 * w * (25.87 / FOOT) / (2.77 / FOOT).powi(2);
        assert!(ib > ip, "this regional fuselage is bending-dominated");
        let index = (ip * ip + ib * ib) / (2.0 * ib);
        let expected_kg = (1.051 + 0.102 * index) * 190.0 / (FOOT * FOOT) * POUND_MASS;
        let mass_kg = pressurized_fuselage_kg(&inputs, wing_kg);
        assert!((mass_kg - expected_kg).abs() < 1e-9 * expected_kg);

        // A higher differential never lightens the shell.
        let mut jet = inputs;
        jet.pressure_differential_pa = 8.32 * PSI_PA;
        assert!(pressurized_fuselage_kg(&jet, wing_kg) > mass_kg);
        // Above the bending index the pressure index governs alone.
        jet.pressure_differential_pa = 40.0 * PSI_PA;
        let pressure_only = (1.051
            + 0.102 * 1.5e-3 * (40.0 * PSI_PA / (POUND_FORCE / (FOOT * FOOT))) * (2.77 / FOOT))
            * 190.0
            / (FOOT * FOOT)
            * POUND_MASS;
        assert!(
            (pressurized_fuselage_kg(&jet, wing_kg) - pressure_only).abs() < 1e-9 * pressure_only
        );
    }
}
