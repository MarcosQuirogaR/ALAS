// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Sandbox framing and overlay layout: the drawn scale must not change
//! with the camera orientation or with a geometry edit, the theme must
//! reach the live preview, and the floating action block, category stack
//! and Summary card must sit where the layout promises on wide and narrow
//! viewports without taking the orbit gesture.

mod common;

use alas_gui::sandbox::overlays::{action_block_rect, metric_chips};
use alas_gui::sandbox::scene::SANDBOX_CAMERA_ID;
use alas_gui::sandbox::viewport::{
    overlay_rect_tagged, overlay_rects, overlay_rects_tagged, pointer_over_overlay, viewport_rect,
};
use alas_gui::state::{AppState, PreviewCamera};
use alas_gui::theme::AppTheme;
use alas_report::scene::Color;
use common::{click_on, drag_on, frame_on, rasterize_frame, write_png};
use egui::{pos2, Context, Rect};

const WIDE: (f32, f32) = (1280.0, 820.0);
const NARROW: (f32, f32) = (520.0, 900.0);
const SHORT: (f32, f32) = (900.0, 420.0);

fn sandbox() -> AppState {
    let mut state = AppState::default();
    assert!(state.enter_sandbox(true), "sandbox opens from AVE");
    state.set_sandbox_viewport_size((920.0, 730.0));
    state
}

fn framing(state: &AppState) -> alas_report::families::geometry::SceneFraming {
    state.sandbox.scene.as_ref().expect("sandbox scene").1
}

#[test]
fn orbit_presets_and_zoom_keep_the_pixels_per_metre_and_the_pivot() {
    let mut state = sandbox();
    let reference = framing(&state).reference();
    let scale = framing(&state).scale();
    assert!(scale.is_finite() && scale > 0.0);
    let mut cameras = vec![
        PreviewCamera::top(),
        PreviewCamera::front(),
        PreviewCamera::side(),
        PreviewCamera::isometric(),
    ];
    for azim in (-180..180).step_by(45) {
        for elev in [-70.0, -20.0, 10.0, 55.0] {
            cameras.push(PreviewCamera {
                pitch_deg: elev,
                yaw_deg: f64::from(azim),
                zoom: 1.0,
            });
        }
    }
    for camera in cameras {
        *state.preview_camera_mut(SANDBOX_CAMERA_ID) = camera;
        state.reproject_sandbox_scene();
        let f = framing(&state);
        assert_eq!(f.reference(), reference, "framing kept for {camera:?}");
        assert!(
            (f.scale() - scale).abs() < 1e-12,
            "scale kept for {camera:?}"
        );
    }
    // A drag orbit through the state's own path keeps it too.
    state
        .preview_camera_mut(SANDBOX_CAMERA_ID)
        .apply_orbit_motion(egui::vec2(140.0, -60.0));
    state.reproject_sandbox_scene();
    assert_eq!(framing(&state).reference(), reference);
    // Zoom scales uniformly and is the only thing that does.
    state
        .preview_camera_mut(SANDBOX_CAMERA_ID)
        .apply_zoom_factor(2.0);
    state.reproject_sandbox_scene();
    assert_eq!(framing(&state).reference(), reference);
    assert!((framing(&state).scale() - 2.0 * scale).abs() < 1e-9);
}

