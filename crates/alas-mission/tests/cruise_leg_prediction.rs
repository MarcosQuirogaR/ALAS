// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Route-scale checks for the native cruise step-climb recommendation.

#![allow(clippy::expect_used)]

use alas_config::{airports::Airport, AlasConfig, MissionProfileConfig};
use alas_mission::{configure_cruise_legs, propose_profile_for_route};

const TWO_THOUSAND_FEET_M: f64 = 2000.0 * 0.3048;

fn airport(name: &str, icao: &str) -> Airport {
    Airport {
        name: name.to_owned(),
        icao: icao.to_owned(),
        elevation_m: 0.0,
        toda_m: 0.0,
        lda_m: 0.0,
        isa_deviation_c: 0.0,
        notes: String::new(),
        latitude_deg: 0.0,
        longitude_deg: 0.0,
    }
}

#[test]
fn preset_route_recommendations_match_the_observed_adsb_step_scale() {
    // Exact-route Sep 1, 2026 traces support zero climbs on the short
    // A220/A320/ATR sectors and about one climb on the A380/B787 sectors
    // with adequate or partly adequate coverage. The other listed routes are
    // model regression cases, not claimed ADS-B validations.
    let cases = [
        ("A220-300", 466_000.0, 1),
        ("A320-200", 546_000.0, 1),
        ("ATR72-600", 546_000.0, 1),
        ("A340-300", 5_889_000.0, 2),
        ("A380-800", 5_497_000.0, 2),
        ("B787-9", 7_817_000.0, 2),
        ("DC-10", 6_614_000.0, 2),
        ("AVE", 5_497_000.0, 2),
    ];
    for (preset_name, route_distance_m, expected_legs) in cases {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": preset_name}))
            .expect("registered preset loads");
        let operational = alas_config::presets::get(preset_name)
            .expect("registered preset exists")
            .operational_mission_defaults();
        let origin = airport(operational.departure_airport, "ORIG");
        let destination = airport(operational.arrival_airport, "DEST");
        let proposal = propose_profile_for_route(&config, &origin, &destination, route_distance_m)
            .expect("the preset route has a valid proposal");
        assert_eq!(
            proposal.active_cruise_legs, expected_legs,
            "unexpected recommendation for {preset_name}"
        );
    }
}

#[test]
fn selected_cruise_count_builds_a_matching_two_thousand_foot_ladder() {
    let cruise_altitude_m = 11_887.2;
    let mut profile = MissionProfileConfig::default();

    configure_cruise_legs(&mut profile, 1, cruise_altitude_m, 0.0);
    let initial = cruise_altitude_m * profile.initial_climb_altitude_fraction;
    assert!((initial - cruise_altitude_m).abs() < 2.0);
    assert_eq!(profile.cruise_1_distance_fraction, 1.0);
    assert_eq!(profile.cruise_2_distance_fraction, 0.0);
    assert_eq!(profile.cruise_3_distance_fraction, 0.0);

    configure_cruise_legs(&mut profile, 2, cruise_altitude_m, 0.0);
    let initial = cruise_altitude_m * profile.initial_climb_altitude_fraction;
    let step = cruise_altitude_m * profile.step_climb_1_altitude_fraction;
    assert!((initial - (cruise_altitude_m - TWO_THOUSAND_FEET_M)).abs() < 2.0);
    assert!((step - cruise_altitude_m).abs() < 2.0);
    assert_eq!(profile.cruise_3_distance_fraction, 0.0);

    configure_cruise_legs(&mut profile, 3, cruise_altitude_m, 0.0);
    let initial = cruise_altitude_m * profile.initial_climb_altitude_fraction;
    let first_step = cruise_altitude_m * profile.step_climb_1_altitude_fraction;
    assert!((initial - (cruise_altitude_m - 2.0 * TWO_THOUSAND_FEET_M)).abs() < 2.0);
    assert!((first_step - (cruise_altitude_m - TWO_THOUSAND_FEET_M)).abs() < 2.0);
    assert!(profile.cruise_1_distance_fraction > 0.0);
    assert!(profile.cruise_2_distance_fraction > 0.0);
    assert!(profile.cruise_3_distance_fraction > 0.0);
}
