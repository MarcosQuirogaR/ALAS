// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Constant-property free-turbine cycle reconstructed at the catalogue's
//! per-engine maximum-cruise shaft-power and fuel-flow anchors. OPR and TIT
//! are explicit design assumptions, not measured OEM internal station data.
//! This preview does not alter the mission power model or supply an off-design
//! engine deck. SI units, positive compressor work and shaft output throughout.
//! Component balances follow the same perfect-gas/polytropic relations as
//! `cycle`; the gas-generator turbine drives both core compressors.

use alas_config::{PropulsionCycleConfig, TurbopropEngineSpec};

use crate::cycle::states::{
    relative_entropy_j_kgk, CycleBranch, CycleGas, CycleStation, EntropyReference, StateKind,
};

/// Thermodynamic reconstruction of one engine's cruise anchors.
#[derive(Debug, Clone, PartialEq)]
pub struct TurbopropCycleStates {
    /// Entropy reference: ambient static temperature (K) and pressure (Pa).
    pub reference: EntropyReference,
    /// Ambient static state, with zero reference entropy.
    pub freestream_static: CycleStation,
    /// Total states 0 through 6, followed by static exhaust state 9.
    pub core: Vec<CycleStation>,
    /// Air flow inferred from combustor balance and the fuel-flow anchor, kg/s.
    pub air_mass_flow_kg_s: f64,
    /// Catalogue cruise fuel flow per engine, kg/s.
    pub fuel_mass_flow_kg_s: f64,
    /// Delivered shaft power, after mechanical transmission losses, W.
    pub shaft_power_w: f64,
}

