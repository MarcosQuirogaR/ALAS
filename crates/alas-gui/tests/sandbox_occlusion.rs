// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Depth ordering of the sandbox preview: a nearer opaque surface must
//! cover what lies behind it from every camera.
//!
//! The desktop painter draws the scene's polygons in order through the
//! egui tessellator; this test rasterizes those triangles the way the GPU
//! would and compares every interior pixel against a z-buffer oracle
//! built from the same lofted faces. Sorting whole faces by mean depth
//! (the previous order) painted wing and tail roots over the fuselage;
//! the partition order must not.

mod common;

use alas_gui::sandbox::scene::{build_sandbox_airplane, build_sandbox_model, SANDBOX_CAMERA_ID};
use alas_gui::state::{AppState, PreviewCamera};
use alas_report::families::geometry::{SandboxSceneModel, SceneComponent, SceneFraming};
use alas_report::scene::{Color, Fill, Scene, SceneElement, Stroke};
use alas_viz::{render_scene_to_shapes, ViewportTransform};
use egui::epaint::{tessellator::Tessellator, TessellationOptions};
use egui::{pos2, Pos2, Rect};

const WIDTH: usize = 920;
const HEIGHT: usize = 730;

fn rect() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(WIDTH as f32, HEIGHT as f32))
}

fn camera(elev: f64, azim: f64, zoom: f64) -> PreviewCamera {
    PreviewCamera {
        pitch_deg: elev,
        yaw_deg: azim,
        zoom,
    }
}

/// What a painted pixel reads as, by hue: the wing blue, the fuselage
/// grey, or something else (tails, nacelles, outlines, background).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Painted {
    Wing,
    Fuselage,
    Other,
}

/// The outline colour of the dark palette the tests render with.
fn spine() -> Color {
    Color::from_hex(alas_gui::theme::AppTheme::Dark.palette().spine)
}

fn classify(r: u8, g: u8, b: u8, a: u8) -> Painted {
    if a < 128 {
        return Painted::Other;
    }
    let s = spine();
    if r.abs_diff(s.r) < 8 && g.abs_diff(s.g) < 8 && b.abs_diff(s.b) < 8 {
        // An outline stroke, drawn over both surfaces.
        return Painted::Other;
    }
    let (r, g, b) = (i32::from(r), i32::from(g), i32::from(b));
    if b - r > 70 {
        Painted::Wing
    } else if (r - g).abs() < 16 && (g - b).abs() < 24 && r > 60 {
        Painted::Fuselage
    } else {
        Painted::Other
    }
}

/// Fill a screen triangle, calling `pixel(index, weights)` for each
/// covered pixel centre with its barycentric weights.
fn fill_triangle_weights(p: [Pos2; 3], mut pixel: impl FnMut(usize, [f64; 3])) {
    let min_x = p
        .iter()
        .map(|q| q.x)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as usize;
    let max_x = (p
        .iter()
        .map(|q| q.x)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil() as usize)
        .min(WIDTH - 1);
    let min_y = p
        .iter()
        .map(|q| q.y)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as usize;
    let max_y = (p
        .iter()
        .map(|q| q.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil() as usize)
        .min(HEIGHT - 1);
    let area =
        f64::from((p[1].x - p[0].x) * (p[2].y - p[0].y) - (p[2].x - p[0].x) * (p[1].y - p[0].y));
    if area.abs() < 1e-9 || min_x > max_x || min_y > max_y {
        return;
    }
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5);
            let w0 = ((f64::from(p[1].x) - cx) * (f64::from(p[2].y) - cy)
                - (f64::from(p[2].x) - cx) * (f64::from(p[1].y) - cy))
                / area;
            let w1 = ((f64::from(p[2].x) - cx) * (f64::from(p[0].y) - cy)
                - (f64::from(p[0].x) - cx) * (f64::from(p[2].y) - cy))
                / area;
            let w2 = 1.0 - w0 - w1;
            if w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0 {
                pixel(y * WIDTH + x, [w0, w1, w2]);
            }
        }
    }
}

