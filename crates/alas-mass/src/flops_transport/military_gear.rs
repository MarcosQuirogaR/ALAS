// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Alighting gear of a military transport with fuselage-mounted main gear.
//!
//! FLOPS equations 63-64 are fitted on transports whose main gear is
//! wing-mounted. Beltramo et al., *Parametric Study of Transport Aircraft
//! Systems Cost and Weight*, NASA CR-151970 (1977), p. 5-16, fit military
//! transports separately because their main gear is fuselage-mounted: shorter
//! struts and no wing-attach load path, with the fuselage attach bulkheads
//! booked in the body. The relations used, read on the page:
//!
//! - p. 5-22: basic alighting gear (main and nose) of medium and large
//!   military transports `W4' = 0.0439 TOGW - 2,050` lb, and the footnote for
//!   small military transports `W4' = 0.0302 TOGW` lb. The two lines cross at
//!   149,635 lb; the medium/large fit starts at its lightest point, the C-130E
//!   at 155,000 lb, so the larger of the two is taken (continuous, and each
//!   line is used only on its own side of the crossing).
//! - p. 5-29 (Fig. 5.13): subtract for carbon brakes
//!   `W4H = W4' [0.0786 - 0.071e-6 TOGW]`, applied when the aircraft declares
//!   carbon brakes (`FlopsStructureConfig::main_gear_carbon_brakes`).
//!
//! - p. 5-29: add for low-pressure tyres `W4E = W4' [0.125 - 0.0102e-5 TOGW]`,
//!   applied when the aircraft declares low-pressure main tyres
//!   (`FlopsStructureConfig::main_gear_low_pressure_tyres`). The military line
//!   was fitted with the low-to-high-pressure tyre weight removed from the
//!   C-130E (500 lb), C-133B (946 lb) and C-5A (1,583 lb) (Table 5.5, p. 5-20),
//!   so an aircraft with low-pressure tyres takes it back. The report classes
//!   the tyres only as Low or High (Table 5.4, p. 5-19: C-130E, C-133B, C-5A
//!   low; KC-135A, C-141A high); the representative design pressures of the
//!   same aircraft in UFC 3-260-02 (30 June 2001), chapter 4, paragraph 3e,
//!   p. 4-2, are C-130 95 psi, C-5A 115 psi (low) and C-141 180 psi (high).
//!   The class is therefore a declared attribute, set per aircraft from the
//!   manufacturer's own classification, not a pressure threshold invented here.
//!   A tyre the type's own documentation calls standard is not low-pressure
//!   (Jane's 2008-09 p.292: the CN-235's 5.58 bar military tyre is standard,
//!   low-pressure tyres are a separate option), so the C295 does not declare it.
//!
//! Not applied: the p. 5-29 sink-speed increment `W4F = 0.038 W4'` per ft/s
//! above the 9-10 ft/s of the data aircraft (Table 5.4); no preset declares a
//! design sink speed. Needed: the limit sink speed of each military preset
//! from its type-certificate data sheet or landing-gear specification.
//!
//! Not applied: the 0.184 W4' increment for kneeling, prepositioning and tyre
//! inflation/deflation (p. 5-29) is one lump taken from the C-5A, which has all
//! three (Table 5.5, p. 5-20: 5,836 lb removed); no source splits the kneeling
//! share. A declared kneeling main gear is priced instead by Raymer's kneeling
//! factor on the main-gear share ([`kneeling`]), which the lump bounds above.
//! Needed for prepositioning and tyre inflation/deflation: the C-5A detailed
//! alighting-gear weight statement the report used for that row
//! (Lockheed-Georgia C-5A actual weight report, landing gear detail).
//!
//! The airframe sources record which method priced the gear
//! (`FlopsAirframeSources::gear_mass`: [`FLOPS_GEAR_SOURCE`] or
//! [`MILITARY_GEAR_SOURCE`]).
//!
//! The class is read from two declared attributes: a military freight hold
//! (`cabin.cargo.freight_hold`) and main gear in fuselage sponsons
//! (`geometry.fuselage.gear_sponson_length_m`). Every other aircraft keeps
//! equations 63-64 and is bit-identical.

use alas_config::{CabinConfig, GeometryConfig};

use super::structure::FlopsStructureBreakdown;
use super::units::{kg, lb};
use super::FlopsAirframeBreakdown;

pub mod kneeling;

/// Gear-mass source when FLOPS equations 63-64 price the gear.
pub const FLOPS_GEAR_SOURCE: &str = "flops_equations_63_64";
/// Gear-mass source when the CR-151970 military relation replaced them.
pub const MILITARY_GEAR_SOURCE: &str = "beltramo_cr151970_military";

