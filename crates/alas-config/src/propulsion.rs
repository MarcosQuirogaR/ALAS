// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/config/propulsion_config.py
// Reference: alas @ rust-port-baseline.

//! Component efficiencies and losses of the on-design turbofan cycle.
//!
//! The cycle model walks the engine station by station -- inlet, fan,
//! boosters, high-pressure compressor, combustor, both turbines, both nozzles
//! -- and each station needs an efficiency or a pressure ratio. These are
//! those numbers.
//!
//! They deliberately reproduce the assumptions the mission's own engine model
//! is built with, so the on-design cycle and the flown engine start from the
//! same component physics. Changing a value here does not change the
//! mission's, because that model is built by a separate frozen script; what
//! it buys is that someone auditing whether the two agree finds the same
//! numbers in both places rather than having to reconstruct one of them.
//!
//! Several fields declare no explanation upstream -- a polytropic efficiency
//! is largely self-describing to whoever is editing one -- and the
//! explanations here are this port's, as CONTRIBUTING.md requires. They
//! change no value.

use serde::{Deserialize, Serialize};

use crate::ConfigNode;

/// Component efficiencies for the on-design turbofan cycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ConfigNode)]
#[serde(deny_unknown_fields)]
pub struct PropulsionCycleConfig {
    /// Total-pressure recovery through the inlet.
    #[config(
        label = "Inlet pressure recovery",
        unit = "-",
        help = "Total-pressure recovery through the inlet (ram + duct losses). Matches the mission inlet_nozzle.pressure_ratio convention."
    )]
    pub inlet_pressure_recovery: f64,

    /// Fixed booster pressure ratio; the compressor makes up the rest.
    #[config(
        label = "LPC pressure-ratio split",
        unit = "-",
        help = "Fixed low-pressure-compressor (booster) pressure ratio; the high-pressure compressor makes up the rest of the overall (core) pressure ratio (HPC = OPR / this value). Matches the fixed mission LPC split."
    )]
    pub lpc_pressure_ratio_split: f64,

    /// Polytropic efficiency of the booster.
    #[config(
        label = "LPC polytropic efficiency",
        unit = "-",
        help = "Polytropic (small-stage) compression efficiency of the low-pressure compressor. Lower values cost work at every stage and show up as higher fuel burn."
    )]
    pub lpc_polytropic_efficiency: f64,

    /// Polytropic efficiency of the high-pressure compressor.
    #[config(
        label = "HPC polytropic efficiency",
        unit = "-",
        help = "Polytropic (small-stage) compression efficiency of the high-pressure compressor."
    )]
    pub hpc_polytropic_efficiency: f64,

    /// Polytropic efficiency of the fan.
    #[config(
        label = "Fan polytropic efficiency",
        unit = "-",
        help = "Polytropic compression efficiency of the fan, which sets most of the thrust on a high-bypass engine and so matters more here than either compressor."
    )]
    pub fan_polytropic_efficiency: f64,

    /// Pressure retained across the combustor.
    #[config(
        label = "Combustor pressure ratio",
        unit = "-",
        help = "Total-pressure loss fraction across the combustor."
    )]
    pub combustor_pressure_ratio: f64,

    /// Share of the fuel's heat released in the combustor.
    #[config(
        label = "Combustor efficiency",
        unit = "-",
        help = "Fraction of the fuel's heating value actually released as heat in the combustor."
    )]
    pub combustor_efficiency: f64,

    /// Polytropic efficiency of the high-pressure turbine.
    #[config(
        label = "HPT polytropic efficiency",
        unit = "-",
        help = "Polytropic (small-stage) expansion efficiency of the high-pressure turbine, which drives the high-pressure compressor."
    )]
    pub hpt_polytropic_efficiency: f64,

    /// Polytropic efficiency of the low-pressure turbine.
    #[config(
        label = "LPT polytropic efficiency",
        unit = "-",
        help = "Polytropic (small-stage) expansion efficiency of the low-pressure turbine, which drives the fan and the booster."
    )]
    pub lpt_polytropic_efficiency: f64,

    /// Shaft transmission efficiency of both spools.
    #[config(
        label = "Turbine mechanical efficiency",
        unit = "-",
        help = "Shaft power-transmission efficiency for both spools (HPT-HPC, LPT-LPC+fan)."
    )]
    pub turbine_mechanical_efficiency: f64,

    /// Pressure retained across the core nozzle.
    #[config(
        label = "Core nozzle pressure ratio",
        unit = "-",
        help = "Total pressure retained across the core exhaust nozzle."
    )]
    pub core_nozzle_pressure_ratio: f64,

    /// Pressure retained across the fan nozzle.
    #[config(
        label = "Fan nozzle pressure ratio",
        unit = "-",
        help = "Total pressure retained across the fan (bypass) exhaust nozzle."
    )]
    pub fan_nozzle_pressure_ratio: f64,

    /// Expansion efficiency of the core nozzle.
    #[config(
        label = "Core nozzle efficiency",
        unit = "-",
        help = "Polytropic expansion efficiency of the core exhaust nozzle."
    )]
    pub core_nozzle_efficiency: f64,

    /// Expansion efficiency of the fan nozzle.
    #[config(
        label = "Fan nozzle efficiency",
        unit = "-",
        help = "Polytropic expansion efficiency of the fan (bypass) nozzle."
    )]
    pub fan_nozzle_efficiency: f64,

    /// Heat released by burning a kilogram of fuel.
    #[config(
        label = "Fuel heating value",
        unit = "kJ/kg",
        help = "Lower heating value of Jet-A/Jet-A1 fuel."
    )]
    pub fuel_heating_value_kj_kg: f64,

    /// Specific heat used for the unburned-air stations.
    #[config(
        label = "Cold-section specific heat (cp)",
        unit = "J/(kg.K)",
        help = "Air-standard specific heat used for the inlet/fan/compressor (unburned-air) stations."
    )]
    pub cp_cold_j_kgk: f64,

    /// Ratio of specific heats for the unburned-air stations.
    #[config(
        label = "Cold-section ratio of specific heats",
        unit = "-",
        help = "Ratio of specific heats for the unburned-air stations, which sets how temperature rises with pressure through the inlet, fan and compressors."
    )]
    pub gamma_cold: f64,

    /// Specific heat used for the combustion-gas stations.
    #[config(
        label = "Hot-section specific heat (cp)",
        unit = "J/(kg.K)",
        help = "Combustion-gas specific heat used for the combustor/turbine/core-nozzle stations."
    )]
    pub cp_hot_j_kgk: f64,

    /// Ratio of specific heats for the combustion-gas stations.
    #[config(
        label = "Hot-section ratio of specific heats",
        unit = "-",
        help = "Ratio of specific heats for the combustion-gas stations. Lower than the cold-section value because the burned gas is hotter and its molecules have more ways to store energy."
    )]
    pub gamma_hot: f64,

    /// Fan-face Mach number the mass-flow check assumes.
    #[config(
        label = "Assumed fan-face Mach number (static anchor)",
        unit = "-",
        help = "Used only to sanity-check the design mass flow implied by anchoring the cycle to the engine's rated static thrust -- a conceptual-design-level assumption, not a real corrected-flow schedule."
    )]
    pub fan_face_mach: f64,
}

