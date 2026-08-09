// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/propulsion.py
// Reference: alas @ rust-port-baseline.

//! Separate-flow, two-spool turbofan on-design cycle.
//!
//! [`compute_turbofan_cycle`] evaluates the cycle at one flight condition:
//! ram/inlet stagnation state, a fan branch parallel to the core compressors
//! off the same inlet state, low- then high-pressure compression, the
//! combustor energy balance, the high- and low-pressure turbines that drive
//! them, and both nozzle expansions. It returns specific thrust per unit
//! *total* (core + bypass) mass flow, thrust-specific fuel consumption, the
//! thermal/propulsive/overall efficiency decomposition and every station
//! stagnation temperature -- or [`TurbofanCycleResult::cycle_feasible`] set
//! false with a reason when the requested parameters have no physical solution.
//!
//! Every equation is first-principles compressible flow (isentropic plus
//! polytropic-efficiency relations, energy and momentum conservation) with no
//! curve-fit constants; the only external input is the ambient state, read
//! from [`alas_atmo::Atmosphere::new`] -- the fitted model upstream's
//! `asb.Atmosphere(altitude=...)` selects when no method is named, not the
//! closed-form ISA.
//!
//! All station temperatures are stagnation ("total") temperatures in kelvin.
//! Station naming mirrors the standard two-spool notation: 0 = freestream,
//! t2 = post-inlet (fan/LPC face), t13 = post-fan (bypass duct), t25 =
//! post-LPC, t3 = post-HPC (combustor inlet), t4 = combustor exit (turbine
//! inlet temperature, the design input), t45 = post-HPT, t5 = post-LPT (core
//! nozzle inlet), t6 = core nozzle exit.

use alas_atmo::Atmosphere;
use alas_config::PropulsionCycleConfig;

pub mod sweeps;

/// Design-point flight condition and cycle parameters for one evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TurbofanCycleInputs {
    /// Freestream Mach number.
    pub mach: f64,
    /// Geometric altitude, m.
    pub altitude_m: f64,
    /// Bypass ratio (bypass mass flow / core mass flow).
    pub bypass_ratio: f64,
    /// Core (LPC * HPC) overall pressure ratio, i.e. `EngineSpec::overall_pressure_ratio`.
    pub overall_pressure_ratio: f64,
    /// Fan pressure ratio.
    pub fan_pressure_ratio: f64,
    /// Turbine inlet temperature (combustor exit stagnation temperature), K.
    pub turbine_inlet_temperature_k: f64,
}

/// Output of one on-design cycle evaluation.
///
/// When [`cycle_feasible`](Self::cycle_feasible) is false, every numeric field
/// is `NaN` and [`infeasibility_reason`](Self::infeasibility_reason) says why.
#[derive(Debug, Clone, PartialEq)]
pub struct TurbofanCycleResult {
    /// Whether the requested parameters have a physical solution.
    pub cycle_feasible: bool,
    /// Why the cycle is infeasible, or empty when it is feasible.
    pub infeasibility_reason: String,

    /// Specific thrust, `F / mdot_total`, m/s.
    pub specific_thrust_ms: f64,
    /// Thrust-specific fuel consumption, mg fuel / (N.s).
    pub tsfc_mg_ns: f64,
    /// Combustor fuel-air ratio.
    pub fuel_air_ratio: f64,

    /// Thermal efficiency (kinetic-energy gain / fuel power).
    pub thermal_efficiency: f64,
    /// Propulsive efficiency (thrust power / kinetic-energy gain).
    pub propulsive_efficiency: f64,
    /// Overall efficiency (thrust power / fuel power).
    pub overall_efficiency: f64,

    /// Post-inlet stagnation temperature (station t0/tt0), K.
    pub temperature_t0_k: f64,
    /// Post-fan stagnation temperature (station t13), K.
    pub temperature_t13_k: f64,
    /// Post-LPC stagnation temperature (station t25), K.
    pub temperature_t25_k: f64,
    /// Post-HPC / combustor-inlet stagnation temperature (station t3), K.
    pub temperature_t3_k: f64,
    /// Combustor-exit / turbine-inlet stagnation temperature (station t4), K.
    pub temperature_t4_k: f64,
    /// Post-HPT stagnation temperature (station t45), K.
    pub temperature_t45_k: f64,
    /// Post-LPT / core-nozzle-inlet stagnation temperature (station t5), K.
    pub temperature_t5_k: f64,
    /// Core-nozzle-exit stagnation temperature (station t6), K.
    pub temperature_t6_k: f64,

