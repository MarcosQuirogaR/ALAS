// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Framing of the sandbox scene: the fit to the part of the viewport the
//! floating controls leave free, and the view state a sandbox starts from.

use super::fields::Discipline;
use super::scene::{discipline_component, LIVE_PREVIEW_FILL, SANDBOX_CAMERA_ID, SANDBOX_VIEW_KEY};
use crate::state::AppState;

/// What the floating controls cover before they are measured, in points:
/// the category stack on the left, the camera row above, the action block
/// below.
const DEFAULT_FIT_INSET: (f32, f32, f32, f32) = (200.0, 44.0, 12.0, 64.0);

impl AppState {
    /// Fit the framing to the components now shown and redraw: the explicit
    /// Fit action, and the deliberate reframe on a focus change or a new
    /// design. Camera presets, orbit, zoom, resizing and geometry edits
    /// never call this, so the pixels per metre they show stay comparable.
    pub fn refit_sandbox_framing(&mut self) {
        self.fit_sandbox_view();
        self.reproject_sandbox_scene();
    }

    /// Set the framing, the orbit pivot and its screen offset from the tight
    /// fit. The pivot is the centre of the shown components' bounding box;
    /// the pan puts the tightly fitted picture in the free part of the
    /// viewport, so an orbit about the pivot never moves it on screen.
    pub(super) fn fit_sandbox_view(&mut self) {
        let tight = self.tight_sandbox_framing();
        self.sandbox.framing = tight;
        self.sandbox.pivot = None;
        self.sandbox.pan = (0.0, 0.0);
        self.sandbox.view_touched = false;
        self.sandbox.pivot_marker_at = None;
        let (Some(tight), Some(model), Some((width, height))) =
            (tight, &self.sandbox.model, self.sandbox.viewport_size)
        else {
            return;
        };
        let isolate = self.sandbox.focus().map(discipline_component);
        let pivot = model.framing_reference(isolate).center;
        let camera: alas_report::scene::Camera3D = self
            .preview_cameras
            .get(SANDBOX_CAMERA_ID)
            .copied()
            .unwrap_or_default()
            .into();
        let pan = tight.offset_of(&camera, (f64::from(width), f64::from(height)), pivot);
        self.sandbox.pivot = Some(pivot);
        self.sandbox.pan = pan;
    }

    /// The tight fit of the shown components to the free part of the measured
    /// viewport, or `None` before there is a viewport or a model.
    pub(super) fn tight_sandbox_framing(
        &self,
    ) -> Option<alas_report::families::geometry::FramingReference> {
        let mut framing = None;
        // The tight fit of what is shown for the current camera and viewport,
        // so a wide viewport is used across its width; without a measured
        // viewport the orientation-independent fit stands.
        if let (Some(model), Some((width, height))) =
            (&self.sandbox.model, self.sandbox.viewport_size)
        {
            let camera = self
                .preview_cameras
                .get(SANDBOX_CAMERA_ID)
                .copied()
                .unwrap_or_default();
            let (left, top, right, bottom) = self.sandbox.fit_inset.unwrap_or(DEFAULT_FIT_INSET);
            // The model fits the part of the viewport the controls leave
            // free, so it is neither under the buttons nor past an edge.
            framing = Some(model.tight_framing_reference(
                self.sandbox.focus().map(discipline_component),
                camera.into(),
                (f64::from(width), f64::from(height)),
                LIVE_PREVIEW_FILL,
                (
                    f64::from(left),
                    f64::from(top),
                    f64::from(right),
                    f64::from(bottom),
                ),
            ));
        }
        framing
    }

    /// Make the surface point under `canvas_point` the orbit pivot, keeping
    /// the picture where it is. Returns whether a surface was hit.
    pub fn set_sandbox_pivot_at(
        &mut self,
        framing: &alas_report::families::geometry::SceneFraming,
        canvas_point: [f64; 2],
    ) -> bool {
        let isolate = self.sandbox.focus().map(discipline_component);
        let Some(model) = &self.sandbox.model else {
            return false;
        };
        let Some(hit) =
            super::navigation::pick_surface_point(model.faces(), isolate, framing, canvas_point)
        else {
            return false;
        };
        let [x, y] = framing.project(hit);
        self.sandbox.pivot = Some(hit);
        self.sandbox.pan = (x - framing.canvas.0 * 0.5, y - framing.canvas.1 * 0.5);
        true
    }

    /// Focus one discipline (isolating it in the preview) or return to the
    /// overview, refitting the framing to what is now shown.
    pub fn set_sandbox_focus(&mut self, discipline: Option<Discipline>) {
        let changed = self.sandbox.focus() != discipline;
        self.sandbox.set_focus(discipline);
        if changed && discipline == Some(Discipline::VerticalTail) {
            // A fin seen from the front is an edge-on sliver: open on its side.
            *self.preview_camera_mut(SANDBOX_CAMERA_ID) = crate::state::PreviewCamera::side();
        }
        self.refit_sandbox_framing();
    }