/// Solve the anchored free-turbine cycle, rejecting invalid or infeasible inputs.
pub fn compute_turboprop_cycle_states(
    spec: &TurbopropEngineSpec,
    mach: f64,
    altitude_m: f64,
    cfg: &PropulsionCycleConfig,
) -> Result<TurbopropCycleStates, String> {
    if !mach.is_finite() || mach < 0.0 || !altitude_m.is_finite() {
        return Err("Flight condition must be finite with non-negative Mach.".into());
    }
    for (name, value) in [
        ("cold-gas heat capacity", cfg.cp_cold_j_kgk),
        ("hot-gas heat capacity", cfg.cp_hot_j_kgk),
        ("fuel heating value", cfg.fuel_heating_value_kj_kg),
        ("cruise shaft power", spec.maximum_cruise_shaft_power_kw),
        ("cruise fuel flow", spec.maximum_cruise_fuel_flow_kg_h),
        (
            "turbine inlet temperature",
            cfg.turboprop_turbine_inlet_temperature_k,
        ),
    ] {
        if !value.is_finite() || value <= 0.0 {
            return Err(format!("{name} must be finite and positive."));
        }
    }
    for (name, value) in [
        ("inlet recovery", cfg.inlet_pressure_recovery),
        ("LPC efficiency", cfg.lpc_polytropic_efficiency),
        ("HPC efficiency", cfg.hpc_polytropic_efficiency),
        ("combustor pressure ratio", cfg.combustor_pressure_ratio),
        ("combustor efficiency", cfg.combustor_efficiency),
        ("gas-generator efficiency", cfg.hpt_polytropic_efficiency),
        ("power-turbine efficiency", cfg.lpt_polytropic_efficiency),
        ("mechanical efficiency", cfg.turbine_mechanical_efficiency),
        ("nozzle pressure ratio", cfg.core_nozzle_pressure_ratio),
        ("nozzle efficiency", cfg.core_nozzle_efficiency),
    ] {
        if !value.is_finite() || value <= 0.0 || value > 1.0 {
            return Err(format!("{name} must be in (0, 1]."));
        }
    }
    let (gc, gh) = (cfg.gamma_cold, cfg.gamma_hot);
    let opr = cfg.turboprop_overall_pressure_ratio;
    let split = cfg.lpc_pressure_ratio_split;
    if !gc.is_finite()
        || gc <= 1.0
        || !gh.is_finite()
        || gh <= 1.0
        || !opr.is_finite()
        || !split.is_finite()
        || split < 1.0
        || opr < split
    {
        return Err("Gas gamma must exceed one; OPR must be at least the LPC ratio >= 1.".into());
    }
    let atmosphere =
        alas_atmo::Atmosphere::try_new(altitude_m).map_err(|error| error.to_string())?;
    let reference = EntropyReference {
        temperature_k: atmosphere.temperature(),
        pressure_pa: atmosphere.pressure(),
    };
    let (t0, p0) = (reference.temperature_k, reference.pressure_pa);
    let (cpc, cph) = (cfg.cp_cold_j_kgk, cfg.cp_hot_j_kgk);
    let ram = 1.0 + (gc - 1.0) * mach * mach / 2.0;
    let tt0 = t0 * ram;
    let pt0 = p0 * ram.powf(gc / (gc - 1.0));
    let (tt2, pt2) = (tt0, pt0 * cfg.inlet_pressure_recovery);
    let tt25 = tt2 * split.powf((gc - 1.0) / (gc * cfg.lpc_polytropic_efficiency));
    let pt25 = pt2 * split;
    let tt3 = tt25 * (opr / split).powf((gc - 1.0) / (gc * cfg.hpc_polytropic_efficiency));
    let pt3 = pt2 * opr;
    let tt4 = cfg.turboprop_turbine_inlet_temperature_k;
    let pt4 = pt3 * cfg.combustor_pressure_ratio;
    let denominator = cfg.combustor_efficiency * cfg.fuel_heating_value_kj_kg * 1000.0 - cph * tt4;
    let f = (cph * tt4 - cpc * tt3) / denominator;
    if tt4 <= tt3 || denominator <= 0.0 || !f.is_finite() || f <= 0.0 {
        return Err(
            "Combustor cannot supply the requested temperature with positive fuel addition.".into(),
        );
    }
    let fuel_mass_flow_kg_s = spec.cruise_fuel_flow_per_engine_kg_h() / 3600.0;
    let air_mass_flow_kg_s = fuel_mass_flow_kg_s / f;
    let shaft_power_w = spec.maximum_cruise_shaft_power_kw * 1000.0;
    let hot_capacity = (1.0 + f) * cph * cfg.turbine_mechanical_efficiency;
    let tt45 = tt4 - cpc * (tt3 - tt2) / hot_capacity;
    let tt5 = tt45 - shaft_power_w / (air_mass_flow_kg_s * hot_capacity);
    if tt45 <= 0.0 || tt5 <= 0.0 {
        return Err("Required compressor/shaft work exceeds available turbine enthalpy.".into());
    }
    let pt45 = pt4 * (tt45 / tt4).powf(gh / ((gh - 1.0) * cfg.hpt_polytropic_efficiency));
    let pt5 = pt45 * (tt5 / tt45).powf(gh / ((gh - 1.0) * cfg.lpt_polytropic_efficiency));
    let pt6 = pt5 * cfg.core_nozzle_pressure_ratio;
    if pt6 <= p0 {
        return Err(
            "Requested shaft power leaves insufficient exhaust pressure above ambient.".into(),
        );
    }
    // Combine h_t = h + V²/2 with V² = gamma R T at choking.
    // Nozzle efficiency changes critical pressure, not sonic T/Tt = 2/(gamma+1).
    let critical_base = 1.0 - (gh - 1.0) / ((gh + 1.0) * cfg.core_nozzle_efficiency);
    let critical_pressure = pt6 * critical_base.max(0.0).powf(gh / (gh - 1.0));
    let p9 = p0.max(critical_pressure);
    let t9 = tt5 * (1.0 - cfg.core_nozzle_efficiency * (1.0 - (p9 / pt6).powf((gh - 1.0) / gh)));
    use CycleBranch::{Core, Inlet};
    use CycleGas::{Cold, Hot};
    use StateKind::{Stagnation, Static};
    let station = |station, name, branch, kind, gas, temperature_k, pressure_pa| CycleStation {
        station,
        name,
        branch,
        kind,
        gas,
        temperature_k,
        pressure_pa,
        entropy_j_kgk: relative_entropy_j_kgk(cfg, gas, reference, temperature_k, pressure_pa),
    };
    let freestream_static = station("0s", "Freestream", Inlet, Static, Cold, t0, p0);
    let core = vec![
        station("0", "Ram", Inlet, Stagnation, Cold, tt0, pt0),
        station("2", "Inlet", Inlet, Stagnation, Cold, tt2, pt2),
        station("25", "LPC exit", Core, Stagnation, Cold, tt25, pt25),
        station("3", "HPC exit", Core, Stagnation, Cold, tt3, pt3),
        station("4", "Combustor exit", Core, Stagnation, Hot, tt4, pt4),
        station(
            "45",
            "Gas-generator turbine exit",
            Core,
            Stagnation,
            Hot,
            tt45,
            pt45,
        ),
        station("5", "Power-turbine exit", Core, Stagnation, Hot, tt5, pt5),
        station("6", "Exhaust nozzle inlet", Core, Stagnation, Hot, tt5, pt6),
        station("9", "Exhaust", Core, Static, Hot, t9, p9),
    ];
    if core
        .iter()
        .chain(std::iter::once(&freestream_static))
        .any(|s| {
            !s.temperature_k.is_finite()
                || s.temperature_k <= 0.0
                || !s.pressure_pa.is_finite()
                || s.pressure_pa <= 0.0
                || !s.entropy_j_kgk.is_finite()
        })
        || !air_mass_flow_kg_s.is_finite()
        || air_mass_flow_kg_s <= 0.0
        || !fuel_mass_flow_kg_s.is_finite()
        || !shaft_power_w.is_finite()
    {
        return Err("Cycle produced non-finite or non-positive thermodynamic states.".into());
    }
    Ok(TurbopropCycleStates {
        reference,
        freestream_static,
        core,
        air_mass_flow_kg_s,
        fuel_mass_flow_kg_s,
        shaft_power_w,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn spec() -> TurbopropEngineSpec {
        alas_config::engines::database()
            .iter()
            .find_map(|e| e.turboprop.clone())
            .unwrap()
    }

    fn solve(spec: &TurbopropEngineSpec, cfg: &PropulsionCycleConfig) -> TurbopropCycleStates {
        compute_turboprop_cycle_states(spec, 0.45, 5181.6, cfg).unwrap()
    }

    fn close(a: f64, b: f64) {
        assert!(
            (a - b).abs() < 1e-9 * a.abs().max(b.abs()).max(1.0),
            "{a} != {b}"
        );
    }

    #[test]
    fn compressor_combustor_and_shaft_energy_close() {
        let cfg = PropulsionCycleConfig::default();
        let spec = spec();
        let result = solve(&spec, &cfg);
        let s = &result.core;
        let f = result.fuel_mass_flow_kg_s / result.air_mass_flow_kg_s;
        let hot_capacity = (1.0 + f) * cfg.cp_hot_j_kgk;
        close(
            cfg.cp_cold_j_kgk * (s[3].temperature_k - s[1].temperature_k),
            hot_capacity
                * cfg.turbine_mechanical_efficiency
                * (s[4].temperature_k - s[5].temperature_k),
        );
        close(
            cfg.cp_cold_j_kgk * s[3].temperature_k
                + f * cfg.combustor_efficiency * cfg.fuel_heating_value_kj_kg * 1000.0,
            hot_capacity * s[4].temperature_k,
        );
        close(
            result.shaft_power_w,
            result.air_mass_flow_kg_s
                * hot_capacity
                * cfg.turbine_mechanical_efficiency
                * (s[5].temperature_k - s[6].temperature_k),
        );
        close(
            result.fuel_mass_flow_kg_s * 3600.0,
            spec.maximum_cruise_fuel_flow_kg_h / 2.0,
        );
        close(
            s[3].pressure_pa / s[1].pressure_pa,
            cfg.turboprop_overall_pressure_ratio,
        );
        close(s[0].entropy_j_kgk, 0.0);
        for pair in s.windows(2) {
            assert!(pair[1].entropy_j_kgk >= pair[0].entropy_j_kgk - 1e-8);
        }
        for pair in s[4..].windows(2) {
            assert!(pair[1].pressure_pa <= pair[0].pressure_pa);
            assert!(pair[1].temperature_k <= pair[0].temperature_k);
        }
    }

    #[test]
    fn flight_design_and_catalogue_parameters_change_the_solved_states() {
        let cfg = PropulsionCycleConfig::default();
        let spec = spec();
        let base = solve(&spec, &cfg);
        let mut changed = cfg.clone();
        changed.turboprop_overall_pressure_ratio *= 1.05;
        assert!(solve(&spec, &changed).core[3].temperature_k > base.core[3].temperature_k);
        changed = cfg.clone();
        changed.turboprop_turbine_inlet_temperature_k += 40.0;
        assert!(solve(&spec, &changed).air_mass_flow_kg_s < base.air_mass_flow_kg_s);
        changed = cfg.clone();
        changed.hpc_polytropic_efficiency *= 0.98;
        assert!(solve(&spec, &changed).core[3].temperature_k > base.core[3].temperature_k);
        let mut engine = spec.clone();
        engine.maximum_cruise_shaft_power_kw *= 0.95;
        assert!(solve(&engine, &cfg).core[6].temperature_k > base.core[6].temperature_k);
        engine = spec.clone();
        engine.maximum_cruise_fuel_flow_kg_h *= 1.05;
        let fueled = solve(&engine, &cfg);
        assert!(fueled.core[6].temperature_k > base.core[6].temperature_k);
        close(fueled.air_mass_flow_kg_s / base.air_mass_flow_kg_s, 1.05);
        let faster = compute_turboprop_cycle_states(&spec, 0.50, 5181.6, &cfg).unwrap();
        assert!(faster.core[0].temperature_k > base.core[0].temperature_k);
        let higher = compute_turboprop_cycle_states(&spec, 0.45, 5500.0, &cfg).unwrap();
        assert!(higher.core[0].pressure_pa < base.core[0].pressure_pa);
    }

    #[test]
    fn nozzle_efficiency_preserves_sonic_enthalpy_and_subsonic_expansion() {
        let spec = spec();
        let mut cfg = PropulsionCycleConfig::default();
        let mut engine = spec.clone();
        engine.maximum_cruise_shaft_power_kw *= 0.1;
        for eta in [1.0, 0.95, 0.8] {
            cfg.core_nozzle_efficiency = eta;
            let result = solve(&engine, &cfg);
            let inlet = &result.core[7];
            let exit = &result.core[8];
            assert!(exit.pressure_pa > result.reference.pressure_pa);
            let rg = cfg.cp_hot_j_kgk * (cfg.gamma_hot - 1.0) / cfg.gamma_hot;
            close(
                2.0 * cfg.cp_hot_j_kgk * (inlet.temperature_k - exit.temperature_k),
                cfg.gamma_hot * rg * exit.temperature_k,
            );
        }
        cfg.core_nozzle_efficiency = 0.05;
        let result = solve(&engine, &cfg);
        close(result.core[8].pressure_pa, result.reference.pressure_pa);
        let mach_squared = 2.0
            * (result.core[7].temperature_k / result.core[8].temperature_k - 1.0)
            / (cfg.gamma_hot - 1.0);
        assert!((0.0..1.0).contains(&mach_squared));
    }

    #[test]
    fn invalid_inputs_and_impossible_shaft_draw_are_rejected() {
        let spec = spec();
        let cfg = PropulsionCycleConfig::default();
        for (mach, altitude) in [
            (f64::NAN, 0.0),
            (-0.1, 0.0),
            (0.4, f64::INFINITY),
            (0.4, 100_000.0),
            (0.4, -10_000.0),
        ] {
            assert!(compute_turboprop_cycle_states(&spec, mach, altitude, &cfg).is_err());
        }
        let mut changed = cfg.clone();
        changed.hpc_polytropic_efficiency = 1.1;
        assert!(compute_turboprop_cycle_states(&spec, 0.45, 5181.6, &changed).is_err());
        changed = cfg.clone();
        changed.turboprop_turbine_inlet_temperature_k = 300.0;
        assert!(compute_turboprop_cycle_states(&spec, 0.45, 5181.6, &changed).is_err());
        let mut engine = spec.clone();
        engine.maximum_cruise_shaft_power_kw *= 100.0;
        assert!(compute_turboprop_cycle_states(&engine, 0.45, 5181.6, &cfg).is_err());
        engine = spec;
        engine.maximum_cruise_fuel_flow_kg_h = 0.0;
        assert!(compute_turboprop_cycle_states(&engine, 0.45, 5181.6, &cfg).is_err());
    }
}
