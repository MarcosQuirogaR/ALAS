// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Unit and property tests for `alas-viz` scene rendering and transforms.

use alas_report::scene::{
    Color, Fill, Point2D, Scene, SceneElement, Stroke, TextAlign, TextBaseline,
};
use alas_viz::{
    render_scene_to_shapes, to_egui_color, to_egui_stroke, SceneViewState, ViewportTransform,
};
use egui::{pos2, vec2, Color32, Rect};

#[test]
fn viewport_transform_fit_centers_and_scales_preserving_aspect() {
    let target = Rect::from_min_max(pos2(0.0, 0.0), pos2(800.0, 600.0));
    let transform = ViewportTransform::fit(400.0, 200.0, target);

    // 400x200 in 800x600: width ratio is 2.0, height ratio is 3.0 -> scale should be 2.0
    assert!((transform.scale - 2.0).abs() < 1e-4);
    assert!((transform.offset_x - 0.0).abs() < 1e-4);
    // Rendered height is 400.0 in 600.0 height -> offset_y should be (600 - 400) / 2 = 100.0
    assert!((transform.offset_y - 100.0).abs() < 1e-4);
}

#[test]
fn coordinate_round_trip_mapping_is_consistent() {
    let target = Rect::from_min_max(pos2(50.0, 100.0), pos2(850.0, 700.0));
    let transform = ViewportTransform::fit(600.0, 400.0, target);

    let canvas_pt: Point2D = [150.0, 250.0];
    let screen_pos = transform.to_screen(canvas_pt);
    let recovered_pt = transform.to_canvas(screen_pos);

    assert!((recovered_pt[0] - canvas_pt[0]).abs() < 1e-3);
    assert!((recovered_pt[1] - canvas_pt[1]).abs() < 1e-3);
}

#[test]
fn color_conversion_premultiplies_straight_alpha() {
    let opaque = to_egui_color(&Color::rgb(25, 100, 200));
    assert_eq!(opaque, Color32::from_rgb(25, 100, 200));

    // `Color32` stores premultiplied channels; passing the straight channels
    // through would paint a translucent white grid line as opaque white.
    let egui_col = to_egui_color(&Color::rgba(25, 100, 200, 128));
    assert_eq!(egui_col.r(), 13);
    assert_eq!(egui_col.g(), 50);
    assert_eq!(egui_col.b(), 100);
    assert_eq!(egui_col.a(), 128);
}

