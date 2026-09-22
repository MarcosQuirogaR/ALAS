// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (figure_structures_loads, L5819-5906).
// Reference: alas @ rust-port-baseline.

//! Bending stiffness `EI(y)` and moment `M(y)` for the sizing-governing load
//! case (left, dual axis), and the spanwise deflection curve for every load
//! case with real MSC and NASA NASTRAN-95 tip-deflection markers overlaid when
//! available (right). Python does not plot torsion anywhere in this figure: a stale
//! doc comment in the previous stub implied otherwise; verified against
//! `visualization.py` L5819-5906 directly.

use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::{get_palette, Palette};
use alas_pipeline::structural::StructuralAnalysisResult;
use alas_struct::analytical::LoadCaseResult;
use alas_struct::nastran::ResultStatus;

use super::{status_message_scene, structures_unavailable_message, TAB_GRAY};

const MSC_NASTRAN_COLOR: &str = "tab:red";
const NASTRAN95_COLOR: &str = "tab:orange";

/// `figure_structures_loads`: degrades to [`status_message_scene`] on the
/// same terms [`super::sizing::figure_structures_sizing`] does.
pub fn figure_structures_loads(
    result: Option<&StructuralAnalysisResult>,
    theme: Option<&str>,
) -> Scene {
    if let Some(message) = structures_unavailable_message(result) {
        return status_message_scene("Structural Analysis: Loads", &message, theme);
    }
    let Some(result) = result else {
        return status_message_scene("Structural Analysis: Loads", "no result", theme);
    };
    let (Some(sizing), Some(analysis)) = (result.sizing.as_ref(), result.analysis.as_ref()) else {
        return status_message_scene(
            "Structural Analysis: Loads",
            "Structural analysis result is missing sizing or analytical data.",
            theme,
        );
    };
    let Some(governing) = analysis
        .load_cases
        .iter()
        .find(|lc| lc.name == sizing.sizing_load_case)
    else {
        return status_message_scene(
            "Structural Analysis: Loads",
            "Sizing load case not found among the analytical load cases.",
            theme,
        );
    };

    let pal = get_palette(theme);
    let mut scene = Scene::new(1100.0, 600.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Structural Analysis: Loads".to_owned());

    draw_stiffness_moment_panel(
        &mut scene,
        pal,
        (70.0, 60.0, 440.0, 400.0),
        analysis,
        sizing,
        governing,
    );
    draw_deflection_panel(
        &mut scene,
        pal,
        (620.0, 60.0, 440.0, 400.0),
        analysis,
        result,
    );

    scene
}

/// Left panel: `EI(y)` (blue, filled under the curve, left axis) and `M(y)`
/// for the governing case (orange dashed, right axis): a dual axis built
/// as two [`Axes2D`] sharing one pixel rect and x-range with independent
/// y-ranges, one frame drawn once, the right axis's ticks placed manually.
fn draw_stiffness_moment_panel(
    scene: &mut Scene,
    pal: &Palette,
    rect: (f64, f64, f64, f64),
    analysis: &alas_struct::analytical::StructuralAnalysisReport,
    sizing: &alas_struct::sizing::WingboxSizing,
    governing: &LoadCaseResult,
) {
    let y_max = analysis.y.last().copied().unwrap_or(1.0).max(1e-6);
    let ei_max = analysis.ei_nm2.iter().copied().fold(0.0_f64, f64::max) / 1e9;
    let (m_lo, m_hi) = autoscale_padded(governing.moment_nm.iter().map(|&m| m / 1e6));

    let axes_ei = Axes2D::new(rect, (0.0, y_max), (0.0, (ei_max * 1.08).max(1e-6)));
    let axes_m = Axes2D::new(rect, (0.0, y_max), (m_lo, m_hi));
    axes_ei.draw_frame_with_labels(
        scene,
        pal,
        "Spanwise position Y [m]",
        "Bending stiffness EI [GN.m^2]",
    );

    scene.add(SceneElement::Text {
        text: "Stiffness & moment distribution".to_owned(),
        pos: [rect.0, rect.1 - 6.0],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });

    // EI(y), filled under the curve down to y=0 (the data baseline, not the
    // axis minimum: matches `ax_l.fill_between(y, 0, EI/1e9)`).
    let ei_pts: Vec<(f64, f64)> = analysis
        .y
        .iter()
        .zip(&analysis.ei_nm2)
        .map(|(&yv, &ei)| (yv, ei / 1e9))
        .collect();
    let mut area: Vec<[f64; 2]> = ei_pts
        .iter()
        .map(|&(x, y)| axes_ei.map_point(x, y))
        .collect();
    area.extend(ei_pts.iter().rev().map(|&(x, _)| axes_ei.map_point(x, 0.0)));
    let mut fill_color = Color::from_hex("tab:blue");
    fill_color.a = 30;
    scene.add(SceneElement::Polygon {
        points: area,
        fill: Some(Fill::new(fill_color)),
        stroke: None,
    });
    axes_ei.add_line_series(
        scene,
        &ei_pts,
        Stroke::new(Color::from_hex("tab:blue"), 2.0),
    );

    // M(y) for the governing case, dashed orange on the right scale.
    let m_pts: Vec<(f64, f64)> = governing
        .y
        .iter()
        .zip(&governing.moment_nm)
        .map(|(&yv, &m)| (yv, m / 1e6))
        .collect();
    axes_m.add_line_series(
        scene,
        &m_pts,
        Stroke::dashed(Color::from_hex("tab:orange"), 1.8, 6.0, 4.0),
    );

    // The shared chart frame supplies the left-axis tick marks and labels;
    // only the independent right moment scale needs a local tick pass.
    draw_axis_ticks(scene, &axes_m, rect, "tab:orange", false);
    draw_rotated_label(
        scene,
        "Bending moment M [MN.m]",
        "tab:orange",
        (rect.0 + rect.2 + 46.0, rect.1 + rect.3 * 0.5),
    );

    crate::chart_kit::draw_legend(
        scene,
        [rect.0 + rect.2 - 150.0, rect.1 + 6.0],
        &[
            (
                "EI(y)".to_owned(),
                crate::chart_kit::LegendMarker::Line(Stroke::new(Color::from_hex("tab:blue"), 2.0)),
            ),
            (
                format!("M(y), {}", sizing.sizing_load_case),
                crate::chart_kit::LegendMarker::Line(Stroke::dashed(
                    Color::from_hex("tab:orange"),
                    1.8,
                    6.0,
                    4.0,
                )),
            ),
        ],
        pal,
        8.0,
    );
}

/// Right panel: spanwise deflection for every load case (colored by name,
/// gray fallback), plus red diamond MSC and orange hollow-circular
/// NASTRAN-95 tip-deflection markers when the corresponding static solve
/// succeeded. The scale includes every numerical tip so a solver comparison
/// cannot be clipped by the analytical-only envelope.
fn draw_deflection_panel(
    scene: &mut Scene,
    pal: &Palette,
    rect: (f64, f64, f64, f64),
    analysis: &alas_struct::analytical::StructuralAnalysisReport,
    result: &StructuralAnalysisResult,
) {
    let y_max = analysis.y.last().copied().unwrap_or(1.0).max(1e-6);
    let mut deflections = analysis
        .load_cases
        .iter()
        .flat_map(|lc| lc.deflection_m.iter().copied())
        .collect::<Vec<_>>();
    if let Some(nastran) = result.nastran.as_ref() {
        if nastran.static_solve.status == ResultStatus::Ok {
            deflections.extend(
                nastran
                    .static_solve
                    .tip_deflection_m
                    .iter()
                    .map(|(_, value)| value),
            );
        }
    }
    if let Some(nastran95) = result.nastran95.as_ref() {
        if nastran95.static_solve.status == ResultStatus::Ok {
            deflections.extend(
                nastran95
                    .static_solve
                    .tip_deflection_m
                    .iter()
                    .map(|(_, value)| value),
            );
        }
    }
    let (d_lo, d_hi) = autoscale_padded(deflections.into_iter());
    let axes = Axes2D::new(rect, (0.0, y_max), (d_lo, d_hi));
    axes.draw_frame_with_labels(
        scene,
        pal,
        "Spanwise position Y [m]",
        "Deflection delta [m]",
    );
    scene.add(SceneElement::Text {
        text: "Spanwise deflection (analytical, Euler-Bernoulli)".to_owned(),
        pos: [rect.0, rect.1 - 6.0],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });

    // axhline(0): the zero-deflection reference.
    axes.add_line_series(
        scene,
        &[(0.0, 0.0), (y_max, 0.0)],
        Stroke::new(Color::rgba(0, 0, 0, 102), 0.7),
    );

    let mut legend_entries = Vec::new();
    for lc in &analysis.load_cases {
        let color_hex = load_case_color(lc.name);
        let color = Color::from_hex(color_hex);
        let pts: Vec<(f64, f64)> =
            lc.y.iter()
                .zip(&lc.deflection_m)
                .map(|(&y, &d)| (y, d))
                .collect();
        axes.add_line_series(scene, &pts, Stroke::new(color, 2.0));
        legend_entries.push((
            format!("{} analytical", lc.name),
            crate::chart_kit::LegendMarker::Line(Stroke::new(color, 2.0)),
        ));
    }

    if let Some(nastran) = result.nastran.as_ref() {
        if nastran.static_solve.status == ResultStatus::Ok {
            for (_, tip_defl) in nastran.static_solve.tip_deflection_m.iter() {
                let color = Color::from_hex(MSC_NASTRAN_COLOR);
                let center = axes.map_point(y_max, tip_defl);
                draw_diamond(scene, center, 6.0, color);
            }
            legend_entries.push((
                "MSC NASTRAN SOL 101 tips".to_owned(),
                crate::chart_kit::LegendMarker::Patch(Color::from_hex(MSC_NASTRAN_COLOR)),
            ));
        }
    }

    if let Some(nastran95) = result.nastran95.as_ref() {
        if nastran95.static_solve.status == ResultStatus::Ok {
            for (_, tip_defl) in nastran95.static_solve.tip_deflection_m.iter() {
                let color = Color::from_hex(NASTRAN95_COLOR);
                let center = axes.map_point(y_max, tip_defl);
                draw_circle_marker(scene, center, 5.0, color);
            }
            legend_entries.push((
                "NASTRAN-95 SOL 101 tips".to_owned(),
                crate::chart_kit::LegendMarker::Circle(Color::from_hex(NASTRAN95_COLOR)),
            ));
        } else if nastran95.static_solve.status == ResultStatus::Error {
            let detail = nastran95
                .static_solve
                .error
                .as_deref()
                .map_or_else(|| "no detail was retained".to_owned(), compact_error);
            scene.add(SceneElement::Text {
                text: format!("NASTRAN-95 SOL 101 unavailable: {detail}"),
                pos: [rect.0, rect.1 + rect.3 + 64.0],
                font_size: 8.0,
                color: Color::from_hex("tab:red"),
                align: TextAlign::Left,
                baseline: TextBaseline::Middle,
                angle_deg: 0.0,
                bold: false,
            });
        }
    }

    crate::chart_kit::draw_horizontal_legend_columns(
        scene,
        [70.0, 530.0],
        &legend_entries,
        pal,
        7.5,
    );
}

/// Keep a retained solver diagnostic legible in the narrow figure footer.
fn compact_error(error: &str) -> String {
    const MAX_CHARS: usize = 82;
    let first_line = error
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(error);
    let mut chars = first_line.trim().chars();
    let mut compact: String = chars.by_ref().take(MAX_CHARS).collect();
    if chars.next().is_some() {
        compact.push_str("...");
    }
    compact
}

/// The load-case-to-color map `figure_structures_loads` uses (`colors =
/// {"pull-up": "tab:red", "push-down": "tab:blue", "level": "tab:green"}`),
/// falling back to `tab:gray` for anything else.
fn load_case_color(name: &str) -> &'static str {
    match name {
        "pull-up" => "tab:red",
        "push-down" => "tab:blue",
        "level" => "tab:green",
        _ => TAB_GRAY,
    }
}

