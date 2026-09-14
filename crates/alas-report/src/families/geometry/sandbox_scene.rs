// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sandbox workspace's central aircraft scene.
//!
//! Unlike the report wireframes, this scene shows every lifting surface as
//! its actual lofted skin: each cross-section's resolved airfoil outline is
//! placed in aircraft axes by [`Wing::section_outlines`] (chord, twist,
//! placement and symmetry applied), consecutive outlines are joined into
//! quadrilaterals, and the faces are painted back to front so camber,
//! thickness, taper and twist read directly from the drawing. The fuselage
//! and nacelles are lofted between their bulkhead ellipses the same way.
//!
//! The scene can isolate one component. Isolation is a view filter only:
//! the caller passes the complete aircraft and the framing is fitted to the
//! filtered subset, so the aircraft definition is untouched.
//!
//! Axes: `x` aft, `y` toward the right wing tip, `z` up. The returned
//! [`SceneFraming`] lets a caller project further model points (drag handles,
//! labels) with exactly the transform the faces were drawn with.

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::section_outline::mirror_y;
use alas_geom::aircraft::wing::Wing;

use crate::scene::{Camera3D, Color, Fill, Point2D, Point3D, Scene, SceneElement, Stroke};
use crate::theme::get_palette;

use super::wireframe::framing;

const BULKHEAD_SEGMENTS: usize = 24;
const PADDING_FRACTION: f64 = 0.08;

/// One editable aircraft component the sandbox can isolate in the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SceneComponent {
    /// The main wing, both halves.
    Wing,
    /// The horizontal stabilizer.
    HorizontalTail,
    /// The vertical stabilizer.
    VerticalTail,
    /// The fuselage body.
    Fuselage,
    /// Every nacelle.
    Nacelles,
}

impl SceneComponent {
    /// Classify a lifting surface by the builder's naming convention.
    pub fn of_wing(wing: &Wing) -> Self {
        let name = wing.name.to_ascii_lowercase();
        if name.contains("horizontal") {
            Self::HorizontalTail
        } else if name.contains("vertical") || name.contains("fin") {
            Self::VerticalTail
        } else {
            Self::Wing
        }
    }

    /// Classify a body: the first fuselage is the fuselage, the rest are
    /// nacelles unless named otherwise.
    pub fn of_fuselage(index: usize, fuselage: &Fuselage) -> Self {
        let name = fuselage.name.to_ascii_lowercase();
        if name.contains("nacelle") || (index > 0 && !name.contains("fuselage")) {
            Self::Nacelles
        } else {
            Self::Fuselage
        }
    }
}

/// How the sandbox scene is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SandboxSceneOptions {
    /// Show only this component, fitted to the viewport; `None` shows all.
    pub isolate: Option<SceneComponent>,
    /// Upper bound on outline points per section for interactive rendering.
    pub max_section_points: usize,
    /// Canvas size in scene units.
    pub canvas: (f64, f64),
}

impl Default for SandboxSceneOptions {
    fn default() -> Self {
        Self {
            isolate: None,
            max_section_points: 40,
            canvas: (600.0, 500.0),
        }
    }
}

/// The projection a sandbox scene was drawn with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneFraming {
    /// The camera used for every face.
    pub camera: Camera3D,
    /// The model point mapped to the viewport centre.
    pub center: Point3D,
    /// The model span that fills the viewport.
    pub max_span: f64,
    /// The viewport rectangle inside the scene canvas.
    pub viewport: (f64, f64, f64, f64),
    /// The scene canvas size.
    pub canvas: (f64, f64),
}

impl SceneFraming {
    /// Project a model point into scene canvas coordinates.
    pub fn project(&self, point: Point3D) -> Point2D {
        self.camera
            .project(point, self.center, self.max_span, self.viewport)
    }

    /// The unit view direction (from the model toward the viewer) in model
    /// axes, used for headlight shading.
    pub fn view_direction(&self) -> Point3D {
        let elev = self.camera.elev_deg.to_radians();
        let azim = self.camera.azim_deg.to_radians();
        [azim.sin() * elev.cos(), azim.cos() * elev.cos(), elev.sin()]
    }
}

