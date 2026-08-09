// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/physics/propulsion.py
// Reference: alas @ rust-port-baseline.

//! Parametric sweeps over the on-design cycle.
//!
//! Each function re-evaluates [`compute_turbofan_cycle`] over a grid of one or
//! two design parameters, holding the rest fixed, and returns the results laid
//! out for plotting: an infeasible point leaves its cell `NaN` and its
//! `feasible_mask` entry false, so a caller reads the trade surface and the
//! region it exists over from the same result. This is orchestration over the
//! single-point kernel and adds no new physics -- it is the closed-form
//! stand-in for a semi-empirical installed-thrust-lapse table.

use alas_config::PropulsionCycleConfig;

use super::{compute_turbofan_cycle, TurbofanCycleInputs};

/// Specific-thrust / TSFC trade surface over (overall pressure ratio, turbine
/// inlet temperature), for a carpet-plot-style visualisation.
///
/// The two matrix fields are indexed `[tit][pressure ratio]`, matching the
/// `tit_vector_k` (rows) x `compressor_pressure_ratio_vector` (columns) grid.
#[derive(Debug, Clone, PartialEq)]
pub struct CarpetPlotResult {
    /// The swept overall (core) pressure ratios (columns).
    pub compressor_pressure_ratio_vector: Vec<f64>,
    /// The swept turbine inlet temperatures, K (rows).
    pub tit_vector_k: Vec<f64>,
    /// Specific thrust, m/s, per grid cell; `NaN` where infeasible.
    pub specific_thrust_ms: Vec<Vec<f64>>,
    /// TSFC, mg/(N.s), per grid cell; `NaN` where infeasible.
    pub tsfc_mg_ns: Vec<Vec<f64>>,
    /// Whether the cycle was feasible at each grid cell.
    pub feasible_mask: Vec<Vec<bool>>,
}

/// Sweep (overall pressure ratio, turbine inlet temperature) at a fixed flight
/// condition / bypass ratio / fan pressure ratio.
pub fn compute_carpet_plot(
    compressor_pressure_ratio_vector: &[f64],
    tit_vector_k: &[f64],
    mach: f64,
    altitude_m: f64,
    fan_pressure_ratio: f64,
    bypass_ratio: f64,
    cfg: &PropulsionCycleConfig,
) -> CarpetPlotResult {
    let mut specific_thrust_ms = Vec::with_capacity(tit_vector_k.len());
    let mut tsfc_mg_ns = Vec::with_capacity(tit_vector_k.len());
    let mut feasible_mask = Vec::with_capacity(tit_vector_k.len());

    for &tit in tit_vector_k {
        let mut sfn_row = Vec::with_capacity(compressor_pressure_ratio_vector.len());
        let mut tsfc_row = Vec::with_capacity(compressor_pressure_ratio_vector.len());
        let mut mask_row = Vec::with_capacity(compressor_pressure_ratio_vector.len());
        for &pic in compressor_pressure_ratio_vector {
            let out = compute_turbofan_cycle(
                &TurbofanCycleInputs {
                    mach,
                    altitude_m,
                    bypass_ratio,
                    overall_pressure_ratio: pic,
                    fan_pressure_ratio,
                    turbine_inlet_temperature_k: tit,
                },
                cfg,
            );
            sfn_row.push(if out.cycle_feasible {
                out.specific_thrust_ms
            } else {
                f64::NAN
            });
            tsfc_row.push(if out.cycle_feasible {
                out.tsfc_mg_ns
            } else {
                f64::NAN
            });
            mask_row.push(out.cycle_feasible);
        }
        specific_thrust_ms.push(sfn_row);
        tsfc_mg_ns.push(tsfc_row);
        feasible_mask.push(mask_row);
    }

    CarpetPlotResult {
        compressor_pressure_ratio_vector: compressor_pressure_ratio_vector.to_vec(),
        tit_vector_k: tit_vector_k.to_vec(),
        specific_thrust_ms,
        tsfc_mg_ns,
        feasible_mask,
    }
}

