// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The drawing-fitted outer nose and aft-body shape of the 787-9 and DC-10
//! fuselages (kept apart from the preset tables to bound their size).

use crate::FuselageConfig;

/// Outer shape fields of the b787_9 fuselage; every other field is the default.
pub(super) fn b787_9() -> FuselageConfig {
    FuselageConfig {
        // FUSELAGE-G2 shaped nose and aft body. Outline traced from the manufacturer drawing
        // (side: same; top: Boeing 787 D6-58333 Rev Q),
        // calibrated to the drawn length; the nose law (windshield, radome, keel and plan
        // exponents) is a least-squares fit to the traced upper, lower and plan lines (RMS 0.177 m),
        // the aft law (belly ramp, crown drop, plan taper, closing section) to the traced aft lines
        // (RMS 0.065 m). Cabin start, tailcone length and every payload and mass station are unchanged:
        // only the outer loft moves. Section exponent 2.0 (ellipse): no drawing measures it.
        nose_length_m: Some(7.85),
        nose_windshield_angle_deg: Some(31.2),
        nose_crown_end_fraction: Some(1.0),
        nose_radome_length_fraction: Some(0.1),
        nose_keel_exponent: Some(3.49),
        nose_plan_exponent: Some(1.84),
        nose_section_exponent: Some(2.0),
        belly_upsweep_length_m: Some(17.15),
        aft_belly_exponent: Some(1.47),
        tail_end_z_m: Some(1.19),
        tail_end_width_m: Some(0.4),
        tail_end_height_m: Some(0.79),
        aft_plan_length_m: Some(16.31),
        aft_plan_exponent: Some(1.28),
        aft_crown_length_m: Some(14.45),
        aft_crown_exponent: Some(1.27),
        // Cabin section: Boeing D6-58333 rev Q fig. 2.5.1 (floor 3.34 m below the crown) and fig. 2.6.2 cross-section, measured at the scale of its own LD-3 outline (1.626 m = 64 in high, 173 px/m, which also gives the container 2.02 m across the top and 1.56 m at the base, the published 2.006 / 1.562 m): hold 1.695 m high, 3.17 m flat floor (two LD-3 bases, 2 x 1.562 m), 4.17 m between the walls, 0.34 m chamfer; the ceiling takes the class floor-beam depth (the drawing carries no skin); cabin-datum-evidence.md.
        cabin_section: Some(crate::CabinSectionConfig {
            cabin_widths: None,
            floor_below_crown_m: Some(3.34),
            hold: Some(crate::HoldSectionConfig {
                ceiling_below_crown_m: None,
                height_m: 1.695,
                floor_width_m: 3.17,
                side_width_m: 4.17,
                chamfer_height_m: 0.34,
            }),
        }),
        ..FuselageConfig::default()
    }
}

/// Outer shape fields of the dc_10 fuselage; every other field is the default.
pub(in crate::presets::widebody::american) fn dc_10() -> FuselageConfig {
    FuselageConfig {
        // FUSELAGE-G2 shaped nose and aft body. Outline traced from the manufacturer drawing
        // (side: same; top: Boeing DC-10 ACAP),
        // calibrated to the drawn length; the nose law (windshield, radome, keel and plan
        // exponents) is a least-squares fit to the traced upper, lower and plan lines (RMS 0.083 m),
        // the aft law (belly ramp, crown drop, plan taper, closing section) to the traced aft lines
        // (RMS 0.095 m). Cabin start, tailcone length and every payload and mass station are unchanged:
        // only the outer loft moves. Section exponent 2.0 (ellipse): no drawing measures it.
        // Data quality: LOW confidence aft: the drawing is a binarised raster crossed by grid lines and the tail engine hides the aft plan edge; read by eye.
        // Aft law read for the hull end at the printed 51.97 m: starts held, closing section on the hull closure; the exponents are not refitted, the built body is the loft.
        nose_length_m: Some(10.39),
        nose_windshield_angle_deg: Some(24.2),
        nose_crown_end_fraction: Some(0.79),
        nose_radome_length_fraction: Some(0.1),
        nose_keel_exponent: Some(2.52),
        nose_plan_exponent: Some(2.5),
        nose_section_exponent: Some(2.0),
        belly_upsweep_length_m: Some(14.61),
        aft_belly_exponent: Some(2.5),
        tail_end_z_m: Some(1.2),
        tail_end_width_m: Some(0.15),
        tail_end_height_m: Some(0.13),
        aft_plan_length_m: Some(19.84),
        aft_plan_exponent: Some(1.92),
        aft_crown_length_m: Some(10.69),
        aft_crown_exponent: Some(1.33),
        // Cabin section: Boeing DC-10 ACAP fig. 2.5.3 (circle fit, 106 px/m): floor 3.40 m, hold ceiling 3.73 m below the crown, 66 in high, 125.5 in floor, 164 in between walls, chamfer 0.45 m; cabin-datum-evidence.md.
        cabin_section: Some(crate::CabinSectionConfig {
            cabin_widths: None,
            floor_below_crown_m: Some(3.4),
            hold: Some(crate::HoldSectionConfig {
                ceiling_below_crown_m: Some(3.73),
                height_m: 1.676,
                floor_width_m: 3.188,
                side_width_m: 4.166,
                chamfer_height_m: 0.45,
            }),
        }),
        ..FuselageConfig::default()
    }
}
