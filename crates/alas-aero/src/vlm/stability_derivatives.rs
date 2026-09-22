// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Stability derivatives for ALAS's in-process vortex-lattice model.
// Numerical provenance is recorded in docs/PORTING.md and the repository's
// third-party notice.

//! The central-difference
//! stability-derivative sweep layered on top of [`super::run`].
//!
//! # Scope
//!
//! The one call site that reaches this is `alas/physics/dynamics.py`'s
//! `compute_dynamic_modes`, which calls
//! `run_with_stability_derivatives(alpha=True, beta=True, p=True, q=True, r=True)`,
//! every axis on. Upstream's five boolean flags exist only so a caller that
//! needs, say, only the longitudinal derivatives can skip the lateral solves
//! for speed; the sole caller in this program's inputs wants all five, so they
//! are not translated as parameters and this always computes the full set.
//! `docs/PORTING.md` records the scoping.
//!
//! # What it computes
//!
//! One central difference per state variable: run the base point once, then
//! re-run each of `alpha`, `beta`, `p`, `q`, `r` at both sides of a small
//! perturbation and take `(plus - minus) / (2 step) * scale`
//! for each of the six force/moment coefficients. `alpha`/`beta` are perturbed
//! by 0.001 degree and scaled by `degrees(1)` so the reported slope is
//! per-radian; `p`/`q`/`r` are perturbed and scaled by the same nondimensional
//! rate factor `(2 V) / b_ref` (or `c_ref` for `q`), so the reported slope is
//! with respect to the nondimensional rate `p_hat = p b / (2 V)` and its
//! kin, exactly the derivatives `flight_dynamics.get_modes` expects. It also
//! reports the longitudinal and lateral neutral points `x_np`/`x_np_lateral`,
//! which upstream appends after the `alpha` and `beta` passes.
//!
//! The step policy is explicit and can be varied with
//! [`run_with_stability_derivatives_with_steps`] for a refinement study; the
//! default entry point uses the historical 0.001-degree/0.001-rate scale.

use alas_geom::aircraft::airplane::Airplane;

use super::{VlmError, VlmResult, VlmSystem};
use crate::operating_point::OperatingPoint;

/// The finite-difference step upstream perturbs `alpha` and `beta` by, in
/// degrees: `finite_difference_amounts["alpha"]`/`["beta"]`.
const ANGLE_STEP_DEG: f64 = 0.001;

/// The nondimensional-rate step multiplier upstream perturbs `p`, `q`, `r` by,
/// before dividing by the reference length: the `0.001` in
/// `0.001 * (2 * velocity) / b_ref`.
const RATE_STEP_FRACTION: f64 = 0.001;

/// Which point the rotation-induced velocity is evaluated about: the product
/// convention (`airplane.xyz_ref`, as [`super::run`]) or the frozen reference
/// convention (the geometry origin, as [`super::run_reference_compatibility`]).
#[derive(Clone, Copy)]
enum RotationReference {
    Product,
    Origin,
}

impl RotationReference {
    fn solve(
        self,
        system: &VlmSystem<'_>,
        op_point: &OperatingPoint,
    ) -> Result<VlmResult, VlmError> {
        match self {
            Self::Product => system.solve(op_point),
            Self::Origin => system.solve_reference_compatibility(op_point),
        }
    }
}

/// The run and finite-difference choices for one derivative sweep.
#[derive(Clone, Copy)]
struct DerivativePolicy {
    angle_step_deg: f64,
    rate_step_fraction: f64,
    rotation: RotationReference,
    central_difference: bool,
}

/// The six force- and moment-coefficient derivatives with respect to one state
/// variable: upstream's `{CL,CD,CY,Cl,Cm,Cn}` + the denominator's
/// abbreviation (`CLa`, `CDa`, ... for the `alpha` pass, and so on). Field
/// names match [`VlmResult`]'s coefficient names, since each is the derivative
/// of the like-named coefficient.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CoefficientDerivatives {
    /// `d CL / d(state)`: lift-coefficient slope.
    pub cl_lift: f64,
    /// `d CD / d(state)`: drag-coefficient slope.
    pub cd_drag: f64,
    /// `d CY / d(state)`: side-force-coefficient slope.
    pub cy_side: f64,
    /// `d Cl / d(state)`: rolling-moment-coefficient slope.
    pub cl_roll: f64,
    /// `d Cm / d(state)`: pitching-moment-coefficient slope.
    pub cm_pitch: f64,
    /// `d Cn / d(state)`: yawing-moment-coefficient slope.
    pub cn_yaw: f64,
}

