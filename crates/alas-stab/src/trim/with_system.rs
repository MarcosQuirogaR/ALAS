// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! [`neutral_point`] and [`stability_and_trim`]'s variants that reuse an
//! already-assembled VLM system instead of assembling a fresh one, split
//! out of `super` purely to keep that assembled module under the
//! workspace's per-module line budget.
//!
//! `FullAnalysis::run` (`alas-pipeline`) solves the polar sweep, the
//! neutral-point probes and the trim probes on the same aircraft at the
//! same fine mesh; the influence matrix depends on the geometry and mesh
//! alone, so one factorization serves all three instead of each
//! assembling its own copy of an identical matrix.

use alas_aero::vlm::{VlmError, VlmGeometryCache, VlmSystem};
use alas_atmo::Atmosphere;
use alas_config::analysis::AnalysisConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_math::linalg;
use std::sync::{Arc, Mutex};

use super::{
    fuselage_cm_alpha_with_reference_mode, hstab, main_wing_ac_x, probe, refine_trim,
    with_hstab_twist, StabilityTrimResult, TrimAssembly, DEGENERACY_FLOOR,
};

/// [`super::neutral_point`], reusing an already-assembled VLM system instead
/// of assembling a fresh one.
///
/// `system` must be [`VlmSystem::assemble`]d from `airplane` at `analysis`'s
/// spanwise/chordwise resolution: the influence matrix depends on the
/// geometry and mesh alone, so a caller that already paid for that
/// factorization for the same aircraft (a polar sweep at the same fine mesh,
/// say) reuses it here instead of refactoring an identical matrix.
///
/// # Errors
///
/// See [`VlmError`].
pub fn neutral_point_with_system(
    system: &VlmSystem<'_>,
    airplane: &Airplane,
    analysis: &AnalysisConfig,
) -> Result<(f64, f64, f64), VlmError> {
    neutral_point_with_reference_mode(system, airplane, analysis, false)
}

/// [`super::neutral_point_reference_compatibility`], reusing an
/// already-assembled VLM system; see [`neutral_point_with_system`] for the
/// reuse contract.
///
/// # Errors
///
/// See [`VlmError`].
pub fn neutral_point_reference_compatibility_with_system(
    system: &VlmSystem<'_>,
    airplane: &Airplane,
    analysis: &AnalysisConfig,
) -> Result<(f64, f64, f64), VlmError> {
    neutral_point_with_reference_mode(system, airplane, analysis, true)
}

fn neutral_point_with_reference_mode(
    system: &VlmSystem<'_>,
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    reference_compatibility: bool,
) -> Result<(f64, f64, f64), VlmError> {
    let atmosphere = Atmosphere::new(0.0);
    let velocity = analysis.autobalance_velocity_m_s;
    let (a_lo, a_hi) = (
        analysis.autobalance_alpha_low_deg,
        analysis.autobalance_alpha_high_deg,
    );
    let r_lo = probe(system, atmosphere, velocity, a_lo)?;
    let r_hi = probe(system, atmosphere, velocity, a_hi)?;

    let d_cl = r_hi.cl_lift - r_lo.cl_lift;
    let d_cm = r_hi.cm_pitch - r_lo.cm_pitch;
    let d_alpha =
        (analysis.autobalance_alpha_high_deg - analysis.autobalance_alpha_low_deg).to_radians();
    let c_ref = airplane.c_ref.max(0.1);
    let x_cg = airplane.xyz_ref[0];

    if d_cl.abs() < DEGENERACY_FLOOR || d_alpha.abs() < DEGENERACY_FLOOR {
        return Ok((x_cg, f64::NAN, f64::NAN));
    }

    let cl_alpha = d_cl / d_alpha;

    let x_np = if reference_compatibility {
        let sm_vlm = -d_cm / d_cl;
        let x_np_vlm = x_cg + sm_vlm * c_ref;
        let x_wing_ac = main_wing_ac_x(airplane);
        let eta_t = analysis.tail_efficiency;
        let mut x_np = x_wing_ac + eta_t * (x_np_vlm - x_wing_ac);
        if analysis.include_fuselage_stability && cl_alpha > 0.1 {
            let cm_a_fus = fuselage_cm_alpha_with_reference_mode(airplane, cl_alpha, true);
            x_np -= cm_a_fus * c_ref / cl_alpha;
        }
        x_np
    } else {
        crate::neutral_point::product_neutral_point(
            airplane, analysis, atmosphere, velocity, a_lo, a_hi, &r_lo, &r_hi,
        )?
    };

    let sm = (x_np - x_cg) / c_ref;
    Ok((x_np, sm, cl_alpha))
}

