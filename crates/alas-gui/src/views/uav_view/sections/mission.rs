// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Competition, search and mission-requirement inputs.

use egui::{DragValue, RichText, Ui};

use crate::theme::selectable_button;
use crate::uav::UavWorkflowState;

use super::super::presentation::{card, card_column_count};
use super::super::uav_fields::{usize_value, value};
use super::form_grid;
use crate::views::tr;

pub(in crate::views::uav_view) fn mission_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    competition_card(state, ui);
    ui.add_space(8.0);
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
}

fn competition_card(state: &mut UavWorkflowState, ui: &mut Ui) {
    card(
        ui,
        "Competition",
        Some("The search chooses the propulsion bundle and the airframe, flies the preset's mission and scores it with the preset's own score engine."),
        |ui| {
            ui.label(RichText::new(tr("Competition preset")).strong());
            let columns = card_column_count(ui.available_width()).min(2);
            let ids = alas_uav::score::presets::ids();
            for row in ids.chunks(columns) {
                ui.columns(columns, |column_uis| {
                    for (index, id) in row.iter().enumerate() {
                        let name = alas_uav::score::presets::load(id)
                            .map_or_else(|_| (*id).to_owned(), |preset| preset.name);
                        let selected = state.competition.preset_id == *id;
                        let response = column_uis[index].add_sized(
                            [column_uis[index].available_width(), 30.0],
                            selectable_button(RichText::new(name), selected),
                        );
                        if response.clicked() {
                            state.competition.preset_id = (*id).to_owned();
                        }
                    }
                });
                ui.add_space(4.0);
            }
            ui.add_space(6.0);
            form_grid(ui, "uav_competition_grid", |ui| {
                value(
                    ui,
                    "Announced take-off distance",
                    &mut state.competition.announced_takeoff_m,
                    "m",
                );
                value(
                    ui,
                    "Payload count (ACC cans gate)",
                    &mut state.competition.payload_count,
                    "",
                );
                value(
                    ui,
                    "Payload volume (XC gate)",
                    &mut state.competition.payload_volume_m3,
                    "m3",
                );
                usize_value(ui, "Evaluation budget", &mut state.evaluations);
                seed_value(ui, "Random seed", &mut state.seed);
            });
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

/// Editable integer seed of the deterministic search.
fn seed_value(ui: &mut Ui, label: &str, seed: &mut u64) {
    ui.label(RichText::new(tr(label)).strong());
    ui.add(DragValue::new(seed).range(0..=u64::MAX));
    ui.end_row();
}
