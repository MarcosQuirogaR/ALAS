// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from mission reference/Components/Energy/Converters/{Ram,Compression_Nozzle,
// Compressor,Fan,Combustor,Turbine,Expansion_Nozzle}.py,
// mission reference/Components/Energy/Processes/Thrust.py and
// mission reference/Methods/Propulsion/fm_id.py.
// Upstream: mission reference 2.5.2, LGPL-2.1 (relicensed under GPL-2.0-or-later per
// LGPL-2.1 section 3; compatible with this program's AGPL-3.0-or-later).
// Reference: alas @ rust-port-baseline.

//! The individual gas-turbine components `turbofan_sizing` links together.
//!
//! Each function is one mission reference `compute` method: it takes the constant
//! freestream state (which [`super`]'s network walk fills in once, from the
//! `Ram` outputs, exactly as mission reference stores them on `conditions.freestream`)
//! plus the one component's linked stagnation inputs, and returns the outputs
//! the next component reads. The functions carry no state of their own: the
//! in-place mutation of shared component objects that makes the *fixture*
//! generator monkeypatch its way around a cruise/sea-level-static aliasing
//! trap (see `gen_prop_mission_turbofan.py`) is a property of mission reference's object
//! model, not of the arithmetic, and disappears here where each pass is a
//! separate call with its own values.

use super::{
    CombustorOutput, CompressionNozzleOutput, CompressorOutput, ExpansionNozzleOutput, Freestream,
    PartPowerModel, RamOutput, ThrustOutput, TurbineOutput,
};

/// `Attributes.Gases.Air.compute_gamma`: a cubic fit of the ratio of specific
/// heats to static temperature `t_k` (kelvin), valid 233-1273 K upstream.
pub(super) fn air_compute_gamma(t_k: f64) -> f64 {
    let c = [1.629e-10, -3.588e-07, 0.000_141_8, 1.386];
    c[0] * t_k * t_k * t_k + c[1] * t_k * t_k + c[2] * t_k + c[3]
}

/// `Attributes.Gases.Air.compute_cp`: a cubic fit of the specific heat at
/// constant pressure, J/(kg*K), to static temperature `t_k`, valid 123-673 K.
pub(super) fn air_compute_cp(t_k: f64) -> f64 {
    let c = [-7.357e-07, 0.001_307, -0.5558, 1074.0];
    c[0] * t_k * t_k * t_k + c[1] * t_k * t_k + c[2] * t_k + c[3]
}

/// `Methods/Propulsion/fm_id.py`: the compressible mass-flow function `f(M)`,
/// used by the expansion nozzles to form their freestream-to-exit area ratio.
fn fm_id(mach: f64, gamma: f64) -> f64 {
    let m0 = (gamma + 1.0) / (2.0 * (gamma - 1.0));
    let m1 = ((gamma + 1.0) / 2.0).powf(m0);
    let m2 = (1.0 + (gamma - 1.0) / 2.0 * mach * mach).powf(m0);
    m1 * mach / m2
}

/// `Ram.compute`: stagnation state from the static freestream, plus the gas
/// properties (`gamma`, `cp`, `r`) every downstream component then reads off
/// `conditions.freestream`.
///
/// `gamma` and `cp` come from the [`air_compute_gamma`]/[`air_compute_cp`]
/// fits at the *static* temperature; `r` is the fixed air gas constant. The
/// caller passes these already-evaluated on the [`Freestream`] because mission reference
/// evaluates them (identically) before `Ram` runs and `Ram` merely restates
/// them.
pub(super) fn ram(freestream: &Freestream) -> RamOutput {
    let gamma = freestream.gamma;
    let m = freestream.mach;
    let stagnation_temperature_k = freestream.temperature_k * (1.0 + (gamma - 1.0) / 2.0 * m * m);
    let stagnation_pressure_pa =
        freestream.pressure_pa * (1.0 + (gamma - 1.0) / 2.0 * m * m).powf(gamma / (gamma - 1.0));
    RamOutput {
        stagnation_temperature_k,
        stagnation_pressure_pa,
        isentropic_expansion_factor: gamma,
        specific_heat_at_constant_pressure_j_kgk: freestream.cp_j_kgk,
        gas_specific_constant_j_kgk: freestream.r_j_kgk,
    }
}

