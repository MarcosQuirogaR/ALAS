// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Uniform cell height for multi-column schema forms.
//!
//! The form places each field in the currently shortest column. That keeps
//! tall editors from stalling later fields, but single-line editors of
//! slightly different natural heights then drift apart row by row. Use a
//! style-derived minimum, never a maximum measured across unrelated forms:
//! a wrapping checkbox must only grow its own cell.

use alas_config::Kind;

/// Single-line editors, which share one cell height in a multi-column form.
pub(super) fn uniform_height_kind(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::Bool | Kind::Int | Kind::Float | Kind::WeightSlider | Kind::Str | Kind::Optional
    )
}

/// One label row, its gap, and one editor row. It follows local font/theme
/// changes immediately, without retaining sizes from previous pages/frames.
pub(super) fn uniform_leaf_height(ui: &egui::Ui) -> f32 {
    let spacing = ui.spacing();
    let editor_height = spacing
        .interact_size
        .y
        .max(ui.text_style_height(&egui::TextStyle::Body) + 2.0 * spacing.button_padding.y);
    spacing.interact_size.y + spacing.item_spacing.y + editor_height
}
