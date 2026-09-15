// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission reference/Components/Energy/Networks/Turbofan.py's component linking
// and mission reference/Components/Energy/Converters/Ram.py's freestream packing.
// Upstream: mission reference 2.5.2, LGPL-2.1 (relicensed under GPL-2.0-or-later per
// LGPL-2.1 section 3; compatible with this program's AGPL-3.0-or-later).
// Reference: alas @ rust-port-baseline.

//! Assembling a freestream, and walking the network across it once.
//!
//! `Turbofan.evaluate_thrust` is two hundred lines of wiring: it hands each
//! component the stagnation state of the one before it, in an order fixed by
//! the shafts rather than by the flow, and then hands the thrust process what
//! the two nozzles and the burner produced. [`walk_network`] is that wiring,
//! and it is shared, `turbofan_sizing` runs it twice, at cruise and at
//! sea-level-static, and a mission segment runs it once per control point.
//!
//! It lives apart from [`super`] only so the module that drives it stays under
//! the file-length limit; the two are one row.

use alas_atmo::{us1976_compute_values, Us1976Values};

use super::{
    components, CombustorOutput, CompressionNozzleOutput, CompressorOutput, ExpansionNozzleOutput,
    Freestream, RamOutput, TurbineOutput, TurbofanInputs, VehicleBuilderParams, GAS_CONSTANT_AIR,
    JET_A_SPECIFIC_ENERGY_J_KG,
};

/// Build the network's freestream from an already-computed atmosphere.
///
/// The gas properties are the `Ram` component's (the [`components`] fits at
/// the *static* temperature, and the fixed air gas constant) and the two
/// stagnation quantities are `Ram`'s outputs, restated on the freestream
/// because every expansion nozzle reads them as its reference state.
///
/// `velocity_m_s` is taken rather than formed as `speed_of_sound * mach`
/// because a mission segment's freestream carries both and they are not the
/// same double: `mach` there is `|V| / a`, so `a * mach` is `|V|` rounded
/// twice. `Thrust.compute` reads `u0` and `M0` from separate fields, and this
/// signature keeps them separate too.
pub fn freestream_from_atmosphere(
    atmosphere: &Us1976Values,
    altitude_m: f64,
    velocity_m_s: f64,
    mach: f64,
    gravity_m_s2: f64,
) -> Freestream {
    let gamma = components::air_compute_gamma(atmosphere.temperature_k);
    let cp = components::air_compute_cp(atmosphere.temperature_k);
    let stagnation_temperature_k =
        atmosphere.temperature_k * (1.0 + (gamma - 1.0) / 2.0 * mach * mach);
    let stagnation_pressure_pa = atmosphere.pressure_pa
        * (1.0 + (gamma - 1.0) / 2.0 * mach * mach).powf(gamma / (gamma - 1.0));
    Freestream {
        pressure_pa: atmosphere.pressure_pa,
        temperature_k: atmosphere.temperature_k,
        density_kg_m3: atmosphere.density_kg_m3,
        dynamic_viscosity_pa_s: atmosphere.dynamic_viscosity_pa_s,
        gravity_m_s2,
        gamma,
        cp_j_kgk: cp,
        r_j_kgk: GAS_CONSTANT_AIR,
        speed_of_sound_m_s: atmosphere.speed_of_sound_m_s,
        velocity_m_s,
        mach,
        stagnation_temperature_k,
        stagnation_pressure_pa,
        altitude_m,
    }
}

/// The freestream at one design point, where the velocity *is* `a * mach`.
///
/// `turbofan_sizing` has no flight state to read a velocity off: it is handed
/// a Mach number and an altitude and forms the velocity from them, which is
/// what this reproduces.
pub(super) fn build_freestream(altitude_m: f64, mach: f64, gravity_m_s2: f64) -> Freestream {
    let atmosphere = us1976_compute_values(altitude_m, 0.0);
    let velocity_m_s = atmosphere.speed_of_sound_m_s * mach;
    freestream_from_atmosphere(&atmosphere, altitude_m, velocity_m_s, mach, gravity_m_s2)
}