/// p. 5-22, medium and large military transports: slope per lb of TOGW.
const MEDIUM_LARGE_SLOPE: f64 = 0.0439;
/// p. 5-22, medium and large military transports: intercept, lb.
const MEDIUM_LARGE_INTERCEPT_LB: f64 = 2_050.0;
/// p. 5-22 footnote, small military transports: slope per lb of TOGW.
const SMALL_SLOPE: f64 = 0.0302;
/// p. 5-29, carbon brakes: constant term of the fraction of `W4'`.
const CARBON_BRAKE_CONSTANT: f64 = 0.0786;
/// p. 5-29, carbon brakes: TOGW term of the fraction of `W4'`, per lb.
const CARBON_BRAKE_PER_LB: f64 = 0.071e-6;
/// p. 5-29, low-pressure tyres: constant term of the fraction of `W4'`.
const LOW_PRESSURE_TYRE_CONSTANT: f64 = 0.125;
/// p. 5-29, low-pressure tyres: TOGW term of the fraction of `W4'`, per lb.
const LOW_PRESSURE_TYRE_PER_LB: f64 = 0.0102e-5;

/// Whether the aircraft is a military transport with fuselage-mounted main
/// gear: a declared military freight hold and declared gear sponsons.
pub fn has_military_fuselage_gear(cabin: &CabinConfig, geometry: &GeometryConfig) -> bool {
    cabin.cargo.freight_hold.is_some()
        && geometry.fuselage.gear_sponson_length_m.is_some_and(|length| length > 0.0)
}

/// Basic military alighting gear `W4'`, lb, from the takeoff gross weight, lb.
pub fn basic_gear_lb(takeoff_gross_lb: f64) -> f64 {
    (SMALL_SLOPE * takeoff_gross_lb)
        .max(MEDIUM_LARGE_SLOPE * takeoff_gross_lb - MEDIUM_LARGE_INTERCEPT_LB)
}

/// The declared gear features that carry a p. 5-29 adjustment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MilitaryGearFeatures {
    /// Carbon main-gear brakes: subtract `W4H`.
    pub carbon_brakes: bool,
    /// Low-pressure main tyres: add `W4E`.
    pub low_pressure_tyres: bool,
    /// Kneeling main gear: Raymer's kneeling factor on the main-gear share,
    /// applied by [`apply_military_gear`] once the main/nose split is known
    /// ([`kneeling`]); [`military_gear_kg`] has no split and leaves it out.
    pub kneeling_main_gear: bool,
}

/// Military alighting gear (main and nose), kg, at the design gross mass.
pub fn military_gear_kg(design_gross_mass_kg: f64, features: MilitaryGearFeatures) -> f64 {
    let togw_lb = lb(design_gross_mass_kg);
    let basic_lb = basic_gear_lb(togw_lb);
    let carbon_lb = if features.carbon_brakes {
        basic_lb * (CARBON_BRAKE_CONSTANT - CARBON_BRAKE_PER_LB * togw_lb)
    } else {
        0.0
    };
    let low_pressure_lb = if features.low_pressure_tyres {
        basic_lb * (LOW_PRESSURE_TYRE_CONSTANT - LOW_PRESSURE_TYRE_PER_LB * togw_lb)
    } else {
        0.0
    };
    kg(basic_lb - carbon_lb + low_pressure_lb)
}

/// Replace the equation 63-64 gear in `structure` by `total_kg`, keeping the
/// FLOPS main/nose split so the gear stations carry the same shares. Returns
/// whether the replacement was made (both totals finite and positive).
pub fn replace_gear(structure: &mut FlopsStructureBreakdown, total_kg: f64) -> bool {
    let flops_kg = structure.main_gear_kg + structure.nose_gear_kg;
    let usable = |mass_kg: f64| mass_kg.is_finite() && mass_kg > 0.0;
    if !usable(flops_kg) || !usable(total_kg) {
        return false;
    }
    let ratio = total_kg / flops_kg;
    structure.main_gear_kg *= ratio;
    structure.nose_gear_kg *= ratio;
    structure.total_kg += total_kg - flops_kg;
    true
}

