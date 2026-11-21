// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Selection of the un-trimmed cruise design point from a polar sweep.

use alas_aero::analysis::PolarSweep;

use super::DesignPoint;

/// The sweep point whose lift coefficient is closest to `cl_target`, the
/// first one on a tie. A NaN lift never wins over a finite one.
///
/// An empty sweep, or one whose columns differ in length, has no design point
/// and is reported as an error rather than indexed out of bounds.
pub(super) fn design_point_nearest(
    polar: &PolarSweep,
    cl_target: f64,
) -> Result<DesignPoint, String> {
    let mut best_idx = 0usize;
    let mut min_diff = f64::INFINITY;
    for (i, &cl) in polar.cl.iter().enumerate() {
        let diff = (cl - cl_target).abs();
        if diff < min_diff {
            min_diff = diff;
            best_idx = i;
        }
    }
    let point = |values: &[f64]| values.get(best_idx).copied();
    match (
        point(&polar.alpha_deg),
        point(&polar.cl),
        point(&polar.cd),
        point(&polar.l_over_d),
    ) {
        (Some(alpha_deg), Some(cl), Some(cd), Some(l_over_d)) => Ok(DesignPoint {
            alpha_deg,
            cl,
            cd,
            l_over_d,
        }),
        _ => Err(format!(
            "polar sweep has no design point: {} alpha, {} CL, {} CD and {} L/D values",
            polar.alpha_deg.len(),
            polar.cl.len(),
            polar.cd.len(),
            polar.l_over_d.len()
        )),
    }
}

/// The polar at exactly `cl_target`: the angle by linear interpolation
/// between the two samples that bracket it, the drag by the quadratic through
/// the bracketing pair and the next sample (the polar is a parabola in `CL`,
/// so the sample spacing does not quantise the answer), and `CL / CD` for the
/// lift-to-drag ratio. A target outside the sweep, or a sweep that is not
/// monotone in lift or has non-finite values, falls back to
/// [`design_point_nearest`].
pub(super) fn design_point_at_cl(
    polar: &PolarSweep,
    cl_target: f64,
) -> Result<DesignPoint, String> {
    let n = polar.cl.len();
    let aligned = [polar.alpha_deg.len(), polar.cd.len(), polar.l_over_d.len()]
        .iter()
        .all(|&len| len == n);
    let finite = polar
        .cl
        .iter()
        .chain(&polar.cd)
        .chain(&polar.alpha_deg)
        .all(|v| v.is_finite());
    let rising = polar.cl.windows(2).all(|w| w[1] > w[0]);
    if n < 3 || !aligned || !finite || !rising || !cl_target.is_finite() {
        return design_point_nearest(polar, cl_target);
    }
    let Some(i) = (0..n - 1).find(|&i| polar.cl[i] <= cl_target && cl_target <= polar.cl[i + 1])
    else {
        return design_point_nearest(polar, cl_target);
    };
    let (c0, c1) = (polar.cl[i], polar.cl[i + 1]);
    let t = (cl_target - c0) / (c1 - c0);
    let alpha_deg = polar.alpha_deg[i] + t * (polar.alpha_deg[i + 1] - polar.alpha_deg[i]);
    // Three samples for the parabola: the bracket plus its neighbour on the
    // side with the nearer target.
    let first = if i + 2 < n && (t >= 0.5 || i == 0) {
        i
    } else {
        i - 1
    };
    let idx = [first, first + 1, first + 2];
    let cd: f64 = (0..3)
        .map(|a| {
            let weight: f64 = (0..3)
                .filter(|&b| b != a)
                .map(|b| (cl_target - polar.cl[idx[b]]) / (polar.cl[idx[a]] - polar.cl[idx[b]]))
                .product();
            weight * polar.cd[idx[a]]
        })
        .sum();
    Ok(DesignPoint {
        alpha_deg,
        cl: cl_target,
        cd,
        l_over_d: cl_target / cd,
    })
}

#[cfg(test)]
// Failed expectations and unwraps here are failed test assertions.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn sweep(cl: Vec<f64>) -> PolarSweep {
        let n = cl.len();
        PolarSweep {
            alpha_deg: (0..n).map(|i| i as f64 - 1.0).collect(),
            geometric_alpha_deg: vec![0.0; n],
            compressible_mach: None,
            cl,
            cd: vec![0.02; n],
            cd_induced: vec![0.0; n],
            cd_wave: vec![0.0; n],
            cd_parasite: vec![0.0; n],
            cm: vec![0.0; n],
            l_over_d: vec![0.0; n],
        }
    }

    #[test]
    fn the_interpolated_design_point_is_exact_on_a_parabolic_polar() {
        // CD = 0.02 + 0.05 (CL - 0.1)^2 on a coarse lift grid, alpha linear.
        let cl = vec![-0.1, 0.1, 0.3, 0.5, 0.7, 0.9];
        let mut polar = sweep(cl.clone());
        polar.alpha_deg = cl.iter().map(|c| 10.0 * c).collect();
        polar.cd = cl
            .iter()
            .map(|c| 0.02 + 0.05 * (c - 0.1) * (c - 0.1))
            .collect();
        for target in [0.12, 0.31, 0.45, 0.5, 0.88] {
            let point = design_point_at_cl(&polar, target).unwrap();
            assert_eq!(point.cl, target);
            assert!((point.alpha_deg - 10.0 * target).abs() < 1e-12);
            let cd = 0.02 + 0.05 * (target - 0.1) * (target - 0.1);
            assert!(
                (point.cd - cd).abs() < 1e-12,
                "{target}: {} vs {cd}",
                point.cd
            );
            assert!((point.l_over_d - target / cd).abs() < 1e-9);
        }
    }

    #[test]
    fn the_interpolated_design_point_falls_back_to_the_nearest_sample() {
        let polar = sweep(vec![0.1, 0.3, 0.5]);
        // Outside the sweep.
        let beyond = design_point_at_cl(&polar, 0.9).unwrap();
        assert_eq!(beyond.cl, 0.5);
        // Not monotone in lift.
        let ragged = sweep(vec![0.1, 0.5, 0.3, 0.6]);
        assert_eq!(
            design_point_at_cl(&ragged, 0.45).unwrap(),
            design_point_nearest(&ragged, 0.45).unwrap()
        );
        assert!(design_point_at_cl(&sweep(Vec::new()), 0.5).is_err());
    }

    #[test]
    fn design_point_is_the_first_closest_lift_and_malformed_sweeps_are_errors() {
        let polar = sweep(vec![0.1, f64::NAN, 0.5, 0.3, 0.5]);
        let point = design_point_nearest(&polar, 0.45).unwrap();
        assert_eq!((point.alpha_deg, point.cl), (1.0, 0.5));

        assert!(design_point_nearest(&sweep(Vec::new()), 0.5).is_err());
        let mut ragged = sweep(vec![0.1, 0.5]);
        ragged.l_over_d.truncate(1);
        assert!(design_point_nearest(&ragged, 0.5).is_err());
    }
}
