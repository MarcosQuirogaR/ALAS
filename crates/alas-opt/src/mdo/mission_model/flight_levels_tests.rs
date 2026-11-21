// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The legal-level rule and the step-climb trigger (`flight_levels`).

use super::super::{highest_qualifying_m, step_increment_m, LEVEL_RESOLUTION_M};
use super::*;

/// Flight level `fl` in metres.
fn fl_m(fl: f64) -> f64 {
    fl * 100.0 * FOOT
}

/// The flight level of `altitude_m`.
fn fl_of(altitude_m: f64) -> f64 {
    altitude_m / (100.0 * FOOT)
}

/// The initial level is the ICAO cruising level at or below the capability:
/// every 1,000 ft below FL410 (both directions together), every 2,000 ft from
/// FL410 up; a level is its own legal level, also after the metre round trip.
#[test]
fn the_initial_level_is_the_icao_level_at_or_below() {
    for (altitude_m, expected_fl) in [
        (9_398.736, 300.0), // DC-10 charted trip capability-limited level, FL308.4
        (9_461.95, 310.0),  // DC-10 charted capability, FL310.4
        (10_668.0, 350.0),  // FL350 exactly (configured DC-10 level)
        (10_667.9, 340.0),
        (11_887.2, 390.0), // FL390 exactly
        (12_496.8, 410.0), // FL410 exactly
        (12_800.0, 410.0), // FL419.9: no level between 410 and 430
        (13_106.4, 430.0), // FL430
        (13_700.0, 430.0), // FL449.5
        (13_716.0, 450.0), // FL450
        (3_048.0, 100.0),
    ] {
        let legal_m = legal_level_at_or_below_m(altitude_m);
        assert!(
            (fl_of(legal_m) - expected_fl).abs() < 1.0e-9,
            "{altitude_m} m -> FL{}",
            fl_of(legal_m)
        );
        assert!(legal_m <= altitude_m + 1.0e-6, "{altitude_m} m rounded up");
        assert!(
            altitude_m - legal_m < legal_level_spacing_m(legal_m) + 1.0e-6,
            "{altitude_m} m dropped more than one level"
        );
        assert_eq!(
            legal_level_at_or_below_m(legal_m).to_bits(),
            legal_m.to_bits()
        );
    }
    assert!(legal_level_at_or_below_m(f64::NAN).is_nan());
}

/// A step climb goes to the next level of the same direction (ICAO Annex 2,
/// App. 3, table a): 2,000 ft up to FL410, then FL400 -> 430 and FL410 ->
/// 450, and 4,000 ft above; every step from a legal level lands on a legal
/// level, and the frozen plan's level bookkeeping steps the same way.
#[test]
fn a_step_climbs_to_the_next_level_of_the_same_direction() {
    for (from_fl, to_fl) in [
        (300.0, 320.0),
        (310.0, 330.0),
        (370.0, 390.0),
        (390.0, 410.0),
        (400.0, 430.0),
        (410.0, 450.0),
        (430.0, 470.0),
        (450.0, 490.0),
    ] {
        let next_m = next_same_direction_level_m(fl_m(from_fl));
        assert!(
            (fl_of(next_m) - to_fl).abs() < 1.0e-9,
            "FL{from_fl} -> FL{}",
            fl_of(next_m)
        );
        assert_eq!(
            legal_level_at_or_below_m(next_m).to_bits(),
            next_m.to_bits()
        );
        assert!((step_increment_m(fl_m(from_fl)) - (next_m - fl_m(from_fl))).abs() < 1.0e-9);
    }
    // A continuous (prescribed, lowered) level keeps the band's separation.
    let lowered_m = 9_000.0;
    assert!((step_increment_m(lowered_m) - 2_000.0 * FOOT).abs() < 1.0e-9);
}

/// A chain of steps from every legal level stays on legal levels bit for bit
/// (the metre-foot round trip of the stepped value is snapped away).
#[test]
fn step_chains_stay_on_legal_levels_bit_for_bit() {
    for start_fl in (10..600).step_by(10) {
        let mut level_m = legal_level_at_or_below_m(fl_m(f64::from(start_fl)));
        for _ in 0..6 {
            level_m = next_same_direction_level_m(level_m);
            assert_eq!(
                legal_level_at_or_below_m(level_m).to_bits(),
                level_m.to_bits(),
                "FL{start_fl} chain reached {level_m} m"
            );
        }
    }
}

/// A configured level stored as metres rounded from a flight level is read
/// as that flight level: 5,180 m (FL169.95, FL170 = 5,181.6 m) caps at
/// FL170, not FL160; an exact level is kept bit for bit.
#[test]
fn a_configured_level_in_rounded_metres_is_the_flight_level_it_names() {
    let cap_m = configured_level_m(5_180.0);
    assert_eq!(
        cap_m.to_bits(),
        legal_level_at_or_below_m(fl_m(170.0)).to_bits()
    );
    assert_eq!(legal_level_at_or_below_m(cap_m).to_bits(), cap_m.to_bits());
    assert!((fl_of(legal_level_at_or_below_m(5_180.0)) - 160.0).abs() < 1.0e-9);
    for fl in [170.0, 350.0, 410.0] {
        let exact_m = legal_level_at_or_below_m(fl_m(fl));
        assert_eq!(configured_level_m(exact_m).to_bits(), exact_m.to_bits());
    }
    assert!((configured_level_m(10_660.0) - fl_m(350.0)).abs() < 1.0e-9);
    assert!(configured_level_m(f64::NAN).is_nan());
}

/// The step-climb trigger: the next level must qualify and give the better
/// specific air range; equal ranges do not step (the crossover itself), and
/// a level that does not qualify never steps, however much range it offers.
#[test]
fn a_step_is_due_at_the_specific_range_crossover_of_a_qualifying_level() {
    assert!(step_climb_due(true, 0.100, 0.101));
    assert!(!step_climb_due(true, 0.100, 0.100));
    assert!(!step_climb_due(true, 0.100, 0.099));
    assert!(!step_climb_due(false, 0.100, 0.200));
    assert!(!step_climb_due(true, f64::NAN, 0.2));
    assert!(!step_climb_due(true, 0.1, f64::NAN));
}

/// `min(configured, capability)` rounded down: the rule never plans above the
/// configured level or the capability, and the rounded level qualifies when
/// the capability does (a margin falling with altitude, as above the Mach
/// crossover).
#[test]
fn the_planned_level_is_the_legal_level_under_both_bounds() {
    let floor_m = 3_048.0;
    for (cap_m, capability_m) in [
        (10_668.0, 9_461.95), // capability governs: FL310
        (10_668.0, 11_500.0), // configured governs: FL350
        (11_887.2, 11_160.7), // A340 closure capability: FL360
        (8_200.0, 9_000.0),   // a short route's fit level: FL260
    ] {
        let margin = |h: f64| capability_m - h;
        let found_m = highest_qualifying_m(floor_m, cap_m, margin);
        let planned_m = legal_level_at_or_below_m(found_m).max(floor_m);
        assert!(planned_m <= cap_m + 1.0e-6 && planned_m <= capability_m + 1.0e-6);
        assert!(margin(planned_m) >= 0.0);
        assert!(found_m - planned_m < legal_level_spacing_m(planned_m) + LEVEL_RESOLUTION_M);
        assert_eq!(
            legal_level_at_or_below_m(planned_m).to_bits(),
            planned_m.to_bits()
        );
    }
}