/// Fill a screen triangle, calling `pixel(index, depth)` for each covered
/// pixel centre with the linearly interpolated `depth`.
fn fill_triangle(p: [Pos2; 3], depth: [f64; 3], mut pixel: impl FnMut(usize, f64)) {
    fill_triangle_weights(p, |i, w| {
        pixel(i, w[0] * depth[0] + w[1] * depth[1] + w[2] * depth[2]);
    });
}

/// The desktop path: every scene element to egui shapes, tessellated,
/// then each triangle composited in order. Pixels in the outline colour
/// read as neither surface.
fn painted(scene: &Scene) -> Vec<Painted> {
    let transform = ViewportTransform::fit(scene.width, scene.height, rect());
    let shapes = render_scene_to_shapes(scene, &transform);
    let mut tessellator = Tessellator::new(1.0, TessellationOptions::default(), [0, 0], vec![]);
    let mut mesh = egui::epaint::Mesh::default();
    for shape in shapes {
        tessellator.tessellate_shape(shape, &mut mesh);
    }
    // Premultiplied RGBA, blended over black the way the GPU composites
    // the feathered triangles, so a thin fragment covers a pixel partially.
    let mut buffer = vec![[0.0f64; 4]; WIDTH * HEIGHT];
    for triangle in mesh.indices.chunks(3) {
        let v: Vec<_> = triangle
            .iter()
            .map(|&i| mesh.vertices[i as usize])
            .collect();
        if v.iter()
            .any(|q| !q.pos.x.is_finite() || !q.pos.y.is_finite())
        {
            continue;
        }
        let colours: Vec<[f64; 4]> = v
            .iter()
            .map(|q| {
                let c = q.color;
                [
                    f64::from(c.r()),
                    f64::from(c.g()),
                    f64::from(c.b()),
                    f64::from(c.a()),
                ]
            })
            .collect();
        fill_triangle_weights([v[0].pos, v[1].pos, v[2].pos], |i, w| {
            let mut src = [0.0; 4];
            for k in 0..4 {
                src[k] = w[0] * colours[0][k] + w[1] * colours[1][k] + w[2] * colours[2][k];
            }
            let keep = 1.0 - src[3] / 255.0;
            for k in 0..4 {
                buffer[i][k] = src[k] + buffer[i][k] * keep;
            }
        });
    }
    buffer
        .iter()
        .map(|c| {
            let a = c[3].clamp(0.0, 255.0);
            if a < 128.0 {
                return Painted::Other;
            }
            // Un-premultiply before judging the hue.
            let f = 255.0 / a;
            classify(
                (c[0] * f).clamp(0.0, 255.0) as u8,
                (c[1] * f).clamp(0.0, 255.0) as u8,
                (c[2] * f).clamp(0.0, 255.0) as u8,
                a as u8,
            )
        })
        .collect()
}

/// The z-buffer oracle over the lofted faces: the nearest component at
/// every pixel centre, by the camera's own view depth.
fn oracle(model: &SandboxSceneModel, framing: &SceneFraming) -> Vec<Option<SceneComponent>> {
    let transform = ViewportTransform::fit(framing.canvas.0, framing.canvas.1, rect());
    let mut depth = vec![f64::NEG_INFINITY; WIDTH * HEIGHT];
    let mut nearest = vec![None; WIDTH * HEIGHT];
    for face in model.faces() {
        let screen: Vec<Pos2> = face
            .points
            .iter()
            .map(|&p| transform.to_screen(framing.project(p)))
            .collect();
        let depths: Vec<f64> = face
            .points
            .iter()
            .map(|&p| framing.camera.view_depth(p, framing.center))
            .collect();
        for k in 1..screen.len() - 1 {
            fill_triangle(
                [screen[0], screen[k], screen[k + 1]],
                [depths[0], depths[k], depths[k + 1]],
                |i, d| {
                    if d > depth[i] {
                        depth[i] = d;
                        nearest[i] = Some(face.component);
                    }
                },
            );
        }
    }
    nearest
}

