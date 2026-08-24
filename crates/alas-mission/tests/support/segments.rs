// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reading `golden/mission/mission_segments.json`.
//!
//! Split out of `parity_mission_segments.rs` so that file stays under the
//! source-length limit: transcribing forty-odd recorded arrays is bulk, and
//! the comparison that reads them is the part worth reading.

use alas_mission::segments::Initials;
use serde::Deserialize;

use super::{Spec, SurrogateTraining, Vehicle};

#[derive(Debug, Deserialize)]
pub struct Fixture {
    pub inputs: Inputs,
    pub segments: Vec<Held>,
}

#[derive(Debug, Deserialize)]
pub struct Inputs {
    pub vehicle: Vehicle,
    pub surrogate_training: SurrogateTraining,
}

#[derive(Debug, Deserialize)]
pub struct Held {
    pub tag: String,
    pub spec: Spec,
    pub initials: FixtureInitials,
    pub unknowns: Unknowns,
    pub conditions: RecordedConditions,
}

#[derive(Debug, Deserialize)]
pub struct FixtureInitials {
    pub time_s: f64,
    pub total_mass_kg: f64,
    pub position_vector_x_m: f64,
    pub position_vector_y_m: f64,
    pub position_vector_z_m: f64,
    pub aircraft_range_m: f64,
    pub latitude_deg: f64,
    pub longitude_deg: f64,
}

#[derive(Debug, Deserialize)]
pub struct Unknowns {
    pub throttle: Vec<f64>,
    pub body_angle_rad: Vec<f64>,
}

/// Every array the iterate chain writes, grouped by the method that wrote it.
#[derive(Debug, Deserialize)]
pub struct RecordedConditions {
    pub time_s: Vec<f64>,
    pub position_vector_x_m: Vec<f64>,
    pub position_vector_y_m: Vec<f64>,
    pub position_vector_z_m: Vec<f64>,
    pub velocity_vector_x_m_s: Vec<f64>,
    pub velocity_vector_z_m_s: Vec<f64>,
    pub aircraft_range_m: Vec<f64>,
    pub acceleration_vector_x_m_s2: Vec<f64>,
    pub acceleration_vector_z_m_s2: Vec<f64>,
    pub altitude_m: Vec<f64>,
    pub pressure_pa: Vec<f64>,
    pub temperature_k: Vec<f64>,
    pub density_kg_m3: Vec<f64>,
    pub speed_of_sound_m_s: Vec<f64>,
    pub dynamic_viscosity_pa_s: Vec<f64>,
    pub gravity_m_s2: Vec<f64>,
    pub velocity_m_s: Vec<f64>,
    pub mach: Vec<f64>,
    pub reynolds_number_per_m: Vec<f64>,
    pub dynamic_pressure_pa: Vec<f64>,
    pub body_angle_rad: Vec<f64>,
    pub angle_of_attack_rad: Vec<f64>,
    pub side_slip_angle_rad: Vec<f64>,
    pub transform_body_to_inertial: Vec<Vec<f64>>,
    pub transform_wind_to_inertial: Vec<Vec<f64>>,
    pub throttle: Vec<f64>,
    pub thrust_force_x_n: Vec<f64>,
    pub vehicle_mass_rate_kg_s: Vec<f64>,
    pub lift_coefficient: Vec<f64>,
    pub drag_coefficient: Vec<f64>,
    pub lift_force_z_n: Vec<f64>,
    pub drag_force_x_n: Vec<f64>,
    pub drag_parasite: Vec<f64>,
    pub drag_induced: Vec<f64>,
    pub drag_compressible: Vec<f64>,
    pub drag_miscellaneous: Vec<f64>,
    pub drag_untrimmed: Vec<f64>,
    pub total_mass_kg: Vec<f64>,
    pub gravity_force_z_n: Vec<f64>,
    pub total_force_x_n: Vec<f64>,
    pub total_force_y_n: Vec<f64>,
    pub total_force_z_n: Vec<f64>,
    pub latitude_deg: Vec<f64>,
    pub longitude_deg: Vec<f64>,
    pub residual_horizontal: Vec<f64>,
    pub residual_vertical: Vec<f64>,
}

impl From<&FixtureInitials> for Initials {
    fn from(recorded: &FixtureInitials) -> Self {
        Self {
            time_s: recorded.time_s,
            total_mass_kg: recorded.total_mass_kg,
            position_vector_m: [
                recorded.position_vector_x_m,
                recorded.position_vector_y_m,
                recorded.position_vector_z_m,
            ],
            aircraft_range_m: recorded.aircraft_range_m,
            latitude_deg: recorded.latitude_deg,
            longitude_deg: recorded.longitude_deg,
        }
    }
}

/// Load the recorded held-unknown iterations.
pub fn fixture() -> Fixture {
    alas_testkit::load("mission", "mission_segments")
}
