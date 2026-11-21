// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The generic form, rendered straight from a configuration group's schema.
//!
//! It walks the [`Field`]
//! list the derive macro emits and edits the matching `serde_json::Value` in
//! place, so every group's every field appears with zero per-field code and a
//! new field on the model side needs none here. Field prose is looked up in the
//! active language the way the schema documents.

mod airfoil_combo;
mod baseline;
mod caption;
mod cell_grid;
#[cfg(test)]
mod detail_tests;
mod display_scale;
mod editors;
mod feedback;
mod field;
mod help;
mod layout;
mod number_text;
mod options;
mod pack;
mod table_editors;
#[cfg(test)]
mod tests;
mod text_cache;
mod unit_display;
mod value_table;
mod visibility;

pub(crate) use airfoil_combo::take_import_request as take_airfoil_import_request;
pub(crate) use baseline::{publish as publish_baseline, with_group as with_baseline_group};
pub(crate) use cell_grid::{drag_box, drag_cell, grid_form, label_row, CellGrid};
#[cfg(test)]
pub(crate) use feedback::MODIFIED_DOT;
pub(crate) use help::DOCUMENTATION_URL;
pub(crate) use unit_display::display_unit;
pub(crate) use value_table::{value_table, Column as TableColumn, Value as TableValue};
pub(crate) use visibility::publish as publish_visibility_context;
pub(crate) use visibility::published_value;

use alas_config::Field;
use egui::Ui;
use layout::render_fields;
pub(crate) use pack::{equalise, flow_cards, leaf_columns};
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
#[cfg_attr(not(test), allow(dead_code))]
const MENU_GLYPH: &str = "\u{23f7}";

/// The narrowest readable form column and adaptive label bounds.
const MIN_FORM_COLUMN_WIDTH: f32 = 300.0;

/// Most columns of a form grid: wide windows flow fields into more columns.
const MAX_FORM_COLUMNS: usize = 8;

/// One localized schema field whose displayed value changed this frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormEdit {
    /// Human-readable, localized schema label.
    pub label: String,
    /// Final rendered value, including a declared unit where applicable.
    pub value: String,
}

fn editors_locked_key() -> egui::Id {
    egui::Id::new("alas_form_editors_locked")
}

/// Whether the schema editors drawn now are locked by an enclosing
/// [`with_locked_editors`].
pub(crate) fn editors_locked(ctx: &egui::Context) -> bool {
    ctx.data(|data| data.get_temp::<bool>(editors_locked_key()))
        .unwrap_or(false)
}

/// Draw `add` with every schema value editor inside it disabled when `locked`.
/// Unlike `Ui::add_enabled_ui`, collapsing headers and their cards stay
/// clickable, so a locked (read-only) page can still be browsed: only the
/// values cannot be edited.
pub(crate) fn with_locked_editors<R>(
    ui: &mut Ui,
    locked: bool,
    add: impl FnOnce(&mut Ui) -> R,
) -> R {
    let previous = editors_locked(ui.ctx());
    ui.ctx()
        .data_mut(|data| data.insert_temp(editors_locked_key(), previous || locked));
    let result = add(ui);
    ui.ctx()
        .data_mut(|data| data.insert_temp(editors_locked_key(), previous));
    result
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

/// Render a dynamic form for the fields at `id_prefix` below their group, in the
/// renderers' `::` path form (for example `::wing`), so a search jump can find them.
pub fn dynamic_form_at(
    ui: &mut Ui,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    id_prefix: &str,
) -> Vec<FormEdit> {
    dynamic_form_at_help(ui, fields, values, error_fields, lang, false, id_prefix)
}

/// [`dynamic_form_at`] with the help-text switch of [`dynamic_form`].
pub fn dynamic_form_at_help(
    ui: &mut Ui,
    fields: &[Field],
    values: &mut Value,
    error_fields: &HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
    id_prefix: &str,
) -> Vec<FormEdit> {
    let mut edits = Vec::new();
    render_fields(
        ui,
        fields,
        values,
        error_fields,
        lang,
        show_help,
        false,
        id_prefix,
        &[],
        &mut edits,
    );
    edits
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
