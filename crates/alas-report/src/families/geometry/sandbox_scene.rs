// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The sandbox workspace's central aircraft scene.
//!
//! Shared with the report surface figures, this scene shows every lifting surface as
//! its actual lofted skin: each cross-section's resolved airfoil outline is
//! placed in aircraft axes by [`Wing::section_outlines`] (chord, twist,
//! placement and symmetry applied), consecutive outlines are joined into
//! quadrilaterals, and the faces are painted back to front so camber,
//! thickness, taper and twist read directly from the drawing. The fuselage
//! and nacelles are lofted between their bulkhead ellipses the same way.
//!
//! Painting order is exact, not a depth sort: [`SandboxSceneModel`] cuts
//! the faces where they cross each other (wing and tail roots through the
//! fuselage skin) into a binary space partition once per geometry, and each
//! camera walks that tree back to front, so a nearer opaque surface always
//! covers what lies behind it from every angle. Only the edges of the
//! original faces are outlined; an edge made by a cut is sealed with a
//! hairline in the fragment's own colour, so the partition leaves no
//! visible seams or false lines on the skin.
//!
//! The scene can isolate one component. Isolation is a view filter only:
//! the caller passes the complete aircraft and the framing is fitted to the
//! filtered subset, so the aircraft definition is untouched.
//!
//! Framing is orientation independent: a [`FramingReference`] holds the
//! model point at the viewport centre and the model extent (the radius of
//! a sphere enclosing the fitted points) that maps to 45 % of the
//! viewport's smaller side, so rotating the camera never changes the
//! pixels per metre and the whole model stays inside the viewport at zoom 1
//! from any angle.
//! A caller that keeps the reference across geometry edits sees a span
//! change as a change of drawn size, not as a refit.
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
use rayon::prelude::*;

use super::sandbox_bsp::{BspPolygon, BspTree};
pub use super::sandbox_framing::{FramingReference, SceneFraming};

const BULKHEAD_SEGMENTS: usize = 24;
/// Width of the outline stroke on original face edges, in canvas units.
const OUTLINE_WIDTH: f64 = 0.35;
/// Width of the seam stroke that hides a cut edge, in canvas units.
const SEAM_WIDTH: f64 = 0.7;
/// Plane tolerance of the partition, as a fraction of the model extent:
/// lofted quads are only nearly planar, and a vertex this close to a
/// neighbouring face's plane must not cut a hairline off it.
const PLANE_TOLERANCE_FRACTION: f64 = 5e-4;
/// Cutting stops once the fragments exceed this multiple of the faces;
/// the AVE interactive model needs about twice its faces.
const MAX_FRAGMENTS_PER_FACE: usize = 16;
/// Fragments per parallel drawing chunk; a smaller model is drawn on the
/// calling thread.
const PARALLEL_FRAGMENTS: usize = 1024;

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

    /// The fill colour the component is drawn with before shading.
    pub fn color(self) -> Color {
        match self {
            Self::Wing => Color::from_hex("#2563eb"),
            Self::HorizontalTail => Color::from_hex("#22b8a0"),
            Self::VerticalTail => Color::from_hex("#b57cf0"),
            Self::Fuselage => Color::from_hex("#b8bec8"),
            Self::Nacelles => Color::from_hex("#ff9900"),
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
    /// The framing to draw with; `None` encloses the shown components.
    pub reference: Option<FramingReference>,
}

impl Default for SandboxSceneOptions {
    fn default() -> Self {
        Self {
            isolate: None,
            max_section_points: 40,
            canvas: (600.0, 500.0),
            reference: None,
        }
    }
}

/// One lofted face of the aircraft in model axes, before partitioning.
#[derive(Debug, Clone, PartialEq)]
pub struct SandboxFace {
    /// The component the face belongs to.
    pub component: SceneComponent,
    /// The face vertices.
    pub points: Vec<Point3D>,
    /// The unit normal used for shading.
    pub normal: Point3D,
}

/// Headlight shading of a face colour. Faces are fully opaque: a skin
/// that lets even a few per cent of what lies behind it through reads as
/// a wing showing through the fuselage.
fn shade(color: Color, factor: f64) -> Color {
    let channel = |value: u8| (f64::from(value) * factor).clamp(0.0, 255.0) as u8;
    Color::rgb(channel(color.r), channel(color.g), channel(color.b))
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

fn face(component: SceneComponent, points: Vec<Point3D>) -> SandboxFace {
    let normal = normal(&points);
    SandboxFace {
        component,
        points,
        normal,
    }
}

fn wing_faces(wing: &Wing, max_points: usize, component: SceneComponent) -> Vec<SandboxFace> {
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
                faces.push(face(
                    component,
                    vec![
                        place(inboard[k]),
                        place(inboard[k + 1]),
                        place(outboard[k + 1]),
                        place(outboard[k]),
                    ],
                ));
            }
        }
    }
    // The end faces: without them a tip section is a ring to look through.
    for outline in [outlines.first(), outlines.last()].into_iter().flatten() {
        for &mirror in mirrors {
            let place = |p: Point3D| if mirror { mirror_y(p) } else { p };
            let placed: Vec<Point3D> = outline.iter().map(|&p| place(p)).collect();
            faces.extend(
                section_end_faces(&placed)
                    .into_iter()
                    .map(|points| face(component, points)),
            );
        }
    }
    faces
}

