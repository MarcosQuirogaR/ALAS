// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Persistent interaction state for the fixed-wing UAV page: the competition
//! search, its inputs, and its last completed result.

mod model;
mod request;
mod support;
mod workflow;

pub use model::{CompetitionResult, UavExecutionStatus, UavSection, UavWorkflowOutcome};

use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::sync::Arc;

use alas_uav::optimizer::{
    DesignObjectives, GeometrySearchBounds, PreliminaryModel, SystemsDefinition,
};
use request::UavWorkerMessage;
use support::{bounds, dimensions};

/// Competition inputs: the preset and the rule quantities the simulation cannot
/// know (the test values are not competition figures).
#[derive(Debug, Clone, PartialEq)]
pub struct CompetitionSettings {
    /// Preset identifier, for example `acc-2026`.
    pub preset_id: String,
    /// Announced take-off distance the aircraft must meet, m.
    pub announced_takeoff_m: f64,
    /// Payload count for the ACC cans gates (rule quantity), count.
    pub payload_count: f64,
    /// Payload volume for the XC payload gates (rule quantity), m^3.
    pub payload_volume_m3: f64,
}

impl Default for CompetitionSettings {
    fn default() -> Self {
        Self {
            preset_id: "acc-2026".to_owned(),
            announced_takeoff_m: 60.0,
            payload_count: 2.0,
            payload_volume_m3: 0.25 * 0.12 * 0.10,
        }
    }
}

/// User-editable competition, mission and airframe inputs, and the worker state.
#[derive(Debug)]
pub struct UavWorkflowState {
    /// Mission and performance objectives of the design brief.
    pub objectives: DesignObjectives,
    /// Which workflow section the desktop is currently displaying.
    pub active_section: UavSection,
    /// Search bounds on the airframe geometry variables.
    pub geometry_bounds: GeometrySearchBounds,
    /// Preliminary sizing model assumptions.
    pub model: PreliminaryModel,
    /// Systems definition (payload, avionics, and similar loads).
    pub systems: SystemsDefinition,
    /// Competition preset and rule quantities.
    pub competition: CompetitionSettings,
    /// Objective-evaluation budget of the competition search.
    pub evaluations: usize,
    /// Random seed of the competition search.
    pub seed: u64,
    /// Last completed search result.
    pub outcome: UavWorkflowOutcome,
    /// Current worker state; failures do not erase [`Self::outcome`].
    pub execution: UavExecutionStatus,
    worker_rx: Option<Receiver<UavWorkerMessage>>,
    cancel_flag: Arc<AtomicBool>,
}

impl Default for UavWorkflowState {
    fn default() -> Self {
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
            competition: CompetitionSettings::default(),
            evaluations: 256,
            seed: 0x5eed_cafe,
            outcome: UavWorkflowOutcome::NotRun,
            execution: UavExecutionStatus::Idle,
            worker_rx: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }
}
