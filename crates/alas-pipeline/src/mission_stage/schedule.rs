// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission profile construction and route-distance closure.

use alas_atmo::us1976_try_compute_values;
use alas_config::mission::{resolve_true_airspeed_m_s, SpeedReference};
use alas_mission::segments::{SegmentKind, SegmentSpec};
use alas_mission::MissionRequest;

mod closure;
use closure::profile_horizontal_distance_of_schedule;
pub(super) use closure::{
    close_schedule_distance, fit_altitude_profile, schedule_horizontal_distance,
};

const CONTROL_POINTS: usize = 16;
const METRES_PER_FOOT: f64 = 0.3048;
const DISTANCE_TOLERANCE_M: f64 = 1.0e-6;
const ALTITUDE_TOLERANCE_M: f64 = 1.0e-9;
const PROFILE_SCALE_ITERATIONS: usize = 64;

pub(super) fn build_schedule(request: &MissionRequest) -> Result<Vec<SegmentSpec>, String> {
    let p = &request.profile;
    if !request.route_distance_m.is_finite() || request.route_distance_m < 0.0 {
        return Err(format!(
            "mission route distance must be finite and non-negative, got {}",
            request.route_distance_m
        ));
    }
    validate_profile_inputs(request)?;
    // Every configured climb/descent has vertical rate below true airspeed,
    // so changing elevation requires positive horizontal distance. Check
    // this before altitude scaling can discard opposite-direction legs.
    if request.route_distance_m <= DISTANCE_TOLERANCE_M
        && (request.arrival_elevation_m - request.departure_elevation_m).abs()
            > ALTITUDE_TOLERANCE_M
    {
        return Err("mission route distance cannot connect the airport elevations".to_owned());
    }
    let cruise_fractions = [
        p.cruise_1_distance_fraction,
        p.cruise_2_distance_fraction,
        p.cruise_3_distance_fraction,
    ];
    if cruise_fractions
        .iter()
        .any(|fraction| !fraction.is_finite() || *fraction < 0.0)
    {
        return Err("mission cruise distance fractions must be finite and non-negative".to_owned());
    }
    let cruise_altitude = request.cruise_altitude_m;
    let first_level = (cruise_altitude * p.initial_climb_altitude_fraction)
        .max(request.departure_elevation_m + 3000.0);
    let second_level =
        (cruise_altitude * p.step_climb_1_altitude_fraction).max(first_level + 300.0);
    let temperature_deviation_k = request.departure_isa_deviation_c;
    let reference = p.climb_descent_speed_reference;
    let mut schedule = Vec::with_capacity(12);

    schedule.push(climb(
        "takeoff",
        Some(request.departure_elevation_m),
        request.departure_elevation_m + p.takeoff_altitude_gain_m,
        p.takeoff_air_speed_m_s,
        reference,
        p.takeoff_climb_rate_m_s,
        temperature_deviation_k,
    ));
    schedule.push(climb(
        "initial_climb",
        None,
        first_level,
        p.initial_climb_air_speed_m_s,
        reference,
        p.initial_climb_rate_m_s,
        temperature_deviation_k,
    ));
    let active_cruise = [
        p.cruise_1_distance_fraction > 0.0,
        p.cruise_2_distance_fraction > 0.0,
        p.cruise_3_distance_fraction > 0.0,
    ];
    if active_cruise[0] {
        schedule.push(cruise(
            "cruise_step_1",
            None,
            0.0,
            p.cruise_1_air_speed_m_s,
            temperature_deviation_k,
        ));
    }
    if active_cruise[1] {
        schedule.push(climb(
            "step_climb_1",
            None,
            second_level,
            p.step_climb_1_air_speed_m_s,
            reference,
            p.step_climb_1_rate_m_s,
            temperature_deviation_k,
        ));
        schedule.push(cruise(
            "cruise_step_2",
            None,
            0.0,
            p.cruise_2_air_speed_m_s,
            temperature_deviation_k,
        ));
    }
    if active_cruise[2] {
        schedule.push(climb(
            "step_climb_2",
            None,
            cruise_altitude,
            p.step_climb_2_air_speed_m_s,
            reference,
            p.step_climb_2_rate_m_s,
            temperature_deviation_k,
        ));
        schedule.push(cruise(
            "cruise_step_3",
            None,
            0.0,
            p.cruise_3_air_speed_m_s,
            temperature_deviation_k,
        ));
    }

    let descent_steps = [
        (
            p.descent_1_altitude_ft,
            p.descent_1_air_speed_m_s,
            p.descent_1_rate_m_s,
        ),
        (
            p.descent_2_altitude_ft,
            p.descent_2_air_speed_m_s,
            p.descent_2_rate_m_s,
        ),
        (
            p.descent_3_altitude_ft,
            p.descent_3_air_speed_m_s,
            p.descent_3_rate_m_s,
        ),
        (
            p.descent_4_altitude_ft,
            p.descent_4_air_speed_m_s,
            p.descent_4_rate_m_s,
        ),
    ];
    let arrival_ft = request.arrival_elevation_m / METRES_PER_FOOT;
    for (index, (altitude_ft, speed, rate)) in descent_steps.into_iter().enumerate() {
        if altitude_ft > arrival_ft {
            schedule.push(descent(
                &format!("descent_{}", index + 1),
                None,
                altitude_ft * METRES_PER_FOOT,
                speed,
                reference,
                rate,
                temperature_deviation_k,
            ));
        }
    }
    schedule.push(descent(
        "final_landing",
        None,
        request.arrival_elevation_m,
        p.landing_air_speed_m_s,
        reference,
        p.landing_descent_rate_m_s,
        temperature_deviation_k,
    ));

    // Preserve the full configured altitude profile when it fits. For a short
    // route, reduce its altitude excursion continuously while keeping both
    // airport elevations fixed. This is a profile adaptation, not a minimum
    // route-length rule: climb/descent footprint depends on the selected
    // altitudes, rates and speeds.
    schedule = fit_altitude_profile(
        &schedule,
        request.departure_elevation_m,
        request.arrival_elevation_m,
        request.route_distance_m,
    )?;
    let non_cruise_distance_m =
        profile_horizontal_distance_of_schedule(&schedule, request.departure_elevation_m);
    if !non_cruise_distance_m.is_finite() {
        return Err(
            "mission profile produces a non-finite climb/descent horizontal distance".to_owned(),
        );
    }
    if request.route_distance_m + DISTANCE_TOLERANCE_M < non_cruise_distance_m {
        return Err(format!(
            "mission route distance {:.3} m cannot accommodate the {:.3} m horizontal footprint required to connect the airport elevations",
            request.route_distance_m, non_cruise_distance_m
        ));
    }
    let cruise_remainder_m = (request.route_distance_m - non_cruise_distance_m).max(0.0);
    let cruise_shares = [
        ("cruise_step_1", p.cruise_1_distance_fraction),
        ("cruise_step_2", p.cruise_2_distance_fraction),
        ("cruise_step_3", p.cruise_3_distance_fraction),
    ];
    let fraction_total: f64 = cruise_shares
        .iter()
        .filter(|(tag, _)| schedule.iter().any(|segment| segment.tag == *tag))
        .map(|(_, fraction)| *fraction)
        .sum();
    if !fraction_total.is_finite() {
        return Err("mission cruise distance fractions sum to a non-finite value".to_owned());
    }
    if cruise_remainder_m > 1.0e-9 && fraction_total <= 0.0 {
        return Err(
            "mission cruise distance fractions must include an active leg when route distance remains after profile legs"
                .to_owned(),
        );
    }
    if fraction_total > 0.0 {
        for (tag, fraction) in cruise_shares {
            if let Some(segment) = schedule.iter_mut().find(|segment| segment.tag == tag) {
                if let SegmentKind::Cruise { distance_m, .. } = &mut segment.kind {
                    *distance_m = cruise_remainder_m * fraction / fraction_total;
                }
            }
        }
    }

    let flown_distance_m = schedule_horizontal_distance(&schedule, request.departure_elevation_m);
    if !flown_distance_m.is_finite()
        || (flown_distance_m - request.route_distance_m).abs() > DISTANCE_TOLERANCE_M
    {
        return Err(format!(
            "mission schedule does not close route distance: requested {:.6} m, scheduled {:.6} m",
            request.route_distance_m, flown_distance_m
        ));
    }

    Ok(schedule)
}