/// [`super::stability_and_trim`], reusing an already-assembled VLM system
/// instead of assembling a fresh one for the `r1`/`r2` alpha probes.
///
/// `system` must be [`VlmSystem::assemble`]d from `airplane` at `analysis`'s
/// spanwise/chordwise resolution; see [`neutral_point_with_system`]'s doc
/// for why that makes the factorization identical. The perturbed-stabilizer
/// probe and the Newton refinement still assemble their own systems: both
/// solve a different (twisted) geometry, so there is nothing to share there.
///
/// # Errors
///
/// See [`VlmError`].
pub fn stability_and_trim_with_system(
    system: &VlmSystem<'_>,
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    cl_target: f64,
    mach: f64,
    altitude_m: f64,
) -> Result<StabilityTrimResult, VlmError> {
    stability_and_trim_with_reference_mode(
        system,
        airplane,
        &TrimAssembly::new(analysis, None),
        cl_target,
        mach,
        altitude_m,
        false,
    )
}

/// Product trim solve retaining exact geometry-only work across conditions.
///
/// Base, incidence and Newton-refinement probes share `cache`. A rotated
/// stabilizer still changes the panels and uses the ordinary full-LU solve;
/// only influence entries whose panel geometry is identical are retained.
/// No angle, incidence or circulation is approximated or rounded. The cache
/// may also span CG changes, since the reference point does not change the
/// influence matrix; each solve still uses the supplied aircraft reference.
///
/// # Errors
///
/// See [`VlmError`].
pub fn stability_and_trim_with_cache(
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    cl_target: f64,
    mach: f64,
    altitude_m: f64,
    cache: &Arc<Mutex<VlmGeometryCache>>,
) -> Result<StabilityTrimResult, VlmError> {
    let assembly = TrimAssembly::new(analysis, Some(cache));
    let system = assembly.assemble(airplane)?;
    stability_and_trim_with_reference_mode(
        &system, airplane, &assembly, cl_target, mach, altitude_m, false,
    )
}

/// [`super::stability_and_trim_reference_compatibility`], reusing an
/// already-assembled VLM system; see [`stability_and_trim_with_system`] for
/// the reuse contract.
///
/// # Errors
///
/// See [`VlmError`].
pub fn stability_and_trim_reference_compatibility_with_system(
    system: &VlmSystem<'_>,
    airplane: &Airplane,
    analysis: &AnalysisConfig,
    cl_target: f64,
    mach: f64,
    altitude_m: f64,
) -> Result<StabilityTrimResult, VlmError> {
    stability_and_trim_with_reference_mode(
        system,
        airplane,
        &TrimAssembly::new(analysis, None),
        cl_target,
        mach,
        altitude_m,
        true,
    )
}

