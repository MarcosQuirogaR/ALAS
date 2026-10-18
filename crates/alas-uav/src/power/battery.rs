// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Battery pack: open-circuit voltage, internal resistance, sag, energy, cutoff.
//!
//! ```text
//! V_term = Voc(SOC) - I_b R_pack(SOC, T)           I_b > 0 discharging
//! R_pack = R0 (1 + k (1 - SOC)) exp(b (1/T - 1/T_ref))     (default form)
//! dSOC/dt = -I_b / Q_cap                           Q_cap in coulomb
//! ```
//!
//! The SOC rise of the resistance follows the plan (k = 0.5-1.0; published
//! studies report 50-100 % between full charge and 15 % SOC).  The default
//! `Voc(SOC)` shape is a GENERIC LiPo placeholder (about 4.20 V/cell full,
//! 3.80 V/cell at 40-50 %, 3.3 V empty), not taken from one datasheet; replace
//! it with the database curve (`voc_vs_soc`) or a discharge log.  The
//! temperature exponent `b` defaults to 0 (no temperature dependence) because
//! no published default has been adopted; set it, or supply measured
//! resistance curves per temperature, to use `IR(SOC, T)`.

use super::interp::Table1;
use super::{finite_nonneg, finite_positive, PowerError};

/// Generic LiPo open-circuit voltage per cell versus SOC (PLACEHOLDER shape).
const DEFAULT_VOC_CELL: [(f64, f64); 12] = [
    (0.00, 3.30),
    (0.05, 3.55),
    (0.10, 3.68),
    (0.20, 3.73),
    (0.30, 3.77),
    (0.40, 3.80),
    (0.50, 3.83),
    (0.60, 3.87),
    (0.70, 3.92),
    (0.80, 3.99),
    (0.90, 4.08),
    (1.00, 4.20),
];

/// Battery pack model.
#[derive(Debug, Clone, PartialEq)]
pub struct BatteryModel {
    capacity_c: f64,
    voc_pack: Table1,
    r0_pack_ohm: f64,
    r_curves: Vec<(f64, Table1)>,
    /// Resistance rise slope `k` over (1 - SOC).
    pub r_soc_slope: f64,
    /// Arrhenius-style exponent `b`, K (0 disables temperature dependence).
    pub r_temp_b_k: f64,
    /// Temperature of the quoted resistance, K.
    pub t_ref_k: f64,
    /// Loaded-voltage cutoff, V (pack).
    pub v_cutoff_v: f64,
    /// Lowest usable state of charge (0..1).
    pub soc_min: f64,
    /// Pack mass, kg (for the lumped heating model).
    pub mass_kg: f64,
    /// Specific heat of the pack, J/(kg K) (ESTIMATE 1000 for LiPo pouch cells).
    pub c_p_j_per_kg_k: f64,
    /// Pack-to-ambient conductance, W/K (0 = adiabatic, the conservative bound).
    pub cooling_w_per_k: f64,
    /// Continuous discharge rating of the pack, A.
    pub i_cont_a: Option<f64>,
}

/// Dynamic state of the pack.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BatteryState {
    /// State of charge, 0..1.
    pub soc: f64,
    /// Pack temperature, K.
    pub t_k: f64,
    /// Charge drawn so far, C.
    pub charge_out_c: f64,
    /// Energy delivered at the terminals so far, J.
    pub energy_out_j: f64,
    /// Energy lost in the internal resistance so far, J.
    pub energy_lost_j: f64,
}

impl BatteryState {
    /// Fresh state at `soc` and `t_k`.
    pub fn new(soc: f64, t_k: f64) -> Self {
        Self {
            soc,
            t_k,
            charge_out_c: 0.0,
            energy_out_j: 0.0,
            energy_lost_j: 0.0,
        }
    }
}

/// Result of one integration step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BatteryStep {
    /// Terminal voltage at the start of the step, V.
    pub v_term_v: f64,
    /// True when the cutoff (voltage or SOC) was reached during the step.
    pub cutoff: bool,
}

impl BatteryModel {
    /// Build from pack capacity (coulomb), series count and the pack DC
    /// resistance quoted at full charge and `t_ref` (Ohm).  Uses the generic
    /// per-cell `Voc` placeholder and a 3.0 V/cell loaded cutoff.
    ///
    /// # Errors
    ///
    /// [`PowerError::InvalidInput`] for a non-physical value.
    pub fn new(capacity_c: f64, cells_series: u32, r0_pack_ohm: f64) -> Result<Self, PowerError> {
        finite_positive(capacity_c, "battery capacity")?;
        finite_positive(r0_pack_ohm, "battery resistance")?;
        if cells_series == 0 {
            return Err(PowerError::InvalidInput(
                "battery needs >= 1 series cell".into(),
            ));
        }
        let n = f64::from(cells_series);
        let x = DEFAULT_VOC_CELL.iter().map(|p| p.0).collect();
        let y = DEFAULT_VOC_CELL.iter().map(|p| p.1 * n).collect();
        Ok(Self {
            capacity_c,
            voc_pack: Table1::new(x, y, "default Voc")?,
            r0_pack_ohm,
            r_curves: Vec::new(),
            r_soc_slope: 0.75,
            r_temp_b_k: 0.0,
            t_ref_k: 298.15,
            v_cutoff_v: 3.0 * n,
            soc_min: 0.0,
            mass_kg: 0.0,
            c_p_j_per_kg_k: 1000.0,
            cooling_w_per_k: 0.0,
            i_cont_a: None,
        })
    }

