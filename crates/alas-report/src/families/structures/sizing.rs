// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (figure_structures_sizing, L5706-5818).
// Reference: alas @ rust-port-baseline.

//! Wingbox planform (spar lines + faint rib-station lines), the semi-wing
//! mass breakdown as a pie chart, and a FEM-vs-Torenbeek wing mass
//! comparison bar chart -- a read-only accuracy check against
//! `physics.mass`'s own Torenbeek estimate for this same design, not a
//! feedback loop.

use std::f64::consts::TAU;

use crate::chart_kit::{draw_axes_without_x_tick_labels, draw_title};
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::{get_palette, Palette};
use alas_pipeline::structural::StructuralAnalysisResult;

use super::{
    chord_bounds, draw_wing_outline, format_thousands, linspace, mass_breakdown_items,
    root_connected_spar_series, spar_line_series, status_message_scene,
    structures_unavailable_message, SPAR_COLORS,
};

/// Points sampled per full-circle wedge arc. A filled-contour-style
/// approximation, the same choice `Axes2D::add_heatmap_grid` documents for
/// its own curved shape: exact enough at chart scale, far simpler than a
/// true arc primitive this scene graph does not have.
const ARC_SAMPLES_FULL_CIRCLE: usize = 48;

/// `figure_structures_sizing`: degrades to [`status_message_scene`] when
/// `result` is `None` or its status is not `"ok"`, matching
/// `_structures_unavailable_message`.
pub fn figure_structures_sizing(
    result: Option<&StructuralAnalysisResult>,
    theme: Option<&str>,
) -> Scene {
    if let Some(message) = structures_unavailable_message(result) {
        return status_message_scene("Structural Analysis", &message, theme);
    }
    // `structures_unavailable_message` returning `None` means `result` is
    // `Some` with `status == "ok"`; `sizing`/`wsg` are populated by
    // construction (`alas-pipeline::structural::run_structural_analysis`
    // never returns `status: "ok"` without both). Guarding anyway rather
    // than unwrapping: a violated invariant degrades to a message instead of
    // a panic, which Python's own unguarded attribute access would not do.
    let result = match result {
        Some(r) => r,
        None => return status_message_scene("Structural Analysis", "no result", theme),
    };
    let (Some(sizing), Some(wsg)) = (result.sizing.as_ref(), result.wsg.as_ref()) else {
        return status_message_scene(
            "Structural Analysis",
            "Structural analysis result is missing sizing or geometry data.",
            theme,
        );
    };
    if sizing.y_stations.is_empty()
        || sizing.eta_stations.len() != sizing.y_stations.len()
        || sizing.chord.len() != sizing.y_stations.len()
    {
        return status_message_scene(
            "Structural Analysis",
            "Structural analysis result has no complete wingbox station data.",
            theme,
        );
    }

    let pal = get_palette(theme);
    let mut scene = Scene::new(1300.0, 520.0, Some(Color::from_hex(pal.bg)));
    let title = format!(
        "Wingbox Sizing -- governing load case: {}",
        sizing.sizing_load_case
    );
    scene.title = Some(title.clone());
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();

    // -- left: planform + rib stations -------------------------------------
    let y = &sizing.y_stations;
    let le: Vec<f64> = sizing.eta_stations.iter().map(|&e| wsg.x_le(e)).collect();
    let te: Vec<f64> = le.iter().zip(&sizing.chord).map(|(&l, &c)| l + c).collect();
    let y_max = y.last().copied().unwrap_or(1.0).max(1e-6);
    let (mut x_lo, mut x_hi) = chord_bounds(&le, &te);
    for spar in &sizing.spars {
        let root_spar_x = wsg.x_le(0.0) + spar.chord_fraction * wsg.local_chord(0.0);
        x_lo = x_lo.min(root_spar_x);
        x_hi = x_hi.max(root_spar_x);
    }
    let rib_count = sizing.num_ribs.max(0) as usize;
    let plan_rect = (60.0, 60.0, 340.0, 400.0);
    let axes = Axes2D::new(plan_rect, (0.0, y_max), (x_lo, x_hi));
    axes.draw_frame_with_labels(
        &mut scene,
        pal,
        "Spanwise position Y [m]",
        "Chordwise position X [m]",
    );
    scene.add(SceneElement::Text {
        text: format!("Wingbox planform  ({} ribs)", rib_count),
        pos: [plan_rect.0, plan_rect.1 - 6.0],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
    let installed_spacing = if rib_count > 1 {
        y_max / (rib_count - 1) as f64
    } else {
        f64::NAN
    };
    let rib_layout = if installed_spacing.is_finite() {
        format!(
            "Maximum buckling spacing: {:.3} m\nInstalled spacing: {:.3} m (root + tip included)",
            sizing.rib_spacing_m, installed_spacing
        )
    } else {
        "Rib layout is incomplete".to_owned()
    };
    scene.add(SceneElement::Text {
        text: rib_layout,
        pos: [plan_rect.0 + 4.0, plan_rect.1 + 5.0],
        font_size: 8.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });

    let rib_stroke = Stroke::new(Color::from_hex("lightgray"), 0.5);
    for ry in linspace(0.0, y_max, rib_count) {
        let eta_r = ry / wsg.semi_span;
        let le_r = wsg.x_le(eta_r);
        let te_r = le_r + wsg.local_chord(eta_r);
        axes.add_line_series(&mut scene, &[(ry, le_r), (ry, te_r)], rib_stroke.clone());
    }
    draw_wing_outline(
        &mut scene,
        &axes,
        y,
        &le,
        &te,
        Color::from_hex(pal.title),
        1.5,
    );
    for (i, spar) in sizing.spars.iter().enumerate() {
        let (spar_y, spar_x) = root_connected_spar_series(wsg, y, spar.chord_fraction);
        let color = Color::from_hex(SPAR_COLORS[i % SPAR_COLORS.len()]);
        spar_line_series(&axes, &mut scene, &spar_y, &spar_x, Stroke::new(color, 2.2));
        scene.add(SceneElement::Text {
            text: format!("Spar x/c={:.2}", spar.chord_fraction),
            pos: [
                plan_rect.0 + plan_rect.2 - 4.0,
                plan_rect.1 + 4.0 + 12.0 * i as f64,
            ],
            font_size: 8.0,
            color,
            align: TextAlign::Right,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: true,
        });
    }

    // -- middle: mass-breakdown pie chart ------------------------------------
    let pie_center = (620.0, 250.0);
    let pie_radius = 140.0;
    let items = mass_breakdown_items(&sizing.mass_breakdown_kg);
    draw_pie(&mut scene, pal, pie_center, pie_radius, &items);
    scene.add(SceneElement::Text {
        text: format!(
            "Semi-wing structural mass\n{} kg",
            format_thousands(sizing.total_mass_kg)
        ),
        pos: [pie_center.0, pie_center.1 - pie_radius - 58.0],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: true,
    });

    // -- right: FEM vs Torenbeek bar chart ------------------------------------
    let fem_full_wing = 2.0 * sizing.total_mass_kg;
    let torenbeek = result.torenbeek_wing_mass_kg;
    draw_fem_torenbeek_bars(
        &mut scene,
        pal,
        (960.0, 60.0, 300.0, 400.0),
        fem_full_wing,
        torenbeek,
    );

    scene
}

/// The pie chart: one wedge per `(label, value)` entry, tab-colored in
/// `SPAR_COLORS` order, with a white percentage label inside the wedge and
/// the component name outside it -- `ax_pie.pie(..., autopct="%1.0f%%")`.
fn draw_pie(
    scene: &mut Scene,
    pal: &Palette,
    center: (f64, f64),
    radius: f64,
    items: &[(&str, f64); 4],
) {
    let total: f64 = items.iter().map(|&(_, v)| v.max(0.0)).sum();
    if total <= 0.0 {
        return;
    }
    let mut angle = 0.0_f64;
    for (i, &(label, value)) in items.iter().enumerate() {
        let frac = value.max(0.0) / total;
        let sweep = frac * TAU;
        let color = Color::from_hex(SPAR_COLORS[i % SPAR_COLORS.len()]);

        let n_samples = ((sweep.abs() / TAU) * ARC_SAMPLES_FULL_CIRCLE as f64)
            .ceil()
            .max(1.0) as usize;
        let mut pts: Vec<[f64; 2]> = vec![[center.0, center.1]];
        for k in 0..=n_samples {
            let t = angle + sweep * (k as f64 / n_samples as f64);
            pts.push([center.0 + radius * t.cos(), center.1 - radius * t.sin()]);
        }
        pts.push([center.0, center.1]);
        scene.add(SceneElement::Polygon {
            points: pts,
            fill: Some(Fill::new(color)),
            stroke: Some(Stroke::new(Color::from_hex(pal.bg), 1.0)),
        });

        let mid = angle + sweep * 0.5;
        let label_pt = (
            center.0 + radius * 0.62 * mid.cos(),
            center.1 - radius * 0.62 * mid.sin(),
        );
        scene.add(SceneElement::Text {
            text: format!("{:.0}%", frac * 100.0),
            pos: [label_pt.0, label_pt.1],
            font_size: 9.0,
            color: Color::rgb(255, 255, 255),
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });

        let outer = (
            center.0 + radius * 1.29 * mid.cos(),
            center.1 - radius * 1.29 * mid.sin(),
        );
        scene.add(SceneElement::Text {
            text: label.to_owned(),
            pos: [outer.0, outer.1],
            font_size: 9.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Center,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: true,
        });

        angle += sweep;
    }
}

/// The FEM(both wings)-vs-Torenbeek bar chart, with the value labelled above
/// each bar and a delta-percent title when the Torenbeek estimate is a
/// finite positive number (matching `np.isfinite(torenbeek) and torenbeek >
/// 0`).
fn draw_fem_torenbeek_bars(
    scene: &mut Scene,
    pal: &Palette,
    rect: (f64, f64, f64, f64),
    fem_full_wing: f64,
    torenbeek: f64,
) {
    let max_val = fem_full_wing.max(if torenbeek.is_finite() {
        torenbeek
    } else {
        0.0
    });
    let y_max = (max_val * 1.45).max(1.0);
    let axes = Axes2D::new(rect, (0.0, 2.0), (0.0, y_max));
    draw_axes_without_x_tick_labels(&axes, scene, pal, None, Some("Mass [kg]"));

    let bars = [
        ("FEM wingbox\n(both wings)", fem_full_wing, "tab:blue"),
        ("Torenbeek\nestimate", torenbeek, "tab:red"),
    ];
    for (i, &(label, value, color)) in bars.iter().enumerate() {
        if !value.is_finite() {
            continue;
        }
        let x_center = i as f64 + 0.5;
        let bar_half_width = 0.24;
        let left = axes.map_point(x_center - bar_half_width, 0.0)[0];
        let right = axes.map_point(x_center + bar_half_width, 0.0)[0];
        let clipped_value = value.max(0.0).min(axes.y_max);
        let top = axes.map_point(x_center, clipped_value)[1];
        let baseline = axes.map_point(x_center, 0.0)[1];
        scene.add(SceneElement::Rect {
            x: left.min(right),
            y: top.min(baseline),
            width: (right - left).abs(),
            height: (baseline - top).abs(),
            rx: 0.0,
            fill: Some(Fill::new(Color::from_hex(color))),
            stroke: None,
        });
        let value_label_y = (top - 4.0).max(rect.1 + 12.0);
        scene.add(SceneElement::Text {
            text: format!("{} kg", format_thousands(value)),
            pos: [axes.map_point(x_center, 0.0)[0], value_label_y],
            font_size: 9.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Center,
            baseline: TextBaseline::Bottom,
            angle_deg: 0.0,
            bold: true,
        });
        scene.add(SceneElement::Text {
            text: label.to_owned(),
            pos: [axes.map_point(x_center, 0.0)[0], rect.1 + rect.3 + 19.0],
            font_size: 8.5,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Center,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }

    let title = if torenbeek.is_finite() && torenbeek > 0.0 {
        let err_pct = (fem_full_wing - torenbeek) / torenbeek * 100.0;
        format!("FEM vs Torenbeek wing mass  (delta = {err_pct:+.0}%)")
    } else {
        "FEM vs Torenbeek wing mass".to_owned()
    };
    scene.add(SceneElement::Text {
        text: title,
        pos: [rect.0, rect.1 - 6.0],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });
}

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alas_config::materials::get as get_material;
    use alas_config::{DesignRequirements, DesignVector, StructuresConfig, WingConfig};
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::wing_structure::WingStructureGeometry;
    use alas_struct::sizing::size_wingbox;

    fn sample_result() -> StructuralAnalysisResult {
        let cfg = StructuresConfig::default();
        let req = DesignRequirements::default();
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
        StructuralAnalysisResult {
            status: "ok".to_owned(),
            error: None,
            wsg: Some(wsg),
            sizing: Some(sizing),
            mesh_health: None,
            analysis: None,
            nastran: None,
            nastran95: None,
            patran: None,
            torenbeek_wing_mass_kg: 15000.0,
        }
    }

    #[test]
    fn a_missing_result_degrades_to_a_status_message() {
        let scene = figure_structures_sizing(None, None);
        let has_status_text = scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Text { text, .. } if text.contains("not run")));
        assert!(has_status_text);
    }

    #[test]
    fn a_failed_result_reports_its_own_error_string() {
        let failed = StructuralAnalysisResult {
            status: "error".to_owned(),
            error: Some("bad spar layout".to_owned()),
            ..Default::default()
        };
        let scene = figure_structures_sizing(Some(&failed), None);
        let has_error_text = scene.elements.iter().any(
            |e| matches!(e, SceneElement::Text { text, .. } if text.contains("bad spar layout")),
        );
        assert!(has_error_text);
    }

    #[test]
    fn an_ok_result_draws_four_pie_wedges_and_two_bars() {
        let result = sample_result();
        let scene = figure_structures_sizing(Some(&result), Some("dark"));
        let polygons = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Polygon { .. }))
            .count();
        assert_eq!(polygons, 4, "one wedge per mass component");
        let rects = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Rect { fill: Some(_), .. }))
            .count();
        assert_eq!(rects, 2, "one bar for FEM, one for Torenbeek");
        let visible_planform_lines = scene
            .elements
            .iter()
            .filter(|element| match element {
                SceneElement::Polyline { points, .. } => points.iter().all(|point| {
                    (60.0..=400.0).contains(&point[0]) && (60.0..=460.0).contains(&point[1])
                }),
                _ => false,
            })
            .count();
        assert!(
            visible_planform_lines >= 6,
            "got {visible_planform_lines} visible lines"
        );
        let title = scene.title.expect("title set");
        assert!(title.contains(result.sizing.expect("sizing present").sizing_load_case));
    }

    #[test]
    fn comparison_bar_categories_are_wrapped_below_numeric_ticks() {
        let result = sample_result();
        let scene = figure_structures_sizing(Some(&result), None);
        let labels = scene.elements.iter().filter_map(|element| match element {
            SceneElement::Text { text, pos, .. }
                if text == "FEM wingbox\n(both wings)" || text == "Torenbeek\nestimate" =>
            {
                Some(pos)
            }
            _ => None,
        });
        assert_eq!(labels.count(), 2);
        assert!(!scene.elements.iter().any(
            |element| matches!(element, SceneElement::Text { text, .. } if text == "Estimate")
        ));
    }

    #[test]
    fn the_bar_chart_title_reports_the_real_delta_percent() {
        let mut result = sample_result();
        result.torenbeek_wing_mass_kg = result
            .sizing
            .as_ref()
            .expect("sizing present")
            .total_mass_kg;
        // FEM full wing is 2x the semi-wing sizing mass; with Torenbeek set
        // to the semi-wing mass exactly, delta = 2x - 1x = +100%.
        let scene = figure_structures_sizing(Some(&result), None);
        let has_delta = scene
            .elements
            .iter()
            .any(|e| matches!(e, SceneElement::Text { text, .. } if text.contains("+100%")));
        assert!(has_delta);
    }
}