/// `Compression_Nozzle.compute`, `compressibility_effects=False` branch (the
/// only one the inlet nozzle this program builds ever takes).
///
/// The negative-pressure guard is reproduced: when the recovered stagnation
/// pressure falls below ambient, it is clamped to ambient (upstream's
/// `Pt_out[Pt_out<Po] = Po`), which drives the recovered Mach and exit
/// velocity to zero, exactly what the sea-level-static point produces.
/// `static_pressure` is not among the outputs because this branch never sets
/// it upstream.
pub(super) fn compression_nozzle(
    freestream: &Freestream,
    tt_in: f64,
    pt_in: f64,
    pressure_ratio: f64,
    polytropic_efficiency: f64,
    pressure_recovery: f64,
) -> CompressionNozzleOutput {
    let gamma = freestream.gamma;
    let cp = freestream.cp_j_kgk;
    let po = freestream.pressure_pa;

    let mut pt_out = pt_in * pressure_ratio * pressure_recovery;
    let tt_out = tt_in
        * (pressure_ratio * pressure_recovery)
            .powf((gamma - 1.0) / (gamma * polytropic_efficiency));
    let ht_out = cp * tt_out;

    // in case pressures go too low (upstream clamps then warns)
    if pt_out < po {
        pt_out = po;
    }

    let mach = ((((pt_out / po).powf((gamma - 1.0) / gamma)) - 1.0) * 2.0 / (gamma - 1.0)).sqrt();
    let static_temperature_k = tt_out / (1.0 + (gamma - 1.0) / 2.0 * mach * mach);
    let static_enthalpy_j_kg = cp * static_temperature_k;
    let velocity_m_s = (2.0 * (ht_out - static_enthalpy_j_kg)).sqrt();

    CompressionNozzleOutput {
        stagnation_temperature_k: tt_out,
        stagnation_pressure_pa: pt_out,
        stagnation_enthalpy_j_kg: ht_out,
        mach,
        static_temperature_k,
        static_enthalpy_j_kg,
        velocity_m_s,
    }
}

/// `Compressor.compute` and, identically, `Fan.compute`: a fixed pressure
/// ratio and polytropic efficiency, returning the stagnation state and the
/// specific work done (which the driving turbine reads back).
pub(super) fn compressor(
    freestream: &Freestream,
    tt_in: f64,
    pt_in: f64,
    pressure_ratio: f64,
    polytropic_efficiency: f64,
) -> CompressorOutput {
    let gamma = freestream.gamma;
    let cp = freestream.cp_j_kgk;
    let ht_in = cp * tt_in;
    let pt_out = pt_in * pressure_ratio;
    let tt_out = tt_in * pressure_ratio.powf((gamma - 1.0) / (gamma * polytropic_efficiency));
    let ht_out = cp * tt_out;
    CompressorOutput {
        stagnation_temperature_k: tt_out,
        stagnation_pressure_pa: pt_out,
        stagnation_enthalpy_j_kg: ht_out,
        work_done_j_kg: ht_out - ht_in,
    }
}

/// `Combustor.compute`: fixes the exit stagnation temperature to the turbine
/// inlet temperature, applies the burner pressure ratio, and backs out the
/// fuel-air ratio from the enthalpy rise and the fuel heating value.
///
/// `nondim_mass_ratio` is left at its upstream default of 1 (nothing in this
/// network sets it), so it drops out of every term.
pub(super) fn combustor(
    freestream: &Freestream,
    tt_in: f64,
    pt_in: f64,
    turbine_inlet_temperature_k: f64,
    pressure_ratio: f64,
    efficiency: f64,
    fuel_specific_energy_j_kg: f64,
) -> CombustorOutput {
    let cp = freestream.cp_j_kgk;
    let nondim_r = 1.0;
    let tt4 = turbine_inlet_temperature_k;
    let ht4 = cp * tt4 * nondim_r;
    let ht_in = cp * tt_in * nondim_r;
    let fuel_to_air_ratio = (ht4 - ht_in) / (efficiency * fuel_specific_energy_j_kg - ht4);
    let ht_out = cp * tt4;
    CombustorOutput {
        stagnation_temperature_k: tt4,
        stagnation_pressure_pa: pt_in * pressure_ratio,
        stagnation_enthalpy_j_kg: ht_out,
        fuel_to_air_ratio,
    }
}

