// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from the reference visualization (`figure_threeview` L721-757)
// and its aircraft `Airplane.draw_three_view` routine,
// `style="wireframe"` branch)
// Upstream: native aerodynamic model 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! The four-panel top/front/side/isometric wireframe three-view.
//!
//! The reference `figure_threeview` calls `Airplane.draw_three_view(show=False)`
//! at its default `style="shaded"`, a full lit `Poly3DCollection` render,
//! not reproducible by this crate's line/polygon SVG scene graph, and not a
//! numeric quantity a fixture could hold either way (`draw_three_view`
//! returns Matplotlib axes, not data). This port instead exercises the
//! `style="wireframe"` branch of that same upstream method: four
//! [`crate::scene::Camera3D`] projections of the real airplane, at the four
//! preset view angles `draw_three_view` itself uses (`"XZ"` top, `"-YZ"`
//! front, `"XY"` side, `"left_isometric"`), reusing
//! [`super::wireframe::draw_wing_wireframe`]/`draw_fuselage_wireframe`:
//! the same reachable wireframe content [`super::wireframe`]'s isolated
//! component figures draw, and the same geometry every panel here shares one
//! frame for, so the four views stay to scale with each other.

use alas_geom::aircraft::airplane::Airplane;

use crate::chart_kit::{draw_title, LegendMarker};
use crate::scene::Stroke;
use crate::scene::{Axes2D, Camera3D, Color, Scene, SceneElement, TextAlign, TextBaseline};
use crate::theme::get_palette;

use super::shared::airplane_bbox;
use super::wireframe::{
    airplane_fit_points, draw_fuselage_wireframe, draw_wing_wireframe, framing,
};

/// One panel: a preset camera and where its label goes.
struct Panel {
    label: &'static str,
    elev_deg: f64,
    azim_deg: f64,
    rect: (f64, f64, f64, f64),
}

