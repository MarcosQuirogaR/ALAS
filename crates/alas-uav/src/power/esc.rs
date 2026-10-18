// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Electronic speed controller: duty-cycle limit, conduction and switching loss.
//!
//! One ESC per motor.  With duty `D` (0..`d_max`), pack voltage `V` at the ESC
//! input and motor current `i` (A):
//!
//! ```text
//! v_motor = D V - i r_on                       conduction drop
//! i_in    = (D + k_sw) i + i_idle              battery-side current
//! loss    = k_sw V i + i^2 r_on + V i_idle     switching + conduction + idle
//! ```
//!
//! so `V i_in = v_motor i + loss` holds exactly.  Battery current is below the
//! motor current at part throttle (`i_in ~ D i`), which is the mechanism the
//! plan names.  The current limiter is applied by the solver, which lowers `D`
//! until the battery current is under the cap.  Defaults are NOT database
//! values: the nominal efficiency (0.96 full throttle) is an ESTIMATE in the
//! 0.95-0.97 band of the plan, idle current zero.

use super::{finite_nonneg, finite_positive, PowerError};

/// Default nominal ESC efficiency at full duty (ESTIMATE, plan band 0.95-0.97).
pub const DEFAULT_ETA_NOMINAL: f64 = 0.96;

/// ESC loss model and ratings.
#[derive(Debug, Clone, PartialEq)]
pub struct EscModel {
    /// Largest commandable duty (0..=1).
    pub d_max: f64,
    /// Effective on-resistance of the conduction path (FETs, wiring), Ohm.
    pub r_on_ohm: f64,
    /// Switching loss as a fraction of `V i`.
    pub k_sw: f64,
    /// Quiescent current from the pack, A.
    pub i_idle_a: f64,
    /// Continuous output current rating, A.
    pub i_cont_a: Option<f64>,
    /// Burst output current rating, A.
    pub i_burst_a: Option<f64>,
}

impl EscModel {
    /// A lossless ESC (`D` scales the voltage exactly, `i_in = D i`).
    pub fn ideal() -> Self {
        Self {
            d_max: 1.0,
            r_on_ohm: 0.0,
            k_sw: 0.0,
            i_idle_a: 0.0,
            i_cont_a: None,
            i_burst_a: None,
        }
    }

    /// ESC with efficiency `eta` at full duty, voltage `v_ref` and current
    /// `i_ref`, splitting the loss equally between conduction and switching:
    /// `x = i r_on / V = k_sw = (1 - eta)/(1 + eta)` gives exactly
    /// `eta = (1 - x)/(1 + x)`.
    ///
    /// # Errors
    ///
    /// [`PowerError::InvalidInput`] unless `0 < eta <= 1` and the references are positive.
    pub fn from_nominal_efficiency(eta: f64, v_ref: f64, i_ref: f64) -> Result<Self, PowerError> {
        if !(eta.is_finite() && eta > 0.0 && eta <= 1.0) {
            return Err(PowerError::InvalidInput(format!(
                "ESC efficiency must be in (0, 1], got {eta}"
            )));
        }
        finite_positive(v_ref, "ESC reference voltage")?;
        finite_positive(i_ref, "ESC reference current")?;
        let x = (1.0 - eta) / (1.0 + eta);
        Ok(Self {
            r_on_ohm: x * v_ref / i_ref,
            k_sw: x,
            ..Self::ideal()
        })
    }

    /// Validate the parameters.
    ///
    /// # Errors
    ///
    /// [`PowerError::InvalidInput`] for a non-physical value.
    pub fn validate(&self) -> Result<(), PowerError> {
        finite_nonneg(self.r_on_ohm, "ESC r_on")?;
        finite_nonneg(self.k_sw, "ESC k_sw")?;
        finite_nonneg(self.i_idle_a, "ESC idle current")?;
        if !(self.d_max.is_finite() && self.d_max > 0.0 && self.d_max <= 1.0) {
            return Err(PowerError::InvalidInput(format!(
                "ESC d_max must be in (0, 1], got {}",
                self.d_max
            )));
        }
        Ok(())
    }

    /// Voltage at the motor terminals, V.
    pub fn v_motor(&self, duty: f64, v_in: f64, i_motor: f64) -> f64 {
        duty * v_in - i_motor * self.r_on_ohm
    }

    /// Battery-side current of this ESC, A.
    pub fn i_input(&self, duty: f64, i_motor: f64) -> f64 {
        (duty + self.k_sw) * i_motor + self.i_idle_a
    }

    /// Total ESC loss, W.
    pub fn loss_w(&self, v_in: f64, i_motor: f64) -> f64 {
        self.k_sw * v_in * i_motor + i_motor * i_motor * self.r_on_ohm + v_in * self.i_idle_a
    }

    /// Efficiency `v_motor i / (V i_in)` (0 when no power flows).
    pub fn efficiency(&self, duty: f64, v_in: f64, i_motor: f64) -> f64 {
        let p_in = v_in * self.i_input(duty, i_motor);
        if p_in > 0.0 {
            self.v_motor(duty, v_in, i_motor) * i_motor / p_in
        } else {
            0.0
        }
    }
}
