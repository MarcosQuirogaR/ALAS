// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

use super::*;

/// Return each line-box center relative to a multiline text anchor.
///
/// Backends use the same centers so top, middle, and bottom alignment occupy
/// identical bounds in exported and interactive figures.
pub fn text_line_center_offsets(
    line_count: usize,
    line_height: f64,
    baseline: TextBaseline,
) -> Vec<f64> {
    let count = line_count.max(1);
    let block_height = count as f64 * line_height;
    let first = match baseline {
        TextBaseline::Top => line_height * 0.5,
        TextBaseline::Middle => -block_height * 0.5 + line_height * 0.5,
        TextBaseline::Bottom => -block_height + line_height * 0.5,
    };
    (0..count)
        .map(|index| first + index as f64 * line_height)
        .collect()
}

/// Upper bound on the mean advance of one character of proportional
/// sans-serif text, in em, used by backends without glyph metrics.
///
/// Uppercase-heavy DejaVu Sans averages about 0.72 em per character and
/// Arial or the egui proportional face noticeably less, so a line budgeted
/// with this figure stays inside its box for anything short of a run of the
/// widest glyphs.
pub const CONSERVATIVE_ADVANCE_EM: f64 = 0.75;

/// Characters that fit in `width_px` at `font_size_pt` under
/// [`CONSERVATIVE_ADVANCE_EM`], never fewer than one.
pub fn conservative_char_budget(font_size_pt: f64, width_px: f64) -> usize {
    let advance = font_size_pt.max(0.1) * CSS_PIXELS_PER_POINT * CONSERVATIVE_ADVANCE_EM;
    (width_px / advance).floor().max(1.0) as usize
}

/// Wrap `text` so that no line exceeds the conservative character budget for
/// `width_px` at `font_size_pt`.
///
/// Words are packed greedily. A single word longer than the budget (a path,
/// an identifier, a solver token) is split across lines instead of being left
/// to overflow, so every character stays visible. Caller line breaks are kept
/// as paragraph breaks.
pub fn wrap_text_to_width(text: &str, font_size_pt: f64, width_px: f64) -> String {
    let budget = conservative_char_budget(font_size_pt, width_px);
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut current = String::new();
        for word in paragraph.split_whitespace() {
            let word_len = word.chars().count();
            if word_len > budget {
                if !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                }
                let mut remaining = word;
                while remaining.chars().count() > budget {
                    let split_at = remaining
                        .char_indices()
                        .nth(budget)
                        .map_or(remaining.len(), |(index, _)| index);
                    let (chunk, rest) = remaining.split_at(split_at);
                    lines.push(chunk.to_owned());
                    remaining = rest;
                }
                current.push_str(remaining);
                continue;
            }
            let candidate_len = if current.is_empty() {
                word_len
            } else {
                current.chars().count() + 1 + word_len
            };
            if candidate_len > budget && !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
            if !current.is_empty() {
                current.push(' ');
            }
            current.push_str(word);
        }
        lines.push(current);
    }
    lines.join("\n")
}

/// Height in scene pixels that a [`SceneElement::TextBlock`] needs on a
/// metric-free backend: the conservative line count times the shared line
/// advance.
pub fn text_block_height(text: &str, font_size_pt: f64, width_px: f64) -> f64 {
    let lines = wrap_text_to_width(text, font_size_pt, width_px)
        .lines()
        .count()
        .max(1);
    lines as f64 * font_size_pt * CSS_PIXELS_PER_POINT * TEXT_LINE_HEIGHT_EM
}
