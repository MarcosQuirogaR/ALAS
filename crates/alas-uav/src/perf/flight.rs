// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airborne point-mass model: level flight, turns, speed-scheduled climbs and
//! glides.
//!
//! Along the path: `m dv/dt = T - D - W sin(gamma)` with `L = n W cos(gamma)`,
//! where `n = 1/cos(bank)` in a coordinated level turn and `n = 1` otherwise.
//! `Vertical::Level` fixes `gamma = 0` and lets `v` evolve under `T - D`;
//! `Vertical::Balance` makes `v` follow a commanded speed with time constant
//! `tau` (acceleration limited) and takes `gamma` from the force balance, which
//! is the standard speed-scheduled climb or glide. Ground-projected distance
//! rate is `v cos(gamma) - headwind`.

use super::airframe::{Airframe, G0};
use super::integrate::{Model, Sample, IE, IH, IPSI, IQ, IV, IX, NY, Y};
use super::powertrain::{BatteryState, PowerPoint, Powertrain};
use super::{Environment, PerfError};

/// Acceleration limit of the speed schedule, m/s^2.
const A_LIM: f64 = 3.0;

/// Vertical treatment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vertical {
    /// Constant height, speed from `T - D`.
    Level,
    /// Speed follows the schedule, climb angle from the force balance.
    Balance,
}

/// Throttle law.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThrottleMode {
    /// Constant throttle.
    Fixed(f64),
    /// Throttle chosen each step so speed follows the commanded value (level flight).
    HoldSpeed,
}

/// Event that ends the segment (absolute values in the mission state).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum End {
    /// Mission time, s.
    Time(f64),
    /// Ground-projected distance, m.
    Distance(f64),
    /// Height reached from below, m.
    AltitudeUp(f64),
    /// Height reached from above, m.
    AltitudeDown(f64),
    /// Heading change, rad.
    Heading(f64),
}

/// Airborne segment model.
pub struct PathModel<'a> {
    /// Airframe.
    pub af: &'a Airframe,
    /// Powertrain.
    pub pt: &'a dyn Powertrain,
    /// Environment.
    pub env: Environment,
    /// Headwind component applied to ground distance, m/s.
    pub wind: f64,
    /// Vertical treatment.
    pub vertical: Vertical,
    /// Throttle law.
    pub throttle: ThrottleMode,
    /// Commanded airspeed, m/s.
    pub v_cmd: f64,
    /// Speed time constant, s.
    pub tau: f64,
    /// Bank angle, rad (level turns only).
    pub bank: f64,
    /// End event.
    pub end: End,
    /// Optional absolute time cap, s: the segment also ends there.
    pub cap_time: Option<f64>,
    /// Segment label for error messages.
    pub label: &'static str,
    th: f64,
}

struct Eval {
    dy: Y,
    point: PowerPoint,
    gamma: f64,
    cl: f64,
    drag: f64,
    n: f64,
}

