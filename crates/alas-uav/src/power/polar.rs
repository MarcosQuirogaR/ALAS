// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Blade-section polars for BEMT at low Reynolds number.
//!
//! A [`SectionPolar`] returns `(cl, cd)` for any angle of attack (radians,
//! wrapped to +-pi) and chord Reynolds number.  Two implementations:
//!
//! * [`AnalyticPolar`]: linear lift saturating smoothly at `cl_max`, a
//!   Reynolds-dependent profile drag, then the flat-plate post-stall law
//!   `cl = (cd90/2) sin 2a`, `cd = cd90 sin^2 a` (Hoerner flat-plate normal
//!   force, `cd90 ~ 1.98` for a plate of infinite span; 1.8 used for the
//!   finite-span blade).  It is a placeholder when no airfoil data exist; its
//!   drag is an ESTIMATE (laminar skin friction `2.66/sqrt(Re)` times an
//!   assumed 1.5 form/bubble factor).
//! * [`TabulatedPolar`]: tables over alpha and Re, normally filled from
//!   NeuralFoil (Re 5e4-2e5 per the plan) by [`TabulatedPolar::from_neuralfoil`],
//!   blended to the flat-plate law beyond the tabulated alpha range.

use alas_aero::neuralfoil::{Conditions, ModelSize, PreparedAirfoil};
use alas_geom::aircraft::airfoil::Airfoil;

use super::PowerError;

/// Normal-force coefficient of a finite flat plate used after stall.
pub const CD90: f64 = 1.8;

/// Section lift and drag as functions of angle of attack and Reynolds number.
pub trait SectionPolar: Send + Sync + std::fmt::Debug {
    /// `(cl, cd)` at angle of attack `alpha_rad` and chord Reynolds `re`.
    fn cl_cd(&self, alpha_rad: f64, re: f64) -> (f64, f64);
}

fn wrap(alpha: f64) -> f64 {
    let two_pi = std::f64::consts::TAU;
    let a = (alpha + std::f64::consts::PI).rem_euclid(two_pi) - std::f64::consts::PI;
    if a.is_finite() {
        a
    } else {
        0.0
    }
}

fn plate(alpha: f64, cd_min: f64) -> (f64, f64) {
    let (s, c) = alpha.sin_cos();
    (CD90 * s * c, CD90 * s * s + cd_min)
}

fn smoothstep(x: f64) -> f64 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Linear-lift section with smooth stall and flat-plate post-stall.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnalyticPolar {
    /// Zero-lift angle, rad (negative for camber).
    pub alpha0_rad: f64,
    /// Lift slope, 1/rad (below 2 pi at low Re).
    pub cl_alpha: f64,
    /// Maximum lift coefficient (symmetric saturation).
    pub cl_max: f64,
    /// Angle from `alpha0` where the plate blend starts, rad.
    pub stall_rad: f64,
    /// Quadratic drag-polar curvature about `cl = 0.3`.
    pub cd_k: f64,
}

impl Default for AnalyticPolar {
    /// A generic thin cambered section at Re ~ 1e5 (ESTIMATE, uncalibrated).
    fn default() -> Self {
        Self {
            alpha0_rad: (-3.0_f64).to_radians(),
            cl_alpha: 5.5,
            cl_max: 1.15,
            stall_rad: 12.0_f64.to_radians(),
            cd_k: 0.02,
        }
    }
}

impl AnalyticPolar {
    /// Profile drag floor at Reynolds `re` (ESTIMATE, see module docs).
    pub fn cd_min(re: f64) -> f64 {
        (2.66 / re.max(1.0e4).sqrt() * 1.5).max(0.010)
    }
}

