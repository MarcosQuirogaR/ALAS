// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Station states of the on-design turbofan cycle for a temperature-entropy
//! preview (clarified Advanced Settings rework, Propulsion, decision D12).
//!
//! The states come from the same walk as [`super::compute_turbofan_cycle`];
//! nothing is fabricated beyond what that model computes:
//!
//! * **Stagnation** ("total") states at the numbered stations, in the order
//!   the flow meets them. Core path: 0 (freestream, ram), 2 (fan/LPC face),
//!   25 (LPC exit), 3 (HPC exit, combustor inlet), 4 (combustor exit,
//!   turbine inlet), 45 (HPT exit), 5 (LPT exit), 6 (core nozzle inlet).
//!   Bypass path: 2, 13 (fan exit).
//! * **Static** nozzle-exit states 9 (core) and 19 (bypass) after the
//!   expansion to ambient pressure or the choke pressure.
//! * The freestream **static** state, which is the entropy reference.
//!
//! Entropy is specific entropy relative to the freestream static state,
//! `s - s_0 = cp ln(T / T_0) - R ln(p / p_0)`, J/(kg K), with the configured
//! cold-gas `cp` and `gamma` up to the combustor and along the bypass, and
//! the hot-gas values from the combustor exit through the core nozzle
//! (`R = cp (gamma - 1) / gamma`). Each station uses its own gas constants,
//! which is the model's constant-property assumption; the combustor segment
//! therefore joins a cold-gas point to a hot-gas point. Temperatures in
//! kelvin, pressures in pascal, SI throughout.

use alas_config::PropulsionCycleConfig;

use super::{walk_cycle, CycleWalk, TurbofanCycleInputs};

/// Which flow path a station belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleBranch {
    /// Shared inlet up to the fan/LPC face.
    Inlet,
    /// Core compressors, combustor, turbines and core nozzle.
    Core,
    /// Fan and bypass nozzle.
    Bypass,
}

/// Whether a state is a stagnation (total) or a static state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateKind {
    /// Total temperature and pressure.
    Stagnation,
    /// Static temperature and pressure (nozzle exit, freestream).
    Static,
}

/// The gas constants a station's entropy is formed with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleGas {
    /// Configured cold-gas `cp`/`gamma` (air before combustion, bypass).
    Cold,
    /// Configured hot-gas `cp`/`gamma` (combustion products).
    Hot,
}

/// One thermodynamic state on the cycle path.
#[derive(Debug, Clone, PartialEq)]
pub struct CycleStation {
    /// Standard two-spool station number (`"0"`, `"2"`, `"13"`, ..., `"19"`).
    pub station: &'static str,
    /// Short name of the station.
    pub name: &'static str,
    /// Flow path.
    pub branch: CycleBranch,
    /// Stagnation or static.
    pub kind: StateKind,
    /// Gas constants used for the entropy.
    pub gas: CycleGas,
    /// Temperature, K.
    pub temperature_k: f64,
    /// Pressure, Pa.
    pub pressure_pa: f64,
    /// Specific entropy relative to the freestream static state, J/(kg K).
    pub entropy_j_kgk: f64,
}

/// The entropy reference state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EntropyReference {
    /// Freestream static temperature, K.
    pub temperature_k: f64,
    /// Freestream static pressure, Pa.
    pub pressure_pa: f64,
}

/// Station states of one on-design cycle evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct TurbofanCycleStates {
    /// False when the cycle has no physical solution; the vectors are then empty.
    pub cycle_feasible: bool,
    /// Why, when infeasible.
    pub infeasibility_reason: String,
    /// The freestream static state the entropies are relative to.
    pub reference: EntropyReference,
    /// Freestream static state (entropy zero by definition).
    pub freestream_static: Option<CycleStation>,
    /// Core path from station 0 (stagnation) to the static core exit 9.
    pub core: Vec<CycleStation>,
    /// Bypass path from station 2 (stagnation) to the static fan exit 19.
    pub bypass: Vec<CycleStation>,
}

struct Gas {
    cp: f64,
    r: f64,
}

impl Gas {
    fn of(cfg: &PropulsionCycleConfig, gas: CycleGas) -> Self {
        let (cp, gamma) = match gas {
            CycleGas::Cold => (cfg.cp_cold_j_kgk, cfg.gamma_cold),
            CycleGas::Hot => (cfg.cp_hot_j_kgk, cfg.gamma_hot),
        };
        Self {
            cp,
            r: cp * (gamma - 1.0) / gamma,
        }
    }
}

