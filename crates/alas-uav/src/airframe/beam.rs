// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Euler-Bernoulli cantilever for a wing spar (UAV-PLAN M4, section 3.7 / 9.1).
//!
//! The half-wing is a clamped beam at the root (`y = 0`) with a free tip
//! (`y = L`, `L` = semispan).  Sign convention: upward loads and upward
//! deflections are positive; the bending moment `M(y)` is the positive
//! root-hogging magnitude `M = int_y^L w (eta - y) d eta + P (L - y)`, and the
//! curvature is `w'' = M / EI`.
//!
//! Discretisation: trapezoidal integration on the supplied stations, which
//! must be strictly increasing with `y[0] = 0`.  Verified against the
//! closed-form cantilever cases in the tests.
//!
//! Units: m, N, N/m, N*m, N*m^2 (EI).

use super::AirframeError;

/// Spanwise stations of a cantilever with its bending stiffness.
#[derive(Debug, Clone, PartialEq)]
pub struct CantileverGrid {
    /// Station positions from the root, m (`y[0] = 0`, strictly increasing).
    pub y_m: Vec<f64>,
    /// Bending stiffness at each station, N m^2.
    pub ei_nm2: Vec<f64>,
}

/// Loads on a cantilever: a distributed load per station and a tip force.
#[derive(Debug, Clone, PartialEq)]
pub struct CantileverLoad {
    /// Distributed upward load per unit span at each station, N/m.
    pub w_n_per_m: Vec<f64>,
    /// Upward tip force, N.
    pub tip_force_n: f64,
}

/// Solution on the grid.
#[derive(Debug, Clone, PartialEq)]
pub struct CantileverSolution {
    /// Shear force at each station (root value is the reaction), N.
    pub shear_n: Vec<f64>,
    /// Bending moment at each station (root value is the root moment), N m.
    pub moment_nm: Vec<f64>,
    /// Deflection at each station (upward positive), m.
    pub deflection_m: Vec<f64>,
    /// Slope at each station, rad.
    pub slope_rad: Vec<f64>,
}

impl CantileverSolution {
    /// Root shear, N.
    #[must_use]
    pub fn root_shear_n(&self) -> f64 {
        self.shear_n[0]
    }

    /// Root bending moment magnitude, N m.
    #[must_use]
    pub fn root_moment_nm(&self) -> f64 {
        self.moment_nm[0]
    }

    /// Tip deflection, m.
    #[must_use]
    pub fn tip_deflection_m(&self) -> f64 {
        self.deflection_m[self.deflection_m.len() - 1]
    }
}

/// Cumulative trapezoid from the tip inwards: `out[i] = int_{y_i}^{y_end} f`.
fn tail_integral(y: &[f64], f: &[f64]) -> Vec<f64> {
    let n = y.len();
    let mut out = vec![0.0; n];
    for i in (0..n - 1).rev() {
        out[i] = out[i + 1] + 0.5 * (f[i] + f[i + 1]) * (y[i + 1] - y[i]);
    }
    out
}

/// Cumulative trapezoid from the root outwards: `out[i] = int_0^{y_i} f`.
fn root_integral(y: &[f64], f: &[f64]) -> Vec<f64> {
    let n = y.len();
    let mut out = vec![0.0; n];
    for i in 1..n {
        out[i] = out[i - 1] + 0.5 * (f[i] + f[i - 1]) * (y[i] - y[i - 1]);
    }
    out
}

