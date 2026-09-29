// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Persistent interaction state for the fixed-wing UAV workflow.

mod model;
mod request;
mod support;
mod workflow;

pub use model::{
    ComponentRole, MissionPlanMode, PropulsionInputMode, UavExecutionStatus, UavSection,
    UavSelections, UavWorkflowOutcome,
};

use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use alas_uav::optimizer::{
    DesignObjectives, GeometrySearchBounds, PreliminaryModel, SystemsDefinition,
};
use alas_uav::propulsion_electric::{ElectricMissionPhase, ElectricMissionResult};
use alas_uav::{ComponentKind, SharedCoreInputs, UavTopology};
use request::{default_selections, selected_record, UavWorkerMessage};
use support::{bounds, dimensions};

/// User-editable UAV objectives, assumptions, hardware, and solver evidence.
#[derive(Debug)]
pub struct UavWorkflowState {
    pub objectives: DesignObjectives,
    /// Which workflow section the desktop is currently displaying.
    pub active_section: UavSection,
    /// Fixed-wing arrangement selected before a sizing run.
    pub topology: UavTopology,
    pub geometry_bounds: GeometrySearchBounds,
    pub model: PreliminaryModel,
    pub systems: SystemsDefinition,
    pub selections: UavSelections,
    pub required_electronics_role: String,
    pub evaluations: usize,
    pub seed: u64,
    /// Normal workflow derives map points from the selected source-backed hardware.
    pub propulsion_input_mode: PropulsionInputMode,
    /// Number of identical motor, ESC, and propeller installations on one pack.
    pub propulsion_motor_count: u16,
    /// Normal workflow derives four steady phases from the design brief.
    pub mission_plan_mode: MissionPlanMode,
    /// Designer-entered phases used only when [`MissionPlanMode::Advanced`] is active.
    pub mission_phases: Vec<ElectricMissionPhase>,
    /// Manual-map battery cell count, used behind the advanced mode.
    pub propulsion_series_cells: u16,
    /// Manual-map provenance, used behind the advanced mode.
    pub propulsion_evidence: String,
    pub stall_thrust_n: f64,
    pub stall_current_a: f64,
    pub stall_power_w: f64,
    pub cruise_thrust_n: f64,
    pub cruise_current_a: f64,
    pub cruise_power_w: f64,
    pub shared_core_inputs: SharedCoreInputs,
    /// Latest automatically resolved electrical mission, including rejected designs.
    pub last_electrical_mission: Option<ElectricMissionResult>,
    /// Last completed accepted or evidence-rejected result.
    pub outcome: UavWorkflowOutcome,
    /// Design convention used by the last completed search, if any.
    pub last_completed_topology: Option<UavTopology>,
    /// Current worker state; failures do not erase [`Self::outcome`].
    pub execution: UavExecutionStatus,
    worker_rx: Option<Receiver<UavWorkerMessage>>,
    cancel_flag: Arc<AtomicBool>,
}

impl Default for UavWorkflowState {
    fn default() -> Self {
        let selections = default_selections();
        let required_electronics_role = selected_record(&selections, ComponentRole::Electronics)
            .and_then(|record| match &record.kind {
                ComponentKind::Electronics(spec) => Some(spec.role.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "gps_sensor".to_owned());
        let propulsion_series_cells = selected_record(&selections, ComponentRole::Battery)
            .and_then(|record| match &record.kind {
                ComponentKind::Battery(spec) => spec.series_cells,
                _ => None,
            })
            .unwrap_or(3);
        Self {
            objectives: DesignObjectives {
                endurance_s: 900.0,
                range_m: 10_000.0,
                cruise_speed_m_s: 20.0,
                maximum_stall_speed_m_s: 10.0,
                payload_mass_kg: 0.5,
                payload_dimensions: dimensions(0.25, 0.12, 0.10),
                minimum_propulsive_efficiency: 0.15,
                efficiency_priority: 0.5,
            },
            active_section: UavSection::Mission,
            topology: UavTopology::ConventionalTail,
            geometry_bounds: GeometrySearchBounds {
                wing_area_m2: bounds(0.9, 1.2),
                wing_aspect_ratio: bounds(8.0, 10.0),
                fuselage_length_m: bounds(1.8, 2.0),
                wing_leading_edge_fraction: bounds(0.25, 0.45),
            },
            model: PreliminaryModel {
                air_density_kg_m3: 1.225,
                maximum_lift_coefficient: 1.6,
                zero_lift_drag_coefficient: 0.035,
                oswald_efficiency: 0.8,
                limit_load_factor: 3.0,
                structural_safety_factor: 1.5,
                horizontal_tail_volume_coefficient: 0.5,
                vertical_tail_volume_coefficient: 0.04,
                horizontal_tail_aspect_ratio: 4.0,
                vertical_tail_aspect_ratio: 1.6,
                forward_cg_chord_fraction: 0.15,
                aft_cg_chord_fraction: 0.35,
                equipment_clearance_m: 0.015,
                equipment_gap_m: 0.015,
                nose_length_fraction: 0.10,
                tailcone_length_fraction: 0.30,
                spar_cap_width_fraction: 0.06,
                spar_cap_separation_fraction: 0.14,
                aileron_area_fraction: 0.08,
                aileron_chord_fraction: 0.25,
                elevator_area_fraction: 0.30,
                elevator_chord_fraction: 0.30,
                hinge_moment_coefficient: 0.01,
                landing_gear_track_fraction: 0.18,
                landing_gear_wheelbase_fraction: 0.35,
                propeller_ground_clearance_m: 0.05,
                fixed_systems_mass_kg: 0.15,
            },
            systems: SystemsDefinition {
                avionics_mass_kg: 0.10,
                avionics_dimensions: dimensions(0.08, 0.06, 0.03),
                avionics_power_w: 25.0,
                control_bus_current_a: 0.8,
                control_bus_voltage_v: 6.0,
                minimum_receiver_channels: 6,
                servo_continuous_current_fraction: 0.2,
                maximum_depth_of_discharge: 0.8,
                reserve_fraction: 0.2,
            },
            selections,
            required_electronics_role,
            evaluations: 256,
            seed: 0x5eed_cafe,
            propulsion_input_mode: PropulsionInputMode::Automatic,
            propulsion_motor_count: 1,
            mission_plan_mode: MissionPlanMode::Standard,
            mission_phases: Vec::new(),
            propulsion_series_cells,
            propulsion_evidence: String::new(),
            stall_thrust_n: 40.0,
            stall_current_a: 25.0,
            stall_power_w: 500.0,
            cruise_thrust_n: 30.0,
            cruise_current_a: 35.0,
            cruise_power_w: 750.0,
            shared_core_inputs: SharedCoreInputs {
                main_airfoil_name: "naca2412".to_owned(),
                tail_airfoil_name: "naca0012".to_owned(),
                altitude_m: 0.0,
                speed_m_s: 20.0,
                angle_of_attack_deg: 4.0,
                spanwise_resolution: 8,
                chordwise_resolution: 4,
            },
            last_electrical_mission: None,
            outcome: UavWorkflowOutcome::NotRun,
            last_completed_topology: None,
            execution: UavExecutionStatus::Idle,
            worker_rx: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}