    /// Core-nozzle exit velocity, m/s.
    pub exit_velocity_core_ms: f64,
    /// Fan-nozzle exit velocity, m/s.
    pub exit_velocity_fan_ms: f64,
}

impl TurbofanCycleResult {
    /// A feasible result carrying nothing but the two labelling fields; the
    /// caller fills the numeric fields in as the cycle is walked.
    fn feasible_blank() -> Self {
        Self {
            cycle_feasible: true,
            infeasibility_reason: String::new(),
            specific_thrust_ms: f64::NAN,
            tsfc_mg_ns: f64::NAN,
            fuel_air_ratio: f64::NAN,
            thermal_efficiency: f64::NAN,
            propulsive_efficiency: f64::NAN,
            overall_efficiency: f64::NAN,
            temperature_t0_k: f64::NAN,
            temperature_t13_k: f64::NAN,
            temperature_t25_k: f64::NAN,
            temperature_t3_k: f64::NAN,
            temperature_t4_k: f64::NAN,
            temperature_t45_k: f64::NAN,
            temperature_t5_k: f64::NAN,
            temperature_t6_k: f64::NAN,
            exit_velocity_core_ms: f64::NAN,
            exit_velocity_fan_ms: f64::NAN,
        }
    }
}

/// A `cycle_feasible = false` result with every numeric field `NaN`.
fn infeasible(reason: String) -> TurbofanCycleResult {
    let mut result = TurbofanCycleResult::feasible_blank();
    result.cycle_feasible = false;
    result.infeasibility_reason = reason;
    result
}

/// Expand a stagnation state to ambient pressure (or choke), returning
/// `(exit velocity, exit temperature, exit pressure)`.
fn expand_nozzle(
    tt: f64,
    pt: f64,
    p_ambient: f64,
    gamma: f64,
    cp: f64,
    eta_n: f64,
) -> (f64, f64, f64) {
    let r_gas = cp * (gamma - 1.0) / gamma;
    let p_star_over_pt = (2.0 / (gamma + 1.0)).powf(gamma / (gamma - 1.0));
    let p_star = p_star_over_pt * pt;
    let choked = p_star >= p_ambient;
    let p_exit = if choked { p_star } else { p_ambient };
    let t_exit_ideal = tt * (p_exit / pt).powf((gamma - 1.0) / gamma);
    let t_exit = tt - eta_n * (tt - t_exit_ideal);
    let v_exit = if choked {
        (gamma * r_gas * t_exit).max(0.0).sqrt()
    } else {
        (2.0 * cp * (tt - t_exit)).max(0.0).sqrt()
    };
    (v_exit, t_exit, p_exit)
}

