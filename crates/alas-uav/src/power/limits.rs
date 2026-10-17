// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Demands, the current/power limiter, thrust-demand inversion and time stepping.
//!
//! The limiter lowers the duty `D` until every active limit holds.  Battery
//! current, motor current, motor power, shaft speed and ESC current all rise
//! monotonically with `D`, so a bisection on `D` finds the largest compliant
//! duty.  This is the explicit "current limiter" mode of the plan (AeroUD and
//! UVigo implemented one): the cap is on the BATTERY-side current.

use super::battery::BatteryStep;
use super::operating::{Dynamics, Flight, OperatingPoint, Powertrain};
use super::PowerError;

/// What the pilot or mission asks of the powertrain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Demand {
    /// Throttle as ESC duty, 0..1 (clamped to `d_max`).
    Throttle(f64),
    /// Total thrust, N (all motors).
    Thrust(f64),
}

/// Which motor/ESC rating applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RatingBasis {
    /// Continuous ratings (steady cruise).
    Continuous,
    /// Burst ratings (short take-off pulses).
    Burst,
}

/// A limit or condition attached to a solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitFlag {
    /// The duty was clamped to the ESC `d_max`.
    DutyLimit,
    /// The battery-side current cap of the limiter.
    BatteryCurrentCap,
    /// Motor current rating.
    MotorCurrent,
    /// Motor input power rating.
    MotorPower,
    /// Motor maximum speed.
    MotorSpeed,
    /// ESC output current rating.
    EscCurrent,
    /// Pack continuous discharge rating.
    BatteryRating,
    /// The steady winding temperature exceeds the motor limit.
    WindingTemperature,
    /// Loaded pack voltage or SOC at the cutoff.
    BatteryCutoff,
    /// BEMT annuli fell back to zero induction.
    PropDegraded,
    /// Blade section Reynolds number below the NeuralFoil warning limit.
    PropLowRe,
    /// The thrust demand was not met because a limit bound.
    ThrustShortfall,
}

/// Limiter configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limiter {
    /// Cap on the pack current, A.
    pub battery_current_cap_a: Option<f64>,
    /// Enforce motor/ESC/pack ratings (otherwise they are reported as warnings).
    pub enforce_ratings: bool,
    /// Which rating set applies.
    pub basis: RatingBasis,
}

impl Limiter {
    /// No cap, ratings reported only.
    pub fn none() -> Self {
        Self {
            battery_current_cap_a: None,
            enforce_ratings: false,
            basis: RatingBasis::Continuous,
        }
    }
}

impl Powertrain {
    fn ratios(&self, op: &OperatingPoint, lim: &Limiter) -> Vec<(LimitFlag, f64, bool)> {
        let mut out = Vec::new();
        if let Some(cap) = lim.battery_current_cap_a {
            out.push((LimitFlag::BatteryCurrentCap, op.i_battery_a / cap, true));
        }
        let e = lim.enforce_ratings;
        let m = &self.motor.limits;
        let pick = |cont: Option<f64>, burst: Option<f64>| match lim.basis {
            RatingBasis::Continuous => cont,
            RatingBasis::Burst => burst.or(cont),
        };
        if let Some(i) = pick(m.i_cont_a, m.i_burst_a) {
            out.push((LimitFlag::MotorCurrent, op.i_motor_a / i, e));
        }
        if let Some(p) = m.p_max_w {
            out.push((LimitFlag::MotorPower, op.v_motor_v * op.i_motor_a / p, e));
        }
        if let Some(w) = m.omega_max_rad_s {
            out.push((LimitFlag::MotorSpeed, op.omega_rad_s / w, e));
        }
        if let Some(i) = pick(self.esc.i_cont_a, self.esc.i_burst_a) {
            out.push((LimitFlag::EscCurrent, op.i_motor_a / i, e));
        }
        if let Some(i) = self.battery.i_cont_a {
            out.push((LimitFlag::BatteryRating, op.i_battery_a / i, e));
        }
        if let Some(t) = m.t_winding_max_k {
            let rise = op.t_winding_steady_k - op.t_winding_k.min(op.t_winding_steady_k);
            let allowed = (t - op.t_winding_k).max(1.0e-9);
            if op.t_winding_steady_k > t {
                out.push((LimitFlag::WindingTemperature, 1.0 + rise / allowed, false));
            }
        }
        out
    }

    fn compliant(&self, op: &OperatingPoint, lim: &Limiter) -> bool {
        self.ratios(op, lim)
            .iter()
            .all(|&(_, r, enforced)| !enforced || r <= 1.0 + 1.0e-9)
    }

    /// Thrust (N) at duty `d`, 0 when no equilibrium exists.
    fn thrust_at(&self, fl: &Flight, dy: &Dynamics, d: f64) -> f64 {
        self.solve_duty(fl, dy, d).map_or(0.0, |op| op.thrust_n)
    }

