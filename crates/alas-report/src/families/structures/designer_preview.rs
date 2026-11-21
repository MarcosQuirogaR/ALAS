// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (figure_structures_designer_preview, L5634-5705).

//! Narrow live preview for the Structural Analysis
//! Advanced Settings tab: the same relationship as
//! `figure_engine_designer_preview` vs. `figure_propulsion_cycle_summary`
//! upstream describes: the Results tab's wide `figure_structures_sizing`
//! is squeezed and illegible in this tab's narrow column, so this is a
//! distinct, cheap (sizing-only, no FEM mesh, no NASTRAN) figure built
//! directly from [`WingStructureGeometry`]/[`WingboxSizing`].

use crate::chart_kit::{draw_inline_legend, LegendMarker};
use crate::scene::{Axes2D, Color, Scene, Stroke};
use crate::theme::get_palette;
use alas_geom::wing_structure::WingStructureGeometry;
use alas_struct::sizing::WingboxSizing;

use super::{
    chord_bounds, draw_wing_outline, root_connected_spar_series, spar_line_series, SPAR_COLORS,
};

/// The canvas the planform is drawn on when the caller names none.
pub const STRUCTURES_PREVIEW_CANVAS: (f64, f64) = (640.0, 420.0);

/// The plot rectangle `(left, top, width, height)` for a canvas: room above for
/// the spar key, left for the tick labels and the rotated title, below for the
/// tick labels and the x title.
pub fn structures_plot_rect(canvas: (f64, f64)) -> (f64, f64, f64, f64) {
    let (left, top, right, bottom) = (84.0, 44.0, 20.0, 60.0);
    (left, top, canvas.0 - left - right, canvas.1 - top - bottom)
}

/// Wingbox planform and spar traces without a secondary sizing summary.
pub fn figure_structures_designer_preview(
    wsg: &WingStructureGeometry,
    sizing: &WingboxSizing,
    theme: Option<&str>,
) -> Scene {
    figure_structures_designer_preview_sized(wsg, sizing, theme, STRUCTURES_PREVIEW_CANVAS)
}

/// The scene size that draws the full planform (both wings, as seen from
/// above) at one scale on both axes, no larger than `limit` (scene units):
/// the width is used first, and the height follows from the geometry unless
/// `limit` does not allow it, in which case the width shrinks to match.
pub fn structures_true_aspect_canvas(y_max: f64, x_extent: f64, limit: (f64, f64)) -> (f64, f64) {
    let (_, _, plot_w, plot_h) = structures_plot_rect(limit);
    let margins = (limit.0 - plot_w, limit.1 - plot_h);
    let scale = (plot_w / (2.0 * y_max.max(1e-6))).min(plot_h / x_extent.max(1e-6));
    (
        (2.0 * y_max * scale + margins.0).round(),
        (x_extent * scale + margins.1).round(),
    )
}

