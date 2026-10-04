// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Takeoff-speed derivation and per-type mission speed schedules for presets that carry one.

/// International knot, m/s.
const KNOT_M_S: f64 = 1852.0 / 3600.0;
/// Feet per minute, m/s.
const FT_MIN_M_S: f64 = 0.3048 / 60.0;
/// ISA sea-level density, kg/m^3, and standard gravity, m/s^2, for the
/// takeoff-speed derivation below (a sea-level, standard-day reference
/// stall speed, at which CAS, EAS and TAS coincide).
const ISA_SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;
const STANDARD_GRAVITY_M_S2: f64 = 9.806_65;
/// CS-25.107(b) / FAR 25.107(b): V2 may not be less than 1.13 V_SR for a
/// two-engine turbopropeller aeroplane. The regulatory minimum ratio, used
/// here as the scheduled ratio (an assumption: an operator's actual V2 is
/// tabulated per weight, flap setting and atmosphere from AFM data this
/// preset does not carry).
const ATR72_600_V2_OVER_VSR: f64 = 1.13;

/// The ATR 72-600 takeoff-segment speed, m/s CAS: `1.13 * V_SR` with the
/// reference stall speed taken at `mtow_kg`, `wing_area_m2` and the preset's
/// configured takeoff lift limit `cl_max_takeoff` on a sea-level standard
/// day, floored at the factsheet's published V2 min of 116 KCAS.
///
/// The lift limit is the preset's `conservative_simple_flaps` modelling
/// value, not ATR high-lift data, so the result (about 135 KCAS at 23 t,
/// 61 m^2 and CL_max 1.6) is a model-consistent operational speed under a
/// declared assumption, and the mission deck's takeoff lift check then
/// holds with the 1.13^2 margin. It is not a validated all-mass V2
/// schedule; replacing the lift limit with source-specific high-lift data
/// moves this speed with it.
pub fn atr72_600_takeoff_speed_m_s(mtow_kg: f64, wing_area_m2: f64, cl_max_takeoff: f64) -> f64 {
    let published_v2_min_m_s = 116.0 * KNOT_M_S;
    let stall_reference_m_s = (2.0 * mtow_kg * STANDARD_GRAVITY_M_S2
        / (ISA_SEA_LEVEL_DENSITY_KG_M3 * wing_area_m2 * cl_max_takeoff))
        .sqrt();
    (ATR72_600_V2_OVER_VSR * stall_reference_m_s).max(published_v2_min_m_s)
}

