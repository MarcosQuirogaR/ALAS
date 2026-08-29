// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Responsive geometry owned by the desktop shell.
//!
//! Keeping the rail and preview-dock bounds in one place prevents the egui
//! composition from growing several slightly different notions of a usable
//! viewport. The functions operate in egui points, so Windows display scale
//! is already accounted for before they are called.

/// Width of the collapsed navigation affordance in egui points.
pub const NAV_RAIL_WIDTH: f32 = 8.0;
/// Vertical inset around the attached navigation overlay.
pub const NAV_OVERLAY_MARGIN: f32 = 12.0;
/// Visual gutter retained between the navigation surface and page content.
pub const NAV_CONTENT_GAP: f32 = 14.0;
/// Preferred width of the expanded navigation panel in egui points.
pub const NAV_PANEL_WIDTH: f32 = 232.0;
/// Smallest width a pinned navigation panel may use.
pub const NAV_PANEL_MIN_WIDTH: f32 = 200.0;
/// Largest width a pinned or hovered navigation panel may use.
pub const NAV_PANEL_MAX_WIDTH: f32 = 360.0;
/// Minimum menu width required by Spanish labels at 100% zoom.
pub const MENU_MIN_WIDTH: f32 = 190.0;

/// Minimum run-log height, including its panel frame.
pub const RUN_LOG_MIN_HEIGHT: f32 = 76.0;
/// Maximum run-log height on a comfortably sized window.
pub const RUN_LOG_MAX_HEIGHT: f32 = 420.0;
/// Space reserved for the central content when the log is enlarged.
const CENTRAL_CONTENT_RESERVE: f32 = 180.0;
/// Default width of the vertical live-preview dock.
pub const PREVIEW_DOCK_DEFAULT_WIDTH: f32 = 360.0;
/// Smallest useful width for the live-preview dock.
pub const PREVIEW_DOCK_MIN_WIDTH: f32 = 300.0;
/// Largest width for the live-preview dock.
pub const PREVIEW_DOCK_MAX_WIDTH: f32 = 520.0;

/// Return the width available to an expanded navigation surface.
///
/// A narrow viewport may be smaller than the normal panel width. Clamping to
/// the viewport keeps the tree reachable without making the central pane
/// wider than the window itself.
pub fn expanded_navigation_width(viewport_width: f32) -> f32 {
    viewport_width.clamp(NAV_RAIL_WIDTH, NAV_PANEL_WIDTH)
}

/// Where the live preview belongs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewPlacement {
    /// Reserve a vertical column on the right of the editor.
    Side,
}

/// Return the fixed right-side preview placement for every window size.
pub fn preview_placement(_available_width: f32) -> PreviewPlacement {
    PreviewPlacement::Side
}

/// Return a usable width range for the right-side preview dock.
///
/// A narrow client area gets a compact dock, but never a bottom-dock
/// promotion. The fixed minimum keeps the controls and canvas readable while
/// the clamp protects embedded and test contexts from invalid egui ranges.
pub fn preview_width_range(available_width: f32) -> std::ops::RangeInclusive<f32> {
    let maximum = (available_width * 0.42)
        .clamp(PREVIEW_DOCK_MIN_WIDTH, PREVIEW_DOCK_MAX_WIDTH)
        .max(PREVIEW_DOCK_MIN_WIDTH);
    PREVIEW_DOCK_MIN_WIDTH..=maximum
}

/// Return the largest usable run-log height for a viewport in egui points.
///
/// The reserve leaves a scrollable central pane even when the user drags the
/// separator on a short window. The panel itself still owns the final clamp
/// against egui's available rect.
pub fn run_log_max_height(viewport_height: f32) -> f32 {
    (viewport_height - CENTRAL_CONTENT_RESERVE).clamp(RUN_LOG_MIN_HEIGHT, RUN_LOG_MAX_HEIGHT)
}

/// Clamp a remembered run-log height to the current responsive range.
pub fn run_log_height(viewport_height: f32, requested_height: f32) -> f32 {
    requested_height.clamp(RUN_LOG_MIN_HEIGHT, run_log_max_height(viewport_height))
}

#[cfg(test)]
mod tests {
    use super::{preview_placement, preview_width_range, PreviewPlacement};

    #[test]
    fn preview_stays_to_the_right_of_the_editor_at_every_window_width() {
        for width in [360.0, 760.0, 1_280.0, 3_840.0] {
            assert_eq!(preview_placement(width), PreviewPlacement::Side);
        }
    }

    #[test]
    fn preview_width_is_bounded_without_responsive_bottom_promotion() {
        let narrow = preview_width_range(640.0);
        assert_eq!(*narrow.start(), 300.0);
        assert_eq!(*narrow.end(), 300.0);

        let wide = preview_width_range(1_600.0);
        assert_eq!(*wide.start(), 300.0);
        assert_eq!(*wide.end(), 520.0);
    }
}
