// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airframe controls for the fixed-wing UAV workflow.

use egui::Ui;

use crate::uav::UavWorkflowState;

use super::presentation::{card, card_column_count, collapsing_card};
use super::uav_fields::{bounds_row, integer, seed_value, usize_value, value};
use crate::views::form::grid_form;

pub(super) fn airframe_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    if card_column_count(ui.available_width()) == 2 {
        ui.columns(2, |columns| {
            planform_bounds_card(state, &mut columns[0]);
            aerodynamic_model_card(state, &mut columns[1]);
        });
    } else {
        planform_bounds_card(state, ui);
        ui.add_space(8.0);
        aerodynamic_model_card(state, ui);
    }
    ui.add_space(8.0);
    collapsing_card(
        ui,
        "uav_tail_control",
        "Tail, control, and balance assumptions",
        None,
        false,
        |ui| tail_control_inputs(state, ui),
    );
    ui.add_space(8.0);
    collapsing_card(
        ui,
        "uav_packaging",
        "Packaging and landing gear assumptions",
        None,
        false,
        |ui| packaging_inputs(state, ui),
    );
    ui.add_space(8.0);
    collapsing_card(
        ui,
        "uav_systems",
        "Systems and search controls",
        None,
        false,
        |ui| systems_inputs(state, ui),
    );
}

fn planform_bounds_card(state: &mut UavWorkflowState, ui: &mut Ui) {
    card(
        ui,
        "Planform search bounds",
        Some("These are editable modelling inputs, not aircraft data recovered from the component catalogue."),
        |ui| {
            grid_form(ui, "uav_bounds_grid", |grid, ui| {
                bounds_row(grid, ui, "Wing area", &mut state.geometry_bounds.wing_area_m2, "m2");
                bounds_row(
                    grid,
                    ui,
                    "Wing aspect ratio",
                    &mut state.geometry_bounds.wing_aspect_ratio,
                    "",
                );
                bounds_row(
                    grid,
                    ui,
                    "Fuselage length",
                    &mut state.geometry_bounds.fuselage_length_m,
                    "m",
                );
                bounds_row(
                    grid,
                    ui,
                    "Wing leading-edge fraction",
                    &mut state.geometry_bounds.wing_leading_edge_fraction,
                    "",
                );
            });
        },
    );
}

fn aerodynamic_model_card(state: &mut UavWorkflowState, ui: &mut Ui) {
    card(ui, "Aerodynamic and structural assumptions", None, |ui| {
        grid_form(ui, "uav_aero_model_grid", |grid, ui| {
            value(
                grid,
                ui,
                "Air density",
                &mut state.model.air_density_kg_m3,
                "kg/m3",
            );
            value(
                grid,
                ui,
                "Maximum lift coefficient",
                &mut state.model.maximum_lift_coefficient,
                "",
            );
            value(
                grid,
                ui,
                "Zero-lift drag coefficient",
                &mut state.model.zero_lift_drag_coefficient,
                "",
            );
            value(
                grid,
                ui,
                "Oswald efficiency",
                &mut state.model.oswald_efficiency,
                "",
            );
            value(
                grid,
                ui,
                "Limit load factor",
                &mut state.model.limit_load_factor,
                "g",
            );
            value(
                grid,
                ui,
                "Structural safety factor",
                &mut state.model.structural_safety_factor,
                "",
            );
        });
    });
}

