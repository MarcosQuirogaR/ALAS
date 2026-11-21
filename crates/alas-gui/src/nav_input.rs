// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The mouse mapping of every 3-D view, one place so the views cannot drift:
//!
//! - left drag on empty space orbits about the pivot;
//! - right drag, middle drag or Shift + left drag pans;
//! - the wheel zooms towards the cursor (handled by each view);
//! - a middle click or a double left click sets the pivot.
//!
//! [`step`] is the pure mapping; [`read`] feeds it from egui. Motion is the
//! logical-point `drag_delta()`, never raw device counts.

use egui::{PointerButton, Response, Vec2};

/// What the pointer does to the camera this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum NavAction {
    /// Nothing for the camera.
    None,
    /// Move the view with the pointer, in points.
    Pan(Vec2),
    /// Orbit about the pivot by the pointer motion, in points.
    Rotate(Vec2),
}

/// The button that is dragging, and the Shift state.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NavInput {
    pub primary: bool,
    pub secondary: bool,
    pub middle: bool,
    pub shift: bool,
    /// Drag motion this frame, in points.
    pub delta: Vec2,
}

/// The action of one frame.
pub(crate) fn step(input: NavInput) -> NavAction {
    if input.secondary || input.middle || (input.primary && input.shift) {
        NavAction::Pan(input.delta)
    } else if input.primary {
        NavAction::Rotate(input.delta)
    } else {
        NavAction::None
    }
}

/// This frame's action on the view `response` belongs to. A frame that ends
/// in a double click navigates nothing, so the click's tiny motion does not
/// move the camera as the double click acts.
pub(crate) fn read(response: &Response) -> NavAction {
    if response.double_clicked() {
        return NavAction::None;
    }
    let shift = response.ctx.input(|i| i.modifiers.shift);
    step(NavInput {
        primary: response.dragged_by(PointerButton::Primary),
        secondary: response.dragged_by(PointerButton::Secondary),
        middle: response.dragged_by(PointerButton::Middle),
        shift,
        delta: response.drag_delta(),
    })
}

/// Move a 3-D view's canvas by `delta` points (the pan of a view whose camera
/// has no pan of its own). Leaves fit-to-view, so the widget applies it.
pub(crate) fn pan_canvas(view: &mut alas_viz::SceneViewState, delta: Vec2) {
    if delta.is_finite() {
        view.pan += delta;
        view.auto_fit = false;
    }
}

/// Re-centre a canvas-panned view on the point under the pointer.
pub(crate) fn recentre_canvas(view: &mut alas_viz::SceneViewState, response: &Response) {
    if let Some(p) = response
        .interact_pointer_pos()
        .or_else(|| response.hover_pos())
    {
        pan_canvas(view, response.rect.center() - p);
    }
}

/// Whether a middle click without a drag happened on the view.
pub(crate) fn pivot_clicked(response: &Response) -> bool {
    response.clicked_by(PointerButton::Middle)
}

/// Pivot request: a middle click or a double left click.
pub(crate) fn pivot_requested(response: &Response) -> bool {
    pivot_clicked(response) || response.double_clicked_by(PointerButton::Primary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    fn input(primary: bool, secondary: bool, middle: bool, shift: bool) -> NavInput {
        NavInput {
            primary,
            secondary,
            middle,
            shift,
            delta: vec2(5.0, -2.0),
        }
    }

    #[test]
    fn left_orbits_and_the_other_drags_pan() {
        let d = vec2(5.0, -2.0);
        assert_eq!(step(input(true, false, false, false)), NavAction::Rotate(d));
        assert_eq!(step(input(false, true, false, false)), NavAction::Pan(d));
        assert_eq!(step(input(false, false, true, false)), NavAction::Pan(d));
        assert_eq!(step(input(true, false, false, true)), NavAction::Pan(d));
        assert_eq!(step(input(false, false, false, false)), NavAction::None);
    }
}