#[test]
fn a_span_edit_changes_the_drawn_size_and_only_fit_or_focus_reframe() {
    let mut state = sandbox();
    *state.preview_camera_mut(SANDBOX_CAMERA_ID) = PreviewCamera::top();
    state.reproject_sandbox_scene();
    let reference = framing(&state).reference();
    let tip_before = wing_tip_screen_y(&state);
    let span = state.design_values["span_m"];
    state.design_values.insert("span_m".to_owned(), span + 10.0);
    state.on_sandbox_model_changed();
    let f = framing(&state);
    assert_eq!(f.reference(), reference, "an edit never refits");
    let tip_after = wing_tip_screen_y(&state);
    let expected = 5.0 * f.scale();
    assert!(
        ((tip_after - tip_before) - expected).abs() < 0.05 * expected,
        "the right tip moved {} canvas units, expected about {expected}",
        tip_after - tip_before
    );
    // Explicit Fit encloses the wider aircraft.
    state.refit_sandbox_framing();
    assert!(framing(&state).reference().extent > reference.extent);
    // Focusing a component reframes to that component; the overview
    // reframes back to the whole aircraft.
    let whole = framing(&state).reference();
    state.set_sandbox_focus(Some(alas_gui::sandbox::fields::Discipline::Wing));
    let wing = framing(&state).reference();
    assert!(wing.extent < whole.extent);
    state.set_sandbox_focus(None);
    assert_eq!(framing(&state).reference(), whole);
    // A resized viewport keeps the reference in metres; the canvas follows.
    state.set_sandbox_viewport_size((1400.0, 600.0));
    let resized = framing(&state);
    assert_eq!(resized.reference(), whole);
    assert_eq!(resized.canvas, (1400.0, 600.0));
    let (_, _, vw, vh) = resized.viewport;
    assert!((resized.scale() - vw.min(vh) * 0.45 / whole.extent).abs() < 1e-9);
}

/// The largest screen y of a main-wing vertex (the right tip in the top
/// view maps model y to screen y).
fn wing_tip_screen_y(state: &AppState) -> f64 {
    let model = state.sandbox.model.as_ref().expect("model");
    let f = framing(state);
    model
        .faces()
        .iter()
        .filter(|face| face.component == alas_report::families::geometry::SceneComponent::Wing)
        .flat_map(|face| face.points.iter())
        .map(|&p| f.project(p)[1])
        .fold(f64::NEG_INFINITY, f64::max)
}

fn settled(theme: AppTheme, size: (f32, f32)) -> (Context, AppState) {
    let mut state = AppState::default();
    state.theme = theme;
    assert!(state.enter_sandbox(true));
    let ctx = Context::default();
    alas_gui::apply_theme(theme, &ctx);
    // Rows centre on the width they measured the frame before.
    for _ in 0..4 {
        frame_on(&ctx, &mut state, size, vec![]);
    }
    (ctx, state)
}

fn pixel(rgba: &[u8], width: u32, x: f32, y: f32) -> [u8; 3] {
    let i = ((y as u32) * width + x as u32) as usize * 4;
    [rgba[i], rgba[i + 1], rgba[i + 2]]
}

#[test]
fn the_live_preview_background_and_colours_follow_the_theme() {
    for theme in [AppTheme::Light, AppTheme::Grey, AppTheme::Dark] {
        let (ctx, mut state) = settled(theme, WIDE);
        let palette = theme.palette();
        let background = Color::from_hex(palette.bg);
        let (scene, _) = state.sandbox.scene.as_ref().expect("scene");
        assert_eq!(
            scene.background,
            Some(background),
            "{theme:?} scene background"
        );
        let output = frame_on(&ctx, &mut state, WIDE, vec![]);
        let (width, _, rgba) = rasterize_frame(&ctx, output, WIDE, ctx.style().visuals.panel_fill);
        let viewport = viewport_rect(&ctx).expect("viewport");
        // The corners of the design space are clear of the aircraft and of
        // every overlay; they show the scene's own background paint.
        for corner in [
            pos2(viewport.right() - 24.0, viewport.top() + 60.0),
            pos2(viewport.left() + 24.0, viewport.top() + 60.0),
        ] {
            assert!(!pointer_over_overlay(&overlay_rects(&ctx), Some(corner)));
            let [r, g, b] = pixel(&rgba, width, corner.x, corner.y);
            assert!(
                r.abs_diff(background.r) <= 2
                    && g.abs_diff(background.g) <= 2
                    && b.abs_diff(background.b) <= 2,
                "{theme:?}: rendered background {r},{g},{b} at {corner:?}, palette {background:?}"
            );
        }
        // Outline and wing read clearly on the background; the fuselage
        // skin is a lit surface bounded by outlines, so it may be lighter.
        let outline = Color::from_hex(palette.spine);
        assert!(
            outline.contrast_against(background) >= 3.0,
            "{theme:?} outline"
        );
        let wing = alas_report::families::geometry::SceneComponent::Wing.color();
        // The wing blue sits at 2.3:1 on the grey canvas; its edges carry the
        // 3:1 outline, which is what separates it from the background.
        assert!(wing.contrast_against(background) >= 2.0, "{theme:?} wing");
        let fuselage = alas_report::families::geometry::SceneComponent::Fuselage.color();
        assert!(
            fuselage.contrast_against(background) >= 1.4,
            "{theme:?} fuselage"
        );
    }
}

