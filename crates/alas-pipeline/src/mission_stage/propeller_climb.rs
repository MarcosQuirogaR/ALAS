// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The default-rule climb speed of a propeller aircraft: the best rate of climb.
//!
//! The default ladder (`alas_config` `apply_default_ladder`) climbs at the calibrated airspeed of
//! the cruise Mach at the cruise level, which is the jet practice of arriving at the cruise speed.
//! A propeller's available power, not its thrust, is nearly independent of speed, so its rate of
//! climb `(eta P - D V) / W` is greatest at the speed of minimum power required, where
//! `CL = sqrt(3 CD0 / K)` (Sadraey, *Aircraft Design: A Systems Engineering Approach*, Wiley
//! 2013, Eq. 4.85, p. 139). With the linear term of the fitted polar `CD = CD0 + c1 CL + K CL^2`
//! the minimum of `CD / CL^1.5` is the root of `K CL^2 - c1 CL - 3 CD0 = 0`.
//!
//! That lift coefficient is bounded by the clean-configuration speed margin of CS 25.107(g): the
//! final take-off speed, the en-route climb speed the take-off path ends at, may not be less
//! than 1.18 `VSR` (EASA CS-25, Amendment 27, CS 25.107(g)(1)), so `CL <= CLmax / 1.18^2`.
//!
//! Only the default rule is replaced: a preset with a published or hand-written schedule (the
//! ATR 72 at its factsheet 170 kt, the A400M) keeps its own ladder, and a jet keeps the default.

use std::borrow::Cow;

use alas_config::{presets::SpeedScheduleSource, AlasConfig, SpeedReference};

use crate::full_analysis::AnalysisReport;

/// CS 25.107(g)(1): the final take-off speed is not less than 1.18 `VSR`.
const FINAL_TAKEOFF_SPEED_OVER_VSR: f64 = 1.18;

/// Sea-level standard density, kg/m^3 (the equivalent-airspeed basis).
const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;

/// `config` with its climb flown at the best rate of climb when it is a propeller aircraft on
/// the default schedule rule; `config` itself otherwise.
pub(super) fn with_best_rate_climb<'a>(
    config: &'a AlasConfig,
    report: &AnalysisReport,
) -> Cow<'a, AlasConfig> {
    match best_rate_climb_cas_m_s(config, report) {
        Some(cas_m_s) => {
            let mut climbing = config.clone();
            let profile = &mut climbing.mission.profile;
            // The take-off band is held to it too: climbing out faster than the en-route climb
            // speed only to slow down to it after the band (the C295 accelerated to 190 kt CAS
            // by 1,500 ft and then traded speed for height down to its 140 kt best rate).
            for speed in [
                &mut profile.takeoff_air_speed_m_s,
                &mut profile.initial_climb_air_speed_m_s,
                &mut profile.step_climb_1_air_speed_m_s,
                &mut profile.step_climb_2_air_speed_m_s,
            ] {
                // Never faster than the ladder: its caps (250 kt below 10,000 ft, VMO) and the
                // cruise speed it arrives at stay upper limits.
                *speed = speed.min(cas_m_s);
            }
            Cow::Owned(climbing)
        }
        None => Cow::Borrowed(config),
    }
}

/// The calibrated airspeed of the best rate of climb at the maximum take-off mass, m/s, or
/// `None` when the rule does not apply (a jet, a published schedule, no fitted polar).
///
/// The climb holds the lift coefficient, so it holds the equivalent airspeed
/// `sqrt(2 m g / (rho0 S CL))`; that is taken as the calibrated airspeed, which differs from it
/// by the compressibility of the impact pressure, under 1 % below Mach 0.4.
pub(super) fn best_rate_climb_cas_m_s(config: &AlasConfig, report: &AnalysisReport) -> Option<f64> {
    let default_rule = config.preset.is_empty()
        || alas_config::presets::get(&config.preset)
            .is_ok_and(|preset| preset.speed_schedule_source == SpeedScheduleSource::DefaultRule);
    let propeller = matches!(
        config.geometry.engine.active_model(),
        Ok(alas_config::ActiveEngineModel::Turboprop(_))
    );
    let calibrated =
        config.mission.profile.climb_descent_speed_reference == SpeedReference::CalibratedAirspeed;
    if !(default_rule && propeller && calibrated) {
        return None;
    }
    let polar = &report.polar_fit;
    if !polar.status.is_fitted() || !(polar.k > 0.0 && polar.cd0 > 0.0) {
        return None;
    }
    let minimum_power_cl =
        (polar.c1 + (polar.c1 * polar.c1 + 12.0 * polar.k * polar.cd0).sqrt()) / (2.0 * polar.k);
    let margin_cl = config.performance.cl_max_clean
        / (FINAL_TAKEOFF_SPEED_OVER_VSR * FINAL_TAKEOFF_SPEED_OVER_VSR);
    let cl = minimum_power_cl.min(margin_cl);
    let weight_n = config.requirements.mtow_kg * alas_atmo::STANDARD_GRAVITY;
    let area_m2 = report.airplane.s_ref;
    let cas_m_s = (2.0 * weight_n / (SEA_LEVEL_DENSITY_KG_M3 * area_m2 * cl)).sqrt();
    (cas_m_s.is_finite() && cas_m_s > 0.0).then_some(cas_m_s)
}