impl<'a> PathModel<'a> {
    /// New model; the initial throttle is `Fixed` value or 0.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        af: &'a Airframe,
        pt: &'a dyn Powertrain,
        env: Environment,
        vertical: Vertical,
        throttle: ThrottleMode,
        v_cmd: f64,
        end: End,
        label: &'static str,
    ) -> Self {
        let th = match throttle {
            ThrottleMode::Fixed(t) => t,
            ThrottleMode::HoldSpeed => 0.5,
        };
        Self {
            af,
            pt,
            env,
            wind: 0.0,
            vertical,
            throttle,
            v_cmd,
            tau: 2.0,
            bank: 0.0,
            end,
            cap_time: None,
            label,
            th,
        }
    }

    fn battery(y: &Y) -> BatteryState {
        BatteryState {
            energy_used_j: y[IE],
            charge_used_c: y[IQ],
        }
    }

    fn load_factor(&self) -> f64 {
        1.0 / self.bank.cos()
    }

    /// Level-flight drag at speed `v` and load factor `n`.
    fn level_drag(&self, v: f64, n: f64) -> Result<f64, PerfError> {
        let cl = self
            .af
            .cl_required(self.env.rho, v, n * self.af.weight(), self.label)?;
        Ok(self.af.drag(self.env.rho, v, cl, 1.0, 0.0))
    }

    fn eval(&self, y: &Y) -> Result<Eval, PerfError> {
        let (rho, v, m, w) = (self.env.rho, y[IV], self.af.mass_kg, self.af.weight());
        if v < 1.0 {
            return Err(PerfError::CannotSustain(format!(
                "{}: airspeed {v:.2} m/s below 1 m/s",
                self.label
            )));
        }
        let point = self.pt.evaluate(self.th, v, rho, &Self::battery(y))?;
        let (gamma, dv, cl, drag, n) = match self.vertical {
            Vertical::Level => {
                let n = self.load_factor();
                let cl = self.af.cl_required(rho, v, n * w, self.label)?;
                let drag = self.af.drag(rho, v, cl, 1.0, 0.0);
                (0.0, (point.thrust_n - drag) / m, cl, drag, n)
            }
            Vertical::Balance => {
                let dv = ((self.v_cmd - v) / self.tau).clamp(-A_LIM, A_LIM);
                let mut gamma = 0.0_f64;
                let (mut cl, mut drag) = (0.0, 0.0);
                for _ in 0..4 {
                    cl = self.af.cl_required(rho, v, w * gamma.cos(), self.label)?;
                    drag = self.af.drag(rho, v, cl, 1.0, 0.0);
                    let s = ((point.thrust_n - drag) - m * dv) / w;
                    gamma = s.clamp(-0.9, 0.9).asin();
                }
                (gamma, dv, cl, drag, 1.0)
            }
        };
        let ground_speed = v * gamma.cos() - self.wind;
        if ground_speed < 0.1 {
            return Err(PerfError::CannotSustain(format!(
                "{}: headwind {:.1} m/s leaves no ground speed",
                self.label, self.wind
            )));
        }
        let mut dy = [0.0; NY];
        dy[IX] = ground_speed;
        dy[IH] = v * gamma.sin();
        dy[IV] = dv;
        dy[IE] = point.battery_current_a * point.battery_voltage_v;
        dy[IQ] = point.battery_current_a;
        dy[IPSI] = if self.bank == 0.0 {
            0.0
        } else {
            G0 * self.bank.tan() / v
        };
        Ok(Eval {
            dy,
            point,
            gamma,
            cl,
            drag,
            n,
        })
    }

    /// Throttle in `[0, 1]` that makes `T - D = m a` at speed `v` (clamped).
    fn solve_throttle(&self, y: &Y) -> Result<f64, PerfError> {
        let (rho, v) = (self.env.rho, y[IV]);
        let a = ((self.v_cmd - v) / self.tau).clamp(-A_LIM, A_LIM);
        let need = self.level_drag(v, self.load_factor())? + self.af.mass_kg * a;
        let bat = Self::battery(y);
        let thrust = |th: f64| self.pt.evaluate(th, v, rho, &bat).map(|p| p.thrust_n);
        if thrust(1.0)? <= need {
            return Ok(1.0);
        }
        if thrust(0.0)? >= need {
            return Ok(0.0);
        }
        let (mut lo, mut hi) = (0.0, 1.0);
        for _ in 0..30 {
            let mid = 0.5 * (lo + hi);
            if thrust(mid)? < need {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        Ok(0.5 * (lo + hi))
    }
}

impl Model for PathModel<'_> {
    fn prepare(&mut self, _t: f64, y: &Y) -> Result<(), PerfError> {
        if self.throttle == ThrottleMode::HoldSpeed {
            self.th = self.solve_throttle(y)?;
        }
        Ok(())
    }

    fn rhs(&self, _t: f64, y: &Y) -> Result<Y, PerfError> {
        Ok(self.eval(y)?.dy)
    }

    fn event(&self, t: f64, y: &Y) -> f64 {
        let main = match self.end {
            End::Time(te) => t - te,
            End::Distance(xe) => y[IX] - xe,
            End::AltitudeUp(h) => y[IH] - h,
            End::AltitudeDown(h) => h - y[IH],
            End::Heading(p) => y[IPSI] - p,
        };
        match self.cap_time {
            Some(cap) => main.max(t - cap),
            None => main,
        }
    }

    fn sample(&self, t: f64, y: &Y) -> Result<Sample, PerfError> {
        let e = self.eval(y)?;
        Ok(Sample {
            t,
            x: y[IX],
            h: y[IH],
            v: y[IV],
            gamma: e.gamma,
            throttle: self.th,
            thrust_n: e.point.thrust_n,
            drag_n: e.drag,
            cl: e.cl,
            load_factor: e.n,
            current_a: e.point.battery_current_a,
            voltage_v: e.point.battery_voltage_v,
            power_w: e.point.battery_current_a * e.point.battery_voltage_v,
            energy_j: y[IE],
            charge_c: y[IQ],
            psi: y[IPSI],
            segment: 0,
        })
    }
}
