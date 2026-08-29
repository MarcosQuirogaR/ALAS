// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fixture records and per-station comparisons for the mission turbofan parity
//! test.
//!
//! The `*Record` types deserialize one component's station values out of
//! `golden/prop/suave_turbofan.json`, and each `compare_*` walks one component's
//! outputs against its record so a wrong value anywhere in the cycle prints as
//! its own line. They live here rather than in the test file only so neither
//! file crosses the source-length limit.

use alas_prop::mission_turbofan::{
    CombustorOutput, CompressionNozzleOutput, CompressorOutput, ExpansionNozzleOutput, Freestream,
    RamOutput, StationSet, ThrustOutput, TurbineOutput,
};
use alas_testkit::Comparison;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct FreestreamRecord {
    pressure: f64,
    temperature: f64,
    density: f64,
    dynamic_viscosity: f64,
    gravity: f64,
    isentropic_expansion_factor: f64,
    #[serde(rename = "Cp")]
    cp: f64,
    #[serde(rename = "R")]
    r: f64,
    speed_of_sound: f64,
    velocity: f64,
    mach_number: f64,
    stagnation_temperature: f64,
    stagnation_pressure: f64,
    altitude: f64,
}

#[derive(Debug, Deserialize)]
pub struct RamRecord {
    stagnation_temperature: f64,
    stagnation_pressure: f64,
    isentropic_expansion_factor: f64,
    specific_heat_at_constant_pressure: f64,
    gas_specific_constant: f64,
}

#[derive(Debug, Deserialize)]
pub struct CompressionNozzleRecord {
    stagnation_temperature: f64,
    stagnation_pressure: f64,
    stagnation_enthalpy: f64,
    mach_number: f64,
    static_temperature: f64,
    static_enthalpy: f64,
    velocity: f64,
}

#[derive(Debug, Deserialize)]
pub struct CompressorRecord {
    stagnation_temperature: f64,
    stagnation_pressure: f64,
    stagnation_enthalpy: f64,
    work_done: f64,
}

#[derive(Debug, Deserialize)]
pub struct CombustorRecord {
    stagnation_temperature: f64,
    stagnation_pressure: f64,
    stagnation_enthalpy: f64,
    fuel_to_air_ratio: f64,
}

#[derive(Debug, Deserialize)]
pub struct TurbineRecord {
    stagnation_temperature: f64,
    stagnation_pressure: f64,
    stagnation_enthalpy: f64,
}

#[derive(Debug, Deserialize)]
pub struct ExpansionNozzleRecord {
    stagnation_temperature: f64,
    stagnation_pressure: f64,
    stagnation_enthalpy: f64,
    mach_number: f64,
    static_temperature: f64,
    density: f64,
    static_enthalpy: f64,
    velocity: f64,
    static_pressure: f64,
    area_ratio: f64,
}

#[derive(Debug, Deserialize)]
pub struct ThrustRecord {
    thrust: f64,
    thrust_specific_fuel_consumption: f64,
    non_dimensional_thrust: f64,
    core_mass_flow_rate: f64,
    fuel_flow_rate: f64,
    power: f64,
    specific_impulse: f64,
}

#[derive(Debug, Deserialize)]
pub struct StationSetRecord {
    freestream: FreestreamRecord,
    ram: RamRecord,
    inlet_nozzle: CompressionNozzleRecord,
    low_pressure_compressor: CompressorRecord,
    high_pressure_compressor: CompressorRecord,
    fan: CompressorRecord,
    combustor: CombustorRecord,
    high_pressure_turbine: TurbineRecord,
    low_pressure_turbine: TurbineRecord,
    core_nozzle: ExpansionNozzleRecord,
    fan_nozzle: ExpansionNozzleRecord,
    thrust: ThrustRecord,
}

fn compare_freestream(c: &mut Comparison, p: &str, got: &Freestream, want: &FreestreamRecord) {
    c.scalar(&format!("{p}.pressure"), got.pressure_pa, want.pressure);
    c.scalar(
        &format!("{p}.temperature"),
        got.temperature_k,
        want.temperature,
    );
    c.scalar(&format!("{p}.density"), got.density_kg_m3, want.density);
    c.scalar(
        &format!("{p}.dynamic_viscosity"),
        got.dynamic_viscosity_pa_s,
        want.dynamic_viscosity,
    );
    c.scalar(&format!("{p}.gravity"), got.gravity_m_s2, want.gravity);
    c.scalar(
        &format!("{p}.isentropic_expansion_factor"),
        got.gamma,
        want.isentropic_expansion_factor,
    );
    c.scalar(&format!("{p}.Cp"), got.cp_j_kgk, want.cp);
    c.scalar(&format!("{p}.R"), got.r_j_kgk, want.r);
    c.scalar(
        &format!("{p}.speed_of_sound"),
        got.speed_of_sound_m_s,
        want.speed_of_sound,
    );
    c.scalar(&format!("{p}.velocity"), got.velocity_m_s, want.velocity);
    c.scalar(&format!("{p}.mach_number"), got.mach, want.mach_number);
    c.scalar(
        &format!("{p}.stagnation_temperature"),
        got.stagnation_temperature_k,
        want.stagnation_temperature,
    );
    c.scalar(
        &format!("{p}.stagnation_pressure"),
        got.stagnation_pressure_pa,
        want.stagnation_pressure,
    );
    c.scalar(&format!("{p}.altitude"), got.altitude_m, want.altitude);
}

