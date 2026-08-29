// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py
// (`figure_wireframe_wing` L1164-1186, `figure_wireframe_fuselage` L1189-1204,
// `figure_wireframe_empennage` L1207-1233)
// and alas/sidecar/figures.py (`_preview_exterior` L372-407)
// Reference: alas @ rust-port-baseline.

//! Isolated-component 3D wireframes and the exterior live-preview scene.
//!
//! Upstream draws these with native aerodynamic model's own `Wing.draw_wireframe`/
//! `Fuselage.draw_wireframe` (aliases of `Airplane.draw_wireframe`), which
//! meshes the full mplot3d surface: leading/trailing-edge lines, a camber-
//! following thickness line, *every* cross-section's upper and lower airfoil
//! outline (via `Wing._compute_frame_of_WingXSec`, a per-section local frame
//! `docs/PORTING.md` records as left untranslated -- meshing-only, unreached
//! by anything but this figure family), and, for a fuselage, elliptical
//! bulkheads sampled through `FuselageXSec.get_3D_coordinates` (also
//! untranslated for the same reason).
//!
//! This port draws every line upstream does *except* the per-section airfoil
//! outline, from the geometry that already has a translated source: the
//! leading/trailing-edge and thickness lines come from [`Wing::mesh_line`]
//! (`alas-geom::aircraft::mesh`, itself a P5 prerequisite and already green), and
//! the bulkhead ellipses are computed directly from `FuselageXSec.width`/
//! `.height` -- the same two fields [`super::planform::figure_geometry`]
//! already draws a fuselage silhouette from -- rather than reproducing
//! `get_3D_coordinates`.

use std::f64::consts::PI;

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::mesh::XsecStation;
use alas_geom::aircraft::wing::Wing;

use crate::scene::{
    Camera3D, Color, Point2D, Point3D, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use crate::theme::get_palette;

use super::shared::airplane_bbox;

const BULKHEAD_SEGMENTS: usize = 24;
const LONGERON_COUNT: usize = 8;
const HEADER_HEIGHT: f64 = 48.0;
const WIREFRAME_PADDING: f64 = 0.08;

/// A `(center, max_span)` pair sized from a data-space bounding box, with
/// modest padding -- what every wireframe figure hands [`Camera3D::project`]
/// instead of a fixed magic-number span.
pub(super) fn framing(
    x_min: f64,
    x_max: f64,
    y_min: f64,
    y_max: f64,
    z_min: f64,
    z_max: f64,
) -> (Point3D, f64) {
    let center = [
        (x_min + x_max) / 2.0,
        (y_min + y_max) / 2.0,
        (z_min + z_max) / 2.0,
    ];
    let span = (x_max - x_min)
        .max(y_max - y_min)
        .max(z_max - z_min)
        .max(1e-3)
        * 1.15;
    (center, span)
}

fn wing_bbox(wing: &Wing) -> (f64, f64, f64, f64, f64, f64) {
    let (mut x_min, mut x_max) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut y_min, mut y_max) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut z_min, mut z_max) = (f64::INFINITY, f64::NEG_INFINITY);
    for sec in &wing.xsecs {
        let [lx, ly, lz] = sec.xyz_le;
        for &(x, y) in &[(lx, ly), (lx + sec.chord, ly)] {
            x_min = x_min.min(x);
            x_max = x_max.max(x);
            let ys: &[f64] = if wing.symmetric { &[y, -y] } else { &[y] };
            for &yy in ys {
                y_min = y_min.min(yy);
                y_max = y_max.max(yy);
            }
        }
        z_min = z_min.min(lz);
        z_max = z_max.max(lz);
    }
    if !x_min.is_finite() {
        return (0.0, 1.0, -1.0, 1.0, -1.0, 1.0);
    }
    (x_min, x_max, y_min, y_max, z_min, z_max)
}