/// Evaluate the on-design separate-flow turbofan cycle at one flight condition.
///
/// See the module documentation for the returned quantities and the
/// station-by-station structure. The `cfg` supplies every component efficiency
/// and pressure loss; upstream defaults it to `PropulsionCycleConfig()`, which
/// callers reproduce with `PropulsionCycleConfig::default()`.
pub fn compute_turbofan_cycle(
    inputs: &TurbofanCycleInputs,
    cfg: &PropulsionCycleConfig,
) -> TurbofanCycleResult {
    let (gc, cpc) = (cfg.gamma_cold, cfg.cp_cold_j_kgk);
    let (gh, cph) = (cfg.gamma_hot, cfg.cp_hot_j_kgk);

    let atmo = Atmosphere::new(inputs.altitude_m);
    let t0 = atmo.temperature();
    let p0 = atmo.pressure();
    let a0 = atmo.speed_of_sound();
    let m0 = inputs.mach.max(0.0);
    let v0 = m0 * a0;

    let tt0 = t0 * (1.0 + 0.5 * (gc - 1.0) * m0.powi(2));
    let pt0_ideal = p0 * (1.0 + 0.5 * (gc - 1.0) * m0.powi(2)).powf(gc / (gc - 1.0));
    let pt0 = pt0_ideal * cfg.inlet_pressure_recovery;
    let (tt2, pt2) = (tt0, pt0);

    // -- Fan branch (parallel to the core compressors, same inlet state) -----
    let pi_f = inputs.fan_pressure_ratio.max(1.0);
    let tt13 = tt2 * pi_f.powf((gc - 1.0) / (gc * cfg.fan_polytropic_efficiency));
    let pt13 = pt2 * pi_f * cfg.fan_nozzle_pressure_ratio;

    // -- Core compressors (LPC then HPC) -------------------------------------
    let pi_lpc = cfg.lpc_pressure_ratio_split;
    let pi_hpc = (inputs.overall_pressure_ratio / pi_lpc).max(1.0);
    let tt25 = tt2 * pi_lpc.powf((gc - 1.0) / (gc * cfg.lpc_polytropic_efficiency));
    let tt3 = tt25 * pi_hpc.powf((gc - 1.0) / (gc * cfg.hpc_polytropic_efficiency));
    let pt3 = pt2 * pi_lpc * pi_hpc;

    // -- Combustor -----------------------------------------------------------
    let tt4 = inputs.turbine_inlet_temperature_k;
    if tt4 <= tt3 {
        return infeasible(format!(
            "Turbine inlet temperature ({tt4:.0} K) must exceed the compressor \
             discharge temperature ({tt3:.0} K)."
        ));
    }
    let pt4 = pt3 * cfg.combustor_pressure_ratio;
    let h_pr = cfg.fuel_heating_value_kj_kg * 1000.0;
    let denom = cfg.combustor_efficiency * h_pr - cph * tt4;
    if denom <= 0.0 {
        return infeasible(
            "Turbine inlet temperature too high relative to the fuel heating value.".to_owned(),
        );
    }
    let f_ratio = (cph * tt4 - cpc * tt3) / denom;
    if f_ratio <= 0.0 {
        return infeasible("Computed fuel-air ratio is non-positive.".to_owned());
    }

    // -- HPT (drives HPC) ----------------------------------------------------
    let hpc_work = cpc * (tt3 - tt25);
    let d_tt_hpt = hpc_work / ((1.0 + f_ratio) * cph * cfg.turbine_mechanical_efficiency);
    let tt45 = tt4 - d_tt_hpt;
    if tt45 <= 0.0 {
        return infeasible(
            "HPT work required exceeds available combustor exit enthalpy.".to_owned(),
        );
    }
    let pt45 = pt4 * (tt45 / tt4).powf(gh / ((gh - 1.0) * cfg.hpt_polytropic_efficiency));

    // -- LPT (drives LPC + fan) ----------------------------------------------
    let lpc_work = cpc * (tt25 - tt2);
    let fan_work = cpc * (tt13 - tt2);
    let d_tt_lpt = (lpc_work + inputs.bypass_ratio * fan_work)
        / ((1.0 + f_ratio) * cph * cfg.turbine_mechanical_efficiency);
    let tt5 = tt45 - d_tt_lpt;
    if tt5 <= 0.0 {
        return infeasible(
            "LPT work required (LPC + fan) exceeds available HPT exit enthalpy.".to_owned(),
        );
    }
    let pt5 = pt45 * (tt5 / tt45).powf(gh / ((gh - 1.0) * cfg.lpt_polytropic_efficiency));

    // -- Core nozzle ---------------------------------------------------------
    let pt6 = pt5 * cfg.core_nozzle_pressure_ratio;
    let tt6 = tt5;
    let (v9, t9, p9) = expand_nozzle(tt6, pt6, p0, gh, cph, cfg.core_nozzle_efficiency);

    // -- Fan nozzle ----------------------------------------------------------
    let (v19, t19, p19) = expand_nozzle(tt13, pt13, p0, gc, cpc, cfg.fan_nozzle_efficiency);

    // -- Thrust (per unit CORE mass flow), including pressure-thrust if choked.
    // Folded into an "equivalent" exit velocity (V9_eq = V9 + (p9-p0)/(rho9*V9))
    // rather than kept as a separate additive pressure term: algebraically
    // identical for the thrust equation, but it also lets the efficiency
    // energy-balance below use the SAME equivalent velocity for its
    // kinetic-energy term -- required for a choked/underexpanded nozzle, where
    // the exiting flow still carries recoverable pressure energy a longer
    // nozzle would have converted to KE. The bare V9/V19 in the KE-gain
    // denominator let momentum+pressure thrust exceed KE-only accounting
    // whenever a nozzle choked, producing propulsive efficiency > 1. This is
    // the standard Mattingly "equivalent velocity" treatment.
    let r_hot = cph * (gh - 1.0) / gh;
    let r_cold = cpc * (gc - 1.0) / gc;
    let rho9 = p9 / (r_hot * t9).max(1e-9);
    let rho19 = p19 / (r_cold * t19).max(1e-9);

    let v9_eq = v9 + (p9 - p0) / (rho9 * v9).max(1e-9);
    let v19_eq = v19 + (p19 - p0) / (rho19 * v19).max(1e-9);

    let f_core = (1.0 + f_ratio) * (v9_eq - v0);
    let f_bypass = inputs.bypass_ratio * (v19_eq - v0);
    let f_per_mdot_core = f_core + f_bypass;

    if f_per_mdot_core <= 0.0 {
        return infeasible(
            "Computed net thrust is non-positive at this flight condition.".to_owned(),
        );
    }

    let bpr = inputs.bypass_ratio;
    let sfn = f_per_mdot_core / (1.0 + bpr);
    let tsfc_si = f_ratio / f_per_mdot_core;
    let tsfc_mg_ns = tsfc_si * 1.0e6;

    let ke_gain = (1.0 + f_ratio) * (v9_eq.powi(2) - v0.powi(2)) / 2.0
        + bpr * (v19_eq.powi(2) - v0.powi(2)) / 2.0;
    let fuel_power = f_ratio * h_pr;
    let eta_th = if fuel_power > 0.0 {
        ke_gain / fuel_power
    } else {
        f64::NAN
    };
    let (eta_p, eta_o) = if v0 > 1e-6 && ke_gain > 0.0 {
        let eta_p = (f_per_mdot_core * v0) / ke_gain;
        let eta_o = if fuel_power > 0.0 {
            (f_per_mdot_core * v0) / fuel_power
        } else {
            f64::NAN
        };
        (eta_p, eta_o)
    } else {
        (0.0, 0.0)
    };

    let mut result = TurbofanCycleResult::feasible_blank();
    result.specific_thrust_ms = sfn;
    result.tsfc_mg_ns = tsfc_mg_ns;
    result.fuel_air_ratio = f_ratio;
    result.thermal_efficiency = eta_th;
    result.propulsive_efficiency = eta_p;
    result.overall_efficiency = eta_o;
    result.temperature_t0_k = tt0;
    result.temperature_t13_k = tt13;
    result.temperature_t25_k = tt25;
    result.temperature_t3_k = tt3;
    result.temperature_t4_k = tt4;
    result.temperature_t45_k = tt45;
    result.temperature_t5_k = tt5;
    result.temperature_t6_k = tt6;
    result.exit_velocity_core_ms = v9;
    result.exit_velocity_fan_ms = v19;
    result
}

