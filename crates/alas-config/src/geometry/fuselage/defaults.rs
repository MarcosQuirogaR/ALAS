// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Default fuselage configuration.

use super::*;

impl Default for FuselageConfig {
    fn default() -> Self {
        // Circular twin-aisle body: 6 m nose taper, long cabin, 14 m upswept tailcone.
        Self {
            diameter_m: 6.2,
            height_m: None,
            nose_z_m: -0.5,
            cabin_start_x_m: 6.0,
            cabin_z_m: 0.2,
            tailcone_length_m: 14.0,
            tail_z_m: 1.8,
            belly_upsweep_length_m: None,
            nose_windshield_angle_deg: None,
            nose_crown_end_fraction: None,
            nose_radome_length_fraction: None,
            nose_keel_exponent: None,
            nose_plan_exponent: None,
            nose_section_exponent: None,
            nose_length_m: None,
            tail_end_width_m: None,
            tail_end_height_m: None,
            tail_end_z_m: None,
            aft_belly_exponent: None,
            gear_sponson_length_m: None,
            gear_sponson_half_width_m: None,
            gear_sponson_depth_m: None,
            aft_plan_length_m: None,
            aft_plan_exponent: None,
            aft_crown_length_m: None,
            aft_crown_exponent: None,
            hold_interior_end_x_m: None,
            hump_height_m: None,
            hump_start_x_m: None,
            hump_crown_start_x_m: None,
            hump_crown_end_x_m: None,
            hump_end_x_m: None,
            hump_fairing_exponent: None,
            upper_deck_floor_height_m: None,
            upper_deck_start_x_m: None,
            upper_deck_end_x_m: None,
            custom_sections: Vec::new(),
            generated_sections: Vec::new(),
            cabin_section: None,
            loft: None,
            n_subdivisions: 12,
        }
    }
}