#[test]
fn thin_scene_lines_keep_their_fractional_screen_width() {
    let mut scene = Scene::new(100.0, 100.0, None);
    scene.add(SceneElement::Line {
        p1: [10.0, 10.0],
        p2: [90.0, 90.0],
        stroke: Stroke::new(Color::rgb(0, 0, 0), 0.6),
    });
    scene.add(SceneElement::Polyline {
        points: vec![[10.0, 90.0], [50.0, 50.0], [90.0, 10.0]],
        stroke: Stroke::new(Color::rgb(0, 0, 0), 1.2),
    });
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0));
    let transform = ViewportTransform::fit(100.0, 100.0, rect);
    let shapes = render_scene_to_shapes(&scene, &transform);

    // Truncating a sub-pixel width to whole pixels gives an invisible stroke.
    let widths = shapes
        .iter()
        .filter_map(|shape| match shape {
            egui::Shape::LineSegment { stroke, .. } => Some(stroke.width),
            egui::Shape::Path(path) => Some(path.stroke.width),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(widths, vec![0.6, 1.2]);
}

#[test]
fn stroke_conversion_scales_line_width() {
    let stroke = Stroke::new(Color::rgb(255, 0, 0), 2.0);
    let egui_stroke = to_egui_stroke(&stroke, 1.5);

    assert!((egui_stroke.width - 3.0).abs() < 1e-3);
    assert_eq!(egui_stroke.color, Color32::from_rgb(255, 0, 0));
}

#[test]
fn scene_with_all_primitives_produces_valid_shapes() {
    let mut scene = Scene::new(600.0, 400.0, Some(Color::rgb(20, 20, 20)));

    scene.add(SceneElement::Line {
        p1: [0.0, 0.0],
        p2: [100.0, 100.0],
        stroke: Stroke::new(Color::rgb(255, 255, 255), 1.0),
    });
    scene.add(SceneElement::Polyline {
        points: vec![[0.0, 0.0], [50.0, 20.0], [100.0, 0.0]],
        stroke: Stroke::new(Color::rgb(0, 200, 0), 1.5),
    });
    scene.add(SceneElement::Polygon {
        points: vec![[10.0, 10.0], [30.0, 10.0], [20.0, 40.0]],
        fill: Some(Fill::new(Color::rgba(0, 0, 255, 100))),
        stroke: Some(Stroke::new(Color::rgb(0, 0, 255), 1.0)),
    });
    scene.add(SceneElement::Rect {
        x: 50.0,
        y: 50.0,
        width: 100.0,
        height: 80.0,
        rx: 4.0,
        fill: Some(Fill::new(Color::rgb(50, 50, 50))),
        stroke: None,
    });
    scene.add(SceneElement::Circle {
        center: [200.0, 200.0],
        radius: 30.0,
        fill: Some(Fill::new(Color::rgb(255, 200, 0))),
        stroke: Some(Stroke::new(Color::rgb(0, 0, 0), 2.0)),
    });
    scene.add(SceneElement::Text {
        text: "ALAS Test Label".to_string(),
        pos: [100.0, 100.0],
        font_size: 14.0,
        color: Color::rgb(255, 255, 255),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: true,
    });

    let target = Rect::from_min_max(pos2(0.0, 0.0), pos2(600.0, 400.0));
    let transform = ViewportTransform::fit(scene.width, scene.height, target);
    let shapes = render_scene_to_shapes(&scene, &transform);

    // The bold label uses a second offset glyph pass to emulate font weight.
    assert_eq!(shapes.len(), 8);
    assert!(
        shapes
            .iter()
            .filter(|shape| matches!(shape, egui::Shape::Text(_)))
            .count()
            >= 2
    );
}

#[test]
fn rotated_scene_text_reaches_egui_with_angle_and_bold_weight() {
    let mut scene = Scene::new(200.0, 120.0, None);
    scene.add(SceneElement::Text {
        text: "Y axis".to_owned(),
        pos: [40.0, 60.0],
        font_size: 12.0,
        color: Color::rgb(0, 0, 0),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: -90.0,
        bold: true,
    });

    let transform = ViewportTransform::fit(
        scene.width,
        scene.height,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 120.0)),
    );
    let shapes = render_scene_to_shapes(&scene, &transform);
    let text_shapes = shapes
        .iter()
        .filter_map(|shape| match shape {
            egui::Shape::Text(text) => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(text_shapes.len(), 2);
    assert!((text_shapes[0].angle + std::f32::consts::FRAC_PI_2).abs() < 1e-5);
}

#[test]
fn multiline_scene_text_has_one_centered_shape_per_line() {
    let mut scene = Scene::new(200.0, 120.0, None);
    scene.add(SceneElement::Text {
        text: "Required\nAvailable".to_owned(),
        pos: [100.0, 60.0],
        font_size: 12.0,
        color: Color::rgb(0, 0, 0),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });

    let transform = ViewportTransform::fit(
        scene.width,
        scene.height,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 120.0)),
    );
    let shapes = render_scene_to_shapes(&scene, &transform);
    let text = shapes
        .iter()
        .filter_map(|shape| match shape {
            egui::Shape::Text(text) => Some(text),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(text.len(), 2);
    assert!(text[0].pos.y < 60.0);
    assert!(text[1].pos.y > text[0].pos.y);
    assert!((text[0].galley.job.sections[0].format.font_id.size - 16.0).abs() < 1e-4);
}

#[test]
fn text_size_scales_with_the_viewport_without_an_overlap_floor() {
    let mut scene = Scene::new(200.0, 120.0, None);
    scene.add(SceneElement::Text {
        text: "Scale".to_owned(),
        pos: [100.0, 60.0],
        font_size: 12.0,
        color: Color::rgb(0, 0, 0),
        align: TextAlign::Center,
        baseline: TextBaseline::Middle,
        angle_deg: 0.0,
        bold: false,
    });
    let transform = ViewportTransform::fit(
        scene.width,
        scene.height,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(50.0, 30.0)),
    );

    let shapes = render_scene_to_shapes(&scene, &transform);
    let egui::Shape::Text(text) = &shapes[0] else {
        panic!("text scene must render as text");
    };
    assert!((text.galley.job.sections[0].format.font_id.size - 4.0).abs() < 1e-4);
}

