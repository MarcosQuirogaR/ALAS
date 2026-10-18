// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Brushless motor: Drela's first-order model with a lumped thermal state.
//!
//! Electrical model (Drela, "First-Order DC Electric Motor Model", MIT, Feb
//! 2007, eqs. 1-3 and 12, with `Kt = 1/KQ = 1/Kv` in SI):
//!
//! ```text
//! v      = i Rm + omega Kt          terminal voltage (eq. 3)
//! Q      = Kt (i - i0)              shaft torque (eq. 1)
//! eta_m  = (1 - i0/i)(1 - i Rm / v) efficiency (eq. 7 with KQ = KV)
//! ```
//!
//! Two extensions beyond the paper:
//!
//! * `i0` depends on speed: the no-load *torque* `Q0(omega) = Kt_ref i0_ref
//!   (omega / (Kv_ref v_test))^a` (a = 1 by default; the plan's calibratable
//!   exponent) so a no-load current quoted at `v_test` is normalised to the
//!   operating speed.  An optional `i0(omega)` table overrides the power law.
//! * Temperature `T` of the winding: `Rm(T) = Rm_ref (1 + alpha (T - T_ref))`
//!   with the copper coefficient `alpha = 0.00393 1/K` (resistivity of copper,
//!   20 C), and magnet flux `phi(T) = 1 + beta (T - T_ref)` with
//!   `beta = -0.0012 1/K`, a typical NdFeB reversible coefficient of `Br`
//!   (ESTIMATE, not read from a datasheet; override per magnet grade).  The
//!   torque and back-EMF constant scale with flux, `Kt(T) = Kt_ref phi(T)`, so
//!   `Kv = 1/Kt` RISES when the magnets are hot.  The loss torque `Q0` is kept
//!   fixed, hence `i0(T) = Q0 / Kt(T)`.
//!
//! Power balance (exact): `v i = i^2 Rm + omega Q0 + omega Q`.

use super::interp::Table1;
use super::{finite_nonneg, finite_positive, PowerError};

/// Copper temperature coefficient of resistance at 20 C, 1/K.
pub const ALPHA_COPPER_PER_K: f64 = 0.003_93;
/// Default NdFeB flux temperature coefficient, 1/K (ESTIMATE; see module docs).
pub const BETA_NDFEB_PER_K: f64 = -0.001_2;
/// Reference temperature at which `Rm`, `Kv` and `i0` are quoted, K (20 C).
pub const T_REF_K: f64 = 293.15;

/// Lumped winding thermal model, `C dT/dt = P_heat - (T - T_amb)/R_th`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotorThermal {
    /// Winding-to-ambient thermal resistance, K/W.
    pub r_th_k_per_w: f64,
    /// Lumped heat capacity of winding and stator, J/K.
    pub c_th_j_per_k: f64,
}

/// Motor ratings used by the limiter and the verdicts.  `None` is unknown.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct MotorLimits {
    /// Continuous current, A.
    pub i_cont_a: Option<f64>,
    /// Burst current, A.
    pub i_burst_a: Option<f64>,
    /// Burst duration, s.
    pub burst_s: Option<f64>,
    /// Maximum electrical input power, W.
    pub p_max_w: Option<f64>,
    /// Maximum shaft speed, rad/s.
    pub omega_max_rad_s: Option<f64>,
    /// Maximum winding temperature, K.
    pub t_winding_max_k: Option<f64>,
}

/// Motor parameters at the reference temperature.
#[derive(Debug, Clone, PartialEq)]
pub struct MotorModel {
    kv_ref_rad_s_per_v: f64,
    rm_ref_ohm: f64,
    i0_ref_a: f64,
    i0_test_v: f64,
    /// Exponent `a` of the no-load torque power law.
    pub i0_exponent: f64,
    i0_table: Option<Table1>,
    /// Copper coefficient, 1/K.
    pub alpha_rm_per_k: f64,
    /// Magnet flux coefficient, 1/K (negative for NdFeB).
    pub beta_flux_per_k: f64,
    /// Temperature at which the constants were quoted, K.
    pub t_ref_k: f64,
    /// Thermal model; `None` keeps the winding at the state temperature.
    pub thermal: Option<MotorThermal>,
    /// Ratings.
    pub limits: MotorLimits,
}