/// Whether every pixel within `radius` of `index` has the same oracle
/// component, so anti-aliased edges and outlines are not counted.
fn interior(nearest: &[Option<SceneComponent>], index: usize, radius: usize) -> bool {
    let (x, y) = (index % WIDTH, index / WIDTH);
    let here = nearest[index];
    for dy in y.saturating_sub(radius)..=(y + radius).min(HEIGHT - 1) {
        for dx in x.saturating_sub(radius)..=(x + radius).min(WIDTH - 1) {
            if nearest[dy * WIDTH + dx] != here {
                return false;
            }
        }
    }
    true
}

/// Pixels within one pixel of a drawn fragment edge (every fragment edge
/// carries a hairline in the scene), where anti-aliasing decides the
/// colour rather than the painting order.
fn edge_mask(scene: &Scene) -> Vec<bool> {
    let transform = ViewportTransform::fit(scene.width, scene.height, rect());
    let mut mask = vec![false; WIDTH * HEIGHT];
    for element in &scene.elements {
        let SceneElement::Line { p1, p2, .. } = element else {
            continue;
        };
        let a = transform.to_screen(*p1);
        let b = transform.to_screen(*p2);
        let steps = (a.distance(b).ceil() as usize).max(1);
        for k in 0..=steps {
            let t = k as f32 / steps as f32;
            let p = a + (b - a) * t;
            let (x, y) = (p.x.floor() as i64, p.y.floor() as i64);
            for dy in -1..=1 {
                for dx in -1..=1 {
                    let (px, py) = (x + dx, y + dy);
                    if px >= 0 && py >= 0 && (px as usize) < WIDTH && (py as usize) < HEIGHT {
                        mask[py as usize * WIDTH + px as usize] = true;
                    }
                }
            }
        }
    }
    mask
}

/// Interior pixels away from every fragment edge where the painter shows
/// the wing but the fuselage is nearer, or the fuselage where the wing is
/// nearer.
fn mismatches(image: &[Painted], nearest: &[Option<SceneComponent>], edges: &[bool]) -> usize {
    (0..WIDTH * HEIGHT)
        .filter(|&i| {
            let expected = match nearest[i] {
                Some(SceneComponent::Wing) => Painted::Wing,
                Some(SceneComponent::Fuselage) => Painted::Fuselage,
                _ => return false,
            };
            let got = image[i];
            got != Painted::Other && got != expected && !edges[i] && interior(nearest, i, 2)
        })
        .count()
}

/// The previous painter: whole faces sorted by their mean view depth.
fn mean_depth_scene(model: &SandboxSceneModel, framing: &SceneFraming) -> Scene {
    let mut scene = Scene::new(framing.canvas.0, framing.canvas.1, None);
    scene.render_title = false;
    let mut faces: Vec<_> = model.faces().iter().collect();
    let depth = |points: &[[f64; 3]]| {
        points
            .iter()
            .map(|&p| framing.camera.view_depth(p, framing.center))
            .sum::<f64>()
            / points.len() as f64
    };
    faces.sort_by(|a, b| depth(&a.points).total_cmp(&depth(&b.points)));
    for face in faces {
        let c = face.component.color();
        let points: Vec<[f64; 2]> = face.points.iter().map(|&p| framing.project(p)).collect();
        scene.add(SceneElement::Polygon {
            points: points.clone(),
            fill: Some(Fill::new(Color::rgb(c.r, c.g, c.b))),
            stroke: None,
        });
        for (i, &p1) in points.iter().enumerate() {
            scene.add(SceneElement::Line {
                p1,
                p2: points[(i + 1) % points.len()],
                stroke: Stroke::new(spine(), 0.35),
            });
        }
    }
    scene
}

fn cameras() -> Vec<PreviewCamera> {
    let mut cameras = vec![
        camera(22.0, -125.0, 1.0),
        camera(22.0, -125.0, 1.8),
        PreviewCamera::top(),
        PreviewCamera::front(),
        PreviewCamera::side(),
    ];
    for azim in (-180..180).step_by(30) {
        for elev in [-60.0, -20.0, 0.0, 15.0, 45.0, 80.0] {
            cameras.push(camera(elev, f64::from(azim), 1.4));
        }
    }
    cameras
}

