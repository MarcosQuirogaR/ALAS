// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission profile construction and route-distance closure.

use alas_atmo::us1976_try_compute_values;
use alas_config::mission::SpeedReference;
use alas_mission::segments::{SegmentKind, SegmentSpec};
use alas_mission::MissionRequest;

mod closure;
pub(super) mod configuration;
pub(super) mod junction;
mod reach;
mod specs;
pub(super) mod speed;
pub(super) mod speed_change;
pub(crate) mod takeoff;
use closure::footprint_of_schedule;
#[cfg(test)]
use closure::profile_horizontal_distance_of_schedule;
pub(super) use closure::{
    close_schedule_distance, fit_altitude_profile, schedule_horizontal_distance,
};
use reach::cruise_level;
pub(super) use reach::Reach;
pub(super) use specs::{climb, cruise, descent};
use speed::validate_speed_rate;
use speed_change::SpeedMemory;

const CONTROL_POINTS: usize = 16;
const METRES_PER_FOOT: f64 = 0.3048;
const DISTANCE_TOLERANCE_M: f64 = 1.0e-6;
const ALTITUDE_TOLERANCE_M: f64 = 1.0e-9;
const PROFILE_SCALE_ITERATIONS: usize = 64;

#[cfg(test)]
pub(super) fn build_schedule(request: &MissionRequest) -> Result<Vec<SegmentSpec>, String> {
    build_schedule_with_memory(request, None, None, None).map(|(schedule, _)| schedule)
}

