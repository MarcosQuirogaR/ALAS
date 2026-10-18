// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Coupled motor - ESC - battery - propeller operating point.
//!
//! For `N` identical motors on one pack, duty `D`, airspeed `V` and density
//! `rho`, the unknown is the shaft speed `omega`.  For a trial `omega` the
//! propeller gives the torque `Q_p(omega)`, the motor current follows directly
//! (`i = (Q_p + Q0(omega)) / Kt(T)`), the ESC gives the pack current
//! `I_b = N ((D + k_sw) i + i_idle)`, the pack gives its terminal voltage
//! `V = Voc - I_b R_pack`, and the residual
//!
//! ```text
//! g(omega) = (D V - i r_on) - (i Rm(T) + omega Kt(T))
//! ```
//!
//! must vanish.  The highest root (a stable equilibrium: `g` falls with
//! `omega`) is found by a scan from the electrical no-load speed downward and
//! Brent's method.  Energy is conserved by construction and returned as
//! [`OperatingPoint::balance_residual`] (relative, within the root tolerance).
//!
//! Sign and unit conventions are in the [module docs](super).

use super::battery::{BatteryModel, BatteryState};
use super::esc::EscModel;
use super::interp::brent;
use super::motor::MotorModel;
use super::prop::{PropModel, PropSource};
use super::PowerError;

use super::limits::LimitFlag;

/// Ambient and flight condition.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flight {
    /// True airspeed along the shaft axis, m/s (>= 0).
    pub v_m_s: f64,
    /// Air density, kg/m3.
    pub rho_kg_m3: f64,
    /// Ambient air temperature, K.
    pub t_amb_k: f64,
    /// Dynamic viscosity, Pa*s (ISA sea level 1.789e-5).
    pub mu_pa_s: f64,
}

impl Flight {
    /// Flight condition with viscosity from Sutherland's law at `t_amb_k`.
    pub fn new(v_m_s: f64, rho_kg_m3: f64, t_amb_k: f64) -> Self {
        let mu = 1.458e-6 * t_amb_k.powf(1.5) / (t_amb_k + 110.4);
        Self {
            v_m_s,
            rho_kg_m3,
            t_amb_k,
            mu_pa_s: mu,
        }
    }

    fn validate(&self) -> Result<(), PowerError> {
        let ok = self.v_m_s.is_finite()
            && self.v_m_s >= 0.0
            && self.rho_kg_m3 > 0.0
            && self.rho_kg_m3.is_finite()
            && self.t_amb_k > 150.0
            && self.mu_pa_s > 0.0;
        if ok {
            Ok(())
        } else {
            Err(PowerError::InvalidInput(format!(
                "invalid flight condition {self:?}"
            )))
        }
    }
}

/// Slowly varying state: pack and winding.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dynamics {
    /// Battery state.
    pub battery: BatteryState,
    /// Winding temperature, K.
    pub t_winding_k: f64,
}

/// Solved operating point (totals are for all motors unless "per motor").
#[derive(Debug, Clone, PartialEq)]
pub struct OperatingPoint {
    /// Duty actually applied (0..`d_max`).
    pub duty: f64,
    /// Duty requested before the limiter acted.
    pub duty_requested: f64,
    /// Shaft speed, rad/s.
    pub omega_rad_s: f64,
    /// Shaft speed, rpm.
    pub rpm: f64,
    /// Advance ratio.
    pub j: f64,
    /// Total thrust, N.
    pub thrust_n: f64,
    /// Torque per motor, N*m.
    pub torque_n_m: f64,
    /// Motor current (per motor), A.
    pub i_motor_a: f64,
    /// Battery current (all motors), A.
    pub i_battery_a: f64,
    /// Open-circuit pack voltage, V.
    pub v_oc_v: f64,
    /// Pack terminal voltage under load, V.
    pub v_pack_v: f64,
    /// Voltage sag `I_b R_pack`, V.
    pub sag_v: f64,
    /// Motor terminal voltage, V.
    pub v_motor_v: f64,
    /// Pack terminal electrical power, W.
    pub p_battery_w: f64,
    /// Total shaft power, W.
    pub p_shaft_w: f64,
    /// Total thrust power `T V`, W.
    pub p_thrust_w: f64,
    /// Motor efficiency.
    pub eta_motor: f64,
    /// ESC efficiency.
    pub eta_esc: f64,
    /// Propeller efficiency `T V / P_shaft` (0 at static).
    pub eta_prop: f64,
    /// Overall `T V / P_battery_terminal` (0 at static).
    pub eta_total: f64,
    /// Copper loss, all motors, W.
    pub p_copper_w: f64,
    /// Iron/friction loss, all motors, W.
    pub p_iron_w: f64,
    /// ESC loss, all ESCs, W.
    pub p_esc_loss_w: f64,
    /// Pack internal loss `I_b^2 R_pack`, W.
    pub p_pack_loss_w: f64,
    /// Winding temperature used for this solve, K.
    pub t_winding_k: f64,
    /// Steady winding temperature at this heat input, K (ambient w/o thermal model).
    pub t_winding_steady_k: f64,
    /// Winding temperature rise rate now, K/s.
    pub dt_winding_dt_k_s: f64,
    /// Time until the winding reaches its limit at constant power, s.
    pub time_to_t_max_s: Option<f64>,
    /// State-of-charge rate, 1/s (negative when discharging).
    pub dsoc_dt: f64,
    /// Relative power-balance residual (within the root tolerance, ~1e-12).
    pub balance_residual: f64,
    /// Where the propeller loads came from.
    pub prop_source: PropSource,
    /// Limits that bound the solution (empty when unconstrained).
    pub limited_by: Vec<LimitFlag>,
    /// Non-binding warnings (ratings exceeded when not enforced, cutoff, degraded BEMT).
    pub warnings: Vec<LimitFlag>,
}

