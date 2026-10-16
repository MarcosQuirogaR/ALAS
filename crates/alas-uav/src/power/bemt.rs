// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Blade-element / momentum propeller model with Prandtl tip and hub loss.
//!
//! Per annulus at radius `r` (axial thrust forward, `V` airspeed along the
//! shaft, `omega` rotation, `B` blades, chord `c`, pitch angle `theta` between
//! the chord line and the rotation plane, `alpha = theta - phi`):
//!
//! ```text
//! W sin(phi) = V + u_a          W cos(phi) = omega r - u_t
//! Cn = cl cos(phi) - cd sin(phi)        Ct = cl sin(phi) + cd cos(phi)
//! dT/dr = B (1/2) rho W^2 c Cn          dQ/dr = B (1/2) rho W^2 c Ct r
//! momentum: dT/dr = 4 pi r rho (V + u_a) u_a F
//!           dQ/dr = 4 pi r^2 rho (V + u_a) u_t F      (Adkins & Liebeck 1994)
//! ```
//!
//! Eliminating `W`, `u_a`, `u_t` leaves ONE residual in `phi` (the guaranteed
//! bracket idea of Ning, Wind Energy 2021, written in induced velocities so the
//! static case `V = 0` is regular):
//!
//! ```text
//! R(phi) = omega r (sin^2 phi - k Cn) - V (sin phi cos phi + k Ct),  k = B c / (8 pi r F)
//! ```
//!
//! `F = F_tip F_hub`, `F_tip = (2/pi) acos(exp(-B (R - r) / (2 r sin phi)))`.
//! The root nearest pi/2 (lightest induction) is taken.  Validity: positive
//! thrust (no windmill-brake or vortex-ring branch), incompressible blade
//! sections (tip Mach below about 0.6), chord Reynolds from the polar range.

use std::sync::Arc;

use super::interp::brent;
use super::polar::{AnalyticPolar, SectionPolar};
use super::{finite_positive, PowerError};

/// Blade geometry and section polars.
#[derive(Debug, Clone)]
pub struct BemtGeometry {
    /// Number of blades.
    pub blades: u32,
    /// Station radii, m (strictly increasing; first = hub cut-out, last = tip).
    r_m: Vec<f64>,
    chord_m: Vec<f64>,
    theta_rad: Vec<f64>,
    polars: Vec<Arc<dyn SectionPolar>>,
    /// Annuli used for the radial integration.
    pub n_annuli: usize,
    /// True when the geometry was generated from diameter and pitch only.
    pub generic: bool,
}

/// Integrated BEMT loads of one propeller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BemtSolution {
    /// Thrust, N.
    pub thrust_n: f64,
    /// Shaft torque absorbed, N*m.
    pub torque_n_m: f64,
    /// Fraction of annuli with no momentum root (fell back to zero induction).
    pub fallback_fraction: f64,
    /// Section Reynolds number at 0.75 R from the unperturbed relative speed.
    pub re_75: f64,
}

/// Chord distribution shape (r/R, c/c_max) of the generic blade.
const GENERIC_CHORD: [(f64, f64); 8] = [
    (0.15, 0.55),
    (0.25, 0.80),
    (0.40, 0.95),
    (0.55, 1.00),
    (0.70, 0.92),
    (0.85, 0.72),
    (0.95, 0.45),
    (1.00, 0.20),
];

impl BemtGeometry {
    /// Build from station arrays: radius m, chord m, pitch angle rad.
    ///
    /// # Errors
    ///
    /// [`PowerError::InvalidInput`] for short, ragged or non-physical arrays.
    pub fn new(
        blades: u32,
        r_m: Vec<f64>,
        chord_m: Vec<f64>,
        theta_rad: Vec<f64>,
        polar: Arc<dyn SectionPolar>,
    ) -> Result<Self, PowerError> {
        let n = r_m.len();
        let ok = blades >= 1
            && n >= 2
            && chord_m.len() == n
            && theta_rad.len() == n
            && r_m
                .iter()
                .chain(&chord_m)
                .all(|&v| v.is_finite() && v > 0.0)
            && theta_rad.iter().all(|v| v.is_finite())
            && r_m.windows(2).all(|w| w[1] > w[0]);
        if !ok {
            return Err(PowerError::InvalidInput(
                "BEMT geometry needs >= 2 stations, equal lengths, increasing radius".into(),
            ));
        }
        Ok(Self {
            blades,
            r_m,
            chord_m,
            theta_rad,
            polars: vec![polar; n],
            n_annuli: 30,
            generic: false,
        })
    }

    /// Replace the polar of station `i` (for example a NeuralFoil table).
    pub fn set_station_polar(&mut self, i: usize, polar: Arc<dyn SectionPolar>) {
        if let Some(slot) = self.polars.get_mut(i) {
            *slot = polar;
        }
    }