    /// Record the viewport size the scene is drawn for. A changed size
    /// redraws the kept framing on the new canvas, so the model scales
    /// with the viewport's smaller side and nothing is refitted.
    pub fn set_sandbox_viewport_size(&mut self, size: (f32, f32)) -> bool {
        let first = self.sandbox.viewport_size.is_none();
        let changed = self
            .sandbox
            .viewport_size
            .is_none_or(|(w, h)| (w - size.0).abs() > 0.5 || (h - size.1).abs() > 0.5);
        if changed {
            // A view the user left as fitted (inside the free part) stays so.
            let was_inside = !self.drawn_outside_free_viewport();
            let previous = self.sandbox.viewport_size;
            self.sandbox.viewport_size = Some(size);
            if first {
                // The first measured viewport frames the aircraft to its shape.
                self.refit_sandbox_framing();
            } else if !self.sandbox.view_touched {
                // A view nobody panned or zoomed follows the window: refit.
                self.refit_sandbox_framing();
            } else {
                // A kept view scales with the smaller viewport side about the
                // centre of the part of the viewport the floating controls
                // leave free, so a smaller window never slides the aircraft
                // under the discipline buttons. The pan is the pivot's screen
                // offset from the viewport centre, x right and y down.
                if let Some((old_w, old_h)) = previous {
                    let side = |w: f32, h: f32| f64::from(w.min(h) - 40.0).max(1.0);
                    let ratio = side(size.0, size.1) / side(old_w, old_h);
                    let (left, top, right, bottom) =
                        self.sandbox.fit_inset.unwrap_or(DEFAULT_FIT_INSET);
                    let free = (f64::from(left - right) * 0.5, f64::from(top - bottom) * 0.5);
                    let pan = self.sandbox.pan;
                    self.sandbox.pan = (
                        free.0 + (pan.0 - free.0) * ratio,
                        free.1 + (pan.1 - free.1) * ratio,
                    );
                }
                self.reproject_sandbox_scene();
                // A window that narrows more than it shortens would push a wing
                // tip past an edge: refit then. A view the user had zoomed out
                // of the free part is left alone.
                if was_inside && self.drawn_outside_free_viewport() {
                    self.refit_sandbox_framing();
                }
            }
        }
        changed
    }

    /// Whether the scene drawn now reaches outside the part of the viewport the
    /// floating controls leave free (a few points of slack for outlines).
    pub(super) fn drawn_outside_free_viewport(&self) -> bool {
        use alas_report::scene::SceneElement;
        let (Some((scene, _)), Some((width, height))) =
            (&self.sandbox.scene, self.sandbox.viewport_size)
        else {
            return false;
        };
        let (left, top, right, bottom) = self.sandbox.fit_inset.unwrap_or(DEFAULT_FIT_INSET);
        let (x0, y0) = (f64::from(left) - 2.0, f64::from(top) - 2.0);
        let (x1, y1) = (
            f64::from(width - right) + 2.0,
            f64::from(height - bottom) + 2.0,
        );
        scene.elements.iter().any(|element| match element {
            SceneElement::Polygon { points, .. } => points
                .iter()
                .any(|p| p[0] < x0 || p[0] > x1 || p[1] < y0 || p[1] > y1),
            _ => false,
        })
    }

    /// Record what the floating controls cover of the viewport, so the next
    /// fit keeps the model clear of them. A view nobody touched is refitted
    /// when the measured controls differ from what its fit assumed (the first
    /// frame fits before the controls have been drawn).
    pub fn set_sandbox_fit_inset(&mut self, inset: (f32, f32, f32, f32)) {
        let assumed = self.sandbox.fit_inset.unwrap_or(DEFAULT_FIT_INSET);
        self.sandbox.fit_inset = Some(inset);
        let moved = (assumed.0 - inset.0).abs() > 1.5
            || (assumed.1 - inset.1).abs() > 1.5
            || (assumed.2 - inset.2).abs() > 1.5
            || (assumed.3 - inset.3).abs() > 1.5;
        if moved && !self.sandbox.view_touched && self.sandbox.viewport_size.is_some() {
            self.refit_sandbox_framing();
        }
    }

    /// Show every component again at the default isometric Overview, fitted.
    pub fn show_whole_aircraft(&mut self) {
        *self.preview_camera_mut(SANDBOX_CAMERA_ID) = crate::state::PreviewCamera::isometric();
        self.view_state_mut(SANDBOX_VIEW_KEY).reset();
        self.set_sandbox_focus(None);
    }