fn compare_ram(c: &mut Comparison, p: &str, got: &RamOutput, want: &RamRecord) {
    c.scalar(
        &format!("{p}.stagnation_temperature"),
        got.stagnation_temperature_k,
        want.stagnation_temperature,
    );
    c.scalar(
        &format!("{p}.stagnation_pressure"),
        got.stagnation_pressure_pa,
        want.stagnation_pressure,
    );
    c.scalar(
        &format!("{p}.isentropic_expansion_factor"),
        got.isentropic_expansion_factor,
        want.isentropic_expansion_factor,
    );
    c.scalar(
        &format!("{p}.specific_heat_at_constant_pressure"),
        got.specific_heat_at_constant_pressure_j_kgk,
        want.specific_heat_at_constant_pressure,
    );
    c.scalar(
        &format!("{p}.gas_specific_constant"),
        got.gas_specific_constant_j_kgk,
        want.gas_specific_constant,
    );
}

fn compare_inlet(
    c: &mut Comparison,
    p: &str,
    got: &CompressionNozzleOutput,
    want: &CompressionNozzleRecord,
) {
    c.scalar(
        &format!("{p}.stagnation_temperature"),
        got.stagnation_temperature_k,
        want.stagnation_temperature,
    );
    c.scalar(
        &format!("{p}.stagnation_pressure"),
        got.stagnation_pressure_pa,
        want.stagnation_pressure,
    );
    c.scalar(
        &format!("{p}.stagnation_enthalpy"),
        got.stagnation_enthalpy_j_kg,
        want.stagnation_enthalpy,
    );
    c.scalar(&format!("{p}.mach_number"), got.mach, want.mach_number);
    c.scalar(
        &format!("{p}.static_temperature"),
        got.static_temperature_k,
        want.static_temperature,
    );
    c.scalar(
        &format!("{p}.static_enthalpy"),
        got.static_enthalpy_j_kg,
        want.static_enthalpy,
    );
    c.scalar(&format!("{p}.velocity"), got.velocity_m_s, want.velocity);
}

fn compare_compressor(
    c: &mut Comparison,
    p: &str,
    got: &CompressorOutput,
    want: &CompressorRecord,
) {
    c.scalar(
        &format!("{p}.stagnation_temperature"),
        got.stagnation_temperature_k,
        want.stagnation_temperature,
    );
    c.scalar(
        &format!("{p}.stagnation_pressure"),
        got.stagnation_pressure_pa,
        want.stagnation_pressure,
    );
    c.scalar(
        &format!("{p}.stagnation_enthalpy"),
        got.stagnation_enthalpy_j_kg,
        want.stagnation_enthalpy,
    );
    c.scalar(
        &format!("{p}.work_done"),
        got.work_done_j_kg,
        want.work_done,
    );
}

fn compare_combustor(c: &mut Comparison, p: &str, got: &CombustorOutput, want: &CombustorRecord) {
    c.scalar(
        &format!("{p}.stagnation_temperature"),
        got.stagnation_temperature_k,
        want.stagnation_temperature,
    );
    c.scalar(
        &format!("{p}.stagnation_pressure"),
        got.stagnation_pressure_pa,
        want.stagnation_pressure,
    );
    c.scalar(
        &format!("{p}.stagnation_enthalpy"),
        got.stagnation_enthalpy_j_kg,
        want.stagnation_enthalpy,
    );
    c.scalar(
        &format!("{p}.fuel_to_air_ratio"),
        got.fuel_to_air_ratio,
        want.fuel_to_air_ratio,
    );
}

