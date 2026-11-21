// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Checked quadratic segments of trimmed Trefftz-plane wake energy. The
//! trim Jacobian places each solve; independent checks refine the CL range
//! when its finite-angle geometry cannot be represented by one parabola.

use std::sync::Arc;

use alas_aero::analysis::{AeroAnalysis, TrimPoint, TrimmedInviscid};
use alas_geom::aircraft::airplane::Airplane;
use alas_stab::trim::StabilityTrimResult;

/// The attitude and Jacobian a trim node starts from, per degree.
struct TrimAttitude {
    alpha_deg: f64,
    incidence_deg: f64,
    cl_alpha: f64,
    cl_ih: f64,
    cm_alpha: f64,
    cm_ih: f64,
}

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

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
pub(super) struct InducedCurve {
    pub(super) pieces: Vec<QuadraticPiece>,
    /// `(CL, alpha)` of every trimmed node solved for the curve, deg,
    /// ascending in CL: the body attitude of the trimmed airplane.
    pub(super) attitude: Vec<(f64, f64)>,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq)]
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

    /// Smallest value of the curve over its CL range: the wake energy of the
    /// zero-lift (basic) span loading, which no lift removes.
    pub(super) fn minimum(&self) -> f64 {
        self.pieces
            .iter()
            .map(|piece| {
                let [c0, c1, c2] = piece.coefficients;
                let cl = (-c1 / (2.0 * c2)).clamp(piece.low, piece.high);
                c0 + cl * (c1 + cl * c2)
            })
            .fold(f64::INFINITY, f64::min)
    }

    /// The curve with the wake energy of its basic loading ([`Self::minimum`])
    /// replaced by the non-optimum-twist allowance `factor CL^2`
    /// (`alas_aero::analysis::NON_OPTIMUM_TWIST_DRAG_FACTOR`), and the value
    /// removed. The lift-proportional loading and the trim load stay the
    /// lattice's.
    pub(super) fn with_twist_allowance(mut self, factor: f64) -> (Self, f64) {
        let basic = self.minimum();
        if !basic.is_finite() {
            return (self, 0.0);
        }
        for piece in &mut self.pieces {
            piece.coefficients[0] -= basic;
            piece.coefficients[2] += factor;
        }
        (self, basic)
    }

    pub(super) fn ranges(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        self.pieces.iter().map(|piece| (piece.low, piece.high))
    }

    /// Trimmed body angle of attack at `cl` and its CL slope, deg, through
    /// the solved nodes ([`super::upsweep::monotone_attitude`]); `None` with
    /// fewer than two distinct nodes.
    pub(super) fn alpha_and_slope(&self, cl: f64) -> Option<(f64, f64)> {
        super::upsweep::monotone_attitude(&self.attitude, cl)
    }
}

/// The fourth trimmed solve and what the quadratic predicts there.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq)]
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

/// The Goethert-stretched lattice the compressible trim ran on: the trim
/// nodes of the table are solved there when the trim's attitude is the
/// compressible one, so the induced and trim drag carry the Mach number.
#[derive(Debug, Clone)]
pub(crate) struct StretchedBasis {
    /// `goethert_stretch(plane, beta)` of the candidate (at its trim CG).
    pub plane: Airplane,
    /// The Prandtl-Glauert factor of the cruise Mach.
    pub beta: f64,
    /// The airframe moment slope the compressible trim solved against, per
    /// degree (`TrimAttitudeView::cm_body_per_deg`).
    pub cm_body_per_deg: f64,
}

