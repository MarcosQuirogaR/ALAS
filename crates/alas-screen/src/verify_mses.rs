// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/analysis/airfoil_screening.py
// Reference: alas @ rust-port-baseline.

//! Stage 3: MSES coupled viscous/inviscid Euler analysis for transonic verification.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use alas_aero::mses::{run_mses_polar_with_cancel, MsesStatus};
use alas_atmo::Atmosphere;
use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_geom::airfoil_library::{build_section, AirfoilLibrary};

use crate::score::interp_linear;
use crate::types::AirfoilCandidateResult;

/// Verify candidate section using MSES coupled viscous-inviscid solver.
// The flight condition, the tool location and the cancellation flag are
// independent inputs of one external run; a struct would only rename them.
#[allow(clippy::too_many_arguments)]
pub fn verify_candidate_mses(
    candidate: &mut AirfoilCandidateResult,
    config: &AlasConfig,
    dv: &DesignVector,
    mach: f64,
    altitude: f64,
    cl_target: f64,
    mses_dir: &Path,
    should_cancel: Option<&AtomicBool>,
) {
    if !config.mses.enabled {
        candidate.mses_status = Some("error".to_string());
        candidate.mses_error = Some("MSES is disabled (Setup > External Tools)".to_string());
        return;
    }

    let airfoil = match AirfoilLibrary::get(&candidate.name) {
        Some(a) => a,
        None => {
            candidate.mses_status = Some("error".to_string());
            candidate.mses_error = Some("Airfoil not found in library".to_string());
            return;
        }
    };

    let section = match build_section(dv, &airfoil.coordinates) {
        Ok(s) => s,
        Err(e) => {
            candidate.mses_status = Some("error".to_string());
            candidate.mses_error = Some(e.to_string());
            return;
        }
    };

    let m_effective = mach * dv.sweep_deg.to_radians().cos();
    let atmo = Atmosphere::new(altitude);
    let v = mach * atmo.speed_of_sound();
    let reynolds = atmo.density() * v * dv.root_chord_m / atmo.dynamic_viscosity();
    let bracket_alpha = candidate
        .alpha_deg
        .unwrap_or(candidate.alpha_3d_deg.unwrap_or(0.0));

    if should_cancel.is_some_and(|cancel_flag| cancel_flag.load(Ordering::Relaxed)) {
        candidate.mses_status = Some("cancelled".to_string());
        return;
    }

    // Screening verification evaluates the configured operating-point bracket
    // exactly once. Widening it after an inconvenient result changes the
    // analysis request and can make a candidate look verified by a different
    // condition than every other candidate.
    let polar = run_mses_polar_with_cancel(
        &section,
        m_effective,
        reynolds,
        bracket_alpha,
        &config.mses,
        mses_dir,
        should_cancel,
    );

    candidate.mses_status = Some(polar.status.as_str().to_string());
    if polar.status != MsesStatus::Ok {
        candidate.mses_error = polar.error;
        return;
    }

    if polar.cl.is_empty() {
        candidate.mses_status = Some("error".to_string());
        candidate.mses_error = Some("MSES produced no converged points".to_string());
        return;
    }

    let mut indexed: Vec<(f64, f64, f64, f64)> = polar
        .cl
        .iter()
        .zip(&polar.cd)
        .zip(&polar.cdw)
        .zip(&polar.alpha_deg)
        .map(|(((&cl, &cd), &cdw), &alpha)| (cl, cd, cdw, alpha))
        .collect();

    indexed.sort_by(|a, b| a.0.total_cmp(&b.0));

    let cl_sorted: Vec<f64> = indexed.iter().map(|p| p.0).collect();
    let cd_sorted: Vec<f64> = indexed.iter().map(|p| p.1).collect();
    let cdw_sorted: Vec<f64> = indexed.iter().map(|p| p.2).collect();

    if cl_target < cl_sorted[0] || cl_target > cl_sorted[cl_sorted.len() - 1] {
        candidate.mses_status = Some("error".to_string());
        candidate.mses_error = Some(format!(
            "target CL {:.3} is outside MSES's configured converged range [{:.3}, {:.3}]",
            cl_target,
            cl_sorted[0],
            cl_sorted[cl_sorted.len() - 1]
        ));
        return;
    }

    let cd_at_target = interp_linear(cl_target, &cl_sorted, &cd_sorted);
    let cdw_at_target = interp_linear(cl_target, &cl_sorted, &cdw_sorted);

    if cd_at_target <= 0.0 || !cd_at_target.is_finite() || !cdw_at_target.is_finite() {
        candidate.mses_status = Some("error".to_string());
        candidate.mses_error = Some("non-physical MSES result at target CL".to_string());
        return;
    }

    candidate.l_over_d_mses = Some(cl_target / cd_at_target);
    candidate.cd_mses = Some(cd_at_target);
    candidate.cdw_mses = Some(cdw_at_target);
    candidate.mses_verified = true;
}
