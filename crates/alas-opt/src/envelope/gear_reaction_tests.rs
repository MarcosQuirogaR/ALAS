// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The main-gear design reaction against the aircraft weight and against the
//! maximum main-gear load the airport-planning documents publish.

// Tests assert on inputs they constructed here, so a failed expect is the
// assertion failing rather than a library invariant breaking.
#![allow(clippy::expect_used)]

use super::tests::preset_case;
use super::{assess_model_cg_envelope, ModelCgEnvelopeAssessment};

/// Registered presets that place a main-gear station and are assessed.
const ASSESSED_PRESETS: [&str; 11] = [
    "A220-300", "A320-200", "A340-300", "A380-800", "A400M", "AVE", "B747-400", "B787-9", "C919",
    "DC-10", "E195-E2",
];

/// Neutral point at the CG plus `np_offset_mac` x MAC. 0.10 leaves the
/// aerodynamic aft limit ahead of the gear (the ground limit does not govern);
/// 0.60 puts it behind the gear, as the model's does on the wide-bodies,
/// so the ground minimum-nose-load limit governs the sizing.
fn assessment(preset: &str, np_offset_mac: f64) -> (ModelCgEnvelopeAssessment, f64) {
    let (config, plane, masses, coordinates, cg_x) = preset_case(preset);
    let x_np = cg_x + np_offset_mac * plane.c_ref;
    let class_minimum = config.mass_model.pct_load_nlg_min;
    let result = assess_model_cg_envelope(
        &plane,
        &masses,
        &coordinates,
        cg_x,
        x_np,
        x_np,
        plane.c_ref,
        &config,
    )
    .unwrap_or_else(|error| panic!("{preset} must be assessed: {error}"));
    (result, class_minimum)
}

/// No design main reaction may exceed the weight it was sized at, and the
/// nose gear keeps at least the class minimum nose share (`mass_model.pct_load_nlg_min`).
#[test]
fn no_registered_preset_has_a_main_reaction_above_its_design_weight() {
    for np_offset in [0.10, 0.60] {
        let mut floored = 0;
        for preset in ASSESSED_PRESETS {
            let (assessment, class_minimum) = assessment(preset, np_offset);
            let share = assessment.gear_design_main_reaction_kg / assessment.gear_design_weight_kg;
            assert!(
                share <= 1.0 - class_minimum + 1e-9,
                "{preset} (NP offset {np_offset}): main reaction {:.0} kg is {:.1} % of the design weight {:.0} kg",
                assessment.gear_design_main_reaction_kg,
                100.0 * share,
                assessment.gear_design_weight_kg
            );
            if np_offset < 0.2 && assessment.gear_aero_aft_limit_pulled_back {
                floored += 1;
            }
        }
        if np_offset < 0.2 {
            assert!(
                floored < ASSESSED_PRESETS.len(),
                "the unlimited path must be taken for at least one preset"
            );
        }
    }
}

/// Published maximum main-gear load at the most-aft CG, per wheel, kg.
///
/// Boeing 747-400 ACAP D6-58326-1 Rev F section 7.3 (877,000 lb: 204,600 lb
/// per strut, 4 struts of 4 wheels); Boeing 787 ACAP D6-58333 Rev Q section
/// 7.3 (563,000 lb: 259,574 lb per strut, 2 struts of 4 wheels); Airbus A320
/// Aircraft Characteristics section 7-3-0 (78,400 kg: 36,410 kg per strut,
/// 2 struts of 2 wheels). The model sizes the gear at the class minimum nose
/// share (`mass_model.pct_load_nlg_min`), not at the manufacturers' shares:
/// the design main load per wheel is `(1 - f) W / wheels` exactly, and it lies
/// above the published maximum by the difference of the shares (finding
/// F-NOSE-2: 3 to 6 % here, the class share being the lowest certified one).
#[test]
fn the_main_load_per_wheel_follows_the_class_nose_share_above_the_published_load() {
    for (preset, published_kg_per_wheel, wheels) in [
        ("B747-400", 4.0 * 204_600.0 * 0.453_592_37 / 16.0, 16),
        ("B787-9", 2.0 * 259_574.0 * 0.453_592_37 / 8.0, 8),
        ("A320-200", 2.0 * 36_410.0 / 4.0, 4),
    ] {
        let (assessment, class_minimum) = assessment(preset, 0.60);
        let per_wheel = assessment.gear_design_main_reaction_kg / wheels as f64;
        assert!(
            assessment.gear_aero_aft_limit_pulled_back,
            "{preset}: the ground limit must govern in this setup"
        );
        let sized = (1.0 - class_minimum) * assessment.gear_design_weight_kg / wheels as f64;
        assert!(
            (per_wheel / sized - 1.0).abs() < 1.0e-6,
            "{preset}: {per_wheel:.0} kg per wheel against the class-share load {sized:.0} kg"
        );
        let ratio = per_wheel / published_kg_per_wheel;
        assert!(
            ratio > 0.99 && ratio < 1.06,
            "{preset}: {per_wheel:.0} kg per wheel against the published {published_kg_per_wheel:.0} kg"
        );
    }
}
