// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Kneeling main gear of a military transport with fuselage-mounted gear.
//!
//! A kneeling main gear lowers the hold sill for loading over the ramp. The
//! Beltramo military line `W4'` (NASA CR-151970, p. 5-22) does not contain
//! one: of its data aircraft only the C-5A kneels, and its kneeling,
//! prepositioning and tyre inflation/deflation weight (5,836 lb) was removed
//! before the fit (Table 5.5, p. 5-20). The report's own adjustment
//! (p. 5-29, `W4G = 0.184 W4'`) is that C-5A lump, not split by function.
//!
//! The kneeling share is priced by Raymer, *Aircraft Design: A Conceptual
//! Approach*, 6th ed., section 15.3.2: the cargo/transport main-gear equation
//! 15.29 (p. 574) multiplies the whole main gear by `K_mp`, "1.126 for
//! kneeling gear; = 1.0 otherwise" (p. 577). The increment is therefore 12.6
//! percent of the main gear as priced (wheels, tyres and brakes included, as
//! in equation 15.29), on the main-gear share only. A kneeling nose gear
//! (Raymer `K_np` 1.15, C-5) is a separate fact and is not priced here.
//!
//! Validity: fuselage-mounted military-transport main gear with a declared
//! kneeling capability. The increment stays inside the C-5A lump: at 141 t the
//! A400M takes 621 kg against `0.184 W4'` = 968 kg.

use crate::flops_transport::structure::FlopsStructureBreakdown;

/// Raymer, 6th ed., p. 577: `K_mp` for a kneeling main gear.
pub const KNEELING_MAIN_GEAR_FACTOR: f64 = 1.126;

/// Kneeling increment, kg, on a main gear of `main_gear_kg`.
pub fn kneeling_increment_kg(main_gear_kg: f64) -> f64 {
    (KNEELING_MAIN_GEAR_FACTOR - 1.0) * main_gear_kg
}

/// Add the kneeling increment to the main gear of `structure` and to its
/// total; the nose gear is unchanged.
pub(super) fn add_kneeling_increment(structure: &mut FlopsStructureBreakdown) {
    let increment_kg = kneeling_increment_kg(structure.main_gear_kg);
    structure.main_gear_kg += increment_kg;
    structure.total_kg += increment_kg;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_a400m_main_gear_takes_the_raymer_kneeling_factor() {
        // 141 t: gear 5,453.4 kg (carbon brakes, low-pressure tyres), main
        // share 0.904192 of the FLOPS split = 4,930.9 kg; 0.126 x 4,930.9 =
        // 621.3 kg, below the C-5A lump 0.184 x 5,260.0 = 967.8 kg.
        let increment = kneeling_increment_kg(4_930.9);
        assert!((increment - 621.3).abs() < 0.05, "{increment}");
        assert!(increment < 0.184 * 5_260.0);
    }

    #[test]
    fn only_the_main_gear_and_the_total_grow() {
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
        add_kneeling_increment(&mut structure);
        // 0.126 x 900 = 113.4 kg.
        assert!((structure.main_gear_kg - 1_013.4).abs() < 1e-9);
        assert!((structure.nose_gear_kg - 100.0).abs() < 1e-12);
        assert!((structure.total_kg - 5_113.4).abs() < 1e-9);
    }
}