/// The ATR 72-600's takeoff/climb/descent/landing schedule, stated in
/// calibrated airspeed and resolved to true airspeed against the real
/// ambient state at each leg's own altitude by both mission paths.
///
/// `MissionProfileConfig::default()` is the long-range-widebody worked
/// example (128.6-250 m/s *true* airspeeds with a 10,000 ft takeoff
/// segment), which a PW127M-powered turboprop cannot fly: the mission deck
/// rejected it with a typed climb energy deficit at 991 m and 128.6 m/s.
///
/// Sourced values (ATR 72-600 factsheet, 2020, page 2):
/// - optimum climb speed 170 KCAS (initial climb and both step climbs);
/// - V2 min 116 KCAS. This is a published *minimum* at an unspecified
///   weight, configuration and atmosphere, not a V2 valid at every mass: at
///   this preset's MTOW it implies a takeoff lift coefficient above the
///   preset's configured takeoff limit (1.615 against 1.60), and the
///   mission deck rightly refuses to fly it. The takeoff segment therefore
///   flies [`atr72_600_takeoff_speed_m_s`], a speed derived from the
///   preset's own MTOW, reference wing area and configured takeoff lift
///   limit, with the published minimum kept only as a floor;
/// - approach speed 113 KIAS: an *indicated* airspeed, not a published
///   CAS. The landing leg flies 113 KCAS as an operational approximation
///   that ASSUMES ZERO position and instrument error (unsourced; on a
///   transport aircraft the difference is of the order of one to a few
///   knots). It is therefore not an independent CAS reference and must not
///   be used to validate the CAS conversion;
/// - sea-level, MTOW rate of climb 1,355 ft/min (requested for the takeoff
///   segment only; the deck's rating limit caps whatever cannot be
///   delivered).
///
/// Everything else below is an UNSOURCED profile assumption, chosen to be
/// operationally plausible for a 23 t turboprop cruising at FL170 and
/// labelled as such: the 1,500 ft AGL takeoff-segment top (a typical
/// acceleration altitude), the en-route climb rates, the whole descent
/// ladder (altitudes, calibrated speeds and rates: the factsheet publishes
/// no altitude-resolved climb or descent table) and the 3-degree-like
/// approach rate. None of it is calibrated to the factsheet's block fuel or
/// time figures; see an internal speed-schedule-integration study
/// (2026-09-07) for the comparison that was actually run.
pub(super) fn apply_atr72_600_speed_schedule(
    profile: &mut crate::MissionProfileConfig,
    mtow_kg: f64,
    wing_area_m2: f64,
    cl_max_takeoff: f64,
) {
    profile.climb_descent_speed_reference = crate::SpeedReference::CalibratedAirspeed;
    // One assigned cruise level, not a cruise-climb; see `atr72_600_schedule_fix`.
    super::atr72_600_schedule_fix::fly_single_assigned_level(profile);
    // Takeoff segment: the derived V2 held to a 1,500 ft AGL acceleration
    // altitude at the published sea-level MTOW rate of climb.
    profile.takeoff_altitude_gain_m = 1_500.0 * 0.3048; // unsourced
    profile.takeoff_air_speed_m_s =
        atr72_600_takeoff_speed_m_s(mtow_kg, wing_area_m2, cl_max_takeoff);
    profile.takeoff_climb_rate_m_s = 1_355.0 * FT_MIN_M_S; // factsheet SL/MTOW ROC
                                                           // En-route climb at the published optimum climb speed; the rates are
                                                           // unsourced and below the sea-level figure because the deck's available
                                                           // power falls with altitude (a request above the rating is capped, not
                                                           // silently met).
    profile.initial_climb_air_speed_m_s = 170.0 * KNOT_M_S; // factsheet
    profile.initial_climb_rate_m_s = 1_000.0 * FT_MIN_M_S; // unsourced
    profile.step_climb_1_air_speed_m_s = 170.0 * KNOT_M_S; // factsheet
    profile.step_climb_1_rate_m_s = 600.0 * FT_MIN_M_S; // unsourced
    profile.step_climb_2_air_speed_m_s = 170.0 * KNOT_M_S; // factsheet
    profile.step_climb_2_rate_m_s = 600.0 * FT_MIN_M_S; // unsourced
                                                        // Descent ladder: entirely unsourced. Speeds are kept below the 250 KIAS
                                                        // class VMO with margin and step down towards the approach speed.
    profile.descent_1_altitude_ft = 10_000.0;
    profile.descent_1_air_speed_m_s = 220.0 * KNOT_M_S;
    profile.descent_1_rate_m_s = 1_500.0 * FT_MIN_M_S;
    profile.descent_2_altitude_ft = 6_000.0;
    profile.descent_2_air_speed_m_s = 200.0 * KNOT_M_S;
    profile.descent_2_rate_m_s = 1_200.0 * FT_MIN_M_S;
    profile.descent_3_altitude_ft = 3_000.0;
    profile.descent_3_air_speed_m_s = 170.0 * KNOT_M_S;
    profile.descent_3_rate_m_s = 1_000.0 * FT_MIN_M_S;
    profile.descent_4_altitude_ft = 1_500.0;
    profile.descent_4_air_speed_m_s = 140.0 * KNOT_M_S;
    profile.descent_4_rate_m_s = 800.0 * FT_MIN_M_S;
    // Final approach at the published approach speed on a nominal 3-degree
    // path (600 ft/min at ~113 kt ground speed; unsourced rate).
    // Factsheet 113 KIAS taken as 113 KCAS: zero position/instrument error
    // assumed (unsourced approximation, see above).
    profile.landing_air_speed_m_s = 113.0 * KNOT_M_S;
    profile.landing_descent_rate_m_s = 600.0 * FT_MIN_M_S; // unsourced
}

/// CS-25.107(b)(1)/FAR 25.107(b)(1): V2 may not be less than 1.13 V_SR for a
/// two-engine turbojet without provisions for obtaining a significant
/// reduction in one-engine-inoperative stall speed. The same regulatory
/// minimum the ATR helper above uses, for the same reason and with the same
/// caveat: an operator's V2 is tabulated per weight, flap setting and
/// atmosphere from AFM data this preset does not carry.
const NARROWBODY_V2_OVER_VSR: f64 = 1.13;

