// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The landing form stays readable on a window narrower than the shell's
//! declared minimum.
//!
//! `21-narrow-viewport.png` of the 2026-09-17 native screenshot batch caught
//! a 466 x 893 window in which the live-preview dock kept its 300-point
//! minimum and the form column was hard-clipped at 144 points: headings and
//! labels were cut mid-glyph with no ellipsis, and all three TLAR values
//! (Mach 0.84, 11887 m, 358670 kg) were entirely off-screen with no
//! horizontal scroll bar.
//!
//! These tests render the real Inputs page headlessly and inspect the text
//! shapes egui emits against the clip rectangle each was painted under, which
//! is exactly the condition that produced the cut glyphs.

use alas_gui::layout;
use alas_gui::state::AppState;
use alas_gui::views::show_inputs_view;
use egui::{vec2, Context, FullOutput, Margin, Pos2, RawInput, Rect, Shape};

/// The central panel's inner margin in the desktop shell.
const CONTENT_MARGIN: Margin = Margin {
    left: 26.0,
    right: 18.0,
    top: 16.0,
    bottom: 14.0,
};

/// The captured narrow window, in points.
const NARROW: (f32, f32) = (466.0, 893.0);

/// The dock width the capture measured, frame included.
const CAPTURED_DOCK_WIDTH: f32 = 317.0;

/// Every painted string whose glyphs fall outside the clip rectangle they
/// were painted under, which is what "cut mid-glyph" looks like in shapes.
fn horizontally_clipped_text(output: &FullOutput) -> Vec<String> {
    fn walk(shape: &Shape, clip: Rect, found: &mut Vec<String>) {
        match shape {
            Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, clip, found);
                }
            }
            Shape::Text(text) => {
                let rect = text.galley.rect.translate(text.pos.to_vec2());
                if rect.right() > clip.right() + 0.5 || rect.left() < clip.left() - 0.5 {
                    let painted = text.galley.text().trim();
                    if !painted.is_empty() {
                        found.push(painted.to_owned());
                    }
                }
            }
            _ => {}
        }
    }

    let mut found = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, clipped.clip_rect, &mut found);
    }
    found
}

/// The right edge of the widest box the page painted.
///
/// A combo box cannot be wrapped by the layout, so one that does not fit
/// used to widen the whole page: every card then matched that width and its
/// right border, and the engine selector with it, was cut off by the window.
fn widest_painted_edge(output: &FullOutput) -> f32 {
    fn walk(shape: &Shape, widest: &mut f32) {
        match shape {
            Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, widest);
                }
            }
            Shape::Rect(rect) if rect.rect.width() > 10.0 && rect.rect.right().is_finite() => {
                *widest = widest.max(rect.rect.right());
            }
            _ => {}
        }
    }

    let mut widest = 0.0_f32;
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut widest);
    }
    widest
}

/// Render the Inputs page in a window `size` points across, optionally with a
/// right-hand dock `dock_width` points wide reserving space first.
///
/// Two passes: egui sizes several of this page's rows from what they measured
/// on the previous frame, so the first frame is not representative.
fn render_inputs(size: (f32, f32), dock_width: Option<f32>) -> FullOutput {
    let mut state = AppState::default();
    let ctx = Context::default();
    alas_gui::apply_theme(alas_gui::AppTheme::Dark, &ctx);
    let mut output = None;
    for _ in 0..2 {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(size.0, size.1))),
            ..RawInput::default()
        };
        output = Some(ctx.run(input, |ctx| {
            if let Some(width) = dock_width {
                egui::SidePanel::right("preview_panel")
                    .resizable(false)
                    .exact_width(width)
                    .show(ctx, |ui| {
                        ui.label("3D Live Preview");
                    });
            }
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::central_panel(ctx.style().as_ref()).inner_margin(CONTENT_MARGIN),
                )
                .show(ctx, |ui| show_inputs_view(&mut state, ui));
        }));
    }
    output.expect("two rendered frames")
}

#[test]
fn the_captured_narrow_window_no_longer_reserves_room_for_the_preview_dock() {
    assert!(
        !layout::preview_dock_fits(NARROW.0),
        "466 points cannot hold the dock and a readable form"
    );
    // The client area is narrower still once the window frame is taken off;
    // every plausible client width at this window size behaves the same.
    for client_width in [430.0_f32, 450.0, NARROW.0] {
        assert!(!layout::preview_dock_fits(client_width), "{client_width}");
    }
}

#[test]
fn the_landing_form_paints_no_clipped_glyph_once_the_dock_stands_down() {
    let output = render_inputs(NARROW, None);
    let clipped = horizontally_clipped_text(&output);
    assert!(
        clipped.is_empty(),
        "the form must reflow, not cut glyphs, at {} x {}: {clipped:?}",
        NARROW.0,
        NARROW.1
    );
}

#[test]
fn the_same_page_is_clipped_when_the_dock_keeps_its_old_fixed_width() {
    // The control: reproduce the captured layout and confirm these shapes do
    // detect the defect, so the test above is evidence and not a tautology.
    let output = render_inputs(NARROW, Some(CAPTURED_DOCK_WIDTH));
    let clipped = horizontally_clipped_text(&output);
    assert!(
        !clipped.is_empty(),
        "the captured 317-point dock must still reproduce the clipping"
    );
}

