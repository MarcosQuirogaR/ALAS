// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Steady coordinated level turn (closed form).
//!
//! `n = 1 / cos(phi)`, `R = V^2 / (g tan phi)`, `omega = g tan phi / V`,
//! `CL = n W / (q S)`, `D = q S (CD0 + k CL^2)`. The time-resolved turn flown
//! by the mission integrates the same relations with the speed free to
//! change; this function gives the steady values for planning and for the
//! verification of that integration.

use super::airframe::{Airframe, G0};
use super::PerfError;

/// Steady-turn quantities.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SteadyTurn {
    /// Load factor.
    pub load_factor: f64,
    /// Turn radius, m.
    pub radius_m: f64,
    /// Turn rate, rad/s.
    pub rate_rad_s: f64,
    /// Lift coefficient.
    pub cl: f64,
    /// Drag, N.
    pub drag_n: f64,
    /// `CLmax / CL` (> 1 means margin to stall).
    pub stall_margin: f64,
}

/// Steady turn at true airspeed `v` (m/s) and bank `bank_rad` in `(0, pi/2)`.
pub fn steady_turn(
    af: &Airframe,
    rho: f64,
    v: f64,
    bank_rad: f64,
) -> Result<SteadyTurn, PerfError> {
    if !(v.is_finite() && v > 0.0 && bank_rad.is_finite() && bank_rad > 0.0 && bank_rad < 1.5) {
        return Err(PerfError::InvalidInput(
            "steady turn needs v > 0 and a bank in (0, 1.5) rad".into(),
        ));
    }
    let n = 1.0 / bank_rad.cos();
    if n > af.n_max {
        return Err(PerfError::InvalidInput(format!(
            "load factor {n:.2} exceeds n_max {:.2}",
            af.n_max
        )));
    }
    let cl = af.cl_required(rho, v, n * af.weight(), "steady turn")?;
    Ok(SteadyTurn {
        load_factor: n,
        radius_m: v * v / (G0 * bank_rad.tan()),
        rate_rad_s: G0 * bank_rad.tan() / v,
        cl,
        drag_n: af.drag(rho, v, cl, 1.0, 0.0),
        stall_margin: af.polar.cl_max() / cl,
    })
}
