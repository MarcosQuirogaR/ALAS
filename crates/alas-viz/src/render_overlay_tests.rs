// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The chunked, parallel vector overlay paints exactly what the sequential
//! shape renderer and one tessellator paint for the same scene.

use alas_report::scene::{Color, Fill, Scene, SceneElement, Stroke};
use egui::epaint::Mesh;
use egui::Shape;

use crate::render::{
    join_mesh_runs, render_scene_to_shapes_with_context, vector_overlay_shapes, Baker,
    ViewportTransform,
};
use crate::view_cache::vector_overlay_scene;

fn context() -> egui::Context {
    let context = egui::Context::default();
    let _ = context.run(egui::RawInput::default(), |_| {});
    context
}

/// A dense scene of shaded quads, each outlined by four segments, like the
/// lofted aircraft the live preview draws: well past the parallel threshold.
fn dense_scene() -> Scene {
    let mut scene = Scene::new(800.0, 600.0, Some(Color::rgb(20, 20, 30)));
    scene.title = Some("Exterior".to_owned());
    for i in 0..4000_u32 {
        let x = f64::from(i % 80) * 9.7 + 3.0;
        let y = f64::from(i / 80) * 11.3 + 5.0;
        let quad = vec![
            [x, y],
            [x + 8.5, y + 0.7],
            [x + 7.9, y + 9.1],
            [x - 0.4, y + 8.2],
        ];
        let shade = (i % 200) as u8;
        scene.add(SceneElement::Polygon {
            points: quad.clone(),
            fill: Some(Fill::new(Color::rgb(shade, 90, 200 - shade / 2))),
            stroke: None,
        });
        for k in 0..4 {
            scene.add(SceneElement::Line {
                p1: quad[k],
                p2: quad[(k + 1) % 4],
                stroke: Stroke::new(Color::rgb(10, 10, 10), 0.6 + f64::from(k as u8) * 0.1),
            });
        }
    }
    scene
}

/// Everything a list of shapes paints, in order: the meshes appended into
/// one, and the text shapes' positions between them.
fn painted(shapes: Vec<Shape>) -> (Mesh, Vec<(usize, egui::Pos2)>) {
    let mut mesh = Mesh::default();
    let mut texts = Vec::new();
    for shape in shapes {
        match shape {
            Shape::Mesh(part) => mesh.append(part),
            Shape::Text(text) => texts.push((mesh.vertices.len(), text.pos)),
            other => panic!("an unbaked shape survived: {other:?}"),
        }
    }
    (mesh, texts)
}

#[test]
fn the_parallel_overlay_paints_the_same_vertices_in_the_same_order() {
    let context = context();
    let scene = dense_scene();
    let target = egui::Rect::from_min_size(egui::pos2(12.0, 30.0), egui::vec2(640.0, 480.0));
    let transform = ViewportTransform::fit(scene.width, scene.height, target);
    let baker = Baker::new(&context);

    let reference = baker.bake(render_scene_to_shapes_with_context(
        &vector_overlay_scene(&scene),
        &transform,
        &context,
    ));
    let parallel = vector_overlay_shapes(&scene, &transform, &context, Some(&baker));
    let meshes = |shapes: &[Shape]| {
        shapes
            .iter()
            .filter(|shape| matches!(shape, Shape::Mesh(_)))
            .count()
    };
    assert!(meshes(&parallel) > 1, "the dense scene is drawn in chunks");
    let joined = join_mesh_runs(parallel.clone());
    assert_eq!(meshes(&joined), 1, "the chunk meshes are joined into one");

    let (reference_mesh, reference_text) = painted(reference);
    assert!(!reference_text.is_empty(), "the title is drawn");
    for shapes in [parallel, joined] {
        let (mesh, text) = painted(shapes);
        assert_eq!(text, reference_text);
        assert_eq!(mesh.texture_id, reference_mesh.texture_id);
        assert_eq!(mesh.indices, reference_mesh.indices);
        assert_eq!(mesh.vertices.len(), reference_mesh.vertices.len());
        for (a, b) in mesh.vertices.iter().zip(&reference_mesh.vertices) {
            assert_eq!((a.pos, a.uv, a.color), (b.pos, b.uv, b.color));
        }
    }
}

#[test]
fn the_unbaked_overlay_matches_the_reference_shapes() {
    let context = context();
    let scene = dense_scene();
    let target = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 600.0));
    let transform = ViewportTransform::fit(scene.width, scene.height, target);
    let reference =
        render_scene_to_shapes_with_context(&vector_overlay_scene(&scene), &transform, &context);
    let parallel = vector_overlay_shapes(&scene, &transform, &context, None);
    assert_eq!(format!("{parallel:?}"), format!("{reference:?}"));
}