/// `s - s_0` for one state relative to `reference`, J/(kg K).
pub fn relative_entropy_j_kgk(
    cfg: &PropulsionCycleConfig,
    gas: CycleGas,
    reference: EntropyReference,
    temperature_k: f64,
    pressure_pa: f64,
) -> f64 {
    let gas = Gas::of(cfg, gas);
    gas.cp * (temperature_k / reference.temperature_k).ln()
        - gas.r * (pressure_pa / reference.pressure_pa).ln()
}

/// The station states of the cycle at `inputs`, or an infeasible record
/// carrying the same reason [`super::compute_turbofan_cycle`] reports.
pub fn compute_turbofan_cycle_states(
    inputs: &TurbofanCycleInputs,
    cfg: &PropulsionCycleConfig,
) -> TurbofanCycleStates {
    let walk = match walk_cycle(inputs, cfg) {
        Ok(walk) => walk,
        Err(reason) => {
            let atmosphere = alas_atmo::Atmosphere::new(inputs.altitude_m);
            return TurbofanCycleStates {
                cycle_feasible: false,
                infeasibility_reason: reason,
                reference: EntropyReference {
                    temperature_k: atmosphere.temperature(),
                    pressure_pa: atmosphere.pressure(),
                },
                freestream_static: None,
                core: Vec::new(),
                bypass: Vec::new(),
            };
        }
    };
    let reference = EntropyReference {
        temperature_k: walk.t0,
        pressure_pa: walk.p0,
    };
    let station = |station: &'static str,
                   name: &'static str,
                   branch: CycleBranch,
                   kind: StateKind,
                   gas: CycleGas,
                   temperature_k: f64,
                   pressure_pa: f64| CycleStation {
        station,
        name,
        branch,
        kind,
        gas,
        temperature_k,
        pressure_pa,
        entropy_j_kgk: relative_entropy_j_kgk(cfg, gas, reference, temperature_k, pressure_pa),
    };
    use CycleBranch::{Bypass, Core, Inlet};
    use CycleGas::{Cold, Hot};
    use StateKind::{Stagnation, Static};
    let CycleWalk {
        t0,
        p0,
        tt0,
        pt0,
        tt2,
        pt2,
        tt13,
        pt13,
        tt25,
        pt25,
        tt3,
        pt3,
        tt4,
        pt4,
        tt45,
        pt45,
        tt5,
        pt5,
        tt6,
        pt6,
        t9,
        p9,
        t19,
        p19,
        ..
    } = walk;
    TurbofanCycleStates {
        cycle_feasible: true,
        infeasibility_reason: String::new(),
        reference,
        freestream_static: Some(station(
            "0s",
            "Freestream (static)",
            Inlet,
            Static,
            Cold,
            t0,
            p0,
        )),
        core: vec![
            station("0", "Freestream (ram)", Inlet, Stagnation, Cold, tt0, pt0),
            station("2", "Fan/LPC face", Inlet, Stagnation, Cold, tt2, pt2),
            station("25", "LPC exit", Core, Stagnation, Cold, tt25, pt25),
            station("3", "HPC exit", Core, Stagnation, Cold, tt3, pt3),
            station("4", "Combustor exit", Core, Stagnation, Hot, tt4, pt4),
            station("45", "HPT exit", Core, Stagnation, Hot, tt45, pt45),
            station("5", "LPT exit", Core, Stagnation, Hot, tt5, pt5),
            station("6", "Core nozzle inlet", Core, Stagnation, Hot, tt6, pt6),
            station("9", "Core nozzle exit (static)", Core, Static, Hot, t9, p9),
        ],
        bypass: vec![
            station("2", "Fan/LPC face", Inlet, Stagnation, Cold, tt2, pt2),
            station("13", "Fan exit", Bypass, Stagnation, Cold, tt13, pt13),
            station(
                "19",
                "Fan nozzle exit (static)",
                Bypass,
                Static,
                Cold,
                t19,
                p19,
            ),
        ],
    }
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cycle::compute_turbofan_cycle;

    fn cruise() -> TurbofanCycleInputs {
        TurbofanCycleInputs {
            mach: 0.85,
            altitude_m: 10668.0,
            bypass_ratio: 10.0,
            overall_pressure_ratio: 50.0,
            fan_pressure_ratio: 1.5,
            turbine_inlet_temperature_k: 1600.0,
        }
    }

    fn entropy(states: &[CycleStation], station: &str) -> f64 {
        states
            .iter()
            .find(|state| state.station == station)
            .unwrap()
            .entropy_j_kgk
    }

    #[test]
    fn the_station_path_matches_the_cycle_result_and_entropy_never_falls() {
        let cfg = PropulsionCycleConfig::default();
        let result = compute_turbofan_cycle(&cruise(), &cfg);
        let states = compute_turbofan_cycle_states(&cruise(), &cfg);
        assert!(states.cycle_feasible && result.cycle_feasible);
        assert_eq!(states.core.len(), 9);
        assert_eq!(states.bypass.len(), 3);
        let by = |station: &str| {
            states
                .core
                .iter()
                .chain(&states.bypass)
                .find(|state| state.station == station)
                .unwrap()
                .temperature_k
        };
        for (station, expected) in [
            ("0", result.temperature_t0_k),
            ("13", result.temperature_t13_k),
            ("25", result.temperature_t25_k),
            ("3", result.temperature_t3_k),
            ("4", result.temperature_t4_k),
            ("45", result.temperature_t45_k),
            ("5", result.temperature_t5_k),
            ("6", result.temperature_t6_k),
        ] {
            assert_eq!(by(station), expected, "station {station}");
        }
        let freestream = states.freestream_static.as_ref().unwrap();
        assert_eq!(freestream.entropy_j_kgk, 0.0);
        assert_eq!(freestream.temperature_k, states.reference.temperature_k);
        // Every stagnation state and every static exit state has a
        // positive temperature and pressure, and the path is
        // thermodynamically ordered: no component lowers the entropy.
        for state in states.core.iter().chain(&states.bypass) {
            assert!(state.temperature_k > 0.0 && state.pressure_pa > 0.0);
            assert!(state.entropy_j_kgk.is_finite());
        }
        let core: Vec<f64> = states.core.iter().map(|s| s.entropy_j_kgk).collect();
        for pair in core.windows(2) {
            assert!(pair[1] >= pair[0] - 1e-9, "core entropy falls: {core:?}");
        }
        let bypass: Vec<f64> = states.bypass.iter().map(|s| s.entropy_j_kgk).collect();
        for pair in bypass.windows(2) {
            assert!(
                pair[1] >= pair[0] - 1e-9,
                "bypass entropy falls: {bypass:?}"
            );
        }
        assert!(entropy(&states.core, "4") > entropy(&states.core, "3") + 100.0);
        assert_eq!(states.core[4].gas, CycleGas::Hot);
        assert_eq!(states.core[3].gas, CycleGas::Cold);
        assert_eq!(states.core[8].kind, StateKind::Static);
        assert_eq!(states.bypass[2].kind, StateKind::Static);
    }

    #[test]
    fn an_isentropic_change_has_zero_relative_entropy() {
        let cfg = PropulsionCycleConfig::default();
        let reference = EntropyReference {
            temperature_k: 220.0,
            pressure_pa: 24_000.0,
        };
        assert_eq!(
            relative_entropy_j_kgk(&cfg, CycleGas::Cold, reference, 220.0, 24_000.0),
            0.0
        );
        let pressure_ratio = 12.0_f64;
        let exponent = (cfg.gamma_cold - 1.0) / cfg.gamma_cold;
        let isentropic_t = 220.0 * pressure_ratio.powf(exponent);
        let ds = relative_entropy_j_kgk(
            &cfg,
            CycleGas::Cold,
            reference,
            isentropic_t,
            24_000.0 * pressure_ratio,
        );
        assert!(ds.abs() < 1e-9, "{ds}");
        let heated = relative_entropy_j_kgk(&cfg, CycleGas::Cold, reference, 440.0, 24_000.0);
        assert!((heated - cfg.cp_cold_j_kgk * 2.0_f64.ln()).abs() < 1e-9);
    }

    #[test]
    fn an_infeasible_cycle_reports_the_same_reason_and_no_states() {
        let cfg = PropulsionCycleConfig::default();
        let mut inputs = cruise();
        inputs.turbine_inlet_temperature_k = 400.0;
        let result = compute_turbofan_cycle(&inputs, &cfg);
        let states = compute_turbofan_cycle_states(&inputs, &cfg);
        assert!(!result.cycle_feasible && !states.cycle_feasible);
        assert_eq!(states.infeasibility_reason, result.infeasibility_reason);
        assert!(states.core.is_empty() && states.bypass.is_empty());
        assert!(states.freestream_static.is_none());
        assert!(states.reference.temperature_k > 0.0);
    }
}
