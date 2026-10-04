// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Native trapezoidal virtual-work error against the exported continuum beam.

use std::io;

pub(super) fn station_bound(
    y: &[f64],
    moment: &[f64],
    ei: &[f64],
    q: &[f64],
    points: &[(f64, f64)],
    station: usize,
) -> io::Result<f64> {
    let n = y.len();
    if n < 2
        || station >= n
        || [moment.len(), ei.len(), q.len()]
            .iter()
            .any(|&len| len != n)
        || y.iter()
            .chain(moment)
            .chain(ei)
            .chain(q)
            .any(|v| !v.is_finite())
        || ei.iter().any(|&v| v <= 0.0)
        || y.windows(2).any(|p| p[1] <= p[0])
        || points
            .iter()
            .any(|&(s, f)| !s.is_finite() || !f.is_finite() || s < y[0] || s > y[n - 1])
    {
        return Err(io::Error::other("invalid continuum quadrature inputs"));
    }
    let mut bound = 0.0;
    for i in 0..station {
        let h = y[i + 1] - y[i];
        let load = 0.5 * (q[i] + q[i + 1]);
        let slope_ei = (ei[i + 1] - ei[i]) / h;
        let local: Vec<_> = points
            .iter()
            .copied()
            .filter(|&(s, _)| s > y[i] && s < y[i + 1])
            .map(|(s, f)| (s - y[i], f))
            .collect();
        let slope_m = (moment[i + 1]
            - moment[i]
            - 0.5 * load * h * h
            - local.iter().map(|&(r, f)| f * (h - r)).sum::<f64>())
            / h;
        let mut breaks = vec![0.0, h];
        breaks.extend(local.iter().map(|&(r, _)| r));
        breaks.sort_by(f64::total_cmp);
        breaks.dedup();
        // For f=(target-s) M/EI, T(f)-integral(f)=integral(K f''),
        // K(r)=r(h-r)/2. Point loads contribute jumps in f' to this measure.
        for pair in breaks.windows(2) {
            let (left, right) = (pair[0], pair[1]);
            let active: Vec<_> = local.iter().copied().filter(|&(r, _)| r <= left).collect();
            let linear = slope_m + active.iter().map(|&(_, f)| f).sum::<f64>();
            let constant = moment[i] - active.iter().map(|&(r, f)| r * f).sum::<f64>();
            let m = |r: f64| constant + linear * r + 0.5 * load * r * r;
            let mut max_m = m(left).abs().max(m(right).abs());
            if load != 0.0 {
                let vertex = -linear / load;
                if vertex >= left && vertex <= right {
                    max_m = max_m.max(m(vertex).abs());
                }
            }
            let max_dm = (linear + load * left)
                .abs()
                .max((linear + load * right).abs());
            let min_ei = (ei[i] + slope_ei * left).min(ei[i] + slope_ei * right);
            let kappa_prime = max_dm / min_ei + max_m * slope_ei.abs() / min_ei.powi(2);
            let kappa_second = load.abs() / min_ei
                + 2.0 * max_dm * slope_ei.abs() / min_ei.powi(2)
                + 2.0 * max_m * slope_ei.powi(2) / min_ei.powi(3);
            let max_second = (y[station] - y[i] - left) * kappa_second + 2.0 * kappa_prime;
            let kernel_integral = |r: f64| h * r * r / 4.0 - r.powi(3) / 6.0;
            bound += max_second * (kernel_integral(right) - kernel_integral(left));
        }
        for &(r, force) in &local {
            let jump = (y[station] - y[i] - r) * force / (ei[i] + slope_ei * r);
            bound += 0.5 * r * (h - r) * jump.abs();
        }
    }
    if !bound.is_finite() {
        return Err(io::Error::other("non-finite quadrature bound"));
    }
    Ok(bound)
}

// F06 beam tables print E-format values with six decimal places (seven
// significant digits). This is a decimal printing bound, not a solver tolerance.
pub(super) fn printed_roundoff(value: f64) -> f64 {
    if value == 0.0 {
        0.0
    } else {
        0.5 * 10.0_f64.powf(value.abs().log10().floor() - 6.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tip_load_bound_recovers_closed_form_trapezoid_error_and_refinement() {
        let length: f64 = 10.0;
        let force = 1000.0;
        let stiffness = 70e6;
        for cells in [1, 2, 8, 32] {
            let y: Vec<_> = (0..=cells)
                .map(|i| length * i as f64 / cells as f64)
                .collect();
            let moment: Vec<_> = y.iter().map(|s| force * (length - s)).collect();
            let bound = station_bound(
                &y,
                &moment,
                &vec![stiffness; cells + 1],
                &vec![0.0; cells + 1],
                &[(length, force)],
                cells,
            )
            .unwrap();
            let exact_error = force * length.powi(3) / (6.0 * stiffness * (cells as f64).powi(2));
            assert!((bound - exact_error).abs() <= 32.0 * f64::EPSILON * exact_error);
        }
    }

    #[test]
    fn off_station_point_load_bound_includes_derivative_jump() {
        let (length, position, force, stiffness): (f64, f64, f64, f64) = (10.0, 3.5, 1000.0, 70e6);
        let bound = station_bound(
            &[0.0, length],
            &[force * position, 0.0],
            &[stiffness; 2],
            &[0.0; 2],
            &[(position, force)],
            1,
        )
        .unwrap();
        let exact = force * position.powi(2) * (3.0 * length - position) / (6.0 * stiffness);
        let trapezoid = force * position * length.powi(2) / (2.0 * stiffness);
        assert!((bound - (trapezoid - exact)).abs() <= 32.0 * f64::EPSILON * bound);
    }

    #[test]
    fn linear_stiffness_bound_contains_independent_logarithmic_solution() {
        let bound = station_bound(
            &[0.0, 1.0],
            &[1.0, 0.0],
            &[1.0, 2.0],
            &[0.0; 2],
            &[(1.0, 1.0)],
            1,
        )
        .unwrap();
        let continuum = 4.0 * 2.0_f64.ln() - 2.5;
        assert!(0.5 - continuum <= bound);
        assert!(station_bound(&[0.0, 1.0], &[1.0, 0.0], &[1.0, 0.0], &[0.0; 2], &[], 1).is_err());
    }
}