/// `Turbine.compute`: the enthalpy drop that balances the shaft work of the
/// compressor(s) and, for the low-pressure spool, the fan.
///
/// `bypass_ratio` scales the fan work; the high-pressure turbine is called
/// with it set to zero (mission reference sets `high_pressure_turbine.inputs.bypass_ratio
/// = 0.0` "to ensure that fan not linked here"), so only the low-pressure
/// turbine carries the fan term. No shaft-power off-take is fitted, so that
/// term is zero.
#[allow(clippy::too_many_arguments)] // one turbine call names every mission reference input it reads
pub(super) fn turbine(
    freestream: &Freestream,
    tt_in: f64,
    pt_in: f64,
    fuel_to_air_ratio: f64,
    compressor_work_j_kg: f64,
    fan_work_j_kg: f64,
    bypass_ratio: f64,
    mechanical_efficiency: f64,
    polytropic_efficiency: f64,
) -> TurbineOutput {
    let gamma = freestream.gamma;
    let cp = freestream.cp_j_kgk;
    let delta_h = -1.0 / (1.0 + fuel_to_air_ratio) / mechanical_efficiency
        * (compressor_work_j_kg + bypass_ratio * fan_work_j_kg);
    let tt_out = tt_in + delta_h / cp;
    let pt_out = pt_in * (tt_out / tt_in).powf(gamma / ((gamma - 1.0) * polytropic_efficiency));
    TurbineOutput {
        stagnation_temperature_k: tt_out,
        stagnation_pressure_pa: pt_out,
        stagnation_enthalpy_j_kg: cp * tt_out,
    }
}

/// `Expansion_Nozzle.compute`: expands a stagnation state toward ambient,
/// choking at Mach 1 when the pressure ratio calls for it.
///
/// Reproduces the two upstream quirks exactly: the stagnation-temperature
/// exponent multiplies (not divides) by the polytropic efficiency, unlike the
/// compression nozzle; and the negative-pressure clamp raises `Pt_out` to
/// ambient before the Mach is formed. The area ratio uses [`fm_id`] at the
/// freestream and exit Mach numbers.
pub(super) fn expansion_nozzle(
    freestream: &Freestream,
    tt_in: f64,
    pt_in: f64,
    pressure_ratio: f64,
    polytropic_efficiency: f64,
) -> ExpansionNozzleOutput {
    let gamma = freestream.gamma;
    let cp = freestream.cp_j_kgk;
    let po = freestream.pressure_pa;
    let r = freestream.r_j_kgk;
    let mo = freestream.mach;
    let pto = freestream.stagnation_pressure_pa;
    let tto = freestream.stagnation_temperature_k;

    let mut pt_out = pt_in * pressure_ratio;
    let tt_out = tt_in * pressure_ratio.powf((gamma - 1.0) / gamma * polytropic_efficiency);
    let ht_out = cp * tt_out;

    // A cap so pressure doesn't go negative.
    if pt_out < po {
        pt_out = po;
    }

    let mut mach =
        ((((pt_out / po).powf((gamma - 1.0) / gamma)) - 1.0) * 2.0 / (gamma - 1.0)).sqrt();
    let p_out = if mach >= 1.0 {
        // Choked: the exit Mach is pinned to 1 and the static pressure follows
        // the choked isentropic relation.
        mach = 1.0;
        pt_out / (1.0 + (gamma - 1.0) / 2.0 * mach * mach).powf(gamma / (gamma - 1.0))
    } else {
        // Subsonic: the flow expands fully to ambient.
        po
    };

    let static_temperature_k = tt_out / (1.0 + (gamma - 1.0) / 2.0 * mach * mach);
    let static_enthalpy_j_kg = cp * static_temperature_k;
    let velocity_m_s = (2.0 * (ht_out - static_enthalpy_j_kg)).sqrt();
    let density_kg_m3 = p_out / (r * static_temperature_k);
    let area_ratio =
        fm_id(mo, gamma) / fm_id(mach, gamma) * (1.0 / (pt_out / pto)) * (tt_out / tto).sqrt();

    ExpansionNozzleOutput {
        stagnation_temperature_k: tt_out,
        stagnation_pressure_pa: pt_out,
        stagnation_enthalpy_j_kg: ht_out,
        mach,
        static_temperature_k,
        density_kg_m3,
        static_enthalpy_j_kg,
        velocity_m_s,
        static_pressure_pa: p_out,
        area_ratio,
    }
}

/// Seconds per hour: mission reference's `Units.hour`, the factor `Thrust.compute` folds
/// into TSFC and then back out of the fuel-flow rate, so the two cancel.
const SECONDS_PER_HOUR: f64 = 3600.0;

/// The design-point reference temperature `Thrust` normalizes core mass flow
/// against, K. `Thrust.__defaults__.reference_temperature` (288.15).
const REFERENCE_TEMPERATURE_K: f64 = 288.15;