/// [`build_schedule`], with what the planner remembered about the speed
/// changes between legs (kept through every later re-closing of the route).
///
/// The climb is continuous: the three rungs of the profile (250 kt below 10,000 ft,
/// the type's CAS, its Mach) are the altitudes at which the climb schedule changes,
/// and the aircraft climbs through them to the first cruise level. The active cruise
/// legs are a step ladder one level interval (2,000 ft) apart, the last at the top; a
/// lower leg is flown only where the route lasts long enough to pay for its step
/// (`reach::unjustified_step_legs`, the first-order altitude-drift rule the route
/// proposal uses), so a short sector flies one level. `cruise_rule` only
/// resolves the declared cruise speed at the level each leg ends up at.
pub(super) fn build_schedule_with_memory(
    request: &MissionRequest,
    cruise_rule: Option<speed_change::CruiseSpeedRule>,
    takeoff: Option<&takeoff::TakeoffAcceleration>,
    reach: Option<&Reach>,
) -> Result<(Vec<SegmentSpec>, SpeedMemory), String> {
    let mut memory = SpeedMemory::for_profile(&request.profile, cruise_rule);
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
    // The top level is the configured one unless the aircraft cannot hold it at this mass: the
    // whole ladder is then fitted under the level it can (the leg count is kept).
    let cruise_altitude = reach.map_or(request.cruise_altitude_m, |reach| {
        request.cruise_altitude_m.min(reach.top_m)
    });
    let (first_level, second_level) = reach::levels(request, cruise_altitude);
    let temperature_deviation_k = request.departure_isa_deviation_c;
    // Declared cruise speeds are standard-day `M a_ISA`; the same Mach is
    // `M a(T)` on the flown day (MIS-04).
    let cruise_speed_ratio =
        speed::standard_day_speed_ratio(cruise_altitude, temperature_deviation_k)?;
    let reference = p.climb_descent_speed_reference;
    let mut schedule = Vec::with_capacity(12);

    // The take-off band is flown at its rating too (the take-off rating where `takeoff` makes it
    // take-off-rated): the climb-out holds the rating and the speed, not a vertical rate.
    schedule.push(rated(climb(
        "takeoff",
        Some(request.departure_elevation_m),
        request.departure_elevation_m + p.takeoff_altitude_gain_m,
        p.takeoff_air_speed_m_s,
        reference,
        p.takeoff_climb_rate_m_s,
        temperature_deviation_k,
    )));
    let active_cruise = [
        p.cruise_1_distance_fraction > 0.0,
        p.cruise_2_distance_fraction > 0.0,
        p.cruise_3_distance_fraction > 0.0,
    ];
    // The climb schedule's rungs (250 kt below 10,000 ft, then the type's CAS, then its Mach) are
    // climbed continuously to the first cruise level. The active cruise legs are a step ladder at
    // the top: one level interval apart (2,000 ft, the same-direction spacing of the RVSM
    // cruising levels, `alas_mission::CRUISE_LEVEL_INTERVAL_M`), the last at the top. A lower
    // leg is kept only where the route justifies its step (`reach::unjustified_step_legs`).
    let mut active_legs: Vec<usize> = (0..3).filter(|&leg| active_cruise[leg]).collect();
    let ladder_floor_m = request.departure_elevation_m + p.takeoff_altitude_gain_m;
    let ladder_step_for = |steps: usize| {
        ((cruise_altitude - ladder_floor_m).max(0.0) / steps.max(1) as f64)
            .min(alas_mission::CRUISE_LEVEL_INTERVAL_M)
    };
    // A ladder too low for real steps (under the 30 m rung tolerance each) is one level.
    if active_legs.len() > 1 && ladder_step_for(active_legs.len() - 1) < 30.0 {
        active_legs.drain(..active_legs.len() - 1);
    }
    let steps = active_legs.len().saturating_sub(1);
    let ladder_step_m = if steps == 0 {
        0.0
    } else {
        ladder_step_for(steps)
    };
    let ladder_base_m = cruise_altitude - steps as f64 * ladder_step_m;
    let first_leg = active_legs.first().copied().unwrap_or(0);
    let mut first_only = [false; 3];
    first_only[first_leg] = !active_legs.is_empty();
    let flown = reach::flown_cruise_legs(
        first_only,
        first_level.min(ladder_base_m),
        second_level.min(ladder_base_m),
        ladder_base_m,
    );
    let rungs = [
        (
            "initial_climb",
            p.initial_climb_air_speed_m_s,
            p.initial_climb_rate_m_s,
        ),
        (
            "step_climb_1",
            p.step_climb_1_air_speed_m_s,
            p.step_climb_1_rate_m_s,
        ),
        (
            "step_climb_2",
            p.step_climb_2_air_speed_m_s,
            p.step_climb_2_rate_m_s,
        ),
    ];
    let cruise_legs = [
        ("cruise_step_1", p.cruise_1_air_speed_m_s),
        ("cruise_step_2", p.cruise_2_air_speed_m_s),
        ("cruise_step_3", p.cruise_3_air_speed_m_s),
    ];
    for (index, (rung_end_m, &(tag, speed, rate))) in
        flown.rung_ends_m.iter().zip(rungs.iter()).enumerate()
    {
        if let Some(end_m) = *rung_end_m {
            schedule.push(rated(climb(
                tag,
                None,
                end_m,
                speed,
                reference,
                rate,
                temperature_deviation_k,
            )));
        }
        for (leg, (tag, speed)) in cruise_legs.into_iter().enumerate() {
            if flown.cruise_after[leg] == Some(index) {
                schedule.push(cruise(
                    tag,
                    None,
                    0.0,
                    speed * cruise_speed_ratio,
                    temperature_deviation_k,
                ));
            }
        }
    }
    // The steps of the ladder, at the rate of the last climb rung and at the cruise speed of the
    // level they climb to: a step climb is flown at the cruise Mach (Airbus, "Getting to Grips
    // with Aircraft Performance", 2002, chapter "Cruise", step climb), so it neither slows to a
    // climb CAS that is far below the cruise speed at a low first level (idle, then the rating)
    // nor accelerates back after it. The step flies the speed of the cruise before it: the same
    // declared speed, resolved by the priced route's cruise-speed rule at the level it starts
    // from and held to the speed the priced route flies there, so it leaves that cruise without
    // a speed change. It is held to VMO/MMO, not to the climb schedule's Mach (an A340-300 step
    // had slowed at idle from its M 0.82 cruise to the M 0.80 climb Mach first).
    let mut ladder_steps = Vec::new();
    for (step, &leg) in active_legs.iter().enumerate().skip(1) {
        let step_tag = format!("step_climb_{}", step + 2);
        ladder_steps.push(step_tag.clone());
        let (cruise_tag, speed) = cruise_legs[leg];
        let below_m_s = cruise_legs[active_legs[step - 1]].1;
        memory.declare_cruise(&step_tag, below_m_s);
        let from_m = ladder_base_m + (step - 1) as f64 * ladder_step_m;
        if let Some(cap) = reach.and_then(|reach| reach.speed_near(from_m)) {
            memory.cap_cruise_speed(&step_tag, cap);
        }
        schedule.push(rated(climb(
            &step_tag,
            None,
            ladder_base_m + step as f64 * ladder_step_m,
            below_m_s * cruise_speed_ratio,
            SpeedReference::TrueAirspeed,
            p.step_climb_2_rate_m_s,
            temperature_deviation_k,
        )));
        schedule.push(cruise(
            cruise_tag,
            None,
            0.0,
            speed * cruise_speed_ratio,
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
    // A step whose altitude is not above the arrival field is not flown. The landing
    // configuration is flown only below its BADA height above the field: when the last step ends
    // higher (a high field dropped the low steps), a descent at the approach rung's speed and rate
    // closes the height down to it first.
    let arrival_ft = request.arrival_elevation_m / METRES_PER_FOOT;
    let mut last_end_m = f64::INFINITY;
    for (index, (altitude_ft, speed, rate)) in descent_steps.into_iter().enumerate() {
        if altitude_ft > arrival_ft {
            last_end_m = altitude_ft * METRES_PER_FOOT;
            schedule.push(descent(
                &format!("descent_{}", index + 1),
                None,
                last_end_m,
                speed,
                reference,
                rate,
                temperature_deviation_k,
            ));
        }
    }
    let landing_top_m =
        request.arrival_elevation_m + alas_config::mission::LANDING_CONFIGURATION_HEIGHT_M;
    if last_end_m.is_finite() && last_end_m > landing_top_m + 1.0 {
        schedule.push(descent(
            "descent_approach",
            None,
            landing_top_m,
            p.descent_4_air_speed_m_s,
            reference,
            p.descent_4_rate_m_s,
            temperature_deviation_k,
        ));
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

    // A cruise leg flies at most the speed the priced route holds at its level.
    if let Some(reach) = reach {
        for spec in &schedule {
            if let (true, Some(level_m)) = (
                spec.tag.starts_with("cruise_step"),
                cruise_level(spec, &schedule),
            ) {
                if let Some(speed) = reach.speed_near(level_m) {
                    memory.cap_cruise_speed(&spec.tag, speed);
                }
            }
        }
    }
    speed::apply_speed_caps(&mut schedule, p);
    for spec in schedule
        .iter_mut()
        .filter(|spec| ladder_steps.contains(&spec.tag))
    {
        spec.speed_caps = p.certified_caps();
    }
    if let Some(acceleration) = takeoff {
        let to_m_s = speed_change::leg_speed_m_s(&schedule[0], request.departure_elevation_m);
        if let Some(to_m_s) =
            to_m_s.filter(|to| *to > acceleration.v2_m_s + speed_change::MINIMUM_SPEED_CHANGE_M_S)
        {
            takeoff::split_takeoff_climb(
                &mut schedule,
                acceleration,
                request.departure_elevation_m,
                to_m_s,
            );
        }
    }

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
        &memory,
    )?;
    let non_cruise_distance_m = footprint_of_schedule(&schedule, request.departure_elevation_m);
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
    let cruise_shares: Vec<(String, f64)> = vec![
        ("cruise_step_1".to_owned(), p.cruise_1_distance_fraction),
        ("cruise_step_2".to_owned(), p.cruise_2_distance_fraction),
        ("cruise_step_3".to_owned(), p.cruise_3_distance_fraction),
    ];
    // A step cruise the route is too short to justify is not flown: the climb carries on through
    // its level (the cruise distance goes to the legs that remain).
    let ladder: Vec<reach::LadderLeg> = cruise_shares
        .iter()
        .filter_map(|(tag, fraction)| {
            let spec = schedule.iter().find(|spec| spec.tag == *tag)?;
            Some((
                tag.clone(),
                cruise_level(spec, &schedule)?,
                *fraction,
                spec.air_speed_m_s,
            ))
        })
        .collect();
    let dropped = reach::unjustified_step_legs(&ladder, cruise_remainder_m);
    schedule.retain(|spec| !dropped.contains(&spec.tag));
    // A step whose lower cruise is not flown continues the climb: it climbs on the schedule of
    // the climb before it (its CAS/Mach and climb Mach), not at the cruise speed of a level it
    // does not stop at (an A320-200 had decelerated at idle at the rung's end to that speed).
    // The junction pieces of the fitted profile blend to the speeds they were cut for, so the
    // climb is read from the leg itself (not its pieces) and the pieces are cut again.
    let mut previous_leg: Option<usize> = None;
    let mut continued = false;
    for index in 0..schedule.len() {
        if junction::family(&schedule[index].tag) != schedule[index].tag {
            continue;
        }
        if let Some(previous) = previous_leg.filter(|_| ladder_steps.contains(&schedule[index].tag))
        {
            if matches!(schedule[previous].kind, SegmentKind::Climb { .. }) {
                let climb = schedule[previous].clone();
                let step = &mut schedule[index];
                step.air_speed_m_s = climb.air_speed_m_s;
                step.air_speed_reference = climb.air_speed_reference;
                step.speed_caps = climb.speed_caps;
                memory.forget_cruise(&step.tag);
                continued = true;
            }
        }
        previous_leg = Some(index);
    }
    if continued {
        schedule = fit_altitude_profile(
            &schedule,
            request.departure_elevation_m,
            request.arrival_elevation_m,
            request.route_distance_m,
            &memory,
        )?;
    }
    // Those steps now fly the climb speed, so the horizontal footprint is taken again for the
    // cruise distance that closes the route.
    let non_cruise_distance_m = footprint_of_schedule(&schedule, request.departure_elevation_m);
    if !non_cruise_distance_m.is_finite()
        || request.route_distance_m + DISTANCE_TOLERANCE_M < non_cruise_distance_m
    {
        return Err(format!(
            "mission route distance {:.3} m cannot accommodate the {:.3} m horizontal footprint required to connect the airport elevations",
            request.route_distance_m, non_cruise_distance_m
        ));
    }
    let cruise_remainder_m = (request.route_distance_m - non_cruise_distance_m).max(0.0);
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

    Ok((schedule, memory))
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
    // The landing configuration is extended below VFE: the approach rung and
    // the landing must not be faster (a true airspeed at or below a
    // calibrated one is at or below it in calibrated terms above sea level).
    let vfe = p.vfe_landing_cas_m_s;
    if vfe.is_finite() && vfe > 0.0 {
        for (name, speed) in [
            ("descent 4", p.descent_4_air_speed_m_s),
            ("final landing", p.landing_air_speed_m_s),
        ] {
            if speed > vfe * (1.0 + 1.0e-9) {
                return Err(format!(
                    "mission {name} airspeed {speed} m/s is above the landing-flap VFE {vfe} m/s"
                ));
            }
        }
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
                let caps = if name.starts_with("descent") || name == "final landing" {
                    p.descent_caps()
                } else {
                    p.climb_caps()
                };
                let tas = caps
                    .true_airspeed_m_s(
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
        // MMO at the cruise level's pressure altitude.
        let mmo = alas_atmo::us1976_try_compute_values(
            request.cruise_altitude_m,
            request.departure_isa_deviation_c,
        )
        .map_or(p.max_operating_mach, |ambient| {
            p.certified_caps().max_mach_at(ambient.pressure_pa)
        });
        if mmo > 0.0 {
            let mach = speed::cruise_mach(
                speed,
                request.cruise_altitude_m,
                request.departure_isa_deviation_c,
            )?;
            if mach > mmo * (1.0 + 1.0e-9) {
                return Err(format!(
                    "mission {name} airspeed is Mach {mach:.4} at the cruise altitude, above MMO {mmo}"
                ));
            }
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

/// An en-route climb flown at the maximum-climb rating on its speed schedule.
///
/// A transport climbs with the thrust levers in the climb detent and the autothrust holding the
/// maximum-climb rating, the speed held on the elevator; the vertical speed is whatever the excess
/// power gives (Airbus, "Getting to Grips with Aircraft Performance", 2002, chapter "Climb", the
/// climb at constant CAS/Mach with maximum-climb thrust; BADA 3.8 User Manual, Sec. 3.2 and Sec.
/// 3.7.1, the total-energy model with maximum-climb thrust in climb). The schedule's rate is then
/// only the solver's starting guess and the mean rate the leg is planned with, so the throttle
/// command is the same rating through the whole climb instead of a share that changes at every
/// rung.
fn rated(mut spec: SegmentSpec) -> SegmentSpec {
    spec.rating_limited = true;
    spec
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

    /// The legs on their own speed schedules, without the speed blended in from
    /// the previous leg: the reduced model compares against these (it changes
    /// speed at a boundary, not along a leg).
    fn own_schedules(schedule: &[SegmentSpec]) -> Vec<SegmentSpec> {
        junction::merge_pieces(schedule.to_vec())
            .into_iter()
            .map(|mut spec| {
                spec.entry_speed_m_s = 0.0;
                spec.exit_speed_m_s = 0.0;
                spec
            })
            .collect()
    }

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
        // `apply_atr72_600_speed_schedule` (`alas-config` speed schedules)
        // configures a single climb + single cruise leg, so the minimum
        // vertical-leg count is takeoff, initial_climb, four descent rungs
        // and final_landing: 7, not the 9 a 3-leg cruise-climb schedule
        // would produce.
        assert!(vertical >= 7, "{vertical} vertical legs");
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
    // rounding, not to a discretization tolerance. The plain true-airspeed
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
            let native_m =
                profile_horizontal_distance_of_schedule(&own_schedules(&schedule), LEMD_M);

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
                std::sync::Arc::new(alas_opt::mdo::mission_model::ParabolicPolar::new(
                    0.03,
                    0.045,
                    0.0,
                    config.requirements.cruise_mach,
                )),
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
            // The ATR schedule sets `initial_climb_altitude_fraction` to
            // 0.999 and disables the second and third cruise legs (zero
            // distance share), so the native schedule stops climbing at that
            // fraction and cruises wherever it ends, never reaching the
            // literal declared cruise altitude. `ProfileGeometry` gates each
            // climb band on the same `cruise_2_distance_fraction`/
            // `cruise_3_distance_fraction` activity `build_schedule` uses
            // (see `climb_bands::bands`/`top_m`) and flies cruise/descent
            // from that same ladder top, matching the native schedule.
            assert!(
                production_error < 5.0e-4,
                "ISA{isa_deviation_c:+}: 8 sub-rungs: MDO {production_m} m vs native {native_m} m ({production_error:e})"
            );
            assert!(
                refined_error < 1.0e-6,
                "ISA{isa_deviation_c:+}: 4096 sub-rungs: MDO {refined_m} m vs native {native_m} m ({refined_error:e})"
            );
            assert!(
                refined_error < production_error,
                "ISA{isa_deviation_c:+}: refinement did not shrink the error: production {production_error:e}, refined {refined_error:e}"
            );
        }
    }

    // The CAS/Mach ladder of a widebody (B787-9: 163 m/s CAS, M 0.84 above
    // the crossover, MMO 0.90): both paths apply the one `SpeedCaps`
    // definition, so their climb and descent footprints agree to the
    // discretization of the kink at the crossover, on a standard and a hot
    // day. The gaps are recorded in the failure messages.
    #[test]
    fn native_and_mdo_cas_mach_footprints_agree_for_a_widebody_ladder() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "B787-9"})).unwrap();
        let operational = alas_config::presets::get("B787-9")
            .unwrap()
            .operational_mission_defaults();
        let cruise_m = operational.cruise_altitude_m;
        assert!(
            operational.profile.climb_mach > 0.0 && operational.profile.max_operating_mach > 0.0
        );
        for isa_deviation_c in [0.0, 15.0] {
            let request = MissionRequest {
                mission_tag: "RJTT_to_YSSY".to_owned(),
                cruise_altitude_m: cruise_m,
                departure_elevation_m: 0.0,
                arrival_elevation_m: 0.0,
                departure_isa_deviation_c: isa_deviation_c,
                route_distance_m: 7.8e6,
                profile: operational.profile.clone(),
            };
            let native_m = profile_horizontal_distance_of_schedule(
                &own_schedules(&build_schedule(&request).unwrap()),
                0.0,
            );
            let deck = PropulsionDeck::from_engine(
                &config.geometry.engine,
                config.requirements.cruise_mach,
                cruise_m,
                max_climb_rate_ft_min(request.profile.initial_climb_rate_m_s),
            )
            .unwrap();
            let model = SegmentMissionModel::new(
                request.profile.clone(),
                config.requirements.cruise_mach,
                cruise_m,
                0.0,
                0.0,
                377.0,
                std::sync::Arc::new(alas_opt::mdo::mission_model::ParabolicPolar::new(
                    0.02,
                    0.045,
                    0.0,
                    config.requirements.cruise_mach,
                )),
                config.requirements.gravity_m_s2,
                457.2,
                PhaseAeroLimits::from_config(&config),
                deck,
            )
            .unwrap()
            .with_isa_deviation_c(isa_deviation_c);
            let footprint = |model: &SegmentMissionModel| {
                let plan = model.plan_trip(7.8e6).unwrap();
                assert!(!plan.adapted);
                plan.climb_footprint_m + plan.descent_footprint_m
            };
            let production_m = footprint(&model);
            let refined_m = footprint(&model.clone().with_cas_subdivisions(4096));
            let production_error = (production_m - native_m).abs() / native_m;
            let refined_error = (refined_m - native_m).abs() / native_m;
            assert!(
                production_error < 5.0e-3,
                "ISA{isa_deviation_c:+}: 8 sub-rungs: MDO {production_m} m vs native {native_m} m ({production_error:e})"
            );
            assert!(
                refined_error < 5.0e-3,
                "ISA{isa_deviation_c:+}: 4096 sub-rungs: MDO {refined_m} m vs native {native_m} m ({refined_error:e})"
            );
        }
    }

    #[test]
    fn native_and_mdo_literal_tas_footprints_match_on_an_adapted_route() {
        let route_m = 120_000.0;
        let request = atr_request(SpeedReference::TrueAirspeed, route_m);
        let schedule = build_schedule(&request).expect("the adapted TAS schedule builds");
        let native_m = profile_horizontal_distance_of_schedule(&own_schedules(&schedule), LEMD_M);

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
            std::sync::Arc::new(alas_opt::mdo::mission_model::ParabolicPolar::new(
                0.03,
                0.045,
                0.0,
                config.requirements.cruise_mach,
            )),
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
        // Both fit the one share of the route (the priced model has no level speed
        // changes to fit; the native flight holds them in that share), so the whole
        // native footprint matches the priced one and its climbs and descents are
        // the part of it that is not a level change.
        let native_total_m = footprint_of_schedule(&schedule, LEMD_M);
        let relative_error = (production_m - native_total_m).abs() / native_total_m;
        assert!(
            relative_error < 1.0e-3 && native_m <= native_total_m,
            "literal TAS adaptation changed the footprint: MDO {production_m} m vs native {native_total_m} m ({relative_error:e})"
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

    /// A route too short for the full profile lowers it, but the descent rung that ends at the
    /// 10,000 ft speed restriction (14 CFR 91.117(a)) still ends there: scaled down with the rest
    /// (A350-900 diversion of the Commercial Transport plan), its CAS above 250 kt was flown down
    /// to 6,000 ft at a vertical rate set for the band above 10,000 ft, below flight idle.
    #[test]
    fn a_shortened_profile_keeps_the_descent_to_the_speed_restriction() {
        let preset = alas_config::presets::get("A350-900").unwrap();
        let defaults = preset.operational_mission_defaults();
        let restriction_m = alas_config::mission::SPEED_RESTRICTION_ALTITUDE_M;
        let request = MissionRequest {
            mission_tag: "short".to_owned(),
            cruise_altitude_m: defaults.cruise_altitude_m,
            departure_elevation_m: 0.0,
            arrival_elevation_m: 0.0,
            departure_isa_deviation_c: 0.0,
            route_distance_m: 370_000.0,
            profile: defaults.profile,
        };
        let schedule = own_schedules(&build_schedule(&request).unwrap());
        let mut top_m: f64 = 0.0;
        let mut ends = Vec::new();
        for spec in &schedule {
            match spec.kind {
                SegmentKind::Climb { altitude_end_m, .. } => top_m = top_m.max(altitude_end_m),
                SegmentKind::Descent { altitude_end_m, .. }
                    if junction::family(&spec.tag).starts_with("descent_2") =>
                {
                    ends.push(altitude_end_m);
                }
                _ => {}
            }
        }
        assert!(
            top_m < request.cruise_altitude_m - 100.0,
            "the profile is lowered: {top_m}"
        );
        assert!(top_m > restriction_m, "{top_m}");
        let last = ends.last().copied().expect("a descent to 10,000 ft");
        assert!(
            (last - restriction_m).abs() < 1.0,
            "descent_2 ends at {last} m"
        );
    }

    /// A profile lowered to between 10,000 and 12,000 ft: the descent rung flown above 250 kt to
    /// the crossover is not scaled below 10,000 ft, and the rung meeting 10,000 ft still flies.
    #[test]
    fn a_profile_topped_just_above_the_restriction_keeps_every_fast_descent_above_it() {
        let preset = alas_config::presets::get("A340-300").unwrap();
        let defaults = preset.operational_mission_defaults();
        let restriction_m = alas_config::mission::SPEED_RESTRICTION_ALTITUDE_M;
        let mut in_gap = 0;
        for route_km in (130..=230).chain((240..=600).step_by(20)) {
            let request = MissionRequest {
                mission_tag: "short".to_owned(),
                cruise_altitude_m: defaults.cruise_altitude_m,
                departure_elevation_m: 0.0,
                arrival_elevation_m: 0.0,
                departure_isa_deviation_c: 0.0,
                route_distance_m: f64::from(route_km) * 1000.0,
                profile: defaults.profile.clone(),
            };
            let schedule = own_schedules(&build_schedule(&request).unwrap());
            let top_m = schedule
                .iter()
                .filter_map(|spec| match spec.kind {
                    SegmentKind::Climb { altitude_end_m, .. } => Some(altitude_end_m),
                    _ => None,
                })
                .fold(0.0_f64, f64::max);
            let mut meets_restriction = false;
            for spec in &schedule {
                if let SegmentKind::Descent { altitude_end_m, .. } = spec.kind {
                    let fast = closure::faster_than_restriction(spec)
                        && !junction::family(&spec.tag).starts_with("final_landing");
                    assert!(
                        !fast || altitude_end_m > restriction_m - 1.0,
                        "{route_km} km: {} ends at {altitude_end_m} m above 250 kt",
                        spec.tag
                    );
                    meets_restriction |= (altitude_end_m - restriction_m).abs() < 1.0;
                }
            }
            if top_m > restriction_m + 1.0 && top_m < 3657.6 {
                in_gap += 1;
                assert!(
                    meets_restriction,
                    "{route_km} km, top {top_m} m: no descent to 10,000 ft"
                );
            }
        }
        assert!(
            in_gap > 0,
            "no route lowers the A340-300 profile to 10,000-12,000 ft"
        );
    }
}