/// Specific thrust and TSFC against bypass ratio, everything else fixed.
#[derive(Debug, Clone, PartialEq)]
pub struct BprSensitivityResult {
    /// The swept bypass ratios.
    pub bypass_ratio_vector: Vec<f64>,
    /// Specific thrust, m/s, per bypass ratio; `NaN` where infeasible.
    pub specific_thrust_ms: Vec<f64>,
    /// TSFC, mg/(N.s), per bypass ratio; `NaN` where infeasible.
    pub tsfc_mg_ns: Vec<f64>,
    /// Whether the cycle was feasible at each bypass ratio.
    pub feasible_mask: Vec<bool>,
}

/// Sweep bypass ratio at fixed OPR / T4t / FPR / flight condition.
pub fn compute_bpr_sensitivity(
    bpr_vector: &[f64],
    overall_pressure_ratio: f64,
    turbine_inlet_temperature_k: f64,
    fan_pressure_ratio: f64,
    mach: f64,
    altitude_m: f64,
    cfg: &PropulsionCycleConfig,
) -> BprSensitivityResult {
    let mut specific_thrust_ms = Vec::with_capacity(bpr_vector.len());
    let mut tsfc_mg_ns = Vec::with_capacity(bpr_vector.len());
    let mut feasible_mask = Vec::with_capacity(bpr_vector.len());

    for &bpr in bpr_vector {
        let out = compute_turbofan_cycle(
            &TurbofanCycleInputs {
                mach,
                altitude_m,
                bypass_ratio: bpr,
                overall_pressure_ratio,
                fan_pressure_ratio,
                turbine_inlet_temperature_k,
            },
            cfg,
        );
        specific_thrust_ms.push(if out.cycle_feasible {
            out.specific_thrust_ms
        } else {
            f64::NAN
        });
        tsfc_mg_ns.push(if out.cycle_feasible {
            out.tsfc_mg_ns
        } else {
            f64::NAN
        });
        feasible_mask.push(out.cycle_feasible);
    }

    BprSensitivityResult {
        bypass_ratio_vector: bpr_vector.to_vec(),
        specific_thrust_ms,
        tsfc_mg_ns,
        feasible_mask,
    }
}

/// Overall efficiency decomposed into its thermal and propulsive factors,
/// against overall pressure ratio.
#[derive(Debug, Clone, PartialEq)]
pub struct EfficiencyDecompositionResult {
    /// The swept overall (core) pressure ratios.
    pub compressor_pressure_ratio_vector: Vec<f64>,
    /// Thermal efficiency per pressure ratio; `NaN` where infeasible.
    pub thermal_efficiency: Vec<f64>,
    /// Propulsive efficiency per pressure ratio; `NaN` where infeasible.
    pub propulsive_efficiency: Vec<f64>,
    /// Overall efficiency per pressure ratio; `NaN` where infeasible.
    pub overall_efficiency: Vec<f64>,
    /// Whether the cycle was feasible at each pressure ratio.
    pub feasible_mask: Vec<bool>,
}

/// Sweep overall pressure ratio at fixed T4t / BPR / FPR / flight condition,
/// decomposing overall efficiency into thermal x propulsive.
pub fn compute_efficiency_decomposition(
    pi_c_vector: &[f64],
    turbine_inlet_temperature_k: f64,
    bypass_ratio: f64,
    fan_pressure_ratio: f64,
    mach: f64,
    altitude_m: f64,
    cfg: &PropulsionCycleConfig,
) -> EfficiencyDecompositionResult {
    let mut thermal_efficiency = Vec::with_capacity(pi_c_vector.len());
    let mut propulsive_efficiency = Vec::with_capacity(pi_c_vector.len());
    let mut overall_efficiency = Vec::with_capacity(pi_c_vector.len());
    let mut feasible_mask = Vec::with_capacity(pi_c_vector.len());

    for &pic in pi_c_vector {
        let out = compute_turbofan_cycle(
            &TurbofanCycleInputs {
                mach,
                altitude_m,
                bypass_ratio,
                overall_pressure_ratio: pic,
                fan_pressure_ratio,
                turbine_inlet_temperature_k,
            },
            cfg,
        );
        thermal_efficiency.push(if out.cycle_feasible {
            out.thermal_efficiency
        } else {
            f64::NAN
        });
        propulsive_efficiency.push(if out.cycle_feasible {
            out.propulsive_efficiency
        } else {
            f64::NAN
        });
        overall_efficiency.push(if out.cycle_feasible {
            out.overall_efficiency
        } else {
            f64::NAN
        });
        feasible_mask.push(out.cycle_feasible);
    }

    EfficiencyDecompositionResult {
        compressor_pressure_ratio_vector: pi_c_vector.to_vec(),
        thermal_efficiency,
        propulsive_efficiency,
        overall_efficiency,
        feasible_mask,
    }
}