/// Total design mass flow (kg/s) such that the on-design cycle's static
/// (sea-level, `M0 = 0`) specific thrust reproduces `thrust_kn` exactly, paired
/// with that static cycle result.
///
/// A conceptual-design normalisation, not an independent validation: it lets a
/// caller report a dimensional thrust at any flight condition
/// (`specific_thrust_ms(condition) * mdot_total`) consistent with the engine's
/// rated static thrust. Returns `NaN` for the mass flow when the static cycle
/// is infeasible or produces non-positive specific thrust.
pub fn anchor_mass_flow_kg_s(
    thrust_kn: f64,
    overall_pressure_ratio: f64,
    fan_pressure_ratio: f64,
    bypass_ratio: f64,
    turbine_inlet_temperature_k: f64,
    cfg: &PropulsionCycleConfig,
) -> (f64, TurbofanCycleResult) {
    let static_inputs = TurbofanCycleInputs {
        mach: 0.0,
        altitude_m: 0.0,
        bypass_ratio,
        overall_pressure_ratio,
        fan_pressure_ratio,
        turbine_inlet_temperature_k,
    };
    let static_result = compute_turbofan_cycle(&static_inputs, cfg);
    if !static_result.cycle_feasible || static_result.specific_thrust_ms <= 0.0 {
        return (f64::NAN, static_result);
    }
    let mdot_total = (thrust_kn * 1000.0) / static_result.specific_thrust_ms;
    (mdot_total, static_result)
}