    /// Solve for a demand, applying the limiter.
    ///
    /// # Errors
    ///
    /// [`PowerError::DemandUnreachable`] when a thrust demand exceeds the
    /// unlimited maximum; [`PowerError::NoSolution`] / `InvalidInput` as in
    /// [`Powertrain::solve_duty`].
    pub fn solve(
        &self,
        fl: &Flight,
        dy: &Dynamics,
        demand: Demand,
        lim: &Limiter,
    ) -> Result<OperatingPoint, PowerError> {
        let d_max = self.esc.d_max;
        let (d_req, clamped) = match demand {
            Demand::Throttle(d) => {
                if !(d.is_finite() && (0.0..=1.0).contains(&d)) {
                    return Err(PowerError::InvalidInput(format!(
                        "throttle must be in [0, 1], got {d}"
                    )));
                }
                (d.min(d_max), d > d_max)
            }
            Demand::Thrust(t) => {
                if !(t.is_finite() && t >= 0.0) {
                    return Err(PowerError::InvalidInput(format!(
                        "thrust demand must be >= 0, got {t}"
                    )));
                }
                let available = self.thrust_at(fl, dy, d_max);
                if t > available {
                    return Err(PowerError::DemandUnreachable {
                        demand_n: t,
                        available_n: available,
                    });
                }
                let (mut lo, mut hi) = (0.0, d_max);
                for _ in 0..60 {
                    let mid = 0.5 * (lo + hi);
                    if self.thrust_at(fl, dy, mid) < t {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                (hi, false)
            }
        };
        let mut op = self.solve_duty(fl, dy, d_req)?;
        op.duty_requested = d_req;
        if clamped {
            op.limited_by.push(LimitFlag::DutyLimit);
        }
        if !self.compliant(&op, lim) {
            let (mut lo, mut hi) = (0.0, d_req);
            for _ in 0..60 {
                let mid = 0.5 * (lo + hi);
                let ok = self
                    .solve_duty(fl, dy, mid)
                    .map_or(true, |p| self.compliant(&p, lim));
                if ok {
                    lo = mid;
                } else {
                    hi = mid;
                }
            }
            let mut limited = self.solve_duty(fl, dy, lo)?;
            limited.duty_requested = d_req;
            limited.limited_by = op.limited_by.clone();
            for (flag, r, enforced) in self.ratios(&op, lim) {
                if enforced && r > 1.0 {
                    limited.limited_by.push(flag);
                }
            }
            op = limited;
            if let Demand::Thrust(t) = demand {
                if op.thrust_n < t * (1.0 - 1.0e-6) {
                    op.limited_by.push(LimitFlag::ThrustShortfall);
                }
            }
        }
        for (flag, r, enforced) in self.ratios(&op, lim) {
            if !enforced && r > 1.0 {
                op.warnings.push(flag);
            }
        }
        Ok(op)
    }

    /// Solve with the winding at its steady temperature for the operating
    /// heat (fixed-point iteration, converged to 1e-4 K); without a motor
    /// thermal model the winding stays at the ambient temperature.
    ///
    /// # Errors
    ///
    /// As [`Powertrain::solve`]; also `NoSolution` if the iteration diverges.
    pub fn solve_steady_thermal(
        &self,
        fl: &Flight,
        dy: &Dynamics,
        demand: Demand,
        lim: &Limiter,
    ) -> Result<OperatingPoint, PowerError> {
        let mut state = *dy;
        state.t_winding_k = fl.t_amb_k;
        for _ in 0..60 {
            let op = self.solve(fl, &state, demand, lim)?;
            if (op.t_winding_steady_k - state.t_winding_k).abs() < 1.0e-4 {
                return Ok(op);
            }
            if !op.t_winding_steady_k.is_finite() || op.t_winding_steady_k > 800.0 {
                return Err(PowerError::NoSolution(
                    "thermal runaway: winding above 800 K".into(),
                ));
            }
            state.t_winding_k = op.t_winding_steady_k;
        }
        Err(PowerError::NoSolution(
            "steady winding temperature did not converge".into(),
        ))
    }

    /// Advance the pack and winding by `dt` seconds at constant `op`.
    pub fn advance(
        &self,
        dy: &mut Dynamics,
        op: &OperatingPoint,
        fl: &Flight,
        dt: f64,
    ) -> BatteryStep {
        let heat = (op.p_copper_w + op.p_iron_w) / f64::from(self.n_motors.max(1));
        dy.t_winding_k = self
            .motor
            .advance_temperature(dy.t_winding_k, fl.t_amb_k, heat, dt);
        self.battery
            .step(&mut dy.battery, op.i_battery_a, fl.t_amb_k, dt)
    }
}
