// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airframe and shared-core controls for the fixed-wing UAV workflow.

use egui::{DragValue, Grid, RichText, Ui};

use crate::uav::UavWorkflowState;

use super::super::tr;
use super::presentation::{card, card_column_count, collapsing_card};
use super::uav_fields::{bounds_row, integer, text_value, usize_value, value};

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
            form_grid(ui, "uav_bounds_grid", |ui| {
                bounds_row(ui, "Wing area", &mut state.geometry_bounds.wing_area_m2, "m2");
                bounds_row(
                    ui,
                    "Wing aspect ratio",
                    &mut state.geometry_bounds.wing_aspect_ratio,
                    "",
                );
                bounds_row(
                    ui,
                    "Fuselage length",
                    &mut state.geometry_bounds.fuselage_length_m,
                    "m",
                );
                bounds_row(
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
        form_grid(ui, "uav_aero_model_grid", |ui| {
            value(
                ui,
                "Air density",
                &mut state.model.air_density_kg_m3,
                "kg/m3",
            );
            value(
                ui,
                "Maximum lift coefficient",
                &mut state.model.maximum_lift_coefficient,
                "",
            );
            value(
                ui,
                "Zero-lift drag coefficient",
                &mut state.model.zero_lift_drag_coefficient,
                "",
            );
            value(
                ui,
                "Oswald efficiency",
                &mut state.model.oswald_efficiency,
                "",
            );
            value(
                ui,
                "Limit load factor",
                &mut state.model.limit_load_factor,
                "g",
            );
            value(
                ui,
                "Structural safety factor",
                &mut state.model.structural_safety_factor,
                "",
            );
        });
    });
}

fn tail_control_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    form_grid(ui, "uav_tail_control_grid", |ui| {
        value(
            ui,
            "Horizontal tail-volume coefficient",
            &mut state.model.horizontal_tail_volume_coefficient,
            "",
        );
        value(
            ui,
            "Vertical tail-volume coefficient",
            &mut state.model.vertical_tail_volume_coefficient,
            "",
        );
        value(
            ui,
            "Horizontal tail aspect ratio",
            &mut state.model.horizontal_tail_aspect_ratio,
            "",
        );
        value(
            ui,
            "Vertical tail aspect ratio",
            &mut state.model.vertical_tail_aspect_ratio,
            "",
        );
        value(
            ui,
            "Forward CG chord fraction",
            &mut state.model.forward_cg_chord_fraction,
            "",
        );
        value(
            ui,
            "Aft CG chord fraction",
            &mut state.model.aft_cg_chord_fraction,
            "",
        );
        value(
            ui,
            "Aileron area fraction",
            &mut state.model.aileron_area_fraction,
            "",
        );
        value(
            ui,
            "Aileron chord fraction",
            &mut state.model.aileron_chord_fraction,
            "",
        );
        value(
            ui,
            "Elevator area fraction",
            &mut state.model.elevator_area_fraction,
            "",
        );
        value(
            ui,
            "Elevator chord fraction",
            &mut state.model.elevator_chord_fraction,
            "",
        );
        value(
            ui,
            "Hinge-moment coefficient",
            &mut state.model.hinge_moment_coefficient,
            "",
        );
    });
}

fn packaging_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    form_grid(ui, "uav_packaging_grid", |ui| {
        value(
            ui,
            "Equipment clearance",
            &mut state.model.equipment_clearance_m,
            "m",
        );
        value(ui, "Equipment gap", &mut state.model.equipment_gap_m, "m");
        value(
            ui,
            "Nose length fraction",
            &mut state.model.nose_length_fraction,
            "",
        );
        value(
            ui,
            "Tailcone length fraction",
            &mut state.model.tailcone_length_fraction,
            "",
        );
        value(
            ui,
            "Spar-cap width fraction",
            &mut state.model.spar_cap_width_fraction,
            "",
        );
        value(
            ui,
            "Spar-cap separation fraction",
            &mut state.model.spar_cap_separation_fraction,
            "",
        );
        value(
            ui,
            "Landing-gear track fraction",
            &mut state.model.landing_gear_track_fraction,
            "",
        );
        value(
            ui,
            "Landing-gear wheelbase fraction",
            &mut state.model.landing_gear_wheelbase_fraction,
            "",
        );
        value(
            ui,
            "Propeller ground clearance",
            &mut state.model.propeller_ground_clearance_m,
            "m",
        );
    });
}