/// The base [`super::run`] result plus the stability derivatives with respect
/// to each state variable and the two neutral points: the superset dict
/// `run_with_stability_derivatives` returns.
#[derive(Debug, Clone, PartialEq)]
pub struct VlmStabilityResult {
    /// The unperturbed [`super::run`] output.
    pub base: VlmResult,
    /// Derivatives with respect to angle of attack, per radian: the `*a`
    /// keys (`CLa`, `Cma`, ...).
    pub d_alpha: CoefficientDerivatives,
    /// Derivatives with respect to sideslip, per radian: the `*b` keys
    /// (`CYb`, `Cnb`, ...).
    pub d_beta: CoefficientDerivatives,
    /// Derivatives with respect to the nondimensional roll rate: the `*p`
    /// keys (`Clp`, ...).
    pub d_p: CoefficientDerivatives,
    /// Derivatives with respect to the nondimensional pitch rate: the `*q`
    /// keys (`Cmq`, ...).
    pub d_q: CoefficientDerivatives,
    /// Derivatives with respect to the nondimensional yaw rate: the `*r`
    /// keys (`Cnr`, `Clr`, ...).
    pub d_r: CoefficientDerivatives,
    /// Longitudinal neutral point `x_np = xyz_ref[0] - Cma (c_ref / CLa)`.
    pub x_np: f64,
    /// Lateral neutral point `x_np_lateral = xyz_ref[0] - Cnb (b_ref / CYb)`.
    pub x_np_lateral: f64,
}

/// The six coefficient derivatives from a base and a perturbed run, evaluated
/// in upstream's historical order as `(perturbed - base) / step * scale`.
fn forward_differences(
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

/// The six coefficient derivatives from symmetric perturbed runs, evaluated
/// in coefficient order as `(plus - minus) / (2 step) * scale`.
fn central_differences(
    plus: &VlmResult,
    minus: &VlmResult,
    step: f64,
    scale: f64,
) -> CoefficientDerivatives {
    let slope = |after: f64, before: f64| (after - before) / (2.0 * step) * scale;
    CoefficientDerivatives {
        cl_lift: slope(plus.cl_lift, minus.cl_lift),
        cd_drag: slope(plus.cd_drag, minus.cd_drag),
        cy_side: slope(plus.cy_side, minus.cy_side),
        cl_roll: slope(plus.cl_roll, minus.cl_roll),
        cm_pitch: slope(plus.cm_pitch, minus.cm_pitch),
        cn_yaw: slope(plus.cn_yaw, minus.cn_yaw),
    }
}

/// Evaluate one derivative with either the historical forward difference or
/// the product central difference, depending on whether `minus_point` is
/// supplied.
struct DerivativeEvaluation<'a> {
    system: &'a VlmSystem<'a>,
    rotation: RotationReference,
    base: &'a VlmResult,
}

impl DerivativeEvaluation<'_> {
    fn evaluate(
        &self,
        plus_point: &OperatingPoint,
        minus_point: Option<&OperatingPoint>,
        step: f64,
        scale: f64,
    ) -> Result<CoefficientDerivatives, VlmError> {
        let plus = self.rotation.solve(self.system, plus_point)?;
        match minus_point {
            Some(minus_point) => {
                let minus = self.rotation.solve(self.system, minus_point)?;
                Ok(central_differences(&plus, &minus, step, scale))
            }
            None => Ok(forward_differences(self.base, &plus, step, scale)),
        }
    }
}

/// Run a vortex-lattice solve of `airplane` at `op_point` and the stability
/// derivatives about it: `VortexLatticeMethod(...).run_with_stability_derivatives()`
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
    run_with_stability_derivatives_with_steps(
        airplane,
        op_point,
        spanwise_resolution,
        chordwise_resolution,
        ANGLE_STEP_DEG,
        RATE_STEP_FRACTION,
    )
}

/// Run stability derivatives with an explicit finite-difference step policy.
///
/// This is the refinement-study seam for derivative consumers: callers can
/// compare the default steps with half/double steps without changing the
/// solver's state or silently relying on one hard-coded perturbation.
pub fn run_with_stability_derivatives_with_steps(
    airplane: &Airplane,
    op_point: &OperatingPoint,
    spanwise_resolution: usize,
    chordwise_resolution: usize,
    angle_step_deg: f64,
    rate_step_fraction: f64,
) -> Result<VlmStabilityResult, VlmError> {
    run_with_stability_derivatives_options(
        airplane,
        op_point,
        spanwise_resolution,
        chordwise_resolution,
        DerivativePolicy {
            angle_step_deg,
            rate_step_fraction,
            rotation: RotationReference::Product,
            central_difference: true,
        },
    )
}

/// Run the frozen reference stability-derivative convention used by the
/// AeroSandbox fixtures.
///
/// The translated reference solver uses the geometry origin for rotational
/// velocity and a forward finite difference. Product callers should use
/// [`run_with_stability_derivatives`], which uses the product rotation
/// reference and central differences. This seam is for frozen parity fixtures
/// only.
pub fn run_with_stability_derivatives_reference_compatibility(
    airplane: &Airplane,
    op_point: &OperatingPoint,
    spanwise_resolution: usize,
    chordwise_resolution: usize,
) -> Result<VlmStabilityResult, VlmError> {
    run_with_stability_derivatives_options(
        airplane,
        op_point,
        spanwise_resolution,
        chordwise_resolution,
        DerivativePolicy {
            angle_step_deg: ANGLE_STEP_DEG,
            rate_step_fraction: RATE_STEP_FRACTION,
            rotation: RotationReference::Origin,
            central_difference: false,
        },
    )
}

