// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Stability derivatives for ALAS's in-process vortex-lattice model.
// Numerical provenance is recorded in docs/PORTING.md and the repository's
// third-party notice.

//! The forward-difference
//! stability-derivative sweep layered on top of [`super::run`].
//!
//! # Scope
//!
//! The one call site that reaches this is `alas/physics/dynamics.py`'s
//! `compute_dynamic_modes`, which calls
//! `run_with_stability_derivatives(alpha=True, beta=True, p=True, q=True, r=True)`
//! -- every axis on. Upstream's five boolean flags exist only so a caller that
//! needs, say, only the longitudinal derivatives can skip the lateral solves
//! for speed; the sole caller in this program's inputs wants all five, so they
//! are not translated as parameters and this always computes the full set.
//! `docs/PORTING.md` records the scoping.
//!
//! # What it computes
//!
//! One forward difference per state variable: run the base point once, then
//! re-run five times, each with one of `alpha`, `beta`, `p`, `q`, `r`
//! incremented by a small step, and take `(perturbed - base) / step * scale`
//! for each of the six force/moment coefficients. `alpha`/`beta` are perturbed
//! by 0.001 degree and scaled by `degrees(1)` so the reported slope is
//! per-radian; `p`/`q`/`r` are perturbed and scaled by the same nondimensional
//! rate factor `(2 V) / b_ref` (or `c_ref` for `q`), so the reported slope is
//! with respect to the nondimensional rate `p_hat = p b / (2 V)` and its
//! kin -- exactly the derivatives `flight_dynamics.get_modes` expects. It also
//! reports the longitudinal and lateral neutral points `x_np`/`x_np_lateral`,
//! which upstream appends after the `alpha` and `beta` passes.
//!
//! Nothing here is numerically new: it is [`super::run`] evaluated six times
//! and differenced, so it inherits that function's tier. See `docs/PORTING.md`.

use alas_geom::aircraft::airplane::Airplane;

use super::{run, VlmError, VlmResult};
use crate::operating_point::OperatingPoint;

/// The finite-difference step upstream perturbs `alpha` and `beta` by, in
/// degrees -- `finite_difference_amounts["alpha"]`/`["beta"]`.
const ANGLE_STEP_DEG: f64 = 0.001;

/// The nondimensional-rate step multiplier upstream perturbs `p`, `q`, `r` by,
/// before dividing by the reference length -- the `0.001` in
/// `0.001 * (2 * velocity) / b_ref`.
const RATE_STEP_FRACTION: f64 = 0.001;

/// The six force- and moment-coefficient derivatives with respect to one state
/// variable -- upstream's `{CL,CD,CY,Cl,Cm,Cn}` + the denominator's
/// abbreviation (`CLa`, `CDa`, ... for the `alpha` pass, and so on). Field
/// names match [`VlmResult`]'s coefficient names, since each is the derivative
/// of the like-named coefficient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoefficientDerivatives {
    /// `d CL / d(state)` -- lift-coefficient slope.
    pub cl_lift: f64,
    /// `d CD / d(state)` -- drag-coefficient slope.
    pub cd_drag: f64,
    /// `d CY / d(state)` -- side-force-coefficient slope.
    pub cy_side: f64,
    /// `d Cl / d(state)` -- rolling-moment-coefficient slope.
    pub cl_roll: f64,
    /// `d Cm / d(state)` -- pitching-moment-coefficient slope.
    pub cm_pitch: f64,
    /// `d Cn / d(state)` -- yawing-moment-coefficient slope.
    pub cn_yaw: f64,
}

/// The base [`super::run`] result plus the stability derivatives with respect
/// to each state variable and the two neutral points -- the superset dict
/// `run_with_stability_derivatives` returns.
#[derive(Debug, Clone, PartialEq)]
pub struct VlmStabilityResult {
    /// The unperturbed [`super::run`] output.
    pub base: VlmResult,
    /// Derivatives with respect to angle of attack, per radian -- the `*a`
    /// keys (`CLa`, `Cma`, ...).
    pub d_alpha: CoefficientDerivatives,
    /// Derivatives with respect to sideslip, per radian -- the `*b` keys
    /// (`CYb`, `Cnb`, ...).
    pub d_beta: CoefficientDerivatives,
    /// Derivatives with respect to the nondimensional roll rate -- the `*p`
    /// keys (`Clp`, ...).
    pub d_p: CoefficientDerivatives,
    /// Derivatives with respect to the nondimensional pitch rate -- the `*q`
    /// keys (`Cmq`, ...).
    pub d_q: CoefficientDerivatives,
    /// Derivatives with respect to the nondimensional yaw rate -- the `*r`
    /// keys (`Cnr`, `Clr`, ...).
    pub d_r: CoefficientDerivatives,
    /// Longitudinal neutral point `x_np = xyz_ref[0] - Cma (c_ref / CLa)`.
    pub x_np: f64,
    /// Lateral neutral point `x_np_lateral = xyz_ref[0] - Cnb (b_ref / CYb)`.
    pub x_np_lateral: f64,
}

