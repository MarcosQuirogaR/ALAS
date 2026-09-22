// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from the legacy mission-request integration (`build_mission_request`).
// Reference: alas @ rust-port-baseline.

//! The mission half of the mission request.
//!
//! Cruise altitude comes from the design requirements; the departure and
//! arrival field elevations and the departure ISA deviation come from the two
//! airports the route runs between; the route distance is supplied by whoever
//! computed the route (see `alas-route`). The flown profile, every climb,
//! cruise and descent speed and rate, is the user-editable
//! [`MissionProfileConfig`] carried straight through, because those numbers are
//! a configuration the operator tunes rather than anything this builder
//! derives.
//!
//! One field the reference deliberately does *not* read is worth naming, since
//! its presence in the vehicle request invites the assumption: cruise Mach is
//! not part of the mission request. Every cruise segment's air speed is an
//! explicit true airspeed taken from the profile above, never derived from
//! Mach; the vehicle request carries its own cruise Mach for engine sizing, and
//! the two do not meet here.

use alas_atmo::{pressure_isa, temperature_isa, Atmosphere};
use alas_config::airports::Airport;
use alas_config::mission::{resolve_true_airspeed_m_s, MissionProfileConfig, SpeedReference};
use alas_config::AlasConfig;
use serde::Serialize;

/// A route-aware mission-profile proposal for the guided Inputs editor.
///
/// The proposal is deliberately separate from [`MissionRequest`]. A request
/// is the exact profile the caller selected; this type describes a suggested
/// profile before the user accepts it. Registered presets therefore continue
/// to carry their stored profiles unchanged.
#[derive(Debug, Clone, PartialEq)]
pub struct MissionProfileProposal {
    /// The profile to show or apply after explicit user acceptance.
    pub profile: MissionProfileConfig,
    /// Cruise altitude used for the proposal, in metres MSL.
    pub cruise_altitude_m: f64,
    /// Cruise true airspeed derived from the configured cruise Mach, m/s.
    pub cruise_true_airspeed_m_s: f64,
    /// Number of active cruise legs. A value greater than one implies a
    /// physically fitted step climb in the proposed profile.
    pub active_cruise_legs: usize,
    /// Estimated horizontal footprint of non-cruise legs, in metres.
    pub non_cruise_distance_m: f64,
    /// Route distance used to select the proposal, in metres.
    pub route_distance_m: f64,
}

/// A conservative preflight check for a profile retained after a route edit.
///
/// This is intentionally a diagnostic, not a replacement for the pipeline's
/// exact schedule closure. It uses the same units and the selected profile's
/// own speeds/rates, so a retained profile that cannot fit is reported before
/// a run rather than silently accepted by the editor.
#[derive(Debug, Clone, PartialEq)]
pub struct MissionProfileRouteCheck {
    /// Estimated horizontal distance required by climb and descent legs, m.
    pub non_cruise_distance_m: f64,
    /// The route length supplied to the check, m.
    pub route_distance_m: f64,
    /// Whether the estimated non-cruise footprint fits the route.
    pub fits_route: bool,
}