fn tail_control_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    grid_form(ui, "uav_tail_control_grid", |grid, ui| {
        value(
            grid,
            ui,
            "Horizontal tail-volume coefficient",
            &mut state.model.horizontal_tail_volume_coefficient,
            "",
        );
        value(
            grid,
            ui,
            "Vertical tail-volume coefficient",
            &mut state.model.vertical_tail_volume_coefficient,
            "",
        );
        value(
            grid,
            ui,
            "Horizontal tail aspect ratio",
            &mut state.model.horizontal_tail_aspect_ratio,
            "",
        );
        value(
            grid,
            ui,
            "Vertical tail aspect ratio",
            &mut state.model.vertical_tail_aspect_ratio,
            "",
        );
        value(
            grid,
            ui,
            "Forward CG chord fraction",
            &mut state.model.forward_cg_chord_fraction,
            "",
        );
        value(
            grid,
            ui,
            "Aft CG chord fraction",
            &mut state.model.aft_cg_chord_fraction,
            "",
        );
        value(
            grid,
            ui,
            "Aileron area fraction",
            &mut state.model.aileron_area_fraction,
            "",
        );
        value(
            grid,
            ui,
            "Aileron chord fraction",
            &mut state.model.aileron_chord_fraction,
            "",
        );
        value(
            grid,
            ui,
            "Elevator area fraction",
            &mut state.model.elevator_area_fraction,
            "",
        );
        value(
            grid,
            ui,
            "Elevator chord fraction",
            &mut state.model.elevator_chord_fraction,
            "",
        );
        value(
            grid,
            ui,
            "Hinge-moment coefficient",
            &mut state.model.hinge_moment_coefficient,
            "",
        );
    });
}

fn packaging_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    grid_form(ui, "uav_packaging_grid", |grid, ui| {
        value(
            grid,
            ui,
            "Equipment clearance",
            &mut state.model.equipment_clearance_m,
            "m",
        );
        value(
            grid,
            ui,
            "Equipment gap",
            &mut state.model.equipment_gap_m,
            "m",
        );
        value(
            grid,
            ui,
            "Nose length fraction",
            &mut state.model.nose_length_fraction,
            "",
        );
        value(
            grid,
            ui,
            "Tailcone length fraction",
            &mut state.model.tailcone_length_fraction,
            "",
        );
        value(
            grid,
            ui,
            "Spar-cap width fraction",
            &mut state.model.spar_cap_width_fraction,
            "",
        );
        value(
            grid,
            ui,
            "Spar-cap separation fraction",
            &mut state.model.spar_cap_separation_fraction,
            "",
        );
        value(
            grid,
            ui,
            "Landing-gear track fraction",
            &mut state.model.landing_gear_track_fraction,
            "",
        );
        value(
            grid,
            ui,
            "Landing-gear wheelbase fraction",
            &mut state.model.landing_gear_wheelbase_fraction,
            "",
        );
        value(
            grid,
            ui,
            "Propeller ground clearance",
            &mut state.model.propeller_ground_clearance_m,
            "m",
        );
    });
}

fn systems_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    grid_form(ui, "uav_systems_grid", |grid, ui| {
        value(
            grid,
            ui,
            "Fixed systems mass",
            &mut state.model.fixed_systems_mass_kg,
            "kg",
        );
        value(
            grid,
            ui,
            "Avionics mass",
            &mut state.systems.avionics_mass_kg,
            "kg",
        );
        value(
            grid,
            ui,
            "Avionics power",
            &mut state.systems.avionics_power_w,
            "W",
        );
        value(
            grid,
            ui,
            "Control-bus current",
            &mut state.systems.control_bus_current_a,
            "A",
        );
        value(
            grid,
            ui,
            "Control-bus voltage",
            &mut state.systems.control_bus_voltage_v,
            "V",
        );
        integer(
            grid,
            ui,
            "Minimum receiver channels",
            &mut state.systems.minimum_receiver_channels,
        );
        value(
            grid,
            ui,
            "Servo continuous-current fraction",
            &mut state.systems.servo_continuous_current_fraction,
            "",
        );
        value(
            grid,
            ui,
            "Maximum depth of discharge",
            &mut state.systems.maximum_depth_of_discharge,
            "",
        );
        value(
            grid,
            ui,
            "Energy reserve fraction",
            &mut state.systems.reserve_fraction,
            "",
        );
        value(
            grid,
            ui,
            "Avionics length",
            &mut state.systems.avionics_dimensions.length_m,
            "m",
        );
        value(
            grid,
            ui,
            "Avionics width",
            &mut state.systems.avionics_dimensions.width_m,
            "m",
        );
        value(
            grid,
            ui,
            "Avionics height",
            &mut state.systems.avionics_dimensions.height_m,
            "m",
        );
        usize_value(grid, ui, "Candidate evaluations", &mut state.evaluations);
        seed_value(grid, ui, "Random seed", &mut state.seed);
    });
}