/// Electrical state of one motor at one speed and torque.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MotorState {
    /// Shaft speed, rad/s.
    pub omega_rad_s: f64,
    /// Shaft torque delivered to the propeller, N*m.
    pub torque_n_m: f64,
    /// Terminal current, A.
    pub current_a: f64,
    /// Terminal voltage, V.
    pub voltage_v: f64,
    /// Electrical input power `v i`, W.
    pub p_electric_w: f64,
    /// Shaft power, W.
    pub p_shaft_w: f64,
    /// Winding (copper) loss `i^2 Rm`, W.
    pub p_copper_w: f64,
    /// Iron, friction and windage loss `omega Q0`, W.
    pub p_iron_w: f64,
    /// Motor efficiency `p_shaft / p_electric` (0 when no power flows).
    pub efficiency: f64,
}

impl MotorModel {
    /// Build from `Kv` (rad/s/V), `Rm` (Ohm), `i0` (A) and the voltage `i0` was
    /// measured at (V), all at [`T_REF_K`].  `i0 >= 0`, others `> 0`.
    ///
    /// # Errors
    ///
    /// [`PowerError::InvalidInput`] for a non-finite or non-physical value.
    pub fn new(
        kv_rad_s_per_v: f64,
        rm_ohm: f64,
        i0_a: f64,
        i0_test_v: f64,
    ) -> Result<Self, PowerError> {
        Ok(Self {
            kv_ref_rad_s_per_v: finite_positive(kv_rad_s_per_v, "Kv")?,
            rm_ref_ohm: finite_positive(rm_ohm, "Rm")?,
            i0_ref_a: finite_nonneg(i0_a, "I0")?,
            i0_test_v: finite_positive(i0_test_v, "I0 test voltage")?,
            i0_exponent: 1.0,
            i0_table: None,
            alpha_rm_per_k: ALPHA_COPPER_PER_K,
            beta_flux_per_k: BETA_NDFEB_PER_K,
            t_ref_k: T_REF_K,
            thermal: None,
            limits: MotorLimits::default(),
        })
    }

    /// Replace the power law by a measured `i0(omega)` table (rad/s, A at the
    /// reference temperature); evaluated clamped at its ends.
    ///
    /// # Errors
    ///
    /// [`PowerError::InvalidInput`] for a malformed table.
    pub fn with_i0_table(mut self, omega: Vec<f64>, i0_a: Vec<f64>) -> Result<Self, PowerError> {
        self.i0_table = Some(Table1::new(omega, i0_a, "i0(omega)")?);
        Ok(self)
    }

    /// Reference `Kv`, rad/s/V.
    pub fn kv_ref(&self) -> f64 {
        self.kv_ref_rad_s_per_v
    }

    /// Reference `Rm`, Ohm.
    pub fn rm_ref(&self) -> f64 {
        self.rm_ref_ohm
    }

    fn flux(&self, t_k: f64) -> f64 {
        (1.0 + self.beta_flux_per_k * (t_k - self.t_ref_k)).max(0.05)
    }

    /// Torque and back-EMF constant `Kt(T)`, N*m/A (= V*s/rad).
    pub fn kt(&self, t_k: f64) -> f64 {
        self.flux(t_k) / self.kv_ref_rad_s_per_v
    }

    /// Speed constant `Kv(T) = 1/Kt(T)`, rad/s/V.
    pub fn kv(&self, t_k: f64) -> f64 {
        1.0 / self.kt(t_k)
    }

    /// Terminal resistance at winding temperature `t_k`, Ohm.
    pub fn rm(&self, t_k: f64) -> f64 {
        self.rm_ref_ohm * (1.0 + self.alpha_rm_per_k * (t_k - self.t_ref_k)).max(0.1)
    }