/// Propose a profile for a route using the configuration's declared cruise
/// Mach, cruise altitude and existing climb/descent schedule.
///
/// The number of step climbs is selected from the profile's own vertical
/// levels, rates and speeds. The candidate with the greatest number of active
/// cruise legs is retained only when its estimated non-cruise footprint fits
/// the supplied route; otherwise the next simpler candidate is tried. No
/// route-length constant or aircraft-class threshold is introduced here. A
/// route that is shorter than even the one-cruise footprint returns a one-leg
/// proposal plus a failed route check for the caller to display.
pub fn propose_profile_for_route(
    config: &AlasConfig,
    origin: &Airport,
    dest: &Airport,
    route_distance_m: f64,
) -> Result<MissionProfileProposal, String> {
    if !route_distance_m.is_finite() || route_distance_m < 0.0 {
        return Err(format!(
            "mission proposal route distance must be finite and non-negative, got {route_distance_m}"
        ));
    }

    let cruise_altitude_m = route_cruise_altitude_m(config, origin, dest);
    if !cruise_altitude_m.is_finite() || cruise_altitude_m <= 0.0 {
        return Err(format!(
            "mission proposal needs a finite positive cruise altitude, got {cruise_altitude_m}"
        ));
    }
    let cruise_mach = config.requirements.cruise_mach;
    if !cruise_mach.is_finite() || cruise_mach <= 0.0 {
        return Err(format!(
            "mission proposal needs a finite positive cruise Mach, got {cruise_mach}"
        ));
    }
    let cruise_true_airspeed_m_s =
        cruise_mach * Atmosphere::isa(cruise_altitude_m).speed_of_sound();
    if !cruise_true_airspeed_m_s.is_finite() || cruise_true_airspeed_m_s <= 0.0 {
        return Err("mission proposal produced a non-finite cruise true airspeed".to_owned());
    }

    let mut profile = config.mission.profile.clone();
    profile.cruise_1_air_speed_m_s = cruise_true_airspeed_m_s;
    profile.cruise_2_air_speed_m_s = cruise_true_airspeed_m_s;
    profile.cruise_3_air_speed_m_s = cruise_true_airspeed_m_s;

    let origin_elevation_m = origin.elevation_m;
    let arrival_elevation_m = dest.elevation_m;
    let mut selected_legs = 1;
    let mut selected_distance_m = estimate_non_cruise_distance(
        &profile,
        cruise_altitude_m,
        origin_elevation_m,
        arrival_elevation_m,
        selected_legs,
    )?;
    for active_cruise_legs in (1..=3).rev() {
        let candidate_distance_m = estimate_non_cruise_distance(
            &profile,
            cruise_altitude_m,
            origin_elevation_m,
            arrival_elevation_m,
            active_cruise_legs,
        )?;
        if candidate_distance_m <= route_distance_m {
            selected_legs = active_cruise_legs;
            selected_distance_m = candidate_distance_m;
            break;
        }
        // Keep the one-leg candidate even when this route cannot accommodate
        // its full declared altitude ladder. The exact pipeline closure will
        // scale that ladder or report the physical shortfall.
        if active_cruise_legs == 1 {
            selected_distance_m = candidate_distance_m;
        }
    }

    set_active_cruise_legs(&mut profile, selected_legs);
    Ok(MissionProfileProposal {
        profile,
        cruise_altitude_m,
        cruise_true_airspeed_m_s,
        active_cruise_legs: selected_legs,
        non_cruise_distance_m: selected_distance_m,
        route_distance_m,
    })
}

/// Check a profile after a route change without changing any configuration.
pub fn check_profile_for_route(
    config: &AlasConfig,
    origin: &Airport,
    dest: &Airport,
    route_distance_m: f64,
) -> Result<MissionProfileRouteCheck, String> {
    if !route_distance_m.is_finite() || route_distance_m < 0.0 {
        return Err(format!(
            "mission profile route distance must be finite and non-negative, got {route_distance_m}"
        ));
    }
    let cruise_altitude_m = route_cruise_altitude_m(config, origin, dest);
    let active_cruise_legs = active_cruise_legs(&config.mission.profile);
    let non_cruise_distance_m = estimate_non_cruise_distance(
        &config.mission.profile,
        cruise_altitude_m,
        origin.elevation_m,
        dest.elevation_m,
        active_cruise_legs,
    )?;
    Ok(MissionProfileRouteCheck {
        non_cruise_distance_m,
        route_distance_m,
        fits_route: non_cruise_distance_m <= route_distance_m,
    })
}

fn active_cruise_legs(profile: &MissionProfileConfig) -> usize {
    [
        profile.cruise_1_distance_fraction,
        profile.cruise_2_distance_fraction,
        profile.cruise_3_distance_fraction,
    ]
    .iter()
    .rposition(|fraction| fraction.is_finite() && *fraction > 1.0e-9)
    .map_or(1, |index| index + 1)
}