/// Validate the aircraft-independent kinematic contract before any
/// pseudospectral segment is constructed. These are profile inputs, not
/// hidden defaults: every aircraft supplies its own speeds, rates and cruise
/// shares, while the mission solver remains agnostic to aircraft type.
fn validate_profile_inputs(request: &MissionRequest) -> Result<(), String> {
    for (name, value) in [
        ("cruise altitude", request.cruise_altitude_m),
        ("departure elevation", request.departure_elevation_m),
        ("arrival elevation", request.arrival_elevation_m),
        ("departure ISA deviation", request.departure_isa_deviation_c),
    ] {
        if !value.is_finite() {
            return Err(format!("mission {name} must be finite, got {value}"));
        }
    }
    if request.cruise_altitude_m <= 0.0 {
        return Err("mission cruise altitude must be positive".to_owned());
    }

    let p = &request.profile;
    if !p.takeoff_altitude_gain_m.is_finite() || p.takeoff_altitude_gain_m < 0.0 {
        return Err("mission takeoff altitude gain must be finite and non-negative".to_owned());
    }
    let vertical_legs = [
        ("takeoff", p.takeoff_air_speed_m_s, p.takeoff_climb_rate_m_s),
        (
            "initial climb",
            p.initial_climb_air_speed_m_s,
            p.initial_climb_rate_m_s,
        ),
        (
            "step climb 1",
            p.step_climb_1_air_speed_m_s,
            p.step_climb_1_rate_m_s,
        ),
        (
            "step climb 2",
            p.step_climb_2_air_speed_m_s,
            p.step_climb_2_rate_m_s,
        ),
        ("descent 1", p.descent_1_air_speed_m_s, p.descent_1_rate_m_s),
        ("descent 2", p.descent_2_air_speed_m_s, p.descent_2_rate_m_s),
        ("descent 3", p.descent_3_air_speed_m_s, p.descent_3_rate_m_s),
        ("descent 4", p.descent_4_air_speed_m_s, p.descent_4_rate_m_s),
        (
            "final landing",
            p.landing_air_speed_m_s,
            p.landing_descent_rate_m_s,
        ),
    ];
    for (name, speed, rate) in vertical_legs {
        validate_speed_rate(name, speed, rate)?;
    }
    if p.climb_descent_speed_reference == SpeedReference::CalibratedAirspeed {
        // At constant CAS the true airspeed rises with altitude, so the
        // highest altitude a leg can reach bounds its Mach validity and the
        // lowest bounds its margin over the vertical rate. Checking both ends
        // here turns an unresolvable schedule into a named-leg error before
        // the altitude fit sees a non-finite footprint.
        let lowest_m = request
            .departure_elevation_m
            .min(request.arrival_elevation_m);
        let highest_m = request.cruise_altitude_m.max(lowest_m);
        for (name, cas_m_s, rate) in vertical_legs {
            for altitude_m in [lowest_m, highest_m] {
                let atmosphere =
                    us1976_try_compute_values(altitude_m, request.departure_isa_deviation_c)
                        .map_err(|error| {
                            format!("mission atmosphere at {altitude_m} m is unusable: {error}")
                        })?;
                let tas = resolve_true_airspeed_m_s(
                    SpeedReference::CalibratedAirspeed,
                    cas_m_s,
                    atmosphere.pressure_pa,
                    atmosphere.temperature_k,
                )
                .map_err(|error| {
                    format!(
                        "mission {name} calibrated airspeed {cas_m_s} m/s cannot be flown at {altitude_m:.0} m: {error}"
                    )
                })?;
                if rate >= tas {
                    return Err(format!(
                        "mission {name} vertical rate {rate} m/s must be below the {tas:.3} m/s true airspeed that {cas_m_s} m/s calibrated resolves to at {altitude_m:.0} m"
                    ));
                }
            }
        }
    }
    for (name, speed) in [
        ("cruise 1", p.cruise_1_air_speed_m_s),
        ("cruise 2", p.cruise_2_air_speed_m_s),
        ("cruise 3", p.cruise_3_air_speed_m_s),
    ] {
        if !speed.is_finite() || speed <= 0.0 {
            return Err(format!(
                "mission {name} airspeed must be finite and positive, got {speed}"
            ));
        }
    }
    for (name, fraction) in [
        (
            "initial climb altitude fraction",
            p.initial_climb_altitude_fraction,
        ),
        (
            "step climb 1 altitude fraction",
            p.step_climb_1_altitude_fraction,
        ),
    ] {
        if !fraction.is_finite() || fraction < 0.0 {
            return Err(format!(
                "mission {name} must be finite and non-negative, got {fraction}"
            ));
        }
    }
    for (name, altitude_ft) in [
        ("descent 1 altitude", p.descent_1_altitude_ft),
        ("descent 2 altitude", p.descent_2_altitude_ft),
        ("descent 3 altitude", p.descent_3_altitude_ft),
        ("descent 4 altitude", p.descent_4_altitude_ft),
    ] {
        if !altitude_ft.is_finite() || altitude_ft < 0.0 {
            return Err(format!(
                "mission {name} must be finite and non-negative, got {altitude_ft}"
            ));
        }
    }
    Ok(())
}

