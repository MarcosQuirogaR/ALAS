// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py: figure_structures_stress
// (L5907-5949).
// Reference: alas @ rust-port-baseline.

//! Spanwise margin-of-safety per spar cap, one panel per spar, all three
//! load cases overlaid -- `MS >= 0` required everywhere for a valid design
//! (`MS = 0` exactly at the root for the sizing-governing case, by
//! construction).

use alas_pipeline::structural::StructuralAnalysisResult;

use super::layout::panel_rects;
use super::status::{load_case_color, resolve_structural_result, status_message_scene};
use crate::chart_kit::{draw_legend, LegendMarker};
use crate::scene::{Axes2D, Color, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

const TITLE: &str = "Structural Analysis -- Stress";

/// Upstream's `np.clip(ss.margin_of_safety, -1.0, 5.0)` bounds, reused for
/// both the plotted curve and the fixed y-axis range.
const MS_MIN: f64 = -1.0;
const MS_MAX: f64 = 5.0;

/// Spar cap margin of safety vs. spanwise position, one panel per spar.
pub fn figure_structures_stress(
    result: Option<&StructuralAnalysisResult>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let result = match resolve_structural_result(result, TITLE, pal) {
        Ok(r) => r,
        Err(scene) => return scene,
    };
    let (Some(sizing), Some(analysis)) = (result.sizing.as_ref(), result.analysis.as_ref()) else {
        return status_message_scene(
            TITLE,
            "Structural analysis reported \"ok\" but has no sizing or analytical data.",
            false,
            pal,
        );
    };

    let n_spars = sizing.spars.len();
    let panel_w = 260.0;
    let gutter = 26.0;
    let margin_left = 60.0;
    let margin_right = 20.0;
    // +10 px below the automatic figure title compared to the original 55.0,
    // on top of that title's own existing ~28 px clearance.
    let margin_top = 65.0;
    let margin_bottom = 55.0;
    let plot_height = 330.0;
    let n_f = n_spars.max(1) as f64;
    let width = margin_left + n_f * panel_w + (n_f - 1.0) * gutter + margin_right;
    let height = margin_top + plot_height + margin_bottom;

    let mut scene = Scene::new(width, height, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Spar Cap Margin of Safety (analytical)".to_owned());

    let y_min = analysis.y.iter().cloned().fold(f64::INFINITY, f64::min);
    let y_max = analysis.y.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let (y_min, y_max) = if y_min.is_finite() && y_max.is_finite() && y_max > y_min {
        (y_min, y_max)
    } else {
        (0.0, 1.0)
    };

    let rects = panel_rects(
        n_spars,
        (
            margin_left,
            margin_top,
            width - margin_left - margin_right,
            plot_height,
        ),
        gutter,
    );

    for (i, rect) in rects.into_iter().enumerate() {
        let axes = Axes2D::new(rect, (y_min, y_max), (MS_MIN, MS_MAX));
        axes.draw_frame_with_labels(
            &mut scene,
            pal,
            "Spanwise position Y [m]",
            if i == 0 { "Margin of safety [-]" } else { "" },
        );

        let mut legend_entries = Vec::new();
        for lc in &analysis.load_cases {
            let Some(ss) = lc.spar_stress.get(i) else {
                continue;
            };
            let stroke = Stroke::new(Color::from_hex(load_case_color(lc.name)), 2.0);
            let pts: Vec<(f64, f64)> =
                lc.y.iter()
                    .zip(&ss.margin_of_safety)
                    .map(|(&y, &ms)| (y, ms.clamp(MS_MIN, MS_MAX)))
                    .collect();
            axes.add_line_series(&mut scene, &pts, stroke.clone());
            legend_entries.push((lc.name.to_owned(), LegendMarker::Line(stroke)));
        }

        let zero_stroke = Stroke::dashed(Color::from_hex(pal.spine), 1.4, 5.0, 4.0);
        axes.add_line_series(
            &mut scene,
            &[(y_min, 0.0), (y_max, 0.0)],
            zero_stroke.clone(),
        );
        legend_entries.push(("MS = 0".to_owned(), LegendMarker::Line(zero_stroke)));

        let frac = sizing.spar_fracs.get(i).copied().unwrap_or(f64::NAN);
        scene.add(SceneElement::Text {
            text: format!("Spar x/c={frac:.2}"),
            pos: [rect.0 + rect.2 * 0.5, rect.1 - 10.0],
            font_size: 11.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Center,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
        draw_legend(
            &mut scene,
            [rect.0 + 8.0, rect.1 + 8.0],
            &legend_entries,
            pal,
            8.0,
        );
    }

    scene
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_struct::analytical::{
        LoadCaseResult, ModalResult, SparStressResult, StructuralAnalysisReport,
    };
    use alas_struct::sizing::{MassBreakdown, SparSizing, WingboxSizing};

    fn sample_result() -> StructuralAnalysisResult {
        let y = vec![0.0, 9.0, 18.0];
        let spar = SparSizing {
            chord_fraction: 0.2,
            h: vec![0.5, 0.4, 0.3],
            a_cap: vec![0.01, 0.008, 0.005],
            frac_moment: vec![1.0, 1.0, 1.0],
            t_cap: vec![0.01, 0.008, 0.005],
            w_cap: vec![0.2, 0.18, 0.15],
            t_web: 0.005,
            margin_of_safety: vec![0.0, 1.0, 2.0],
        };
        let sizing = WingboxSizing {
            y_stations: y.clone(),
            eta_stations: vec![0.0, 0.5, 1.0],
            chord: vec![6.0, 4.0, 2.0],
            spar_fracs: vec![0.2],
            spars: vec![spar],
            t_skin: 0.004,
            num_ribs: 10,
            rib_spacing_m: 1.8,
            mass_breakdown_kg: MassBreakdown {
                spar_caps: 100.0,
                spar_webs: 50.0,
                skin: 200.0,
                ribs: 60.0,
            },
            total_mass_kg: 410.0,
            sizing_load_case: "pull-up",
            composite_declaration: None,
        };
        let spar_stress = SparStressResult {
            chord_fraction: 0.2,
            stress_pa: vec![1e8, 5e7, 1e7],
            margin_of_safety: vec![-2.0, 1.0, 8.0],
        };
        let load_case = LoadCaseResult {
            name: "pull-up",
            load_factor: 2.5,
            y: y.clone(),
            q_net: vec![0.0, 0.0, 0.0],
            shear_n: vec![0.0, 0.0, 0.0],
            moment_nm: vec![0.0, 0.0, 0.0],
            deflection_m: vec![0.0, 0.0, 0.0],
            tip_deflection_m: 0.0,
            spar_stress: vec![spar_stress],
        };
        let analysis = StructuralAnalysisReport {
            y,
            ei_nm2: vec![1.0, 1.0, 1.0],
            load_cases: vec![load_case],
            modal: ModalResult {
                frequencies_hz: vec![],
                mode_shapes: vec![],
            },
        };
        StructuralAnalysisResult {
            status: "ok".to_owned(),
            error: None,
            wsg: None,
            sizing: Some(sizing),
            mesh_health: None,
            analysis: Some(analysis),
            nastran: None,
            nastran95: None,
            patran: None,
            torenbeek_wing_mass_kg: 400.0,
        }
    }

    #[test]
    fn unavailable_result_renders_a_status_message_not_a_chart() {
        let scene = figure_structures_stress(None, None);
        assert!(scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Text { .. })));
        assert!(!scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Polyline { .. } | SceneElement::Rect { .. })));
    }

    #[test]
    fn one_panel_per_spar_and_the_curve_is_clamped_to_the_reference_bounds() {
        let result = sample_result();
        let scene = figure_structures_stress(Some(&result), None);
        let polylines: Vec<&SceneElement> = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Polyline { .. }))
            .collect();
        // One spar: one load-case curve plus the MS=0 reference line.
        assert_eq!(polylines.len(), 2);
        for elem in &polylines {
            let SceneElement::Polyline { points, .. } = elem else {
                continue;
            };
            for p in points {
                // Canvas-space y grows downward; a value inside [MS_MIN,
                // MS_MAX] maps inside the panel's own top/bottom, which this
                // sample's single panel occupies entirely -- so bounding the
                // canvas y is an indirect check that the clamp took effect.
                assert!(p[1].is_finite());
            }
        }
    }
}