fn set_active_cruise_legs(profile: &mut MissionProfileConfig, count: usize) {
    let count = count.clamp(1, 3);
    let fractions = match count {
        1 => [1.0, 0.0, 0.0],
        2 => [0.5, 0.5, 0.0],
        _ => {
            let total = profile.cruise_1_distance_fraction.max(0.0)
                + profile.cruise_2_distance_fraction.max(0.0)
                + profile.cruise_3_distance_fraction.max(0.0);
            if total > 0.0 {
                [
                    profile.cruise_1_distance_fraction.max(0.0) / total,
                    profile.cruise_2_distance_fraction.max(0.0) / total,
                    profile.cruise_3_distance_fraction.max(0.0) / total,
                ]
            } else {
                [1.0 / 3.0; 3]
            }
        }
    };
    profile.cruise_1_distance_fraction = fractions[0];
    profile.cruise_2_distance_fraction = fractions[1];
    profile.cruise_3_distance_fraction = fractions[2];
}

fn estimate_non_cruise_distance(
    profile: &MissionProfileConfig,
    cruise_altitude_m: f64,
    departure_elevation_m: f64,
    arrival_elevation_m: f64,
    active_cruise_legs: usize,
) -> Result<f64, String> {
    let mut current_m = departure_elevation_m;
    let mut distance_m = 0.0;
    let leg = |current_m: &mut f64,
               distance_m: &mut f64,
               target_m: f64,
               speed_m_s: f64,
               rate_m_s: f64,
               midpoint_m: f64| {
        let delta_m = (target_m - *current_m).abs();
        if delta_m <= 1.0e-9 {
            *current_m = target_m;
            return Ok::<(), String>(());
        }
        let speed_m_s = resolved_profile_speed(profile, speed_m_s, midpoint_m)?;
        if !speed_m_s.is_finite() || speed_m_s <= 0.0 || !rate_m_s.is_finite() || rate_m_s <= 0.0 {
            return Err(format!(
                "mission proposal needs positive finite speed/rate, got speed={speed_m_s}, rate={rate_m_s}"
            ));
        }
        if speed_m_s <= rate_m_s {
            return Err(format!(
                "mission proposal vertical rate {rate_m_s} m/s is not below airspeed {speed_m_s} m/s"
            ));
        }
        let horizontal_speed_m_s = (speed_m_s * speed_m_s - rate_m_s * rate_m_s).sqrt();
        *distance_m += delta_m / rate_m_s * horizontal_speed_m_s;
        *current_m = target_m;
        Ok(())
    };

    leg(
        &mut current_m,
        &mut distance_m,
        departure_elevation_m + profile.takeoff_altitude_gain_m,
        profile.takeoff_air_speed_m_s,
        profile.takeoff_climb_rate_m_s,
        departure_elevation_m,
    )?;
    let first_level_m = (cruise_altitude_m * profile.initial_climb_altitude_fraction)
        .max(departure_elevation_m + 3000.0);
    let first_level_midpoint_m = 0.5 * (current_m + first_level_m);
    leg(
        &mut current_m,
        &mut distance_m,
        first_level_m,
        profile.initial_climb_air_speed_m_s,
        profile.initial_climb_rate_m_s,
        first_level_midpoint_m,
    )?;
    if active_cruise_legs >= 2 {
        let second_level_m =
            (cruise_altitude_m * profile.step_climb_1_altitude_fraction).max(first_level_m + 300.0);
        let second_level_midpoint_m = 0.5 * (current_m + second_level_m);
        leg(
            &mut current_m,
            &mut distance_m,
            second_level_m,
            profile.step_climb_1_air_speed_m_s,
            profile.step_climb_1_rate_m_s,
            second_level_midpoint_m,
        )?;
    }
    if active_cruise_legs >= 3 {
        let cruise_midpoint_m = 0.5 * (current_m + cruise_altitude_m);
        leg(
            &mut current_m,
            &mut distance_m,
            cruise_altitude_m,
            profile.step_climb_2_air_speed_m_s,
            profile.step_climb_2_rate_m_s,
            cruise_midpoint_m,
        )?;
    }

    for (altitude_ft, speed_m_s, rate_m_s) in [
        (
            profile.descent_1_altitude_ft,
            profile.descent_1_air_speed_m_s,
            profile.descent_1_rate_m_s,
        ),
        (
            profile.descent_2_altitude_ft,
            profile.descent_2_air_speed_m_s,
            profile.descent_2_rate_m_s,
        ),
        (
            profile.descent_3_altitude_ft,
            profile.descent_3_air_speed_m_s,
            profile.descent_3_rate_m_s,
        ),
        (
            profile.descent_4_altitude_ft,
            profile.descent_4_air_speed_m_s,
            profile.descent_4_rate_m_s,
        ),
    ] {
        let target_m = altitude_ft * 0.3048;
        if target_m > arrival_elevation_m && target_m < current_m {
            let descent_midpoint_m = 0.5 * (current_m + target_m);
            leg(
                &mut current_m,
                &mut distance_m,
                target_m,
                speed_m_s,
                rate_m_s,
                descent_midpoint_m,
            )?;
        }
    }
    if arrival_elevation_m < current_m {
        let landing_midpoint_m = 0.5 * (current_m + arrival_elevation_m);
        leg(
            &mut current_m,
            &mut distance_m,
            arrival_elevation_m,
            profile.landing_air_speed_m_s,
            profile.landing_descent_rate_m_s,
            landing_midpoint_m,
        )?;
    }
    Ok(distance_m)
}

