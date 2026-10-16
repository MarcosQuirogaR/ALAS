// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tabulate a BEMT propeller into a coefficient map.
//!
//! A BEMT evaluation costs about a millisecond (30 annuli, root bracketing),
//! and a mission integration asks the operating-point solver for thousands of
//! equilibria, each of which scans the propeller dozens of times.  Sampling
//! the BEMT once on a `(omega, J)` grid at the mission density and viscosity
//! turns every later lookup into two bilinear interpolations.  The grid is
//! 24 shaft speeds from 30 rad/s to the tip-Mach ceiling and, per speed, 48
//! advance ratios from 0 to the zero-thrust advance ratio (found by marching
//! in steps of 0.05, at most `J = 2.5`).  The interpolation error against the
//! BEMT is reported by [`TabulatedProp::max_ct_error`].
//!
//! The Reynolds correction of the BEMT is frozen at the sampling density
//! (`rho`, `mu`); [`TabulatedProp::low_re_fraction`] records how much of the
//! grid sat in the NeuralFoil warning band so the caller can raise the
//! low-Re warning that a map no longer carries.

use super::prop::PropModel;
use super::prop_map::PropMapTable;
use super::PowerError;

const N_OMEGA: usize = 24;
const N_J: usize = 48;
const J_CAP: f64 = 2.5;
const OMEGA_MIN: f64 = 30.0;

/// A BEMT propeller sampled into a map.
#[derive(Debug, Clone)]
pub struct TabulatedProp {
    /// The map as a propeller model (source `ComputedMap`).
    pub model: PropModel,
    /// Fraction of grid points whose 0.75 R blade Reynolds number was below
    /// the NeuralFoil warning limit.
    pub low_re_fraction: f64,
    /// Largest relative thrust-coefficient deviation of the map from the BEMT
    /// at off-grid points where `CT` exceeds a tenth of its maximum at that
    /// speed.
    pub max_ct_error: f64,
}

fn omega_at(ceiling: f64, k: usize) -> f64 {
    OMEGA_MIN + (ceiling - OMEGA_MIN) * k as f64 / (N_OMEGA - 1) as f64
}

/// Sample `bemt` (a BEMT [`PropModel`]) at density `rho` and viscosity `mu`.
///
/// # Errors
///
/// [`PowerError::InvalidInput`] when `bemt` is not a BEMT model or the density
/// or viscosity is not positive; [`PowerError::OutOfRange`] when the BEMT
/// fails on the grid.
pub fn tabulate_bemt(bemt: &PropModel, rho: f64, mu: f64) -> Result<TabulatedProp, PowerError> {
    let ceiling = bemt
        .bemt_omega_ceiling()
        .ok_or_else(|| PowerError::InvalidInput("only a BEMT propeller can be tabulated".into()))?;
    if !(rho > 0.0 && mu > 0.0) {
        return Err(PowerError::InvalidInput("rho and mu must be > 0".into()));
    }
    let d = bemt.diameter_m();
    let (mut low, mut total) = (0usize, 0usize);
    let mut curves = Vec::with_capacity(N_OMEGA);
    for k in 0..N_OMEGA {
        let omega = omega_at(ceiling, k);
        let n = omega / std::f64::consts::TAU;
        let at = |j: f64| bemt.loads(omega, j * n * d, rho, mu);
        let mut j_top = J_CAP;
        let mut j = 0.0;
        while j <= J_CAP {
            match at(j) {
                Ok(l) if l.ct > 0.0 => j += 0.05,
                _ => {
                    j_top = (j - 0.025).max(0.05);
                    break;
                }
            }
        }
        let mut rows = Vec::with_capacity(N_J + 1);
        for i in 0..=N_J {
            let j = j_top * i as f64 / N_J as f64;
            let l = at(j)?;
            total += 1;
            low += usize::from(l.low_re);
            rows.push((j, l.ct, l.cp));
        }
        curves.push((omega, rows));
    }
    let table = PropMapTable::from_curves(d, curves)?;
    let mut worst = 0.0_f64;
    for k in [3usize, 9, 15, 20] {
        let omega = 0.5 * (omega_at(ceiling, k) + omega_at(ceiling, k + 1));
        let n = omega / std::f64::consts::TAU;
        let mut pairs = Vec::new();
        let mut ct_max = 0.0_f64;
        for i in 0..8 {
            let j = 0.037 + 0.121 * i as f64;
            if let (Ok(l), Some((ct, _))) = (
                bemt.loads(omega, j * n * d, rho, mu),
                table.coefficients(omega, j),
            ) {
                ct_max = ct_max.max(l.ct);
                pairs.push((l.ct, ct));
            }
        }
        for (exact, map) in pairs {
            if exact > 0.1 * ct_max {
                worst = worst.max(((map - exact) / exact).abs());
            }
        }
    }
    Ok(TabulatedProp {
        model: PropModel::from_map(table, false),
        low_re_fraction: low as f64 / total.max(1) as f64,
        max_ct_error: worst,
    })
}
