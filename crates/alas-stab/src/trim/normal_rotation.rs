// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The trim iteration on one influence matrix, settled on the exact lattice.
//!
//! A stabilizer incidence change moves the tail's horseshoes, so rebuilding
//! the lattice at every trial incidence refills the tail's rows and columns
//! of the influence matrix (a few hundred of a thousand panels on a
//! transport) and refactors it, at every Newton iteration and for every
//! finite-difference step. To first order the incidence enters only the
//! flow-tangency condition: the panel normals turn about the pitch axis
//! while the vortex positions move by a second-order amount (Katz and
//! Plotkin, *Low-Speed Aerodynamics*, 2nd ed., ch. 12, small-angle boundary
//! condition). The iteration therefore runs in two stages.
//!
//! 1. Newton on the reference lattice assembled at the starting incidence,
//!    the incidence change applied as a rotation of the tail's normals
//!    ([`alas_aero::vlm::VlmSystem::solve_with_surface_pitch`]): every trial
//!    is a back-substitution. The finite-difference Jacobian of this stage
//!    is kept.
//! 2. Defect correction on the exactly rebuilt lattice: each trial
//!    incidence is assembled as before and its residual is the reported
//!    one; the step uses the stage-1 Jacobian, updated by Broyden's rank-one
//!    formula from the exact residuals. The iteration stops only when the
//!    exact residual meets [`EXACT_TOLERANCE`], tighter than the gate of
//!    the exact Newton iteration, so the trim is a root of the exact lattice
//!    residual, not of the rotated-normal model.
//!
//! Anything unexpected (a non-finite value, a singular Jacobian, no
//! convergence in the iteration budget) returns `None` and the caller runs
//! the exact Newton iteration from the same start.

use super::*;

/// Stage-1 residual tolerance on the rotated-normal model.
const MODEL_TOLERANCE: f64 = 1.0e-10;
/// Stage-2 residual tolerance on the exact lattice, one order inside the
/// `1e-7` of the exact Newton iteration so the settled trim does not depend
/// on where in the tolerance band the iteration happened to stop.
const EXACT_TOLERANCE: f64 = 1.0e-8;
const MODEL_ITERATIONS: usize = 8;
const EXACT_ITERATIONS: usize = 6;
const ALPHA_STEP_DEG: f64 = 0.1;
const INCIDENCE_STEP_DEG: f64 = 0.1;
const MAX_UPDATE_DEG: f64 = 5.0;

type Jacobian = [[f64; 2]; 2];

/// Solve `j * d = f`; `None` for a singular or non-finite system.
fn solve2(j: &Jacobian, f: [f64; 2]) -> Option<[f64; 2]> {
    let det = j[0][0] * j[1][1] - j[0][1] * j[1][0];
    let scale = j.iter().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
    if !det.is_finite() || det.abs() <= 1.0e-12 * scale * scale {
        return None;
    }
    let d = [
        (f[0] * j[1][1] - j[0][1] * f[1]) / det,
        (j[0][0] * f[1] - f[0] * j[1][0]) / det,
    ];
    (d[0].is_finite() && d[1].is_finite()).then_some(d)
}

/// The limited Newton update `(d_alpha, d_incidence)` for the step `d`.
fn limited(d: [f64; 2]) -> [f64; 2] {
    let scale = 1.0_f64
        .min(MAX_UPDATE_DEG / d[0].abs().max(1.0e-12))
        .min(MAX_UPDATE_DEG / d[1].abs().max(1.0e-12));
    [scale * d[0], scale * d[1]]
}

