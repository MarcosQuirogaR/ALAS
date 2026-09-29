// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Propulsion inputs: automatic summary and manual motor and propeller data.

use egui::{TextEdit, Ui};

use crate::theme::selectable_button;
use crate::uav::{ComponentRole, PropulsionInputMode, UavWorkflowState};

use super::super::presentation::{card, card_column_count};
use super::super::uav_fields::{integer, value};
use super::form_grid;
use crate::views::tr;

pub(in crate::views::uav_view) fn propulsion_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    card(
        ui,
        "Propulsion map evidence",
        Some(
            "The normal path resolves thrust, motor current, battery current, and power from the selected battery, motor, ESC, and APC performance table.",
        ),
        |ui| {
            propulsion_mode_tabs(state, ui);
            ui.add_space(6.0);
            form_grid(ui, "uav_propulsion_count_grid", |ui| {
                integer(ui, "Installed motor count", &mut state.propulsion_motor_count);
            });
        },
    );
    ui.add_space(8.0);
    match state.propulsion_input_mode {
        PropulsionInputMode::Automatic => automatic_propulsion_summary(state, ui),
        PropulsionInputMode::AdvancedManual => manual_propulsion_inputs(state, ui),
    }
}

fn propulsion_mode_tabs(state: &mut UavWorkflowState, ui: &mut Ui) {
    let modes = [
        PropulsionInputMode::Automatic,
        PropulsionInputMode::AdvancedManual,
    ];
    let columns = card_column_count(ui.available_width()).min(modes.len());
    for row in modes.chunks(columns) {
        ui.columns(columns, |column_uis| {
            for (index, mode) in row.iter().enumerate() {
                if column_uis[index]
                    .add_sized(
                        [column_uis[index].available_width(), 30.0],
                        selectable_button(tr(mode.label()), state.propulsion_input_mode == *mode),
                    )
                    .clicked()
                {
                    state.propulsion_input_mode = *mode;
                }
            }
        });
        ui.add_space(4.0);
    }
}

fn automatic_propulsion_summary(state: &UavWorkflowState, ui: &mut Ui) {
    card(ui, "Required hardware", Some("One selected battery feeds every identical propulsor. Per-motor ratings remain per motor; pack current, mass, thrust, and BOM quantities are summed across the installed count."), |ui| {
        ui.colored_label(
            ui.visuals().warn_fg_color,
            tr("APC source bounds: speed and RPM"),
        )
        .on_hover_text(tr("The solver stays inside each APC table's speed and RPM bounds. A combination without enough source evidence stops with a typed error rather than an invented curve."));
        ui.add_space(4.0);
        for role in [
            ComponentRole::Battery,
            ComponentRole::Motor,
            ComponentRole::Esc,
            ComponentRole::Propeller,
        ] {
            if let Some(record) = state.selected_record(role) {
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!("{}: {}", tr(role.label()), record.model));
                    ui.hyperlink_to(tr("Open evidence"), &record.provenance.source_url);
                });
            }
        }
    });
}

fn manual_propulsion_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    ui.colored_label(
        ui.visuals().warn_fg_color,
        tr("Manual fallback: measured or solver-derived points only"),
    )
    .on_hover_text(tr("Advanced fallback: enter measured or solver-derived points only when the automatic source-bounded model is not the required evidence. A retail static-thrust value is not accepted for cruise."));
    ui.add_space(8.0);
    card(ui, "Propulsion data source or test ID", None, |ui| {
        ui.add_sized(
            [ui.available_width(), ui.spacing().interact_size.y],
            TextEdit::singleline(&mut state.propulsion_evidence)
                .hint_text(tr("Example: dyno-2026-03 or solver-case-17")),
        )
        .on_hover_text(tr(
            "Enter a test ID, report name, solver case, or source URL that supports both operating points.",
        ));
        ui.add_space(6.0);
        form_grid(ui, "uav_propulsion_manual_grid", |ui| {
            integer(
                ui,
                "Battery series cells",
                &mut state.propulsion_series_cells,
            );
        });
    });
    ui.add_space(8.0);
    if card_column_count(ui.available_width()) == 2 {
        ui.columns(2, |columns| {
            operating_point_card(state, &mut columns[0], true);
            operating_point_card(state, &mut columns[1], false);
        });
    } else {
        operating_point_card(state, ui, true);
        ui.add_space(8.0);
        operating_point_card(state, ui, false);
    }
    ui.add_space(6.0);
    ui.weak(tr("Manual fallback: two-point energy surrogate"))
        .on_hover_text(tr("The manual fallback retains the historic two-point energy surrogate; use the automatic catalogue solver for the multi-phase electrical simulation."));
}

fn operating_point_card(state: &mut UavWorkflowState, ui: &mut Ui, low_speed: bool) {
    let (title, description) = if low_speed {
        (
            "Low-speed operating point",
            "Use the speed entered as Maximum stall speed in Mission; static thrust alone is not enough.",
        )
    } else {
        (
            "Cruise operating point",
            "Use the cruise speed entered in Mission.",
        )
    };
    card(ui, title, Some(description), |ui| {
        form_grid(
            ui,
            if low_speed {
                "uav_low_speed_propulsion_grid"
            } else {
                "uav_cruise_propulsion_grid"
            },
            |ui| {
                if low_speed {
                    value(ui, "Low-speed thrust", &mut state.stall_thrust_n, "N");
                    value(ui, "Low-speed current", &mut state.stall_current_a, "A");
                    value(ui, "Low-speed power", &mut state.stall_power_w, "W");
                } else {
                    value(ui, "Cruise thrust", &mut state.cruise_thrust_n, "N");
                    value(ui, "Cruise current", &mut state.cruise_current_a, "A");
                    value(ui, "Cruise power", &mut state.cruise_power_w, "W");
                }
            },
        );
    });
}
