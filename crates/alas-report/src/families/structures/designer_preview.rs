// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (figure_structures_designer_preview, L5634-5705).
// Reference: alas @ rust-port-baseline.

//! Narrow live preview for the Structural Analysis
//! Advanced Settings tab: the same relationship as
//! `figure_engine_designer_preview` vs. `figure_propulsion_cycle_summary`
//! upstream describes -- the Results tab's wide `figure_structures_sizing`
//! is squeezed and illegible in this tab's narrow column, so this is a
//! distinct, cheap (sizing-only, no FEM mesh, no NASTRAN) figure built
//! directly from [`WingStructureGeometry`]/[`WingboxSizing`].

use crate::scene::{Axes2D, Color, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;
use alas_geom::wing_structure::WingStructureGeometry;
use alas_struct::sizing::WingboxSizing;

use super::{
    chord_bounds, draw_wing_outline, root_connected_spar_series, spar_line_series, SPAR_COLORS,
};

/// Wingbox planform and spar traces without a secondary sizing summary.
pub fn figure_structures_designer_preview(
    wsg: &WingStructureGeometry,
    sizing: &WingboxSizing,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(520.0, 430.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Structural Rib & Spar Grid Preview".to_owned());
    scene.suppress_derived_title();

    let y = &sizing.y_stations;
    let le: Vec<f64> = sizing.eta_stations.iter().map(|&e| wsg.x_le(e)).collect();
    let te: Vec<f64> = le.iter().zip(&sizing.chord).map(|(&l, &c)| l + c).collect();

    let y_max = y.last().copied().unwrap_or(1.0).max(1e-6);
    let (x_lo, x_hi) = chord_bounds(&le, &te);
    let rect = (60.0, 44.0, 420.0, 320.0);
    let axes = Axes2D::new(rect, (0.0, y_max), (x_lo, x_hi));
    axes.draw_frame_with_labels(
        &mut scene,
        pal,
        "Spanwise position Y [m]",
        "Chordwise position X [m]",
    );

    scene.add(SceneElement::Text {
        text: "Wingbox planform".to_owned(),
        pos: [rect.0, rect.1 - 6.0],
        font_size: 11.0,
        color: Color::from_hex(pal.title),
        align: TextAlign::Left,
        baseline: TextBaseline::Bottom,
        angle_deg: 0.0,
        bold: true,
    });

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
        spar_line_series(&axes, &mut scene, &spar_y, &spar_x, Stroke::new(color, 2.0));
        scene.add(SceneElement::Text {
            text: format!("x/c={:.2}", spar.chord_fraction),
            pos: [rect.0 + rect.2 - 4.0, rect.1 + 4.0 + 12.0 * i as f64],
            font_size: 8.0,
            color,
            align: TextAlign::Right,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }

    scene
}

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::families::structures::true_spar_xy;
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
            t_web: 0.006,
            frac_moment: vec![0.5; n],
            margin_of_safety: vec![0.1; n],
        };
        WingboxSizing {
            y_stations: y,
            eta_stations: eta,
            chord,
            spar_fracs: vec![0.15, 0.65],
            spars: vec![spar(0.15), spar(0.65)],
            t_skin: 0.004,
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
                    (60.0..=480.0).contains(&point[0]) && (44.0..=364.0).contains(&point[1])
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
    fn the_positioned_planform_heading_replaces_the_derived_scene_title() {
        let scene = figure_structures_designer_preview(&wsg(), &sample_sizing(), Some("dark"));

        assert_eq!(
            scene.title.as_deref(),
            Some("Structural Rib & Spar Grid Preview")
        );
        assert!(!scene.render_title);
        assert!(crate::scene::visual_title(&scene).is_none());
        let planform_headings = scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, color, .. } if text == "Wingbox planform" => {
                    Some(*color)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(planform_headings.len(), 1);
        let background = scene.background.expect("dark scene background");
        assert!(planform_headings[0].contrast_against(background) >= 4.5);
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
        assert!(texts.contains(&"Wingbox planform"));
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

        for color_name in ["tab:blue", "tab:orange"] {
            let color = Color::from_hex(color_name);
            let root_trace = scene.elements.iter().find_map(|element| match element {
                SceneElement::Polyline { points, stroke }
                    if stroke.color == color && points.len() >= 2 =>
                {
                    Some(points)
                }
                _ => None,
            });
            let points = root_trace.expect("each spar has a visible root-connected trace");
            assert!(
                (points[0][0] - 60.0).abs() < 1e-9,
                "{color_name} first point is {:?}",
                points[0]
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