/// The partitioned scene and the previous mean-depth scene for one camera,
/// with the oracle; returns `(partition mismatches, mean-depth mismatches,
/// fuselage or wing interior pixels)`.
fn compare(state: &AppState, model: &SandboxSceneModel) -> (usize, usize, usize) {
    let (scene, framing) = alas_gui::sandbox::scene::project_sandbox_model(state, model);
    let nearest = oracle(model, &framing);
    let counted = (0..WIDTH * HEIGHT)
        .filter(|&i| {
            matches!(
                nearest[i],
                Some(SceneComponent::Wing) | Some(SceneComponent::Fuselage)
            ) && interior(&nearest, i, 2)
        })
        .count();
    let partition = mismatches(&painted(&scene), &nearest, &edge_mask(&scene));
    let previous_scene = mean_depth_scene(model, &framing);
    let previous = mismatches(
        &painted(&previous_scene),
        &nearest,
        &edge_mask(&previous_scene),
    );
    (partition, previous, counted)
}

#[test]
fn nearer_fuselage_skin_covers_wing_and_tail_roots_from_every_camera() {
    let mut state = AppState::default();
    state.sandbox.viewport_size = Some((WIDTH as f32, HEIGHT as f32));
    let (plane, _) = build_sandbox_airplane(&state).expect("AVE builds");
    let model = build_sandbox_model(&plane);
    let mut failures = Vec::new();
    let mut worst_previous = 0usize;
    let mut report = Vec::new();
    for cam in cameras() {
        *state.preview_camera_mut(SANDBOX_CAMERA_ID) = cam;
        let (partition, previous, counted) = compare(&state, &model);
        report.push(format!(
            "elev {:>5} azim {:>5} zoom {:.1}: partition {partition:>5} / mean-depth {previous:>5} mismatched of {counted} interior wing+fuselage pixels",
            cam.pitch_deg, cam.yaw_deg, cam.zoom
        ));
        worst_previous = worst_previous.max(previous);
        // A handful of pixels may differ where two surfaces meet within
        // the partition's plane tolerance; anything visible is a defect.
        let allowed = (counted / 2000).max(8);
        if partition > allowed {
            failures.push(format!(
                "camera elev {} azim {} zoom {}: {partition} mismatched pixels (allowed {allowed})",
                cam.pitch_deg, cam.yaw_deg, cam.zoom
            ));
        }
    }
    println!("{}", report.join("\n"));
    assert!(
        worst_previous > 500,
        "the oracle must catch the previous mean-depth defect: worst {worst_previous}"
    );
    assert!(
        failures.is_empty(),
        "wing or fuselage painted over a nearer surface:\n{}",
        failures.join("\n")
    );
}

/// Writes the oracle comparison log and before/after renders of the worst
/// cameras to an internal evidence directory (2026-09-14); run
/// with `--ignored`.
#[test]
#[ignore = "writes evidence images"]
fn write_occlusion_evidence() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../out/evidence/sandbox-depth-layout-scale-2026-09-14");
    std::fs::create_dir_all(&dir).expect("evidence directory");
    let mut state = AppState::default();
    state.sandbox.viewport_size = Some((WIDTH as f32, HEIGHT as f32));
    let (plane, _) = build_sandbox_airplane(&state).expect("AVE builds");
    let started = std::time::Instant::now();
    let model = build_sandbox_model(&plane);
    let build = started.elapsed();
    let mut rows = vec![format!(
        "AVE interactive model: {} faces, {} fragments, partition built in {:.1} ms (test profile)",
        model.faces().len(),
        model.fragments().count(),
        build.as_secs_f64() * 1e3
    )];
    let mut worst: Vec<(usize, PreviewCamera)> = Vec::new();
    for cam in cameras() {
        *state.preview_camera_mut(SANDBOX_CAMERA_ID) = cam;
        let (partition, previous, counted) = compare(&state, &model);
        rows.push(format!(
            "elev {:>5} azim {:>5} zoom {:.1}: partition {partition:>5} / mean-depth {previous:>5} mismatched of {counted}",
            cam.pitch_deg, cam.yaw_deg, cam.zoom
        ));
        worst.push((previous, cam));
    }
    std::fs::write(dir.join("occlusion-oracle.txt"), rows.join("\n")).expect("write log");
    worst.sort_by_key(|(n, _)| std::cmp::Reverse(*n));
    let mut picks: Vec<(&str, PreviewCamera)> = vec![
        ("iso", camera(22.0, -125.0, 1.0)),
        ("top", PreviewCamera::top()),
        ("front", PreviewCamera::front()),
        ("side", PreviewCamera::side()),
    ];
    picks.push(("above", camera(80.0, -180.0, 1.4)));
    picks.push(("worst-1", worst[0].1));
    picks.push(("worst-2", worst[1].1));
    for (name, cam) in picks {
        *state.preview_camera_mut(SANDBOX_CAMERA_ID) = cam;
        let (after, framing) = alas_gui::sandbox::scene::project_sandbox_model(&state, &model);
        let before = mean_depth_scene(&model, &framing);
        for (label, scene) in [("before", before), ("after", after)] {
            let mut scene = scene;
            scene.background = Some(Color::from_hex("#1e1e1e"));
            let png = alas_viz::raster::render_scene_png(&scene).expect("png");
            std::fs::write(
                dir.join(format!(
                    "occlusion-{name}-elev{}-azim{}-{label}.png",
                    cam.pitch_deg as i64, cam.yaw_deg as i64
                )),
                png,
            )
            .expect("write png");
        }
    }
}