    /// Generic blade from diameter and geometric pitch only: APC-like chord
    /// shape with `c_max = 0.09 D` (ESTIMATE) and `theta = atan(P / (2 pi r))`.
    /// The sections use the NeuralFoil table of a thin cambered airfoil
    /// ([`super::polar::generic_cambered_polar`]); blade Reynolds numbers are
    /// 3e4-1e5, so [`BemtSolution::re_75`] below `LOW_RE_WARN` raises the
    /// low-Re warning of the operating point.  Flagged `generic`; not a
    /// substitute for measured geometry.
    ///
    /// Verified against UIUC wind-tunnel measurements of eight APC thin-electric
    /// propellers (9-16 in, rpm 5000-7000; `tests/power_uiuc_verify.rs`): rms
    /// CT and CP error 2-17 % of the static value (mean 7.5 % / 8.9 %), static
    /// CT within -9..+4 % and static CP within -8..+16 %, efficiency bias -6 %
    /// (J/J0 < 0.6) to 0 % (0.6-0.85).  The zero-thrust advance ratio J0 is 2-8 %
    /// LOW, so thrust at J/J0 > 0.85 is under-predicted, not over-predicted, and
    /// the error amplification there is large (dCT/CT of about -6 per unit
    /// dJ/J near J0).  NOT verified for other blade families (Aero-naut CAMcarbon,
    /// folding props) or for rpm above 7000.
    ///
    /// # Errors
    ///
    /// [`PowerError::InvalidInput`] for non-physical dimensions or when the
    /// NeuralFoil table cannot be built.
    pub fn generic(
        diameter_m: f64,
        pitch_m: f64,
        blades: u32,
        hub_ratio: f64,
    ) -> Result<Self, PowerError> {
        let polar: Arc<dyn SectionPolar> = super::polar::generic_cambered_polar()?;
        Self::generic_with_polar(diameter_m, pitch_m, blades, hub_ratio, polar)
    }

    /// As [`Self::generic`] with the analytic placeholder polar (linear lift,
    /// flat-plate post-stall; static CT 0.073 for the 10x5): kept for
    /// comparisons and as the documented pre-NeuralFoil baseline.
    ///
    /// # Errors
    ///
    /// [`PowerError::InvalidInput`] for non-physical dimensions.
    pub fn generic_analytic(
        diameter_m: f64,
        pitch_m: f64,
        blades: u32,
        hub_ratio: f64,
    ) -> Result<Self, PowerError> {
        Self::generic_with_polar(
            diameter_m,
            pitch_m,
            blades,
            hub_ratio,
            Arc::new(AnalyticPolar::default()),
        )
    }

    fn generic_with_polar(
        diameter_m: f64,
        pitch_m: f64,
        blades: u32,
        hub_ratio: f64,
        polar: Arc<dyn SectionPolar>,
    ) -> Result<Self, PowerError> {
        finite_positive(diameter_m, "propeller diameter")?;
        finite_positive(pitch_m, "propeller pitch")?;
        if !(0.05..0.5).contains(&hub_ratio) {
            return Err(PowerError::InvalidInput(
                "hub ratio must be in [0.05, 0.5)".into(),
            ));
        }
        let radius = 0.5 * diameter_m;
        let c_max = 0.09 * diameter_m;
        let (mut r, mut c, mut th) = (Vec::new(), Vec::new(), Vec::new());
        for k in 0..=11 {
            let x = hub_ratio + (0.99 - hub_ratio) * f64::from(k) / 11.0;
            let shape = shape_at(x);
            r.push(x * radius);
            c.push(c_max * shape);
            th.push((pitch_m / (std::f64::consts::TAU * x * radius)).atan());
        }
        let mut g = Self::new(blades, r, c, th, polar)?;
        g.generic = true;
        Ok(g)
    }

    /// Tip radius, m.
    pub fn radius_m(&self) -> f64 {
        self.r_m[self.r_m.len() - 1]
    }

    /// Hub cut-out radius, m.
    pub fn hub_radius_m(&self) -> f64 {
        self.r_m[0]
    }

    fn interp(&self, r: f64) -> (f64, f64, usize) {
        let n = self.r_m.len();
        let i = self
            .r_m
            .partition_point(|&v| v <= r)
            .saturating_sub(1)
            .min(n - 2);
        let t = ((r - self.r_m[i]) / (self.r_m[i + 1] - self.r_m[i])).clamp(0.0, 1.0);
        let lerp = |a: &[f64]| a[i] + t * (a[i + 1] - a[i]);
        (
            lerp(&self.chord_m),
            lerp(&self.theta_rad),
            if t < 0.5 { i } else { i + 1 },
        )
    }