/// The design-point reference pressure, Pa.
/// `Thrust.__defaults__.reference_pressure` (1.01325e5).
const REFERENCE_PRESSURE_PA: f64 = 1.013_25e5;

/// `Thrust.compute`: the Cantwell nondimensional-thrust method.
///
/// `nondimensional_massflow` is the sized core-flow scale factor: it is zero
/// on the cruise sizing pass (before [`super`] backs it out of the design
/// thrust), so the dimensional thrust, mass flow, fuel flow and power all come
/// out zero there while the specific thrust, TSFC and specific impulse remain
/// meaningful: faithfully what mission reference records when `Thrust.size` runs
/// `compute` before it has solved the scale factor.
///
/// The mission command is a normalized requested net-thrust fraction, not a
/// physical power-lever angle. Product fuel flow follows an empirical ICAO-LTO
/// schedule; the proportional predecessor law is reference-only.
#[allow(clippy::too_many_arguments)] // mirrors the inputs mission reference links onto Thrust
pub(super) fn compute_thrust(
    freestream: &Freestream,
    core_nozzle: &ExpansionNozzleOutput,
    fan_nozzle: &ExpansionNozzleOutput,
    fuel_to_air_ratio: f64,
    total_temperature_reference_k: f64,
    total_pressure_reference_pa: f64,
    bypass_ratio: f64,
    number_of_engines: f64,
    nondimensional_massflow: f64,
    throttle: f64,
    part_power_model: PartPowerModel,
) -> ThrustOutput {
    let gamma = freestream.gamma;
    let u0 = freestream.velocity_m_s;
    let a0 = freestream.speed_of_sound_m_s;
    let m0 = freestream.mach;
    let p0 = freestream.pressure_pa;
    let g = freestream.gravity_m_s2;

    let flow_through_core = 1.0 / (1.0 + bypass_ratio);
    let flow_through_fan = bypass_ratio / (1.0 + bypass_ratio);

    let core_nd = flow_through_core
        * (gamma * m0 * m0 * (core_nozzle.velocity_m_s / u0 - 1.0)
            + core_nozzle.area_ratio * (core_nozzle.static_pressure_pa / p0 - 1.0));
    let fan_nd = flow_through_fan
        * (gamma * m0 * m0 * (fan_nozzle.velocity_m_s / u0 - 1.0)
            + fan_nozzle.area_ratio * (fan_nozzle.static_pressure_pa / p0 - 1.0));
    let thrust_nd = core_nd + fan_nd;

    let fsp = 1.0 / (gamma * m0) * thrust_nd;
    // SFC_adjustment defaults to 0, so it is omitted from the (1 - adj) factor.
    let full_tsfc = fuel_to_air_ratio * g / (fsp * a0 * (1.0 + bypass_ratio)) * SECONDS_PER_HOUR;

    let mdot_core = nondimensional_massflow
        * (REFERENCE_TEMPERATURE_K / total_temperature_reference_k).sqrt()
        * (total_pressure_reference_pa / REFERENCE_PRESSURE_PA);
    let full_thrust = fsp * a0 * (1.0 + bypass_ratio) * mdot_core * number_of_engines;
    let full_fuel_flow = (full_thrust * full_tsfc / g).max(0.0) / SECONDS_PER_HOUR;
    // The frozen SUAVE compatibility path deliberately leaves the scalar
    // solver variable unbounded: SciPy/MINPACK is allowed to evaluate a
    // mathematical root above one and the fixture records that command. The
    // product path is the physical bounded command and clamps it at the
    // propulsion boundary. Keeping the choice here, where the two historical
    // semantics actually diverge, preserves parity without exposing an
    // impossible product-engine operating point.
    let command = match part_power_model {
        PartPowerModel::LegacyLinear => throttle,
        PartPowerModel::IcaoLtoFuelFlow { .. } => throttle.clamp(0.0, 1.0),
    };
    let (thrust_fraction, fuel_fraction) = match part_power_model {
        PartPowerModel::LegacyLinear => (command, command),
        PartPowerModel::IcaoLtoFuelFlow { fuel_flow_ratios } => {
            // Mission engines are operating; shutdown/windmilling is a
            // separate state the current solver does not expose. Commands
            // below the ICAO 7% idle rating therefore saturate at idle.
            let operating_fraction = command.max(0.07);
            (
                operating_fraction,
                part_power_fuel_fraction(operating_fraction, fuel_flow_ratios),
            )
        }
    };
    let fd2 = full_thrust * thrust_fraction;
    let fuel_flow_rate = full_fuel_flow * fuel_fraction;
    let tsfc = if fd2 > 0.0 {
        fuel_flow_rate * g * SECONDS_PER_HOUR / fd2
    } else if full_thrust == 0.0 && fsp > 0.0 {
        // The first sizing pass intentionally has zero dimensional capacity,
        // while its specific cycle quantities remain meaningful.
        full_tsfc
    } else {
        0.0
    };
    let isp = if fuel_flow_rate > 0.0 {
        fd2 / (fuel_flow_rate * g)
    } else if full_thrust == 0.0 && fuel_to_air_ratio > 0.0 {
        fsp * a0 * (1.0 + bypass_ratio) / (fuel_to_air_ratio * g)
    } else {
        0.0
    };
    let power = fd2 * u0;

    ThrustOutput {
        thrust_n: fd2,
        thrust_specific_fuel_consumption: tsfc,
        non_dimensional_thrust: fsp * thrust_fraction,
        core_mass_flow_rate_kg_s: mdot_core,
        fuel_flow_rate_kg_s: fuel_flow_rate,
        power_w: power,
        specific_impulse_s: isp,
    }
}