/// Motor(s), ESC(s), pack and propeller.
#[derive(Debug, Clone)]
pub struct Powertrain {
    /// Motor model (identical motors).
    pub motor: MotorModel,
    /// ESC model (one per motor).
    pub esc: EscModel,
    /// Battery pack.
    pub battery: BatteryModel,
    /// Propeller (identical propellers).
    pub prop: PropModel,
    /// Number of motors on the pack (>= 1).
    pub n_motors: u32,
}

impl Powertrain {
    fn n(&self) -> f64 {
        f64::from(self.n_motors.max(1))
    }

    /// Residual `g(omega)` and the pieces it was built from, for fixed duty.
    fn residual(&self, fl: &Flight, dy: &Dynamics, duty: f64, omega: f64) -> Option<f64> {
        let loads = self
            .prop
            .loads(omega, fl.v_m_s, fl.rho_kg_m3, fl.mu_pa_s)
            .ok()?;
        let t = dy.t_winding_k;
        let i = (loads.torque_n_m + self.motor.loss_torque(omega)) / self.motor.kt(t);
        let ib = self.n() * self.esc.i_input(duty, i);
        let v_pack = self.battery.v_terminal(dy.battery.soc, dy.battery.t_k, ib);
        let v_motor = self.esc.v_motor(duty, v_pack, i);
        Some(v_motor - (i * self.motor.rm(t) + omega * self.motor.kt(t)))
    }

    /// Solve at a fixed duty without any limiter.
    ///
    /// # Errors
    ///
    /// [`PowerError::NoSolution`] when there is no equilibrium in the usable
    /// speed range, [`PowerError::InvalidInput`] for a bad request.
    pub fn solve_duty(
        &self,
        fl: &Flight,
        dy: &Dynamics,
        duty: f64,
    ) -> Result<OperatingPoint, PowerError> {
        fl.validate()?;
        if !(duty.is_finite() && (0.0..=1.0).contains(&duty)) {
            return Err(PowerError::InvalidInput(format!(
                "duty must be in [0, 1], got {duty}"
            )));
        }
        let duty = duty.min(self.esc.d_max);
        if duty <= 1.0e-9 {
            return Err(PowerError::NoSolution(
                "zero duty: motor off, no thrust".into(),
            ));
        }
        let (p_lo, p_hi) = self
            .prop
            .omega_range(fl.v_m_s)
            .ok_or_else(|| PowerError::OutOfRange(format!("V = {:.2} m/s", fl.v_m_s)))?;
        let voc = self.battery.voc(dy.battery.soc);
        let no_load = duty * voc * self.motor.kv(dy.t_winding_k);
        let mut hi = p_hi.min(no_load * 1.02);
        if let Some(w) = self.motor.limits.omega_max_rad_s {
            hi = hi.min(1.5 * w);
        }
        let lo = p_lo;
        if hi <= lo {
            return Err(PowerError::NoSolution(format!(
                "usable speed range empty ({lo:.0}..{hi:.0} rad/s)"
            )));
        }
        let n = 64;
        let omega_at = |k: usize| hi - (hi - lo) * k as f64 / n as f64;
        let mut f = |w: f64| self.residual(fl, dy, duty, w);
        let mut prev: Option<(f64, f64)> = None;
        let mut root = None;
        for k in 0..=n {
            let w = omega_at(k);
            let Some(g) = f(w) else {
                prev = None;
                continue;
            };
            if let Some((w_prev, g_prev)) = prev {
                if g_prev.signum() != g.signum() {
                    root = brent(&mut f, w, w_prev, 1.0e-9 * hi);
                    break;
                }
            }
            prev = Some((w, g));
        }
        let omega = root.ok_or_else(|| {
            PowerError::NoSolution(format!(
                "no equilibrium for duty {duty:.3} at {:.1} m/s in {lo:.0}-{hi:.0} rad/s of the propeller data",
                fl.v_m_s
            ))
        })?;
        self.point(fl, dy, duty, omega)
    }

