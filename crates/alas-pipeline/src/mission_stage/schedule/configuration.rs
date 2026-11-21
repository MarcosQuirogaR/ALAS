// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Configuration drag of the take-off and approach legs.

/// A take-off leg flown below this share of the initial-climb speed is flown at
/// a high-lift speed (V2 against 250 kt CAS is 0.6; a ladder that climbs out
/// at the climb speed is 1.0).
const HIGH_LIFT_SPEED_SHARE_OF_CLIMB_SPEED: f64 = 0.95;

/// Flap, slat and gear drag of the legs flown in a high-lift configuration.
///
/// The take-off leg (liftoff to the 1,500 ft acceleration height) is the
/// gear-up second-segment configuration, so it carries the take-off flap
/// increment; the final approach carries the landing flap and gear
/// increments. The coefficients are the sourced `PerformanceConfig` ones
/// the take-off and landing field models already use (Roskam I Table 3.6,
/// Raymer Eq. 12.61-12.62, Torenbeek Sec. 5.4.3); every other leg is clean.
///
/// The take-off flaps are out only while the leg is flown at a high-lift speed:
/// a take-off leg flown at the speed of the initial climb (a ladder that
/// climbs out at 250 kt CAS) is a clean leg, flaps do not fly at that speed.
/// A speed-change leg carries the mean of the legs around it (flaps retracting
/// or extending during the change).
pub(in crate::mission_stage) fn apply_configuration_drag(
    schedule: &mut [alas_mission::segments::SegmentSpec],
    performance: &alas_config::PerformanceConfig,
) {
    let climb_speed_m_s = schedule
        .iter()
        .find(|spec| spec.tag == "initial_climb")
        .map(|spec| spec.air_speed_m_s);
    let flaps_out_for_acceleration = schedule
        .iter()
        .any(|spec| spec.tag == super::takeoff::TAKEOFF_CLIMB_TAG);
    for spec in schedule.iter_mut() {
        let high_lift_speed = climb_speed_m_s
            .is_some_and(|climb| spec.air_speed_m_s < HIGH_LIFT_SPEED_SHARE_OF_CLIMB_SPEED * climb);
        spec.configuration_delta_cd = match super::junction::family(&spec.tag) {
            "takeoff" | "takeoff_upper" if high_lift_speed => performance.oei_climb_delta_cd,
            // The climb at V2 and the acceleration along it are flown with the take-off flaps
            // out, as the priced model flies them.
            super::takeoff::TAKEOFF_CLIMB_TAG => performance.oei_climb_delta_cd,
            "takeoff" if flaps_out_for_acceleration => performance.oei_climb_delta_cd,
            tag if tag.starts_with("final_landing") => {
                performance.landing_flap_delta_cd + performance.landing_gear_delta_cd
            }
            _ => 0.0,
        };
    }
    super::speed_change::set_junction_configuration(schedule);
}