/// Specific thrust and TSFC against altitude, for each of several Mach numbers,
/// with an optional dimensional thrust from the mass-flow anchor.
///
/// The three matrix fields are indexed `[mach][altitude]`.
#[derive(Debug, Clone, PartialEq)]
pub struct AltitudeSweepResult {
    /// The swept altitudes, m (columns).
    pub altitude_m: Vec<f64>,
    /// Specific thrust, m/s, per grid cell; `NaN` where infeasible.
    pub specific_thrust_ms: Vec<Vec<f64>>,
    /// TSFC, mg/(N.s), per grid cell; `NaN` where infeasible.
    pub tsfc_mg_ns: Vec<Vec<f64>>,
    /// Dimensional thrust, kN, per grid cell from the mass-flow anchor; `NaN`
    /// where infeasible or where no valid `mdot_total_kg_s` was supplied.
    pub dimensional_thrust_kn: Vec<Vec<f64>>,
    /// Whether the cycle was feasible at each grid cell.
    pub feasible_mask: Vec<Vec<bool>>,
    /// The Mach numbers swept (rows).
    pub mach_values: Vec<f64>,
}

/// Specific thrust and TSFC vs altitude, for each of several Mach numbers,
/// holding the cycle design parameters fixed. Pass `NaN` for `mdot_total_kg_s`
/// to leave the dimensional thrust unpopulated.
#[allow(clippy::too_many_arguments)] // mirrors upstream's own signature
pub fn compute_altitude_sweep(
    altitude_vector_m: &[f64],
    mach_values: &[f64],
    bypass_ratio: f64,
    overall_pressure_ratio: f64,
    fan_pressure_ratio: f64,
    turbine_inlet_temperature_k: f64,
    mdot_total_kg_s: f64,
    cfg: &PropulsionCycleConfig,
) -> AltitudeSweepResult {
    let mut specific_thrust_ms = Vec::with_capacity(mach_values.len());
    let mut tsfc_mg_ns = Vec::with_capacity(mach_values.len());
    let mut dimensional_thrust_kn = Vec::with_capacity(mach_values.len());
    let mut feasible_mask = Vec::with_capacity(mach_values.len());

    let mdot_valid = !mdot_total_kg_s.is_nan() && mdot_total_kg_s > 0.0;

    for &mach in mach_values {
        let mut sfn_row = Vec::with_capacity(altitude_vector_m.len());
        let mut tsfc_row = Vec::with_capacity(altitude_vector_m.len());
        let mut thrust_row = Vec::with_capacity(altitude_vector_m.len());
        let mut mask_row = Vec::with_capacity(altitude_vector_m.len());
        for &alt in altitude_vector_m {
            let out = compute_turbofan_cycle(
                &TurbofanCycleInputs {
                    mach,
                    altitude_m: alt,
                    bypass_ratio,
                    overall_pressure_ratio,
                    fan_pressure_ratio,
                    turbine_inlet_temperature_k,
                },
                cfg,
            );
            if out.cycle_feasible {
                sfn_row.push(out.specific_thrust_ms);
                tsfc_row.push(out.tsfc_mg_ns);
                thrust_row.push(if mdot_valid {
                    out.specific_thrust_ms * mdot_total_kg_s / 1000.0
                } else {
                    f64::NAN
                });
            } else {
                sfn_row.push(f64::NAN);
                tsfc_row.push(f64::NAN);
                thrust_row.push(f64::NAN);
            }
            mask_row.push(out.cycle_feasible);
        }
        specific_thrust_ms.push(sfn_row);
        tsfc_mg_ns.push(tsfc_row);
        dimensional_thrust_kn.push(thrust_row);
        feasible_mask.push(mask_row);
    }

    AltitudeSweepResult {
        altitude_m: altitude_vector_m.to_vec(),
        specific_thrust_ms,
        tsfc_mg_ns,
        dimensional_thrust_kn,
        feasible_mask,
        mach_values: mach_values.to_vec(),
    }
}
