// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tail planform used by the mission reduced-order aircraft.
//!
//! The product path reads every tail dimension and position from the surfaces
//! of the built airplane, so tail scale, fuselage length, tail shift and the
//! tail attachment rule all reach the mission surrogate exactly as they reach
//! the analysed aircraft. The frozen-reference path keeps the historical
//! configuration-derived values as a separate, explicit branch.

use alas_config::AlasConfig;
use alas_geom::aircraft::wing::Wing;

use super::MissionReferenceMode;
use crate::full_analysis::AnalysisReport;

/// One tail surface as the mission model consumes it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct TailSurface {
    pub root_chord_m: f64,
    pub tip_chord_m: f64,
    /// Span in the planform-area convention of the surface: lateral
    /// projection for the horizontal tail, vertical extent for the fin.
    pub span_m: f64,
    /// Planform area in the same convention as `span_m`.
    pub area_m2: f64,
    pub mean_aerodynamic_chord_m: f64,
    /// Sweep of the root-to-tip leading-edge offset, radians.
    pub sweep_rad: f64,
    /// Root leading-edge position `[x, z]` in aircraft axes, metres.
    pub origin_xz_m: [f64; 2],
}

fn surface<'a>(report: &'a AnalysisReport, name: &str) -> Result<&'a Wing, String> {
    report
        .airplane
        .wings
        .iter()
        .find(|wing| wing.name == name)
        .ok_or_else(|| {
            format!(
                "mission reduced geometry requires an explicit {name}; no surrogate tail was substituted"
            )
        })
}

fn built_surface(wing: &Wing, vertical: bool) -> Result<TailSurface, String> {
    let (root, tip) = match (wing.xsecs.first(), wing.xsecs.last()) {
        (Some(root), Some(tip)) if wing.xsecs.len() >= 2 => (root, tip),
        _ => return Err(format!("{} has fewer than two cross-sections", wing.name)),
    };
    let dx = tip.xyz_le[0] - root.xyz_le[0];
    let (span_m, area_m2, lateral) = if vertical {
        // `reference_area()` projects onto XY and is zero for the fin; its
        // aerodynamic planform is the unfolded XZ area.
        (
            wing.unfolded_span(),
            wing.unfolded_area(),
            tip.xyz_le[2] - root.xyz_le[2],
        )
    } else {
        (
            wing.projected_span(),
            wing.reference_area(),
            tip.xyz_le[1] - root.xyz_le[1],
        )
    };
    Ok(TailSurface {
        root_chord_m: root.chord,
        tip_chord_m: tip.chord,
        span_m,
        area_m2,
        mean_aerodynamic_chord_m: wing.mean_aerodynamic_chord(),
        sweep_rad: planform_sweep(dx, lateral),
        origin_xz_m: [root.xyz_le[0], root.xyz_le[2]],
    })
}

/// Horizontal and vertical tail for the requested reference mode.
///
/// `wing_x_m` is only used by the frozen-reference branch, whose tail origins
/// are offsets from the main-wing datum.
pub(super) fn tail_surfaces(
    config: &AlasConfig,
    report: &AnalysisReport,
    reference_mode: MissionReferenceMode,
    wing_x_m: f64,
) -> Result<(TailSurface, TailSurface), String> {
    let hstab = surface(report, "Horizontal Stabilizer")?;
    let vstab = surface(report, "Vertical Stabilizer")?;
    match reference_mode {
        MissionReferenceMode::Product => {
            Ok((built_surface(hstab, false)?, built_surface(vstab, true)?))
        }
        MissionReferenceMode::ReferenceCompatibility => {
            let tail = &config.geometry.empennage;
            let shift = report.design.tail_x_shift_m;
            let h_area = hstab.unfolded_area();
            let v_area = vstab.unfolded_area();
            let h_span = 2.0 * h_area / (tail.hstab_root_chord_m + tail.hstab_tip_chord_m);
            let v_span = 2.0 * v_area / (tail.vstab_root_chord_m + tail.vstab_tip_chord_m);
            Ok((
                TailSurface {
                    root_chord_m: tail.hstab_root_chord_m,
                    tip_chord_m: tail.hstab_tip_chord_m,
                    span_m: h_span,
                    area_m2: h_area,
                    mean_aerodynamic_chord_m: (tail.hstab_root_chord_m + tail.hstab_tip_chord_m)
                        / 2.0,
                    sweep_rad: planform_sweep(tail.hstab_tip_le_m.0, tail.hstab_tip_le_m.1),
                    origin_xz_m: [
                        wing_x_m + tail.hstab_offset_from_tail_m + shift,
                        tail.hstab_z_m,
                    ],
                },
                TailSurface {
                    root_chord_m: tail.vstab_root_chord_m,
                    tip_chord_m: tail.vstab_tip_chord_m,
                    span_m: v_span,
                    area_m2: v_area,
                    mean_aerodynamic_chord_m: (tail.vstab_root_chord_m + tail.vstab_tip_chord_m)
                        / 2.0,
                    sweep_rad: planform_sweep(tail.vstab_tip_le_m.0, tail.vstab_tip_le_m.2),
                    origin_xz_m: [
                        wing_x_m + tail.vstab_offset_from_tail_m + shift,
                        tail.vstab_z_m,
                    ],
                },
            ))
        }
    }
}