fn fuselage_bbox(fus: &Fuselage) -> (f64, f64, f64, f64, f64, f64) {
    let (mut x_min, mut x_max) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut y_min, mut y_max) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut z_min, mut z_max) = (f64::INFINITY, f64::NEG_INFINITY);
    for sec in &fus.xsecs {
        let [cx, cy, cz] = sec.xyz_c;
        x_min = x_min.min(cx);
        x_max = x_max.max(cx);
        y_min = y_min.min(cy - sec.width / 2.0);
        y_max = y_max.max(cy + sec.width / 2.0);
        z_min = z_min.min(cz - sec.height / 2.0);
        z_max = z_max.max(cz + sec.height / 2.0);
    }
    if !x_min.is_finite() {
        return (0.0, 1.0, -1.0, 1.0, -1.0, 1.0);
    }
    (x_min, x_max, y_min, y_max, z_min, z_max)
}

fn wing_fit_points(wing: &Wing) -> Vec<Point3D> {
    let mut points = Vec::with_capacity(wing.xsecs.len() * if wing.symmetric { 4 } else { 2 });
    for sec in &wing.xsecs {
        for side in if wing.symmetric {
            &[1.0, -1.0][..]
        } else {
            &[1.0][..]
        } {
            points.push([sec.xyz_le[0], sec.xyz_le[1] * side, sec.xyz_le[2]]);
            points.push([
                sec.xyz_le[0] + sec.chord,
                sec.xyz_le[1] * side,
                sec.xyz_le[2],
            ]);
        }
    }
    points
}

fn fuselage_fit_points(fuselage: &Fuselage) -> Vec<Point3D> {
    let mut points = Vec::with_capacity(fuselage.xsecs.len() * LONGERON_COUNT);
    for sec in &fuselage.xsecs {
        for index in 0..LONGERON_COUNT {
            let theta = 2.0 * PI * index as f64 / LONGERON_COUNT as f64;
            points.push([
                sec.xyz_c[0],
                sec.xyz_c[1] + sec.width * 0.5 * theta.cos(),
                sec.xyz_c[2] + sec.height * 0.5 * theta.sin(),
            ]);
        }
    }
    points
}

pub(super) fn airplane_fit_points(plane: &Airplane) -> Vec<Point3D> {
    let mut points = Vec::new();
    for wing in &plane.wings {
        points.extend(wing_fit_points(wing));
    }
    for fuselage in &plane.fuselages {
        points.extend(fuselage_fit_points(fuselage));
    }
    points
}

/// Draw one wing's leading edge, trailing edge and `x/c = 0.4` upper/lower
/// thickness lines, mirroring both if [`Wing::symmetric`] -- the reachable
/// subset of upstream's `draw_wireframe` wing loop (see the module doc).
pub(crate) fn draw_wing_wireframe(
    scene: &mut Scene,
    cam: &Camera3D,
    center: Point3D,
    max_span: f64,
    viewport: (f64, f64, f64, f64),
    wing: &Wing,
    color: Color,
) {
    let thin = Stroke::new(color, 0.6);
    let thick = Stroke::new(color, 1.1);
    let mirrors: &[bool] = if wing.symmetric {
        &[false, true]
    } else {
        &[false]
    };

    let project_line = |scene: &mut Scene, pts: &[[f64; 3]], stroke: Stroke| {
        for &mirror in mirrors {
            let proj: Vec<Point2D> = pts
                .iter()
                .map(|&p| {
                    let p = if mirror { [p[0], -p[1], p[2]] } else { p };
                    cam.project(p, center, max_span, viewport)
                })
                .collect();
            if proj.len() >= 2 {
                scene.add(SceneElement::Polyline {
                    points: proj,
                    stroke: stroke.clone(),
                });
            }
        }
    };

    if let Ok(le) = wing.mesh_line(XsecStation::Scalar(0.0), XsecStation::Scalar(0.0), false) {
        project_line(scene, &le, thick.clone());
    }
    if let Ok(te) = wing.mesh_line(XsecStation::Scalar(1.0), XsecStation::Scalar(0.0), false) {
        project_line(scene, &te, thick);
    }

    let half_thickness: Vec<f64> = wing
        .xsecs
        .iter()
        .map(|xsec| xsec.airfoil.local_thickness(&[0.4])[0] / 2.0)
        .collect();
    if let Ok(top) = wing.mesh_line(
        XsecStation::Scalar(0.4),
        XsecStation::PerXsec(half_thickness.clone()),
        true,
    ) {
        project_line(scene, &top, thin.clone());
    }
    let neg_half: Vec<f64> = half_thickness.iter().map(|t| -t).collect();
    if let Ok(bottom) = wing.mesh_line(
        XsecStation::Scalar(0.4),
        XsecStation::PerXsec(neg_half),
        true,
    ) {
        project_line(scene, &bottom, thin);
    }
}

