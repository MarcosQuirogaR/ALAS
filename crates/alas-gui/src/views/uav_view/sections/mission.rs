// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Competition, search and mission-requirement inputs.

use egui::{RichText, Ui};

use crate::theme::selectable_button;
use crate::uav::UavWorkflowState;

use super::super::presentation::{card, card_column_count};
use super::super::uav_fields::{seed_value, usize_value, value};
use crate::views::form::grid_form;
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
            crate::views::form::label_row(
                ui,
                RichText::new(tr("Competition preset")).strong(),
                |_| {},
            );
            // The presets stand on the form grid, one button per cell.
            grid_form(ui, "uav_competition_presets", |grid, ui| {
                for id in alas_uav::score::presets::ids() {
                    let name = alas_uav::score::presets::load(id)
                        .map_or_else(|_| (*id).to_owned(), |preset| preset.name);
                    let selected = state.competition.preset_id == *id;
                    grid.cell(ui, |ui, width| {
                        let height = crate::theme::control_height(ui);
                        let button = selectable_button(RichText::new(name), selected);
                        if ui.add_sized([width, height], button).clicked() {
                            state.competition.preset_id = (*id).to_owned();
                        }
                    });
                }
            });
            grid_form(ui, "uav_competition_grid", |grid, ui| {
                value(
                    grid,
                    ui,
                    "Announced take-off distance",
                    &mut state.competition.announced_takeoff_m,
                    "m",
                );
                value(
                    grid,
                    ui,
                    "Payload count (ACC cans gate)",
                    &mut state.competition.payload_count,
                    "",
                );
                value(
                    grid,
                    ui,
                    "Payload volume (XC gate)",
                    &mut state.competition.payload_volume_m3,
                    "m3",
                );
                usize_value(grid, ui, "Evaluation budget", &mut state.evaluations);
                seed_value(grid, ui, "Random seed", &mut state.seed);
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
            grid_form(ui, "uav_mission_grid", |grid, ui| {
                value(
                    grid,
                    ui,
                    "Endurance",
                    &mut state.objectives.endurance_s,
                    "s",
                );
                value(grid, ui, "Range", &mut state.objectives.range_m, "m");
                value(
                    grid,
                    ui,
                    "Cruise speed",
                    &mut state.objectives.cruise_speed_m_s,
                    "m/s",
                );
                value(
                    grid,
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
        grid_form(ui, "uav_payload_grid", |grid, ui| {
            value(
                grid,
                ui,
                "Payload mass",
                &mut state.objectives.payload_mass_kg,
                "kg",
            );
            value(
                grid,
                ui,
                "Payload length",
                &mut state.objectives.payload_dimensions.length_m,
                "m",
            );
            value(
                grid,
                ui,
                "Payload width",
                &mut state.objectives.payload_dimensions.width_m,
                "m",
            );
            value(
                grid,
                ui,
                "Payload height",
                &mut state.objectives.payload_dimensions.height_m,
                "m",
            );
        });
        let title = crate::theme::section_title(ui, tr("Ranking policy"));
        crate::theme::header_label(ui, title);
        grid_form(ui, "uav_ranking_grid", |grid, ui| {
            value(
                grid,
                ui,
                "Minimum propulsive efficiency",
                &mut state.objectives.minimum_propulsive_efficiency,
                "",
            );
            value(
                grid,
                ui,
                "Efficiency ranking weight",
                &mut state.objectives.efficiency_priority,
                "",
            );
        });
    });
}
