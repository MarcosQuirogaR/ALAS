// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from the reference visualization (`figure_threeview` L721-757)
// and its aircraft `Airplane.draw_three_view` routine,
// `style="wireframe"` branch)
// Upstream: native aerodynamic model 4.2.8, MIT.

//! The four-panel top/front/side/isometric wireframe three-view.
//!
//! Four orthographic/isometric projections share the actual airfoil section
//! grid and bulkhead geometry, with the live preview's component colors.
//! Each panel fits the resolved geometry to its available plotting area.
use alas_geom::aircraft::airplane::Airplane;

use crate::chart_kit::{draw_title, LegendMarker};
use crate::scene::Stroke;
use crate::scene::{
    Axes2D, Camera3D, Color, Point3D, Scene, SceneElement, TextAlign, TextBaseline,
};
use crate::theme::get_palette;

use super::shared::airplane_bbox;
use super::wireframe::{
    airplane_fit_points, draw_fuselage_wireframe, draw_wing_wireframe, framing,
};
use super::SceneComponent;

/// One panel: a preset camera and where its label goes.
struct Panel {
    label: &'static str,
    elev_deg: f64,
    azim_deg: f64,
    rect: (f64, f64, f64, f64),
}

/// The canvas the three-view is drawn on when the caller names none.
pub const THREEVIEW_CANVAS: (f64, f64) = (800.0, 700.0);

/// Tick-label font of the panels, scene units.
const TICK_FONT: f64 = 10.5;

/// The four panel rectangles `(left, top, width, height)` for a canvas, in the
/// order top, front, side, isometric. A canvas wider than twice its height
/// lays the panels in one row; otherwise they form a 2 x 2 grid. Every panel
/// keeps room under it for its tick labels and axis title, so a lower row's
/// panel label never meets the upper row's axis title.
pub fn threeview_panel_rects(canvas: (f64, f64)) -> [(f64, f64, f64, f64); 4] {
    let (width, height) = canvas;
    let (left, right, top, gap_x, below) = (66.0, 16.0, 40.0, 72.0, 46.0);
    let legend = 38.0;
    if width >= 2.0 * height {
        let panel_w = (width - left - right - 3.0 * gap_x) / 4.0;
        let panel_h = height - top - below - legend;
        let at = |col: f64| (left + col * (panel_w + gap_x), top, panel_w, panel_h);
        [at(0.0), at(1.0), at(2.0), at(3.0)]
    } else {
        let panel_w = (width - left - right - gap_x) / 2.0;
        let panel_h = (height - top - 2.0 * below - legend - 12.0) / 2.0;
        let x1 = left + panel_w + gap_x;
        let y1 = top + panel_h + below + 12.0;
        [
            (left, top, panel_w, panel_h),
            (x1, top, panel_w, panel_h),
            (left, y1, panel_w, panel_h),
            (x1, y1, panel_w, panel_h),
        ]
    }
}

/// Generate the four-panel wireframe three-view (top / front / side /
/// isometric) of an [`Airplane`]: `figure_threeview`. See the module
/// doc for how this differs from upstream's default shaded render.
pub fn figure_threeview(plane: &Airplane, theme: Option<&str>) -> Scene {
    figure_threeview_sized(plane, theme, THREEVIEW_CANVAS)
}

