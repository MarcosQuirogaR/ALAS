// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Geometry-only incidence for the classical comparison models.

use alas_aero::fourier_lifting_line::FourierLiftingLineSurface;
use alas_pipeline::full_analysis::AnalysisReport;

/// Return the main wing's planform-weighted geometric zero-lift incidence.
///
/// The report polar can cover only positive-lift cruise angles. Deriving this
/// quantity from wing camber and incidence keeps the classical model
/// independent of VLM output while evaluating it at the same alpha schedule.
pub(super) fn classical_zero_lift_angle_rad(report: &AnalysisReport) -> Option<f64> {
    let main_wing = report
        .airplane
        .wings
        .iter()
        .filter(|wing| wing.symmetric)
        .filter_map(|wing| {
            wing.projected_area()
                .is_finite()
                .then_some((wing, wing.projected_area()))
        })
        .max_by(|(_, left), (_, right)| left.total_cmp(right))?
        .0;
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
