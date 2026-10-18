// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fixed-step RK4 driver with exact event location.
//!
//! The state vector is `Y = [x, h, v, energy, charge, psi]` (see the `I*`
//! indices). Control inputs that depend on the state (throttle governors) are
//! held constant over a step (`Model::prepare`, zero-order hold), so the right
//! hand side is smooth within a step and RK4 keeps its order. The step that
//! crosses an event is re-integrated with the step length found by bisection,
//! so segment ends (time, distance, altitude, heading, lift-off) land on the
//! event to machine-level tolerance instead of to the step size.

use super::PerfError;

/// Number of state variables.
pub const NY: usize = 6;
/// State vector.
pub type Y = [f64; NY];
/// Ground-projected path length, m.
pub const IX: usize = 0;
/// Height above the field, m.
pub const IH: usize = 1;
/// True airspeed, m/s.
pub const IV: usize = 2;
/// Energy drawn from the battery, J.
pub const IE: usize = 3;
/// Charge drawn from the battery, C.
pub const IQ: usize = 4;
/// Heading change, rad.
pub const IPSI: usize = 5;

/// Longest step accepted, s (plan 3.3: dt <= 0.05 s so 10 Hz logging is exact).
pub const MAX_DT_S: f64 = 0.05;

/// One logged sample (all SI; angles rad).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Mission time, s.
    pub t: f64,
    /// Ground-projected distance, m.
    pub x: f64,
    /// Height, m.
    pub h: f64,
    /// True airspeed, m/s.
    pub v: f64,
    /// Flight-path angle, rad (positive climbing).
    pub gamma: f64,
    /// Throttle, 0..1.
    pub throttle: f64,
    /// Thrust, N.
    pub thrust_n: f64,
    /// Drag, N.
    pub drag_n: f64,
    /// Lift coefficient.
    pub cl: f64,
    /// Load factor.
    pub load_factor: f64,
    /// Battery current, A.
    pub current_a: f64,
    /// Battery terminal voltage, V.
    pub voltage_v: f64,
    /// Battery power, W.
    pub power_w: f64,
    /// Energy drawn, J.
    pub energy_j: f64,
    /// Charge drawn, C.
    pub charge_c: f64,
    /// Heading change, rad.
    pub psi: f64,
    /// Index of the mission segment.
    pub segment: usize,
}

/// A segment's dynamics.
pub trait Model {
    /// Update held controls at the start of a step.
    fn prepare(&mut self, _t: f64, _y: &Y) -> Result<(), PerfError> {
        Ok(())
    }
    /// State derivative at `(t, y)`.
    fn rhs(&self, t: f64, y: &Y) -> Result<Y, PerfError>;
    /// Event function: `>= 0` once the segment has ended.
    fn event(&self, t: f64, y: &Y) -> f64;
    /// Logged quantities at `(t, y)`.
    fn sample(&self, t: f64, y: &Y) -> Result<Sample, PerfError>;
}

/// End of a run.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RunEnd {
    /// Time of the event, s.
    pub t: f64,
    /// State at the event.
    pub y: Y,
}

fn axpy(y: &Y, k: &Y, a: f64) -> Y {
    let mut out = *y;
    for (o, d) in out.iter_mut().zip(k) {
        *o += a * d;
    }
    out
}

fn rk4(model: &dyn Model, t: f64, y: &Y, dt: f64) -> Result<Y, PerfError> {
    let k1 = model.rhs(t, y)?;
    let k2 = model.rhs(t + 0.5 * dt, &axpy(y, &k1, 0.5 * dt))?;
    let k3 = model.rhs(t + 0.5 * dt, &axpy(y, &k2, 0.5 * dt))?;
    let k4 = model.rhs(t + dt, &axpy(y, &k3, dt))?;
    let mut out = *y;
    for i in 0..NY {
        out[i] += dt / 6.0 * (k1[i] + 2.0 * k2[i] + 2.0 * k3[i] + k4[i]);
    }
    if out.iter().all(|v| v.is_finite()) {
        Ok(out)
    } else {
        Err(PerfError::NoConvergence("non-finite state".into()))
    }
}

/// Integrate from `(t0, y0)` until the model's event fires; push one sample
/// per step and one at the event. The start state is pushed only when `out`
/// is empty (the first segment of a mission), so chained segments do not
/// duplicate the boundary sample.
pub fn run(
    model: &mut dyn Model,
    t0: f64,
    y0: Y,
    dt: f64,
    t_max: f64,
    out: &mut Vec<Sample>,
) -> Result<RunEnd, PerfError> {
    if !(dt.is_finite() && dt > 0.0 && dt <= MAX_DT_S + 1e-12) {
        return Err(PerfError::InvalidInput(format!(
            "time step must be in (0, {MAX_DT_S}] s"
        )));
    }
    let (mut t, mut y) = (t0, y0);
    if out.is_empty() {
        out.push(model.sample(t, &y)?);
    }
    if model.event(t, &y) >= 0.0 {
        return Ok(RunEnd { t, y });
    }
    loop {
        if t - t0 > t_max {
            return Err(PerfError::NoConvergence(format!(
                "segment did not end within {t_max} s"
            )));
        }
        model.prepare(t, &y)?;
        let next = rk4(model, t, &y, dt)?;
        if model.event(t + dt, &next) >= 0.0 {
            let (mut lo, mut hi) = (0.0, dt);
            for _ in 0..60 {
                let mid = 0.5 * (lo + hi);
                let ym = rk4(model, t, &y, mid)?;
                if model.event(t + mid, &ym) >= 0.0 {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            let y_end = rk4(model, t, &y, hi)?;
            out.push(model.sample(t + hi, &y_end)?);
            return Ok(RunEnd {
                t: t + hi,
                y: y_end,
            });
        }
        t += dt;
        y = next;
        out.push(model.sample(t, &y)?);
    }
}
