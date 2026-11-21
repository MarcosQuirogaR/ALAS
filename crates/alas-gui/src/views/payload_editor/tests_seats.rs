// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The side view's seat layer and the subdued airframe: the seat render
//! registers with the airframe render and the overlay at every framing, the
//! wings, nacelles and tail are pushed back, and (with `ALAS_R22_PNG_DIR`
//! set) the composite is written out for a look.

use alas_report::scene::{Color, Fill, SceneElement, Stroke};
use egui::{pos2, vec2, Rect};

use super::canvas::{Frame, Window};
use super::front_render::{build_layer_scene, Layer, Placement, Plan, Projection};
use super::seat_layer;
use super::side_view::SideView;
use crate::state::AppState;

const THEME: &str = "dark-accessible";

fn preset_view() -> SideView {
    let mut state = AppState::default();
    state.load_preset("A340-300");
    let config = state.typed_config_arc().expect("typed config");
    super::side_view::build(&state, &config).expect("side view")
}

fn bounds(scene: &alas_report::scene::Scene) -> (f64, f64, f64, f64) {
    let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for element in &scene.elements {
        if let SceneElement::Polygon { points, .. } = element {
            for p in points {
                x0 = x0.min(p[0]);
                x1 = x1.max(p[0]);
                y0 = y0.min(p[1]);
                y1 = y1.max(p[1]);
            }
        }
    }
    (x0, x1, y0, y1)
}

/// A seat row drawn by the seat layer lands where the overlay and the airframe
/// render put the same metres.
#[test]
fn the_seat_layer_registers_with_the_airframe_and_the_overlay() {
    let mut view = preset_view();
    let airframe = view.airframe.clone().expect("airframe");
    let rows = seat_layer::profiles(&view);
    assert!(rows.len() > 10, "a cabin of seat rows: {}", rows.len());
    let target = rows[rows.len() / 2];
    // One row only, so its bounds are the seat's.
    view.items.retain(|item| {
        item.seat.is_some() && (item.x - target.x).abs() < 1.0e-9 && item.declared.is_empty()
    });
    view.items.truncate(1);
    let rect = Rect::from_min_size(pos2(40.0, 70.0), vec2(900.0, 460.0));
    for (ppp, scale) in [(1.0_f32, 14.0_f32), (1.25, 33.5), (2.0, 90.0)] {
        let placement = Placement {
            origin: pos2(490.0 - 20.0 * (target.x as f32 - 20.0).signum(), 300.0),
            scale,
        };
        // Put the row near the middle of the visible window.
        let placement = Placement {
            origin: pos2(
                rect.center().x - target.x as f32 * scale,
                rect.center().y + target.floor_z as f32 * scale,
            ),
            ..placement
        };
        let plan = Plan::for_view(rect, placement, ppp);
        let seats = build_layer_scene(
            Layer::Seats,
            Projection::Side,
            &view,
            &airframe,
            THEME,
            plan,
        )
        .expect("seat scene");
        let (_, framing) = crate::sandbox::scene::build_framed_background_side_view(
            airframe.geometry.clone(),
            airframe.design,
            THEME,
            plan.viewport_px,
            plan.centre,
            plan.ppm,
        )
        .expect("airframe");
        assert!(scene_matches(&seats, plan), "one canvas for both layers");
        let (x0, _, _, y1) = bounds(&seats);
        let cushion = (target.length * 0.54).max(0.22);
        let want_x = framing.project([target.x - cushion * 0.5, 0.0, target.floor_z]);
        assert!(
            (x0 - want_x[0]).abs() < 0.01,
            "ppp {ppp}: {x0} vs {want_x:?}"
        );
        assert!(
            (y1 - want_x[1]).abs() < 0.01,
            "ppp {ppp}: {y1} vs {want_x:?}"
        );
        let texture = plan.screen_rect(placement);
        let overlay = placement.to_screen(target.x - cushion * 0.5, target.floor_z);
        let from_render = pos2(
            texture.min.x + x0 as f32 / ppp,
            texture.min.y + y1 as f32 / ppp,
        );
        assert!(
            (overlay - from_render).length() < 0.01,
            "ppp {ppp}: overlay {overlay:?} vs seat render {from_render:?}"
        );
    }
}

fn scene_matches(scene: &alas_report::scene::Scene, plan: Plan) -> bool {
    let (width, height) = plan.canvas_px();
    (scene.width - width).abs() < 1.0e-9 && (scene.height - height).abs() < 1.0e-9
}

/// The seat rows are told apart (a seat is four shapes) while the pitch is
/// wide enough, and are class bands when it is not.
#[test]
fn rows_are_seats_when_wide_enough_and_class_bands_when_not() {
    let view = preset_view();
    let airframe = view.airframe.clone().expect("airframe");
    let rows = seat_layer::profiles(&view);
    let polygons = |scale: f64| {
        let plan = Plan {
            centre: (rows[0].x, rows[0].floor_z),
            ppm: scale,
            viewport_px: (600.0, 300.0),
        };
        build_layer_scene(
            Layer::Seats,
            Projection::Side,
            &view,
            &airframe,
            THEME,
            plan,
        )
        .expect("scene")
        .elements
        .iter()
        .filter(|e| matches!(e, SceneElement::Polygon { .. }))
        .count()
    };
    let (seats, bands) = (polygons(30.0), polygons(2.0));
    assert!(seats >= 4 * 5, "seats at close range: {seats}");
    assert!(
        bands < 12,
        "a few class bands at whole-aircraft range: {bands}"
    );
}