impl SectionPolar for AnalyticPolar {
    fn cl_cd(&self, alpha_rad: f64, re: f64) -> (f64, f64) {
        let a = wrap(alpha_rad);
        let cd_min = Self::cd_min(re);
        let lin = self.cl_alpha * (a - self.alpha0_rad);
        let cl_att = self.cl_max * (lin / self.cl_max).tanh();
        let cd_att = cd_min + self.cd_k * (cl_att - 0.3).powi(2);
        let s = (a - self.alpha0_rad).abs();
        let w = smoothstep((s - self.stall_rad) / (8.0_f64.to_radians()));
        let (clp, cdp) = plate(a, cd_min);
        ((1.0 - w) * cl_att + w * clp, (1.0 - w) * cd_att + w * cdp)
    }
}

/// Polar tabulated over `alpha` (degrees internally stored as radians) and Re.
#[derive(Debug, Clone, PartialEq)]
pub struct TabulatedPolar {
    alpha: Vec<f64>,
    re: Vec<f64>,
    cl: Vec<Vec<f64>>,
    cd: Vec<Vec<f64>>,
}

impl TabulatedPolar {
    /// Build from `cl[i_re][i_alpha]` and `cd[i_re][i_alpha]` tables.
    ///
    /// # Errors
    ///
    /// [`PowerError::InvalidInput`] for ragged, non-finite or unordered tables.
    pub fn new(
        alpha_rad: Vec<f64>,
        re: Vec<f64>,
        cl: Vec<Vec<f64>>,
        cd: Vec<Vec<f64>>,
    ) -> Result<Self, PowerError> {
        let ok = alpha_rad.len() >= 2
            && !re.is_empty()
            && alpha_rad.windows(2).all(|w| w[1] > w[0])
            && re.windows(2).all(|w| w[1] > w[0])
            && re.iter().all(|&r| r > 0.0)
            && cl.len() == re.len()
            && cd.len() == re.len()
            && cl
                .iter()
                .chain(&cd)
                .all(|row| row.len() == alpha_rad.len() && row.iter().all(|v| v.is_finite()));
        if ok {
            Ok(Self {
                alpha: alpha_rad,
                re,
                cl,
                cd,
            })
        } else {
            Err(PowerError::InvalidInput(
                "polar tables are ragged, non-finite or unordered".into(),
            ))
        }
    }

    /// Tabulate NeuralFoil (incompressible, `n_crit = 9`) for `airfoil` at each
    /// Reynolds number over `alpha_deg` (inclusive range, 1 degree step).
    ///
    /// # Errors
    ///
    /// [`PowerError::InvalidInput`] if the airfoil cannot be fitted or evaluated.
    pub fn from_neuralfoil(
        airfoil: &Airfoil,
        reynolds: &[f64],
        alpha_deg: (i32, i32),
    ) -> Result<Self, PowerError> {
        let prepared = PreparedAirfoil::prepare(airfoil)
            .map_err(|e| PowerError::InvalidInput(format!("NeuralFoil airfoil fit: {e}")))?;
        let alphas: Vec<f64> = (alpha_deg.0..=alpha_deg.1).map(f64::from).collect();
        let (mut cl, mut cd) = (Vec::new(), Vec::new());
        for &re in reynolds {
            let conditions: Vec<Conditions> =
                alphas.iter().map(|&a| Conditions::new(a, re)).collect();
            let out = prepared
                .aero_sweep(&conditions, 0.0, ModelSize::Large)
                .map_err(|e| PowerError::InvalidInput(format!("NeuralFoil evaluation: {e}")))?;
            cl.push(out.iter().map(|a| a.cl).collect());
            cd.push(out.iter().map(|a| a.cd).collect());
        }
        Self::new(
            alphas.iter().map(|a| a.to_radians()).collect(),
            reynolds.to_vec(),
            cl,
            cd,
        )
    }

    fn at(&self, table: &[Vec<f64>], ia: usize, fa: f64, re: f64) -> f64 {
        let value = |row: &Vec<f64>| row[ia] + fa * (row[ia + 1] - row[ia]);
        let n = self.re.len();
        if n == 1 || re <= self.re[0] {
            return value(&table[0]);
        }
        if re >= self.re[n - 1] {
            return value(&table[n - 1]);
        }
        let hi = self.re.partition_point(|&r| r <= re);
        let f = (re.ln() - self.re[hi - 1].ln()) / (self.re[hi].ln() - self.re[hi - 1].ln());
        let (a, b) = (value(&table[hi - 1]), value(&table[hi]));
        a + f * (b - a)
    }

