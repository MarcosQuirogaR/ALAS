// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One-time search-mesh calibration for the physical cruise-attitude constraint.

use alas_aero::vlm::VlmError;
use alas_config::analysis::AnalysisConfig;
use alas_geom::aircraft::airplane::Airplane;
use alas_stab::trim::stability_and_trim;

/// The coarse-mesh trim alpha shifted by the fine-mesh correction, which is
/// solved on the first call and cached in `cached_correction_deg` after it.
///
/// # Errors
///
/// The fine-mesh trim's own [`VlmError`] when that one-time solve fails.
pub(crate) fn corrected_body_alpha(
    plane: &Airplane,
    analysis: &AnalysisConfig,
    cl_target: f64,
    mach: f64,
    altitude_m: f64,
    coarse_alpha_deg: f64,
    cached_correction_deg: &mut Option<f64>,
) -> Result<f64, VlmError> {
    let correction = match *cached_correction_deg {
        Some(correction) => correction,
        None => {
            let mut fine_analysis = analysis.clone();
            fine_analysis.spanwise_resolution = fine_analysis.fine_spanwise_resolution;
            fine_analysis.chordwise_resolution = fine_analysis.fine_chordwise_resolution;
            let fine_trim = stability_and_trim(plane, &fine_analysis, cl_target, mach, altitude_m)?;
            let correction = fine_trim.trim_alpha_deg - coarse_alpha_deg;
            *cached_correction_deg = Some(correction);
            correction
        }
    };
    Ok(coarse_alpha_deg + correction)
}