/// Solve the clamped-free beam.
///
/// # Errors
///
/// Fewer than three stations, a grid that does not start at the root or is
/// not strictly increasing, mismatched array lengths, or a non-positive `EI`.
pub fn solve_cantilever(
    grid: &CantileverGrid,
    load: &CantileverLoad,
) -> Result<CantileverSolution, AirframeError> {
    let n = grid.y_m.len();
    if n < 3 || grid.ei_nm2.len() != n || load.w_n_per_m.len() != n {
        return Err(AirframeError::Invalid(
            "cantilever needs >= 3 stations with matching EI and load arrays".into(),
        ));
    }
    if grid.y_m[0] != 0.0 || grid.y_m.windows(2).any(|w| w[1].is_nan() || w[1] <= w[0]) {
        return Err(AirframeError::Invalid(
            "cantilever stations must start at the root and increase".into(),
        ));
    }
    if grid.ei_nm2.iter().any(|v| !(v.is_finite() && *v > 0.0)) {
        return Err(AirframeError::Invalid("EI must be positive".into()));
    }
    let y = &grid.y_m;
    let len = y[n - 1];
    let p = load.tip_force_n;
    // V(y) = int_y^L w + P ;  Mom(y) = int_y^L w eta + P L.
    let w_int = tail_integral(y, &load.w_n_per_m);
    let w_mom_f: Vec<f64> = load.w_n_per_m.iter().zip(y).map(|(w, e)| w * e).collect();
    let mom_int = tail_integral(y, &w_mom_f);
    let shear: Vec<f64> = w_int.iter().map(|v| v + p).collect();
    let moment: Vec<f64> = (0..n)
        .map(|i| mom_int[i] + p * len - y[i] * shear[i])
        .collect();
    let curvature: Vec<f64> = moment
        .iter()
        .zip(&grid.ei_nm2)
        .map(|(m, ei)| m / ei)
        .collect();
    let slope = root_integral(y, &curvature);
    let deflection = root_integral(y, &slope);
    Ok(CantileverSolution {
        shear_n: shear,
        moment_nm: moment,
        deflection_m: deflection,
        slope_rad: slope,
    })
}

/// Uniform stations `0..=length` with `n` intervals.
///
/// # Errors
///
/// `n == 0` or a non-positive length.
pub fn uniform_stations(length_m: f64, n: usize) -> Result<Vec<f64>, AirframeError> {
    if n == 0 || !(length_m.is_finite() && length_m > 0.0) {
        return Err(AirframeError::Invalid(
            "stations need a positive length and at least one interval".into(),
        ));
    }
    Ok((0..=n).map(|i| length_m * i as f64 / n as f64).collect())
}

/// Elliptic lift per unit span of one half-wing at `y`: the half-wing carries
/// `total_lift_n / 2` over the semispan `s`, so
/// `w(y) = (2 L / (pi s)) sqrt(1 - (y/s)^2)`, N/m.
///
/// Its root moment is `L b / (3 pi)` with `b = 2 s` (closed form, tested).
///
/// # Errors
///
/// Non-positive span.
pub fn elliptic_lift_per_m(
    total_lift_n: f64,
    semispan_m: f64,
    y_m: &[f64],
) -> Result<Vec<f64>, AirframeError> {
    if !(semispan_m > 0.0 && semispan_m.is_finite()) {
        return Err(AirframeError::Invalid("semispan must be positive".into()));
    }
    let w0 = 2.0 * total_lift_n / (std::f64::consts::PI * semispan_m);
    Ok(y_m
        .iter()
        .map(|&y| {
            let u = (y / semispan_m).clamp(-1.0, 1.0);
            w0 * (1.0 - u * u).sqrt()
        })
        .collect())
}

#[cfg(test)]
// Unit tests: a failed unwrap or expect is the assertion failing on fixtures the test builds.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::needless_range_loop)]
mod tests {
    use super::*;

    fn uniform_grid(length: f64, n: usize, ei: f64) -> CantileverGrid {
        CantileverGrid {
            y_m: uniform_stations(length, n).unwrap(),
            ei_nm2: vec![ei; n + 1],
        }
    }

    fn zeros(n: usize) -> Vec<f64> {
        vec![0.0; n + 1]
    }

    fn rel(a: f64, b: f64) -> f64 {
        ((a - b) / b).abs()
    }

    #[test]
    fn tip_load_matches_closed_form_deflection_and_moment() {
        // delta = P L^3 / (3 EI), M_root = P L, V = P.
        let (len, ei, p) = (1.2, 85.0, 40.0);
        let g = uniform_grid(len, 4000, ei);
        let load = CantileverLoad {
            w_n_per_m: zeros(4000),
            tip_force_n: p,
        };
        let s = solve_cantilever(&g, &load).unwrap();
        // Trapezoidal error of the quadratic slope is h^2/(4 L^2) ~ 1.5e-8 here.
        assert!(rel(s.tip_deflection_m(), p * len.powi(3) / (3.0 * ei)) < 1e-7);
        assert!(rel(s.root_moment_nm(), p * len) < 1e-12);
        assert!(rel(s.root_shear_n(), p) < 1e-12);
    }