fn stability_and_trim_with_reference_mode(
    system: &VlmSystem<'_>,
    airplane: &Airplane,
    assembly: &TrimAssembly<'_>,
    cl_target: f64,
    mach: f64,
    altitude_m: f64,
    reference_compatibility: bool,
) -> Result<StabilityTrimResult, VlmError> {
    let analysis = assembly.analysis;
    let atmosphere = Atmosphere::new(altitude_m);
    let velocity = mach * atmosphere.speed_of_sound();
    let a_lo = analysis.probe_alpha_low_deg;
    let a_hi = analysis.probe_alpha_high_deg;
    let delta_ih = analysis.trim_incidence_probe_delta_deg;

    let r1 = probe(system, atmosphere, velocity, a_lo)?;
    let r2 = probe(system, atmosphere, velocity, a_hi)?;

    let d_alpha = a_hi - a_lo;
    let (cl_alpha, cm_alpha) = if d_alpha.abs() > DEGENERACY_FLOOR {
        (
            (r2.cl_lift - r1.cl_lift) / d_alpha,
            (r2.cm_pitch - r1.cm_pitch) / d_alpha,
        )
    } else {
        (f64::NAN, f64::NAN)
    };

    let x_cg = airplane.xyz_ref[0];
    let c_ref = airplane.c_ref.max(0.1);

    let (x_np, sm) = if cl_alpha.is_nan() || cl_alpha.abs() < DEGENERACY_FLOOR {
        (x_cg, f64::NAN)
    } else if reference_compatibility {
        let sm_vlm = -cm_alpha / cl_alpha;
        let x_np_vlm = x_cg + sm_vlm * c_ref;
        let x_wing_ac = main_wing_ac_x(airplane);
        let eta_t = analysis.tail_efficiency;
        let mut x_np = x_wing_ac + eta_t * (x_np_vlm - x_wing_ac);

        // `cl_alpha` is per degree here; the fuselage term wants it per radian.
        let cl_alpha_per_rad = cl_alpha / 1.0_f64.to_radians();
        if analysis.include_fuselage_stability && cl_alpha_per_rad > 0.1 {
            let cm_a_fus = fuselage_cm_alpha_with_reference_mode(airplane, cl_alpha_per_rad, true);
            x_np -= cm_a_fus * c_ref / cl_alpha_per_rad;
        }
        (x_np, (x_np - x_cg) / c_ref)
    } else {
        // The product path: see `crate::neutral_point`.
        let x_np = crate::neutral_point::product_neutral_point(
            airplane, analysis, atmosphere, velocity, a_lo, a_hi, &r1, &r2,
        )?;
        (x_np, (x_np - x_cg) / c_ref)
    };

    // `has_ih = hstab is not None and len(hstab.xsecs) > 0`, and the root twist
    // in one step: `Some(twist)` exactly when both hold.
    let i_h0 = match hstab(airplane).and_then(|h| h.xsecs.first()) {
        Some(root) => root.twist,
        None => {
            let trim_alpha = if cl_alpha.abs() > DEGENERACY_FLOOR {
                a_lo + (cl_target - r1.cl_lift) / cl_alpha
            } else {
                a_lo
            };
            return Ok(StabilityTrimResult {
                converged: false,
                x_np,
                static_margin: sm,
                cl_alpha,
                cm_alpha,
                trim_alpha_deg: trim_alpha,
                trim_ih_deg: f64::NAN,
                cl_ih: 0.0,
                cm_ih: 0.0,
            });
        }
    };

    let perturbed = with_hstab_twist(airplane, i_h0 + delta_ih);
    let r3 = probe(&assembly.assemble(&perturbed)?, atmosphere, velocity, a_lo)?;

    let cl_ih = (r3.cl_lift - r1.cl_lift) / delta_ih;
    let cm_ih = (r3.cm_pitch - r1.cm_pitch) / delta_ih;

    // Closed-form 2x2 solve: [[cl_alpha, cl_ih], [cm_alpha, cm_ih]] @ [d_a, d_ih]
    //                       = [cl_target - CL_1, -Cm_1]  (slopes per degree).
    let jacobian = vec![vec![cl_alpha, cl_ih], vec![cm_alpha, cm_ih]];
    let rhs = vec![vec![cl_target - r1.cl_lift], vec![-r1.cm_pitch]];
    let (d_a, d_ih, converged) = match linalg::solve(&jacobian, &rhs) {
        Ok(solution)
            if solution.len() >= 2
                && solution[0].first().is_some_and(|value| value.is_finite())
                && solution[1].first().is_some_and(|value| value.is_finite())
                && jacobian
                    .first()
                    .and_then(|row| row.first())
                    .zip(jacobian.get(1).and_then(|row| row.get(1)))
                    .zip(jacobian.first().and_then(|row| row.get(1)))
                    .zip(jacobian.get(1).and_then(|row| row.first()))
                    .is_some_and(|(((a, d), b), c)| (a * d - b * c).abs() > DEGENERACY_FLOOR) =>
        {
            (solution[0][0], solution[1][0], true)
        }
        // Upstream's `except np.linalg.LinAlgError`: a singular Jacobian falls
        // back to the pure-alpha correction with the incidence left as flown.
        Err(_) => {
            let d_a = if cl_alpha.abs() > DEGENERACY_FLOOR {
                (cl_target - r1.cl_lift) / cl_alpha
            } else {
                0.0
            };
            (d_a, 0.0, false)
        }
        Ok(_) => (0.0, 0.0, false),
    };

    // The closed-form result is the same local linear estimate used by the
    // historical translation. Product analyses, however, immediately feed
    // the answer into a finer VLM mesh and then use its actual trim drag. A
    // single linear step can leave a materially non-zero Cm on swept,
    // cambered transport geometries (the B787-9 is one example). Refine the
    // product answer against the actual VLM residuals; compatibility mode
    // deliberately retains the frozen one-shot result for parity fixtures.
    let (trim_alpha_deg, trim_ih_deg, converged) = if !reference_compatibility && converged {
        let (alpha, incidence, refined) = refine_trim(
            airplane,
            assembly,
            cl_target,
            atmosphere,
            velocity,
            a_lo + d_a,
            i_h0 + d_ih,
        )?;
        (alpha, incidence, refined)
    } else {
        (a_lo + d_a, i_h0 + d_ih, converged)
    };

    Ok(StabilityTrimResult {
        converged,
        x_np,
        static_margin: sm,
        cl_alpha,
        cm_alpha,
        trim_alpha_deg,
        trim_ih_deg,
        cl_ih,
        cm_ih,
    })
}
