// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The engine installation card of the Propulsion page.

use alas_config::Field;
use egui::Ui;

use crate::state::AppState;
use crate::views::form::{dynamic_form, FormEdit};
use crate::views::tr_label;

pub(super) fn render_installation_card(
    state: &mut AppState,
    ui: &mut Ui,
    edits: &mut Vec<FormEdit>,
    error_fields: &std::collections::HashSet<String>,
    lang: Option<&str>,
    show_help: bool,
) {
    let installation_names = [
        "nacelle_profile",
        "radius_scale_m",
        "spanwise_positions_m",
        "z_m",
        "inlet_x_offset_m",
    ];
    // The card is the heading: Advanced detail lists every field in it rather
    // than behind a third-level "Advanced (n)" accordion.
    let flatten = crate::detail::published(ui.ctx()) != Some(crate::detail::Detail::Basic);
    let fields: Vec<Field> = alas_config::ConfigNode::schema(&alas_config::EngineConfig::default())
        .fields
        .into_iter()
        .filter(|field| installation_names.contains(&field.name))
        .map(|mut field| {
            field.advanced &= !flatten;
            field
        })
        .collect();
    let locked = state.manual_geometry_locked();
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        let title = crate::theme::card_title(ui, tr_label("Installation & nacelle"));
        let header = egui::CollapsingHeader::new(title)
            .id_salt("propulsion::installation")
            .default_open(true)
            .open(crate::perf_tour::force_open());
        crate::theme::show_header(ui, header, |ui| {
            if locked {
                super::preset_lock_row(ui);
            }
            crate::views::form::with_locked_editors(ui, locked, |ui| {
                if let Some(values) = state.config_values.pointer_mut("/geometry/engine") {
                    edits.extend(crate::views::form::with_baseline_group(
                        "geometry.engine",
                        || dynamic_form(ui, &fields, values, error_fields, lang, show_help),
                    ));
                }
            });
        });
    });
}