    /// Forget the view of the sandbox just left: the focused component, the
    /// framing, the camera, the pan and the open editor windows. The next
    /// sandbox opens on the whole aircraft, fitted afresh.
    pub fn reset_sandbox_view(&mut self) {
        self.sandbox.set_focus(None);
        self.sandbox.framing = None;
        self.sandbox.pivot = None;
        self.sandbox.pan = (0.0, 0.0);
        self.sandbox.viewport_size = None;
        self.sandbox.scene = None;
        self.sandbox.layout.open_disciplines.clear();
        self.sandbox.raise_discipline = None;
        self.preview_cameras.remove(SANDBOX_CAMERA_ID);
        self.view_states.remove(SANDBOX_VIEW_KEY);
    }
}

/// What the floating controls covered of the viewport in the last frame, in
/// points (left, top, right, bottom), or `None` before they were drawn.
pub(super) fn measured_fit_inset(ctx: &egui::Context) -> Option<(f32, f32, f32, f32)> {
    use super::viewport::{overlay_rect_tagged, overlay_rects_tagged, viewport_rect};
    let viewport = viewport_rect(ctx)?;
    let stack = overlay_rect_tagged(ctx, "stack")?;
    let top =
        overlay_rect_tagged(ctx, "camera").map_or(44.0, |r| r.bottom() - viewport.top() + 8.0);
    let action_top = overlay_rects_tagged(ctx, "action")
        .iter()
        .map(|r| r.top())
        .reduce(f32::min);
    let bottom = action_top.map_or(64.0, |y| viewport.bottom() - y + 8.0);
    Some((
        (stack.right() - viewport.left() + 12.0).clamp(0.0, viewport.width() * 0.5),
        top.clamp(0.0, viewport.height() * 0.3),
        12.0,
        bottom.clamp(0.0, viewport.height() * 0.3),
    ))
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::super::session::ExitChoice;
    use super::*;
    use alas_report::scene::SceneElement;

    /// The extent of everything the scene draws, in canvas points.
    fn drawn_extent(scene: &alas_report::scene::Scene) -> (f64, f64, f64, f64) {
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for element in &scene.elements {
            if let SceneElement::Polygon { points, .. } = element {
                for point in points {
                    x0 = x0.min(point[0]);
                    x1 = x1.max(point[0]);
                    y0 = y0.min(point[1]);
                    y1 = y1.max(point[1]);
                }
            }
        }
        (x0, y0, x1, y1)
    }

    /// The overview and a focused component are both fitted to the part of
    /// the viewport the floating controls leave free: neither under the
    /// button stack nor past an edge, and the overview uses the viewport.
    #[test]
    fn a_fit_keeps_the_model_inside_the_free_part_of_the_viewport() {
        let mut state = AppState::default();
        assert!(state.enter_sandbox(true));
        state.set_sandbox_fit_inset((200.0, 44.0, 12.0, 64.0));
        state.set_sandbox_viewport_size((1560.0, 860.0));
        for focus in [None, Some(Discipline::Wing)] {
            state.set_sandbox_focus(focus);
            let (scene, _) = state.sandbox.scene.clone().expect("scene");
            let (x0, y0, x1, y1) = drawn_extent(&scene);
            assert!(
                x0 >= 198.0 && x1 <= 1560.0 - 10.0,
                "{focus:?}: x {x0}..{x1}"
            );
            assert!(y0 >= 42.0 && y1 <= 860.0 - 62.0, "{focus:?}: y {y0}..{y1}");
            let fill = ((x1 - x0) / (1560.0 - 212.0)).max((y1 - y0) / (860.0 - 108.0));
            assert!(fill > 0.8, "{focus:?} fills {fill:.2} of the free part");
        }
    }

    /// Discarding the sandbox forgets the focused component and the framing,
    /// so the next one opens on the whole aircraft.
    #[test]
    fn discarding_the_sandbox_resets_its_view() {
        let mut state = AppState::default();
        assert!(state.enter_sandbox(true));
        state.set_sandbox_viewport_size((1200.0, 800.0));
        state.set_sandbox_focus(Some(Discipline::Wing));
        state
            .sandbox
            .layout
            .open_disciplines
            .push("wing".to_owned());
        assert!(state.resolve_leave_sandbox(ExitChoice::Discard));
        assert!(state.enter_sandbox(true));
        assert_eq!(state.sandbox.focus(), None);
        assert!(state.sandbox.layout.open_disciplines.is_empty());
        state.set_sandbox_viewport_size((1200.0, 800.0));
        let (scene, _) = state.sandbox.scene.clone().expect("scene");
        let (x0, _, x1, _) = drawn_extent(&scene);
        assert!(
            x1 - x0 > 0.5 * 1200.0,
            "the whole aircraft is fitted: {x0}..{x1}"
        );
    }
}

#[cfg(test)]
#[path = "framing_sweep_tests.rs"]
mod sweep_tests;