/// One pass through the network from `Ram` to both nozzles, up to but not
/// including the thrust process (which the caller runs with the scale factor
/// appropriate to the pass).
///
/// Returns the ten component outputs in flow order, and the two low-pressure
/// compressor reference stagnation quantities the thrust process normalizes
/// against.
#[allow(clippy::type_complexity)] // one pass legitimately yields every station
pub(super) fn walk_network(
    freestream: &Freestream,
    inputs: &TurbofanInputs,
    params: &VehicleBuilderParams,
) -> (
    RamOutput,
    CompressionNozzleOutput,
    CompressorOutput,
    CompressorOutput,
    CompressorOutput,
    CombustorOutput,
    TurbineOutput,
    TurbineOutput,
    ExpansionNozzleOutput,
    ExpansionNozzleOutput,
) {
    let ram = components::ram(freestream);

    let inlet = components::compression_nozzle(
        freestream,
        ram.stagnation_temperature_k,
        ram.stagnation_pressure_pa,
        params.inlet_pressure_ratio,
        params.inlet_polytropic_efficiency,
        params.inlet_pressure_recovery,
    );

    let lpc = components::compressor(
        freestream,
        inlet.stagnation_temperature_k,
        inlet.stagnation_pressure_pa,
        params.lpc_pressure_ratio,
        params.lpc_polytropic_efficiency,
    );

    let hpc_pressure_ratio = (inputs.overall_pressure_ratio / params.lpc_pressure_ratio).max(1.0);
    let hpc = components::compressor(
        freestream,
        lpc.stagnation_temperature_k,
        lpc.stagnation_pressure_pa,
        hpc_pressure_ratio,
        params.hpc_polytropic_efficiency,
    );

    // The fan hangs off the inlet, in parallel with the core compressors.
    let fan = components::compressor(
        freestream,
        inlet.stagnation_temperature_k,
        inlet.stagnation_pressure_pa,
        inputs.fan_pressure_ratio,
        params.fan_polytropic_efficiency,
    );

    let combustor = components::combustor(
        freestream,
        hpc.stagnation_temperature_k,
        hpc.stagnation_pressure_pa,
        inputs.turbine_inlet_temperature_k,
        params.combustor_pressure_ratio,
        params.combustor_efficiency,
        JET_A_SPECIFIC_ENERGY_J_KG,
    );

    // The high-pressure turbine drives the high-pressure compressor only; its
    // bypass ratio is set to zero so the fan work is not double-counted here.
    let hpt = components::turbine(
        freestream,
        combustor.stagnation_temperature_k,
        combustor.stagnation_pressure_pa,
        combustor.fuel_to_air_ratio,
        hpc.work_done_j_kg,
        fan.work_done_j_kg,
        0.0,
        params.turbine_mechanical_efficiency,
        params.turbine_polytropic_efficiency,
    );

    // The low-pressure turbine drives the low-pressure compressor and the fan.
    let lpt = components::turbine(
        freestream,
        hpt.stagnation_temperature_k,
        hpt.stagnation_pressure_pa,
        combustor.fuel_to_air_ratio,
        lpc.work_done_j_kg,
        fan.work_done_j_kg,
        inputs.bypass_ratio,
        params.turbine_mechanical_efficiency,
        params.turbine_polytropic_efficiency,
    );

    let core_nozzle = components::expansion_nozzle(
        freestream,
        lpt.stagnation_temperature_k,
        lpt.stagnation_pressure_pa,
        params.core_nozzle_pressure_ratio,
        params.core_nozzle_polytropic_efficiency,
    );

    let fan_nozzle = components::expansion_nozzle(
        freestream,
        fan.stagnation_temperature_k,
        fan.stagnation_pressure_pa,
        params.fan_nozzle_pressure_ratio,
        params.fan_nozzle_polytropic_efficiency,
    );

    (
        ram,
        inlet,
        lpc,
        hpc,
        fan,
        combustor,
        hpt,
        lpt,
        core_nozzle,
        fan_nozzle,
    )
}
