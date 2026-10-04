// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Checked quadratic segments of trimmed Trefftz-plane wake energy. The
//! trim Jacobian places each solve; independent checks refine the CL range
//! when its finite-angle geometry cannot be represented by one parabola.

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

/// The checking solves must be genuinely trimmed before comparing wake
/// energies. The candidate's 1e-3 moment acceptance is a validity gate,
/// not numerical convergence: a small remaining tail-load error can exceed
/// 0.3% of induced drag. Match the trim solver's 1e-7 residual resolution.
const NODE_RESIDUAL_TOLERANCE: f64 = 1.0e-7;

/// Eight dyadic refinements resolve a CL interval to less than 1/256 of its
/// original width; a remaining error is returned rather than accepted.
const MAX_INDUCED_REFINEMENT_DEPTH: usize = 8;

type InducedNode = (f64, f64);

#[derive(Debug, Clone, PartialEq)]
pub(super) struct InducedCurve {
    pub(super) pieces: Vec<QuadraticPiece>,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct QuadraticPiece {
    low: f64,
    high: f64,
    pub(super) nodes: [f64; 3],
    pub(super) coefficients: [f64; 3],
}

impl InducedCurve {
    pub(super) fn coefficients_at(&self, cl: f64) -> [f64; 3] {
        let index = self.pieces.partition_point(|piece| piece.high < cl);
        self.pieces[index.min(self.pieces.len() - 1)].coefficients
    }

    pub(super) fn value_and_slope(&self, cl: f64) -> (f64, f64) {
        let [c0, c1, c2] = self.coefficients_at(cl);
        (c0 + cl * (c1 + cl * c2), c1 + 2.0 * c2 * cl)
    }

    pub(super) fn ranges(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        self.pieces.iter().map(|piece| (piece.low, piece.high))
    }
}

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
    /// Candidate moment validity limit; node convergence is also limited
    /// by `NODE_RESIDUAL_TOLERANCE`.
    pub cm_tolerance: f64,
}

/// Retain the cruise quadratic if it passes independent checks across the
/// clean CL range; otherwise refine checked quadratic segments adaptively.
pub(super) fn induced_curve(
    aero: &AeroAnalysis<'_>,
    design: &DesignTrim,
    screening: bool,
) -> Result<(InducedCurve, InducedCheck), DragTableError> {
    if !(design.cl > 0.0
        && design.cl <= design.cl_max_clean
        && design.cl_max_clean.is_finite()
        && design.cd_induced > 0.0
        && design.cd_induced.is_finite())
    {
        return Err(DragTableError::NonPhysical(
            "design induced state or clean CL range",
        ));
    }
    let mut samples = Vec::<(f64, InducedNode)>::new();
    let mut sample = |cl_target: f64| -> Result<InducedNode, DragTableError> {
        if let Some((_, point)) = samples.iter().find(|(cl, _)| *cl == cl_target) {
            return Ok(*point);
        }
        let point = trimmed_node(aero, design, cl_target)?;
        samples.push((cl_target, point));
        Ok(point)
    };
    let points = [
        sample(INDUCED_NODE_FRACTIONS[0] * design.cl)?,
        sample(design.cl)?,
        sample(INDUCED_NODE_FRACTIONS[2] * design.cl)?,
    ];
    let coefficients =
        quadratic_through(points).ok_or(DragTableError::NonPhysical("induced nodes"))?;
    let checking = sample(INDUCED_CHECK_FRACTION * design.cl)?;
    let mut boundaries = vec![sample(0.0)?];
    boundaries.extend(
        [points[0], points[2]]
            .into_iter()
            .filter(|(cl, _)| *cl > 0.0 && *cl < design.cl_max_clean),
    );
    boundaries.push(sample(design.cl_max_clean)?);
    boundaries.sort_by(|left, right| left.0.total_cmp(&right.0));
    let mut checked = valid_quadratic(coefficients, 0.0, design.cl_max_clean)
        && checked_point(coefficients, checking).relative_error()
            <= INDUCED_CHECK_RELATIVE_TOLERANCE;
    for &point in &boundaries {
        checked &=
            checked_point(coefficients, point).relative_error() <= INDUCED_CHECK_RELATIVE_TOLERANCE;
    }
    if !screening || !checked {
        for pair in boundaries.windows(2) {
            for fraction in [0.25, 0.75] {
                let point = sample(pair[0].0 + fraction * (pair[1].0 - pair[0].0))?;
                checked &= checked_point(coefficients, point).relative_error()
                    <= INDUCED_CHECK_RELATIVE_TOLERANCE;
            }
        }
    }
    let mut pieces = Vec::new();
    if checked {
        pieces.push(QuadraticPiece {
            low: boundaries[0].0,
            high: boundaries[boundaries.len() - 1].0,
            nodes: points.map(|point| point.0),
            coefficients,
        });
    } else {
        for pair in boundaries.windows(2) {
            refine_piece(
                &mut sample,
                [pair[0], pair[1]],
                points[1],
                checking,
                0,
                &mut pieces,
            )?;
        }
    }
    let curve = InducedCurve { pieces };
    let check = InducedCheck {
        cl: checking.0,
        vlm_cd_induced: checking.1,
        quadratic_cd_induced: curve.value_and_slope(checking.0).0,
    };
    if check.relative_error() <= INDUCED_CHECK_RELATIVE_TOLERANCE {
        Ok((curve, check))
    } else {
        Err(DragTableError::InducedCheck(check))
    }
}

