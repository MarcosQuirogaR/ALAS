// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The parameter-sweep card of the airfoil CFD study tab.

use super::layout::field;
use super::widgets::card_title;
use crate::cfd::CfdSweepVariable;
use crate::state::{AppState, LogKind};
use crate::views::{tr, tr_fields};
use egui::{ComboBox, DragValue, RichText, Ui};

pub(super) fn show_sweep_card(state: &mut AppState, ui: &mut Ui) {
    let before = state.cfd.sweep_settings.clone();
    crate::theme::card_frame(ui).show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        card_title(
            ui,
            "Sequential AoA / Reynolds sweep",
            "Run one isolated OpenFOAM case per point after single-case verification. Each point keeps its own inputs, status, case path and numerical evidence.",
        );
        let sweep = &mut state.cfd.sweep_settings;
        let speed = if sweep.variable == CfdSweepVariable::Reynolds { 1.0e4 } else { 0.5 };
        let unit = sweep.variable.unit().to_owned();
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(tr("Sweep variable")).weak());
            ComboBox::from_id_salt("airfoil_cfd_sweep_variable")
.icon(crate::combo_icon::down_arrow)
                .width(150.0)
                .selected_text(tr(sweep.variable.label()))
                .show_ui(ui, |ui| {
                    for variable in [CfdSweepVariable::AngleOfAttack, CfdSweepVariable::Reynolds] {
                        ui.selectable_value(&mut sweep.variable, variable, tr(variable.label()));
                    }
                });
            ui.separator();
            for (label, value) in [
                ("Start [{unit}]", &mut sweep.start),
                ("End [{unit}]", &mut sweep.end),
                ("Step [{unit}]", &mut sweep.step),
            ] {
                ui.label(RichText::new(tr_fields(label, &[("unit", unit.clone())])).weak());
                field(ui, DragValue::new(value).speed(speed));
            }
        });
        ui.horizontal_wrapped(|ui| {
            if ui
                .add_enabled(!state.cfd.running, egui::Button::new(tr("Run sweep")))
                .on_hover_text(tr("Run the configured points sequentially in isolated case directories."))
                .on_disabled_hover_text(tr("A CFD run is in progress."))
                .clicked()
            {
                match state.cfd.start_sweep() {
                    Ok(run_id) => state.log(format!("Airfoil CFD sweep #{run_id} started."), LogKind::Info),
                    Err(error) => state.log(error, LogKind::Error),
                }
            }
            if ui
                .add_enabled(
                    state.cfd.running && state.cfd.sweep_running,
                    egui::Button::new(tr("Cancel sweep")),
                )
                .clicked()
            {
                state.cfd.cancel_run();
                state.log("Airfoil CFD sweep cancellation requested.", LogKind::Warn);
            }
            if state.cfd.sweep_running {
                ui.spinner();
            }
            ui.separator();
            match state.cfd.sweep_settings.values() {
                Ok(values) => {
                    ui.label(
                        RichText::new(tr_fields(
                            "{count} cases",
                            &[("count", values.len().to_string())],
                        ))
                        .weak()
                        .small(),
                    )
                    .on_hover_text(tr_fields(
                        "{count} sequential cases will be generated.",
                        &[("count", values.len().to_string())],
                    ));
                }
                Err(error) => {
                    ui.colored_label(ui.visuals().error_fg_color, tr(&error));
                }
            }
        });
    });
    if state.cfd.sweep_settings != before {
        state.cfd.mark_inputs_changed();
    }
}