impl Default for PropulsionCycleConfig {
    fn default() -> Self {
        Self {
            inlet_pressure_recovery: 0.98,
            lpc_pressure_ratio_split: 1.20,
            lpc_polytropic_efficiency: 0.91,
            hpc_polytropic_efficiency: 0.93,
            fan_polytropic_efficiency: 0.93,
            combustor_pressure_ratio: 0.95,
            combustor_efficiency: 0.99,
            hpt_polytropic_efficiency: 0.95,
            lpt_polytropic_efficiency: 0.95,
            turbine_mechanical_efficiency: 0.99,
            core_nozzle_pressure_ratio: 0.99,
            fan_nozzle_pressure_ratio: 0.99,
            core_nozzle_efficiency: 0.95,
            fan_nozzle_efficiency: 0.95,
            fuel_heating_value_kj_kg: 42_800.0,
            cp_cold_j_kgk: 1004.5,
            gamma_cold: 1.4,
            cp_hot_j_kgk: 1156.9,
            gamma_hot: 1.33,
            fan_face_mach: 0.55,
        }
    }
}

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_component_is_better_than_lossless() {
        // An efficiency or pressure ratio above one is a component producing
        // work from nothing, and the cycle would happily report the resulting
        // fuel burn.
        let config = PropulsionCycleConfig::default();
        for (name, value) in [
            ("inlet", config.inlet_pressure_recovery),
            ("lpc", config.lpc_polytropic_efficiency),
            ("hpc", config.hpc_polytropic_efficiency),
            ("fan", config.fan_polytropic_efficiency),
            ("combustor pressure", config.combustor_pressure_ratio),
            ("combustor", config.combustor_efficiency),
            ("hpt", config.hpt_polytropic_efficiency),
            ("lpt", config.lpt_polytropic_efficiency),
            ("shaft", config.turbine_mechanical_efficiency),
            ("core nozzle pressure", config.core_nozzle_pressure_ratio),
            ("fan nozzle pressure", config.fan_nozzle_pressure_ratio),
            ("core nozzle", config.core_nozzle_efficiency),
            ("fan nozzle", config.fan_nozzle_efficiency),
        ] {
            assert!(
                value > 0.0 && value <= 1.0,
                "{name} is {value}, which is outside a physical efficiency"
            );
        }
    }

    #[test]
    fn the_hot_section_gas_is_less_stiff_than_cold_air() {
        // Burned gas has more internal degrees of freedom, so its ratio of
        // specific heats is lower and its specific heat higher. Swapping the
        // cold and hot pairs is an easy transposition and would shift every
        // turbine temperature.
        let config = PropulsionCycleConfig::default();
        assert!(config.gamma_hot < config.gamma_cold);
        assert!(config.cp_hot_j_kgk > config.cp_cold_j_kgk);
    }

    #[test]
    fn the_booster_takes_only_a_small_share_of_the_core_pressure_ratio() {
        // The high-pressure compressor is sized as the overall ratio divided
        // by this, so a booster ratio at or above a realistic overall ratio
        // would ask the compressor for less than unity.
        let config = PropulsionCycleConfig::default();
        assert!(config.lpc_pressure_ratio_split > 1.0);
        assert!(config.lpc_pressure_ratio_split < 3.0);
    }

    #[test]
    fn a_field_with_no_upstream_explanation_still_reaches_the_form_with_one() {
        let schema = PropulsionCycleConfig::default().schema();
        let field = schema.field("lpc_polytropic_efficiency").unwrap();
        assert!(!field.help.is_empty());
        assert_eq!(field.unit, "-");
    }
}
