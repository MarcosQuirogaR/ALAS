// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The "Mesh resolution" group of the Analyses settings: the vortex-lattice
//! panel density of the wing and of the tail, and the spanwise multiplier.
//!
//! These three describe how finely an analysis meshes the aircraft, not the
//! aircraft's shape, so they sit together here and are removed from the
//! Geometry page and from the analysis form's own mesh card (see
//! `placement::relocated_paths`). Each is still read and written at its own
//! configuration path (`geometry.wing.n_subdivisions`,
//! `geometry.empennage.n_subdivisions`, `analysis.spanwise_resolution`), so
//! saved files and the solver see no change.

use std::collections::HashSet;

use alas_config::Field;
use egui::Ui;

use super::placement::{cached_section_fields, node_fields_at};
use crate::state::AppState;
use crate::views::form::{dynamic_form, dynamic_form_at, FormEdit};
use crate::views::tr_label;

/// The Mesh resolution group of the Analysis fidelity page, as (schema node
/// pointer, dotted node path, field name): the page draws these fields itself
/// (see `mesh_resolution`), so the search index lists them there.
pub(crate) const MESH_FIELDS: &[(&str, &str, &str)] = &[
    ("/geometry/wing", "geometry.wing", "n_subdivisions"),
    (
        "/geometry/empennage",
        "geometry.empennage",
        "n_subdivisions",
    ),
    ("/analysis", "analysis", "spanwise_resolution"),
];

/// Pick the named leaf fields of the schema node at `path`, cached.
fn schema_fields(
    state: &AppState,
    owner: &str,
    path: &str,
    names: &'static [&'static str],
) -> std::rc::Rc<Vec<Field>> {
    let identity = (
        state.schema.fields.as_ptr() as usize,
        state.schema.fields.len(),
    );
    cached_section_fields("mesh_resolution", owner, identity, || {
        node_fields_at(&state.schema, path)
            .unwrap_or_default()
            .into_iter()
            .filter(|field| names.contains(&field.name))
            .collect()
    })
}

/// Render the group. The panel counts belong to the geometry a registered
/// aircraft protects, so on a reference aircraft they are shown locked.
pub(super) fn render(state: &mut AppState, ui: &mut Ui, lang: Option<&str>) {
    let wing = schema_fields(state, "wing", "/geometry/wing", &["n_subdivisions"]);
    let tail = schema_fields(state, "tail", "/geometry/empennage", &["n_subdivisions"]);
    let spanwise = schema_fields(state, "spanwise", "/analysis", &["spanwise_resolution"]);
    let locked = state.manual_geometry_locked();
    let no_errors = HashSet::<String>::new();
    let mut edits: Vec<FormEdit> = Vec::new();
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        // A search jump to one of the three counts opens the group.
        let forced = MESH_FIELDS.iter().any(|(_, prefix, name)| {
            crate::search::focus_wants(ui.ctx(), &format!("{prefix}.{name}"))
        });
        let mut header =
            egui::CollapsingHeader::new(crate::theme::card_title(ui, tr_label("Mesh resolution")))
                .id_salt("analysis::mesh_resolution")
                .default_open(true)
                .open(crate::perf_tour::force_open());
        if forced {
            header = header.open(Some(true));
        }
        crate::theme::show_header(ui, header, |ui| {
            if locked {
                super::preset_lock_row(ui);
            }
            // The three counts come from three groups but stand in one grid:
            // one cell each, captions and boxes level across the row.
            let mut grid = crate::views::form::CellGrid::new(ui, "mesh_resolution_grid");
            grid.cell(ui, |ui, _| {
                crate::views::form::with_locked_editors(ui, locked, |ui| {
                    if let Some(values) = state.config_values.pointer_mut("/geometry/wing") {
                        edits.extend(crate::views::form::with_baseline_group("geometry", || {
                            dynamic_form_at(ui, &wing, values, &no_errors, lang, "::wing")
                        }));
                    }
                });
            });
            grid.cell(ui, |ui, _| {
                crate::views::form::with_locked_editors(ui, locked, |ui| {
                    if let Some(values) = state.config_values.pointer_mut("/geometry/empennage") {
                        edits.extend(crate::views::form::with_baseline_group("geometry", || {
                            dynamic_form_at(ui, &tail, values, &no_errors, lang, "::empennage")
                        }));
                    }
                });
            });
            grid.cell(ui, |ui, _| {
                if let Some(values) = state.group_mut("analysis") {
                    edits.extend(crate::views::form::with_baseline_group("analysis", || {
                        dynamic_form(ui, &spanwise, values, &no_errors, lang, false)
                    }));
                }
            });
            grid.finish(ui);
        });
    });
    if !edits.is_empty() {
        state.on_config_modified();
        for edit in edits {
            state.note_parameter_modified(edit.label, edit.value);
        }
    }
}