fn run_with_stability_derivatives_options(
    airplane: &Airplane,
    op_point: &OperatingPoint,
    spanwise_resolution: usize,
    chordwise_resolution: usize,
    policy: DerivativePolicy,
) -> Result<VlmStabilityResult, VlmError> {
    let angle_step_deg = policy.angle_step_deg;
    let rate_step_fraction = policy.rate_step_fraction;
    if !angle_step_deg.is_finite()
        || angle_step_deg <= 0.0
        || !rate_step_fraction.is_finite()
        || rate_step_fraction <= 0.0
    {
        return Err(VlmError::InvalidDerivativeStep);
    }
    // One mesh and one factorization serve the base point and every
    // perturbation: the stencil changes the operating point, never the
    // geometry.
    let system = VlmSystem::assemble(airplane, spanwise_resolution, chordwise_resolution)?;
    let base = policy.rotation.solve(&system, op_point)?;

    // Upstream's step sizes and scale factors, transcribed. The angle steps
    // are in degrees (op_point.alpha/beta are degrees); the rate steps and
    // their matching scales are the nondimensional-rate factor (2 V) / L.
    let velocity = op_point.velocity;
    let rate_scale_span = (2.0 * velocity) / airplane.b_ref;
    let rate_scale_chord = (2.0 * velocity) / airplane.c_ref;
    if !velocity.is_finite()
        || velocity <= 0.0
        || !airplane.b_ref.is_finite()
        || airplane.b_ref <= 0.0
        || !airplane.c_ref.is_finite()
        || airplane.c_ref <= 0.0
        || !rate_scale_span.is_finite()
        || !rate_scale_span.is_sign_positive()
        || !rate_scale_chord.is_finite()
        || !rate_scale_chord.is_sign_positive()
    {
        return Err(VlmError::InvalidDerivativeStep);
    }
    // `np.degrees(1)`: one radian in degrees, so a per-degree slope becomes
    // per-radian.
    let angle_scale = 1.0_f64.to_degrees();
    let p_step = rate_step_fraction * rate_scale_span;
    let q_step = rate_step_fraction * rate_scale_chord;
    let r_step = rate_step_fraction * rate_scale_span;

    // One perturbed solve per state variable. Each starts from a copy of the
    // base operating point with a single field incremented, exactly as
    // upstream's `copy.copy(original_op_point)` + one `__setattr__` does.
    let mut alpha_point = *op_point;
    alpha_point.alpha += angle_step_deg;
    let mut alpha_minus_point = *op_point;
    alpha_minus_point.alpha -= angle_step_deg;
    let mut beta_point = *op_point;
    beta_point.beta += angle_step_deg;
    let mut beta_minus_point = *op_point;
    beta_minus_point.beta -= angle_step_deg;
    let mut p_point = *op_point;
    p_point.p += p_step;
    let mut p_minus_point = *op_point;
    p_minus_point.p -= p_step;
    let mut q_point = *op_point;
    q_point.q += q_step;
    let mut q_minus_point = *op_point;
    q_minus_point.q -= q_step;
    let mut r_point = *op_point;
    r_point.r += r_step;
    let mut r_minus_point = *op_point;
    r_minus_point.r -= r_step;

    let alpha_minus = if policy.central_difference {
        Some(&alpha_minus_point)
    } else {
        None
    };
    let beta_minus = if policy.central_difference {
        Some(&beta_minus_point)
    } else {
        None
    };
    let p_minus = if policy.central_difference {
        Some(&p_minus_point)
    } else {
        None
    };
    let q_minus = if policy.central_difference {
        Some(&q_minus_point)
    } else {
        None
    };
    let r_minus = if policy.central_difference {
        Some(&r_minus_point)
    } else {
        None
    };

    let evaluation = DerivativeEvaluation {
        system: &system,
        rotation: policy.rotation,
        base: &base,
    };
    let d_alpha = evaluation.evaluate(&alpha_point, alpha_minus, angle_step_deg, angle_scale)?;
    let d_beta = evaluation.evaluate(&beta_point, beta_minus, angle_step_deg, angle_scale)?;
    let d_p = evaluation.evaluate(&p_point, p_minus, p_step, rate_scale_span)?;
    let d_q = evaluation.evaluate(&q_point, q_minus, q_step, rate_scale_chord)?;
    let d_r = evaluation.evaluate(&r_point, r_minus, r_step, rate_scale_span)?;

    // Upstream appends the neutral points inside the alpha and beta passes:
    // x_np after alpha, x_np_lateral after beta.
    let x_np = if d_alpha.cl_lift.abs() > 1.0e-12 {
        airplane.xyz_ref[0] - (d_alpha.cm_pitch * (airplane.c_ref / d_alpha.cl_lift))
    } else {
        f64::NAN
    };
    let x_np_lateral = if d_beta.cy_side.abs() > 1.0e-12 {
        airplane.xyz_ref[0] - (d_beta.cn_yaw * (airplane.b_ref / d_beta.cy_side))
    } else {
        f64::NAN
    };

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
