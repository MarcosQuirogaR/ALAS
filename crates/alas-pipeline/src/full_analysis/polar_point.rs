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