/// The three-view on a `canvas` of the caller's choosing (scene units; the GUI
/// passes about the size it draws it at, so the text keeps its point size).
pub fn figure_threeview_sized(plane: &Airplane, theme: Option<&str>, canvas: (f64, f64)) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(canvas.0, canvas.1, Some(Color::from_hex(pal.bg)));
    scene.title = Some("Three-View Drawing".to_owned());
    draw_title(&mut scene, "Three-View Drawing", pal);
    scene.suppress_derived_title();

    let (x0, x1, y0, y1, z0, z1) = airplane_bbox(plane);
    let (fallback_center, _span) = framing(x0, x1, y0, y1, z0, z1);
    let fit_points = airplane_fit_points(plane);

    let rects = threeview_panel_rects(canvas);
    // One scale for every view of the drawing (all panels share one size).
    let common = common_scale(rects[0], &fit_points);
    let panels = [
        Panel {
            label: "Top view (X-Y)",
            elev_deg: 90.0,
            azim_deg: 0.0,
            rect: rects[0],
        },
        Panel {
            label: "Front view (Y-Z)",
            elev_deg: 0.0,
            azim_deg: 90.0,
            rect: rects[1],
        },
        Panel {
            label: "Side view (X-Z)",
            elev_deg: 0.0,
            azim_deg: 0.0,
            rect: rects[2],
        },
        Panel {
            label: "Isometric",
            elev_deg: 22.0,
            azim_deg: -125.0,
            rect: rects[3],
        },
    ];

    for panel in &panels {
        let cam = Camera3D {
            elev_deg: panel.elev_deg,
            azim_deg: panel.azim_deg,
            zoom: 1.0,
        };
        let (center, panel_span, axes) = match panel.label {
            label if label.starts_with("Top") => {
                orthographic(&cam, panel.rect, &fit_points, 0, 1, common)
            }
            label if label.starts_with("Front") => {
                orthographic(&cam, panel.rect, &fit_points, 1, 2, common)
            }
            label if label.starts_with("Side") => {
                orthographic(&cam, panel.rect, &fit_points, 0, 2, common)
            }
            _ => {
                let center = cam.fit_center_to_points(&fit_points, fallback_center);
                let panel_span = cam.fit_span_to_points(&fit_points, panel.rect, 0.08);
                // Projected metres: the camera centre sits at the rect centre,
                // so the axes span half the rect either side at the camera scale.
                let pixels_per_unit =
                    (panel.rect.2.min(panel.rect.3) * 0.45 / panel_span.max(1e-6)).min(common);
                let panel_span = panel.rect.2.min(panel.rect.3) * 0.45 / pixels_per_unit;
                let axes = Axes2D::new(
                    panel.rect,
                    (
                        -0.5 * panel.rect.2 / pixels_per_unit,
                        0.5 * panel.rect.2 / pixels_per_unit,
                    ),
                    (
                        -0.5 * panel.rect.3 / pixels_per_unit,
                        0.5 * panel.rect.3 / pixels_per_unit,
                    ),
                );
                (center, panel_span, axes)
            }
        };
        let axes = axes.with_tick_font(TICK_FONT);
        let labels = match panel.label {
            label if label.starts_with("Top") => ("X [m]", "Y [m]"),
            label if label.starts_with("Front") => ("Y [m]", "Z [m]"),
            label if label.starts_with("Side") => ("X [m]", "Z [m]"),
            _ => ("View horizontal [m]", "View vertical [m]"),
        };
        axes.draw_frame_with_labels(&mut scene, pal, labels.0, labels.1);
        scene.add(SceneElement::Text {
            text: panel.label.to_owned(),
            pos: [panel.rect.0 + 6.0, panel.rect.1 + 5.0],
            font_size: TICK_FONT + 1.5,
            color: Color::from_hex(pal.title),
            align: TextAlign::Left,
            baseline: TextBaseline::Top,
            angle_deg: 0.0,
            bold: true,
        });

        for wing in &plane.wings {
            let color = SceneComponent::of_wing(wing).color();
            draw_wing_wireframe(
                &mut scene, &cam, center, panel_span, panel.rect, wing, color,
            );
        }
        for (index, fus) in plane.fuselages.iter().enumerate() {
            draw_fuselage_wireframe(
                &mut scene,
                &cam,
                center,
                panel_span,
                panel.rect,
                fus,
                SceneComponent::of_fuselage(index, fus).color(),
            );
        }
    }

    draw_horizontal_legend(
        &mut scene,
        [rects[2].0, canvas.1 - 16.0],
        &[
            (
                "Main wing".to_owned(),
                LegendMarker::Line(Stroke::new(SceneComponent::Wing.color(), 1.5)),
            ),
            (
                "Horizontal stabilizer".to_owned(),
                LegendMarker::Line(Stroke::new(SceneComponent::HorizontalTail.color(), 1.5)),
            ),
            (
                "Vertical stabilizer".to_owned(),
                LegendMarker::Line(Stroke::new(SceneComponent::VerticalTail.color(), 1.5)),
            ),
            (
                "Fuselage".to_owned(),
                LegendMarker::Line(Stroke::new(SceneComponent::Fuselage.color(), 1.5)),
            ),
        ],
        pal,
        TICK_FONT + 1.0,
    );

    scene
}

/// Orthographic panel: the data axes and the camera that draws into them.
///
/// The camera scale and centre are derived from the axes, so one data unit
/// has the same pixel length on the frame ticks and in the drawn wireframe;
/// a horizontal or vertical data direction the camera mirrors is mirrored on
/// the axes as well (reversed range), never silently flipped.
pub(super) struct OrthoView {
    pub axes: Axes2D,
    pub center: Point3D,
    pub span: f64,
}