#[test]
fn scene_view_state_resets_cleanly() {
    let mut state = SceneViewState {
        zoom: 4.5,
        pan: egui::vec2(100.0, -50.0),
        auto_fit: false,
        cursor_canvas: Some([123.0, 456.0]),
    };

    state.reset();

    assert!((state.zoom - 1.0).abs() < 1e-4);
    assert_eq!(state.pan, egui::Vec2::ZERO);
    assert!(state.auto_fit);
    assert_eq!(state.cursor_canvas, None);
}

#[test]
fn scene_view_state_clamps_pan_to_a_recoverable_canvas_position() {
    let mut state = SceneViewState {
        pan: egui::vec2(10_000.0, -10_000.0),
        zoom: 1.0,
        auto_fit: false,
        cursor_canvas: None,
    };

    state.clamp_pan(egui::vec2(400.0, 300.0), egui::vec2(800.0, 200.0));

    assert_eq!(state.pan, egui::vec2(600.0, -250.0));
}

#[test]
fn scene_view_state_sanitizes_non_finite_pan() {
    let mut state = SceneViewState {
        pan: egui::vec2(f32::NAN, f32::INFINITY),
        ..SceneViewState::default()
    };

    state.clamp_pan(egui::vec2(400.0, 300.0), egui::vec2(800.0, 200.0));

    assert_eq!(state.pan, egui::Vec2::ZERO);
}

#[test]
fn scene_view_state_keeps_the_pointer_canvas_position_fixed_while_zooming() {
    let mut state = SceneViewState::default();
    let fit_offset = egui::vec2(180.0, 60.0);
    let pointer = egui::vec2(360.0, 210.0);
    let canvas_before = pointer - fit_offset;

    state.zoom_about(fit_offset, pointer, 1.5);

    let canvas_after = fit_offset + state.pan + canvas_before * state.zoom;
    assert!(!state.auto_fit);
    assert!((state.zoom - 1.5).abs() < 1e-6);
    assert!((canvas_after.x - pointer.x).abs() < 1e-5);
    assert!((canvas_after.y - pointer.y).abs() < 1e-5);
}

/// Tessellate the shapes of a scene the way the desktop painter does and
/// return the vertex bounds and the count of non-finite vertices.
fn tessellated_bounds(scene: &Scene, rect: egui::Rect) -> (egui::Rect, usize) {
    let transform = ViewportTransform::fit(scene.width, scene.height, rect);
    let shapes = render_scene_to_shapes(scene, &transform);
    let mut tessellator = egui::epaint::tessellator::Tessellator::new(
        1.0,
        egui::epaint::TessellationOptions::default(),
        [0, 0],
        vec![],
    );
    let mut mesh = egui::epaint::Mesh::default();
    for shape in shapes {
        tessellator.tessellate_shape(shape, &mut mesh);
    }
    let mut bounds = egui::Rect::NOTHING;
    let mut non_finite = 0;
    for v in &mesh.vertices {
        if v.pos.x.is_finite() && v.pos.y.is_finite() {
            bounds = bounds.union(egui::Rect::from_min_max(v.pos, v.pos));
        } else {
            non_finite += 1;
        }
    }
    (bounds, non_finite)
}