/// A diamond marker (a 4-point polygon rotated 45 degrees), approximating
/// matplotlib's `marker="D"` scatter point; this scene graph has no
/// dedicated diamond primitive.
fn draw_diamond(scene: &mut Scene, center: [f64; 2], half_size: f64, color: Color) {
    let [cx, cy] = center;
    scene.add(SceneElement::Polygon {
        points: vec![
            [cx, cy - half_size],
            [cx + half_size, cy],
            [cx, cy + half_size],
            [cx - half_size, cy],
        ],
        fill: Some(Fill::new(color)),
        stroke: Some(Stroke::new(Color::rgb(0, 0, 0), 1.0)),
    });
}

/// A hollow circular marker for the local NASTRAN-95 backend, kept visually
/// distinct from, and non-occluding of, the diamond used for the modern
/// MSC result at the same physical tip location.
fn draw_circle_marker(scene: &mut Scene, center: [f64; 2], radius: f64, color: Color) {
    scene.add(SceneElement::Circle {
        center,
        radius,
        fill: None,
        stroke: Some(Stroke::new(color, 2.0)),
    });
}

/// Autoscale a Y range from data, matplotlib-style: force the zero baseline
/// into range and pad the finite extent by 5%, falling back to `[0, 1]` when
/// there is no finite data.
fn autoscale_padded(values: impl Iterator<Item = f64>) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in values {
        if v.is_finite() {
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    if !lo.is_finite() || !hi.is_finite() {
        return (0.0, 1.0);
    }
    lo = lo.min(0.0);
    hi = hi.max(0.0);
    let span = (hi - lo).max(1e-9);
    (lo - span * 0.05, hi + span * 0.05)
}

/// Five evenly spaced tick labels along one axis's Y range, placed outside
/// the shared frame: left-aligned-right for the left (EI) axis,
/// left-aligned for the right (moment) axis.
fn draw_axis_ticks(
    scene: &mut Scene,
    axes: &Axes2D,
    rect: (f64, f64, f64, f64),
    color: &str,
    left: bool,
) {
    for i in 0..=4 {
        let frac = i as f64 / 4.0;
        let val = axes.y_min + frac * (axes.y_max - axes.y_min);
        let p = axes.map_point(axes.x_min, val);
        let (x, align) = if left {
            (rect.0 - 4.0, TextAlign::Right)
        } else {
            (rect.0 + rect.2 + 4.0, TextAlign::Left)
        };
        let tick_x = if left { rect.0 } else { rect.0 + rect.2 };
        let tick_end = if left { tick_x - 4.0 } else { tick_x + 4.0 };
        scene.add(SceneElement::Line {
            p1: [tick_x, p[1]],
            p2: [tick_end, p[1]],
            stroke: Stroke::new(Color::from_hex(color), 1.0),
        });
        scene.add(SceneElement::Text {
            text: format!("{val:.1}"),
            pos: [x, p[1]],
            font_size: 8.0,
            color: Color::from_hex(color),
            align,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
    }
}

/// A rotated axis-label, matching `draw_colorbar`'s own rotated-label
/// convention in `chart_kit`.
fn draw_rotated_label(scene: &mut Scene, text: &str, color: &str, pos: (f64, f64)) {
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos: [pos.0, pos.1],
        font_size: 9.0,
        color: Color::from_hex(color),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: false,
    });
}

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alas_config::materials::get as get_material;
    use alas_config::{
        DesignRequirements, DesignVector, EngineConfig, MassModelConfig, StructuresConfig,
        WingConfig,
    };
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::wing_structure::WingStructureGeometry;
    use alas_struct::analytical::analyze_structure;
    use alas_struct::nastran::{LabelledValues, NastranResults, StaticResult};
    use alas_struct::sizing::size_wingbox;

    fn sample_result(with_nastran: bool) -> StructuralAnalysisResult {
        let cfg = StructuresConfig::default();
        let req = DesignRequirements::default();
        let engine_cfg = EngineConfig::default();
        let mass_cfg = MassModelConfig::default();
        let skin = get_material(&cfg.skin_material).expect("valid material");
        let web = get_material(&cfg.spar_web_material).expect("valid material");
        let cap = get_material(&cfg.spar_cap_material).expect("valid material");
        let rib = get_material(&cfg.rib_material).expect("valid material");
        let wsg = WingStructureGeometry::new(
            &DesignVector::default(),
            &WingConfig::default(),
            &Airfoil::from_name("naca4412").expect("valid NACA name"),
            &Airfoil::from_name("naca2410").expect("valid NACA name"),
            &[0.15, 0.65],
            None,
        )
        .expect("two full-span spars is a valid configuration");
        let sizing = size_wingbox(&wsg, &cfg, &req, skin, web, cap, rib);
        let analysis = analyze_structure(
            &wsg,
            &sizing,
            &cfg,
            &req,
            &engine_cfg,
            &mass_cfg,
            skin,
            web,
            cap,
        );

        let nastran = if with_nastran {
            let mut tip = LabelledValues::default();
            for lc in &analysis.load_cases {
                tip.push(lc.name, lc.tip_deflection_m * 0.97);
            }
            Some(NastranResults {
                static_solve: StaticResult {
                    status: ResultStatus::Ok,
                    tip_deflection_m: tip,
                    ..Default::default()
                },
                ..Default::default()
            })
        } else {
            None
        };

        StructuralAnalysisResult {
            status: "ok".to_owned(),
            error: None,
            wsg: Some(wsg),
            sizing: Some(sizing),
            mesh_health: None,
            analysis: Some(analysis),
            nastran,
            nastran95: None,
            patran: None,
            torenbeek_wing_mass_kg: 15000.0,
        }
    }

    #[test]
    fn a_missing_result_degrades_to_a_status_message() {
        let scene = figure_structures_loads(None, None);
        assert!(scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Text { text, .. } | SceneElement::TextBlock { text, .. } if text.contains("not run"))));
    }

    #[test]
    fn draws_one_deflection_curve_per_load_case() {
        let result = sample_result(false);
        let scene = figure_structures_loads(Some(&result), Some("light"));
        let polylines = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Polyline { .. }))
            .count();
        // 3 load-case curves + EI curve + M curve + the zero reference line, at least.
        assert!(polylines >= 6, "got {polylines} polylines");
        // No NASTRAN markers without a NASTRAN result.
        assert!(!scene.elements.iter().any(
            |e| matches!(e, SceneElement::Text { text, .. } if text.contains("SOL 101 tips"))
        ));
    }

    #[test]
    fn nastran_tip_deflection_draws_one_diamond_marker_per_load_case() {
        let result = sample_result(true);
        let scene = figure_structures_loads(Some(&result), None);
        let diamonds = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Polygon { points, .. } if points.len() == 4))
            .count();
        assert_eq!(diamonds, 3);
        assert!(scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Text { text, .. } if text.contains("MSC NASTRAN SOL 101 tips"))));
    }

    #[test]
    fn both_solver_tip_deflections_are_visible_in_the_same_panel() {
        let mut result = sample_result(true);
        let mut tip = LabelledValues::default();
        for lc in result
            .analysis
            .as_ref()
            .expect("sample has analysis")
            .load_cases
            .iter()
        {
            tip.push(lc.name, lc.tip_deflection_m * 0.91);
        }
        result.nastran95 = Some(NastranResults {
            static_solve: StaticResult {
                status: ResultStatus::Ok,
                tip_deflection_m: tip,
                ..Default::default()
            },
            ..Default::default()
        });

        let scene = figure_structures_loads(Some(&result), None);
        let circles = scene
            .elements
            .iter()
            .filter(|element| matches!(element, SceneElement::Circle { .. }))
            .count();
        assert!(
            circles >= 4,
            "three local markers plus the local-solver legend entry"
        );
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("MSC NASTRAN SOL 101 tips"))
        }));
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. } if text.contains("NASTRAN-95 SOL 101 tips"))
        }));
    }

    #[test]
    fn numerical_tip_markers_expand_the_deflection_axis_instead_of_clipping() {
        let mut result = sample_result(true);
        result
            .nastran
            .as_mut()
            .expect("sample contains the MSC result")
            .static_solve
            .tip_deflection_m
            .push("outlier", 100.0);

        let scene = figure_structures_loads(Some(&result), None);
        let outlier = scene.elements.iter().find_map(|element| match element {
            SceneElement::Polygon { points, .. }
                if points.len() == 4
                    && points.iter().all(|point| point[0] > 1_000.0)
                    && points.iter().map(|point| point[1]).sum::<f64>() < 400.0 =>
            {
                Some(points)
            }
            _ => None,
        });
        let Some(outlier) = outlier else {
            panic!("the numerical outlier should retain its MSC diamond");
        };
        assert!(
            outlier
                .iter()
                .all(|point| point[1] > 60.0 && point[1] < 460.0),
            "autoscaling must keep the complete numerical marker inside the plot: {outlier:?}"
        );
    }

    #[test]
    fn a_local_solver_failure_is_named_in_the_deflection_panel() {
        let mut result = sample_result(true);
        result.nastran95 = Some(NastranResults {
            static_solve: StaticResult {
                status: ResultStatus::Error,
                error: Some("open-core allocation exceeds the local build cap".to_owned()),
                ..Default::default()
            },
            ..Default::default()
        });

        let scene = figure_structures_loads(Some(&result), None);
        assert!(scene.elements.iter().any(|element| {
            matches!(element, SceneElement::Text { text, .. }
                if text.contains("NASTRAN-95 SOL 101 unavailable")
                && text.contains("open-core allocation"))
        }));
    }
}
