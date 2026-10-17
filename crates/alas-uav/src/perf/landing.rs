// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Approach, flare and braked rollout.
//!
//! Approach: speed-scheduled glide at `r_app Vs` (idle throttle by default)
//! down to the flare height. Flare: exponential flare `dh/dt = -h / tau`,
//! `tau = h_f / sink_0`, speed decays under drag and the lift coefficient
//! needed for `L = W cos(gamma)` is checked against `CLmax`. The flare ends
//! at touchdown, which is the first of two events: `h = 0.05 h_f`, or the
//! lift coefficient reaching `touchdown_cl_fraction * CLmax` (see
//! [`LandingSpec`]). Without the second event an idle flare decelerates to
//! the stall boundary before it reaches the ground. Rollout: see `ground`
//! (brakes `mu_brake`). The flare is a kinematic idealisation, not a pilot
//! model.

use super::airframe::Airframe;
use super::flight::{End, PathModel, ThrottleMode, Vertical};
use super::ground::rollout_from;
use super::integrate::{run, Model, Sample, IE, IH, IQ, IV, IX, NY, Y};
use super::powertrain::{BatteryState, Powertrain};
use super::{Environment, PerfError};

/// Landing parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandingSpec {
    /// Approach speed over the 1 g stall speed (1.3 typical).
    pub approach_speed_ratio: f64,
    /// Approach throttle (0 = idle glide).
    pub approach_throttle: f64,
    /// Flare height, m.
    pub flare_height_m: f64,
    /// Landing zone length, m, if it is to be checked.
    pub zone_m: Option<f64>,
    /// Lift coefficient at touchdown as a fraction of `CLmax`, in (0, 1].
    /// The flare ends there (touchdown). The default `1 / 1.15^2` puts
    /// touchdown at 1.15 times the 1 g stall speed (ASSUMPTION: the usual
    /// touchdown margin over stall for light aircraft; not a cited value).
    pub touchdown_cl_fraction: f64,
}

impl Default for LandingSpec {
    fn default() -> Self {
        Self {
            approach_speed_ratio: 1.3,
            approach_throttle: 0.0,
            flare_height_m: 1.5,
            zone_m: None,
            touchdown_cl_fraction: 1.0 / (1.15 * 1.15),
        }
    }
}

/// Result of a landing.
#[derive(Debug, Clone, PartialEq)]
pub struct LandingResult {
    /// Time from the start of the approach to standstill, s.
    pub duration_s: f64,
    /// Ground distance from the flare start to touchdown, m.
    pub flare_distance_m: f64,
    /// Ground distance of the braked rollout, m.
    pub rollout_m: f64,
    /// Airspeed at touchdown, m/s.
    pub touchdown_speed_m_s: f64,
    /// Touchdown airspeed over the 1 g stall speed.
    pub touchdown_over_stall: f64,
    /// Rollout fits in the zone (touchdown at the zone start), when given.
    pub within_zone: Option<bool>,
    /// Samples (not including the starting state).
    pub samples: Vec<Sample>,
    /// Final state.
    pub end_state: Y,
    /// Final time, s.
    pub end_time_s: f64,
}

struct FlareModel<'a> {
    af: &'a Airframe,
    pt: &'a dyn Powertrain,
    env: Environment,
    th: f64,
    tau: f64,
    h_end: f64,
    /// Lift coefficient at which the flare ends (touchdown), `fraction CLmax`.
    cl_touchdown: f64,
}

impl FlareModel<'_> {
    fn parts(&self, y: &Y) -> Result<(f64, f64, f64, super::PowerPoint, f64), PerfError> {
        let (rho, v, w) = (self.env.rho, y[IV], self.af.weight());
        let sin_g = (-y[IH] / self.tau / v).clamp(-0.5, 0.5);
        let gamma = sin_g.asin();
        let cl = self.af.cl_required(rho, v, w * gamma.cos(), "flare")?;
        let drag = self.af.drag(rho, v, cl, 1.0, 0.0);
        let bat = BatteryState {
            energy_used_j: y[IE],
            charge_used_c: y[IQ],
        };
        let point = self.pt.evaluate(self.th, v, rho, &bat)?;
        Ok((gamma, cl, drag, point, sin_g))
    }
}