/// Mean saturation (max - min channel) of the polygons of a scene.
fn mean_chroma(scene: &alas_report::scene::Scene) -> f64 {
    let fills: Vec<Color> = scene
        .elements
        .iter()
        .filter_map(|e| match e {
            SceneElement::Polygon {
                fill: Some(Fill { color, .. }),
                ..
            } => Some(*color),
            _ => None,
        })
        .collect();
    fills
        .iter()
        .map(|c| f64::from(c.r.max(c.g).max(c.b) - c.r.min(c.g).min(c.b)))
        .sum::<f64>()
        / fills.len().max(1) as f64
}

#[test]
fn the_airframe_render_is_subdued_but_keeps_its_shape() {
    let view = preset_view();
    let airframe = view.airframe.clone().expect("airframe");
    let args = (
        airframe.geometry.clone(),
        airframe.design,
        THEME,
        (900.0, 400.0),
        (30.0, 0.0),
        12.0,
    );
    let (plain, _) = crate::sandbox::scene::build_framed_side_view(
        args.0.clone(),
        args.1,
        args.2,
        args.3,
        args.4,
        args.5,
    )
    .expect("plain");
    let (quiet, _) = crate::sandbox::scene::build_framed_background_side_view(
        args.0, args.1, args.2, args.3, args.4, args.5,
    )
    .expect("quiet");
    assert_eq!(plain.elements.len(), quiet.elements.len());
    assert!(
        mean_chroma(&quiet) < 0.5 * mean_chroma(&plain),
        "{} vs {}",
        mean_chroma(&quiet),
        mean_chroma(&plain)
    );
}

/// The composite of the layers this change draws (airframe, cutaway body,
/// floor, seats) for a look; written only when `ALAS_R22_PNG_DIR` is set.
#[test]
// reason: tells where the images went.
#[allow(clippy::print_stdout)]
fn the_composite_is_written_for_inspection() {
    let Some(dir) = std::env::var_os("ALAS_R22_PNG_DIR") else {
        return;
    };
    let view = preset_view();
    let airframe = view.airframe.clone().expect("airframe");
    let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(1500.0, 330.0));
    let cabin_mid = f64::midpoint(view.cabin_x[0], view.cabin_x[1]);
    let mut zoomed = Window::default();
    let whole = Frame::windowed(rect, &view, Window::default());
    zoomed.zoom_about(&view, rect, whole.to_screen(cabin_mid, 0.0), 7.0);
    for (name, window) in [("whole", Window::default()), ("zoom", zoomed)] {
        let frame = Frame::windowed(rect, &view, window);
        let scale = frame.sx;
        let origin = frame.to_screen(0.0, 0.0);
        let centre = (
            f64::from((rect.center().x - origin.x) / scale),
            f64::from((origin.y - rect.center().y) / scale),
        );
        let plan = Plan {
            centre,
            ppm: f64::from(scale),
            viewport_px: (f64::from(rect.width()), f64::from(rect.height())),
        };
        let (mut scene, framing) = crate::sandbox::scene::build_framed_background_side_view(
            airframe.geometry.clone(),
            airframe.design,
            THEME,
            plan.viewport_px,
            plan.centre,
            plan.ppm,
        )
        .expect("airframe");
        let project = |x: f64, z: f64| framing.project([x, 0.0, z]);
        let mut body: Vec<[f64; 2]> = view
            .stations
            .iter()
            .zip(&view.top)
            .map(|(&x, &z)| project(x, z))
            .collect();
        body.extend(
            view.stations
                .iter()
                .zip(&view.bottom)
                .rev()
                .map(|(&x, &z)| project(x, z)),
        );
        scene.add(SceneElement::Polygon {
            points: body,
            fill: Some(Fill::new(Color::rgba(14, 14, 16, 205))),
            stroke: Some(Stroke::new(Color::rgba(190, 190, 190, 255), 1.0)),
        });
        let [from, to] = view.cabin_x;
        scene.add(SceneElement::Line {
            p1: project(from, view.floor_z(from)),
            p2: project(to, view.floor_z(to)),
            stroke: Stroke::new(Color::rgba(230, 230, 230, 255), 1.5),
        });
        let seats = build_layer_scene(
            Layer::Seats,
            Projection::Side,
            &view,
            &airframe,
            THEME,
            plan,
        )
        .expect("seats");
        scene.elements.extend(seats.elements);
        let png = alas_viz::raster::render_scene_png(&scene).expect("raster");
        let path = std::path::Path::new(&dir).join(format!("r22-side-{name}.png"));
        std::fs::write(&path, png).expect("write png");
        println!("wrote {}", path.display());
    }
}