fn rows_of(rects: &[Rect]) -> Vec<Vec<Rect>> {
    let mut rows: Vec<Vec<Rect>> = Vec::new();
    for rect in rects {
        match rows
            .iter_mut()
            .find(|row| (row[0].center().y - rect.center().y).abs() < 1.0)
        {
            Some(row) => row.push(*rect),
            None => rows.push(vec![*rect]),
        }
    }
    rows.sort_by(|a, b| a[0].center().y.total_cmp(&b[0].center().y));
    rows
}

fn span(rects: &[Rect]) -> (f32, f32) {
    let left = rects.iter().map(|r| r.left()).fold(f32::INFINITY, f32::min);
    let right = rects
        .iter()
        .map(|r| r.right())
        .fold(f32::NEG_INFINITY, f32::max);
    (left, right)
}

#[test]
fn actions_float_centred_at_the_bottom_and_the_stack_with_summary_is_centred_on_the_left() {
    for size in [WIDE, NARROW, SHORT] {
        let (ctx, _state) = settled(AppTheme::Dark, size);
        let viewport = viewport_rect(&ctx).expect("viewport");
        let block = action_block_rect(&ctx, viewport);
        assert!((block.bottom() - (viewport.bottom() - 8.0)).abs() < 1e-3);
        let actions = overlay_rects_tagged(&ctx, "action");
        assert!(actions.len() >= 5, "{size:?}: {actions:?}");
        // No metric rows: the metrics are behind the Summary button.
        assert!(overlay_rects_tagged(&ctx, "metric").is_empty(), "{size:?}");
        for rect in &actions {
            assert!(
                block.expand(1.0).contains_rect(*rect),
                "{size:?}: {rect:?} outside {block:?}"
            );
        }
        // One action row (a launch row over an edit row when narrow), each
        // centred on the viewport like the camera row.
        let action_rows = rows_of(&actions);
        assert_eq!(
            action_rows.len(),
            alas_gui::sandbox::overlays::action_rows(viewport.width()),
            "{size:?}"
        );
        for row in &action_rows {
            let (left, right) = span(row);
            assert!(
                ((left + right) * 0.5 - viewport.center().x).abs() < 2.0,
                "{size:?}"
            );
        }
        // The category stack: on the left, centred about the viewport's
        // horizontal centreline, clear of the camera row and the block.
        let stack = overlay_rect_tagged(&ctx, "stack").expect("stack");
        let camera = overlay_rects_tagged(&ctx, "camera");
        let camera_bottom = camera
            .iter()
            .map(|r| r.bottom())
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(stack.left() < viewport.left() + 40.0, "{size:?}");
        assert!(
            stack.top() >= camera_bottom,
            "{size:?}: {stack:?} vs camera {camera_bottom}"
        );
        assert!(
            stack.bottom() <= block.top(),
            "{size:?}: {stack:?} vs block {block:?}"
        );
        let fits = viewport.height() > stack.height() + 2.0 * (block.height() + 60.0);
        if fits {
            assert!(
                (stack.center().y - viewport.center().y).abs() < 1.5,
                "{size:?}: {stack:?} in {viewport:?}"
            );
        }
        let search = overlay_rect_tagged(&ctx, "search").expect("search");
        assert!(stack.contains_rect(search), "{size:?}");
        // The Summary button is the last of the stack, below Propulsion, and
        // its card is closed until pressed.
        let summary = overlay_rect_tagged(&ctx, "summary").expect("summary");
        let propulsion = overlay_rect_tagged(&ctx, "category:propulsion").expect("propulsion");
        assert!(stack.contains_rect(summary), "{size:?}");
        assert!(summary.top() >= propulsion.bottom(), "{size:?}");
        assert!(
            overlay_rect_tagged(&ctx, "summary_card").is_none(),
            "{size:?}"
        );
    }
}