/// Build the [`OrthoView`] of a panel that shows data coordinates
/// `horizontal` x `vertical` (indices into `[x, y, z]`) of `points`.
pub(super) fn ortho_view(
    cam: &Camera3D,
    rect: (f64, f64, f64, f64),
    points: &[Point3D],
    horizontal: usize,
    vertical: usize,
    padding: f64,
    pixels_per_unit_override: Option<f64>,
) -> OrthoView {
    let range = |axis: usize| {
        let lo = points.iter().map(|p| p[axis]).fold(f64::INFINITY, f64::min);
        let hi = points
            .iter()
            .map(|p| p[axis])
            .fold(f64::NEG_INFINITY, f64::max);
        let (lo, hi) = if lo.is_finite() && hi.is_finite() {
            (lo, hi)
        } else {
            (-1.0, 1.0)
        };
        let pad = ((hi - lo) * padding).max(1e-3);
        (lo - pad, hi + pad)
    };
    let (h_lo, h_hi) = range(horizontal);
    let (v_lo, v_hi) = range(vertical);
    let mut ascending = Axes2D::new(rect, (h_lo, h_hi), (v_lo, v_hi)).with_equal_aspect();
    if let Some(scale) = pixels_per_unit_override.filter(|s| s.is_finite() && *s > 0.0) {
        // A fixed scale: keep the data midpoint at the rect centre and span the
        // rect at exactly `scale` pixels per data unit.
        let mid_h = 0.5 * (ascending.x_min + ascending.x_max);
        let mid_v = 0.5 * (ascending.y_min + ascending.y_max);
        ascending.x_min = mid_h - 0.5 * rect.2 / scale;
        ascending.x_max = mid_h + 0.5 * rect.2 / scale;
        ascending.y_min = mid_v - 0.5 * rect.3 / scale;
        ascending.y_max = mid_v + 0.5 * rect.3 / scale;
    }
    let mid = [
        0.5 * (ascending.x_min + ascending.x_max),
        0.5 * (ascending.y_min + ascending.y_max),
    ];
    let mut center = [0.0; 3];
    for (axis, value) in center.iter_mut().enumerate() {
        *value = if axis == horizontal {
            mid[0]
        } else if axis == vertical {
            mid[1]
        } else {
            0.5 * (range(axis).0 + range(axis).1)
        };
    }
    let pixels_per_unit = ascending.width / (ascending.x_max - ascending.x_min).max(1e-9);
    let span = rect.2.min(rect.3) * 0.45 / pixels_per_unit;
    // Which way the camera draws each data direction: project a unit step.
    let origin = cam.project(center, center, span, rect);
    let mut step_h = center;
    step_h[horizontal] += 1.0;
    let mut step_v = center;
    step_v[vertical] += 1.0;
    let right = cam.project(step_h, center, span, rect)[0] - origin[0];
    let up = origin[1] - cam.project(step_v, center, span, rect)[1];
    let mut axes = ascending;
    if right < 0.0 {
        std::mem::swap(&mut axes.x_min, &mut axes.x_max);
    }
    if up < 0.0 {
        std::mem::swap(&mut axes.y_min, &mut axes.y_max);
    }
    OrthoView { axes, center, span }
}

/// The one pixels-per-metre scale of a three-view: the largest scale at which
/// each of the orthographic projections still fits its own panel.
fn common_scale(rect: (f64, f64, f64, f64), points: &[Point3D]) -> f64 {
    [(0, 1), (1, 2), (0, 2)]
        .into_iter()
        .map(|(h, v)| {
            let cam = Camera3D {
                elev_deg: 0.0,
                azim_deg: 0.0,
                zoom: 1.0,
            };
            let view = ortho_view(&cam, rect, points, h, v, 0.05, None);
            view.axes.width / (view.axes.x_max - view.axes.x_min).abs().max(1e-9)
        })
        .fold(f64::INFINITY, f64::min)
}