    fn point(
        &self,
        fl: &Flight,
        dy: &Dynamics,
        duty: f64,
        omega: f64,
    ) -> Result<OperatingPoint, PowerError> {
        let n = self.n();
        let t = dy.t_winding_k;
        let loads = self.prop.loads(omega, fl.v_m_s, fl.rho_kg_m3, fl.mu_pa_s)?;
        let ms = self.motor.state(omega, loads.torque_n_m, t);
        if ms.current_a <= 0.0 {
            return Err(PowerError::NoSolution(
                "windmilling: motor current <= 0".into(),
            ));
        }
        let i_b = n * self.esc.i_input(duty, ms.current_a);
        let r_pack = self.battery.r_pack(dy.battery.soc, dy.battery.t_k);
        let v_oc = self.battery.voc(dy.battery.soc);
        let v_pack = v_oc - i_b * r_pack;
        let v_motor = self.esc.v_motor(duty, v_pack, ms.current_a);
        let p_batt = v_pack * i_b;
        let esc_loss = self.esc.loss_w(v_pack, ms.current_a);
        let p_heat = ms.p_copper_w + ms.p_iron_w;
        let t_ss = self.motor.t_steady(fl.t_amb_k, p_heat);
        let thrust = n * loads.thrust_n;
        let p_shaft = n * ms.p_shaft_w;
        let p_thrust = thrust * fl.v_m_s;
        let accounted =
            n * (ms.p_shaft_w + ms.p_copper_w + ms.p_iron_w + esc_loss) + i_b * i_b * r_pack;
        let p_oc = v_oc * i_b;
        let mut warnings = Vec::new();
        if loads.degraded {
            warnings.push(LimitFlag::PropDegraded);
        }
        if loads.low_re {
            warnings.push(LimitFlag::PropLowRe);
        }
        if self.battery.is_cutoff(&dy.battery, i_b) {
            warnings.push(LimitFlag::BatteryCutoff);
        }
        Ok(OperatingPoint {
            duty,
            duty_requested: duty,
            omega_rad_s: omega,
            rpm: omega * 60.0 / std::f64::consts::TAU,
            j: loads.j,
            thrust_n: thrust,
            torque_n_m: loads.torque_n_m,
            i_motor_a: ms.current_a,
            i_battery_a: i_b,
            v_oc_v: v_oc,
            v_pack_v: v_pack,
            sag_v: i_b * r_pack,
            v_motor_v: v_motor,
            p_battery_w: p_batt,
            p_shaft_w: p_shaft,
            p_thrust_w: p_thrust,
            eta_motor: ms.efficiency,
            eta_esc: self.esc.efficiency(duty, v_pack, ms.current_a),
            eta_prop: if p_shaft > 0.0 {
                p_thrust / p_shaft
            } else {
                0.0
            },
            eta_total: if p_batt > 0.0 { p_thrust / p_batt } else { 0.0 },
            p_copper_w: n * ms.p_copper_w,
            p_iron_w: n * ms.p_iron_w,
            p_esc_loss_w: n * esc_loss,
            p_pack_loss_w: i_b * i_b * r_pack,
            t_winding_k: t,
            t_winding_steady_k: t_ss,
            dt_winding_dt_k_s: self.motor.dt_dt(t, fl.t_amb_k, p_heat),
            time_to_t_max_s: self
                .motor
                .limits
                .t_winding_max_k
                .and_then(|tm| self.motor.time_to_temperature(t, fl.t_amb_k, p_heat, tm)),
            dsoc_dt: -i_b / self.battery.capacity_c(),
            balance_residual: if p_oc.abs() > 0.0 {
                (p_oc - accounted) / p_oc
            } else {
                0.0
            },
            prop_source: self.prop.source(),
            limited_by: Vec::new(),
            warnings,
        })
    }
}
