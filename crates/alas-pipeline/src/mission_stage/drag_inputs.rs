// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Drag-buildup and lift-surrogate inputs read off the built surfaces.
//!
//! The design vector's `sweep_deg` is the inboard leading-edge sweep of the
//! geometry builder. The Korn/Lock compressibility drag, the form factor and
//! the VLM surrogate are written for the quarter-chord sweep, which on a
//! tapered wing is several degrees smaller (A380: 36.4 degrees leading edge against 31-34 degrees at c/4, depending on how the built wing is measured).
//! The section thickness is likewise the exposed-area-weighted t/c of the
//! built wing (Raymer, *Aircraft Design: A Conceptual Approach*, eq. 12.30
//! basis), not a constant. Only Product mode uses these; the frozen
//! reference-compatibility path keeps its original constants.

use alas_aero::analysis::AeroAnalysis;
use alas_aero::drag_buildup::DragSettings;

use super::MissionReferenceMode;
use crate::full_analysis::AnalysisReport;

/// Drag-buildup settings for the mission. Product mode holds the ESDU
/// excrescence fit at its vertex so a very large aircraft keeps a
/// non-negative excrescence drag; the frozen path keeps the raw fit.
pub(super) fn drag_settings(reference_mode: MissionReferenceMode) -> DragSettings {
    match reference_mode {
        MissionReferenceMode::Product => DragSettings {
            clamp_excrescence_fit: true,
            ..DragSettings::default()
        },
        MissionReferenceMode::ReferenceCompatibility => DragSettings::reference_compatibility(),
    }
}

/// Quarter-chord sweep of the main wing for the drag buildup and the VLM
/// surrogate, rad.
pub(super) fn main_quarter_chord_sweep(
    report: &AnalysisReport,
    reference_mode: MissionReferenceMode,
) -> f64 {
    match reference_mode {
        MissionReferenceMode::Product => {
            AeroAnalysis::quarter_chord_sweep_deg(&report.airplane, report.design.sweep_deg)
                .to_radians()
        }
        MissionReferenceMode::ReferenceCompatibility => report.design.sweep_deg.to_radians(),
    }
}

/// Main-wing thickness-to-chord for the drag buildup.
pub(super) fn main_thickness(report: &AnalysisReport, reference_mode: MissionReferenceMode) -> f64 {
    match report.airplane.wings.first() {
        Some(main) if reference_mode == MissionReferenceMode::Product && main.xsecs.len() >= 2 => {
            AeroAnalysis::area_weighted_thickness(main)
        }
        _ => 0.12 * report.design.airfoil_thickness_scale,
    }
}

/// A tail surface's thickness-to-chord for the drag buildup, with
/// `constant_tc` retained for reference compatibility or a missing surface.
pub(super) fn tail_thickness(
    report: &AnalysisReport,
    reference_mode: MissionReferenceMode,
    name: &str,
    constant_tc: f64,
) -> f64 {
    report
        .airplane
        .wings
        .iter()
        .find(|surface| surface.name == name && surface.xsecs.len() >= 2)
        .filter(|_| reference_mode == MissionReferenceMode::Product)
        .map_or(constant_tc, AeroAnalysis::area_weighted_thickness)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::full_analysis::FullAnalysis;
    use alas_config::AlasConfig;

    #[test]
    fn product_mode_uses_the_quarter_chord_sweep_and_built_thickness() {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": "A380-800"}))
            .unwrap_or_else(|error| panic!("A380 preset: {error}"));
        let design = alas_config::presets::get("A380-800")
            .unwrap_or_else(|error| panic!("A380 preset: {error}"))
            .design_vector;
        let report = FullAnalysis::new(config)
            .run(&design, true)
            .unwrap_or_else(|error| panic!("A380 report: {error}"));

        let product = main_quarter_chord_sweep(&report, MissionReferenceMode::Product).to_degrees();
        let frozen =
            main_quarter_chord_sweep(&report, MissionReferenceMode::ReferenceCompatibility)
                .to_degrees();
        assert!(
            (30.0..34.5).contains(&product),
            "Product c/4 sweep {product} deg must sit well below the design LE sweep {} deg",
            report.design.sweep_deg
        );
        assert!(
            (frozen - report.design.sweep_deg).abs() < 1e-12 && frozen - product > 2.0,
            "frozen path must keep the design LE sweep ({frozen}) above c/4 ({product})"
        );

        let tc = main_thickness(&report, MissionReferenceMode::Product);
        assert!((0.08..0.16).contains(&tc), "area-weighted t/c {tc}");
        let frozen_tc = main_thickness(&report, MissionReferenceMode::ReferenceCompatibility);
        assert!((frozen_tc - 0.12 * report.design.airfoil_thickness_scale).abs() < 1e-15);
    }
}