/// Span used for the tail aspect ratio. The product path measures it on the
/// built surface; the frozen-reference path derives it from the area and the
/// configured chords, as the historical vehicle model did.
pub(super) fn tail_span(
    tail: &TailSurface,
    area_m2: f64,
    reference_mode: MissionReferenceMode,
) -> f64 {
    match reference_mode {
        MissionReferenceMode::Product => tail.span_m,
        MissionReferenceMode::ReferenceCompatibility => {
            2.0 * area_m2 / (tail.root_chord_m + tail.tip_chord_m)
        }
    }
}

fn planform_sweep(dx_m: f64, lateral_m: f64) -> f64 {
    if lateral_m == 0.0 {
        0.0
    } else {
        dx_m.atan2(lateral_m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::full_analysis::FullAnalysis;
    use crate::mission_stage::{mission_wings, vlm_geometry};
    use alas_config::DesignVector;

    fn scaled_report() -> (AlasConfig, AnalysisReport) {
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" }))
            .unwrap_or_else(|error| panic!("A320 preset: {error}"));
        let preset = alas_config::presets::get("A320-200")
            .unwrap_or_else(|error| panic!("A320 preset: {error}"))
            .design_vector;
        let design = DesignVector {
            tail_scale: 1.3,
            fuselage_length_m: preset.fuselage_length_m + 2.0,
            wing_x_shift_m: preset.wing_x_shift_m + 1.0,
            tail_x_shift_m: preset.tail_x_shift_m + 0.5,
            ..preset
        };
        let report = FullAnalysis::new(config.clone())
            .run(&design, true)
            .unwrap_or_else(|error| panic!("scaled-tail analysis: {error}"));
        (config, report)
    }

    fn close(left: f64, right: f64) -> bool {
        (left - right).abs() <= 1.0e-9 * left.abs().max(right.abs()).max(1.0)
    }

    #[test]
    fn mission_tail_matches_the_built_tail_surfaces_when_scaled_and_moved() {
        let (config, report) = scaled_report();
        let mode = MissionReferenceMode::Product;
        let geometry = vlm_geometry(&config, &report, mode)
            .unwrap_or_else(|error| panic!("mission geometry: {error}"));
        let wings = mission_wings(&config, &report, mode)
            .unwrap_or_else(|error| panic!("mission wings: {error}"));
        let built = |name: &str| {
            report
                .airplane
                .wings
                .iter()
                .find(|wing| wing.name == name)
                .unwrap_or_else(|| panic!("built {name}"))
        };
        let hstab = built("Horizontal Stabilizer");
        let vstab = built("Vertical Stabilizer");
        for (index, (surface, vertical)) in [(hstab, false), (vstab, true)].into_iter().enumerate()
        {
            let vlm = &geometry.wings[index + 1];
            let root = &surface.xsecs[0];
            let tip = surface.xsecs.last().unwrap_or(root);
            assert!(
                close(vlm.chord_root_m, root.chord),
                "{} root chord",
                surface.name
            );
            assert!(
                close(vlm.chord_tip_m, tip.chord),
                "{} tip chord",
                surface.name
            );
            assert!(close(vlm.origin_m[0], root.xyz_le[0]), "{} x", surface.name);
            assert!(close(vlm.origin_m[2], root.xyz_le[2]), "{} z", surface.name);
            let span = if vertical {
                surface.unfolded_span()
            } else {
                surface.projected_span()
            };
            assert!(close(vlm.span_projected_m, span), "{} span", surface.name);
            let area = vlm.area_reference_m2;
            assert!(
                close(vlm.aspect_ratio, span * span / area),
                "{} AR",
                surface.name
            );
            let params = &wings[index + 1];
            assert!(
                close(
                    params.mean_aerodynamic_chord_m,
                    surface.mean_aerodynamic_chord()
                ),
                "{} MAC",
                surface.name
            );
            assert!(
                close(params.aspect_ratio, span * span / area),
                "{} AR",
                surface.name
            );
        }
        // The scaled tail must differ from the unscaled configuration, or the
        // comparison above would not prove anything about the scale.
        let unscaled_root = config.geometry.empennage.hstab_root_chord_m;
        assert!((geometry.wings[1].chord_root_m - unscaled_root).abs() > 1.0e-3);
        let unmoved_x = report.design.wing_x_shift_m
            + config.geometry.wing.root_datum_x_m
            + config.geometry.empennage.hstab_offset_from_tail_m
            + report.design.tail_x_shift_m;
        assert!((geometry.wings[1].origin_m[0] - unmoved_x).abs() > 1.0e-3);
    }
}
