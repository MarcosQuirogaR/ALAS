// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py
// (`figure_wireframe_wing` L1164-1186, `figure_wireframe_fuselage` L1189-1204,
// `figure_wireframe_empennage` L1207-1233)
// and alas/sidecar/figures.py (`_preview_exterior` L372-407)

//! Aircraft surface figures and section-grid overlays using the live-preview geometry.

use std::f64::consts::PI;

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::section_outline::mirror_y;
use alas_geom::aircraft::wing::Wing;

use crate::scene::{
    Camera3D, Color, Point2D, Point3D, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use crate::theme::get_palette;

use super::sandbox_scene::{FramingReference, SandboxSceneModel, SandboxSceneOptions};

const BULKHEAD_SEGMENTS: usize = 24;
const LONGERON_COUNT: usize = 8;
const HEADER_HEIGHT: f64 = 48.0;
const WIREFRAME_PADDING: f64 = 0.025;

mod sections;
mod visibility;
use sections::{draw_surface_sections, section_stations};

/// A `(center, max_span)` pair sized from a data-space bounding box, with
/// modest padding: what every wireframe figure hands [`Camera3D::project`]
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

fn wing_fit_points(wing: &Wing) -> Vec<Point3D> {
    wing.section_outlines(Some(80))
        .into_iter()
        .flatten()
        .flat_map(|point| {
            if wing.symmetric {
                vec![point, mirror_y(point)]
            } else {
                vec![point]
            }
        })
        .collect()
}
fn fuselage_fit_points(fuselage: &Fuselage) -> Vec<Point3D> {
    let mut points = Vec::with_capacity(fuselage.xsecs.len() * BULKHEAD_SEGMENTS);
    for sec in &fuselage.xsecs {
        for index in 0..BULKHEAD_SEGMENTS {
            let theta = 2.0 * PI * index as f64 / BULKHEAD_SEGMENTS as f64;
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

/// Draw resolved airfoil sections and spanwise skin lines in aircraft axes.
pub(crate) fn draw_wing_wireframe(
    scene: &mut Scene,
    cam: &Camera3D,
    center: Point3D,
    max_span: f64,
    viewport: (f64, f64, f64, f64),
    wing: &Wing,
    color: Color,
) {
    let outlines = wing.section_outlines(Some(40));
    let mirrors: &[bool] = if wing.symmetric {
        &[false, true]
    } else {
        &[false]
    };
    let project_line = |scene: &mut Scene, points: &[Point3D], width, closed| {
        for &mirror in mirrors {
            if points.len() >= 2 {
                let mut projected = points
                    .iter()
                    .map(|&point| {
                        cam.project(
                            if mirror { mirror_y(point) } else { point },
                            center,
                            max_span,
                            viewport,
                        )
                    })
                    .collect::<Vec<_>>();
                if closed {
                    projected.push(projected[0]);
                }
                scene.add(SceneElement::Polyline {
                    points: projected,
                    stroke: Stroke::new(color, width),
                });
            }
        }
    };
    for index in section_stations(wing) {
        if let Some(outline) = outlines.get(index) {
            project_line(scene, outline, 0.9, true);
        }
    }
    let count = outlines.iter().map(Vec::len).min().unwrap_or(0);
    if count < 2 {
        return;
    }
    let mut chord_stations = (0..5)
        .map(|fraction| fraction * (count - 1) / 4)
        .collect::<Vec<_>>();
    chord_stations.dedup();
    for index in chord_stations {
        let line = outlines
            .iter()
            .map(|outline| outline[index])
            .collect::<Vec<_>>();
        project_line(scene, &line, 0.35, false);
    }
}
/// Draw one fuselage's per-station bulkhead ellipses, longerons and
/// centerline in aircraft axes from the stored width and height.
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

fn surface_scene(
    plane: &Airplane,
    camera: Option<Camera3D>,
    theme: Option<&str>,
    title: Option<&str>,
) -> Scene {
    let camera = camera.unwrap_or(Camera3D {
        elev_deg: 30.0,
        azim_deg: -150.0,
        zoom: 1.0,
    });
    let top_offset = if title.is_some() {
        HEADER_HEIGHT - 20.0
    } else {
        0.0
    };
    let points = airplane_fit_points(plane);
    let bounds = points
        .iter()
        .map(|&point| camera.project(point, [0.0; 3], 1.0, (0.0, 0.0, 1.0, 1.0)))
        .fold(
            [
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
            ],
            |mut bounds, point| {
                bounds[0] = bounds[0].min(point[0]);
                bounds[1] = bounds[1].max(point[0]);
                bounds[2] = bounds[2].min(point[1]);
                bounds[3] = bounds[3].max(point[1]);
                bounds
            },
        );
    let aspect = (bounds[3] - bounds[2]) / (bounds[1] - bounds[0]).max(1e-6);
    let height = if title.is_some() {
        (HEADER_HEIGHT + 32.0 + 560.0 * aspect).clamp(180.0, 450.0)
    } else {
        500.0
    };
    let canvas = (600.0, height - top_offset);
    let viewport = (20.0, 20.0, canvas.0 - 40.0, canvas.1 - 40.0);
    let center = camera.fit_center_to_points(&points, [0.0; 3]);
    let extent = camera.fit_span_to_points(&points, viewport, WIREFRAME_PADDING);
    let options = SandboxSceneOptions {
        canvas,
        reference: Some(FramingReference { center, extent }),
        ..SandboxSceneOptions::default()
    };
    let model = SandboxSceneModel::new(plane, 80);
    let (mut scene, framing) = model.render(Some(camera), theme, &options);
    scene
        .elements
        .retain(|element| !matches!(element, SceneElement::Line { .. }));
    for element in &mut scene.elements {
        if let SceneElement::Polygon {
            fill: Some(fill),
            stroke,
            ..
        } = element
        {
            let lighter = |channel| (0.7 * f64::from(channel) + 0.3 * 255.0) as u8;
            fill.color = Color::rgb(
                lighter(fill.color.r),
                lighter(fill.color.g),
                lighter(fill.color.b),
            );
            *stroke = Some(Stroke::new(fill.color, 0.5));
        }
    }
    let section_color = Color::from_hex("#17263d");
    let visibility = visibility::ContourVisibility::new(model.faces(), framing);
    draw_surface_sections(&mut scene, plane, &visibility, section_color);
    scene.height = height;
    for element in &mut scene.elements {
        match element {
            SceneElement::Polygon { points, .. } | SceneElement::Polyline { points, .. } => {
                for point in points {
                    point[1] += top_offset;
                }
            }
            SceneElement::Line { p1, p2, .. } => {
                p1[1] += top_offset;
                p2[1] += top_offset;
            }
            _ => {}
        }
    }
    if let Some(title) = title {
        title_text(&mut scene, title, Color::from_hex(get_palette(theme).title));
        scene.render_title = true;
    }
    scene
}

/// Isolated main wing with the live preview's airfoil loft and surface colors.
pub fn figure_wireframe_wing(plane: &Airplane, theme: Option<&str>) -> Scene {
    let mut isolated = plane.clone();
    isolated.wings = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Main Wing")
        .or_else(|| plane.wings.first())
        .cloned()
        .into_iter()
        .collect();
    isolated.fuselages.clear();
    surface_scene(&isolated, None, theme, Some("Wing Wireframe"))
}

/// Isolated fuselage loft with its defining bulkhead outlines.
pub fn figure_wireframe_fuselage(plane: &Airplane, theme: Option<&str>) -> Scene {
    let mut isolated = plane.clone();
    isolated.wings.clear();
    isolated.fuselages = plane
        .fuselages
        .iter()
        .find(|fus| fus.name == "Fuselage")
        .or_else(|| plane.fuselages.first())
        .cloned()
        .into_iter()
        .collect();
    surface_scene(&isolated, None, theme, Some("Fuselage Wireframe"))
}

/// Horizontal and vertical tail lofts, including each resolved airfoil section.
pub fn figure_wireframe_empennage(plane: &Airplane, theme: Option<&str>) -> Scene {
    let mut isolated = plane.clone();
    let hstab = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Horizontal Stabilizer")
        .or_else(|| plane.wings.get(1));
    let vstab = plane
        .wings
        .iter()
        .find(|wing| wing.name == "Vertical Stabilizer")
        .or_else(|| plane.wings.get(2));
    isolated.wings = [hstab, vstab].into_iter().flatten().cloned().collect();
    isolated.fuselages.clear();
    surface_scene(
        &isolated,
        None,
        theme,
        Some("Empennage Wireframe (H-Stab & V-Stab)"),
    )
}

/// Full exterior using the same loft, visibility ordering and colors as the live preview.
pub fn figure_exterior_3d(
    plane: &Airplane,
    camera: Option<Camera3D>,
    theme: Option<&str>,
) -> Scene {
    surface_scene(plane, camera, theme, None)
}
#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::sections::defining_stations;
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
    fn sparse_sections_preserve_kinks_and_endpoints_across_subdivisions() {
        let airfoil = Airfoil::from_name("naca2412").expect("naca");
        let wing = Wing::new(
            "Main Wing",
            (0..=20)
                .map(|index| {
                    let span = f64::from(index);
                    WingXSec::new(
                        [(span - 7.0).max(0.0) * 0.5, span, 0.0],
                        4.0 - 0.1 * span,
                        0.0,
                        airfoil.clone(),
                    )
                })
                .collect(),
            true,
        );
        assert_eq!(defining_stations(&wing), vec![0, 7, 20]);
        let selected = section_stations(&wing);
        assert!(selected.contains(&7));
        assert!(selected.len() < wing.xsecs.len());
    }

    #[test]
    fn profile_breaks_survive_while_linear_airfoil_blends_stay_sparse() {
        let airfoil = Airfoil::from_name("naca2412").expect("naca");
        let wing = Wing::new(
            "Main Wing",
            (0..=20)
                .map(|index| {
                    let span = f64::from(index);
                    let mut section = airfoil.clone();
                    let thickness_scale = 1.0 + 0.02 * span.min(7.0);
                    for point in &mut section.coordinates {
                        point.1 *= thickness_scale;
                    }
                    WingXSec::new([0.0, span, 0.0], 2.0, 0.0, section)
                })
                .collect(),
            true,
        );
        assert_eq!(defining_stations(&wing), vec![0, 7, 20]);
        assert!(section_stations(&wing).contains(&7));
        assert!(section_stations(&wing).len() < wing.xsecs.len());
    }

    #[test]
    fn coarse_wings_get_closed_intermediate_contours_on_the_actual_loft() {
        let plane = Airplane {
            name: "probe".to_owned(),
            xyz_ref: [0.0; 3],
            wings: vec![probe_wing(true)],
            fuselages: vec![],
            s_ref: 40.0,
            c_ref: 2.0,
            b_ref: 20.0,
        };
        for theme in ["light", "dark"] {
            let (mut scene, framing) = SandboxSceneModel::new(&plane, 80).render(
                None,
                Some(theme),
                &SandboxSceneOptions::default(),
            );
            scene.elements.clear();
            // Verify the complete loft contours before hidden-line removal.
            let visibility = visibility::ContourVisibility::new(&[], framing);
            draw_surface_sections(&mut scene, &plane, &visibility, Color::rgb(0, 0, 0));
            let rings = scene
                .elements
                .iter()
                .filter_map(|element| match element {
                    SceneElement::Polyline { points, .. } => Some(points),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(rings.len(), 10);
            for ring in &rings {
                assert_eq!(ring.first(), ring.last());
            }
            for ((root, tip), middle) in rings[0].iter().zip(rings[2]).zip(rings[6]) {
                for axis in 0..2 {
                    let expected = (root[axis] + tip[axis]) * 0.5;
                    assert!(
                        (middle[axis] - expected).abs()
                            <= 128.0 * f64::EPSILON * expected.abs().max(1.0)
                    );
                }
            }
        }
    }

    #[test]
    fn a_symmetric_wing_wireframe_draws_both_sides() {
        let mut scene = Scene::new(200.0, 200.0, None);
        let wing = probe_wing(true);
        let reference = FramingReference::enclosing(&wing_fit_points(&wing)).expect("wing extent");
        let (center, span) = (reference.center, reference.extent);
        draw_wing_wireframe(
            &mut scene,
            &Camera3D::default(),
            center,
            span,
            (0.0, 0.0, 200.0, 200.0),
            &wing,
            Color::rgb(0, 200, 255),
        );
        assert_eq!(
            scene.elements.len(),
            2 * (section_stations(&wing).len() + 5)
        );
        assert!(
            wing_fit_points(&wing).iter().any(|point| point[1] < 0.0),
            "symmetric bbox reaches the mirrored side"
        );
    }

    #[test]
    fn section_grid_preserves_resolved_airfoil_camber_thickness_and_twist() {
        let mut wing = probe_wing(true);
        wing.xsecs[1].twist = -7.0;
        let outlines = wing.section_outlines(Some(40));
        let reference = FramingReference::enclosing(&wing_fit_points(&wing)).expect("extent");
        let camera = Camera3D::default();
        let viewport = (0.0, 0.0, 600.0, 450.0);
        let mut scene = Scene::new(600.0, 450.0, None);
        draw_wing_wireframe(
            &mut scene,
            &camera,
            reference.center,
            reference.extent,
            viewport,
            &wing,
            Color::rgb(0, 0, 0),
        );
        for (index, outline) in outlines.iter().enumerate() {
            let min_z = outline
                .iter()
                .map(|point| point[2])
                .fold(f64::INFINITY, f64::min);
            let max_z = outline
                .iter()
                .map(|point| point[2])
                .fold(f64::NEG_INFINITY, f64::max);
            assert!(max_z > min_z, "resolved section has finite thickness");
            for (side, mirror) in [false, true].into_iter().enumerate() {
                let mut expected = outline
                    .iter()
                    .map(|&point| {
                        camera.project(
                            if mirror { mirror_y(point) } else { point },
                            reference.center,
                            reference.extent,
                            viewport,
                        )
                    })
                    .collect::<Vec<_>>();
                expected.push(expected[0]);
                let SceneElement::Polyline { points, .. } = &scene.elements[2 * index + side]
                else {
                    panic!("section outline must remain a polyline");
                };
                assert_eq!(points, &expected);
            }
        }
    }

    #[test]
    fn surface_figures_contain_opaque_loft_faces_and_respond_to_airfoil_changes() {
        let mut plane = Airplane {
            name: "probe".to_owned(),
            xyz_ref: [0.0; 3],
            wings: vec![probe_wing(true)],
            fuselages: vec![probe_fuselage()],
            s_ref: 40.0,
            c_ref: 2.0,
            b_ref: 20.0,
        };
        let original = figure_wireframe_wing(&plane, None);
        assert!(original
            .elements
            .iter()
            .any(|element| matches!(element, SceneElement::Polygon { fill: Some(_), .. })));
        for element in &original.elements {
            if let SceneElement::Polygon {
                fill: Some(fill), ..
            } = element
            {
                assert_eq!(fill.color.a, 255);
            }
        }
        plane.wings[0].xsecs[1].airfoil = Airfoil::from_name("naca0018").expect("naca");
        plane.wings[0].xsecs[1].twist = -8.0;
        assert_ne!(original, figure_wireframe_wing(&plane, None));
        let fuselage = figure_wireframe_fuselage(&plane, None);
        assert!(fuselage
            .elements
            .iter()
            .any(|element| matches!(element, SceneElement::Polygon { fill: Some(_), .. })));
    }

    #[test]
    fn a_fuselage_wireframe_draws_one_bulkhead_per_station_plus_longerons_and_centerline() {
        let mut scene = Scene::new(200.0, 200.0, None);
        let fus = probe_fuselage();
        let reference =
            FramingReference::enclosing(&fuselage_fit_points(&fus)).expect("fuselage extent");
        let (center, span) = (reference.center, reference.extent);
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
            SceneElement::Polygon { points, .. } | SceneElement::Polyline { points, .. } => {
                Some(points)
            }
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
                SceneElement::Polygon { points, .. } | SceneElement::Polyline { points, .. } => {
                    Some(points)
                }
                _ => None,
            }) {
                assert!(points.iter().all(|point| point[1] >= HEADER_HEIGHT));
            }
        }
    }
}
