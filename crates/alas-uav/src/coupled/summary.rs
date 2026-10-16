// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Quasi-steady performance numbers of a built aircraft, the figures team
//! reports publish: static thrust, stall speed, maximum level speed, best
//! climb rate and the take-off roll.
//!
//! Level flight at weight `W`: `CL = W / (q S)`, `D = q S (CDp + CDi)` from the
//! polar, thrust from the powertrain at full duty (the current limiter, when
//! configured, binds).  Climb rate is the small-angle excess-power form
//! `(T - D) V / W`.  Speeds are true airspeeds in calm air at the density of
//! the models.

use crate::perf::{
    ground_roll, BatteryState, Environment, GroundRollResult, PerfError, Powertrain, TakeoffSpec,
};

use super::assess::Models;
use super::CoupledError;

/// Quasi-steady figures.
#[derive(Debug, Clone, PartialEq)]
pub struct PerformanceSummary {
    /// Static thrust at full duty, N.
    pub static_thrust_n: f64,
    /// Battery current at that point, A.
    pub static_current_a: f64,
    /// Stall speed at 1 g, m/s.
    pub stall_speed_m_s: f64,
    /// Maximum level speed, m/s (`None`: thrust never reaches the drag).
    pub v_max_m_s: Option<f64>,
    /// Best rate of climb, m/s.
    pub max_climb_rate_m_s: f64,
    /// Speed of the best climb rate, m/s.
    pub v_best_climb_m_s: f64,
    /// Battery current at the best-climb point, A.
    pub climb_current_a: f64,
    /// Minimum-drag speed, m/s.
    pub v_min_drag_m_s: f64,
    /// Ground roll to lift-off.
    pub takeoff: GroundRollResult,
}

/// Compute the summary of `models` at `env`.
///
/// # Errors
///
/// [`CoupledError::Perf`] when the powertrain or the roll fails.
pub fn performance_summary(
    models: &Models,
    env: Environment,
    takeoff: TakeoffSpec,
) -> Result<PerformanceSummary, CoupledError> {
    let af = &models.airframe;
    let pt = &models.powertrain;
    let fresh = BatteryState::default();
    let thrust = |v: f64| -> Result<(f64, f64), PerfError> {
        let p = pt.evaluate(1.0, v, env.rho, &fresh)?;
        Ok((p.thrust_n, p.battery_current_a))
    };
    let w = af.weight();
    let drag = |v: f64| -> f64 {
        let cl = w / af.qs(env.rho, v);
        let (cdp, cdi) = af.polar.cd_parts(cl);
        af.qs(env.rho, v) * (cdp + cdi)
    };
    let vs = af.stall_speed(env.rho);
    let (t0, i0) = thrust(0.0)?;
    let mut best = (f64::NEG_INFINITY, vs, 0.0);
    let mut v_min_drag = (f64::INFINITY, vs);
    let mut v_max = None;
    let mut prev: Option<(f64, f64)> = None;
    let mut v = 1.05 * vs;
    while v <= 60.0 {
        let (t, i) = thrust(v)?;
        let d = drag(v);
        let rate = (t - d) * v / w;
        if rate > best.0 {
            best = (rate, v, i);
        }
        if d < v_min_drag.0 {
            v_min_drag = (d, v);
        }
        if let Some((vp, ep)) = prev {
            if ep > 0.0 && t - d <= 0.0 {
                // Bisect the last crossing between vp and v.
                let (mut lo, mut hi) = (vp, v);
                for _ in 0..40 {
                    let mid = 0.5 * (lo + hi);
                    let (tm, _) = thrust(mid)?;
                    if tm - drag(mid) > 0.0 {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                v_max = Some(0.5 * (lo + hi));
            }
        }
        prev = Some((v, t - d));
        v += 0.25;
    }
    let roll = ground_roll(af, pt, env, takeoff, 0.05)?;
    Ok(PerformanceSummary {
        static_thrust_n: t0,
        static_current_a: i0,
        stall_speed_m_s: vs,
        v_max_m_s: v_max,
        max_climb_rate_m_s: best.0,
        v_best_climb_m_s: best.1,
        climb_current_a: best.2,
        v_min_drag_m_s: v_min_drag.1,
        takeoff: roll,
    })
}
