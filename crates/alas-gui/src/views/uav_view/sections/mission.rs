// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission requirement and payload inputs.

use egui::{RichText, TextEdit, Ui};

use crate::theme::selectable_button;
use crate::uav::{MissionPlanMode, UavWorkflowState};

use super::super::presentation::{card, card_column_count};
use super::super::uav_fields::value;
use super::form_grid;
use crate::views::tr;

pub(in crate::views::uav_view) fn mission_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    if card_column_count(ui.available_width()) == 2 {
        ui.columns(2, |columns| {
            mission_card(state, &mut columns[0]);
            payload_card(state, &mut columns[1]);
        });
    } else {
        mission_card(state, ui);
        ui.add_space(8.0);
        payload_card(state, ui);
    }
    ui.add_space(8.0);
    card(
        ui,
        "Electrical mission plan",
        Some(
            "The normal path derives departure, climb, cruise, and approach-reserve phases from the brief. The cruise duration is extended until both range and endurance are met.",
        ),
        |ui| {
            mission_mode_tabs(state, ui);
            ui.add_space(6.0);
            if state.mission_plan_mode == MissionPlanMode::Advanced {
                advanced_mission_inputs(state, ui);
            }
        },
    );
}

fn mission_card(state: &mut UavWorkflowState, ui: &mut Ui) {
    card(
        ui,
        "Mission objectives",
        Some("These values describe what the aircraft must do; they are not catalogue properties."),
        |ui| {
            form_grid(ui, "uav_mission_grid", |ui| {
                value(ui, "Endurance", &mut state.objectives.endurance_s, "s");
                value(ui, "Range", &mut state.objectives.range_m, "m");
                value(
                    ui,
                    "Cruise speed",
                    &mut state.objectives.cruise_speed_m_s,
                    "m/s",
                );
                value(
                    ui,
                    "Maximum stall speed",
                    &mut state.objectives.maximum_stall_speed_m_s,
                    "m/s",
                );
            });
        },
    );
}

fn payload_card(state: &mut UavWorkflowState, ui: &mut Ui) {
    card(ui, "Payload envelope", None, |ui| {
        form_grid(ui, "uav_payload_grid", |ui| {
            value(
                ui,
                "Payload mass",
                &mut state.objectives.payload_mass_kg,
                "kg",
            );
            value(
                ui,
                "Payload length",
                &mut state.objectives.payload_dimensions.length_m,
                "m",
            );
            value(
                ui,
                "Payload width",
                &mut state.objectives.payload_dimensions.width_m,
                "m",
            );
            value(
                ui,
                "Payload height",
                &mut state.objectives.payload_dimensions.height_m,
                "m",
            );
        });
        ui.add_space(8.0);
        ui.label(RichText::new(tr("Ranking policy")).strong());
        form_grid(ui, "uav_ranking_grid", |ui| {
            value(
                ui,
                "Minimum propulsive efficiency",
                &mut state.objectives.minimum_propulsive_efficiency,
                "",
            );
            value(
                ui,
                "Efficiency ranking weight",
                &mut state.objectives.efficiency_priority,
                "",
            );
        });
    });
}

fn mission_mode_tabs(state: &mut UavWorkflowState, ui: &mut Ui) {
    let modes = [MissionPlanMode::Standard, MissionPlanMode::Advanced];
    let columns = card_column_count(ui.available_width()).min(modes.len());
    for row in modes.chunks(columns) {
        ui.columns(columns, |column_uis| {
            for (index, mode) in row.iter().enumerate() {
                let response = column_uis[index]
                    .add_sized(
                        [column_uis[index].available_width(), 30.0],
                        selectable_button(tr(mode.label()), state.mission_plan_mode == *mode),
                    )
                    .on_hover_text(if *mode == MissionPlanMode::Standard {
                        tr("The standard plan keeps the normal workflow short. Each phase is solved at its own speed and command; no cruise-only energy surrogate is used.")
                    } else {
                        tr("Advanced phases are steady flight conditions. Their total duration and still-air distance must meet the mission objectives.")
                    });
                if response.clicked() {
                    state.mission_plan_mode = *mode;
                    if *mode == MissionPlanMode::Advanced && state.mission_phases.is_empty() {
                        state.reset_advanced_mission();
                    }
                }
            }
        });
        ui.add_space(4.0);
    }
}

fn advanced_mission_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    ui.colored_label(
        ui.visuals().warn_fg_color,
        tr("Duration and distance must meet mission objectives"),
    )
    .on_hover_text(tr("Advanced phases are steady flight conditions. Their total duration and still-air distance must meet the mission objectives."));
    let mut remove = None;
    for (index, phase) in state.mission_phases.iter_mut().enumerate() {
        egui::CollapsingHeader::new(
            RichText::new(format!("{} {}", tr("Phase"), index + 1)).strong(),
        )
        .id_salt(format!("uav_mission_phase_{index}"))
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tr("Phase name")).strong());
                ui.add_sized(
                    [
                        ui.available_width().clamp(140.0, 320.0),
                        ui.spacing().interact_size.y,
                    ],
                    TextEdit::singleline(&mut phase.name),
                );
                if ui.small_button(tr("Remove")).clicked() {
                    remove = Some(index);
                }
            });
            form_grid(ui, format!("uav_mission_phase_values_{index}"), |ui| {
                value(ui, "Duration", &mut phase.duration_s, "s");
                value(ui, "Speed", &mut phase.condition.speed_m_s, "m/s");
                value(
                    ui,
                    "Air density",
                    &mut phase.condition.air_density_kg_m3,
                    "kg/m3",
                );
                value(ui, "Throttle", &mut phase.condition.throttle, "");
            });
        });
        ui.add_space(4.0);
    }
    if let Some(index) = remove {
        state.mission_phases.remove(index);
    }
    ui.horizontal(|ui| {
        if ui.button(tr("Add mission phase")).clicked() {
            state
                .mission_phases
                .push(alas_uav::propulsion_electric::ElectricMissionPhase {
                    name: tr("new phase"),
                    duration_s: 60.0,
                    condition: alas_uav::propulsion_electric::ElectricFlightCondition {
                        speed_m_s: state.objectives.cruise_speed_m_s,
                        air_density_kg_m3: state.model.air_density_kg_m3,
                        throttle: 0.95,
                    },
                });
        }
        if ui.button(tr("Restore standard mission phases")).clicked() {
            state.reset_advanced_mission();
        }
    });
}