/// The A320-200's takeoff/climb/descent/landing schedule, stated in
/// calibrated airspeed and resolved to true airspeed against the real ambient
/// state at each leg's own altitude by both mission paths.
///
/// **Why this preset needs its own ladder, measured rather than assumed.**
/// `MissionProfileConfig::default()` is a *literal true airspeed* ladder
/// written for the AVE reference aircraft's design point (FL390, M0.84):
/// 250 m/s true on the upper climb rungs. A true airspeed is not a flight
/// condition. Applied to a narrowbody whose operational cruise is FL280, the
/// same number lands at about 178 m/s equivalent - roughly 345 kt, past what
/// an A320 climbs at by a third - and the mission deck refuses it with a typed
/// climb energy deficit: measured at 6 894 m and 250.0 m/s, **57 046 N of drag
/// against 56 106 N of maximum-climb rating**, so the aircraft cannot hold that
/// speed level, let alone climb 3 m/s at it. That single defect rejected
/// **253 of 253** A320-200 candidates in the all-preset matrix as
/// `dispatch_model_failed`. The aeroplane was never the problem; the schedule
/// was.
///
/// **What is sourced and what is not.** The speeds are the standard
/// air-transport climb and descent profile, which is a published operating
/// convention rather than a measurement of this airframe: 250 kt below
/// 10 000 ft (the regulatory speed limit), 300 kt above it, with no Mach
/// crossover rung because 300 kt calibrated reaches M0.78 near FL290, above
/// this preset's declared FL280 cruise, so one calibrated speed covers the
/// whole upper climb without an invented break point. Every vertical **rate**
/// is unsourced and representative, exactly as in the ATR helper above; none
/// of it is calibrated to a block-fuel or block-time figure. The takeoff
/// segment flies a V2 derived from this preset's own MTOW, reference wing
/// area and configured takeoff lift limit, so no speed is asserted that the
/// preset's own geometry does not support.
///
/// **A latent hazard this does not fix, stated here because it is the same
/// defect:** the shared default ladder is still literal true airspeed, and it
/// is still written for AVE's flight level. Any other preset flown at a lower
/// cruise altitude inherits the same over-speed, and the four widebodies that
/// currently close their dispatch do so because their operational levels
/// happen to sit near the one the default was written for, not because the
/// contract is right.
pub(super) fn apply_a320_200_speed_schedule(
    profile: &mut crate::MissionProfileConfig,
    cruise_altitude_m: f64,
    mtow_kg: f64,
    wing_area_m2: f64,
    cl_max_takeoff: f64,
) {
    profile.climb_descent_speed_reference = crate::SpeedReference::CalibratedAirspeed;
    let transition_m = 10_000.0 * 0.3048;
    let stall_reference_m_s = (2.0 * mtow_kg * STANDARD_GRAVITY_M_S2
        / (ISA_SEA_LEVEL_DENSITY_KG_M3 * wing_area_m2 * cl_max_takeoff))
        .sqrt();

    // Takeoff segment: V2 held to a 1,500 ft AGL acceleration altitude.
    profile.takeoff_altitude_gain_m = 1_500.0 * 0.3048; // unsourced
    profile.takeoff_air_speed_m_s = NARROWBODY_V2_OVER_VSR * stall_reference_m_s;
    profile.takeoff_climb_rate_m_s = 2_500.0 * FT_MIN_M_S; // unsourced

    // The 250 kt speed limit below 10,000 ft, then 300 kt to the cruise
    // level. The rung boundaries are fractions of the cruise altitude, which
    // is the schema's own convention: they are set here so the break lands on
    // 10,000 ft for this preset's *declared* cruise level, and they move with
    // an edited cruise altitude the way every other preset's do.
    let transition_fraction = (transition_m / cruise_altitude_m).clamp(0.05, 0.9);
    profile.initial_climb_altitude_fraction = transition_fraction;
    profile.initial_climb_air_speed_m_s = 250.0 * KNOT_M_S;
    profile.initial_climb_rate_m_s = 2_500.0 * FT_MIN_M_S; // unsourced
    profile.step_climb_1_altitude_fraction = (0.5 * (1.0 + transition_fraction)).clamp(0.1, 0.95);
    profile.step_climb_1_air_speed_m_s = 300.0 * KNOT_M_S;
    profile.step_climb_1_rate_m_s = 1_800.0 * FT_MIN_M_S; // unsourced
    profile.step_climb_2_air_speed_m_s = 300.0 * KNOT_M_S;
    profile.step_climb_2_rate_m_s = 1_000.0 * FT_MIN_M_S; // unsourced

    // Descent ladder: the same 300/250 kt convention read downwards, stepping
    // to the approach speed. Altitudes and rates unsourced.
    profile.descent_1_altitude_ft = 20_000.0;
    profile.descent_1_air_speed_m_s = 300.0 * KNOT_M_S;
    profile.descent_1_rate_m_s = 2_000.0 * FT_MIN_M_S;
    profile.descent_2_altitude_ft = 10_000.0;
    profile.descent_2_air_speed_m_s = 300.0 * KNOT_M_S;
    profile.descent_2_rate_m_s = 2_000.0 * FT_MIN_M_S;
    profile.descent_3_altitude_ft = 5_000.0;
    profile.descent_3_air_speed_m_s = 250.0 * KNOT_M_S;
    profile.descent_3_rate_m_s = 1_500.0 * FT_MIN_M_S;
    profile.descent_4_altitude_ft = 3_000.0;
    profile.descent_4_air_speed_m_s = 210.0 * KNOT_M_S;
    profile.descent_4_rate_m_s = 1_000.0 * FT_MIN_M_S;

    // Final approach at 1.23 V_SR, the CS-25.125 reference landing approach
    // speed ratio, on a nominal 3-degree path (unsourced rate).
    profile.landing_air_speed_m_s = 1.23 * stall_reference_m_s;
    profile.landing_descent_rate_m_s = 700.0 * FT_MIN_M_S; // unsourced
}
