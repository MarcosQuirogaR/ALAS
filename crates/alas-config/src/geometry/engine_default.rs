// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The default [`EngineConfig`]: the GE9X entry of the engine table.

use super::*;

impl Default for EngineConfig {
    fn default() -> Self {
        // The GE9X entry of the engine table, copied out. A bare
        // `EngineConfig` is constructed before anything has had a chance to
        // call `apply_engine_spec`, and it has to describe a real engine by
        // the time something asks it for thrust.
        Self {
            engine_name: "GE9X".to_owned(),
            propulsion_technology: PropulsionTechnology::Turbofan,
            turbofan: crate::engines::get("GE9X")
                .ok()
                .and_then(crate::engines::EngineSpec::turbofan_spec),
            turboprop: None,
            nacelle_profile: vec![
                (0.0, 0.40),
                (0.6, 0.92),
                (1.2, 1.0),
                (4.3, 1.0),
                (5.9, 0.82),
                (7.8, 0.45),
            ],
            nacelle_model: NacelleModel::Legacy,
            radius_scale_m: 2.1,
            spanwise_positions_m: vec![9.8, -9.8],
            z_m: -2.9,
            z_per_engine_m: Vec::new(),
            inlet_x_offset_m: 4.2,
            centre_engine: None,
            bypass_ratio: 10.0,
            overall_pressure_ratio: 60.0,
            fan_pressure_ratio: 1.45,
            turbine_inlet_temp_k: 1670.0,
            cruise_tsfc_kg_kgf_hr: 0.50,
            fan_diameter_m: 3.40,
            // GE9X is not present in the March 2026 EEDB release. Until a
            // manufacturer deck is available, retain an explicit GEnx family
            // proxy rather than pretending the old linear law is measured.
            part_power_fuel_flow_ratios: vec![0.082_578_70, 0.253_287_04, 0.815, 1.0],
            part_power_source: "ICAO EEDB 03/2026 family proxy: 07P27GE235 GEnx-1B74/75/P2"
                .to_owned(),
        }
    }
}