/// The trim of `airplane` near `(alpha_deg, incidence_deg)` by the two
/// stages above, or `None` to leave it to the exact Newton iteration.
#[allow(clippy::too_many_arguments)] // one named physical input each, as `refine_trim`
pub(super) fn refine(
    airplane: &Airplane,
    assembly: &TrimAssembly<'_>,
    cl_target: f64,
    atmosphere: Atmosphere,
    velocity: f64,
    mut alpha_deg: f64,
    mut incidence_deg: f64,
    body_cm_per_deg: f64,
    jacobian_at_trim: bool,
) -> Result<Option<RefinedTrim>, VlmError> {
    let tails: Vec<usize> = airplane
        .wings
        .iter()
        .enumerate()
        .filter(|(_, wing)| wing.name == HSTAB_NAME)
        .map(|(index, _)| index)
        .collect();
    if tails.is_empty() || !alpha_deg.is_finite() || !incidence_deg.is_finite() {
        return Ok(None);
    }
    let op =
        |alpha_deg: f64| OperatingPoint::new(atmosphere, velocity, alpha_deg, 0.0, 0.0, 0.0, 0.0);

    // Stage 1: the reference lattice at the starting incidence.
    let reference_incidence = incidence_deg;
    let mut model_jacobian: Jacobian = [[0.0; 2]; 2];
    {
        let plane = with_hstab_incidence(airplane, reference_incidence);
        let reference = assembly.assemble(&plane)?;
        let model = |alpha_deg: f64, incidence_deg: f64| -> Result<[f64; 2], VlmError> {
            let pitch = (incidence_deg - reference_incidence).to_radians();
            let solved = reference.solve_with_surface_pitch(&op(alpha_deg), &tails, pitch)?;
            Ok([
                solved.cl_lift - cl_target,
                solved.cm_pitch + body_cm_per_deg * alpha_deg,
            ])
        };
        for iteration in 0..=MODEL_ITERATIONS {
            let f = model(alpha_deg, incidence_deg)?;
            let f_alpha = model(alpha_deg + ALPHA_STEP_DEG, incidence_deg)?;
            let f_incidence = model(alpha_deg, incidence_deg + INCIDENCE_STEP_DEG)?;
            model_jacobian = [
                [
                    (f_alpha[0] - f[0]) / ALPHA_STEP_DEG,
                    (f_incidence[0] - f[0]) / INCIDENCE_STEP_DEG,
                ],
                [
                    (f_alpha[1] - f[1]) / ALPHA_STEP_DEG,
                    (f_incidence[1] - f[1]) / INCIDENCE_STEP_DEG,
                ],
            ];
            if !f
                .iter()
                .chain(model_jacobian.iter().flatten())
                .all(|v| v.is_finite())
            {
                return Ok(None);
            }
            if f[0].abs().max(f[1].abs()) <= MODEL_TOLERANCE {
                break;
            }
            if iteration == MODEL_ITERATIONS {
                return Ok(None);
            }
            let Some(d) = solve2(&model_jacobian, f) else {
                return Ok(None);
            };
            let [d_alpha, d_incidence] = limited(d);
            alpha_deg -= d_alpha;
            incidence_deg -= d_incidence;
        }
    }

    // Stage 2: defect correction on the exactly rebuilt lattice.
    let mut jacobian = model_jacobian;
    let mut previous: Option<([f64; 2], [f64; 2])> = None;
    for _ in 0..=EXACT_ITERATIONS {
        let plane = with_hstab_incidence(airplane, incidence_deg);
        let system = assembly.assemble(&plane)?;
        let base = probe(&system, atmosphere, velocity, alpha_deg)?;
        let f = [
            base.cl_lift - cl_target,
            base.cm_pitch + body_cm_per_deg * alpha_deg,
        ];
        if !f.iter().all(|v| v.is_finite()) {
            return Ok(None);
        }
        if f[0].abs().max(f[1].abs()) <= EXACT_TOLERANCE {
            let derivatives = if jacobian_at_trim {
                let alpha_probe = probe(&system, atmosphere, velocity, alpha_deg + ALPHA_STEP_DEG)?;
                drop(system);
                let turned = with_hstab_incidence(airplane, incidence_deg + INCIDENCE_STEP_DEG);
                let incidence_probe = probe(
                    &assembly.assemble(&turned)?,
                    atmosphere,
                    velocity,
                    alpha_deg,
                )?;
                Some([
                    [
                        (alpha_probe.cl_lift - base.cl_lift) / ALPHA_STEP_DEG,
                        (incidence_probe.cl_lift - base.cl_lift) / INCIDENCE_STEP_DEG,
                    ],
                    [
                        (alpha_probe.cm_pitch - base.cm_pitch) / ALPHA_STEP_DEG + body_cm_per_deg,
                        (incidence_probe.cm_pitch - base.cm_pitch) / INCIDENCE_STEP_DEG,
                    ],
                ])
                .filter(|rows: &Jacobian| rows.iter().flatten().all(|v| v.is_finite()))
            } else {
                None
            };
            return Ok(Some(RefinedTrim {
                alpha_deg,
                incidence_deg,
                converged: true,
                jacobian: derivatives,
            }));
        }
        drop(system);
        let x = [alpha_deg, incidence_deg];
        if let Some((x_before, f_before)) = previous {
            // Broyden's rank-one update from the two exact residuals.
            let dx = [x[0] - x_before[0], x[1] - x_before[1]];
            let df = [f[0] - f_before[0], f[1] - f_before[1]];
            let norm = dx[0] * dx[0] + dx[1] * dx[1];
            if norm > 1.0e-24 {
                let predicted = [
                    jacobian[0][0] * dx[0] + jacobian[0][1] * dx[1],
                    jacobian[1][0] * dx[0] + jacobian[1][1] * dx[1],
                ];
                for row in 0..2 {
                    let miss = (df[row] - predicted[row]) / norm;
                    jacobian[row][0] += miss * dx[0];
                    jacobian[row][1] += miss * dx[1];
                }
            }
        }
        previous = Some((x, f));
        let Some(d) = solve2(&jacobian, f) else {
            return Ok(None);
        };
        let [d_alpha, d_incidence] = limited(d);
        alpha_deg -= d_alpha;
        incidence_deg -= d_incidence;
        if !alpha_deg.is_finite() || !incidence_deg.is_finite() {
            return Ok(None);
        }
    }
    Ok(None)
}
