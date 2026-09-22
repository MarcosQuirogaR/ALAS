// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from the `.outputs` structures mission reference's Energy.Converters and
// Energy.Processes.Thrust attach to each component during a network walk.
// Upstream: mission reference 2.5.2, LGPL-2.1 (relicensed under GPL-2.0-or-later per
// LGPL-2.1 section 3; compatible with this program's AGPL-3.0-or-later).
// Reference: alas @ rust-port-baseline.

//! The per-station output records one pass through the turbofan network fills.
//!
//! Each struct mirrors one mission reference component's `.outputs` bag: `Ram.outputs`,
//! `Compressor.outputs`, `Thrust.outputs` and so on. They carry no behaviour:
//! [`super::size_turbofan`]'s network walk fills them and [`super::components`]'
//! functions read and return them, and live here, apart from the walk itself,
//! only so the module that drives them stays under the file-length limit. The
//! parent re-exports the whole set, so `super::RamOutput` and
//! `alas_prop::mission_turbofan::RamOutput` both resolve.

/// The constant freestream state the whole network reads, filled once from the
/// ambient atmosphere and the `Ram` gas-property fits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Freestream {
    /// Static pressure, Pa.
    pub pressure_pa: f64,
    /// Static temperature, K.
    pub temperature_k: f64,
    /// Density, kg/m^3.
    pub density_kg_m3: f64,
    /// Dynamic viscosity, kg/(m*s).
    pub dynamic_viscosity_pa_s: f64,
    /// Gravity at this altitude, m/s^2.
    pub gravity_m_s2: f64,
    /// Ratio of specific heats, from the air fit at the static temperature.
    pub gamma: f64,
    /// Specific heat at constant pressure, J/(kg*K), from the air fit.
    pub cp_j_kgk: f64,
    /// Air's specific gas constant, J/(kg*K).
    pub r_j_kgk: f64,
    /// Speed of sound, m/s (US-1976 constant-gamma value, not `gamma` above).
    pub speed_of_sound_m_s: f64,
    /// Freestream velocity, m/s (`speed_of_sound * mach`).
    pub velocity_m_s: f64,
    /// Freestream Mach number.
    pub mach: f64,
    /// Freestream stagnation temperature, K (the `Ram` output, restated here
    /// because every expansion nozzle reads it as its reference `Tto`).
    pub stagnation_temperature_k: f64,
    /// Freestream stagnation pressure, Pa (the `Ram` output, the nozzles' `Pto`).
    pub stagnation_pressure_pa: f64,
    /// Geometric altitude, m.
    pub altitude_m: f64,
}

/// `Ram.outputs`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RamOutput {
    /// Stagnation temperature, K.
    pub stagnation_temperature_k: f64,
    /// Stagnation pressure, Pa.
    pub stagnation_pressure_pa: f64,
    /// Ratio of specific heats.
    pub isentropic_expansion_factor: f64,
    /// Specific heat at constant pressure, J/(kg*K).
    pub specific_heat_at_constant_pressure_j_kgk: f64,
    /// Air's specific gas constant, J/(kg*K).
    pub gas_specific_constant_j_kgk: f64,
}

/// `Compression_Nozzle.outputs` (the inlet), `compressibility_effects=False`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompressionNozzleOutput {
    /// Stagnation temperature, K.
    pub stagnation_temperature_k: f64,
    /// Stagnation pressure, Pa.
    pub stagnation_pressure_pa: f64,
    /// Stagnation enthalpy, J/kg.
    pub stagnation_enthalpy_j_kg: f64,
    /// Exit Mach number.
    pub mach: f64,
    /// Static temperature, K.
    pub static_temperature_k: f64,
    /// Static enthalpy, J/kg.
    pub static_enthalpy_j_kg: f64,
    /// Exit velocity, m/s.
    pub velocity_m_s: f64,
}

/// `Compressor.outputs`, shared by both compressors and the fan.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompressorOutput {
    /// Stagnation temperature, K.
    pub stagnation_temperature_k: f64,
    /// Stagnation pressure, Pa.
    pub stagnation_pressure_pa: f64,
    /// Stagnation enthalpy, J/kg.
    pub stagnation_enthalpy_j_kg: f64,
    /// Specific work done, J/kg.
    pub work_done_j_kg: f64,
}