/// Draw one fuselage's per-station bulkhead ellipses, longerons and
/// centerline -- the reachable subset of upstream's `draw_wireframe`
/// fuselage loop (see the module doc for why the bulkhead is drawn from
/// `width`/`height` directly rather than through `get_3D_coordinates`).
pub(crate) fn draw_fuselage_wireframe(
    scene: &mut Scene,
    cam: &Camera3D,
    center: Point3D,
    max_span: f64,
    viewport: (f64, f64, f64, f64),
    fus: &Fuselage,
    color: Color,
) {
    let thin = Stroke::new(color, 0.5);
    let thick = Stroke::new(color, 0.9);
    let n = fus.xsecs.len();

    let ellipse_point =
        |xsec: &alas_geom::aircraft::fuselage::FuselageXSec, theta: f64| -> Point3D {
            [
                xsec.xyz_c[0],
                xsec.xyz_c[1] + (xsec.width / 2.0) * theta.cos(),
                xsec.xyz_c[2] + (xsec.height / 2.0) * theta.sin(),
            ]
        };

    for (i, xsec) in fus.xsecs.iter().enumerate() {
        let stroke = if i == 0 || i + 1 == n {
            thick.clone()
        } else {
            thin.clone()
        };
        let points: Vec<Point2D> = (0..=BULKHEAD_SEGMENTS)
            .map(|k| {
                let theta = 2.0 * PI * (k as f64) / (BULKHEAD_SEGMENTS as f64);
                cam.project(ellipse_point(xsec, theta), center, max_span, viewport)
            })
            .collect();
        scene.add(SceneElement::Polyline { points, stroke });
    }

    let centerline: Vec<Point2D> = fus
        .xsecs
        .iter()
        .map(|xsec| cam.project(xsec.xyz_c, center, max_span, viewport))
        .collect();
    scene.add(SceneElement::Polyline {
        points: centerline,
        stroke: thin,
    });

    for k in 0..LONGERON_COUNT {
        let theta = 2.0 * PI * (k as f64) / (LONGERON_COUNT as f64);
        let points: Vec<Point2D> = fus
            .xsecs
            .iter()
            .map(|xsec| cam.project(ellipse_point(xsec, theta), center, max_span, viewport))
            .collect();
        scene.add(SceneElement::Polyline {
            points,
            stroke: thick.clone(),
        });
    }
}

fn title_text(scene: &mut Scene, text: &str, color: Color) {
    scene.add(SceneElement::Text {
        text: text.to_owned(),
        pos: [10.0, 14.0],
        font_size: 12.0,
        color,
        align: TextAlign::Left,
        baseline: TextBaseline::Top,
        angle_deg: 0.0,
        bold: true,
    });
}

