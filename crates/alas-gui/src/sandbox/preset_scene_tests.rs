// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Entering the Sandbox from every shipped preset builds the aircraft scene
//! on the overview and on every discipline page, headless.
//!
//! The entry runs inside a rendered frame whose typed configuration is already
//! memoized for the preset, as in the app: the sandbox replaces the whole
//! configuration, so the memo must not serve the preset's stale geometry with
//! the sandbox's design vector.

use crate::sandbox::fields::Discipline;
use crate::state::AppState;

#[test]
fn every_preset_enters_the_sandbox_with_a_scene_on_every_page() {
    for name in alas_config::presets::available() {
        let mut state = AppState::default();
        state.finish_walkthrough();
        state.load_preset(name);
        state.begin_frame();
        assert!(state.typed_config().is_some(), "{name}: typed config");
        assert!(state.enter_sandbox(false), "{name}: enter");
        assert!(
            state.sandbox.rejected_edit.is_none(),
            "{name}: {:?}",
            state.sandbox.rejected_edit
        );
        assert!(state.sandbox.airplane.is_some(), "{name}: airplane");
        assert!(state.sandbox.model.is_some(), "{name}: model");
        assert!(state.sandbox.scene.is_some(), "{name}: overview scene");
        for discipline in Discipline::ALL {
            state.sandbox.set_focus(Some(discipline));
            state.sandbox.framing = None;
            state.sandbox.scene = None;
            state.reproject_sandbox_scene();
            assert!(
                state.sandbox.scene.is_some(),
                "{name}: {} scene",
                discipline.id()
            );
        }
        state.sandbox.set_focus(None);
    }
}

#[test]
fn a_scene_survives_entering_the_sandbox_after_a_memoized_a380_frame() {
    let mut state = AppState::default();
    state.finish_walkthrough();
    state.load_preset("A380-800");
    state.begin_frame();
    let _ = state.typed_config();
    assert!(state.enter_sandbox(false));
    crate::sandbox::scene::try_build_sandbox_airplane(&state).expect("the sandbox aircraft builds");
}