/// The planform on a `canvas` of the caller's choosing (scene units). The
/// canvas is a limit: the drawing is made at one scale on both axes (the
/// geometry's true aspect) and the scene is as large as that needs, never
/// larger than `canvas`.
pub fn figure_structures_designer_preview_sized(
    wsg: &WingStructureGeometry,
    sizing: &WingboxSizing,
    theme: Option<&str>,
    canvas: (f64, f64),
) -> Scene {
    let pal = get_palette(theme);
    let y = &sizing.y_stations;
    let le: Vec<f64> = sizing.eta_stations.iter().map(|&e| wsg.x_le(e)).collect();
    let te: Vec<f64> = le.iter().zip(&sizing.chord).map(|(&l, &c)| l + c).collect();

    let y_max = y.last().copied().unwrap_or(1.0).max(1e-6);
    let (x_lo, x_hi) = chord_bounds(&le, &te);
    let canvas = structures_true_aspect_canvas(y_max, x_hi - x_lo, canvas);
    let mut scene = Scene::new(canvas.0, canvas.1, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Structural Rib & Spar Grid Preview".to_owned());
    scene.suppress_derived_title();
    let rect = structures_plot_rect(canvas);
    // Both wings: the planform is symmetric about the centreline.
    let axes = Axes2D::new(rect, (-y_max, y_max), (x_lo, x_hi)).with_tick_font(11.0);
    axes.draw_frame_with_labels(
        &mut scene,
        pal,
        "Spanwise position Y [m]",
        "Chordwise position X [m]",
    );

    let mirrored: Vec<f64> = y.iter().map(|value| -value).collect();
    for side in [y.as_slice(), mirrored.as_slice()] {
        draw_wing_outline(
            &mut scene,
            &axes,
            side,
            &le,
            &te,
            Color::from_hex(pal.title),
            1.5,
        );
    }

    // The spar key sits above the plot, outside the drawing: one swatch per
    // spar in its own colour, labelled with its chord fraction.
    let mut key = Vec::new();
    for (i, spar) in sizing.spars.iter().enumerate() {
        let (spar_y, spar_x) = root_connected_spar_series(wsg, y, spar.chord_fraction);
        let color = Color::from_hex(SPAR_COLORS[i % SPAR_COLORS.len()]);
        let spar_mirror: Vec<f64> = spar_y.iter().map(|value| -value).collect();
        for side in [&spar_y, &spar_mirror] {
            spar_line_series(&axes, &mut scene, side, &spar_x, Stroke::new(color, 2.0));
        }
        key.push((
            format!("x/c = {:.2}", spar.chord_fraction),
            LegendMarker::Line(Stroke::new(color, 2.4)),
        ));
    }
    draw_inline_legend(&mut scene, [rect.0, rect.1 * 0.5], &key, pal, 12.0);

    scene
}

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::families::structures::true_spar_xy;
    use crate::scene::SceneElement;
    use alas_config::{DesignVector, WingConfig};
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_struct::sizing::{MassBreakdown, SparSizing};

    fn wsg() -> WingStructureGeometry {
        WingStructureGeometry::new(
            &DesignVector::default(),
            &WingConfig::default(),
            &Airfoil::from_name("naca4412").expect("valid NACA name"),
            &Airfoil::from_name("naca2410").expect("valid NACA name"),
            &[0.15, 0.65],
            None,
        )
        .expect("two full-span spars is a valid configuration")
    }

    fn sample_sizing() -> WingboxSizing {
        let n = 5;
        let y: Vec<f64> = (0..n).map(|i| i as f64 * 18.0 / (n - 1) as f64).collect();
        let eta: Vec<f64> = y.iter().map(|&yv| yv / 18.0).collect();
        let chord: Vec<f64> = eta.iter().map(|&e| 6.0 * (1.0 - 0.7 * e)).collect();
        let spar = |frac: f64| SparSizing {
            chord_fraction: frac,
            h: vec![1.0; n],
            w_cap: vec![0.3; n],
            t_cap: vec![0.02; n],
            a_cap: vec![0.006; n],
            t_web: vec![0.006; n],
            lower_extra_area: vec![0.0; n],
            frac_moment: vec![0.5; n],
            margin_of_safety: vec![0.1; n],
        };
        WingboxSizing {
            y_stations: y,
            axis_cos: Vec::new(),
            cover_shape: Vec::new(),
            cover_depth_m: Vec::new(),
            cover_shear_flow_n_per_m: Vec::new(),
            eta_stations: eta,
            chord,
            spar_fracs: vec![0.15, 0.65],
            spars: vec![spar(0.15), spar(0.65)],
            t_skin: 0.004,
            skin_gauge_m: Vec::new(),
            num_ribs: 12,
            rib_spacing_m: 1.5,
            mass_breakdown_kg: MassBreakdown {
                spar_caps: 1500.0,
                spar_webs: 800.0,
                skin: 3000.0,
                ribs: 900.0,
            },
            total_mass_kg: 6200.0,
            sizing_load_case: "pull-up",
            composite_declaration: None,
        }
    }

    #[test]
    fn draws_one_spar_line_per_spar_and_the_wing_outline() {
        let geometry = wsg();
        let sizing = sample_sizing();
        let scene = figure_structures_designer_preview(&geometry, &sizing, Some("dark"));
        let polylines = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Polyline { .. }))
            .count();
        // 4 outline segments (LE, TE, root, tip) + at least 2 spar runs.
        assert!(polylines >= 6, "got {polylines} polylines");
        let visible_planform_lines = scene
            .elements
            .iter()
            .filter(|element| match element {
                SceneElement::Polyline { points, .. } => points.iter().all(|point| {
                    let rect = structures_plot_rect(STRUCTURES_PREVIEW_CANVAS);
                    (rect.0..=rect.0 + rect.2).contains(&point[0])
                        && (rect.1..=rect.1 + rect.3).contains(&point[1])
                }),
                _ => false,
            })
            .count();
        assert!(
            visible_planform_lines >= 6,
            "got {visible_planform_lines} visible lines"
        );
    }

    #[test]
    fn the_scene_paints_no_second_title_and_keeps_the_spar_key_outside_the_plot() {
        let scene = figure_structures_designer_preview(&wsg(), &sample_sizing(), Some("dark"));

        assert_eq!(
            scene.title.as_deref(),
            Some("Structural Rib & Spar Grid Preview")
        );
        assert!(!scene.render_title);
        assert!(crate::scene::visual_title(&scene).is_none());
        let rect = structures_plot_rect(STRUCTURES_PREVIEW_CANVAS);
        let mut keys = 0;
        for element in &scene.elements {
            if let SceneElement::Text { text, pos, .. } = element {
                assert_ne!(
                    text, "Wingbox planform",
                    "the page heading names the figure"
                );
                if text.starts_with("x/c = ") {
                    keys += 1;
                    assert!(pos[1] < rect.1, "spar key above the plot: {pos:?}");
                }
            }
        }
        assert_eq!(keys, 2);
    }

    #[test]
    fn the_planform_preview_has_no_sizing_prose_panel() {
        let scene = figure_structures_designer_preview(&wsg(), &sample_sizing(), None);
        let texts: Vec<&str> = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert!(!texts
            .iter()
            .any(|text| text.starts_with("Governing load case:")));
        assert!(!texts.iter().any(|text| text.starts_with("Ribs:")));
        assert!(!texts.iter().any(|text| text.contains("Semi-wing mass")));
    }

    #[test]
    fn both_spar_traces_reach_the_spanwise_root_from_existing_geometry() {
        let geometry = wsg();
        let mut sizing = sample_sizing();
        sizing.y_stations = vec![2.0, 6.0, 10.0, 14.0, 18.0];
        sizing.eta_stations = sizing.y_stations.iter().map(|y| y / 18.0).collect();
        sizing.chord = sizing
            .eta_stations
            .iter()
            .map(|&eta| geometry.local_chord(eta))
            .collect();
        let scene = figure_structures_designer_preview(&geometry, &sizing, Some("light"));

        // The planform shows both wings about the centreline, so the root
        // (y = 0) is the horizontal centre of the plot rectangle of the
        // scene actually drawn, and each spar has one trace per half-wing,
        // each starting there.
        let (left, _, width, _) = structures_plot_rect((scene.width, scene.height));
        let centre = left + width / 2.0;
        for color_name in ["tab:blue", "tab:orange"] {
            let color = Color::from_hex(color_name);
            let traces: Vec<&Vec<[f64; 2]>> = scene
                .elements
                .iter()
                .filter_map(|element| match element {
                    SceneElement::Polyline { points, stroke }
                        if stroke.color == color && points.len() >= 2 =>
                    {
                        Some(points)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(traces.len(), 2, "{color_name}: one trace per half-wing");
            let (a, b) = (traces[0], traces[1]);
            for points in [a, b] {
                assert!(
                    (points[0][0] - centre).abs() < 1e-6,
                    "{color_name} first point is {:?}, centreline x is {centre}",
                    points[0]
                );
            }
            // The halves leave the root in opposite directions.
            let (last_a, last_b) = (a[a.len() - 1][0], b[b.len() - 1][0]);
            assert!(
                (last_a - centre) * (last_b - centre) < 0.0,
                "{color_name} halves end at {last_a} and {last_b}"
            );
        }
    }

    #[test]
    fn a_partial_span_spar_breaks_its_line_at_the_break_station() {
        let geometry = WingStructureGeometry::new(
            &DesignVector::default(),
            &WingConfig::default(),
            &Airfoil::from_name("naca4412").expect("valid NACA name"),
            &Airfoil::from_name("naca2410").expect("valid NACA name"),
            &[0.15, 0.65, 0.5],
            Some(&[true, true, false]),
        )
        .expect("mixed full/partial span spars is valid");
        let y = vec![0.0, 5.0, geometry.y_break + 1.0, 15.0, 18.0];
        let spar_x = true_spar_xy(&geometry, &y, 0.5);
        // The partial-span spar (sorted to index 1) must go NaN once the
        // station passes its own break-station endpoint.
        assert!(spar_x[0].is_finite());
        assert!(spar_x.last().expect("non-empty").is_nan());
    }
}