/// `Combustor.outputs`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CombustorOutput {
    /// Stagnation temperature, K (the turbine inlet temperature).
    pub stagnation_temperature_k: f64,
    /// Stagnation pressure, Pa.
    pub stagnation_pressure_pa: f64,
    /// Stagnation enthalpy, J/kg.
    pub stagnation_enthalpy_j_kg: f64,
    /// Fuel-air ratio.
    pub fuel_to_air_ratio: f64,
}

/// `Turbine.outputs`, shared by both turbines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurbineOutput {
    /// Stagnation temperature, K.
    pub stagnation_temperature_k: f64,
    /// Stagnation pressure, Pa.
    pub stagnation_pressure_pa: f64,
    /// Stagnation enthalpy, J/kg.
    pub stagnation_enthalpy_j_kg: f64,
}

/// `Expansion_Nozzle.outputs`, shared by the core and fan nozzles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpansionNozzleOutput {
    /// Stagnation temperature, K.
    pub stagnation_temperature_k: f64,
    /// Stagnation pressure, Pa.
    pub stagnation_pressure_pa: f64,
    /// Stagnation enthalpy, J/kg.
    pub stagnation_enthalpy_j_kg: f64,
    /// Exit Mach number (pinned to 1 when choked).
    pub mach: f64,
    /// Static temperature, K.
    pub static_temperature_k: f64,
    /// Exit density, kg/m^3.
    pub density_kg_m3: f64,
    /// Static enthalpy, J/kg.
    pub static_enthalpy_j_kg: f64,
    /// Exit velocity, m/s.
    pub velocity_m_s: f64,
    /// Static pressure, Pa (ambient when subsonic, choked value otherwise).
    pub static_pressure_pa: f64,
    /// Freestream-to-exit area ratio.
    pub area_ratio: f64,
}

/// `Thrust.outputs`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThrustOutput {
    /// Dimensional thrust, N (zero on the cruise sizing pass; see the module doc).
    pub thrust_n: f64,
    /// Thrust-specific fuel consumption, per hour.
    pub thrust_specific_fuel_consumption: f64,
    /// Specific thrust `Fsp`, nondimensional.
    pub non_dimensional_thrust: f64,
    /// Core mass flow rate, kg/s (zero when unsized).
    pub core_mass_flow_rate_kg_s: f64,
    /// Fuel flow rate, kg/s.
    pub fuel_flow_rate_kg_s: f64,
    /// Power, W.
    pub power_w: f64,
    /// Specific impulse, s.
    pub specific_impulse_s: f64,
}

/// Every station of one pass through the network.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StationSet {
    /// The freestream state the whole pass read.
    pub freestream: Freestream,
    /// `Ram` outputs.
    pub ram: RamOutput,
    /// Inlet (compression nozzle) outputs.
    pub inlet_nozzle: CompressionNozzleOutput,
    /// Low-pressure compressor outputs.
    pub low_pressure_compressor: CompressorOutput,
    /// High-pressure compressor outputs.
    pub high_pressure_compressor: CompressorOutput,
    /// Fan outputs.
    pub fan: CompressorOutput,
    /// Combustor outputs.
    pub combustor: CombustorOutput,
    /// High-pressure turbine outputs.
    pub high_pressure_turbine: TurbineOutput,
    /// Low-pressure turbine outputs.
    pub low_pressure_turbine: TurbineOutput,
    /// Core nozzle outputs.
    pub core_nozzle: ExpansionNozzleOutput,
    /// Fan nozzle outputs.
    pub fan_nozzle: ExpansionNozzleOutput,
    /// Thrust-process outputs.
    pub thrust: ThrustOutput,
}

/// Everything `turbofan_sizing` establishes on the engine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurbofanSizingResult {
    /// `turbofan.design_thrust`, N (the design thrust it was handed).
    pub design_thrust_n: f64,
    /// `thrust.mass_flow_rate_design`, kg/s.
    pub mass_flow_rate_design_kg_s: f64,
    /// `thrust.compressor_nondimensional_massflow`.
    pub compressor_nondimensional_massflow: f64,
    /// `turbofan.sealevel_static_thrust`, N per engine.
    pub sealevel_static_thrust_n_per_engine: f64,
    /// The cruise design-point pass.
    pub cruise: StationSet,
    /// The sea-level-static replay pass.
    pub sea_level_static: StationSet,
    /// `results_sls.thrust_force_vector[0, 0]`, the total sea-level-static
    /// thrust force, N.
    pub sea_level_static_thrust_force_n: f64,
    /// `results_sls.vehicle_mass_rate`, the sea-level-static fuel burn, kg/s.
    pub sea_level_static_vehicle_mass_rate_kg_s: f64,
}
