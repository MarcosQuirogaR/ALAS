// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Hit testing for the compact, unpinned navigation rail.
//!
//! Keeping this independent of application state makes the hover contract
//! testable at every display scale and prevents the shell from carrying a
//! second, hidden source of navigation state.

/// Resolve one frame of the unpinned navigation hover state machine.
///
/// Entry is deliberately limited to the visible 8-point rail. Once open, the
/// pointer may travel anywhere across the 232-point panel without collapsing
/// it; leaving that panel closes it again.
pub fn nav_overlay_open(current_open: bool, pointer_x: Option<f32>, rail_left: f32) -> bool {
    nav_overlay_open_with_bounds(current_open, pointer_x, rail_left, 8.0, 232.0)
}

/// Resolve one frame of the unpinned navigation state machine for explicit
/// logical dimensions.
///
/// Keeping the dimensions as arguments makes the hover contract testable at
/// the compact and wide window sizes used by the desktop acceptance matrix.
/// Display scale factors do not change these values because egui supplies
/// pointer coordinates in logical points.
pub fn nav_overlay_open_with_bounds(
    current_open: bool,
    pointer_x: Option<f32>,
    rail_left: f32,
    rail_width: f32,
    panel_width: f32,
) -> bool {
    let Some(pointer_x) = pointer_x else {
        return false;
    };
    let offset = pointer_x - rail_left;
    if !(0.0..=panel_width).contains(&offset) {
        return false;
    }
    current_open || offset <= rail_width
}
