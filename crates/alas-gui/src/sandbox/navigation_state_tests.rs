// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Sandbox navigation on a built aircraft: preset directions, the orbit pivot
//! and the pivot pick, against the real AVE model.

use super::super::scene::{build_sandbox_airplane, SANDBOX_CAMERA_ID};
use crate::state::{AppState, PreviewCamera};
use alas_report::scene::Camera3D;

/// Unit vector from the model toward the camera, in aircraft axes
/// (x aft from the nose, y starboard, z up).
fn toward_camera(camera: PreviewCamera) -> [f64; 3] {
    let camera: Camera3D = camera.into();
    let (elev, azim) = (camera.elev_deg.to_radians(), camera.azim_deg.to_radians());
    [azim.sin() * elev.cos(), azim.cos() * elev.cos(), elev.sin()]
}

fn close(value: [f64; 3], expected: [f64; 3]) -> bool {
    value
        .iter()
        .zip(expected)
        .all(|(a, b)| (a - b).abs() < 1e-9)
}

/// Every preset looks from the side its name says. The nose is at the lowest
/// x of the built fuselage, so Front (camera ahead of the nose, looking aft)
/// has the camera toward -x and Rear toward +x.
#[test]
fn every_preset_looks_from_the_side_its_name_says() {
    let (plane, _) = build_sandbox_airplane(&AppState::default()).expect("AVE builds");
    let fuselage = plane.fuselages.first().expect("fuselage");
    let nose = fuselage.xsecs.first().expect("nose").xyz_c[0];
    let tail = fuselage.xsecs.last().expect("tail").xyz_c[0];
    assert!(
        nose < tail,
        "frame: x grows aft from the nose ({nose} .. {tail})"
    );

    assert!(close(
        toward_camera(PreviewCamera::front()),
        [-1.0, 0.0, 0.0]
    ));
    assert!(close(toward_camera(PreviewCamera::rear()), [1.0, 0.0, 0.0]));
    assert!(close(toward_camera(PreviewCamera::top()), [0.0, 0.0, 1.0]));
    assert!(close(toward_camera(PreviewCamera::side()), [0.0, 1.0, 0.0]));
    let iso = toward_camera(PreviewCamera::isometric());
    assert!(
        iso[0] < 0.0 && iso[2] > 0.0,
        "the isometric camera is ahead of the nose and above: {iso:?}"
    );
}

/// The Front view's nearest surface is the nose; the Rear view's is the tail.
#[test]
fn the_front_view_sees_the_nose_and_the_rear_view_the_tail() {
    let mut state = AppState::default();
    assert!(state.enter_sandbox(true));
    state.set_sandbox_viewport_size((1400.0, 800.0));
    let (plane, _) = build_sandbox_airplane(&state).expect("AVE builds");
    let fuselage = plane.fuselages.first().expect("fuselage");
    let (nose, tail) = (
        fuselage.xsecs.first().expect("nose").xyz_c,
        fuselage.xsecs.last().expect("tail").xyz_c,
    );
    for (camera, expected_x) in [
        (PreviewCamera::front(), nose[0]),
        (PreviewCamera::rear(), tail[0]),
    ] {
        *state.preview_camera_mut(SANDBOX_CAMERA_ID) = camera;
        state.refit_sandbox_framing();
        let (_, framing) = state.sandbox.scene.clone().expect("scene");
        let model = state.sandbox.model.as_ref().expect("model");
        // Through the middle of the fuselage on screen.
        let centre = framing.project([expected_x, nose[1], nose[2]]);
        let hit =
            super::super::navigation::pick_surface_point(model.faces(), None, &framing, centre)
                .expect("a surface under the fuselage axis");
        // The hit is on the near end of the fuselage, within a few metres.
        assert!(
            (hit[0] - expected_x).abs() < 0.25 * (tail[0] - nose[0]),
            "{camera:?}: the nearest surface is at x = {:.1}, expected near {expected_x:.1}",
            hit[0]
        );
    }
}

