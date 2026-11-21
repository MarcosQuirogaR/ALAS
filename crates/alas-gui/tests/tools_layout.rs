// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The External Tools page keeps one label/field grid at every window size:
//! the field boxes of all cards start at the same x within a column, no grid
//! label is elided, every status badge shares one x, and every card sits
//! inside the window.

// A test asserts on values it constructed, so a failed unwrap there is the
// assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_gui::state::AppState;
use alas_gui::views::show_tools_view;
use egui::{vec2, Context, FullOutput, Margin, Pos2, RawInput, Rect, Shape};

/// The central panel's inner margin in the desktop shell.
const CONTENT_MARGIN: Margin = Margin {
    left: 26.0,
    right: 18.0,
    top: 16.0,
    bottom: 14.0,
};

/// Window widths of the review (A to E), points.
const WIDTHS: [f32; 4] = [1100.0, 1600.0, 2400.0, 3440.0];

/// The labels of the grid, as the page words them.
const GRID_LABELS: [&str; 6] = [
    "Short RF staging directory",
    "Open-core words (OCMEM)",
    "Native OpenFOAM project directory",
    "Navigation-data directory",
    "Fetch timeout",
    "SimBrief username",
];

const CHIP_TEXTS: [&str; 5] = ["Ready", "Not found", "Check path", "Not set", "Configured"];

/// Render the page for `frames` frames in a window `width` wide and a tall
/// one (the page scrolls; the height reveals every card), until the status
/// scan has produced its chips.
fn render_tools(width: f32) -> FullOutput {
    let mut state = AppState::default();
    state.config_values["structures"]["nastran95_open_core_words"] = serde_json::json!("1500000");
    let ctx = Context::default();
    alas_gui::apply_theme(alas_gui::AppTheme::Dark, &ctx);
    let started = std::time::Instant::now();
    let mut time = 0.0;
    loop {
        time += 0.05;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 4200.0))),
            time: Some(time),
            ..RawInput::default()
        };
        let output = ctx.run(input, |ctx| {
            egui::CentralPanel::default()
                .frame(
                    egui::Frame::central_panel(ctx.style().as_ref()).inner_margin(CONTENT_MARGIN),
                )
                .show(ctx, |ui| show_tools_view(&mut state, ui));
        });
        let chips = painted_texts(&output)
            .iter()
            .filter(|(text, _)| CHIP_TEXTS.contains(&text.as_str()))
            .count();
        // Two settled frames after the scan, or give up after a minute.
        if (chips > 0 && time > 1.0) || started.elapsed().as_secs() > 60 {
            assert!(chips > 0, "the tool scan produced no status chips");
            return output;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// Every painted string with the position of its top-left corner. The text is
/// what the rows really show, so an elided label reads with its ellipsis.
fn painted_texts(output: &FullOutput) -> Vec<(String, Pos2)> {
    fn walk(shape: &Shape, found: &mut Vec<(String, Pos2)>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, found)),
            Shape::Text(text) => {
                let shown: String = text.galley.rows.iter().map(|row| row.text()).collect();
                let shown = shown.trim().to_owned();
                if !shown.is_empty() {
                    found.push((shown, text.pos + text.galley.rect.min.to_vec2()));
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut found);
    }
    found
}

/// The rectangles of the page's value boxes (text boxes and the fetch
/// timeout): input-high and at least 250 points wide (wider than any combo box).
fn text_boxes(output: &FullOutput) -> Vec<Rect> {
    fn walk(shape: &Shape, found: &mut Vec<Rect>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, found)),
            Shape::Rect(rect)
                if rect.fill != egui::Color32::TRANSPARENT
                    && rect.rect.width() >= 250.0
                    && (28.0..40.0).contains(&rect.rect.height()) =>
            {
                found.push(rect.rect);
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut found);
    }
    found
}

