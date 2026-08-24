// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/analysis/airfoil_screening.py
// Reference: alas @ rust-port-baseline.

//! Stage 2: 3-D wing re-simulation with stability and trim solve.

use alas_aero::analysis::{AeroAnalysis, TrimPoint};
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{run_mass_analysis_with_model_checked, MassCoordinateModel};
use alas_stab::trim::stability_and_trim;

use crate::types::{AirfoilCandidateResult, CL_FEASIBILITY_TOL, TRIM_ALPHA_SLACK_DEG};

/// Mass-coordinate path used by the 3-D screening refinement.
///
/// The reference-compatible path is kept for the translated screening
/// function and its frozen fixture. Product callers must select the
/// structural path explicitly; silently sharing a mass-coordinate choice
/// would make a parity change alter the physical optimizer (or vice versa).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreeningMassModel {
    /// Reproduce the Python screening calculation and its historical CG.
    ReferenceCompatibility,
    /// Use the product structural wingbox centroid.
    StructuralWingbox,
}

/// Re-simulate candidate on the real 3-D aircraft geometry, checking trim and stability.
pub fn refine_candidate_3d(
    candidate: &mut AirfoilCandidateResult,
    config: &AlasConfig,
    dv: &DesignVector,
    mach: f64,
    altitude: f64,
    cl_target: f64,
    min_static_margin: Option<f64>,
) {
    refine_candidate_3d_with_mass_model(
        candidate,
        config,
        dv,
        mach,
        altitude,
        cl_target,
        min_static_margin,
        ScreeningMassModel::ReferenceCompatibility,
    );
}

/// Refine a candidate using the product structural mass-coordinate model.
pub fn refine_candidate_3d_product(
    candidate: &mut AirfoilCandidateResult,
    config: &AlasConfig,
    dv: &DesignVector,
    mach: f64,
    altitude: f64,
    cl_target: f64,
    min_static_margin: Option<f64>,
) {
    refine_candidate_3d_with_mass_model(
        candidate,
        config,
        dv,
        mach,
        altitude,
        cl_target,
        min_static_margin,
        ScreeningMassModel::StructuralWingbox,
    );
}

// The translated solver inputs stay explicit at the parity/product mode seam.
#[allow(clippy::too_many_arguments)]
pub(crate) fn refine_candidate_3d_with_mass_model(
    candidate: &mut AirfoilCandidateResult,
    config: &AlasConfig,
    dv: &DesignVector,
    mach: f64,
    altitude: f64,
    cl_target: f64,
    min_static_margin: Option<f64>,
    mass_model: ScreeningMassModel,
) {
    let mut cfg2 = config.clone();
    cfg2.geometry.wing.root_airfoil = candidate.name.clone();
    cfg2.geometry.engine.apply_engine_spec();

    let builder = AircraftBuilder::new(Some(cfg2.geometry.clone()));
    let plane = match builder.build(Some(dv), false) {
        Ok(p) => p,
        Err(e) => {
            candidate.refine_error = Some(e.to_string());
            return;
        }
    };

    let coordinate_model = match mass_model {
        ScreeningMassModel::ReferenceCompatibility => MassCoordinateModel::ReferenceCompatibility,
        ScreeningMassModel::StructuralWingbox => {
            MassCoordinateModel::StructuralWingbox(&cfg2.structures)
        }
    };
    let (_masses, _coords, cg) = match run_mass_analysis_with_model_checked(
        &plane,
        &config.requirements,
        &cfg2.geometry,
        &cfg2.cabin,
        &cfg2.control_surfaces,
        Some(&cfg2.mass_model),
        None,
        coordinate_model,
    ) {
        Ok(result) => result,
        Err(error) => {
            candidate.refine_error = Some(format!("mass-coordinate error: {error}"));
            return;
        }
    };

    let mut plane = plane;
    plane.xyz_ref[0] = cg[0];

    let aero = AeroAnalysis::new(
        &plane,
        dv.sweep_deg,
        Some(cfg2.geometry.clone()),
        Some(cfg2.drag_model.clone()),
        Some(cfg2.analysis.clone()),
    );

    let trim = match stability_and_trim(&plane, &cfg2.analysis, cl_target, mach, altitude) {
        Ok(t) => t,
        Err(e) => {
            candidate.refine_error = Some(e.to_string());
            return;
        }
    };

    let tp = TrimPoint {
        trim_alpha_deg: trim.trim_alpha_deg,
        trim_ih_deg: trim.trim_ih_deg,
        cl_alpha: trim.cl_alpha,
    };

    let perf = match aero.trimmed_performance(&tp, mach, altitude) {
        Ok(p) => p,
        Err(e) => {
            candidate.refine_error = Some(e.to_string());
            return;
        }
    };

    let l_over_d_3d = perf.l_over_d;
    let cd_3d = perf.cd;
    let cl_3d = perf.cl;
    let alpha_3d = perf.alpha_deg;
    let cm_residual = perf.cm_residual;

    let vals = [l_over_d_3d, cd_3d, cl_3d, alpha_3d, cm_residual];
    if !vals.iter().all(|v| v.is_finite()) || cd_3d <= 0.0 {
        candidate.refine_error = Some("non-physical 3-D result".to_string());
        return;
    }

    if (cl_3d - cl_target).abs() > CL_FEASIBILITY_TOL * cl_target.max(1e-6) {
        candidate.refine_error = Some(format!(
            "trim solve could not sustain level cruise: trimmed CL={:.3}, required CL={:.3} (L != W for this airfoil on this design)",
            cl_3d, cl_target
        ));
        return;
    }

    let a_lo = cfg2.analysis.probe_alpha_low_deg;
    let a_hi = cfg2.analysis.probe_alpha_high_deg;
    let window_lo = a_lo.min(a_hi) - TRIM_ALPHA_SLACK_DEG;
    let window_hi = a_lo.max(a_hi) + TRIM_ALPHA_SLACK_DEG;

    if !(window_lo..=window_hi).contains(&trim.trim_alpha_deg) {
        candidate.refine_error = Some(format!(
            "trim alpha {:.1} deg is unrealistically far from the probe window [{:.1}, {:.1}] deg -- this airfoil's lift behaviour is too different from the baseline for the closed-form trim solve to be trustworthy",
            trim.trim_alpha_deg, a_lo, a_hi
        ));
        return;
    }

    if let Some(min_sm) = min_static_margin {
        if trim.static_margin.is_finite() && trim.static_margin < min_sm {
            candidate.static_margin_3d = Some(trim.static_margin);
            candidate.refine_error = Some(format!(
                "static margin {:.1}% is below the requested floor {:.1}% -- this airfoil swap would leave the aircraft too weakly stable in pitch",
                trim.static_margin * 100.0, min_sm * 100.0
            ));
            return;
        }
    }

    candidate.l_over_d_3d = Some(l_over_d_3d);
    candidate.cd_3d = Some(cd_3d);
    candidate.alpha_3d_deg = Some(alpha_3d);
    candidate.cm_residual_3d = Some(cm_residual);
    candidate.static_margin_3d = if trim.static_margin.is_finite() {
        Some(trim.static_margin)
    } else {
        None
    };
    candidate.refined = true;
}