/// Price the gear of a military transport with fuselage-mounted main gear by
/// the military relation, in the evaluated airframe, and record the method in
/// its sources; a declared kneeling main gear then takes its increment on the
/// main-gear share. Any other aircraft is left untouched.
pub fn apply_military_gear(
    airframe: &mut FlopsAirframeBreakdown,
    cabin: &CabinConfig,
    geometry: &GeometryConfig,
    features: MilitaryGearFeatures,
) {
    if !has_military_fuselage_gear(cabin, geometry) {
        return;
    }
    let total_kg = military_gear_kg(
        airframe.structure_inputs.wing.design_gross_mass_kg,
        features,
    );
    if let Some(structure) = airframe.structure.as_mut() {
        if replace_gear(structure, total_kg) {
            airframe.sources.gear_mass = MILITARY_GEAR_SOURCE;
            if features.kneeling_main_gear {
                kneeling::add_kneeling_increment(structure);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_class_lines_meet_at_their_crossing() {
        let crossing_lb = MEDIUM_LARGE_INTERCEPT_LB / (MEDIUM_LARGE_SLOPE - SMALL_SLOPE);
        assert!((crossing_lb - 149_635.0).abs() < 1.0);
        assert!((basic_gear_lb(100_000.0) - 3_020.0).abs() < 1e-9);
        assert!((basic_gear_lb(300_000.0) - 11_120.0).abs() < 1e-9);
    }

    const CARBON: MilitaryGearFeatures = MilitaryGearFeatures {
        carbon_brakes: true,
        low_pressure_tyres: false,
        kneeling_main_gear: false,
    };
    const LOW_PRESSURE: MilitaryGearFeatures = MilitaryGearFeatures {
        carbon_brakes: false,
        low_pressure_tyres: true,
        kneeling_main_gear: false,
    };

    #[test]
    fn a_141_t_transport_with_carbon_brakes_matches_the_hand_evaluation() {
        // 141,000 kg = 310,851.8 lb; W4' = 11,596.4 lb; carbon credit
        // 11,596.4 x (0.0786 - 0.022071) = 655.5 lb; 10,940.9 lb = 4,962.7 kg.
        let with_carbon = military_gear_kg(141_000.0, CARBON);
        assert!((with_carbon - 4_962.7).abs() < 0.5, "{with_carbon}");
        let steel = military_gear_kg(141_000.0, MilitaryGearFeatures::default());
        assert!((steel - 5_260.0).abs() < 0.5, "{steel}");
    }

    #[test]
    fn low_pressure_tyres_add_the_p_5_29_increment_on_the_basic_gear() {
        // 141,000 kg: W4E = 11,596.4 x (0.125 - 0.031707) = 1,081.9 lb = 490.7 kg,
        // on top of the carbon-brake gear: 4,962.7 + 490.7 = 5,453.4 kg.
        let both = MilitaryGearFeatures {
            carbon_brakes: true,
            low_pressure_tyres: true,
            kneeling_main_gear: false,
        };
        let gear = military_gear_kg(141_000.0, both);
        // Kneeling needs the main/nose split, so the total relation ignores it.
        let with_kneeling = MilitaryGearFeatures {
            kneeling_main_gear: true,
            ..both
        };
        assert!((military_gear_kg(141_000.0, with_kneeling) - gear).abs() < 1e-12);
        assert!((gear - 5_453.4).abs() < 0.5, "{gear}");
        let increment = gear - military_gear_kg(141_000.0, CARBON);
        assert!((increment - 490.7).abs() < 0.5, "{increment}");
        // The increment stays positive across the medium/large data (to the
        // C-5A, 728,000 lb TOGW, Table 5.4 p. 5-19).
        let c5a_kg = kg(728_000.0);
        let basic = military_gear_kg(c5a_kg, MilitaryGearFeatures::default());
        assert!(military_gear_kg(c5a_kg, LOW_PRESSURE) > basic);
    }

    #[test]
    fn a_21_t_transport_takes_the_small_military_line() {
        // 21,000 kg = 46,297.1 lb; 0.0302 x 46,297.1 = 1,398.2 lb = 634.2 kg.
        let gear = military_gear_kg(21_000.0, MilitaryGearFeatures::default());
        assert!((gear - 634.2).abs() < 0.5, "{gear}");
        // Low-pressure tyres: 1,398.2 x (0.125 - 0.004722) = 168.2 lb = 76.3 kg.
        let low_pressure = military_gear_kg(21_000.0, LOW_PRESSURE);
        assert!((low_pressure - 710.5).abs() < 0.5, "{low_pressure}");
    }

    #[test]
    fn the_replacement_keeps_the_main_nose_split_and_the_total() {
        let mut structure = FlopsStructureBreakdown {
            wing: crate::flops_transport::structure::FlopsWingBreakdown {
                bending_factor: 0.0,
                inertia_relief_factor: 1.0,
                bending_material_kg: 0.0,
                shear_and_control_kg: 0.0,
                miscellaneous_kg: 0.0,
                total_kg: 0.0,
            },
            horizontal_tail_kg: 0.0,
            vertical_tail_kg: 0.0,
            fuselage_kg: 0.0,
            main_gear_kg: 900.0,
            nose_gear_kg: 100.0,
            nacelle_kg: 0.0,
            paint_kg: 0.0,
            total_kg: 5_000.0,
        };
        assert!(replace_gear(&mut structure, 1_500.0));
        assert!((structure.main_gear_kg - 1_350.0).abs() < 1e-9);
        assert!((structure.nose_gear_kg - 150.0).abs() < 1e-9);
        assert!((structure.total_kg - 5_500.0).abs() < 1e-9);
    }
}
