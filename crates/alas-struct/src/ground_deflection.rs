// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Static deflection of a semi-wing standing on the ground under its own
//! weight: an Euler-Bernoulli cantilever with no aerodynamic lift.
//!
//! The wing is clamped at the side of body `y_support` and loaded downward by
//! its distributed mass (structure, fuel) and point masses (engines), each
//! multiplied by `n g`, with `n = 1` at rest and a taxi factor above 1 for a
//! bump. Optional point forces model reactions that act on the wing itself (a
//! wing-mounted gear leg); a positive force acts **downward**.
//!
//! Sign and frame: `y` is metres outboard from the aircraft centreline;
//! deflection is positive **downward** (a drooping wing), slope positive when
//! the deflection grows outboard. Quantities are SI (m, N, N.m, N.m^2).
//!
//! Method: shear `V(y) = int_y^L q deta + sum F_i [y_i > y]` and bending moment
//! `M(y) = int_y^L V deta` by the trapezoid rule (exact for the point terms, which
//! are added analytically), then `w'' = M/EI` integrated from the support with
//! the curvature taken linear between stations, which is exact for a linear
//! `M/EI` (a uniform-EI tip load) and second-order for a distributed load.
//!
//! Validity: small deflections (`w/L < 0.1`, the tip droop of a transport
//! wing on the ground is below 5 % of the semispan); no shear deformation, no
//! torsion or sweep-bending coupling, no change of load direction with slope.
//! The result is only as good as the `EI(y)` supplied; see
//! [`crate::sizing`] for what the sized box represents.

/// A point force on the semi-wing, N positive downward, at a spanwise station.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointForce {
    /// Spanwise station from the centreline, m.
    pub y_m: f64,
    /// Force, N, positive downward (a weight), negative upward (a reaction).
    pub down_n: f64,
}

/// Why a ground deflection could not be computed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum GroundDeflectionError {
    /// The station grid, stiffness or load arrays are inconsistent, not
    /// strictly increasing, or hold a non-finite value, or a stiffness is not
    /// positive.
    #[error("the ground-deflection inputs are inconsistent or non-finite")]
    InvalidInput,
}

/// The spanwise ground-deflection curve of one semi-wing.
#[derive(Debug, Clone, PartialEq)]
pub struct GroundDeflection {
    /// Stations from the centreline, m.
    pub y_m: Vec<f64>,
    /// Shear force, N (positive: the outboard wing hangs from this section).
    pub shear_n: Vec<f64>,
    /// Hogging bending moment from the outboard loads, N.m.
    pub moment_nm: Vec<f64>,
    /// Bending slope, rad, zero at and inboard of the support.
    pub slope_rad: Vec<f64>,
    /// Downward deflection relative to the unloaded shape, m, zero at and
    /// inboard of the support.
    pub deflection_down_m: Vec<f64>,
}

impl GroundDeflection {
    /// Downward deflection at `y`, m, linearly interpolated and clamped to the
    /// tabulated range (zero inboard of the support, tip value beyond the tip).
    pub fn droop_at_m(&self, y_m: f64) -> f64 {
        interpolate(&self.y_m, &self.deflection_down_m, y_m)
    }

    /// Downward deflection of the tip station, m.
    pub fn tip_droop_m(&self) -> f64 {
        self.deflection_down_m.last().copied().unwrap_or(0.0)
    }
}

/// The sized box's effective spanwise-frame bending stiffness, N.m^2, at every
/// sizing station (`EI_n cos^3 L`, see [`crate::sizing::swept`]; `EI_n` is the
/// section normal to the swept box axis, so `w'' = M_x / EI`):
/// the compatible-strain (plane sections) sum over caps, webs and the two
/// covers that the strength sizing and the analytical response both use.
/// The covers and spar flanges are the smeared stiffened skin-stringer panel
/// of [`crate::sizing::panel`], whose compressive allowable the sizing
/// enforces; the stiffness is that of the strength-sized section (a
/// consequence of the allowable, not a separate target). The swept-axis
/// factor of [`crate::flex_check`] is already inside this stiffness, so it must
/// not be applied again.
pub fn box_bending_stiffness_nm2(
    sizing: &crate::sizing::WingboxSizing,
    skin: &alas_config::materials::MaterialSpec,
    web: &alas_config::materials::MaterialSpec,
    cap: &alas_config::materials::MaterialSpec,
) -> Vec<f64> {
    (0..sizing.y_stations.len())
        .map(|station| crate::sizing::section::station_ei_spanwise(sizing, station, skin, web, cap))
        .collect()
}

fn interpolate(x: &[f64], values: &[f64], at: f64) -> f64 {
    let (Some(&first), Some(&last)) = (x.first(), x.last()) else {
        return 0.0;
    };
    if at <= first {
        return values[0];
    }
    if at >= last {
        return values[values.len() - 1];
    }
    let upper = x.partition_point(|&station| station <= at).max(1);
    let lower = upper - 1;
    let t = (at - x[lower]) / (x[upper] - x[lower]);
    values[lower] + t * (values[upper] - values[lower])
}

