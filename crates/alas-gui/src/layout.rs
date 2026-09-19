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
/// Vertical inset around the top-level menu buttons. The menu bar itself uses
/// the interaction height, so this inset keeps hover/open fills around the
/// labels instead of welding them to the bar's top and bottom edges.
pub const MENU_BAR_VERTICAL_INSET: f32 = 6.0;

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
/// Narrowest central content column that still renders the schema-driven
/// form without clipping.
///
/// One form column is `MIN_FORM_COLUMN_WIDTH` = 300 pt wide (see
/// `views::form`). Around it the central panel spends 26 + 18 pt of frame
/// margin, a card spends 2 x `theme::CARD_INNER_MARGIN_X` = 28 pt, the card's
/// collapsing header indents ~18 pt and the page's vertical scroll bar takes
/// ~10 pt, so 400 pt of panel width is the floor at which every label and
/// every value box is still fully drawn.
pub const CONTENT_MIN_WIDTH: f32 = 400.0;
/// Width the Inputs page's preset and engine selectors need side by side.
///
/// A combo box sizes itself to its longest entry and the wrapped layout
/// cannot break before one, so below this the pair used to widen the whole
/// page and run under the window's right edge. Above it they share a row.
pub const SELECTOR_PAIR_MIN_WIDTH: f32 = 460.0;

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

/// Return the user-resizable width range for the right-side preview dock, or
/// `None` when the viewport is too narrow to hold the dock and a readable
/// form at the same time.
///
/// `available_width` is the width left after every panel added before the
/// dock (menu bar, pinned navigation, control bar, run log). It deliberately
/// does *not* include the dock itself, so the returned maximum is constant
/// while the splitter is dragged and `SidePanel`'s stored width never snaps
/// back mid-gesture.
///
/// The maximum is the space left once [`CONTENT_MIN_WIDTH`] is reserved for
/// the form. Below `CONTENT_MIN_WIDTH + PREVIEW_DOCK_MIN_WIDTH` no split can
/// satisfy both, so the dock yields the whole width to the form rather than
/// clipping it; the caller keeps the user's own open/closed preference and
/// says why the dock is not on screen.
pub fn preview_width_range(available_width: f32) -> Option<std::ops::RangeInclusive<f32>> {
    if available_width.is_nan() || available_width < CONTENT_MIN_WIDTH + PREVIEW_DOCK_MIN_WIDTH {
        return None;
    }
    let max =
        (available_width - CONTENT_MIN_WIDTH).clamp(PREVIEW_DOCK_MIN_WIDTH, PREVIEW_DOCK_MAX_WIDTH);
    Some(PREVIEW_DOCK_MIN_WIDTH..=max)
}

/// Whether the right-side preview dock has room beside a readable form.
pub fn preview_dock_fits(available_width: f32) -> bool {
    preview_width_range(available_width).is_some()
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
    use super::{
        preview_dock_fits, preview_placement, preview_width_range, PreviewPlacement,
        CONTENT_MIN_WIDTH, PREVIEW_DOCK_MAX_WIDTH, PREVIEW_DOCK_MIN_WIDTH,
    };

    #[test]
    fn preview_stays_to_the_right_of_the_editor_at_every_window_width() {
        for width in [360.0, 760.0, 1_280.0, 3_840.0] {
            assert_eq!(preview_placement(width), PreviewPlacement::Side);
        }
    }

    #[test]
    fn preview_width_stays_user_resizable_without_responsive_bottom_promotion() {
        // A comfortable window keeps the full, width-independent range, so
        // dragging the splitter never re-clamps the stored width.
        for width in [1_000.0, 1_600.0, 3_840.0] {
            let range = preview_width_range(width).expect("dock fits");
            assert_eq!(*range.start(), PREVIEW_DOCK_MIN_WIDTH, "width {width}");
            assert_eq!(*range.end(), PREVIEW_DOCK_MAX_WIDTH, "width {width}");
        }
    }

    #[test]
    fn the_dock_never_takes_the_width_the_form_needs_to_stay_readable() {
        // In the transition band the dock may only take what is left once the
        // form has its readable floor.
        for width in [720.0, 800.0, 880.0] {
            let range = preview_width_range(width)
                .unwrap_or_else(|| panic!("dock should still fit at {width}"));
            assert_eq!(*range.start(), PREVIEW_DOCK_MIN_WIDTH, "width {width}");
            assert!(
                width - *range.end() >= CONTENT_MIN_WIDTH - 0.5,
                "width {width} leaves only {} for the form",
                width - *range.end()
            );
            assert!(*range.end() <= PREVIEW_DOCK_MAX_WIDTH);
        }
    }

    #[test]
    fn a_viewport_too_narrow_for_both_gives_the_whole_width_to_the_form() {
        // 466 px is the captured narrow-window regression; every width below
        // the split threshold must hide the dock rather than clip the form.
        for width in [320.0, 450.0, 466.0, 640.0, 699.0] {
            assert!(
                preview_width_range(width).is_none(),
                "dock must yield the width at {width}"
            );
            assert!(!preview_dock_fits(width), "width {width}");
        }
        assert!(preview_dock_fits(
            CONTENT_MIN_WIDTH + PREVIEW_DOCK_MIN_WIDTH
        ));
    }

    #[test]
    fn a_non_finite_viewport_width_hides_the_dock_instead_of_panicking() {
        assert!(preview_width_range(f32::NAN).is_none());
        assert!(preview_width_range(f32::INFINITY).is_some());
    }
}