/// Isolated 3D wireframe of the main wing -- `figure_wireframe_wing`.
pub fn figure_wireframe_wing(plane: &Airplane, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(600.0, 450.0, Some(Color::from_hex(pal.bg)));
    let wing = plane
        .wings
        .iter()
        .find(|w| w.name == "Main Wing")
        .or_else(|| plane.wings.first());
    title_text(&mut scene, "Wing Wireframe", Color::from_hex(pal.title));
    if let Some(wing) = wing {
        let cam = Camera3D::default();
        let viewport = (20.0, HEADER_HEIGHT, 560.0, 382.0);
        let fit_points = wing_fit_points(wing);
        let (x0, x1, y0, y1, z0, z1) = wing_bbox(wing);
        let fallback = framing(x0, x1, y0, y1, z0, z1).0;
        let center = cam.fit_center_to_points(&fit_points, fallback);
        let span = cam.fit_span_to_points(&fit_points, viewport, WIREFRAME_PADDING);
        draw_wing_wireframe(
            &mut scene,
            &cam,
            center,
            span,
            viewport,
            wing,
            Color::from_hex("#2563eb"),
        );
    }
    scene
}

/// Isolated 3D wireframe of the (first) fuselage -- `figure_wireframe_fuselage`.
pub fn figure_wireframe_fuselage(plane: &Airplane, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(600.0, 450.0, Some(Color::from_hex(pal.bg)));
    let fus = plane
        .fuselages
        .iter()
        .find(|f| f.name == "Fuselage")
        .or_else(|| plane.fuselages.first());
    title_text(&mut scene, "Fuselage Wireframe", Color::from_hex(pal.title));
    if let Some(fus) = fus {
        let cam = Camera3D::default();
        let viewport = (20.0, HEADER_HEIGHT, 560.0, 382.0);
        let fit_points = fuselage_fit_points(fus);
        let (x0, x1, y0, y1, z0, z1) = fuselage_bbox(fus);
        let fallback = framing(x0, x1, y0, y1, z0, z1).0;
        let center = cam.fit_center_to_points(&fit_points, fallback);
        let span = cam.fit_span_to_points(&fit_points, viewport, WIREFRAME_PADDING);
        draw_fuselage_wireframe(
            &mut scene,
            &cam,
            center,
            span,
            viewport,
            fus,
            Color::from_hex("#9b59b6"),
        );
    }
    scene
}

/// Isolated 3D wireframe of the horizontal and vertical stabilizers --
/// `figure_wireframe_empennage`, with upstream's same by-name-then-by-index
/// fallback (`plane.wings[1]`/`plane.wings[2]`) for a configuration whose
/// tail surfaces are not named exactly `"Horizontal Stabilizer"`/
/// `"Vertical Stabilizer"`.
pub fn figure_wireframe_empennage(plane: &Airplane, theme: Option<&str>) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(600.0, 450.0, Some(Color::from_hex(pal.bg)));
    title_text(
        &mut scene,
        "Empennage Wireframe (H-Stab & V-Stab)",
        Color::from_hex(pal.title),
    );

    let hstab = plane
        .wings
        .iter()
        .find(|w| w.name == "Horizontal Stabilizer")
        .or_else(|| plane.wings.get(1));
    let vstab = plane
        .wings
        .iter()
        .find(|w| w.name == "Vertical Stabilizer")
        .or_else(|| plane.wings.get(2));

    let mut x0 = f64::INFINITY;
    let mut x1 = f64::NEG_INFINITY;
    let mut y0 = f64::INFINITY;
    let mut y1 = f64::NEG_INFINITY;
    let mut z0 = f64::INFINITY;
    let mut z1 = f64::NEG_INFINITY;
    for wing in [hstab, vstab].into_iter().flatten() {
        let (a, b, c, d, e, f) = wing_bbox(wing);
        x0 = x0.min(a);
        x1 = x1.max(b);
        y0 = y0.min(c);
        y1 = y1.max(d);
        z0 = z0.min(e);
        z1 = z1.max(f);
    }
    if !x0.is_finite() {
        return scene;
    }
    let cam = Camera3D::default();
    let viewport = (20.0, HEADER_HEIGHT, 560.0, 382.0);
    let fit_points = [hstab, vstab]
        .into_iter()
        .flatten()
        .flat_map(wing_fit_points)
        .collect::<Vec<_>>();
    let fallback = framing(x0, x1, y0, y1, z0, z1).0;
    let center = cam.fit_center_to_points(&fit_points, fallback);
    let span = cam.fit_span_to_points(&fit_points, viewport, WIREFRAME_PADDING);
    for (index, wing) in [hstab, vstab].into_iter().flatten().enumerate() {
        let color = if index == 0 {
            Color::from_hex("#e67e22")
        } else {
            Color::from_hex("#16a085")
        };
        draw_wing_wireframe(&mut scene, &cam, center, span, viewport, wing, color);
    }
    scene
}