#[test]
fn gestures_on_the_action_boxes_never_orbit_and_actions_still_act() {
    let (ctx, mut state) = settled(AppTheme::Dark, WIDE);
    let actions = overlay_rects_tagged(&ctx, "action");
    for rect in [actions[0], actions[2]] {
        let before = *state.preview_camera_mut(SANDBOX_CAMERA_ID);
        drag_on(&ctx, &mut state, WIDE, rect.center());
        assert_eq!(
            before,
            *state.preview_camera_mut(SANDBOX_CAMERA_ID),
            "{rect:?}"
        );
        assert!(state.sandbox.drag.is_none());
    }
    // Run log is the fifth action while nothing runs; its click toggles
    // the log window, and a disabled Undo does nothing.
    let run_log = actions[4];
    let open = state.sandbox.layout.log_window_open;
    click_on(&ctx, &mut state, WIDE, run_log.center());
    assert_ne!(state.sandbox.layout.log_window_open, open);
    assert!(!state.sandbox.undo.can_undo());
    let revision = state.sandbox.revision;
    click_on(&ctx, &mut state, WIDE, actions[2].center());
    assert_eq!(state.sandbox.revision, revision);
    // Closing the log again leaves the design space beside the block
    // empty, and a drag there still orbits.
    click_on(&ctx, &mut state, WIDE, run_log.center());
    assert_eq!(state.sandbox.layout.log_window_open, open);
    // The empty design space beside the block still orbits.
    let viewport = viewport_rect(&ctx).expect("viewport");
    let empty = pos2(viewport.right() - 40.0, viewport.center().y);
    assert!(!pointer_over_overlay(&overlay_rects(&ctx), Some(empty)));
    let before = *state.preview_camera_mut(SANDBOX_CAMERA_ID);
    drag_on(&ctx, &mut state, WIDE, empty);
    assert_ne!(
        before.yaw_deg,
        state.preview_camera_mut(SANDBOX_CAMERA_ID).yaw_deg
    );
}

#[test]
fn the_summary_button_toggles_the_metric_card_beside_the_stack_without_orbiting() {
    let (ctx, mut state) = settled(AppTheme::Dark, WIDE);
    assert!(!state.sandbox.layout.summary_open);
    let summary = overlay_rect_tagged(&ctx, "summary").expect("summary");
    click_on(&ctx, &mut state, WIDE, summary.center());
    assert!(state.sandbox.layout.summary_open);
    frame_on(&ctx, &mut state, WIDE, vec![]);
    let viewport = viewport_rect(&ctx).expect("viewport");
    let stack = overlay_rect_tagged(&ctx, "stack").expect("stack");
    let card = overlay_rect_tagged(&ctx, "summary_card").expect("card");
    let metrics = overlay_rects_tagged(&ctx, "metric");
    assert_eq!(metrics.len(), 8);
    assert!(card.left() >= stack.right(), "{card:?} vs {stack:?}");
    assert!(viewport.contains_rect(card), "{card:?} in {viewport:?}");
    let block = action_block_rect(&ctx, viewport);
    assert!(card.bottom() <= block.top() + 1.0, "{card:?} vs {block:?}");
    for rect in &metrics {
        assert!(card.contains_rect(*rect), "{rect:?} in {card:?}");
    }
    let texts = metric_chips(&state);
    assert_eq!(texts.len(), 8);
    assert!(texts[0].starts_with("S_ref ") && texts[2].starts_with("MAC "));
    // A drag on a metric row never orbits.
    let before = *state.preview_camera_mut(SANDBOX_CAMERA_ID);
    drag_on(&ctx, &mut state, WIDE, metrics[3].center());
    assert_eq!(before, *state.preview_camera_mut(SANDBOX_CAMERA_ID));
    // Pressing Summary again hides the card and its rows.
    click_on(&ctx, &mut state, WIDE, summary.center());
    assert!(!state.sandbox.layout.summary_open);
    frame_on(&ctx, &mut state, WIDE, vec![]);
    assert!(overlay_rect_tagged(&ctx, "summary_card").is_none());
    assert!(overlay_rects_tagged(&ctx, "metric").is_empty());
}