    /// Replace `Voc(SOC)` by a pack-voltage table (SOC 0..1, volts).
    ///
    /// # Errors
    ///
    /// [`PowerError::InvalidInput`] for a malformed table.
    pub fn with_voc_table(mut self, soc: Vec<f64>, v_pack: Vec<f64>) -> Result<Self, PowerError> {
        self.voc_pack = Table1::new(soc, v_pack, "Voc(SOC)")?;
        Ok(self)
    }

    /// Add one measured resistance curve (pack Ohm versus SOC) at `t_k`.  With
    /// curves present, `R` is interpolated in SOC then in T (clamped) instead
    /// of using the slope form.
    ///
    /// # Errors
    ///
    /// [`PowerError::InvalidInput`] for a malformed table or temperature.
    pub fn with_r_curve(
        mut self,
        t_k: f64,
        soc: Vec<f64>,
        r_ohm: Vec<f64>,
    ) -> Result<Self, PowerError> {
        finite_positive(t_k, "resistance curve temperature")?;
        self.r_curves
            .push((t_k, Table1::new(soc, r_ohm, "R(SOC)")?));
        self.r_curves.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(self)
    }

    /// Pack capacity, coulomb.
    pub fn capacity_c(&self) -> f64 {
        self.capacity_c
    }

    /// Open-circuit pack voltage, V.
    pub fn voc(&self, soc: f64) -> f64 {
        self.voc_pack.eval_clamped(soc)
    }

    /// Pack internal resistance, Ohm.
    pub fn r_pack(&self, soc: f64, t_k: f64) -> f64 {
        let soc = soc.clamp(0.0, 1.0);
        if !self.r_curves.is_empty() {
            let at = |c: &(f64, Table1)| c.1.eval_clamped(soc);
            let first = &self.r_curves[0];
            let last = &self.r_curves[self.r_curves.len() - 1];
            if t_k <= first.0 {
                return at(first);
            }
            if t_k >= last.0 {
                return at(last);
            }
            let hi = self.r_curves.partition_point(|c| c.0 <= t_k);
            let (a, b) = (&self.r_curves[hi - 1], &self.r_curves[hi]);
            let f = (t_k - a.0) / (b.0 - a.0);
            return at(a) + f * (at(b) - at(a));
        }
        let thermal = (self.r_temp_b_k * (1.0 / t_k - 1.0 / self.t_ref_k)).exp();
        self.r0_pack_ohm * (1.0 + self.r_soc_slope * (1.0 - soc)) * thermal
    }

    /// Terminal voltage under discharge current `i_b` (A), V.
    pub fn v_terminal(&self, soc: f64, t_k: f64, i_b: f64) -> f64 {
        self.voc(soc) - i_b * self.r_pack(soc, t_k)
    }

    /// Whether the loaded state is at or beyond the cutoff.
    pub fn is_cutoff(&self, state: &BatteryState, i_b: f64) -> bool {
        state.soc <= self.soc_min || self.v_terminal(state.soc, state.t_k, i_b) < self.v_cutoff_v
    }

    /// Integrate charge, energy and pack heating over `dt` at constant `i_b`
    /// and `t_amb_k`.  SOC is clamped at 0.
    pub fn step(&self, state: &mut BatteryState, i_b: f64, t_amb_k: f64, dt: f64) -> BatteryStep {
        let r = self.r_pack(state.soc, state.t_k);
        let v = self.voc(state.soc) - i_b * r;
        let dq = i_b * dt;
        state.charge_out_c += dq;
        state.energy_out_j += v * i_b * dt;
        state.energy_lost_j += i_b * i_b * r * dt;
        state.soc = (state.soc - dq / self.capacity_c).max(0.0);
        let heat_cap = self.mass_kg * self.c_p_j_per_kg_k;
        if heat_cap > 0.0 {
            let p = i_b * i_b * r - self.cooling_w_per_k * (state.t_k - t_amb_k);
            state.t_k += p * dt / heat_cap;
        }
        BatteryStep {
            v_term_v: v,
            cutoff: state.soc <= self.soc_min || v < self.v_cutoff_v,
        }
    }

    /// Check an input fraction (helper for callers building a start state).
    ///
    /// # Errors
    ///
    /// [`PowerError::InvalidInput`] when `soc` is outside 0..=1.
    pub fn start_state(&self, soc: f64, t_k: f64) -> Result<BatteryState, PowerError> {
        finite_nonneg(soc, "SOC")?;
        if soc > 1.0 {
            return Err(PowerError::InvalidInput(format!(
                "SOC must be <= 1, got {soc}"
            )));
        }
        finite_positive(t_k, "battery temperature")?;
        Ok(BatteryState::new(soc, t_k))
    }
}