    /// Integrate loads at shaft speed `omega` (rad/s, > 0), airspeed `v` (m/s,
    /// >= 0), density `rho` (kg/m3) and dynamic viscosity `mu` (Pa*s).
    pub fn solve(&self, omega: f64, v: f64, rho: f64, mu: f64) -> Option<BemtSolution> {
        if !(omega.is_finite() && omega > 0.0 && v.is_finite() && v >= 0.0 && rho > 0.0 && mu > 0.0)
        {
            return None;
        }
        let (r_hub, r_tip) = (self.hub_radius_m(), self.radius_m());
        let dr = (r_tip - r_hub) / self.n_annuli as f64;
        let (mut thrust, mut torque, mut fallback) = (0.0, 0.0, 0usize);
        for k in 0..self.n_annuli {
            let r = r_hub + (k as f64 + 0.5) * dr;
            let (chord, theta, idx) = self.interp(r);
            let a = Annulus {
                r,
                chord,
                theta,
                omega,
                v,
                rho,
                mu,
                blades: f64::from(self.blades),
                r_hub,
                r_tip,
            };
            let (dt_dr, dq_dr, rooted) = a.loads(self.polars[idx].as_ref());
            thrust += dt_dr * dr;
            torque += dq_dr * dr;
            fallback += usize::from(!rooted);
        }
        let (c75, _, _) = self.interp(r_hub + 0.75 * (r_tip - r_hub));
        let w75 = (omega * 0.75 * r_tip).hypot(v);
        Some(BemtSolution {
            thrust_n: thrust,
            torque_n_m: torque,
            fallback_fraction: fallback as f64 / self.n_annuli as f64,
            re_75: rho * w75 * c75 / mu,
        })
    }
}

fn shape_at(x: f64) -> f64 {
    let t = &GENERIC_CHORD;
    if x <= t[0].0 {
        return t[0].1;
    }
    for w in t.windows(2) {
        if x <= w[1].0 {
            return w[0].1 + (x - w[0].0) / (w[1].0 - w[0].0) * (w[1].1 - w[0].1);
        }
    }
    t[t.len() - 1].1
}

struct Annulus {
    r: f64,
    chord: f64,
    theta: f64,
    omega: f64,
    v: f64,
    rho: f64,
    mu: f64,
    blades: f64,
    r_hub: f64,
    r_tip: f64,
}

impl Annulus {
    fn prandtl(&self, phi: f64) -> f64 {
        let s = phi.sin().abs().max(1.0e-6);
        let f = |x: f64| (2.0 / std::f64::consts::PI) * (-x).exp().clamp(0.0, 1.0).acos();
        let tip = f(self.blades * (self.r_tip - self.r) / (2.0 * self.r * s));
        let hub = f(self.blades * (self.r - self.r_hub) / (2.0 * self.r_hub * s));
        (tip * hub).max(1.0e-3)
    }

    /// Section normal and tangential coefficients at inflow angle `phi`.
    fn coefficients(&self, polar: &dyn SectionPolar, phi: f64, w: f64) -> (f64, f64) {
        let re = self.rho * w * self.chord / self.mu;
        let (cl, cd) = polar.cl_cd(self.theta - phi, re);
        let (s, c) = phi.sin_cos();
        (cl * c - cd * s, cl * s + cd * c)
    }

    fn residual(&self, polar: &dyn SectionPolar, phi: f64) -> f64 {
        let (s, c) = phi.sin_cos();
        let w0 = ((self.omega * self.r).powi(2) + self.v * self.v).sqrt();
        let (cn, ct) = self.coefficients(polar, phi, w0);
        let k =
            self.blades * self.chord / (8.0 * std::f64::consts::PI * self.r * self.prandtl(phi));
        self.omega * self.r * (s * s - k * cn) - self.v * (s * c + k * ct)
    }

    /// `(dT/dr, dQ/dr, root_found)`.
    fn loads(&self, polar: &dyn SectionPolar) -> (f64, f64, bool) {
        let n = 90;
        let top = std::f64::consts::FRAC_PI_2 - 1.0e-4;
        let (mut hi, mut f_hi) = (top, self.residual(polar, top));
        let mut root = None;
        for i in 1..=n {
            let lo = top - (top - 1.0e-3) * i as f64 / n as f64;
            let f_lo = self.residual(polar, lo);
            if f_hi.signum() != f_lo.signum() {
                root = brent(&mut |p| Some(self.residual(polar, p)), lo, hi, 1.0e-12);
                break;
            }
            hi = lo;
            f_hi = f_lo;
        }
        let found = root.is_some();
        let phi = root.unwrap_or_else(|| self.v.atan2(self.omega * self.r).max(1.0e-3));
        let (s, c) = phi.sin_cos();
        let w_free = (self.omega * self.r).hypot(self.v);
        let w = if found {
            // Relative speed from the tangential momentum relation; the section
            // coefficients are evaluated at the unperturbed speed for its Re.
            let (_, ct0) = self.coefficients(polar, phi, w_free);
            let k = self.blades * self.chord
                / (8.0 * std::f64::consts::PI * self.r * self.prandtl(phi));
            let den = s * c + k * ct0;
            if den.abs() > 1.0e-9 {
                (self.omega * self.r * s / den).abs()
            } else {
                w_free
            }
        } else {
            w_free
        };
        let (cn, ct) = self.coefficients(polar, phi, w);
        let q = 0.5 * self.rho * w * w * self.chord * self.blades;
        (q * cn, q * ct * self.r, found)
    }
}