struct Face {
    points: Vec<Point3D>,
    color: Color,
    outline: Color,
}

fn shade(color: Color, factor: f64) -> Color {
    let channel = |value: u8| (f64::from(value) * factor).clamp(0.0, 255.0) as u8;
    Color::rgba(channel(color.r), channel(color.g), channel(color.b), 240)
}

fn normal(points: &[Point3D]) -> Point3D {
    if points.len() < 3 {
        return [0.0, 0.0, 1.0];
    }
    let a = points[0];
    let b = points[1];
    let c = points[points.len() - 1];
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if len < 1e-12 {
        [0.0, 0.0, 1.0]
    } else {
        [n[0] / len, n[1] / len, n[2] / len]
    }
}

fn wing_faces(wing: &Wing, max_points: usize, color: Color, outline: Color) -> Vec<Face> {
    let outlines = wing.section_outlines(Some(max_points));
    let mut faces = Vec::new();
    let mirrors: &[bool] = if wing.symmetric {
        &[false, true]
    } else {
        &[false]
    };
    for pair in outlines.windows(2) {
        let (inboard, outboard) = (&pair[0], &pair[1]);
        let count = inboard.len().min(outboard.len());
        if count < 2 {
            continue;
        }
        for &mirror in mirrors {
            let place = |p: Point3D| if mirror { mirror_y(p) } else { p };
            for k in 0..count - 1 {
                faces.push(Face {
                    points: vec![
                        place(inboard[k]),
                        place(inboard[k + 1]),
                        place(outboard[k + 1]),
                        place(outboard[k]),
                    ],
                    color,
                    outline,
                });
            }
        }
    }
    faces
}

fn bulkhead_ring(xsec: &alas_geom::aircraft::fuselage::FuselageXSec) -> Vec<Point3D> {
    (0..BULKHEAD_SEGMENTS)
        .map(|k| {
            let theta = std::f64::consts::TAU * (k as f64) / (BULKHEAD_SEGMENTS as f64);
            [
                xsec.xyz_c[0],
                xsec.xyz_c[1] + (xsec.width / 2.0) * theta.cos(),
                xsec.xyz_c[2] + (xsec.height / 2.0) * theta.sin(),
            ]
        })
        .collect()
}

fn fuselage_faces(fuselage: &Fuselage, color: Color, outline: Color) -> Vec<Face> {
    let rings: Vec<Vec<Point3D>> = fuselage.xsecs.iter().map(bulkhead_ring).collect();
    let mut faces = Vec::new();
    for pair in rings.windows(2) {
        let (fore, aft) = (&pair[0], &pair[1]);
        for k in 0..BULKHEAD_SEGMENTS {
            let next = (k + 1) % BULKHEAD_SEGMENTS;
            faces.push(Face {
                points: vec![fore[k], fore[next], aft[next], aft[k]],
                color,
                outline,
            });
        }
    }
    faces
}

fn wing_points(wing: &Wing, max_points: usize) -> Vec<Point3D> {
    let mut points = Vec::new();
    for outline in wing.section_outlines(Some(max_points)) {
        for p in outline {
            points.push(p);
            if wing.symmetric {
                points.push(mirror_y(p));
            }
        }
    }
    points
}

fn fuselage_points(fuselage: &Fuselage) -> Vec<Point3D> {
    fuselage.xsecs.iter().flat_map(bulkhead_ring).collect()
}

fn included(options: &SandboxSceneOptions, component: SceneComponent) -> bool {
    options.isolate.is_none_or(|isolated| isolated == component)
}