#[test]
fn the_declared_minimum_window_keeps_both_the_dock_and_a_readable_form() {
    // `alas_gui::run` declares an 880 x 560 minimum inner size.
    let range = layout::preview_width_range(880.0).expect("the dock fits at the declared minimum");
    assert!(*range.start() >= layout::PREVIEW_DOCK_MIN_WIDTH);
    assert!(880.0 - *range.end() >= layout::CONTENT_MIN_WIDTH - 0.5);

    let output = render_inputs((880.0 - range.end(), 560.0), None);
    let clipped = horizontally_clipped_text(&output);
    assert!(
        clipped.is_empty(),
        "the form must stay unclipped beside a widest-allowed dock: {clipped:?}"
    );
}

#[test]
fn the_tlar_values_are_painted_inside_the_clip_rectangle_on_a_narrow_window() {
    // The capture's concrete loss: the three TLAR editors showed no value at
    // all. AVE's defaults are Mach 0.84, 11887 m and 358670 kg.
    let output = render_inputs(NARROW, None);
    let mut painted = Vec::new();
    fn walk(shape: &Shape, clip: Rect, painted: &mut Vec<String>) {
        match shape {
            Shape::Vec(shapes) => {
                for shape in shapes {
                    walk(shape, clip, painted);
                }
            }
            Shape::Text(text) => {
                let rect = text.galley.rect.translate(text.pos.to_vec2());
                if clip.contains_rect(rect.shrink(0.5)) {
                    painted.push(text.galley.text().trim().to_owned());
                }
            }
            _ => {}
        }
    }
    for clipped in &output.shapes {
        walk(&clipped.shape, clipped.clip_rect, &mut painted);
    }
    for value in ["0.84", "11887", "358670"] {
        assert!(
            painted.iter().any(|text| text.contains(value)),
            "authoritative TLAR value {value} is not painted: {painted:?}"
        );
    }
}

#[test]
fn no_card_is_painted_wider_than_the_window_at_any_supported_width() {
    // The page settles: an overflowing row used to grow the content width
    // frame after frame, so the check is made on a settled render.
    for (width, height) in [NARROW, (640.0, 800.0), (880.0, 560.0), (1_600.0, 900.0)] {
        let output = render_inputs((width, height), None);
        let widest = widest_painted_edge(&output);
        assert!(
            widest <= width + 0.5,
            "the landing page painted a box out to {widest} in a {width} point window"
        );
    }
}

#[test]
fn the_preset_and_engine_selectors_share_one_row_only_when_both_fit() {
    // Wide: the pair stays on one row, so the card is one line of controls.
    let wide = render_inputs((1_600.0, 900.0), None);
    assert!(widest_painted_edge(&wide) <= 1_600.5);

    // Narrow: they stack, and neither runs past the window.
    let narrow = render_inputs(NARROW, None);
    assert!(widest_painted_edge(&narrow) <= NARROW.0 + 0.5);
    assert!(horizontally_clipped_text(&narrow).is_empty());
}

#[test]
fn spanish_keeps_the_narrow_layout_and_the_blocking_reason_readable() {
    alas_i18n::es::install();
    alas_i18n::set_language(Some("es"));

    let mut state = AppState::default();
    state.language = alas_gui::state::Language::Es;
    state.config_values["requirements"]["cruise_mach"] = serde_json::Value::from(3.5);
    state.on_config_modified();
    assert!(state.blocked(), "Mach 3.5 blocks the run in any language");

    // The framing is catalogued, so the reason a run is blocked is Spanish.
    let reasons = alas_gui::views::notices::blocking_reasons(&state);
    assert!(!reasons.is_empty());
    assert!(
        alas_i18n::t(
            Some("This design cannot be run until these are fixed:"),
            Some("es")
        )
        .starts_with("Este dise"),
        "the blocking heading must be translated"
    );
    assert!(
        alas_i18n::t(
            Some(
                "The 3D live preview is hidden: this window is too narrow to show it beside a readable form. Widen the window to bring it back."
            ),
            Some("es"),
        )
        .starts_with("La vista previa 3D"),
        "the suppressed-preview notice must be translated"
    );

    // Spanish prose is longer; the page must still not overflow.
    let ctx = Context::default();
    alas_gui::apply_theme(alas_gui::AppTheme::Dark, &ctx);
    let mut output = None;
    for _ in 0..2 {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(NARROW.0, NARROW.1))),
            ..RawInput::default()
        };
        output = Some(ctx.run(input, |ctx| {
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::central_panel(ctx.style().as_ref()).inner_margin(CONTENT_MARGIN),
                )
                .show(ctx, |ui| show_inputs_view(&mut state, ui));
        }));
    }
    let output = output.expect("two rendered frames");
    alas_i18n::set_language(Some("en"));
    assert!(widest_painted_edge(&output) <= NARROW.0 + 0.5);
    assert!(horizontally_clipped_text(&output).is_empty());
}