    #[test]
    fn uniform_load_matches_closed_form() {
        // delta = w L^4 / (8 EI), M_root = w L^2 / 2, V_root = w L.
        let (len, ei, w) = (2.0, 310.0, 25.0);
        let n = 4000;
        let g = uniform_grid(len, n, ei);
        let load = CantileverLoad {
            w_n_per_m: vec![w; n + 1],
            tip_force_n: 0.0,
        };
        let s = solve_cantilever(&g, &load).unwrap();
        assert!(rel(s.tip_deflection_m(), w * len.powi(4) / (8.0 * ei)) < 1e-6);
        assert!(rel(s.root_moment_nm(), w * len * len / 2.0) < 1e-6);
        assert!(rel(s.root_shear_n(), w * len) < 1e-6);
    }

    #[test]
    fn stepped_stiffness_tip_deflection_matches_unit_load_integral() {
        // Root-clamped tip load on a two-segment beam: the tip deflection of
        // a stepped EI is int_0^L M(y)/EI(y) (L - y) dy (unit-load method).
        let n = 2000;
        let len = 1.0;
        let y = uniform_stations(len, n).unwrap();
        let ei: Vec<f64> = y
            .iter()
            .map(|&v| if v < 0.5 { 200.0 } else { 50.0 })
            .collect();
        let g = CantileverGrid {
            y_m: y.clone(),
            ei_nm2: ei.clone(),
        };
        let p = 10.0;
        let load = CantileverLoad {
            w_n_per_m: zeros(n),
            tip_force_n: p,
        };
        let s = solve_cantilever(&g, &load).unwrap();
        // Exact for a point load: 1/EI integrated against the unit moment.
        let integrand: Vec<f64> = y
            .iter()
            .zip(&ei)
            .map(|(&v, &e)| p * (len - v) * (len - v) / e)
            .collect();
        let mut unit = 0.0;
        for i in 0..n {
            unit += 0.5 * (integrand[i] + integrand[i + 1]) * (y[i + 1] - y[i]);
        }
        assert!(rel(s.tip_deflection_m(), unit) < 1e-5);
    }

    #[test]
    fn elliptic_root_moment_is_l_b_over_three_pi() {
        // Closed form: M_root = L b / (3 pi), b = 2 s.
        let (lift, semispan) = (2.4 * 9.81 * 3.0, 1.5);
        let n = 4000;
        let y = uniform_stations(semispan, n).unwrap();
        let w = elliptic_lift_per_m(lift, semispan, &y).unwrap();
        let g = CantileverGrid {
            y_m: y,
            ei_nm2: vec![100.0; n + 1],
        };
        let s = solve_cantilever(
            &g,
            &CantileverLoad {
                w_n_per_m: w,
                tip_force_n: 0.0,
            },
        )
        .unwrap();
        let closed = lift * (2.0 * semispan) / (3.0 * std::f64::consts::PI);
        assert!(
            rel(s.root_moment_nm(), closed) < 1e-3,
            "{}",
            s.root_moment_nm()
        );
        // Half-wing lift is L/2 at the root shear.
        assert!(rel(s.root_shear_n(), lift / 2.0) < 1e-3);
    }

    #[test]
    fn elliptic_distribution_integrates_to_half_lift() {
        let (lift, semispan) = (100.0, 0.8);
        let n = 4000;
        let y = uniform_stations(semispan, n).unwrap();
        let w = elliptic_lift_per_m(lift, semispan, &y).unwrap();
        let mut total = 0.0;
        for i in 0..n {
            total += 0.5 * (w[i] + w[i + 1]) * (y[i + 1] - y[i]);
        }
        assert!(rel(total, lift / 2.0) < 1e-5);
    }

    #[test]
    fn reversed_or_rootless_grid_is_refused() {
        let bad = CantileverGrid {
            y_m: vec![0.1, 0.2, 0.3],
            ei_nm2: vec![1.0; 3],
        };
        let load = CantileverLoad {
            w_n_per_m: vec![0.0; 3],
            tip_force_n: 1.0,
        };
        assert!(solve_cantilever(&bad, &load).is_err());
        let zero_ei = CantileverGrid {
            y_m: vec![0.0, 0.5, 1.0],
            ei_nm2: vec![1.0, 0.0, 1.0],
        };
        assert!(solve_cantilever(&zero_ei, &load).is_err());
    }
}