    fn inside(&self, alpha: f64, re: f64) -> (f64, f64) {
        let n = self.alpha.len();
        let ia = self
            .alpha
            .partition_point(|&v| v <= alpha)
            .saturating_sub(1)
            .min(n - 2);
        let fa = (alpha - self.alpha[ia]) / (self.alpha[ia + 1] - self.alpha[ia]);
        (self.at(&self.cl, ia, fa, re), self.at(&self.cd, ia, fa, re))
    }
}

impl SectionPolar for TabulatedPolar {
    fn cl_cd(&self, alpha_rad: f64, re: f64) -> (f64, f64) {
        let a = wrap(alpha_rad);
        let (lo, hi) = (self.alpha[0], self.alpha[self.alpha.len() - 1]);
        let edge = a.clamp(lo, hi);
        let (cl_e, cd_e) = self.inside(edge, re);
        if a == edge {
            return (cl_e, cd_e);
        }
        // Beyond the table: blend from the edge value to the plate law over 8 deg.
        let (clp, cdp) = plate(a, cd_e.min(0.05));
        let w = smoothstep((a - edge).abs() / 8.0_f64.to_radians());
        ((1.0 - w) * cl_e + w * clp, (1.0 - w) * cd_e + w * cdp)
    }
}

/// Thin cambered section of the generic blade: NACA 4408 (4 % camber at 40 %
/// chord, 8 % thick).  APC thin-electric blades are thin and cambered; this
/// is a structural choice made from the blade type, before any aircraft was
/// predicted.  The alternatives surveyed by M2 lane A (static CT / CP at 6000
/// rpm for the generic 10x5) were NACA 4408 0.095 / 0.035, NACA 4412
/// 0.089 / 0.035, NACA 2410 0.083 / 0.029 and symmetric NACA 0010 0.067,
/// against 0.073 / 0.024 for the analytic placeholder.
pub const GENERIC_BLADE_SECTION: &str = "naca4408";

/// Reynolds numbers of the generic-blade NeuralFoil table (the network is
/// reliable from about 8e4; below that the aero validity guard warns and the
/// propeller model raises [`super::LimitFlag::PropLowRe`]).
pub const GENERIC_BLADE_RE: [f64; 5] = [2.0e4, 4.0e4, 8.0e4, 1.5e5, 3.0e5];

/// Blade Reynolds number (at 0.75 R) below which NeuralFoil section polars
/// are in their warning band (same limit as `aero::validity::Limits`).
pub const LOW_RE_WARN: f64 = 8.0e4;

/// NeuralFoil table of [`GENERIC_BLADE_SECTION`] (`n_crit = 9`, alpha -10 to
/// 22 deg), built once per process.
///
/// # Errors
///
/// [`PowerError::InvalidInput`] if the section cannot be fitted or evaluated.
pub fn generic_cambered_polar() -> Result<std::sync::Arc<TabulatedPolar>, PowerError> {
    use std::sync::{Arc, Mutex};
    static CACHE: Mutex<Option<Arc<TabulatedPolar>>> = Mutex::new(None);
    let mut slot = CACHE
        .lock()
        .map_err(|_| PowerError::InvalidInput("generic polar cache poisoned".into()))?;
    if let Some(p) = slot.as_ref() {
        return Ok(Arc::clone(p));
    }
    let airfoil = Airfoil::from_name(GENERIC_BLADE_SECTION).ok_or_else(|| {
        PowerError::InvalidInput(format!("generic blade section {GENERIC_BLADE_SECTION}"))
    })?;
    let polar = Arc::new(TabulatedPolar::from_neuralfoil(
        &airfoil,
        &GENERIC_BLADE_RE,
        (-10, 22),
    )?);
    *slot = Some(Arc::clone(&polar));
    Ok(polar)
}