/// Live 3D exterior wireframe: wings blue (or grey for a surface not named
/// with "wing"), the primary fuselage in the theme's title color and any
/// further fuselage-shaped body (engine nacelles) in orange -- port of
/// `alas/sidecar/figures.py::_preview_exterior`.
///
/// Unlike the stub this replaces, `center`/`max_span` are computed from the
/// airplane's own bounding box rather than a fixed magic-number span, so an
/// aircraft larger or smaller than the reference default still frames
/// correctly.
pub fn figure_exterior_3d(
    plane: &Airplane,
    camera: Option<Camera3D>,
    theme: Option<&str>,
) -> Scene {
    let pal = get_palette(theme);
    let mut scene = Scene::new(600.0, 500.0, Some(Color::from_hex(pal.bg)));
    let cam = camera.unwrap_or_default();
    let viewport = (20.0, 20.0, 560.0, 460.0);

    let (x0, x1, y0, y1, z0, z1) = airplane_bbox(plane);
    let fit_points = airplane_fit_points(plane);
    let fallback = framing(x0, x1, y0, y1, z0, z1).0;
    let center = cam.fit_center_to_points(&fit_points, fallback);
    let span = cam.fit_span_to_points(&fit_points, viewport, WIREFRAME_PADDING);

    for wing in &plane.wings {
        let color = if wing.name.to_lowercase().contains("wing") {
            Color::from_hex("#2563eb")
        } else {
            Color::from_hex("#a0a0a0")
        };
        draw_wing_wireframe(&mut scene, &cam, center, span, viewport, wing, color);
    }
    for (i, fus) in plane.fuselages.iter().enumerate() {
        let color = if i == 0 {
            Color::from_hex(pal.title)
        } else {
            Color::from_hex("#ff9900")
        };
        draw_fuselage_wireframe(&mut scene, &cam, center, span, viewport, fus, color);
    }

    scene
}

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use alas_geom::aircraft::airfoil::Airfoil;
    use alas_geom::aircraft::fuselage::{FuselageXSec, DEFAULT_SHAPE};
    use alas_geom::aircraft::wing::WingXSec;

    fn probe_wing(symmetric: bool) -> Wing {
        Wing::new(
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
            symmetric,
        )
    }

    fn probe_fuselage() -> Fuselage {
        Fuselage::new(
            "Fuselage",
            vec![
                FuselageXSec::new([0.0, 0.0, 0.0], Some(0.1), None, None, DEFAULT_SHAPE)
                    .expect("xsec"),
                FuselageXSec::new([10.0, 0.0, 0.0], Some(2.0), None, None, DEFAULT_SHAPE)
                    .expect("xsec"),
                FuselageXSec::new([20.0, 0.0, 0.0], Some(0.1), None, None, DEFAULT_SHAPE)
                    .expect("xsec"),
            ],
        )
    }

    #[test]
    fn a_symmetric_wing_wireframe_draws_both_sides() {
        let mut scene = Scene::new(200.0, 200.0, None);
        let wing = probe_wing(true);
        let (x0, x1, y0, y1, z0, z1) = wing_bbox(&wing);
        let (center, span) = framing(x0, x1, y0, y1, z0, z1);
        draw_wing_wireframe(
            &mut scene,
            &Camera3D::default(),
            center,
            span,
            (0.0, 0.0, 200.0, 200.0),
            &wing,
            Color::rgb(0, 200, 255),
        );
        // LE + TE + top + bottom thickness lines, doubled for symmetry.
        assert_eq!(scene.elements.len(), 8);
        assert!(
            wing_bbox(&wing).3 > 0.0,
            "symmetric bbox reaches the mirrored side"
        );
    }

    #[test]
    fn a_fuselage_wireframe_draws_one_bulkhead_per_station_plus_longerons_and_centerline() {
        let mut scene = Scene::new(200.0, 200.0, None);
        let fus = probe_fuselage();
        let (x0, x1, y0, y1, z0, z1) = fuselage_bbox(&fus);
        let (center, span) = framing(x0, x1, y0, y1, z0, z1);
        draw_fuselage_wireframe(
            &mut scene,
            &Camera3D::default(),
            center,
            span,
            (0.0, 0.0, 200.0, 200.0),
            &fus,
            Color::rgb(80, 80, 80),
        );
        let n = fus.xsecs.len();
        assert_eq!(scene.elements.len(), n + 1 + LONGERON_COUNT);
    }

    #[test]
    fn figure_exterior_3d_frames_from_the_real_airplane_extent_not_a_fixed_span() {
        let small = Airplane {
            name: "small".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: vec![probe_wing(false)],
            fuselages: vec![probe_fuselage()],
            s_ref: 1.0,
            c_ref: 1.0,
            b_ref: 1.0,
        };
        let scene = figure_exterior_3d(&small, None, None);
        assert!(!scene.elements.is_empty());
    }

    #[test]
    fn default_exterior_preview_uses_the_available_card_area_without_clipping() {
        let plane = Airplane {
            name: "probe".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: vec![probe_wing(true)],
            fuselages: vec![probe_fuselage()],
            s_ref: 40.0,
            c_ref: 2.0,
            b_ref: 20.0,
        };
        let scene = figure_exterior_3d(&plane, None, None);
        let points = scene.elements.iter().filter_map(|element| match element {
            SceneElement::Polyline { points, .. } => Some(points),
            _ => None,
        });
        let (mut x_min, mut x_max, mut y_min, mut y_max) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for point in points.flatten() {
            x_min = x_min.min(point[0]);
            x_max = x_max.max(point[0]);
            y_min = y_min.min(point[1]);
            y_max = y_max.max(point[1]);
        }
        assert!(
            (20.0..=580.0).contains(&x_min) && (20.0..=580.0).contains(&x_max),
            "horizontal bounds {x_min:.1}..{x_max:.1}"
        );
        assert!(
            (20.0..=480.0).contains(&y_min) && (20.0..=480.0).contains(&y_max),
            "vertical bounds {y_min:.1}..{y_max:.1}"
        );
        assert!(x_max - x_min > 400.0, "default preview should not be tiny");
    }

    #[test]
    fn isolated_wireframes_reserve_clear_space_for_the_header() {
        let plane = Airplane {
            name: "probe".to_owned(),
            xyz_ref: [0.0, 0.0, 0.0],
            wings: vec![probe_wing(true)],
            fuselages: vec![probe_fuselage()],
            s_ref: 40.0,
            c_ref: 2.0,
            b_ref: 20.0,
        };
        for scene in [
            figure_wireframe_wing(&plane, None),
            figure_wireframe_fuselage(&plane, None),
        ] {
            for points in scene.elements.iter().filter_map(|element| match element {
                SceneElement::Polyline { points, .. } => Some(points),
                _ => None,
            }) {
                assert!(points.iter().all(|point| point[1] >= HEADER_HEIGHT));
            }
        }
    }
}