impl Model for FlareModel<'_> {
    fn rhs(&self, _t: f64, y: &Y) -> Result<Y, PerfError> {
        let (gamma, _, drag, point, sin_g) = self.parts(y)?;
        let mut dy = [0.0; NY];
        dy[IX] = y[IV] * gamma.cos() - self.env.headwind_m_s;
        dy[IH] = -y[IH] / self.tau;
        dy[IV] = (point.thrust_n - drag) / self.af.mass_kg - super::airframe::G0 * sin_g;
        dy[IE] = point.battery_current_a * point.battery_voltage_v;
        dy[IQ] = point.battery_current_a;
        Ok(dy)
    }

    fn event(&self, _t: f64, y: &Y) -> f64 {
        // Touchdown at the flare height end, or when the lift coefficient
        // reaches the touchdown limit. Both start negative and cross zero
        // upward, so their maximum does too. Uses the same flight path as
        // `parts`, without its stall error (the limit is below CLmax).
        let sin_g = (-y[IH] / self.tau / y[IV].max(1e-9)).clamp(-0.5, 0.5);
        let lift_n = self.af.weight() * sin_g.asin().cos();
        let cl = lift_n / self.af.qs(self.env.rho, y[IV]).max(1e-9);
        (self.h_end - y[IH]).max(cl - self.cl_touchdown)
    }

    fn sample(&self, t: f64, y: &Y) -> Result<Sample, PerfError> {
        let (gamma, cl, drag, point, _) = self.parts(y)?;
        Ok(Sample {
            t,
            x: y[IX],
            h: y[IH],
            v: y[IV],
            gamma,
            throttle: self.th,
            thrust_n: point.thrust_n,
            drag_n: drag,
            cl,
            load_factor: 1.0,
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

/// Land from state `y0` at time `t0` (any height above the flare height).
pub fn land(
    af: &Airframe,
    pt: &dyn Powertrain,
    env: Environment,
    spec: LandingSpec,
    t0: f64,
    y0: Y,
    dt: f64,
) -> Result<LandingResult, PerfError> {
    af.validate()?;
    if !(spec.approach_speed_ratio >= 1.05
        && spec.flare_height_m > 0.0
        && spec.flare_height_m.is_finite()
        && spec.touchdown_cl_fraction > 0.0
        && spec.touchdown_cl_fraction <= 1.0)
    {
        return Err(PerfError::InvalidInput(
            "landing parameters out of range".into(),
        ));
    }
    let vs = af.stall_speed(env.rho);
    let v_app = spec.approach_speed_ratio * vs;
    let mut samples = Vec::new();
    let (mut t, mut y) = (t0, y0);
    if y[IH] > spec.flare_height_m {
        let mut m = PathModel::new(
            af,
            pt,
            env,
            Vertical::Balance,
            ThrottleMode::Fixed(spec.approach_throttle),
            v_app,
            End::AltitudeDown(spec.flare_height_m),
            "approach",
        );
        m.wind = env.headwind_m_s;
        let end = run(&mut m, t, y, dt, 600.0, &mut samples)?;
        (t, y) = (end.t, end.y);
    } else {
        y[IH] = y[IH].min(spec.flare_height_m);
    }
    // Flare from the sink rate reached on the approach (floor 0.3 m/s).
    let sink = samples
        .last()
        .map_or(0.3, |s| (-s.v * s.gamma.sin()).max(0.3));
    let mut flare = FlareModel {
        af,
        pt,
        env,
        th: spec.approach_throttle,
        tau: y[IH].max(1e-3) / sink,
        h_end: 0.05 * spec.flare_height_m,
        cl_touchdown: spec.touchdown_cl_fraction * af.polar.cl_max(),
    };
    let x_flare0 = y[IX];
    let end = run(&mut flare, t, y, dt, 120.0, &mut samples)?;
    let flare_distance_m = end.y[IX] - x_flare0;
    let (t_td, mut y_td) = (end.t, end.y);
    y_td[IH] = 0.0;
    let v_td = y_td[IV];
    let x_td = y_td[IX];
    let roll = rollout_from(af, pt, env, t_td, y_td, dt, &mut samples)?;
    let rollout_m = roll.y[IX] - x_td;
    Ok(LandingResult {
        duration_s: roll.t - t0,
        flare_distance_m,
        rollout_m,
        touchdown_speed_m_s: v_td,
        touchdown_over_stall: v_td / vs,
        within_zone: spec.zone_m.map(|z| rollout_m <= z),
        samples,
        end_state: roll.y,
        end_time_s: roll.t,
    })
}