/// Deflection of a semi-wing clamped at `support_y_m` under distributed and
/// point loads.
///
/// `running_load_n_m` is the downward line load at each station (for a mass
/// distribution `n g m'`). `ei_nm2` is the bending stiffness at each station.
///
/// # Errors
///
/// [`GroundDeflectionError::InvalidInput`] for fewer than two stations, a grid
/// that does not increase, arrays of different length, non-finite or
/// non-positive stiffness, or a non-finite load, or a support outside the grid.
pub fn static_deflection(
    y_m: &[f64],
    ei_nm2: &[f64],
    running_load_n_m: &[f64],
    point_forces: &[PointForce],
    support_y_m: f64,
) -> Result<GroundDeflection, GroundDeflectionError> {
    let count = y_m.len();
    let valid = count >= 2
        && ei_nm2.len() == count
        && running_load_n_m.len() == count
        && y_m.iter().all(|y| y.is_finite())
        && y_m.windows(2).all(|pair| pair[1] > pair[0])
        && ei_nm2.iter().all(|ei| ei.is_finite() && *ei > 0.0)
        && running_load_n_m.iter().all(|q| q.is_finite())
        && point_forces
            .iter()
            .all(|f| f.y_m.is_finite() && f.down_n.is_finite())
        && support_y_m.is_finite()
        && support_y_m >= y_m[0]
        && support_y_m < y_m[count - 1];
    if !valid {
        return Err(GroundDeflectionError::InvalidInput);
    }

    // Shear and moment from the tip inward. A point force at y_i adds its
    // value to the shear and (y_i - y) F to the moment at every y < y_i.
    let mut shear = vec![0.0; count];
    let mut moment = vec![0.0; count];
    for k in (0..count - 1).rev() {
        let dy = y_m[k + 1] - y_m[k];
        shear[k] = shear[k + 1] + 0.5 * dy * (running_load_n_m[k] + running_load_n_m[k + 1]);
        moment[k] = moment[k + 1] + 0.5 * dy * (shear[k] + shear[k + 1]);
    }
    for force in point_forces {
        for k in 0..count {
            if y_m[k] < force.y_m {
                shear[k] += force.down_n;
                moment[k] += force.down_n * (force.y_m - y_m[k]);
            }
        }
    }

    // Double integration of the curvature from the support.
    let mut slope = vec![0.0; count];
    let mut deflection = vec![0.0; count];
    let first = y_m.partition_point(|&station| station < support_y_m);
    for k in first..count - 1 {
        let h = y_m[k + 1] - y_m[k];
        // A support between stations: start at the grid point at or outboard
        // of it (the clamp error is below one grid step).
        let c0 = moment[k] / ei_nm2[k];
        let c1 = moment[k + 1] / ei_nm2[k + 1];
        slope[k + 1] = slope[k] + 0.5 * h * (c0 + c1);
        deflection[k + 1] = deflection[k] + h * slope[k] + h * h * (2.0 * c0 + c1) / 6.0;
    }
    Ok(GroundDeflection {
        y_m: y_m.to_vec(),
        shear_n: shear,
        moment_nm: moment,
        slope_rad: slope,
        deflection_down_m: deflection,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(length: f64, n: usize) -> Vec<f64> {
        (0..n).map(|i| length * i as f64 / (n - 1) as f64).collect()
    }

    #[test]
    fn a_tip_point_load_gives_p_l_cubed_over_3_ei() {
        // Bruhn A6 / any beam text: w_tip = P L^3 / (3 EI).
        let (length, ei, p) = (14.0, 4.0e8, 2.5e5);
        let y = grid(length, 57);
        let w = static_deflection(
            &y,
            &vec![ei; y.len()],
            &vec![0.0; y.len()],
            &[PointForce {
                y_m: length,
                down_n: p,
            }],
            0.0,
        )
        .unwrap();
        let expected = p * length.powi(3) / (3.0 * ei);
        assert!((w.tip_droop_m() / expected - 1.0).abs() < 1e-12);
        assert!((w.slope_rad[56] - p * length * length / (2.0 * ei)).abs() < 1e-12);
    }

    #[test]
    fn a_uniform_load_gives_q_l_to_the_fourth_over_8_ei() {
        let (length, ei, q) = (12.0, 3.0e8, 2500.0);
        let y = grid(length, 401);
        let w = static_deflection(&y, &vec![ei; 401], &vec![q; 401], &[], 0.0).unwrap();
        let expected = q * length.powi(4) / (8.0 * ei);
        assert!((w.tip_droop_m() / expected - 1.0).abs() < 1e-5);
        // Mid-span, w(x) = q x^2 (6 L^2 - 4 L x + x^2) / (24 EI).
        let x = y[200];
        let analytic = q * x * x * (6.0 * length * length - 4.0 * length * x + x * x) / (24.0 * ei);
        assert!((w.deflection_down_m[200] / analytic - 1.0).abs() < 1e-5);
    }

    #[test]
    fn an_off_node_point_load_is_exact_in_moment_and_close_in_deflection() {
        // P at a: w_tip = P a^2 (3 L - a) / (6 EI).
        let (length, ei, p, a) = (10.0, 1.0e8, 1.0e5, 6.3);
        let y = grid(length, 201);
        let w = static_deflection(
            &y,
            &vec![ei; 201],
            &vec![0.0; 201],
            &[PointForce { y_m: a, down_n: p }],
            0.0,
        )
        .unwrap();
        let expected = p * a * a * (3.0 * length - a) / (6.0 * ei);
        assert!((w.tip_droop_m() / expected - 1.0).abs() < 2e-3);
        // Root moment is exactly P a.
        assert!((w.moment_nm[0] - p * a).abs() < 1e-6);
    }

    #[test]
    fn point_and_distributed_loads_superpose() {
        let y = grid(20.0, 161);
        let ei: Vec<f64> = y.iter().map(|s| 8.0e8 * (1.0 - 0.7 * s / 20.0)).collect();
        let q: Vec<f64> = y.iter().map(|s| 9000.0 * (1.0 - 0.5 * s / 20.0)).collect();
        let none = vec![0.0; y.len()];
        let pf = [PointForce {
            y_m: 7.0,
            down_n: 3.0e5,
        }];
        let both = static_deflection(&y, &ei, &q, &pf, 1.5).unwrap();
        let dist = static_deflection(&y, &ei, &q, &[], 1.5).unwrap();
        let point = static_deflection(&y, &ei, &none, &pf, 1.5).unwrap();
        for k in 0..y.len() {
            let sum = dist.deflection_down_m[k] + point.deflection_down_m[k];
            assert!((both.deflection_down_m[k] - sum).abs() <= 1e-12 * (1.0 + sum.abs()));
            assert!((both.moment_nm[k] - dist.moment_nm[k] - point.moment_nm[k]).abs() < 1e-3);
        }
    }

    #[test]
    fn the_deflection_is_zero_at_and_inboard_of_the_support_and_shifts_with_it() {
        let y = grid(10.0, 101);
        let ei = vec![1.0e8; 101];
        let q = vec![1000.0; 101];
        let clamped = static_deflection(&y, &ei, &q, &[], 2.0).unwrap();
        assert!(clamped.deflection_down_m[..=20].iter().all(|&w| w == 0.0));
        // A cantilever of length L - 2 under the same line load plus the
        // moment/shear of nothing inboard: identical to a root at 2 m.
        let shifted: Vec<f64> = y[20..].iter().map(|s| s - 2.0).collect();
        let reference = static_deflection(&shifted, &ei[20..], &q[20..], &[], 0.0).unwrap();
        assert!((clamped.tip_droop_m() / reference.tip_droop_m() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn an_upward_reaction_reduces_the_droop_and_a_stiffer_wing_droops_less() {
        let y = grid(10.0, 101);
        let q = vec![2000.0; 101];
        let soft = static_deflection(&y, &vec![1.0e8; 101], &q, &[], 0.0).unwrap();
        let stiff = static_deflection(&y, &vec![2.0e8; 101], &q, &[], 0.0).unwrap();
        assert!((soft.tip_droop_m() / stiff.tip_droop_m() - 2.0).abs() < 1e-12);
        let propped = static_deflection(
            &y,
            &vec![1.0e8; 101],
            &q,
            &[PointForce {
                y_m: 4.0,
                down_n: -3.0e4,
            }],
            0.0,
        )
        .unwrap();
        assert!(propped.tip_droop_m() < soft.tip_droop_m());
    }

    #[test]
    fn invalid_inputs_fail_closed() {
        let y = grid(5.0, 11);
        let ei = vec![1.0e8; 11];
        let q = vec![0.0; 11];
        assert!(static_deflection(&y, &ei, &q[..10], &[], 0.0).is_err());
        let mut bad = ei.clone();
        bad[3] = f64::NAN;
        assert!(static_deflection(&y, &bad, &q, &[], 0.0).is_err());
        bad[3] = 0.0;
        assert!(static_deflection(&y, &bad, &q, &[], 0.0).is_err());
        assert!(static_deflection(&y, &ei, &q, &[], 5.0).is_err());
        assert!(static_deflection(&y[..1], &ei[..1], &q[..1], &[], 0.0).is_err());
    }

    #[test]
    fn interpolation_clamps_at_both_ends() {
        let y = grid(4.0, 5);
        let w = static_deflection(&y, &[1.0e6; 5], &[100.0; 5], &[], 1.0).unwrap();
        assert_eq!(w.droop_at_m(-3.0), 0.0);
        assert_eq!(w.droop_at_m(0.5), 0.0);
        assert_eq!(w.droop_at_m(99.0), w.tip_droop_m());
        let mid = w.droop_at_m(3.5);
        assert!(mid > w.deflection_down_m[3] && mid < w.deflection_down_m[4]);
    }
}