/// Draw the sandbox aircraft scene and return the framing it used.
///
/// A filtered scene with nothing to show (an aircraft without the requested
/// component) returns an empty canvas whose framing still projects finite
/// coordinates around the origin.
pub fn figure_sandbox_exterior(
    plane: &Airplane,
    camera: Option<Camera3D>,
    theme: Option<&str>,
    options: &SandboxSceneOptions,
) -> (Scene, SceneFraming) {
    let pal = get_palette(theme);
    let (width, height) = options.canvas;
    let mut scene = Scene::new(width, height, Some(Color::from_hex(pal.bg)));
    scene.render_title = false;
    let cam = camera.unwrap_or_default();
    let viewport = (20.0, 20.0, width - 40.0, height - 40.0);
    let outline = Color::from_hex(pal.spine);

    let mut fit_points = Vec::new();
    let mut faces = Vec::new();
    for wing in &plane.wings {
        let component = SceneComponent::of_wing(wing);
        if !included(options, component) {
            continue;
        }
        let color = match component {
            SceneComponent::Wing => Color::from_hex("#2563eb"),
            _ => Color::from_hex("#8f9bb3"),
        };
        fit_points.extend(wing_points(wing, options.max_section_points));
        faces.extend(wing_faces(wing, options.max_section_points, color, outline));
    }
    for (index, fuselage) in plane.fuselages.iter().enumerate() {
        let component = SceneComponent::of_fuselage(index, fuselage);
        if !included(options, component) {
            continue;
        }
        let color = match component {
            SceneComponent::Fuselage => Color::from_hex("#b8bec8"),
            _ => Color::from_hex("#ff9900"),
        };
        fit_points.extend(fuselage_points(fuselage));
        faces.extend(fuselage_faces(fuselage, color, outline));
    }

    let (center, max_span) = if fit_points.is_empty() {
        ([0.0, 0.0, 0.0], 1.0)
    } else {
        let bbox = bounds(&fit_points);
        let fallback = framing(bbox[0], bbox[1], bbox[2], bbox[3], bbox[4], bbox[5]).0;
        (
            cam.fit_center_to_points(&fit_points, fallback),
            cam.fit_span_to_points(&fit_points, viewport, PADDING_FRACTION),
        )
    };
    let framing = SceneFraming {
        camera: cam,
        center,
        max_span,
        viewport,
        canvas: options.canvas,
    };

    let light = framing.view_direction();
    faces.sort_by(|a, b| {
        let depth = |face: &Face| {
            face.points
                .iter()
                .map(|&p| cam.view_depth(p, center))
                .sum::<f64>()
                / face.points.len() as f64
        };
        depth(a).total_cmp(&depth(b))
    });
    for face in faces {
        let n = normal(&face.points);
        let lambert = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).abs();
        let factor = 0.55 + 0.45 * lambert;
        scene.add(SceneElement::Polygon {
            points: face.points.iter().map(|&p| framing.project(p)).collect(),
            fill: Some(Fill::new(shade(face.color, factor))),
            stroke: Some(Stroke::new(face.outline, 0.35)),
        });
    }
    (scene, framing)
}