/// The cruise trim a table is built around: what `trim_and_polar` solved.
#[derive(Debug, Clone)]
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
    /// The stretched lattice the nodes are solved on when the trim ran
    /// compressible; `None` keeps the incompressible lattice (the opt-out and
    /// reference paths).
    pub stretched: Option<Arc<StretchedBasis>>,
    /// The same cruise state solved on the incompressible lattice, so the
    /// table can blend toward it at low Mach; `None` when `stretched` is.
    pub incompressible: Option<Box<DesignTrim>>,
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
    let mut samples = Vec::<(f64, InducedNode, f64)>::new();
    let mut states = Vec::<NodeState>::new();
    let mut sample = |cl_target: f64| -> Result<InducedNode, DragTableError> {
        if let Some((_, point, _)) = samples.iter().find(|(cl, ..)| *cl == cl_target) {
            return Ok(*point);
        }
        let (point, state) = solve_node(aero, design, cl_target, &states)?;
        samples.push((cl_target, point, state.alpha_deg));
        states.push(state);
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
    let mut attitude: Vec<(f64, f64)> = samples
        .iter()
        .map(|&(_, (cl, _), alpha_deg)| (cl, alpha_deg))
        .collect();
    attitude.sort_by(|left, right| left.0.total_cmp(&right.0));
    attitude.dedup_by(|right, left| right.0 == left.0);
    let curve = InducedCurve { pieces, attitude };
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

#[cfg(test)]
pub(super) fn trimmed_node(
    aero: &AeroAnalysis<'_>,
    design: &DesignTrim,
    cl_target: f64,
) -> Result<InducedNode, DragTableError> {
    solve_node(aero, design, cl_target, &[]).map(|(node, _)| node)
}

/// A converged trim node: its attitude and the local Jacobian its Newton
/// iteration ended on, the start of the neighbouring nodes of the same curve.
#[derive(Debug, Clone, Copy)]
pub(super) struct NodeState {
    pub(super) cl: f64,
    pub(super) alpha_deg: f64,
    incidence_deg: f64,
    /// `[[dCL/da, dCL/dih], [dCm/da, dCm/dih]]`, per degree.
    jacobian: [[f64; 2]; 2],
}

/// The Lagrange polynomial through `points` at `x`.
fn lagrange(points: &[(f64, f64)], x: f64) -> f64 {
    points
        .iter()
        .enumerate()
        .map(|(i, &(xi, yi))| {
            let weight: f64 = points
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, &(xj, _))| (x - xj) / (xi - xj))
                .product();
            yi * weight
        })
        .sum()
}

/// [`solve_node_from`] with the solved nodes of the curve as the start; a
/// start that fails (a trial attitude on which the lattice is too
/// ill-conditioned to trust, say) is dropped for the cold start from the
/// cruise trim, so a warm start never turns a solvable node into an error.
pub(super) fn solve_node(
    aero: &AeroAnalysis<'_>,
    design: &DesignTrim,
    cl_target: f64,
    solved: &[NodeState],
) -> Result<((f64, f64), NodeState), DragTableError> {
    if !solved.is_empty() {
        if let Ok(node) = solve_node_from(aero, design, cl_target, solved) {
            return Ok(node);
        }
    }
    solve_node_from(aero, design, cl_target, &[])
}

