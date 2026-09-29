// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The generic form, rendered straight from a configuration group's schema.
//!
//! It walks the [`Field`]
//! list the derive macro emits and edits the matching `serde_json::Value` in
//! place, so every group's every field appears with zero per-field code and a
//! new field on the model side needs none here. Field prose is looked up in the
//! active language the way the schema documents.

mod cell_height;
mod editors;
mod feedback;
mod field;
mod layout;
mod options;
#[cfg(test)]
mod tests;

pub(crate) use options::display_unit;

use alas_config::Field;
use egui::Ui;
use layout::render_fields;
use serde_json::Value;
use std::collections::HashSet;

/// The log-slider mapping a `weight_slider` field uses: a 0.001..1000
/// logarithmic range.
const WEIGHT_MIN: f64 = 0.001;
const WEIGHT_MAX: f64 = 1000.0;

/// The reset affordance of a modified field: U+21BA ANTICLOCKWISE OPEN CIRCLE
/// ARROW, which the bundled default fonts cover (asserted in `form_tests`).
const RESET_GLYPH: &str = "\u{21ba}";

/// The library menu beside an editable option field: U+23F7 BLACK MEDIUM
/// DOWN-POINTING TRIANGLE, also covered by the bundled fonts.
const MENU_GLYPH: &str = "\u{23f7}";

/// The narrowest readable form column and adaptive label bounds.
const MIN_FORM_COLUMN_WIDTH: f32 = 300.0;
const MIN_LABEL_TEXT_SIZE: f32 = 11.0;
const MAX_LABEL_TEXT_SIZE: f32 = 15.0;

/// One localized schema field whose displayed value changed this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormEdit {
    /// Human-readable, localized schema label.
    pub label: String,
    /// Final rendered value, including a declared unit where applicable.
    pub value: String,
}

/// Render a form for `fields`, editing `values` in place and returning each
/// value changed this frame.
pub fn dynamic_form(
    ui: &mut Ui,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
) -> Vec<FormEdit> {
    dynamic_form_with_open_root_nodes(ui, fields, values, error_fields, lang, show_help, false)
}

/// Render a dynamic form whose direct child groups start open. This is used
/// sparingly for pages whose two root nodes are the primary navigation (for
/// example, optimizer weights and solver settings); nested groups retain the
/// compact default so opening a page never reveals an entire tree at once.
pub fn dynamic_form_with_open_root_nodes(
    ui: &mut Ui,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    open_root_nodes: bool,
) -> Vec<FormEdit> {
    let mut edits = Vec::new();
    render_fields(
        ui,
        fields,
        values,
        error_fields,
        lang,
        show_help,
        open_root_nodes,
        "",
        &[],
        &mut edits,
    );
    edits
}