fn compare_turbine(c: &mut Comparison, p: &str, got: &TurbineOutput, want: &TurbineRecord) {
    c.scalar(
        &format!("{p}.stagnation_temperature"),
        got.stagnation_temperature_k,
        want.stagnation_temperature,
    );
    c.scalar(
        &format!("{p}.stagnation_pressure"),
        got.stagnation_pressure_pa,
        want.stagnation_pressure,
    );
    c.scalar(
        &format!("{p}.stagnation_enthalpy"),
        got.stagnation_enthalpy_j_kg,
        want.stagnation_enthalpy,
    );
}

fn compare_expansion(
    c: &mut Comparison,
    p: &str,
    got: &ExpansionNozzleOutput,
    want: &ExpansionNozzleRecord,
) {
    c.scalar(
        &format!("{p}.stagnation_temperature"),
        got.stagnation_temperature_k,
        want.stagnation_temperature,
    );
    c.scalar(
        &format!("{p}.stagnation_pressure"),
        got.stagnation_pressure_pa,
        want.stagnation_pressure,
    );
    c.scalar(
        &format!("{p}.stagnation_enthalpy"),
        got.stagnation_enthalpy_j_kg,
        want.stagnation_enthalpy,
    );
    c.scalar(&format!("{p}.mach_number"), got.mach, want.mach_number);
    c.scalar(
        &format!("{p}.static_temperature"),
        got.static_temperature_k,
        want.static_temperature,
    );
    c.scalar(&format!("{p}.density"), got.density_kg_m3, want.density);
    c.scalar(
        &format!("{p}.static_enthalpy"),
        got.static_enthalpy_j_kg,
        want.static_enthalpy,
    );
    c.scalar(&format!("{p}.velocity"), got.velocity_m_s, want.velocity);
    c.scalar(
        &format!("{p}.static_pressure"),
        got.static_pressure_pa,
        want.static_pressure,
    );
    c.scalar(&format!("{p}.area_ratio"), got.area_ratio, want.area_ratio);
}

fn compare_thrust(c: &mut Comparison, p: &str, got: &ThrustOutput, want: &ThrustRecord) {
    c.scalar(&format!("{p}.thrust"), got.thrust_n, want.thrust);
    c.scalar(
        &format!("{p}.thrust_specific_fuel_consumption"),
        got.thrust_specific_fuel_consumption,
        want.thrust_specific_fuel_consumption,
    );
    c.scalar(
        &format!("{p}.non_dimensional_thrust"),
        got.non_dimensional_thrust,
        want.non_dimensional_thrust,
    );
    c.scalar(
        &format!("{p}.core_mass_flow_rate"),
        got.core_mass_flow_rate_kg_s,
        want.core_mass_flow_rate,
    );
    c.scalar(
        &format!("{p}.fuel_flow_rate"),
        got.fuel_flow_rate_kg_s,
        want.fuel_flow_rate,
    );
    c.scalar(&format!("{p}.power"), got.power_w, want.power);
    c.scalar(
        &format!("{p}.specific_impulse"),
        got.specific_impulse_s,
        want.specific_impulse,
    );
}

/// Walk one full pass through the network, comparing every station.
pub fn compare_station_set(c: &mut Comparison, p: &str, got: &StationSet, want: &StationSetRecord) {
    compare_freestream(
        c,
        &format!("{p}.freestream"),
        &got.freestream,
        &want.freestream,
    );
    compare_ram(c, &format!("{p}.ram"), &got.ram, &want.ram);
    compare_inlet(
        c,
        &format!("{p}.inlet_nozzle"),
        &got.inlet_nozzle,
        &want.inlet_nozzle,
    );
    compare_compressor(
        c,
        &format!("{p}.low_pressure_compressor"),
        &got.low_pressure_compressor,
        &want.low_pressure_compressor,
    );
    compare_compressor(
        c,
        &format!("{p}.high_pressure_compressor"),
        &got.high_pressure_compressor,
        &want.high_pressure_compressor,
    );
    compare_compressor(c, &format!("{p}.fan"), &got.fan, &want.fan);
    compare_combustor(
        c,
        &format!("{p}.combustor"),
        &got.combustor,
        &want.combustor,
    );
    compare_turbine(
        c,
        &format!("{p}.high_pressure_turbine"),
        &got.high_pressure_turbine,
        &want.high_pressure_turbine,
    );
    compare_turbine(
        c,
        &format!("{p}.low_pressure_turbine"),
        &got.low_pressure_turbine,
        &want.low_pressure_turbine,
    );
    compare_expansion(
        c,
        &format!("{p}.core_nozzle"),
        &got.core_nozzle,
        &want.core_nozzle,
    );
    compare_expansion(
        c,
        &format!("{p}.fan_nozzle"),
        &got.fan_nozzle,
        &want.fan_nozzle,
    );
    compare_thrust(c, &format!("{p}.thrust"), &got.thrust, &want.thrust);
}
