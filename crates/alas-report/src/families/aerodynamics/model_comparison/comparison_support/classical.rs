// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Geometry-only incidence for the classical comparison models.

use alas_aero::fourier_lifting_line::FourierLiftingLineSurface;
use alas_pipeline::full_analysis::AnalysisReport;

/// The symmetric wing of largest projected area: the classical models' main wing.
fn classical_main_wing(report: &AnalysisReport) -> Option<&alas_geom::aircraft::wing::Wing> {
    report
        .airplane
        .wings
        .iter()
        .filter(|wing| wing.symmetric)
        .filter_map(|wing| {
            wing.projected_area()
                .is_finite()
                .then_some((wing, wing.projected_area()))
        })
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|pair| pair.0)
}

/// The affine map `(scale, offset_deg)` that carries an incompressible
/// classical reference curve (`CL = a (alpha - alpha_0)`) onto the
/// compressible polar's angle axis; `None` when the polar is not compressible.
///
/// The compressible curve has slope `r a`, `r` being the DATCOM 4.1.3.2
/// Method 1 ratio of the wing's compressible to incompressible lift-curve
/// slope at the polar's Mach (`alas_stab::compressible_sweep::datcom_slope_ratio`),
/// so the angle for a given lift is `alpha_0 + (alpha - alpha_0) / r`. The
/// ratio is a closed form in aspect ratio, midchord sweep and Mach, so it is
/// independent of the lattice the references are compared with.
pub(in super::super) fn compressible_reference_map(report: &AnalysisReport) -> Option<(f64, f64)> {
    let mach = report.polar.compressible_mach?;
    let alpha_zero_deg = classical_zero_lift_angle_rad(report)?.to_degrees();
    let wing = classical_main_wing(report)?;
    let ratio = alas_stab::compressible_sweep::datcom_slope_ratio(
        report.polar_fit.aspect_ratio,
        mach,
        wing.mean_sweep_angle(0.5),
    );
    (ratio.is_finite() && ratio > 0.0).then(|| (1.0 / ratio, alpha_zero_deg * (1.0 - 1.0 / ratio)))
}

/// Return the main wing's planform-weighted geometric zero-lift incidence.
///
/// The report polar can cover only positive-lift cruise angles. Deriving this
/// quantity from wing camber and incidence keeps the classical model
/// independent of VLM output while evaluating it at the same alpha schedule.
pub(super) fn classical_zero_lift_angle_rad(report: &AnalysisReport) -> Option<f64> {
    let main_wing = classical_main_wing(report)?;
    let surface = FourierLiftingLineSurface::from_wing(main_wing, 1).ok()?;

    let mut weighted_angle = 0.0;
    let mut area_weight = 0.0;
    for pair in surface.sections.windows(2) {
        let [inboard, outboard] = pair else {
            continue;
        };
        let span_fraction = outboard.span_fraction - inboard.span_fraction;
        let inboard_effective = inboard.zero_lift_angle_rad - inboard.twist_rad;
        let outboard_effective = outboard.zero_lift_angle_rad - outboard.twist_rad;
        let segment_weight = span_fraction * (inboard.chord_m + outboard.chord_m) / 2.0;
        let segment_angle = (inboard.chord_m * inboard_effective
            + outboard.chord_m * outboard_effective)
            / (inboard.chord_m + outboard.chord_m);
        if span_fraction.is_finite()
            && segment_weight.is_finite()
            && segment_angle.is_finite()
            && segment_weight > 0.0
        {
            weighted_angle += segment_weight * segment_angle;
            area_weight += segment_weight;
        }
    }
    (area_weight > 0.0).then_some(weighted_angle / area_weight)
}