/// Descriptive label from bypass ratio (informational only -- it feeds no
/// lookup table, unlike the classic turbojet/LBR/HBR engine-deck buckets).
pub fn classify_engine_by_bpr(bypass_ratio: f64) -> &'static str {
    if bypass_ratio < 1.0 {
        return "Turbojet / very-low-bypass";
    }
    if bypass_ratio < 5.0 {
        return "Low-bypass turbofan";
    }
    "High-bypass turbofan"
}

// A test asserts on values it constructs here directly, so a failed unwrap or
// expect is the assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    fn nominal() -> TurbofanCycleInputs {
        TurbofanCycleInputs {
            mach: 0.85,
            altitude_m: 10668.0,
            bypass_ratio: 10.0,
            overall_pressure_ratio: 50.0,
            fan_pressure_ratio: 1.5,
            turbine_inlet_temperature_k: 1600.0,
        }
    }

    #[test]
    fn a_reasonable_cruise_point_is_feasible_and_burns_fuel() {
        let result = compute_turbofan_cycle(&nominal(), &PropulsionCycleConfig::default());
        assert!(result.cycle_feasible);
        assert!(result.infeasibility_reason.is_empty());
        assert!(result.specific_thrust_ms > 0.0);
        assert!(result.fuel_air_ratio > 0.0);
        // Every efficiency in the decomposition is a fraction of one.
        for eta in [
            result.thermal_efficiency,
            result.propulsive_efficiency,
            result.overall_efficiency,
        ] {
            assert!(
                (0.0..=1.0).contains(&eta),
                "efficiency {eta} outside [0, 1]"
            );
        }
    }

    #[test]
    fn station_temperatures_rise_through_compression_and_fall_through_turbines() {
        let result = compute_turbofan_cycle(&nominal(), &PropulsionCycleConfig::default());
        // Compression heats the core; the turbines cool it back down.
        assert!(result.temperature_t0_k < result.temperature_t25_k);
        assert!(result.temperature_t25_k < result.temperature_t3_k);
        assert!(result.temperature_t3_k < result.temperature_t4_k);
        assert!(result.temperature_t45_k < result.temperature_t4_k);
        assert!(result.temperature_t5_k < result.temperature_t45_k);
    }

    #[test]
    fn a_turbine_inlet_below_the_compressor_discharge_is_infeasible() {
        // The combustor cannot cool the flow; asking it to is unphysical.
        let mut inputs = nominal();
        inputs.turbine_inlet_temperature_k = 400.0;
        let result = compute_turbofan_cycle(&inputs, &PropulsionCycleConfig::default());
        assert!(!result.cycle_feasible);
        assert!(result.infeasibility_reason.contains("must exceed"));
        assert!(result.specific_thrust_ms.is_nan());
    }

    #[test]
    fn static_thrust_has_zero_propulsive_efficiency() {
        // With no freestream velocity there is no thrust power, so the
        // propulsive and overall efficiencies collapse to the v0 <= 1e-6 branch.
        let (mdot, result) = anchor_mass_flow_kg_s(
            470.0,
            50.0,
            1.5,
            10.0,
            1700.0,
            &PropulsionCycleConfig::default(),
        );
        assert!(result.cycle_feasible);
        assert_eq!(result.propulsive_efficiency, 0.0);
        assert_eq!(result.overall_efficiency, 0.0);
        // The anchor inverts the static specific thrust, so it is positive.
        assert!(mdot > 0.0);
    }

    #[test]
    fn the_bypass_labels_switch_at_one_and_five() {
        assert_eq!(classify_engine_by_bpr(0.999), "Turbojet / very-low-bypass");
        assert_eq!(classify_engine_by_bpr(1.0), "Low-bypass turbofan");
        assert_eq!(classify_engine_by_bpr(4.999), "Low-bypass turbofan");
        assert_eq!(classify_engine_by_bpr(5.0), "High-bypass turbofan");
    }
}
