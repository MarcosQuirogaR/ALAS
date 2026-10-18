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
        nose_length_m: Some(10.39),
        nose_windshield_angle_deg: Some(24.2),
        nose_crown_end_fraction: Some(0.79),
        nose_radome_length_fraction: Some(0.1),
        nose_keel_exponent: Some(2.52),
        nose_plan_exponent: Some(2.5),
        nose_section_exponent: Some(2.0),
        belly_upsweep_length_m: Some(17.99),
        aft_belly_exponent: Some(2.5),
        tail_end_z_m: Some(1.46),
        tail_end_width_m: Some(0.32),
        tail_end_height_m: Some(1.48),
        aft_plan_length_m: Some(23.22),
        aft_plan_exponent: Some(1.92),
        aft_crown_length_m: Some(14.07),
        aft_crown_exponent: Some(1.33),
        ..FuselageConfig::default()
    }
}