fn systems_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    form_grid(ui, "uav_systems_grid", |ui| {
        value(
            ui,
            "Fixed systems mass",
            &mut state.model.fixed_systems_mass_kg,
            "kg",
        );
        value(
            ui,
            "Avionics mass",
            &mut state.systems.avionics_mass_kg,
            "kg",
        );
        value(
            ui,
            "Avionics power",
            &mut state.systems.avionics_power_w,
            "W",
        );
        value(
            ui,
            "Control-bus current",
            &mut state.systems.control_bus_current_a,
            "A",
        );
        value(
            ui,
            "Control-bus voltage",
            &mut state.systems.control_bus_voltage_v,
            "V",
        );
        integer(
            ui,
            "Minimum receiver channels",
            &mut state.systems.minimum_receiver_channels,
        );
        value(
            ui,
            "Servo continuous-current fraction",
            &mut state.systems.servo_continuous_current_fraction,
            "",
        );
        value(
            ui,
            "Maximum depth of discharge",
            &mut state.systems.maximum_depth_of_discharge,
            "",
        );
        value(
            ui,
            "Energy reserve fraction",
            &mut state.systems.reserve_fraction,
            "",
        );
        value(
            ui,
            "Avionics length",
            &mut state.systems.avionics_dimensions.length_m,
            "m",
        );
        value(
            ui,
            "Avionics width",
            &mut state.systems.avionics_dimensions.width_m,
            "m",
        );
        value(
            ui,
            "Avionics height",
            &mut state.systems.avionics_dimensions.height_m,
            "m",
        );
        usize_value(ui, "Candidate evaluations", &mut state.evaluations);
        ui.label(RichText::new(tr("Random seed")).strong());
        ui.add_sized(
            [
                ui.available_width().clamp(112.0, 220.0),
                ui.spacing().interact_size.y,
            ],
            DragValue::new(&mut state.seed).speed(1),
        );
        ui.end_row();
    });
}

pub(super) fn shared_core_inputs(state: &mut UavWorkflowState, ui: &mut Ui) {
    card(
        ui,
        "Shared production-core verification",
        Some(
            "The accepted geometry is rebuilt as production Airplane primitives and evaluated by native VLM. Parasite drag remains the explicit preliminary CD0 input and is not attributed to VLM.",
        ),
        |ui| {
            form_grid(ui, "uav_shared_core_grid", |ui| {
                text_value(
                    ui,
                    "Main-wing airfoil",
                    &mut state.shared_core_inputs.main_airfoil_name,
                );
                text_value(
                    ui,
                    "Tail or control-surface airfoil",
                    &mut state.shared_core_inputs.tail_airfoil_name,
                );
                value(
                    ui,
                    "VLM altitude",
                    &mut state.shared_core_inputs.altitude_m,
                    "m",
                );
                value(
                    ui,
                    "VLM speed",
                    &mut state.shared_core_inputs.speed_m_s,
                    "m/s",
                );
                value(
                    ui,
                    "VLM angle of attack",
                    &mut state.shared_core_inputs.angle_of_attack_deg,
                    "deg",
                );
                usize_value(
                    ui,
                    "Spanwise resolution",
                    &mut state.shared_core_inputs.spanwise_resolution,
                );
                usize_value(
                    ui,
                    "Chordwise resolution",
                    &mut state.shared_core_inputs.chordwise_resolution,
                );
            });
        },
    );
}

fn form_grid(ui: &mut Ui, id: impl std::hash::Hash, contents: impl FnOnce(&mut Ui)) {
    Grid::new(id)
        .num_columns(2)
        .spacing([20.0, 8.0])
        .show(ui, contents);
}