/// The trimmed node at `cl_target`, started from the nodes already `solved`
/// on this curve.
///
/// Every node of a curve is a root of the same smooth map from lift
/// coefficient to attitude, so a target between solved nodes starts from the
/// quadratic (at most three nearest nodes) through their attitudes, and any
/// other target from the nearest node and the Jacobian its own iteration
/// ended on, not from the cruise trim at the far end of the CL range. The
/// start moves no root: the iteration below still ends only when the lattice
/// itself meets the node tolerances, each trial one exact lattice solve.
fn solve_node_from(
    aero: &AeroAnalysis<'_>,
    design: &DesignTrim,
    cl_target: f64,
    solved: &[NodeState],
) -> Result<((f64, f64), NodeState), DragTableError> {
    // The attitude and Jacobian of the lattice the node is solved on: the
    // compressible trim's when the nodes are on the stretched lattice, else
    // the incompressible trim's own.
    let t = if design.stretched.is_some() {
        let view = design.trim.attitude();
        TrimAttitude {
            alpha_deg: view.alpha_deg,
            incidence_deg: view.incidence_deg,
            cl_alpha: view.cl_alpha,
            cl_ih: view.cl_ih,
            cm_alpha: view.cm_alpha,
            cm_ih: view.cm_ih,
        }
    } else {
        let inc = &design.trim;
        TrimAttitude {
            alpha_deg: inc.trim_alpha_deg,
            incidence_deg: inc.trim_ih_deg,
            cl_alpha: inc.cl_alpha,
            cl_ih: inc.cl_ih,
            cm_alpha: inc.cm_alpha,
            cm_ih: inc.cm_ih,
        }
    };
    let design_state = NodeState {
        cl: design.cl,
        alpha_deg: t.alpha_deg,
        incidence_deg: t.incidence_deg,
        jacobian: [[t.cl_alpha, t.cl_ih], [t.cm_alpha, t.cm_ih]],
    };
    if cl_target == design.cl
        && design.cm_residual.abs() <= design.cm_tolerance.min(NODE_RESIDUAL_TOLERANCE)
    {
        return Ok(((design.cl, design.cd_induced), design_state));
    }
    let solve = |alpha: f64, incidence: f64| -> Result<TrimmedInviscid, DragTableError> {
        let point = TrimPoint {
            trim_alpha_deg: alpha,
            trim_ih_deg: incidence,
            cl_alpha: t.cl_alpha,
        };
        match &design.stretched {
            Some(basis) => aero.trimmed_inviscid_stretched(
                &basis.plane,
                &point,
                Some(t.incidence_deg),
                design.mach,
                design.altitude_m,
                basis.beta,
                basis.cm_body_per_deg,
            ),
            None => aero.trimmed_inviscid(
                &point,
                Some(t.incidence_deg),
                design.mach,
                design.altitude_m,
            ),
        }
        .map_err(|error| DragTableError::Solve(error.to_string()))
    };
    let tolerance = design.cm_tolerance.min(NODE_RESIDUAL_TOLERANCE);
    let finished = |perf: &TrimmedInviscid, r_cl: f64, r_cm: f64| {
        r_cm.abs() <= tolerance
            && r_cl.abs() <= NODE_RESIDUAL_TOLERANCE
            && perf.cl.is_finite()
            && perf.cd_induced.is_finite()
    };

    // The solved nodes and the cruise trim, by distance in lift coefficient.
    let mut known: Vec<NodeState> = solved.to_vec();
    known.push(design_state);
    known.sort_by(|a, b| {
        (a.cl - cl_target)
            .abs()
            .total_cmp(&(b.cl - cl_target).abs())
    });
    known.dedup_by(|a, b| (a.cl - b.cl).abs() < 1.0e-9);
    let nearest = known[0];
    let below = known.iter().any(|k| k.cl < cl_target);
    let above = known.iter().any(|k| k.cl > cl_target);

    let (mut alpha, mut incidence) = (nearest.alpha_deg, nearest.incidence_deg);
    let mut first_estimate = None;
    let (mut r_cl, mut r_cm) = if nearest.cl == design.cl {
        (design.cl - cl_target, design.cm_residual)
    } else {
        (nearest.cl - cl_target, 0.0)
    };
    // The nearest node's Jacobian, `[[dCL/da, dCL/dih], [dCm/da, dCm/dih]]`
    // per degree.
    let mut jacobian = nearest.jacobian;
    if below && above && known.len() >= 2 {
        // Between solved nodes: start on the quadratic through the nearest
        // three and let the lattice give the residual there.
        let take = known.len().min(3);
        let alphas: Vec<(f64, f64)> = known[..take].iter().map(|k| (k.cl, k.alpha_deg)).collect();
        let incidences: Vec<(f64, f64)> = known[..take]
            .iter()
            .map(|k| (k.cl, k.incidence_deg))
            .collect();
        let (a0, i0) = (
            lagrange(&alphas, cl_target),
            lagrange(&incidences, cl_target),
        );
        if a0.is_finite() && i0.is_finite() {
            let perf = solve(a0, i0)?;
            first_estimate = Some((a0, i0));
            alpha = a0;
            incidence = i0;
            r_cl = perf.cl - cl_target;
            r_cm = perf.cm_residual;
            if finished(&perf, r_cl, r_cm) {
                let state = NodeState {
                    cl: perf.cl,
                    alpha_deg: alpha,
                    incidence_deg: incidence,
                    jacobian,
                };
                return Ok(((perf.cl, perf.cd_induced), state));
            }
        }
    }
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
        first_estimate.get_or_insert((alpha, incidence));
        let perf = solve(alpha, incidence)?;
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
        if finished(&perf, r_cl, r_cm) {
            let state = NodeState {
                cl: perf.cl,
                alpha_deg: alpha,
                incidence_deg: incidence,
                jacobian,
            };
            return Ok(((perf.cl, perf.cd_induced), state));
        }
    }
    // The Newton iteration has met a stretch where the moment does not follow
    // the incidence; the trim may lie beyond it.
    if let Some((start_alpha, start_incidence)) = first_estimate {
        let root = alas_stab::trim::bracketed_trim(
            |alpha, incidence| solve(alpha, incidence).map(|perf| (perf.cl, perf.cm_residual)),
            cl_target,
            start_alpha,
            start_incidence,
            tolerance,
        )?;
        if let Some((alpha, incidence)) = root {
            let perf = solve(alpha, incidence)?;
            if perf.cd_induced.is_finite() {
                let state = NodeState {
                    cl: perf.cl,
                    alpha_deg: alpha,
                    incidence_deg: incidence,
                    jacobian,
                };
                return Ok(((perf.cl, perf.cd_induced), state));
            }
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
