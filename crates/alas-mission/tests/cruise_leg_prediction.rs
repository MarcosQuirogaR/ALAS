// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Route-scale checks for the native cruise step-climb recommendation.

#![allow(clippy::expect_used)]
// The test prints its diagnostic table of measured values.
#![allow(clippy::print_stdout)]

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

    // One leg flies at the top; the altitude fractions stay the climb schedule's speed-change
    // altitudes, which the climb goes through.
    let fractions = |profile: &MissionProfileConfig| {
        (
            profile.initial_climb_altitude_fraction,
            profile.step_climb_1_altitude_fraction,
        )
    };
    let before = fractions(&profile);
    configure_cruise_legs(&mut profile, 1, cruise_altitude_m, 0.0);
    assert_eq!(fractions(&profile), before);
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

/// The proposal's rule, stated in `profile::step_climb`: a cruise leg that follows a step must
/// last long enough for the weight lost in it to raise the optimum level by the step (at constant
/// Mach and lift coefficient the air pressure at the optimum follows the weight, so in the
/// isothermal stratosphere the optimum level rises at `H_p * fuel flow / weight`), the first leg
/// half a step. This bisects the route distance at which the proposal goes from two legs to three
/// and checks it is the distance where the second leg lasts exactly the time of one step.
#[test]
fn three_legs_begin_where_the_second_leg_can_drift_a_full_step() {
    let preset = "AVE";
    let config = AlasConfig::from_value(&serde_json::json!({"preset": preset}))
        .expect("registered preset loads");
    let operational = alas_config::presets::get(preset)
        .expect("registered preset exists")
        .operational_mission_defaults();
    let origin = airport(operational.departure_airport, "ORIG");
    let destination = airport(operational.arrival_airport, "DEST");
    let legs = |route_m: f64| {
        propose_profile_for_route(&config, &origin, &destination, route_m)
            .expect("a valid proposal")
    };
    let (mut low_m, mut high_m) = (1.0e6, 2.0e7);
    assert_eq!(legs(low_m).active_cruise_legs, 1);
    assert_eq!(legs(high_m).active_cruise_legs, 3);
    for _ in 0..60 {
        let mid_m = 0.5 * (low_m + high_m);
        if legs(mid_m).active_cruise_legs == 3 {
            high_m = mid_m;
        } else {
            low_m = mid_m;
        }
    }
    let proposal = legs(high_m);
    assert_eq!(proposal.active_cruise_legs, 3);
    let profile = &proposal.profile;
    let weights = [
        profile.cruise_1_distance_fraction,
        profile.cruise_2_distance_fraction,
        profile.cruise_3_distance_fraction,
    ];
    let total: f64 = weights.iter().sum();
    let cruise_m = high_m - proposal.non_cruise_distance_m;
    let durations_s =
        weights.map(|weight| cruise_m * weight / total / profile.cruise_1_air_speed_m_s);
    let step_s = alas_mission::minimum_cruise_time_for_step_s(TWO_THOUSAND_FEET_M);
    // The first leg needs half a step, each later one a whole step: the threshold is where the
    // most demanding of them just lasts what it needs.
    let margins = [
        durations_s[0] / (0.5 * step_s),
        durations_s[1] / step_s,
        durations_s[2] / step_s,
    ];
    let binding = margins.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(
        (binding - 1.0).abs() < 1.0e-3,
        "durations {durations_s:?} s, step {step_s:.0} s, margins {margins:?}, route {high_m:.0} m"
    );
    println!(
        "{preset}: three legs from {:.0} km (non-cruise {:.0} km, cruise speed {:.1} m/s, step time {:.2} h, durations {:?} h)",
        high_m / 1.0e3,
        proposal.non_cruise_distance_m / 1.0e3,
        profile.cruise_1_air_speed_m_s,
        step_s / 3600.0,
        durations_s.map(|s| s / 3600.0),
    );
}