/// Shape-preserving cubic interpolation through normalized fuel-flow anchors
/// at thrust fractions `[0.07, 0.30, 0.85, 1.0]`.
pub(crate) fn part_power_fuel_fraction(thrust_fraction: f64, ratios: [f64; 4]) -> f64 {
    const X: [f64; 4] = [0.07, 0.30, 0.85, 1.0];
    let y = ratios;
    if y.iter().any(|value| !value.is_finite())
        || y.windows(2).any(|pair| pair[1] < pair[0])
        || y[0] < 0.0
        || y[3] <= 0.0
    {
        return f64::NAN;
    }

    let x = thrust_fraction.clamp(0.07, 1.0);
    if x == 0.07 || x == 1.0 {
        return y[if x == 0.07 { 0 } else { 3 }];
    }

    let mut secant = [0.0; 3];
    for i in 0..3 {
        secant[i] = (y[i + 1] - y[i]) / (X[i + 1] - X[i]);
    }
    let mut tangent = [0.0; 4];
    tangent[0] = secant[0];
    tangent[3] = secant[2];
    for i in 1..3 {
        tangent[i] = 0.5 * (secant[i - 1] + secant[i]);
    }
    // Fritsch-Carlson limiter: doi:10.1137/0717021.
    for i in 0..3 {
        if secant[i] == 0.0 {
            tangent[i] = 0.0;
            tangent[i + 1] = 0.0;
            continue;
        }
        let alpha = tangent[i] / secant[i];
        let beta = tangent[i + 1] / secant[i];
        let magnitude = alpha * alpha + beta * beta;
        if magnitude > 9.0 {
            let scale = 3.0 / magnitude.sqrt();
            tangent[i] = scale * alpha * secant[i];
            tangent[i + 1] = scale * beta * secant[i];
        }
    }

    let interval = (0..3).find(|&i| x <= X[i + 1]).unwrap_or(2);
    let width = X[interval + 1] - X[interval];
    let t = (x - X[interval]) / width;
    let h00 = (2.0 * t - 3.0) * t * t + 1.0;
    let h10 = ((t - 2.0) * t + 1.0) * t;
    let h01 = (-2.0 * t + 3.0) * t * t;
    let h11 = (t - 1.0) * t * t;
    (h00 * y[interval]
        + h10 * width * tangent[interval]
        + h01 * y[interval + 1]
        + h11 * width * tangent[interval + 1])
        .clamp(y[interval], y[interval + 1])
}

/// Back out the design core mass flow and its scale factor from the design
/// thrust, as `Thrust.size` does after running `compute`.
///
/// Returns `(mass_flow_rate_design, compressor_nondimensional_massflow)`.
pub(super) fn size_core_flow(
    design_thrust_n: f64,
    non_dimensional_thrust: f64,
    speed_of_sound_m_s: f64,
    bypass_ratio: f64,
    number_of_engines: f64,
    total_temperature_reference_k: f64,
    total_pressure_reference_pa: f64,
) -> (f64, f64) {
    let throttle = 1.0;
    let mdot_core = design_thrust_n
        / (non_dimensional_thrust
            * speed_of_sound_m_s
            * (1.0 + bypass_ratio)
            * number_of_engines
            * throttle);
    let mdhc = mdot_core
        / ((REFERENCE_TEMPERATURE_K / total_temperature_reference_k).sqrt()
            * (total_pressure_reference_pa / REFERENCE_PRESSURE_PA));
    (mdot_core, mdhc)
}