/// An orbit turns the aircraft about the centre of its bounding box: that
/// point stays at one place on screen for every camera orientation.
#[test]
fn orbiting_keeps_the_bounding_box_centre_fixed_on_screen() {
    let mut state = AppState::default();
    assert!(state.enter_sandbox(true));
    state.set_sandbox_fit_inset((200.0, 44.0, 12.0, 64.0));
    state.set_sandbox_viewport_size((1560.0, 860.0));
    let centre = state
        .sandbox
        .model
        .as_ref()
        .expect("model")
        .framing_reference(None)
        .center;
    assert_eq!(
        state.sandbox.pivot,
        Some(centre),
        "the default pivot is the bbox centre"
    );
    let mut seen = Vec::new();
    for delta in [(0.0, 0.0), (130.0, 25.0), (-260.0, -40.0), (90.0, 70.0)] {
        state
            .preview_camera_mut(SANDBOX_CAMERA_ID)
            .apply_orbit_motion(egui::vec2(delta.0, delta.1));
        state.reproject_sandbox_scene();
        let (_, framing) = state.sandbox.scene.clone().expect("scene");
        seen.push(framing.project(centre));
    }
    for point in &seen[1..] {
        assert!(
            (point[0] - seen[0][0]).abs() < 1e-6 && (point[1] - seen[0][1]).abs() < 1e-6,
            "the pivot moved on screen: {seen:?}"
        );
    }
}

/// Picking a new pivot moves nothing on screen; later orbits turn about it,
/// and Fit returns to the bounding-box centre.
#[test]
fn a_picked_pivot_keeps_the_picture_and_becomes_the_orbit_centre() {
    let mut state = AppState::default();
    assert!(state.enter_sandbox(true));
    state.set_sandbox_viewport_size((1560.0, 860.0));
    let (_, framing) = state.sandbox.scene.clone().expect("scene");
    let model_centre = state.sandbox.pivot.expect("pivot");
    // The centre of a fuselage cabin section, on screen: a surface lies there.
    let plane = state.sandbox.airplane.as_ref().expect("plane");
    let xsecs = &plane.fuselages[0].xsecs;
    let mid = xsecs[xsecs.len() / 2].xyz_c;
    let aim = framing.project(mid);
    assert!(
        state.set_sandbox_pivot_at(&framing, aim),
        "a surface lies under the point"
    );
    let picked = state.sandbox.pivot.expect("pivot");
    assert_ne!(picked, model_centre);
    state.reproject_sandbox_scene();
    let (_, after) = state.sandbox.scene.clone().expect("scene");
    let [x, y] = after.project(picked);
    assert!(
        (x - aim[0]).abs() < 1e-6 && (y - aim[1]).abs() < 1e-6,
        "no jump: {x},{y} vs {aim:?}"
    );
    state
        .preview_camera_mut(SANDBOX_CAMERA_ID)
        .apply_orbit_motion(egui::vec2(180.0, 30.0));
    state.reproject_sandbox_scene();
    let (_, turned) = state.sandbox.scene.clone().expect("scene");
    let [x, y] = turned.project(picked);
    assert!(
        (x - aim[0]).abs() < 1e-6 && (y - aim[1]).abs() < 1e-6,
        "the picked pivot orbits in place"
    );
    state.refit_sandbox_framing();
    assert_eq!(state.sandbox.pivot, Some(model_centre));
}

/// The review pictures of the camera presets on the A380-800 and the AVE.
#[test]
fn render_the_presets_for_review() {
    for (name, label) in [("AVE", "ave"), ("A380-800", "a380")] {
        let mut state = AppState::default();
        if name != "AVE" {
            state.load_preset(name);
            state.sandbox.last_design = Some(super::super::session::SandboxDesign {
                config_values: state.config_values.clone(),
                design_values: state.design_values.clone(),
            });
            state.sandbox.resume_exact = true;
        }
        assert!(state.enter_sandbox(name == "AVE"));
        state.set_sandbox_viewport_size((1000.0, 640.0));
        for (preset, camera) in [
            ("iso", PreviewCamera::isometric()),
            ("top", PreviewCamera::top()),
            ("front", PreviewCamera::front()),
            ("rear", PreviewCamera::rear()),
            ("side", PreviewCamera::side()),
        ] {
            *state.preview_camera_mut(SANDBOX_CAMERA_ID) = camera;
            state.refit_sandbox_framing();
            let (scene, _) = state.sandbox.scene.clone().expect("scene");
            let png = alas_viz::raster::render_scene_png(&scene).expect("png");
            let dir_name = crate::test_raster::render_dir();
            let dir = std::path::Path::new(&dir_name);
            if dir.is_dir() {
                let _ = std::fs::write(dir.join(format!("preset-{label}-{preset}.png")), png);
            }
        }
    }
}