/// Writes headless workspace renders for the three themes in English and
/// Spanish on a wide and a narrow viewport, and the four camera presets
/// plus two orbit views, to an internal evidence directory
/// (2026-09-14); run with
/// `--ignored`. Glyphs render as coverage blocks (no font texture in the
/// rasterizer), so these show layout, colours and states, not legible text.
#[test]
#[ignore = "writes evidence images"]
fn write_workspace_evidence_images() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../out/evidence/sandbox-depth-layout-scale-2026-09-14");
    std::fs::create_dir_all(&dir).expect("evidence directory");
    alas_i18n::es::install();
    for (lang, code) in [
        (alas_gui::state::Language::En, "en"),
        (alas_gui::state::Language::Es, "es"),
    ] {
        alas_i18n::set_language(Some(code));
        for theme in [AppTheme::Light, AppTheme::Grey, AppTheme::Dark] {
            for (size_name, size) in [("wide", WIDE), ("narrow", NARROW)] {
                let (ctx, mut state) = settled(theme, size);
                state.language = lang;
                frame_on(&ctx, &mut state, size, vec![]);
                let output = frame_on(&ctx, &mut state, size, vec![]);
                let (w, h, rgba) =
                    rasterize_frame(&ctx, output, size, ctx.style().visuals.panel_fill);
                let name = format!(
                    "workspace-{}-{code}-{size_name}.png",
                    theme.name().to_ascii_lowercase()
                );
                write_png(&dir.join(name), w, h, &rgba);
            }
        }
    }
    alas_i18n::set_language(Some("en"));
    let views = [
        ("iso", PreviewCamera::isometric()),
        ("top", PreviewCamera::top()),
        ("front", PreviewCamera::front()),
        ("side", PreviewCamera::side()),
        (
            "orbit-a",
            PreviewCamera {
                pitch_deg: 35.0,
                yaw_deg: -60.0,
                zoom: 1.0,
            },
        ),
        (
            "orbit-b",
            PreviewCamera {
                pitch_deg: -25.0,
                yaw_deg: 140.0,
                zoom: 1.0,
            },
        ),
    ];
    for (name, camera) in views {
        let (ctx, mut state) = settled(AppTheme::Dark, WIDE);
        *state.preview_camera_mut(SANDBOX_CAMERA_ID) = camera;
        state.reproject_sandbox_scene();
        frame_on(&ctx, &mut state, WIDE, vec![]);
        let output = frame_on(&ctx, &mut state, WIDE, vec![]);
        let (w, h, rgba) = rasterize_frame(&ctx, output, WIDE, ctx.style().visuals.panel_fill);
        write_png(
            &dir.join(format!("workspace-camera-{name}.png")),
            w,
            h,
            &rgba,
        );
    }
    // The same aircraft after a span edit at the kept scale, top view.
    let (ctx, mut state) = settled(AppTheme::Dark, WIDE);
    *state.preview_camera_mut(SANDBOX_CAMERA_ID) = PreviewCamera::top();
    state.reproject_sandbox_scene();
    for (name, extra) in [("span-base", 0.0), ("span-plus-12m", 12.0)] {
        let span = AppState::default().design_values["span_m"];
        state
            .design_values
            .insert("span_m".to_owned(), span + extra);
        state.on_sandbox_model_changed();
        frame_on(&ctx, &mut state, WIDE, vec![]);
        let output = frame_on(&ctx, &mut state, WIDE, vec![]);
        let (w, h, rgba) = rasterize_frame(&ctx, output, WIDE, ctx.style().visuals.panel_fill);
        write_png(&dir.join(format!("workspace-{name}.png")), w, h, &rgba);
    }
}