/// One closed section outline as a strip across the chord: each point paired
/// with its opposite on the other surface, so a cambered, non-convex section
/// is filled exactly. The outline starts and ends at the trailing edge.
fn section_end_faces(outline: &[Point3D]) -> Vec<Vec<Point3D>> {
    let mut strips = Vec::new();
    let (mut low, mut high) = (0, outline.len().saturating_sub(1));
    while low + 1 < high {
        let mut points = vec![outline[low], outline[low + 1]];
        if low + 1 < high - 1 {
            points.push(outline[high - 1]);
        }
        points.push(outline[high]);
        strips.push(points);
        low += 1;
        high -= 1;
    }
    strips
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

fn fuselage_faces(fuselage: &Fuselage, component: SceneComponent) -> Vec<SandboxFace> {
    // The closed surface: no open tail or nozzle ring to look through.
    let rings: Vec<Vec<Point3D>> = fuselage
        .closed_stations()
        .iter()
        .map(|station| {
            let mut ring = bulkhead_ring(station);
            for point in &mut ring {
                point[0] += fuselage.ring_x_shift_m(station, point[2]);
            }
            ring
        })
        .collect();
    let mut faces = Vec::new();
    for pair in rings.windows(2) {
        let (fore, aft) = (&pair[0], &pair[1]);
        for k in 0..BULKHEAD_SEGMENTS {
            let next = (k + 1) % BULKHEAD_SEGMENTS;
            faces.push(face(
                component,
                vec![fore[k], fore[next], aft[next], aft[k]],
            ));
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

fn included(isolate: Option<SceneComponent>, component: SceneComponent) -> bool {
    isolate.is_none_or(|isolated| isolated == component)
}

/// A sandbox aircraft prepared for drawing: its lofted faces in model axes,
/// the partition tree that orders them exactly for any camera, and the
/// points each component is framed by. Build it once per geometry and
/// render it once per camera, focus or theme.
#[derive(Debug, Clone)]
pub struct SandboxSceneModel {
    faces: Vec<SandboxFace>,
    tree: BspTree,
    points: Vec<(SceneComponent, Point3D)>,
}

impl SandboxSceneModel {
    /// Loft and partition `plane` with at most `max_section_points` outline
    /// points per section.
    pub fn new(plane: &Airplane, max_section_points: usize) -> Self {
        let mut faces = Vec::new();
        let mut points = Vec::new();
        for wing in &plane.wings {
            let component = SceneComponent::of_wing(wing);
            points.extend(
                wing_points(wing, max_section_points)
                    .into_iter()
                    .map(|p| (component, p)),
            );
            faces.extend(wing_faces(wing, max_section_points, component));
        }
        for (index, fuselage) in plane.fuselages.iter().enumerate() {
            let component = SceneComponent::of_fuselage(index, fuselage);
            points.extend(
                fuselage_points(fuselage)
                    .into_iter()
                    .map(|p| (component, p)),
            );
            faces.extend(fuselage_faces(fuselage, component));
        }
        let all: Vec<Point3D> = points.iter().map(|&(_, p)| p).collect();
        let extent = FramingReference::enclosing(&all).map_or(1.0, |r| r.extent);
        let polygons = faces
            .iter()
            .enumerate()
            .map(|(source, face)| BspPolygon::face(face.points.clone(), source))
            .collect();
        let tree = BspTree::build(
            polygons,
            PLANE_TOLERANCE_FRACTION * extent,
            MAX_FRAGMENTS_PER_FACE * faces.len().max(1),
        );
        Self {
            faces,
            tree,
            points,
        }
    }

    /// The lofted faces before partitioning, in model axes.
    pub fn faces(&self) -> &[SandboxFace] {
        &self.faces
    }

    /// The face fragments after partitioning, each with its component.
    pub fn fragments(&self) -> impl Iterator<Item = (SceneComponent, &[Point3D])> {
        self.tree
            .polygons()
            .iter()
            .map(|p| (self.faces[p.source].component, p.points.as_slice()))
    }

    /// The orientation-independent framing that encloses the shown
    /// components, or a unit framing about the origin when none is shown.
    pub fn framing_reference(&self, isolate: Option<SceneComponent>) -> FramingReference {
        let points: Vec<Point3D> = self
            .points
            .iter()
            .filter(|(component, _)| included(isolate, *component))
            .map(|&(_, p)| p)
            .collect();
        FramingReference::enclosing(&points).unwrap_or(FramingReference {
            center: [0.0, 0.0, 0.0],
            extent: 1.0,
        })
    }

    /// The framing that makes the shown components fill `fill` of the part of
    /// a `canvas` the `inset` (left, top, right, bottom, canvas units) leaves
    /// free, as seen from `camera`, centred in that part: the tight fit of the
    /// projected bounding box, not the orientation-independent sphere. A caller
    /// keeps it across orbit, so the scale does not change while turning.
    pub fn tight_framing_reference(
        &self,
        isolate: Option<SceneComponent>,
        camera: Camera3D,
        canvas: (f64, f64),
        fill: f64,
        inset: (f64, f64, f64, f64),
    ) -> FramingReference {
        let points: Vec<Point3D> = self
            .points
            .iter()
            .filter(|(component, _)| included(isolate, *component))
            .map(|&(_, p)| p)
            .collect();
        if points.is_empty() {
            return self.framing_reference(isolate);
        }
        super::sandbox_framing::tight_reference(&points, camera, canvas, fill, inset)
    }

    /// Draw the model for `camera` and `options`, returning the framing
    /// used. Without an explicit reference the shown components are
    /// enclosed; with one, the same pixels per metre and centre are kept
    /// whatever the camera orientation or the geometry now drawn.
    pub fn render(
        &self,
        camera: Option<Camera3D>,
        theme: Option<&str>,
        options: &SandboxSceneOptions,
    ) -> (Scene, SceneFraming) {
        self.render_toned(camera, theme, options, &|_, colour| colour, &|_| false)
    }

    /// [`Self::render`] with `tone` applied to every colour a component is
    /// drawn with (fill and outline), so a caller can push chosen components
    /// into the background; components for which `behind` is true are drawn
    /// first, behind the rest whatever their depth.
    pub fn render_toned(
        &self,
        camera: Option<Camera3D>,
        theme: Option<&str>,
        options: &SandboxSceneOptions,
        tone: &(dyn Fn(SceneComponent, Color) -> Color + Sync),
        behind: &(dyn Fn(SceneComponent) -> bool + Sync),
    ) -> (Scene, SceneFraming) {
        let pal = get_palette(theme);
        let (width, height) = options.canvas;
        let mut scene = Scene::new(width, height, Some(Color::from_hex(pal.bg)));
        scene.render_title = false;
        let cam = camera.unwrap_or_default();
        let viewport = (20.0, 20.0, width - 40.0, height - 40.0);
        let outline = Color::from_hex(pal.spine);
        let reference = options
            .reference
            .unwrap_or_else(|| self.framing_reference(options.isolate));
        let framing = SceneFraming {
            camera: cam,
            center: reference.center,
            max_span: reference.extent,
            viewport,
            canvas: options.canvas,
        };
        let light = framing.view_direction();
        let (back, front): (Vec<usize>, Vec<usize>) = (self.tree)
            .painter_order(light)
            .into_iter()
            .partition(|&i| behind(self.faces[self.tree.polygons()[i].source].component));
        let order: Vec<usize> = back.into_iter().chain(front).collect();
        // One fragment's face and its outline, appended to `out`.
        let draw = |index: usize, out: &mut Vec<SceneElement>| {
            let fragment = &self.tree.polygons()[index];
            let face = &self.faces[fragment.source];
            if !included(options.isolate, face.component) {
                return;
            }
            let n = face.normal;
            let lambert = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).abs();
            let factor = 0.55 + 0.45 * lambert;
            let fill = tone(face.component, shade(face.component.color(), factor));
            let projected: Vec<Point2D> = fragment
                .points
                .iter()
                .map(|&p| framing.project(p))
                .collect();
            out.push(SceneElement::Polygon {
                points: projected.clone(),
                fill: Some(Fill::new(fill)),
                stroke: None,
            });
            let seam = Color::rgb(fill.r, fill.g, fill.b);
            for (i, &p1) in projected.iter().enumerate() {
                let p2 = projected[(i + 1) % projected.len()];
                let stroke = if fragment.edge_is_cut(i) {
                    Stroke::new(seam, SEAM_WIDTH)
                } else {
                    Stroke::new(tone(face.component, outline), OUTLINE_WIDTH)
                };
                out.push(SceneElement::Line { p1, p2, stroke });
            }
        };
        // A camera turn redraws every fragment: a large aircraft is drawn in
        // ordered chunks on the rayon pool and the chunks are added in painter
        // order, so the scene is element for element the sequential one.
        // Elements go through `Scene::add` on this thread, where a plot-data
        // capture would observe them.
        let parts: Vec<Vec<SceneElement>> = if order.len() < PARALLEL_FRAGMENTS {
            let mut out = Vec::with_capacity(order.len() * 5);
            order.iter().for_each(|&index| draw(index, &mut out));
            vec![out]
        } else {
            let chunk =
                PARALLEL_FRAGMENTS.max(order.len().div_ceil(4 * rayon::current_num_threads()));
            order
                .par_chunks(chunk)
                .map(|part| {
                    let mut out = Vec::with_capacity(part.len() * 5);
                    part.iter().for_each(|&index| draw(index, &mut out));
                    out
                })
                .collect()
        };
        scene.elements.reserve(parts.iter().map(Vec::len).sum());
        for element in parts.into_iter().flatten() {
            scene.add(element);
        }
        (scene, framing)
    }
}

/// Draw the sandbox aircraft scene and return the framing it used.
///
/// This lofts and partitions the aircraft on every call; a caller drawing
/// the same aircraft for several cameras keeps a [`SandboxSceneModel`]
/// instead. A filtered scene with nothing to show (an aircraft without the
/// requested component) returns an empty canvas whose framing still
/// projects finite coordinates around the origin.
pub fn figure_sandbox_exterior(
    plane: &Airplane,
    camera: Option<Camera3D>,
    theme: Option<&str>,
    options: &SandboxSceneOptions,
) -> (Scene, SceneFraming) {
    SandboxSceneModel::new(plane, options.max_section_points).render(camera, theme, options)
}

pub(super) fn bounds(points: &[Point3D]) -> [f64; 6] {
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

    /// Shoelace area of a polygon in the x-z plane, m2.
    fn area_xz(points: &[Point3D]) -> f64 {
        let n = points.len();
        (0..n)
            .map(|i| {
                let (a, b) = (points[i], points[(i + 1) % n]);
                a[0] * b[2] - b[0] * a[2]
            })
            .sum::<f64>()
            .abs()
            / 2.0
    }

    #[test]
    fn the_end_strip_fills_a_cambered_section_exactly() {
        // Trailing edge, upper surface with a concave stretch, leading edge, lower surface.
        let outline: Vec<Point3D> = [
            (1.0, 0.0),
            (0.7, 0.06),
            (0.4, 0.05),
            (0.1, 0.07),
            (0.0, 0.0),
            (0.1, -0.02),
            (0.4, -0.01),
            (0.7, 0.01),
            (1.0, 0.0),
        ]
        .iter()
        .map(|&(x, z)| [x, 3.0, z])
        .collect();
        let strips = section_end_faces(&outline);
        let total: f64 = strips.iter().map(|s| area_xz(s)).sum();
        assert!((total - area_xz(&outline[..outline.len() - 1])).abs() < 1e-12);
    }

    #[test]
    fn body_ends_are_closed_and_wing_tips_have_faces() {
        let plane = ave();
        let wing = &plane.wings[0];
        let open = wing_faces(wing, 30, SceneComponent::of_wing(wing));
        let outlines = wing.section_outlines(Some(30));
        let side_faces = (outlines.len() - 1) * (outlines[0].len() - 1);
        let mirrors = if wing.symmetric { 2 } else { 1 };
        assert!(open.len() > side_faces * mirrors, "no end faces");
        let body = &plane.fuselages[0];
        let faces = fuselage_faces(body, SceneComponent::of_fuselage(0, body));
        let last_x = body.xsecs.last().expect("stations").xyz_c[0];
        // Every point on the end station is the one closing point.
        let end_points: Vec<Point3D> = faces
            .iter()
            .flat_map(|f| f.points.iter().copied())
            .filter(|p| (p[0] - last_x).abs() < 1e-12)
            .collect();
        assert!(!end_points.is_empty());
        assert!(end_points.windows(2).all(|p| p[0] == p[1]));
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
    fn the_scene_does_not_depend_on_how_its_fragments_are_chunked() {
        let model = SandboxSceneModel::new(&ave(), 40);
        assert!(
            model.fragments().count() > 2 * PARALLEL_FRAGMENTS,
            "the model is drawn on the parallel path"
        );
        let camera = Some(Camera3D {
            elev_deg: 24.0,
            azim_deg: -131.0,
            zoom: 1.3,
        });
        let options = SandboxSceneOptions::default();
        let draw_on = |threads: usize| {
            rayon::ThreadPoolBuilder::new()
                .num_threads(threads)
                .build()
                .expect("thread pool")
                .install(|| model.render(camera, Some("dark"), &options).0)
        };
        let one = draw_on(1);
        assert_eq!(draw_on(3), one);
        assert_eq!(draw_on(8), one);
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
        let model = SandboxSceneModel::new(&plane, 40);
        let wing_fragments = model
            .fragments()
            .filter(|(component, _)| *component == SceneComponent::Wing)
            .count();
        let wing_faces = model
            .faces()
            .iter()
            .filter(|f| f.component == SceneComponent::Wing)
            .count();
        assert!(wing_fragments >= wing_faces);
        assert_eq!(polygon_count(&wing_only.0), wing_fragments);
        assert!(polygon_count(&all.0) > wing_fragments);
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
        let (scene, framing) = figure_sandbox_exterior(&plane, None, None, &options);
        let outlines = wing.section_outlines(Some(options.max_section_points));
        // The root outline's first point is a vertex of some drawn face,
        // projected with the returned framing.
        let projected = framing.project(outlines[0][0]);
        let drawn = scene.elements.iter().any(|element| match element {
            SceneElement::Polygon { points, .. } => points
                .iter()
                .any(|p| (p[0] - projected[0]).abs() < 1e-9 && (p[1] - projected[1]).abs() < 1e-9),
            _ => false,
        });
        assert!(drawn);
    }

    #[test]
    fn a_centre_pod_is_closed_on_its_lip_ahead_of_the_fin_root_and_a_wing_nacelle_is_open() {
        let entry = alas_config::presets::get("DC-10").expect("preset");
        let plane = alas_geom::builder::AircraftBuilder::new(Some(entry.geometry.clone()))
            .build(Some(&entry.design_vector), true)
            .expect("builds");
        let fin = plane
            .wings
            .iter()
            .find(|wing| wing.name.contains("Vertical"))
            .expect("fin");
        let fronts: Vec<(f64, f64)> = plane
            .fuselages
            .iter()
            .filter(|body| body.name.contains("Nacelle"))
            .map(|body| {
                let first = body.closed_stations()[0];
                (first.xyz_c[1], first.width)
            })
            .collect();
        let pods: Vec<_> = fronts.iter().filter(|(y, _)| y.abs() < 1e-9).collect();
        assert_eq!(pods.len(), 1);
        assert_eq!(pods[0].1, 0.0);
        assert!(fronts
            .iter()
            .filter(|(y, _)| y.abs() >= 1e-9)
            .all(|(_, width)| *width > 0.5));
        let pod = plane
            .fuselages
            .iter()
            .find(|body| body.name.contains("Nacelle") && body.xsecs[0].xyz_c[1].abs() < 1e-9)
            .expect("pod");
        assert!(pod.closed_stations()[0].xyz_c[0] < fin.xsecs[0].xyz_le[0]);
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

    #[test]
    fn rotating_the_camera_keeps_the_centre_and_the_pixels_per_metre() {
        let plane = ave();
        let model = SandboxSceneModel::new(&plane, 40);
        let options = SandboxSceneOptions::default();
        let (_, iso) = model.render(Some(Camera3D::isometric()), None, &options);
        let (_, top) = model.render(Some(Camera3D::top()), None, &options);
        let (_, side) = model.render(Some(Camera3D::side()), None, &options);
        let oblique = Camera3D {
            elev_deg: -35.0,
            azim_deg: 47.0,
            zoom: 1.0,
        };
        let (_, oblique) = model.render(Some(oblique), None, &options);
        for framing in [top, side, oblique] {
            assert_eq!(framing.center, iso.center);
            assert_eq!(framing.max_span, iso.max_span);
            assert!((framing.scale() - iso.scale()).abs() < 1e-12);
        }
        // Every projected vertex stays inside the viewport at zoom 1 from
        // any angle: the reference encloses the model.
        let (vx, vy, vw, vh) = iso.viewport;
        for azim in (-180..180).step_by(45) {
            for elev in [-80.0, -30.0, 0.0, 30.0, 80.0] {
                let camera = Camera3D {
                    elev_deg: elev,
                    azim_deg: f64::from(azim),
                    zoom: 1.0,
                };
                let (scene, _) = model.render(Some(camera), None, &options);
                for element in &scene.elements {
                    if let SceneElement::Polygon { points, .. } = element {
                        for p in points {
                            assert!(p[0] >= vx - 1e-6 && p[0] <= vx + vw + 1e-6);
                            assert!(p[1] >= vy - 1e-6 && p[1] <= vy + vh + 1e-6);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_kept_reference_draws_a_larger_span_larger_instead_of_refitting() {
        let config = AlasConfig::default();
        let build = |span: f64| {
            let design = DesignVector {
                span_m: span,
                ..DesignVector::default()
            };
            AircraftBuilder::new(Some(config.geometry.clone()))
                .build(Some(&design), true)
                .expect("aircraft")
        };
        let base = SandboxSceneModel::new(&build(DesignVector::default().span_m), 40);
        let wider = SandboxSceneModel::new(&build(DesignVector::default().span_m + 12.0), 40);
        let reference = base.framing_reference(None);
        let options = SandboxSceneOptions {
            reference: Some(reference),
            ..SandboxSceneOptions::default()
        };
        let camera = Some(Camera3D::top());
        let (_, before) = base.render(camera, None, &options);
        let (_, after) = wider.render(camera, None, &options);
        assert_eq!(before.reference(), after.reference());
        assert!((before.scale() - after.scale()).abs() < 1e-12);
        // Without a kept reference the wider aircraft would be refitted.
        let (_, refit) = wider.render(camera, None, &SandboxSceneOptions::default());
        assert!(refit.max_span > after.max_span);
        // The right tip really moves outward on screen by half the span
        // change (the top view maps model y to screen y).
        let tip = |model: &SandboxSceneModel, framing: &SceneFraming| {
            model
                .faces()
                .iter()
                .filter(|f| f.component == SceneComponent::Wing)
                .flat_map(|f| f.points.iter())
                .map(|&p| framing.project(p)[1])
                .fold(f64::NEG_INFINITY, f64::max)
        };
        let moved = tip(&wider, &after) - tip(&base, &before);
        let expected = 6.0 * after.scale();
        assert!(
            (moved - expected).abs() < 0.03 * expected,
            "tip moved {moved}, expected about {expected}"
        );
    }
}
