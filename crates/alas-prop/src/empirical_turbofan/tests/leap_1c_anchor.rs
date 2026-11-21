// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The LEAP-1C deck is anchored on the maximum-climb ratio of its type
//! certificate sibling, the LEAP-1A26, instead of the unscaled charts.

use super::*;

/// Two engines of `rating_n` with a maximum-climb datum of `datum_n` each at
/// FL350 (10,668 m), M 0.78, take-off bypass ratio `bpr`. The fuel inputs are
/// the LEAP-1C28 LTO row; the anchor does not read them.
fn leap_class_deck(rating_n: f64, datum_n: f64, bpr: f64) -> EmpiricalTurbofanDeck {
    EmpiricalTurbofanDeck {
        takeoff_thrust_n: 2.0 * rating_n,
        max_climb_reference_thrust_n: 2.0 * datum_n,
        max_climb_reference_altitude_m: 10_668.0,
        max_climb_reference_mach: 0.78,
        bypass_ratio: bpr,
        takeoff_fuel_flow_kg_s: 2.0 * 0.9469,
        cruise_reference_tsfc_kg_kgf_h: 0.51,
        part_power_fuel_flow_ratios: [0.098_277_08, 0.276_881_35, 0.819_840_67, 1.0],
        max_climb_rate_ft_min: None,
        flat_rating_temperature_k: None,
        cruise_reference_is_engine_datum: true,
        cruise_reference_is_maximum_climb: true,
        installation: InstallationLosses::NONE,
    }
}

/// The deck on the BPR 10 cycle of the mission-sizing fixture (OPR 60, FPR
/// 1.45, Tt4 1,670 K): the shared fixture cycle (FPR 1.6) cannot drive a
/// bypass ratio of 10.6. The anchor reads none of the cycle values.
fn leap_class_model(deck: EmpiricalTurbofanDeck) -> EmpiricalTurbofanModel {
    model_from_deck_cycle(deck, 60.0, 1.45, 1_670.0)
}

#[test]
fn the_leap_1c_deck_is_scaled_by_its_siblings_maximum_climb_ratio() {
    // LEAP-1C: 24,773 N (= 23,600 / 120,636 x 126,630) at FL350, M 0.78 over
    // the BPR 10.559 chart there. By hand: weights 0.488 (BPR 8) and 0.512
    // (BPR 13); at M 0.78 the BPR 8 rows read 0.2298 (30,000 ft) and 0.1940
    // (36,000 ft), the BPR 13 rows 0.2220 and 0.1802; at 35,000 ft (pressure
    // weight 0.849 toward 36,000 ft) 0.1994 and 0.1865, blended 0.1928.
    // k = (24,773 / 126,630) / 0.1928 = 1.015.
    let leap_1c = leap_class_model(leap_class_deck(126_630.0, 24_773.0, 10.559));
    let factor = leap_1c.engine_scale().expect("anchored");
    let hand = (24_773.0 / 126_630.0) / 0.1928;
    assert!((factor - hand).abs() < 2.0e-3, "{factor} vs {hand}");
    let flight = standard_flight(10_668.0, 0.78);
    let chart = MaximumClimbTable::new(10.559).ratio(0.78, flight.pressure_pa / 101_325.0);
    assert!((chart - 0.1928).abs() < 1.0e-3, "{chart}");
    // Maximum climb at the point is the datum; maximum cruise is 0.93 of it.
    let climb = leap_1c.climb_available_n(flight, None);
    assert!((climb / (2.0 * 24_773.0) - 1.0).abs() < 1.0e-12, "{climb}");
    assert!((leap_1c.catalogue_cruise_over_maximum_climb() - 1.0).abs() < 1.0e-12);
    let (_, cruise) = leap_1c
        .rating_fraction_and_thrust(flight, PropulsionDemand::Rating(PropulsionRating::Cruise))
        .unwrap_or_else(|error| panic!("cruise: {error}"));
    assert!((cruise / climb - MAXIMUM_CRUISE_OVER_MAXIMUM_CLIMB).abs() < 1.0e-12);
    // The sibling's own factor (LEAP-1A26: 23,600 N over 120.636 kN, BPR
    // 10.706) is within 0.5 %: the same ratio over nearly the same chart.
    let leap_1a = leap_class_model(leap_class_deck(120_636.0, 23_600.0, 10.706));
    let sibling = leap_1a.engine_scale().expect("anchored");
    assert!((factor - sibling).abs() < 5.0e-3, "{factor} vs {sibling}");
    // Before: the class statistic 0.2 F_TO + 890 N = 26,216 N was no datum
    // and the chart ran unscaled, 1.5 % below the anchored deck everywhere.
    let mut before = leap_class_deck(126_630.0, 26_216.0, 10.559);
    before.cruise_reference_is_engine_datum = false;
    before.cruise_reference_is_maximum_climb = false;
    let unscaled = leap_class_model(before);
    assert_eq!(unscaled.engine_scale(), None);
    for (altitude_m, mach) in [(10_668.0, 0.78), (11_887.2, 0.785), (6_000.0, 0.6)] {
        let at = standard_flight(altitude_m, mach);
        let ratio = leap_1c.climb_available_n(at, None) / unscaled.climb_available_n(at, None);
        assert!((ratio - factor).abs() < 1.0e-12, "{altitude_m} m: {ratio}");
    }
}
