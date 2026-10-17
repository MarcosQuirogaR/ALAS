// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Ground roll: take-off and braked rollout.
//!
//! `m dV/dt = T(V) - D - mu (W - L)` on the runway, with `V` the airspeed,
//! `V - headwind` the ground speed and `dx/dt` the ground speed. Thrust comes
//! from the powertrain at the instantaneous airspeed and a throttle ramp.
//! Drag uses the polar at the ground lift coefficient with the induced part
//! scaled by the ground-effect factor (Raymer Eq. 12.60) plus the gear drag area.
//!
//! Rotation is a kinematic model: the lift coefficient is `cl_ground` below
//! `V_R = r_rot Vs` and rises linearly with speed to `CLmax / r_lof^2` at
//! `V_LOF = r_lof Vs`, where `L = W` and the wheels leave the ground
//! (event `L >= W`). With `r_lof = 1.15` the aircraft leaves the ground at
//! 1.15 Vs, inside the 1.1-1.2 range of the team reports. Take-off must be
//! into the wind or in calm air.

use super::airframe::Airframe;
use super::integrate::{run, Model, RunEnd, Sample, IE, IH, IQ, IV, IX, NY, Y};
use super::powertrain::{BatteryState, Powertrain};
use super::{Environment, PerfError};

/// Take-off parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TakeoffSpec {
    /// Throttle ramp from 0 to 1, s (1.7 s in the UVigo report).
    pub throttle_ramp_s: f64,
    /// Rotation speed over the 1 g stall speed.
    pub rotation_speed_ratio: f64,
    /// Lift-off speed over the 1 g stall speed (1.1 to 1.2).
    pub liftoff_speed_ratio: f64,
    /// Available runway, m, if it is to be checked.
    pub runway_m: Option<f64>,
}

impl Default for TakeoffSpec {
    fn default() -> Self {
        Self {
            throttle_ramp_s: 1.0,
            rotation_speed_ratio: 1.0,
            liftoff_speed_ratio: 1.15,
            runway_m: None,
        }
    }
}

/// Result of a ground roll.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundRollResult {
    /// Ground distance to lift-off, m.
    pub distance_m: f64,
    /// Time to lift-off, s.
    pub time_s: f64,
    /// Airspeed at lift-off, m/s.
    pub liftoff_airspeed_m_s: f64,
    /// Largest battery current, A.
    pub peak_current_a: f64,
    /// `distance_m <= runway_m`, when a runway was given.
    pub within_runway: Option<bool>,
    /// Logged samples (not including the standing start).
    pub samples: Vec<Sample>,
}

enum Phase {
    /// Accelerating on the runway, throttle ramping.
    Takeoff(TakeoffSpec),
    /// Braked rollout after touchdown.
    Rollout,
}

pub(super) struct GroundModel<'a> {
    af: &'a Airframe,
    pt: &'a dyn Powertrain,
    env: Environment,
    phase: Phase,
    /// Time at which the ramp started, s.
    t_ramp0: f64,
}

impl GroundModel<'_> {
    fn throttle(&self, t: f64) -> f64 {
        match &self.phase {
            Phase::Takeoff(s) => ((t - self.t_ramp0) / s.throttle_ramp_s).clamp(0.0, 1.0),
            Phase::Rollout => 0.0,
        }
    }

    fn cl(&self, v: f64) -> f64 {
        match &self.phase {
            Phase::Rollout => self.af.cl_ground,
            Phase::Takeoff(s) => {
                let vs = self.af.stall_speed(self.env.rho);
                let (v_r, v_lof) = (s.rotation_speed_ratio * vs, s.liftoff_speed_ratio * vs);
                let cl_lof =
                    self.af.polar.cl_max() / (s.liftoff_speed_ratio * s.liftoff_speed_ratio);
                if v <= v_r || v_lof <= v_r {
                    self.af.cl_ground
                } else {
                    let f = ((v - v_r) / (v_lof - v_r)).clamp(0.0, 1.0);
                    self.af.cl_ground + f * (cl_lof - self.af.cl_ground).max(0.0)
                }
            }
        }
    }

    fn forces(&self, t: f64, y: &Y) -> Result<(f64, f64, f64, f64, super::PowerPoint), PerfError> {
        let v = y[IV].max(0.0);
        let rho = self.env.rho;
        let bat = BatteryState {
            energy_used_j: y[IE],
            charge_used_c: y[IQ],
        };
        let th = self.throttle(t);
        let point = self.pt.evaluate(th, v, rho, &bat)?;
        let cl = self.cl(v);
        let lift = self.af.qs(rho, v) * cl;
        let extra = self.af.gear_drag_area_m2 / self.af.wing_area_m2;
        let drag = self
            .af
            .drag(rho, v, cl, self.af.ground_effect_factor(), extra);
        Ok((th, cl, lift, drag, point))
    }
}