fn resolved_profile_speed(
    profile: &MissionProfileConfig,
    configured_speed_m_s: f64,
    altitude_m: f64,
) -> Result<f64, String> {
    match profile.climb_descent_speed_reference {
        SpeedReference::TrueAirspeed => Ok(configured_speed_m_s),
        SpeedReference::CalibratedAirspeed => resolve_true_airspeed_m_s(
            SpeedReference::CalibratedAirspeed,
            configured_speed_m_s,
            pressure_isa(altitude_m),
            temperature_isa(altitude_m),
        )
        .map_err(|error| {
            format!("mission proposal could not resolve calibrated airspeed: {error}")
        }),
    }
}

/// The mission half of the request handed to the segment network.
///
/// The field names are the JSON keys the reference's subprocess boundary used,
/// so the document this serializes to is the one the mission evaluator reads.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MissionRequest {
    /// `"{origin_icao}_to_{dest_icao}"`, the tag the mission is logged under.
    pub mission_tag: String,
    /// The design cruise altitude, in metres.
    pub cruise_altitude_m: f64,
    /// The departure field's elevation, in metres.
    pub departure_elevation_m: f64,
    /// The arrival field's elevation, in metres.
    pub arrival_elevation_m: f64,
    /// The departure field's ISA temperature deviation, in Celsius.
    pub departure_isa_deviation_c: f64,
    /// The total route distance, in metres, after which the profile's climb
    /// and descent legs leave a remainder for the cruise legs.
    pub route_distance_m: f64,
    /// The flown speed, rate and altitude schedule, carried through unchanged.
    pub profile: MissionProfileConfig,
}

/// The cruise altitude this route is actually flown at, in metres MSL.
///
/// `requirements.cruise_altitude_m` is the *sizing* cruise altitude: the
/// design point the wing, the engine deck and the drag polar are built at. It
/// is not a flight level a dispatcher would file for an arbitrary sector, and
/// on a short one it cannot be: the A320-200's LEMD-LEPA sector is 546 km,
/// while the climb-cruise-descent ladder to the 11 278 m design altitude needs
/// 743 km. Flying the design altitude on the declared route therefore reported
/// a range shortfall that is an artefact of using a sizing condition as an
/// operational one, not a property of the aircraft.
///
/// A registered aircraft already declares the route-appropriate altitude next
/// to the route itself, with its own provenance
/// ([`alas_config::AircraftPreset::operational_mission_defaults`]: 28 000 ft
/// for the A320-200, 25 000 ft for the A220-300, 17 000 ft for the ATR
/// 72-600). That declared value is used when the configuration is still flying
/// that preset's own declared city pair. Any other route, any edited airport
/// and any unregistered configuration keeps the design altitude, because
/// nothing else has been declared for it - this resolves a declared datum, it
/// does not derive or cap one.
/// Public so the optimizer's own sizing mission model can fly the same
/// altitude the published mission does. Two models of one mission that
/// disagree about the flight level disagree about everything downstream of
/// it: the optimizer sized the A320-200's declared 546 km sector against the
/// ladder to its 11 278 m design point and rejected every candidate on
/// `mission_profile_range`, while the published mission flew the declared
/// 28 000 ft and reached the destination.
pub fn route_cruise_altitude_m(config: &AlasConfig, origin: &Airport, dest: &Airport) -> f64 {
    let design_altitude_m = config.requirements.cruise_altitude_m;
    let Ok(preset) = alas_config::presets::get(&config.preset) else {
        return design_altitude_m;
    };
    let operational = preset.operational_mission_defaults();
    // The declared defaults name airports the way the registry displays them,
    // which is what the configuration carries, so the comparison is against
    // the same strings the preset loader wrote.
    let flying_declared_route = config.departure_airport == operational.departure_airport
        && config.arrival_airport == operational.arrival_airport
        && origin.name == operational.departure_airport
        && dest.name == operational.arrival_airport;
    if flying_declared_route && operational.cruise_altitude_m > 0.0 {
        operational.cruise_altitude_m
    } else {
        design_altitude_m
    }
}