fn squashed(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn columns_of(width: f32) -> usize {
    // The page puts cards in two columns from 1100 points of content width.
    if width - 44.0 >= 1100.0 {
        2
    } else {
        1
    }
}

#[test]
fn field_boxes_start_at_one_x_per_column_and_share_a_width() {
    for width in WIDTHS {
        let output = render_tools(width);
        let boxes = text_boxes(&output);
        assert!(
            boxes.len() >= 10,
            "{width}: only {} text boxes",
            boxes.len()
        );
        let mut lefts: Vec<f32> = boxes
            .iter()
            .map(|rect| (rect.left() * 2.0).round() / 2.0)
            .collect();
        lefts.sort_by(f32::total_cmp);
        lefts.dedup();
        assert!(
            lefts.len() <= columns_of(width),
            "{width}: field boxes start at {lefts:?}, one x per column expected"
        );
        let mut widths: Vec<f32> = boxes.iter().map(|rect| rect.width().round()).collect();
        widths.sort_by(f32::total_cmp);
        widths.dedup();
        assert!(
            widths.len() == 1,
            "{width}: field widths {widths:?} differ across cards"
        );
    }
}

#[test]
fn grid_labels_are_never_elided() {
    for width in WIDTHS {
        let output = render_tools(width);
        let texts = painted_texts(&output);
        for label in GRID_LABELS {
            let wanted = squashed(&alas_i18n::t(Some(label), None));
            assert!(
                texts.iter().any(|(text, _)| squashed(text) == wanted),
                "{width}: label {label:?} is missing or elided; painted labels: {:?}",
                texts
                    .iter()
                    .filter(|(text, _)| text.contains("...") || text.contains('\u{2026}'))
                    .map(|(text, _)| text.clone())
                    .collect::<Vec<_>>()
            );
        }
    }
}

/// The outline rectangles of the status chips: stroked, unfilled, rounded.
fn chip_frames(output: &FullOutput) -> Vec<Rect> {
    fn walk(shape: &Shape, found: &mut Vec<Rect>) {
        match shape {
            Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, found)),
            Shape::Rect(rect)
                if rect.fill == egui::Color32::TRANSPARENT
                    && rect.rounding.nw >= 7.5
                    && rect.rect.height() < 30.0
                    && rect.rect.width() > 30.0 =>
            {
                found.push(rect.rect);
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    for clipped in &output.shapes {
        walk(&clipped.shape, &mut found);
    }
    found
}

#[test]
fn status_badges_share_one_width_and_one_x_per_row_group() {
    for width in WIDTHS {
        let output = render_tools(width);
        let chips = chip_frames(&output);
        assert!(chips.len() >= 10, "{width}: only {} badges", chips.len());
        let mut widths: Vec<f32> = chips.iter().map(|rect| rect.width().round()).collect();
        widths.sort_by(f32::total_cmp);
        widths.dedup();
        assert_eq!(widths.len(), 1, "{width}: badge widths {widths:?}");
        // One x for the overview rows and one for the card headers, per column.
        let mut lefts: Vec<f32> = chips.iter().map(|rect| rect.left().round()).collect();
        lefts.sort_by(f32::total_cmp);
        lefts.dedup();
        assert!(
            lefts.len() <= 2 * columns_of(width),
            "{width}: badge x positions {lefts:?}"
        );
    }
}

#[test]
fn every_card_sits_inside_the_window() {
    for width in WIDTHS {
        let output = render_tools(width);
        let mut right = 0.0_f32;
        fn walk(shape: &Shape, right: &mut f32) {
            match shape {
                Shape::Vec(shapes) => shapes.iter().for_each(|shape| walk(shape, right)),
                Shape::Rect(rect) if rect.rect.width() > 10.0 && rect.rect.right().is_finite() => {
                    *right = right.max(rect.rect.right());
                }
                _ => {}
            }
        }
        for clipped in &output.shapes {
            walk(&clipped.shape, &mut right);
        }
        assert!(right <= width + 0.5, "{width}: a box reaches x = {right}");
        // No text is cut by the window's right edge either.
        for (text, pos) in painted_texts(&output) {
            assert!(pos.x < width, "{width}: {text:?} starts at {}", pos.x);
        }
    }
}