impl Model for GroundModel<'_> {
    fn rhs(&self, t: f64, y: &Y) -> Result<Y, PerfError> {
        let (_, _, lift, drag, point) = self.forces(t, y)?;
        let w = self.af.weight();
        let mu = match self.phase {
            Phase::Takeoff(_) => self.af.mu_roll,
            Phase::Rollout => self.af.mu_brake,
        };
        let normal = (w - lift).max(0.0);
        let mut dy = [0.0; NY];
        let ground_speed = y[IV] - self.env.headwind_m_s;
        let mut accel = (point.thrust_n - drag - mu * normal) / self.af.mass_kg;
        if ground_speed <= 0.0 && accel < 0.0 {
            // Static friction: the aircraft stays at rest until thrust exceeds it.
            accel = 0.0;
        }
        dy[IX] = ground_speed.max(0.0);
        dy[IV] = accel;
        dy[IE] = point.battery_current_a * point.battery_voltage_v;
        dy[IQ] = point.battery_current_a;
        Ok(dy)
    }

    fn event(&self, t: f64, y: &Y) -> f64 {
        match self.phase {
            Phase::Takeoff(_) => {
                let cl = self.cl(y[IV].max(0.0));
                self.af.qs(self.env.rho, y[IV].max(0.0)) * cl - self.af.weight()
            }
            Phase::Rollout => {
                let _ = t;
                0.05 - (y[IV] - self.env.headwind_m_s)
            }
        }
    }

    fn sample(&self, t: f64, y: &Y) -> Result<Sample, PerfError> {
        let (th, cl, lift, drag, point) = self.forces(t, y)?;
        Ok(Sample {
            t,
            x: y[IX],
            h: 0.0,
            v: y[IV],
            gamma: 0.0,
            throttle: th,
            thrust_n: point.thrust_n,
            drag_n: drag,
            cl,
            load_factor: lift / self.af.weight(),
            current_a: point.battery_current_a,
            voltage_v: point.battery_voltage_v,
            power_w: point.battery_current_a * point.battery_voltage_v,
            energy_j: y[IE],
            charge_c: y[IQ],
            psi: 0.0,
            segment: 0,
        })
    }
}

/// Ground roll from the state `y0` at time `t0` (standing start: `y0[IV]`
/// equals the headwind). Used by the mission; see [`ground_roll`].
#[allow(clippy::too_many_arguments)]
pub(super) fn takeoff_from(
    af: &Airframe,
    pt: &dyn Powertrain,
    env: Environment,
    spec: TakeoffSpec,
    t0: f64,
    y0: Y,
    dt: f64,
    out: &mut Vec<Sample>,
) -> Result<RunEnd, PerfError> {
    if env.headwind_m_s < 0.0 {
        return Err(PerfError::InvalidInput(
            "take-off with a tailwind is not modelled".into(),
        ));
    }
    if !(spec.throttle_ramp_s > 0.0
        && spec.rotation_speed_ratio > 0.0
        && spec.liftoff_speed_ratio >= 1.0
        && spec.liftoff_speed_ratio.is_finite())
    {
        return Err(PerfError::InvalidInput(
            "take-off parameters out of range".into(),
        ));
    }
    let mut m = GroundModel {
        af,
        pt,
        env,
        phase: Phase::Takeoff(spec),
        t_ramp0: t0,
    };
    run(&mut m, t0, y0, dt, 60.0, out)
}

/// Braked rollout from touchdown state `y0` to a stop (ground speed 0.05 m/s).
pub(super) fn rollout_from(
    af: &Airframe,
    pt: &dyn Powertrain,
    env: Environment,
    t0: f64,
    y0: Y,
    dt: f64,
    out: &mut Vec<Sample>,
) -> Result<RunEnd, PerfError> {
    let mut m = GroundModel {
        af,
        pt,
        env,
        phase: Phase::Rollout,
        t_ramp0: t0,
    };
    run(&mut m, t0, y0, dt, 120.0, out)
}

/// Stand-alone ground roll from rest to lift-off.
pub fn ground_roll(
    af: &Airframe,
    pt: &dyn Powertrain,
    env: Environment,
    spec: TakeoffSpec,
    dt: f64,
) -> Result<GroundRollResult, PerfError> {
    af.validate()?;
    let mut y0 = [0.0; NY];
    y0[IV] = env.headwind_m_s;
    y0[IH] = 0.0;
    let mut samples = Vec::new();
    let end = takeoff_from(af, pt, env, spec, 0.0, y0, dt, &mut samples)?;
    let peak = samples.iter().map(|s| s.current_a).fold(0.0, f64::max);
    Ok(GroundRollResult {
        distance_m: end.y[IX],
        time_s: end.t,
        liftoff_airspeed_m_s: end.y[IV],
        peak_current_a: peak,
        within_runway: spec.runway_m.map(|r| end.y[IX] <= r),
        samples,
    })
}