fn validate_speed_rate(name: &str, speed_m_s: f64, rate_m_s: f64) -> Result<(), String> {
    if !speed_m_s.is_finite() || speed_m_s <= 0.0 {
        return Err(format!(
            "mission {name} airspeed must be finite and positive, got {speed_m_s}"
        ));
    }
    if !rate_m_s.is_finite() || rate_m_s <= 0.0 {
        return Err(format!(
            "mission {name} vertical rate must be finite and positive, got {rate_m_s}"
        ));
    }
    if rate_m_s >= speed_m_s {
        return Err(format!(
            "mission {name} vertical rate {rate_m_s} m/s must be below airspeed {speed_m_s} m/s"
        ));
    }
    Ok(())
}

fn climb(
    tag: &str,
    start: Option<f64>,
    end: f64,
    speed: f64,
    speed_reference: SpeedReference,
    rate: f64,
    temperature_deviation_k: f64,
) -> SegmentSpec {
    SegmentSpec {
        tag: tag.to_owned(),
        kind: SegmentKind::Climb {
            altitude_start_m: start,
            altitude_end_m: end,
            climb_rate_m_s: rate,
        },
        air_speed_m_s: speed,
        air_speed_reference: speed_reference,
        true_course_rad: 0.0,
        temperature_deviation_k,
        number_control_points: CONTROL_POINTS,
    }
}

