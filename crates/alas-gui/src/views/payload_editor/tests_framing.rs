// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The default and Fit framing of the side view show the whole airframe (body, fin, engines) at
//! true aspect inside the plot at every page size, the scale labels stay inside the plot, and the
//! front view's canvas follows the page's height.

use super::*;
use crate::test_raster::{render, write, Shot};

/// Content areas of the review sizes with the 3-D preview dock open, points: A 1600x1000,
/// B 2400x1400, D 1100x1400 and E 1100x760 windows less the rail and the dock.
const SIZES: [(&str, f32, f32); 4] = [
    ("A", 1_090.0, 890.0),
    ("B", 1_890.0, 1_290.0),
    ("D", 640.0, 1_290.0),
    ("E", 560.0, 650.0),
];

/// The page in `mode`: the shot, the canvas rectangle and the editor's window as drawn.
fn drawn(
    mode: ViewMode,
    fuselage: bool,
    width: f32,
    height: f32,
) -> (Shot, egui::Rect, canvas::Window) {
    let mut state = AppState::default();
    let mut first = true;
    let mut seen = (egui::Rect::NOTHING, canvas::Window::default());
    let shot = render(width, height, 14, 160, |ui| {
        if first {
            let mut editor: EditorUi = ui
                .ctx()
                .data_mut(|data| data.get_temp(ui_id()))
                .unwrap_or_default();
            editor.view_mode = mode;
            editor.front = front_camera::Camera {
                fuselage,
                ..Default::default()
            };
            ui.ctx().data_mut(|data| data.insert_temp(ui_id(), editor));
            first = false;
        }
        egui::ScrollArea::vertical().show(ui, |ui| show(&mut state, ui));
        let rect = ui
            .ctx()
            .data(|data| data.get_temp(canvas::canvas_rect_id()))
            .unwrap_or(egui::Rect::NOTHING);
        let editor: EditorUi = ui
            .ctx()
            .data(|data| data.get_temp(ui_id()))
            .unwrap_or_default();
        seen = (rect, editor.window);
    });
    (shot, seen.0, seen.1)
}

fn whole_airframe_is_inside(rect: egui::Rect, view: &SideView, window: canvas::Window) -> String {
    let frame = canvas::Frame::windowed(rect, view, window);
    let [x0, x1] = view.frame_x_range();
    let [z0, z1] = view.render_z_range();
    let mut report = String::new();
    for (x, z) in [(x0, z0), (x1, z0), (x0, z1), (x1, z1)] {
        let at = frame.to_screen(x, z);
        if !rect.shrink(4.0).contains(at) {
            report += &format!("({x:.1}, {z:.1}) -> {at:?} outside {rect:?}; ");
        }
    }
    report
}

#[test]
fn the_default_and_fit_framing_show_the_whole_airframe_at_every_size() {
    let state = AppState::default();
    let config = state.typed_config().expect("config");
    let view = side_view::build(&state, &config).expect("view");
    assert!(view.airframe.is_some() && !view.surfaces.is_empty());
    for (name, width, height) in SIZES {
        let (shot, rect, window) = drawn(ViewMode::Side, false, width, height);
        write(&format!("r35-payload-side-{name}.png"), &shot);
        assert!(rect.width() > 100.0, "{name}: the canvas was drawn");
        assert_eq!(window.zoom, 1.0, "{name}: no cabin zoom by default");
        let problems = whole_airframe_is_inside(rect, &view, window);
        assert!(problems.is_empty(), "{name} default: {problems}");
        // Fit is the default window again, whatever the user did before.
        let problems = whole_airframe_is_inside(rect, &view, canvas::Window::default());
        assert!(problems.is_empty(), "{name} fit: {problems}");
        // One scale on both axes.
        let frame = canvas::Frame::windowed(rect, &view, window);
        let per_x = frame.to_screen(11.0, 0.0).x - frame.to_screen(10.0, 0.0).x;
        let per_z = frame.to_screen(10.0, 0.0).y - frame.to_screen(10.0, 1.0).y;
        assert!((per_x - per_z).abs() < 1e-3, "{name}: {per_x} vs {per_z}");
        // The scale labels keep clear of the plot's edges.
        for (text, text_rect) in &shot.texts {
            let is_tick = text.parse::<u32>().is_ok()
                && text_rect.center().y > rect.bottom() - 24.0
                && text_rect.center().y < rect.bottom();
            if is_tick {
                assert!(
                    text_rect.left() >= rect.left() + 4.0
                        && text_rect.right() <= rect.right() - 4.0,
                    "{name}: label {text} {text_rect:?} touches the plot {rect:?}"
                );
            }
        }
    }
}

#[test]
fn the_front_canvas_follows_the_page_height_and_the_fuselage_fills_it() {
    let mut heights = Vec::new();
    for (name, width, height) in SIZES {
        for fuselage in [false, true] {
            let (shot, rect, _) = drawn(ViewMode::Front, fuselage, width, height);
            write(
                &format!(
                    "r35-payload-front{}-{name}.png",
                    if fuselage { "-fuselage" } else { "" }
                ),
                &shot,
            );
            if !fuselage {
                heights.push((name, rect.height()));
                assert!(
                    rect.height() > 0.5 * height - 120.0,
                    "{name}: the canvas ({} pt) uses the page's room ({height} pt)",
                    rect.height()
                );
            }
        }
    }
    // A taller page gives a taller canvas.
    let by = |name: &str| {
        heights
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, h)| *h)
            .unwrap()
    };
    assert!(by("B") > by("A") || by("A") >= 880.0 - 1.0, "{heights:?}");
}

#[test]
fn the_fuselage_framing_centres_the_section_and_fills_85_percent_of_the_canvas_height() {
    let state = AppState::default();
    let config = state.typed_config().expect("config");
    let view = side_view::build(&state, &config).expect("view");
    for (name, width, height) in SIZES {
        let (_, rect, _) = drawn(ViewMode::Front, true, width, height);
        let x = f64::midpoint(view.cabin_x[0], view.cabin_x[1]);
        let camera = front_camera::Camera::fuselage(rect, &view, x);
        let frame = front_camera::Frame::new(rect, &view, x, camera);
        let (low, high) = (view.at(&view.bottom, x), view.at(&view.top, x));
        let (a, b) = (frame.to_screen(0.0, high), frame.to_screen(0.0, low));
        let fill = f64::from((b.y - a.y) / rect.height());
        let middle = f32::midpoint(a.y, b.y) - rect.center().y;
        assert!(
            middle.abs() < 1.0,
            "{name}: section centred ({middle} pt off)"
        );
        assert!(a.x - rect.center().x == 0.0, "{name}: centred across");
        assert!(
            fill <= front_camera::FUSELAGE_FILL + 1e-3,
            "{name}: fills {fill}"
        );
        // Height-limited canvases (wider than the section needs) fill 85 %.
        if width >= 1_000.0 {
            assert!((fill - 0.85).abs() < 0.01, "{name}: fills {fill}");
        }
    }
}