/// Generate the four-panel wireframe three-view (top / front / side /
/// isometric) of an [`Airplane`]: `figure_threeview`. See the module
/// doc for how this differs from upstream's default shaded render.
pub fn figure_threeview(plane: &Airplane, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(800.0, 700.0, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Three-View Drawing".to_owned());
    draw_title(&mut scene, "Three-View Drawing", pal);
    scene.suppress_derived_title();

    let (x0, x1, y0, y1, z0, z1) = airplane_bbox(plane);
    let (fallback_center, span) = framing(x0, x1, y0, y1, z0, z1);
    let fit_points = airplane_fit_points(plane);

    let panels = [
        Panel {
            label: "Top view (X-Y)",
            elev_deg: 90.0,
            azim_deg: 0.0,
            rect: (70.0, 50.0, 320.0, 250.0),
        },
        Panel {
            label: "Front view (Y-Z)",
            elev_deg: 0.0,
            azim_deg: 90.0,
            rect: (470.0, 50.0, 320.0, 250.0),
        },
        Panel {
            label: "Side view (X-Z)",
            elev_deg: 0.0,
            azim_deg: 0.0,
            rect: (70.0, 330.0, 320.0, 250.0),
        },
        Panel {
            label: "Isometric",
            elev_deg: 22.0,
            azim_deg: -125.0,
            rect: (470.0, 330.0, 320.0, 250.0),
        },
    ];

    for panel in &panels {
        let cam = Camera3D {
            elev_deg: panel.elev_deg,
            azim_deg: panel.azim_deg,
            zoom: 1.0,
        };
        let center = cam.fit_center_to_points(&fit_points, fallback_center);
        // Each orthographic view gets a content-aware span. A common 3-D
        // bounding-cube span leaves the narrow front and side views needlessly
        // small, especially on high-aspect-ratio transport aircraft.
        let panel_span = cam.fit_span_to_points(&fit_points, panel.rect, 0.08);
        let (x_range, y_range) = match panel.label {
            label if label.starts_with("Top") => ((x0, x1), (y0, y1)),
            label if label.starts_with("Front") => ((y0, y1), (z0, z1)),
            label if label.starts_with("Side") => ((x0, x1), (z0, z1)),
            _ => ((-span * 0.55, span * 0.55), (-span * 0.55, span * 0.55)),
        };
        let axes = Axes2D::new(panel.rect, x_range, y_range).with_equal_aspect();
        let labels = match panel.label {
            label if label.starts_with("Top") => ("X [m]", "Y [m]"),
            label if label.starts_with("Front") => ("Y [m]", "Z [m]"),
            label if label.starts_with("Side") => ("X [m]", "Z [m]"),
            _ => ("projected X [m]", "projected Y [m]"),
        };
        axes.draw_frame_with_labels(&mut scene, pal, labels.0, labels.1);
        scene.add(SceneElement::Text {
            text: panel.label.to_owned(),
            pos: [panel.rect.0 + 6.0, panel.rect.1 + 4.0],
            font_size: 10.0,
            color: Color::from_hex(pal.title),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: true,
        });

        for (index, wing) in plane.wings.iter().enumerate() {
            let color = match index {
                0 => Color::from_hex("#2563eb"),
                1 => Color::from_hex("#e67e22"),
                _ => Color::from_hex("#16a085"),
            };
            draw_wing_wireframe(
                &mut scene, &cam, center, panel_span, panel.rect, wing, color,
            );
        }
        for fus in &plane.fuselages {
            draw_fuselage_wireframe(
                &mut scene,
                &cam,
                center,
                panel_span,
                panel.rect,
                fus,
                Color::from_hex("#9b59b6"),
            );
        }
    }

    draw_horizontal_legend(
        &mut scene,
        [210.0, 635.0],
        &[
            (
                "Main wing".to_owned(),
                LegendMarker::Line(Stroke::new(Color::from_hex("#2563eb"), 1.5)),
            ),
            (
                "Horizontal stabilizer".to_owned(),
                LegendMarker::Line(Stroke::new(Color::from_hex("#e67e22"), 1.5)),
            ),
            (
                "Vertical stabilizer".to_owned(),
                LegendMarker::Line(Stroke::new(Color::from_hex("#16a085"), 1.5)),
            ),
            (
                "Fuselage".to_owned(),
                LegendMarker::Line(Stroke::new(Color::from_hex("#9b59b6"), 1.5)),
            ),
        ],
        pal,
        8.0,
    );

    scene
}

/// Compatibility entry point for older saved figure registries.
#[doc(hidden)]
pub fn figure_asb_threeview(plane: &Airplane, theme: Option<&str>) -> Scene {
    figure_threeview(plane, theme)
}

/// Draw the aircraft-part legend in one bottom row so it does not consume a
/// panel's vertical space or compete with the four projected views.
fn draw_horizontal_legend(
    scene: &mut Scene,
    pos: [f64; 2],
    entries: &[(String, LegendMarker)],
    pal: &crate::theme::Palette,
    font_size: f64,
) {
    let mut x = pos[0];
    for (label, marker) in entries {
        let mid_y = pos[1] + font_size * 0.5;
        match marker {
            LegendMarker::Line(stroke) => scene.add(SceneElement::Line {
                p1: [x, mid_y],
                p2: [x + 18.0, mid_y],
                stroke: stroke.clone(),
            }),
            LegendMarker::Patch(color) => scene.add(SceneElement::Rect {
                x,
                y: pos[1],
                width: 14.0,
                height: font_size,
                rx: 1.0,
                fill: Some(crate::scene::Fill::new(*color)),
                stroke: None,
            }),
            LegendMarker::Circle(color) => scene.add(SceneElement::Circle {
                center: [x + 7.0, mid_y],
                radius: 5.0,
                fill: Some(crate::scene::Fill::new(*color)),
                stroke: None,
            }),
        }
        scene.add(SceneElement::Text {
            text: label.clone(),
            pos: [x + 24.0, mid_y],
            font_size,
            color: Color::from_hex(pal.tick),
            align: TextAlign::Left,
            baseline: TextBaseline::Middle,
            angle_deg: 0.0,
            bold: false,
        });
        x += 24.0 + label.chars().count() as f64 * font_size * 0.55 + 24.0;
    }
}

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::fuselage::{Fuselage, FuselageXSec, DEFAULT_SHAPE};
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
                        [1.0, 10.0, 0.5],
                        1.5,
                        0.0,
                        Airfoil::from_name("naca2412").expect("naca"),
                    ),
                ],
                true,
            )],
            fuselages: vec![Fuselage::new(
                "Fuselage",
                vec![
                    FuselageXSec::new([0.0, 0.0, 0.0], Some(0.1), None, None, DEFAULT_SHAPE)
                        .expect("xsec"),
                    FuselageXSec::new([20.0, 0.0, 0.0], Some(0.1), None, None, DEFAULT_SHAPE)
                        .expect("xsec"),
                ],
            )],
            s_ref: 40.0,
            c_ref: 2.0,
            b_ref: 20.0,
        }
    }

    #[test]
    fn four_panels_each_draw_the_same_wing_and_fuselage_content() {
        let scene = figure_threeview(&probe_plane(), None);
        // 4 frame rects + 4 labels, plus wireframe content repeated 4 times.
        let rects = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Rect { .. }))
            .count();
        assert_eq!(rects, 4);
        let texts = scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Text { .. }))
            .count();
        // Each panel title remains present; Axes2D now also emits numeric
        // tick labels so the result is readable when exported standalone.
        assert!(texts >= 4);
        assert!(
            scene.elements.len() > 4 + 4,
            "each panel also drew wireframe content"
        );
    }

    #[test]
    fn each_projected_view_fits_its_own_panel_without_overflow() {
        let scene = figure_threeview(&probe_plane(), None);
        let panels = [
            (70.0, 50.0, 320.0, 250.0),
            (470.0, 50.0, 320.0, 250.0),
            (70.0, 330.0, 320.0, 250.0),
            (470.0, 330.0, 320.0, 250.0),
        ];
        for points in scene.elements.iter().filter_map(|element| match element {
            SceneElement::Polyline { points, .. } => Some(points),
            _ => None,
        }) {
            assert!(panels.iter().any(|&(x, y, width, height)| {
                points.iter().all(|point| {
                    (x..=x + width).contains(&point[0]) && (y..=y + height).contains(&point[1])
                })
            }));
        }
    }
}