fn cruise(
    tag: &str,
    altitude: Option<f64>,
    distance: f64,
    speed: f64,
    temperature_deviation_k: f64,
) -> SegmentSpec {
    SegmentSpec {
        tag: tag.to_owned(),
        kind: SegmentKind::Cruise {
            altitude_m: altitude,
            distance_m: distance,
        },
        air_speed_m_s: speed,
        // A cruise leg is always a literal true airspeed (or a Mach-derived
        // one for a preset's operational route): never calibrated airspeed,
        // regardless of the profile's climb/descent speed reference.
        air_speed_reference: SpeedReference::TrueAirspeed,
        true_course_rad: 0.0,
        temperature_deviation_k,
        number_control_points: CONTROL_POINTS,
    }
}

fn descent(
    tag: &str,
    start: Option<f64>,
    end: f64,
    speed: f64,
    speed_reference: SpeedReference,
    rate: f64,
    temperature_deviation_k: f64,
) -> SegmentSpec {
    SegmentSpec {
        tag: tag.to_owned(),
        kind: SegmentKind::Descent {
            altitude_start_m: start,
            altitude_end_m: end,
            descent_rate_m_s: rate,
        },
        air_speed_m_s: speed,
        air_speed_reference: speed_reference,
        true_course_rad: 0.0,
        temperature_deviation_k,
        number_control_points: CONTROL_POINTS,
    }
}