/// Build the mission half of the request for a route between two airports.
///
/// `route_distance_m` is supplied rather than computed here: the great-circle
/// or filed distance is `alas-route`'s concern, and the cruise legs split
/// whatever it hands in.
pub fn build_mission_request(
    config: &AlasConfig,
    origin: &Airport,
    dest: &Airport,
    route_distance_m: f64,
) -> MissionRequest {
    MissionRequest {
        mission_tag: format!("{}_to_{}", origin.icao, dest.icao),
        cruise_altitude_m: route_cruise_altitude_m(config, origin, dest),
        departure_elevation_m: origin.elevation_m,
        arrival_elevation_m: dest.elevation_m,
        departure_isa_deviation_c: origin.isa_deviation_c,
        route_distance_m,
        profile: config.mission.profile.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn airport(icao: &str, elevation_m: f64, isa_deviation_c: f64) -> Airport {
        Airport {
            name: icao.to_owned(),
            icao: icao.to_owned(),
            elevation_m,
            toda_m: 0.0,
            lda_m: 0.0,
            isa_deviation_c,
            notes: String::new(),
            latitude_deg: 0.0,
            longitude_deg: 0.0,
        }
    }

    // What parity cannot see: the tag is assembled from the two ICAO codes in
    // departure-then-arrival order, so a request whose endpoints were swapped
    // reads differently even when every distance is the same.
    #[test]
    fn mission_tag_reads_origin_then_destination() {
        let config = AlasConfig::default();
        let request = build_mission_request(
            &config,
            &airport("LEMD", 0.0, 0.0),
            &airport("EGLL", 0.0, 0.0),
            1000.0,
        );
        assert_eq!(request.mission_tag, "LEMD_to_EGLL");
    }

    // The departure ISA deviation and elevation are read off the *origin*, and
    // the arrival elevation off the *destination*: a builder that read either
    // from the wrong airport would still produce a well-formed request.
    #[test]
    fn elevations_and_deviation_come_from_the_named_airports() {
        let config = AlasConfig::default();
        let request = build_mission_request(
            &config,
            &airport("AAAA", 610.0, 12.0),
            &airport("BBBB", 4.0, -3.0),
            4_242_424.0,
        );
        assert_eq!(request.departure_elevation_m, 610.0);
        assert_eq!(request.departure_isa_deviation_c, 12.0);
        assert_eq!(request.arrival_elevation_m, 4.0);
        assert_eq!(request.route_distance_m, 4_242_424.0);
    }

    fn named(name: &str, icao: &str) -> Airport {
        Airport {
            name: name.to_owned(),
            ..airport(icao, 0.0, 0.0)
        }
    }

    // The sizing cruise altitude and the altitude the declared sector is
    // actually flown at are different quantities, and the A320-200 is where
    // treating them as one produced a range shortfall: its 546 km LEMD-LEPA
    // sector is shorter than the ladder to the 11 278 m design point.
    #[test]
    fn a_preset_flying_its_own_declared_route_cruises_at_the_declared_altitude() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"}))
            .expect("the registered A320-200 loads");
        let request = build_mission_request(
            &config,
            &named("Madrid Barajas (LEMD)", "LEMD"),
            &named("Palma de Mallorca (LEPA)", "LEPA"),
            546_202.0,
        );
        let declared = alas_config::presets::get("A320-200")
            .expect("registered")
            .operational_mission_defaults()
            .cruise_altitude_m;
        assert_eq!(request.cruise_altitude_m, declared);
        // 28 000 ft, and strictly below the design point rather than merely
        // different from it.
        assert!((request.cruise_altitude_m - 28_000.0 * 0.3048).abs() < 1e-6);
        assert!(request.cruise_altitude_m < config.requirements.cruise_altitude_m);
    }

    // The negative control that keeps this a resolution of declared data
    // rather than a general altitude cap: fly the same aircraft somewhere it
    // has declared nothing about and the design altitude is what stands.
    #[test]
    fn a_route_the_preset_does_not_declare_keeps_the_design_altitude() {
        let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"}))
            .expect("the registered A320-200 loads");
        config.arrival_airport = "London Heathrow (EGLL)".to_owned();
        let request = build_mission_request(
            &config,
            &named("Madrid Barajas (LEMD)", "LEMD"),
            &named("London Heathrow (EGLL)", "EGLL"),
            1_264_000.0,
        );
        assert_eq!(
            request.cruise_altitude_m,
            config.requirements.cruise_altitude_m
        );
    }

    // An unregistered configuration has declared no operational altitude at
    // all, so there is nothing to resolve and the design point is used.
    #[test]
    fn an_unregistered_configuration_keeps_the_design_altitude() {
        let config = AlasConfig::default();
        let request = build_mission_request(
            &config,
            &named("Madrid Barajas (LEMD)", "LEMD"),
            &named("Palma de Mallorca (LEPA)", "LEPA"),
            546_202.0,
        );
        assert_eq!(
            request.cruise_altitude_m,
            config.requirements.cruise_altitude_m
        );
    }

    #[test]
    fn a_short_declared_sector_does_not_receive_the_long_route_step_ladder() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"}))
            .expect("the registered A320-200 loads");
        let proposal = propose_profile_for_route(
            &config,
            &named("Madrid Barajas (LEMD)", "LEMD"),
            &named("Palma de Mallorca (LEPA)", "LEPA"),
            546_202.0,
        )
        .expect("the route has a valid declared sizing profile");

        assert_eq!(proposal.active_cruise_legs, 1);
        assert_eq!(proposal.profile.cruise_2_distance_fraction, 0.0);
        assert_eq!(proposal.profile.cruise_3_distance_fraction, 0.0);
        assert!(proposal.profile.cruise_1_distance_fraction > 0.0);
        assert!(proposal.cruise_true_airspeed_m_s.is_finite());
    }

    #[test]
    fn a_long_route_keeps_the_declared_three_leg_profile() {
        let config = AlasConfig::default();
        let proposal = propose_profile_for_route(
            &config,
            &airport("AAAA", 0.0, 0.0),
            &airport("BBBB", 0.0, 0.0),
            12_000_000.0,
        )
        .expect("the long route has a valid default profile");

        assert_eq!(proposal.active_cruise_legs, 3);
        assert!(proposal.profile.cruise_1_distance_fraction > 0.0);
        assert!(proposal.profile.cruise_2_distance_fraction > 0.0);
        assert!(proposal.profile.cruise_3_distance_fraction > 0.0);
        assert!(proposal.non_cruise_distance_m <= proposal.route_distance_m);
    }

    #[test]
    fn a_retained_profile_is_reported_when_its_route_is_too_short() {
        let config = AlasConfig::default();
        let check = check_profile_for_route(
            &config,
            &airport("AAAA", 0.0, 0.0),
            &airport("BBBB", 0.0, 0.0),
            1.0,
        )
        .expect("the check returns a diagnostic for a short route");

        assert!(!check.fits_route);
        assert!(check.non_cruise_distance_m > check.route_distance_m);
    }
}