fn bounds(points: &[Point3D]) -> [f64; 6] {
    let mut b = [
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in points {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].max(p[0]);
        b[2] = b[2].min(p[1]);
        b[3] = b[3].max(p[1]);
        b[4] = b[4].min(p[2]);
        b[5] = b[5].max(p[2]);
    }
    b
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::{AlasConfig, DesignVector};
    use alas_geom::builder::AircraftBuilder;

    fn ave() -> Airplane {
        let config = AlasConfig::default();
        AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&DesignVector::default()), true)
            .expect("default aircraft")
    }

    fn polygon_count(scene: &Scene) -> usize {
        scene
            .elements
            .iter()
            .filter(|e| matches!(e, SceneElement::Polygon { .. }))
            .count()
    }

    #[test]
    fn whole_aircraft_scene_lofts_sections_into_faces_on_every_component() {
        let plane = ave();
        let (scene, framing) =
            figure_sandbox_exterior(&plane, None, None, &SandboxSceneOptions::default());
        assert!(polygon_count(&scene) > 500);
        assert!(framing.max_span.is_finite() && framing.max_span > 10.0);
        for element in &scene.elements {
            if let SceneElement::Polygon { points, .. } = element {
                for p in points {
                    assert!(p[0].is_finite() && p[1].is_finite());
                }
            }
        }
    }

    #[test]
    fn isolating_the_wing_draws_only_wing_faces_and_refits_the_camera() {
        let plane = ave();
        let all = figure_sandbox_exterior(&plane, None, None, &SandboxSceneOptions::default());
        let wing_only = figure_sandbox_exterior(
            &plane,
            None,
            None,
            &SandboxSceneOptions {
                isolate: Some(SceneComponent::Wing),
                ..SandboxSceneOptions::default()
            },
        );
        let wing = plane
            .wings
            .iter()
            .find(|w| SceneComponent::of_wing(w) == SceneComponent::Wing)
            .expect("main wing");
        let expected = wing_faces(wing, 40, Color::rgb(0, 0, 0), Color::rgb(0, 0, 0)).len();
        assert_eq!(polygon_count(&wing_only.0), expected);
        assert!(polygon_count(&all.0) > expected);
        // Fitting to the wing alone uses a smaller model span than the whole
        // aircraft, so the isolated part fills the viewport.
        assert!(wing_only.1.max_span < all.1.max_span);
    }

    #[test]
    fn distinct_airfoils_produce_distinct_section_geometry_without_analysis() {
        let mut config = AlasConfig::default();
        let baseline = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&DesignVector::default()), false)
            .expect("baseline");
        config.geometry.wing.root_airfoil = "naca0012".to_owned();
        config.geometry.wing.tip_airfoil = "naca0012".to_owned();
        let symmetric = AircraftBuilder::new(Some(config.geometry.clone()))
            .build(Some(&DesignVector::default()), false)
            .expect("symmetric sections");
        let options = SandboxSceneOptions {
            isolate: Some(SceneComponent::Wing),
            ..SandboxSceneOptions::default()
        };
        let a = figure_sandbox_exterior(&baseline, None, None, &options).0;
        let b = figure_sandbox_exterior(&symmetric, None, None, &options).0;
        assert!(polygon_count(&a) > 0 && polygon_count(&b) > 0);
        assert_ne!(a.elements, b.elements);
    }

    #[test]
    fn section_faces_follow_the_model_outline_exactly() {
        let plane = ave();
        let wing = &plane.wings[0];
        let options = SandboxSceneOptions::default();
        let (_, framing) = figure_sandbox_exterior(&plane, None, None, &options);
        let outlines = wing.section_outlines(Some(options.max_section_points));
        let faces = wing_faces(
            wing,
            options.max_section_points,
            Color::rgb(0, 0, 0),
            Color::rgb(0, 0, 0),
        );
        // The first face's first two points are the root outline's first two
        // points, projected with the returned framing.
        let projected = framing.project(outlines[0][0]);
        let face_point = framing.project(faces[0].points[0]);
        assert_eq!(projected, face_point);
    }

    #[test]
    fn components_are_classified_by_builder_names() {
        let plane = ave();
        let kinds: Vec<SceneComponent> = plane.wings.iter().map(SceneComponent::of_wing).collect();
        assert_eq!(
            kinds,
            vec![
                SceneComponent::Wing,
                SceneComponent::HorizontalTail,
                SceneComponent::VerticalTail
            ]
        );
        let bodies: Vec<SceneComponent> = plane
            .fuselages
            .iter()
            .enumerate()
            .map(|(i, f)| SceneComponent::of_fuselage(i, f))
            .collect();
        assert_eq!(bodies[0], SceneComponent::Fuselage);
        assert!(bodies[1..].iter().all(|k| *k == SceneComponent::Nacelles));
    }

    #[test]
    fn isolating_a_missing_component_yields_an_empty_but_finite_scene() {
        let mut plane = ave();
        plane.fuselages.truncate(1);
        let (scene, framing) = figure_sandbox_exterior(
            &plane,
            None,
            None,
            &SandboxSceneOptions {
                isolate: Some(SceneComponent::Nacelles),
                ..SandboxSceneOptions::default()
            },
        );
        assert_eq!(polygon_count(&scene), 0);
        let p = framing.project([0.0, 0.0, 0.0]);
        assert!(p[0].is_finite() && p[1].is_finite());
    }
}