fn orthographic(
    cam: &Camera3D,
    rect: (f64, f64, f64, f64),
    points: &[Point3D],
    horizontal: usize,
    vertical: usize,
    scale: f64,
) -> (Point3D, f64, Axes2D) {
    let view = ortho_view(cam, rect, points, horizontal, vertical, 0.05, Some(scale));
    (view.center, view.span, view.axes)
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
        // Each panel title remains present; Axes2D also emits numeric
        // tick labels so the result is readable when exported standalone.
        assert!(texts >= 4);
        assert!(
            scene.elements.len() > 4 + 4,
            "each panel also drew wireframe content"
        );
    }

    /// The frame ticks and the drawn wireframe share one scale and one
    /// orientation: a data point projected by the panel camera lands exactly
    /// where the panel axes put its (horizontal, vertical) coordinates, and a
    /// data unit has the same pixel length on both axes.
    #[test]
    fn orthographic_axes_and_wireframe_share_scale_and_orientation() {
        let plane = probe_plane();
        let points = airplane_fit_points(&plane);
        let rect = threeview_panel_rects(THREEVIEW_CANVAS)[0];
        // (elevation, azimuth, horizontal data axis, vertical data axis)
        for (elev_deg, azim_deg, h, v) in [(90.0, 0.0, 0, 1), (0.0, 90.0, 1, 2), (0.0, 0.0, 0, 2)] {
            let cam = Camera3D {
                elev_deg,
                azim_deg,
                zoom: 1.0,
            };
            let view = ortho_view(&cam, rect, &points, h, v, 0.05, None);
            let mut probes = [view.center; 3];
            probes[1][h] += 3.0;
            probes[2][v] += 2.0;
            for probe in probes {
                let drawn = cam.project(probe, view.center, view.span, rect);
                let framed = view.axes.map_point(probe[h], probe[v]);
                assert!(
                    (drawn[0] - framed[0]).abs() < 1e-6 && (drawn[1] - framed[1]).abs() < 1e-6,
                    "view ({elev_deg}, {azim_deg}): drawn {drawn:?} != framed {framed:?}"
                );
            }
            let unit_h = view.axes.map_point(1.0, 0.0)[0] - view.axes.map_point(0.0, 0.0)[0];
            let unit_v = view.axes.map_point(0.0, 1.0)[1] - view.axes.map_point(0.0, 0.0)[1];
            assert!((unit_h.abs() - unit_v.abs()).abs() < 1e-6);
        }
    }

    /// The panel labels of the lower row sit below the upper row's axis titles,
    /// and the one-row layout keeps four panels side by side.
    #[test]
    fn the_panel_layouts_leave_room_for_axis_titles() {
        let grid = threeview_panel_rects((800.0, 700.0));
        assert!(grid[2].1 >= grid[0].1 + grid[0].3 + 44.0);
        let wide = threeview_panel_rects((1700.0, 460.0));
        assert!(wide
            .windows(2)
            .all(|pair| pair[1].0 >= pair[0].0 + pair[0].2 + 60.0));
        assert!(wide
            .iter()
            .all(|panel| panel.0 + panel.2 <= 1700.0 && panel.3 > 200.0));
        let scene = figure_threeview_sized(&probe_plane(), None, (1700.0, 460.0));
        assert_eq!((scene.width, scene.height), (1700.0, 460.0));
    }

    /// One drawing, one scale: the top, front and side panels show a metre as
    /// the same number of pixels, and the wing and tail colours differ.
    #[test]
    fn the_three_orthographic_views_share_one_scale() {
        let plane = probe_plane();
        let points = airplane_fit_points(&plane);
        let rect = threeview_panel_rects(THREEVIEW_CANVAS)[0];
        let common = common_scale(rect, &points);
        let mut scales = Vec::new();
        for (elev_deg, azim_deg, h, v) in [(90.0, 0.0, 0, 1), (0.0, 90.0, 1, 2), (0.0, 0.0, 0, 2)] {
            let cam = Camera3D {
                elev_deg,
                azim_deg,
                zoom: 1.0,
            };
            let own = ortho_view(&cam, rect, &points, h, v, 0.05, None);
            let fixed = ortho_view(&cam, rect, &points, h, v, 0.05, Some(common));
            let own_scale = own.axes.width / (own.axes.x_max - own.axes.x_min).abs();
            assert!(common <= own_scale + 1e-9, "{common} > {own_scale}");
            scales.push(fixed.axes.width / (fixed.axes.x_max - fixed.axes.x_min).abs());
        }
        assert!(
            scales.iter().all(|s| (s - common).abs() < 1e-9),
            "{scales:?}"
        );
        let colors = [
            SceneComponent::Wing.color(),
            SceneComponent::HorizontalTail.color(),
            SceneComponent::VerticalTail.color(),
            SceneComponent::Fuselage.color(),
        ];
        for (i, a) in colors.iter().enumerate() {
            for b in &colors[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn each_projected_view_fits_its_own_panel_without_overflow() {
        let scene = figure_threeview(&probe_plane(), None);
        let panels = threeview_panel_rects(THREEVIEW_CANVAS);
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