// Tests assert on schedules they just built, so a failed unwrap or expect is
// the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::AlasConfig;
    use alas_mission::segments::Segment;
    use alas_opt::mdo::mission_model::PhaseAeroLimits;
    use alas_opt::mdo::propulsion::{max_climb_rate_ft_min, PropulsionDeck};
    use alas_opt::SegmentMissionModel;

    /// International knot, m/s.
    const KNOT: f64 = 1852.0 / 3600.0;
    /// Madrid-Barajas and Palma de Mallorca field elevations, m MSL.
    const LEMD_M: f64 = 610.0;
    const LEPA_M: f64 = 8.0;
    const FL170_M: f64 = 17_000.0 * METRES_PER_FOOT;

    /// The registered ATR 72-600 schedule (170 KCAS climb) between the
    /// preset's own airports, at an explicit speed reference.
    fn atr_request(reference: SpeedReference, route_distance_m: f64) -> MissionRequest {
        let mut profile = alas_config::presets::get("ATR72-600")
            .unwrap()
            .operational_mission_defaults()
            .profile;
        profile.climb_descent_speed_reference = reference;
        MissionRequest {
            mission_tag: "LEMD_to_LEPA".to_owned(),
            cruise_altitude_m: FL170_M,
            departure_elevation_m: LEMD_M,
            arrival_elevation_m: LEPA_M,
            departure_isa_deviation_c: 0.0,
            route_distance_m,
            profile,
        }
    }

    /// Lay every segment of `schedule` down as the native mission would and
    /// return the horizontal distance its own time-integration operator
    /// assigns to each, chained through the deferred start altitudes.
    fn natively_flown_distances_m(schedule: &[SegmentSpec]) -> Vec<f64> {
        let mut initials = None;
        let mut distances = Vec::with_capacity(schedule.len());
        for spec in schedule {
            let mut segment = Segment::new(spec.clone(), initials).expect("a valid segment");
            segment
                .numerics
                .update_differentials_time(&segment.conditions.time_s);
            let last_row = segment.numerics.time.integrate.last().unwrap();
            let distance_m: f64 = last_row
                .iter()
                .zip(&segment.conditions.velocity_vector_m_s)
                .map(|(&weight, v)| weight * v[0])
                .sum();
            distances.push(distance_m);
            initials = Some(segment.initials_for_next());
        }
        distances
    }

    // The schedule hands the native segments the configured *calibrated*
    // number and the reference that says so; it does not pre-convert to a
    // true airspeed (which the segment would then convert again) and it
    // never marks a cruise leg calibrated.
    #[test]
    fn the_schedule_passes_the_calibrated_reference_and_the_unconverted_speed_to_every_vertical_leg(
    ) {
        let request = atr_request(SpeedReference::CalibratedAirspeed, 370_000.0);
        let schedule = build_schedule(&request).expect("the ATR schedule builds");
        let climb_cas = request.profile.initial_climb_air_speed_m_s;
        assert!((climb_cas - 170.0 * KNOT).abs() < 1.0e-9);
        let mut vertical = 0;
        for spec in &schedule {
            match spec.kind {
                SegmentKind::Climb { .. } | SegmentKind::Descent { .. } => {
                    vertical += 1;
                    assert_eq!(
                        spec.air_speed_reference,
                        SpeedReference::CalibratedAirspeed,
                        "{}",
                        spec.tag
                    );
                    assert_eq!(spec.temperature_deviation_k, 0.0);
                    if spec.tag == "initial_climb" {
                        assert_eq!(
                            spec.air_speed_m_s, climb_cas,
                            "initial climb speed was converted"
                        );
                    }
                }
                SegmentKind::Cruise { .. } => {
                    assert_eq!(
                        spec.air_speed_reference,
                        SpeedReference::TrueAirspeed,
                        "{}",
                        spec.tag
                    );
                }
            }
        }
        assert!(vertical >= 8, "{vertical} vertical legs");
        // The takeoff leg starts at the field elevation, MSL, and its top is
        // the configured gain above that field, not above sea level.
        let takeoff = &schedule[0];
        assert_eq!(takeoff.tag, "takeoff");
        assert!(matches!(
            takeoff.kind,
            SegmentKind::Climb { altitude_start_m: Some(start), altitude_end_m, .. }
                if start == LEMD_M
                    && (altitude_end_m - (LEMD_M + request.profile.takeoff_altitude_gain_m)).abs()
                        < 1.0e-9
        ));
    }

    // The closed route distance is the distance the native segments then fly.
    // In calibrated mode the footprint is a quadrature over a varying true
    // airspeed; using the segments' own operator makes the two agree to
    // rounding, not to a discretization tolerance. The legacy true-airspeed
    // mode closes identically.
    #[test]
    fn a_calibrated_schedule_closes_on_the_distance_the_native_segments_fly() {
        for reference in [
            SpeedReference::CalibratedAirspeed,
            SpeedReference::TrueAirspeed,
        ] {
            let route_m = 370_000.0;
            let request = atr_request(reference, route_m);
            let schedule = build_schedule(&request).expect("the schedule builds");
            let flown: f64 = natively_flown_distances_m(&schedule).iter().sum();
            assert!(
                (flown - route_m).abs() < 1.0e-3,
                "{reference:?}: segments fly {flown} m of a {route_m} m route"
            );
            let closure_m = schedule_horizontal_distance(&schedule, LEMD_M);
            assert!((closure_m - route_m).abs() < DISTANCE_TOLERANCE_M);
        }
        // And the calibrated footprint really is a different number from the
        // CAS-as-TAS one: the climb legs cover more ground at the same rate.
        let cas =
            build_schedule(&atr_request(SpeedReference::CalibratedAirspeed, 370_000.0)).unwrap();
        let tas = build_schedule(&atr_request(SpeedReference::TrueAirspeed, 370_000.0)).unwrap();
        let cas_climb_m = profile_horizontal_distance_of_schedule(&cas, LEMD_M);
        let tas_climb_m = profile_horizontal_distance_of_schedule(&tas, LEMD_M);
        assert!(
            cas_climb_m > 1.02 * tas_climb_m,
            "calibrated footprint {cas_climb_m} m vs CAS-as-TAS {tas_climb_m} m"
        );
    }

    // The MDO segment model and the native schedule discretize the same
    // constant-CAS ladders differently (8 midpoint sub-rungs per rung against
    // 16 Chebyshev nodes per leg). At matching elevations, cruise altitude,
    // ISA deviation and profile their planned climb+descent footprints must
    // agree within the MDO discretization error, and to quadrature precision
    // once the MDO side is refined: the check that both paths resolve the
    // same CAS against the same ambient state rather than two conventions
    // that happen to be close. The acceptance thresholds below are chosen
    // numerical criteria; the measured gaps are in the failure messages and
    // recorded in an internal ATR physics probe run.
    #[test]
    fn native_and_mdo_calibrated_footprints_agree_at_matching_conditions() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"})).unwrap();
        for isa_deviation_c in [0.0, 15.0] {
            let mut request = atr_request(SpeedReference::CalibratedAirspeed, 1.0e6);
            request.departure_isa_deviation_c = isa_deviation_c;
            assert_eq!(
                config.mission.profile, request.profile,
                "the preset profile is the calibrated one"
            );
            let schedule = build_schedule(&request).unwrap();
            let native_m = profile_horizontal_distance_of_schedule(&schedule, LEMD_M);

            let deck = PropulsionDeck::from_engine(
                &config.geometry.engine,
                config.requirements.cruise_mach,
                FL170_M,
                max_climb_rate_ft_min(request.profile.initial_climb_rate_m_s),
            )
            .unwrap();
            let model = SegmentMissionModel::new(
                request.profile.clone(),
                config.requirements.cruise_mach,
                FL170_M,
                LEMD_M,
                LEPA_M,
                61.0,
                0.03,
                0.045,
                0.0,
                config.requirements.gravity_m_s2,
                LEPA_M + 457.2,
                PhaseAeroLimits::from_config(&config),
                deck,
            )
            .unwrap()
            .with_isa_deviation_c(isa_deviation_c);
            let production = model.plan_trip(1.0e6).unwrap();
            assert!(!production.adapted);
            let production_m = production.climb_footprint_m + production.descent_footprint_m;
            let refined = model
                .clone()
                .with_cas_subdivisions(4096)
                .plan_trip(1.0e6)
                .unwrap();
            let refined_m = refined.climb_footprint_m + refined.descent_footprint_m;
            let production_error = (production_m - native_m).abs() / native_m;
            let refined_error = (refined_m - native_m).abs() / native_m;
            assert!(
                production_error < 5.0e-4,
                "ISA{isa_deviation_c:+}: 8 sub-rungs: MDO {production_m} m vs native {native_m} m ({production_error:e})"
            );
            assert!(
                refined_error < 1.0e-6,
                "ISA{isa_deviation_c:+}: 4096 sub-rungs: MDO {refined_m} m vs native {native_m} m ({refined_error:e})"
            );
            assert!(refined_error < production_error);
        }
    }

    #[test]
    fn native_and_mdo_literal_tas_footprints_match_on_an_adapted_route() {
        let route_m = 120_000.0;
        let request = atr_request(SpeedReference::TrueAirspeed, route_m);
        let schedule = build_schedule(&request).expect("the adapted TAS schedule builds");
        let native_m = profile_horizontal_distance_of_schedule(&schedule, LEMD_M);

        let config = AlasConfig::from_value(&serde_json::json!({"preset": "ATR72-600"})).unwrap();
        let deck = PropulsionDeck::from_engine(
            &config.geometry.engine,
            config.requirements.cruise_mach,
            FL170_M,
            max_climb_rate_ft_min(request.profile.initial_climb_rate_m_s),
        )
        .unwrap();
        let model = SegmentMissionModel::new(
            request.profile.clone(),
            config.requirements.cruise_mach,
            FL170_M,
            LEMD_M,
            LEPA_M,
            61.0,
            0.03,
            0.045,
            0.0,
            config.requirements.gravity_m_s2,
            LEPA_M + 457.2,
            PhaseAeroLimits::from_config(&config),
            deck,
        )
        .unwrap()
        .with_isa_deviation_c(request.departure_isa_deviation_c);
        let production = model.plan_trip(route_m).unwrap();
        assert!(
            production.adapted,
            "the route must exercise altitude fitting"
        );
        let production_m = production.climb_footprint_m + production.descent_footprint_m;
        let relative_error = (production_m - native_m).abs() / native_m;
        assert!(
            relative_error < 1.0e-3,
            "literal TAS adaptation changed the vertical footprint: MDO {production_m} m vs native {native_m} m ({relative_error:e})"
        );
    }

    // A calibrated schedule the atmosphere cannot fly is a named-leg error
    // from validation, before any footprint is computed, including the
    // case the plain `rate < speed` check on the CAS number cannot see: on a
    // cold day at a low field the true airspeed is *below* the calibrated
    // one, so a rate just under the CAS value exceeds the resolved TAS.
    #[test]
    fn an_unresolvable_calibrated_leg_is_a_named_error() {
        let mut request = atr_request(SpeedReference::CalibratedAirspeed, 1.0e6);
        request.cruise_altitude_m = 12_000.0;
        request.profile.step_climb_2_air_speed_m_s = 330.0;
        let error = build_schedule(&request).expect_err("330 m/s CAS at 12 km is supersonic");
        assert!(
            error.contains("step climb 2") && error.contains("calibrated airspeed"),
            "{error}"
        );
        let mut steep = atr_request(SpeedReference::CalibratedAirspeed, 1.0e6);
        steep.departure_isa_deviation_c = -30.0;
        steep.profile.landing_descent_rate_m_s = steep.profile.landing_air_speed_m_s - 0.01;
        let error = build_schedule(&steep)
            .expect_err("a rate just under the CAS value exceeds the cold-day TAS");
        assert!(
            error.contains("final landing") && error.contains("true airspeed"),
            "{error}"
        );
    }
}