fn refine_piece(
    sample: &mut impl FnMut(f64) -> Result<InducedNode, DragTableError>,
    ends: [InducedNode; 2],
    design: InducedNode,
    checking: InducedNode,
    depth: usize,
    pieces: &mut Vec<QuadraticPiece>,
) -> Result<(), DragTableError> {
    let [low, high] = ends;
    let mut middle_cl = 0.5 * (low.0 + high.0);
    if middle_cl == design.0 || middle_cl == checking.0 {
        middle_cl = 0.5 * (low.0 + middle_cl);
    }
    let middle = if low.0 < design.0 && design.0 < high.0 {
        design
    } else {
        sample(middle_cl)?
    };
    let coefficients = quadratic_through([low, middle, high])
        .ok_or(DragTableError::NonPhysical("induced nodes"))?;
    let mut checks = Vec::with_capacity(3);
    for fraction in [0.25, 0.75] {
        checks.push(checked_point(
            coefficients,
            sample(low.0 + fraction * (high.0 - low.0))?,
        ));
    }
    if low.0 <= checking.0 && checking.0 <= high.0 {
        checks.push(checked_point(coefficients, checking));
    }
    let worst = checks
        .into_iter()
        .max_by(|left, right| left.relative_error().total_cmp(&right.relative_error()))
        .ok_or(DragTableError::NonPhysical("induced checks"))?;
    let physical = valid_quadratic(coefficients, low.0, high.0);
    if physical && worst.relative_error() <= INDUCED_CHECK_RELATIVE_TOLERANCE {
        pieces.push(QuadraticPiece {
            low: low.0,
            high: high.0,
            nodes: [low.0, middle.0, high.0],
            coefficients,
        });
        return Ok(());
    }
    if depth == MAX_INDUCED_REFINEMENT_DEPTH {
        return Err(if physical {
            DragTableError::InducedCheck(worst)
        } else {
            DragTableError::NonPhysical("induced curvature or energy")
        });
    }
    let split = sample(middle_cl)?;
    refine_piece(sample, [low, split], design, checking, depth + 1, pieces)?;
    refine_piece(sample, [split, high], design, checking, depth + 1, pieces)
}

fn checked_point([c0, c1, c2]: [f64; 3], (cl, vlm_cd_induced): InducedNode) -> InducedCheck {
    InducedCheck {
        cl,
        vlm_cd_induced,
        quadratic_cd_induced: c0 + cl * (c1 + cl * c2),
    }
}

fn valid_quadratic([c0, c1, c2]: [f64; 3], low: f64, high: f64) -> bool {
    let minimum_cl = (-c1 / (2.0 * c2)).clamp(low, high);
    c2 > 0.0 && c0 + minimum_cl * (c1 + minimum_cl * c2) >= 0.0
}

pub(super) fn trimmed_node(
    aero: &AeroAnalysis<'_>,
    design: &DesignTrim,
    cl_target: f64,
) -> Result<InducedNode, DragTableError> {
    let t = &design.trim;
    if cl_target == design.cl
        && design.cm_residual.abs() <= design.cm_tolerance.min(NODE_RESIDUAL_TOLERANCE)
    {
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
            .trimmed_inviscid(&point, design.mach, design.altitude_m)
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
        if r_cm.abs() <= design.cm_tolerance.min(NODE_RESIDUAL_TOLERANCE)
            && r_cl.abs() <= NODE_RESIDUAL_TOLERANCE
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
