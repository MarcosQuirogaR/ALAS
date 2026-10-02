// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The trimmed induced-drag quadratic of [`super::TrimmedDragTable`]: three
//! trimmed vortex-lattice solves placed with the cruise trim Jacobian, and
//! the fourth solve that checks them.

use alas_aero::analysis::{AeroAnalysis, TrimPoint};
use alas_stab::trim::StabilityTrimResult;

use super::{
    DragTableError, INDUCED_CHECK_FRACTION, INDUCED_CHECK_RELATIVE_TOLERANCE,
    INDUCED_NODE_FRACTIONS,
};

/// Newton corrections allowed at one induced node before it is declared
/// untrimmed: the same budget as `alas_stab`'s own trim refinement. Each is
/// one lattice solve; the transport presets reach the moment tolerance in
/// one or two, the linear model's residual being second order in the lift
/// change.
const NODE_TRIM_ITERATIONS: usize = 8;

/// The fourth trimmed solve and what the quadratic predicts there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InducedCheck {
    /// Lift coefficient the checking solve achieved.
    pub cl: f64,
    /// Trimmed induced drag the lattice returned there.
    pub vlm_cd_induced: f64,
    /// The fitted quadratic at the same CL.
    pub quadratic_cd_induced: f64,
}

impl InducedCheck {
    /// `|quadratic - VLM| / VLM`.
    pub fn relative_error(&self) -> f64 {
        (self.quadratic_cd_induced - self.vlm_cd_induced).abs() / self.vlm_cd_induced
    }
}

/// The cruise trim a table is built around: what `trim_and_polar` solved.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DesignTrim {
    /// The converged trim and its Jacobian.
    pub trim: StabilityTrimResult,
    /// Lift coefficient the trimmed re-solve achieved.
    pub cl: f64,
    /// Trimmed induced drag of that re-solve.
    pub cd_induced: f64,
    /// Pitching moment left by that re-solve.
    pub cm_residual: f64,
    /// Cruise Mach and ISA altitude, m.
    pub mach: f64,
    pub altitude_m: f64,
    /// Clean maximum lift coefficient, the top of the CL grid.
    pub cl_max_clean: f64,
    /// Moment residual an induced node must reach.
    pub cm_tolerance: f64,
}

/// The exact quadratic through three trimmed solves, and the fourth-point check.
pub(super) fn induced_quadratic(
    aero: &AeroAnalysis<'_>,
    design: &DesignTrim,
) -> Result<([f64; 3], InducedCheck), DragTableError> {
    let t = &design.trim;
    let node = |fraction: f64| -> Result<(f64, f64), DragTableError> {
        let cl_target = fraction * design.cl;
        if fraction == 1.0 {
            return Ok((design.cl, design.cd_induced));
        }
        let (mut alpha, mut incidence) = (t.trim_alpha_deg, t.trim_ih_deg);
        let (mut r_cl, mut r_cm) = (design.cl - cl_target, design.cm_residual);
        // `[[dCL/da, dCL/dih], [dCm/da, dCm/dih]]`, per degree.
        let mut jacobian = [[t.cl_alpha, t.cl_ih], [t.cm_alpha, t.cm_ih]];
        for _ in 0..NODE_TRIM_ITERATIONS {
            // Newton step on the current Jacobian: J [da, dih] = -r.
            let [[a, b], [c, d]] = jacobian;
            let det = a * d - b * c;
            if !det.is_finite() || det == 0.0 {
                return Err(DragTableError::SingularJacobian);
            }
            let step = [-(d * r_cl - b * r_cm) / det, -(a * r_cm - c * r_cl) / det];
            alpha += step[0];
            incidence += step[1];
            let point = TrimPoint {
                trim_alpha_deg: alpha,
                trim_ih_deg: incidence,
                cl_alpha: t.cl_alpha,
            };
            let perf = aero
                .trimmed_performance(&point, design.mach, design.altitude_m)
                .map_err(|error| DragTableError::Solve(error.to_string()))?;
            let change = [perf.cl - cl_target - r_cl, perf.cm_residual - r_cm];
            r_cl = perf.cl - cl_target;
            r_cm = perf.cm_residual;
            // Broyden's rank-one update (Broyden 1965, Math. Comp. 19, eq. 4.5)
            // keeps the model secant to the solves already paid for.
            let norm = step[0] * step[0] + step[1] * step[1];
            if norm > 0.0 {
                for (row, dy) in jacobian.iter_mut().zip(change) {
                    let miss = (dy - (row[0] * step[0] + row[1] * step[1])) / norm;
                    row[0] += miss * step[0];
                    row[1] += miss * step[1];
                }
            }
            if r_cm.abs() <= design.cm_tolerance
                && perf.cl.is_finite()
                && perf.cd_induced.is_finite()
            {
                return Ok((perf.cl, perf.cd_induced));
            }
        }
        Err(DragTableError::UntrimmedNode {
            cl_target,
            cm_residual: r_cm,
        })
    };
    let points = [
        node(INDUCED_NODE_FRACTIONS[0])?,
        node(INDUCED_NODE_FRACTIONS[1])?,
        node(INDUCED_NODE_FRACTIONS[2])?,
    ];
    let coefficients =
        quadratic_through(points).ok_or(DragTableError::NonPhysical("induced nodes"))?;
    let (cl, vlm_cd_induced) = node(INDUCED_CHECK_FRACTION)?;
    let [c0, c1, c2] = coefficients;
    let check = InducedCheck {
        cl,
        vlm_cd_induced,
        quadratic_cd_induced: c0 + cl * (c1 + cl * c2),
    };
    // Written so a NaN fails each gate rather than slipping through it.
    let checked =
        vlm_cd_induced > 0.0 && check.relative_error() <= INDUCED_CHECK_RELATIVE_TOLERANCE;
    if !checked {
        return Err(DragTableError::InducedCheck(check));
    }
    if c2 > 0.0 {
        Ok((coefficients, check))
    } else {
        Err(DragTableError::NonPhysical("induced curvature"))
    }
}

/// `[c0, c1, c2]` of the parabola through three `(x, y)` points (Newton
/// divided differences), `None` for coincident abscissae.
pub(super) fn quadratic_through(points: [(f64, f64); 3]) -> Option<[f64; 3]> {
    let [(x0, y0), (x1, y1), (x2, y2)] = points;
    let d01 = (y1 - y0) / (x1 - x0);
    let d12 = (y2 - y1) / (x2 - x1);
    let c2 = (d12 - d01) / (x2 - x0);
    let c1 = d01 - c2 * (x0 + x1);
    let c0 = y0 - x0 * (c1 + x0 * c2);
    [c0, c1, c2]
        .iter()
        .all(|c| c.is_finite())
        .then_some([c0, c1, c2])
}