/// The six coefficient derivatives from a base and a perturbed run, evaluated
/// in upstream's own order: `(perturbed - base) / step * scale`, left to right.
fn differences(
    base: &VlmResult,
    perturbed: &VlmResult,
    step: f64,
    scale: f64,
) -> CoefficientDerivatives {
    let slope = |after: f64, before: f64| (after - before) / step * scale;
    CoefficientDerivatives {
        cl_lift: slope(perturbed.cl_lift, base.cl_lift),
        cd_drag: slope(perturbed.cd_drag, base.cd_drag),
        cy_side: slope(perturbed.cy_side, base.cy_side),
        cl_roll: slope(perturbed.cl_roll, base.cl_roll),
        cm_pitch: slope(perturbed.cm_pitch, base.cm_pitch),
        cn_yaw: slope(perturbed.cn_yaw, base.cn_yaw),
    }
}

/// Run a vortex-lattice solve of `airplane` at `op_point` and the stability
/// derivatives about it -- `VortexLatticeMethod(...).run_with_stability_derivatives()`
/// with every axis flag `true`, the only way this program's inputs call it.
///
/// # Errors
///
/// See [`VlmError`]: any of the six underlying [`super::run`] solves can fail
/// the same way a single one can.
pub fn run_with_stability_derivatives(
    airplane: &Airplane,
    op_point: &OperatingPoint,
    spanwise_resolution: usize,
    chordwise_resolution: usize,
) -> Result<VlmStabilityResult, VlmError> {
    let base = run(
        airplane,
        op_point,
        spanwise_resolution,
        chordwise_resolution,
    )?;

    // Upstream's step sizes and scale factors, transcribed. The angle steps
    // are in degrees (op_point.alpha/beta are degrees); the rate steps and
    // their matching scales are the nondimensional-rate factor (2 V) / L.
    let velocity = op_point.velocity;
    let rate_scale_span = (2.0 * velocity) / airplane.b_ref;
    let rate_scale_chord = (2.0 * velocity) / airplane.c_ref;
    // `np.degrees(1)`: one radian in degrees, so a per-degree slope becomes
    // per-radian.
    let angle_scale = 1.0_f64.to_degrees();
    let p_step = RATE_STEP_FRACTION * rate_scale_span;
    let q_step = RATE_STEP_FRACTION * rate_scale_chord;
    let r_step = RATE_STEP_FRACTION * rate_scale_span;

    // One perturbed solve per state variable. Each starts from a copy of the
    // base operating point with a single field incremented, exactly as
    // upstream's `copy.copy(original_op_point)` + one `__setattr__` does.
    let mut alpha_point = *op_point;
    alpha_point.alpha += ANGLE_STEP_DEG;
    let mut beta_point = *op_point;
    beta_point.beta += ANGLE_STEP_DEG;
    let mut p_point = *op_point;
    p_point.p += p_step;
    let mut q_point = *op_point;
    q_point.q += q_step;
    let mut r_point = *op_point;
    r_point.r += r_step;

    let solve =
        |point: &OperatingPoint| run(airplane, point, spanwise_resolution, chordwise_resolution);

    let d_alpha = differences(&base, &solve(&alpha_point)?, ANGLE_STEP_DEG, angle_scale);
    let d_beta = differences(&base, &solve(&beta_point)?, ANGLE_STEP_DEG, angle_scale);
    let d_p = differences(&base, &solve(&p_point)?, p_step, rate_scale_span);
    let d_q = differences(&base, &solve(&q_point)?, q_step, rate_scale_chord);
    let d_r = differences(&base, &solve(&r_point)?, r_step, rate_scale_span);

    // Upstream appends the neutral points inside the alpha and beta passes:
    // x_np after alpha, x_np_lateral after beta.
    let x_np = airplane.xyz_ref[0] - (d_alpha.cm_pitch * (airplane.c_ref / d_alpha.cl_lift));
    let x_np_lateral = airplane.xyz_ref[0] - (d_beta.cn_yaw * (airplane.b_ref / d_beta.cy_side));

    Ok(VlmStabilityResult {
        base,
        d_alpha,
        d_beta,
        d_p,
        d_q,
        d_r,
        x_np,
        x_np_lateral,
    })
}