    /// Loss torque `Q0(omega)` at the reference flux, N*m.
    pub fn loss_torque(&self, omega: f64) -> f64 {
        let kt_ref = 1.0 / self.kv_ref_rad_s_per_v;
        let w = omega.abs();
        let i0_ref = match &self.i0_table {
            Some(table) => table.eval_clamped(w),
            None => {
                let w_test = self.kv_ref_rad_s_per_v * self.i0_test_v;
                self.i0_ref_a * (w / w_test).powf(self.i0_exponent)
            }
        };
        kt_ref * i0_ref
    }

    /// No-load current at speed `omega` and winding temperature, A.
    pub fn i0(&self, omega: f64, t_k: f64) -> f64 {
        self.loss_torque(omega) / self.kt(t_k)
    }

    /// Electrical state delivering shaft torque `q` at `omega` and `t_k`.
    pub fn state(&self, omega: f64, q: f64, t_k: f64) -> MotorState {
        let kt = self.kt(t_k);
        let q0 = self.loss_torque(omega);
        let i = (q + q0) / kt;
        let v = i * self.rm(t_k) + omega * kt;
        let p_el = v * i;
        let p_shaft = q * omega;
        MotorState {
            omega_rad_s: omega,
            torque_n_m: q,
            current_a: i,
            voltage_v: v,
            p_electric_w: p_el,
            p_shaft_w: p_shaft,
            p_copper_w: i * i * self.rm(t_k),
            p_iron_w: q0 * omega,
            efficiency: if p_el > 0.0 { p_shaft / p_el } else { 0.0 },
        }
    }

    /// Current of maximum efficiency at terminal voltage `v` (Drela model,
    /// `d eta / d i = 0`): `sqrt(i0 v / Rm)`, evaluated with the reference constants.
    pub fn i_max_efficiency(&self, v: f64) -> f64 {
        (self.i0_ref_a * v / self.rm_ref_ohm).sqrt()
    }

    /// Winding temperature rate `dT/dt`, K/s, for a heat input `p_heat_w`.
    /// Zero without a thermal model.
    pub fn dt_dt(&self, t_k: f64, t_amb_k: f64, p_heat_w: f64) -> f64 {
        match self.thermal {
            Some(th) => (p_heat_w - (t_k - t_amb_k) / th.r_th_k_per_w) / th.c_th_j_per_k,
            None => 0.0,
        }
    }

    /// Steady winding temperature for a constant heat input; `t_amb` without a
    /// thermal model.
    pub fn t_steady(&self, t_amb_k: f64, p_heat_w: f64) -> f64 {
        t_amb_k + self.thermal.map_or(0.0, |th| th.r_th_k_per_w * p_heat_w)
    }

    /// Time for the winding to reach `t_limit_k` from `t0_k` under constant heat
    /// input (first-order solution); `None` if never reached or no thermal model.
    pub fn time_to_temperature(
        &self,
        t0_k: f64,
        t_amb_k: f64,
        p_heat_w: f64,
        t_limit_k: f64,
    ) -> Option<f64> {
        let th = self.thermal?;
        let t_ss = t_amb_k + th.r_th_k_per_w * p_heat_w;
        if t0_k >= t_limit_k {
            return Some(0.0);
        }
        if t_ss <= t_limit_k {
            return None;
        }
        let tau = th.r_th_k_per_w * th.c_th_j_per_k;
        Some(tau * ((t_ss - t0_k) / (t_ss - t_limit_k)).ln())
    }

    /// Exact first-order step of the winding temperature over `dt` seconds.
    pub fn advance_temperature(&self, t_k: f64, t_amb_k: f64, p_heat_w: f64, dt: f64) -> f64 {
        match self.thermal {
            Some(th) => {
                let t_ss = t_amb_k + th.r_th_k_per_w * p_heat_w;
                let tau = th.r_th_k_per_w * th.c_th_j_per_k;
                t_ss + (t_k - t_ss) * (-dt / tau).exp()
            }
            None => t_k,
        }
    }
}