/// A diagnostic image: painter classes (blue wing, grey fuselage, dark
/// other) with the interior mismatches against the oracle in red.
fn mismatch_png(image: &[Painted], nearest: &[Option<SceneComponent>], edges: &[bool]) -> Vec<u8> {
    let mut rgba = vec![0u8; WIDTH * HEIGHT * 4];
    for i in 0..WIDTH * HEIGHT {
        let expected = match nearest[i] {
            Some(SceneComponent::Wing) => Some(Painted::Wing),
            Some(SceneComponent::Fuselage) => Some(Painted::Fuselage),
            _ => None,
        };
        let got = image[i];
        let bad = expected.is_some_and(|e| {
            got != Painted::Other && got != e && !edges[i] && interior(nearest, i, 2)
        });
        let (r, g, b) = if bad {
            (255, 0, 0)
        } else {
            match got {
                Painted::Wing => (37, 99, 235),
                Painted::Fuselage => (184, 190, 200),
                Painted::Other => (40, 40, 40),
            }
        };
        rgba[i * 4..i * 4 + 4].copy_from_slice(&[r, g, b, 255]);
    }
    alas_viz::raster::encode_png_rgba(WIDTH as u32, HEIGHT as u32, &rgba).expect("png")
}

/// Writes mismatch maps for the top and one oblique camera; run with
/// `--ignored`.
#[test]
#[ignore = "writes evidence images"]
fn write_occlusion_mismatch_maps() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../out/evidence/sandbox-depth-layout-scale-2026-09-14");
    std::fs::create_dir_all(&dir).expect("evidence directory");
    let mut state = AppState::default();
    state.sandbox.viewport_size = Some((WIDTH as f32, HEIGHT as f32));
    let (plane, _) = build_sandbox_airplane(&state).expect("AVE builds");
    let model = build_sandbox_model(&plane);
    for (name, cam) in [
        ("top", PreviewCamera::top()),
        ("elev45-azim-120", camera(45.0, -120.0, 1.4)),
        ("elev80-azim-180", camera(80.0, -180.0, 1.4)),
    ] {
        *state.preview_camera_mut(SANDBOX_CAMERA_ID) = cam;
        let (scene, framing) = alas_gui::sandbox::scene::project_sandbox_model(&state, &model);
        let nearest = oracle(&model, &framing);
        std::fs::write(
            dir.join(format!("mismatch-{name}-partition.png")),
            mismatch_png(&painted(&scene), &nearest, &edge_mask(&scene)),
        )
        .expect("write");
        std::fs::write(dir.join(format!("mismatch-{name}-mean-depth.png")), {
            let previous = mean_depth_scene(&model, &framing);
            mismatch_png(&painted(&previous), &nearest, &edge_mask(&previous))
        })
        .expect("write");
    }
}
