// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (`figure_airfoil_evolution` L1124-1161)
// Reference: alas @ rust-port-baseline.

//! Every main-wing cross-section's airfoil outline, coloured by spanwise
//! position with a colorbar: `figure_airfoil_evolution`.

use alas_geom::aircraft::airplane::Airplane;

use crate::chart_kit::draw_title;
use crate::colormap::Colormap;
use crate::scene::{Axes2D, Color, Fill, Scene, SceneElement, Stroke, TextAlign, TextBaseline};
use crate::theme::get_palette;

/// Draw the spanwise colorbar below the airfoil plot so its label stays
/// horizontal and clear of the plot boundary.
fn draw_horizontal_colorbar(
    scene: &mut Scene,
    rect: (f64, f64, f64, f64),
    cmap: Colormap,
    vmin: f64,
    vmax: f64,
    label: &str,
    pal: &crate::theme::Palette,
) {
    let (x, y, width, height) = rect;
    let steps = 64;
    for index in 0..steps {
        let t = index as f64 / (steps - 1) as f64;
        scene.add(SceneElement::Rect {
            x: x + index as f64 * width / steps as f64,
            y,
            width: width / steps as f64 + 0.5,
            height,
            rx: 0.0,
            fill: Some(Fill::new(cmap.sample(t))),
            stroke: None,
        });
    }
    scene.add(SceneElement::Rect {
        x,
        y,
        width,
        height,
        rx: 0.0,
        fill: None,
        stroke: Some(Stroke::new(Color::from_hex(pal.spine), 1.0)),
    });
    for (fraction, value) in [(0.0, vmin), (0.5, (vmin + vmax) * 0.5), (1.0, vmax)] {
        scene.add(SceneElement::Text {
            text: format!("{value:.3}"),
            pos: [x + fraction * width, y + height + 10.0],
            font_size: 8.0,
            color: Color::from_hex(pal.tick),
            align: if fraction == 0.0 {
                TextAlign::Left
            } else if fraction == 1.0 {
                TextAlign::Right
            } else {
                TextAlign::Center
            },
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: false,
        });
    }
    scene.add(SceneElement::Text {
        text: label.to_owned(),
        pos: [x + width * 0.5, y + height + 25.0],
        font_size: 9.0,
        color: Color::from_hex(pal.tick),
        align: TextAlign::Center,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: false,
    });
}

/// Generate the per-station airfoil cross-section figure of the main wing.
pub fn figure_airfoil_evolution(plane: &Airplane, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(660.0, 460.0, Some(Color::from_hex(pal.bg)));

    let main_wing = plane
        .wings
        .iter()
        .find(|w| w.name == "Main Wing")
        .or_else(|| plane.wings.first());
    let Some(main_wing) = main_wing else {
        scene.title = Some("Wing cross-sections".to_owned());
        draw_title(&mut scene, "Wing cross-sections", pal);
        scene.suppress_derived_title();
        return scene;
    };

    let max_y = main_wing
        .xsecs
        .iter()
        .map(|xsec| xsec.xyz_le[1].abs())
        .fold(0.0_f64, f64::max);

    scene.title = Some(format!(
        "Wing cross-sections \u{2014} {} stations (root to tip)",
        main_wing.xsecs.len()
    ));
    let title = scene.title.clone().unwrap_or_default();
    draw_title(&mut scene, &title, pal);
    scene.suppress_derived_title();

    // Data-driven axis extent: every coordinate any station actually reaches,
    // rather than a fixed magic-number airfoil-shaped box.
    let (mut x_lo, mut x_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut y_lo, mut y_hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for xsec in &main_wing.xsecs {
        for &(x, y) in &xsec.airfoil.coordinates {
            x_lo = x_lo.min(x);
            x_hi = x_hi.max(x);
            y_lo = y_lo.min(y);
            y_hi = y_hi.max(y);
        }
    }
    if !x_lo.is_finite() {
        x_lo = 0.0;
        x_hi = 1.0;
        y_lo = -0.1;
        y_hi = 0.1;
    }
    let pad_x = (x_hi - x_lo).max(1e-3) * 0.05;
    let pad_y = (y_hi - y_lo).max(1e-3) * 0.3;

    let axes = Axes2D::new(
        (60.0, 40.0, 480.0, 300.0),
        (x_lo - pad_x, x_hi + pad_x),
        (y_lo - pad_y, y_hi + pad_y),
    )
    .with_equal_aspect();
    axes.draw_frame_with_labels(&mut scene, pal, "x/c [-]", "y/c [-]");

    for xsec in &main_wing.xsecs {
        let y_val = xsec.xyz_le[1].abs();
        let t = if max_y > 0.0 { y_val / max_y } else { 0.0 };
        let color = Colormap::Plasma.sample(t);
        axes.add_line_series(
            &mut scene,
            &xsec.airfoil.coordinates,
            Stroke::new(color, 1.4),
        );
    }

    draw_horizontal_colorbar(
        &mut scene,
        (210.0, 400.0, 240.0, 14.0),
        Colormap::Plasma,
        0.0,
        max_y,
        "Spanwise station y [m]",
        pal,
    );

    scene
}

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::wing::{Wing, WingXSec};

    fn probe_plane() -> Airplane {
        Airplane {
            name: "probe".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: vec![Wing::new(
                "Main Wing",
                vec![
                    WingXSec::new(
                        [0.0, 0.0, 0.0],
                        4.0,
                        0.0,
                        Airfoil::from_name("naca2412").expect("naca"),
                    ),
                    WingXSec::new(
                        [0.5, 5.0, 0.0],
                        2.0,
                        0.0,
                        Airfoil::from_name("naca0012").expect("naca"),
                    ),
                    WingXSec::new(
                        [1.0, 10.0, 0.0],
                        1.0,
                        0.0,
                        Airfoil::from_name("naca2412").expect("naca"),
                    ),
                ],
                true,
            )],
            fuselages: Vec::new(),
            s_ref: 40.0,
            c_ref: 2.0,
            b_ref: 20.0,
        }
    }

    #[test]
    fn draws_one_polyline_per_cross_section_plus_a_colorbar() {
        let scene = figure_airfoil_evolution(&probe_plane(), None);
        let polylines = scene
            .elements
            .iter()
            .filter(|e| matches!(e, crate::scene::SceneElement::Polyline { .. }))
            .count();
        // 3 sections + 64 colorbar gradient segments are rects, not polylines.
        assert_eq!(polylines, 3);
    }

    #[test]
    fn an_airplane_with_no_wings_still_returns_a_titled_scene() {
        let empty = Airplane {
            name: "empty".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: Vec::new(),
            fuselages: Vec::new(),
            s_ref: 1.0,
            c_ref: 1.0,
            b_ref: 1.0,
        };
        let scene = figure_airfoil_evolution(&empty, None);
        assert!(scene.title.is_some());
    }
}