#[test]
fn sliver_polygons_tessellate_inside_their_own_bounds() {
    // Lofted faces seen edge-on: a hairline quad, an exactly reversed quad
    // (two coincident edges), one with repeated trailing-edge points and a
    // needle triangle.
    let face = |points: Vec<[f64; 2]>| SceneElement::Polygon {
        points,
        fill: Some(Fill::new(Color::rgb(40, 90, 200))),
        stroke: Some(Stroke::new(Color::rgb(200, 200, 200), 0.35)),
    };
    let mut scene = Scene::new(600.0, 500.0, Some(Color::rgb(10, 10, 10)));
    scene.render_title = false;
    scene.add(face(vec![
        [100.0, 100.0],
        [400.0, 300.0],
        [400.0, 300.02],
        [100.0, 100.01],
    ]));
    scene.add(face(vec![
        [120.0, 380.0],
        [420.0, 120.0],
        [120.0, 380.0],
        [420.0, 120.0],
    ]));
    scene.add(face(vec![
        [50.0, 50.0],
        [50.0, 50.0],
        [300.0, 52.0],
        [300.0, 52.0],
        [50.0, 50.0],
    ]));
    scene.add(face(vec![[200.0, 200.0], [260.0, 205.0], [200.0, 200.3]]));
    let rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(600.0, 500.0));
    let (bounds, non_finite) = tessellated_bounds(&scene, rect);
    assert_eq!(non_finite, 0, "tessellation produced non-finite vertices");
    assert!(
        rect.expand(6.0).contains_rect(bounds),
        "sliver faces escaped their canvas: {bounds:?}"
    );
}

#[test]
fn well_conditioned_polygons_keep_the_feathered_convex_path() {
    let mut scene = Scene::new(300.0, 300.0, None);
    scene.render_title = false;
    scene.add(SceneElement::Polygon {
        points: vec![[20.0, 20.0], [200.0, 30.0], [210.0, 220.0], [30.0, 200.0]],
        fill: Some(Fill::new(Color::rgb(1, 2, 3))),
        stroke: None,
    });
    let rect = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(300.0, 300.0));
    let transform = ViewportTransform::fit(scene.width, scene.height, rect);
    let shapes = render_scene_to_shapes(&scene, &transform);
    assert!(
        shapes.iter().any(|s| matches!(s, egui::Shape::Path(_))),
        "a convex quad still uses the anti-aliased egui path"
    );
    assert!(!shapes.iter().any(|s| matches!(s, egui::Shape::Mesh(_))));
}

#[test]
fn concave_faces_are_triangulated_without_changing_the_boundary_area() {
    let boundary = vec![
        [20.0, 20.0],
        [220.0, 20.0],
        [220.0, 100.0],
        [130.0, 100.0],
        [130.0, 220.0],
        [20.0, 220.0],
    ];
    let mut scene = Scene::new(300.0, 300.0, None);
    scene.render_title = false;
    scene.add(SceneElement::Polygon {
        points: boundary.clone(),
        fill: Some(Fill::new(Color::rgb(1, 2, 3))),
        stroke: Some(Stroke::new(Color::rgb(200, 200, 200), 0.35)),
    });
    let rect = egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(300.0, 300.0));
    let transform = ViewportTransform::fit(scene.width, scene.height, rect);
    let shapes = render_scene_to_shapes(&scene, &transform);
    let mesh = shapes
        .iter()
        .find_map(|shape| match shape {
            egui::Shape::Mesh(mesh) => Some(mesh),
            _ => None,
        })
        .expect("concave fill should use a mesh");
    assert_eq!(mesh.indices.len(), (boundary.len() - 2) * 3);

    let polygon_area = |points: &[[f32; 2]]| {
        points
            .iter()
            .zip(points.iter().cycle().skip(1))
            .take(points.len())
            .map(|(left, right)| left[0] * right[1] - right[0] * left[1])
            .sum::<f32>()
            .abs()
            * 0.5
    };
    let expected_area = polygon_area(
        &boundary
            .iter()
            .map(|point| [point[0] as f32, point[1] as f32])
            .collect::<Vec<_>>(),
    );
    let rendered_area = mesh
        .indices
        .chunks_exact(3)
        .map(|triangle| {
            let a = mesh.vertices[triangle[0] as usize].pos;
            let b = mesh.vertices[triangle[1] as usize].pos;
            let c = mesh.vertices[triangle[2] as usize].pos;
            polygon_area(&[[a.x, a.y], [b.x, b.y], [c.x, c.y]])
        })
        .sum::<f32>();
    assert!((rendered_area - expected_area).abs() < 1.0e-3);
}
