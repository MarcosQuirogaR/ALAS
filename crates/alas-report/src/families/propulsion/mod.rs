// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py
// Reference: alas @ rust-port-baseline.

//! Propulsion thermodynamic cycle summary, the engine designer's nacelle
//! preview, and parametric trade-space sweeps (carpet plot, efficiency
//! decomposition, bypass-ratio sensitivity, altitude/Mach envelope).
//!
//! Every plotted quantity comes from [`alas_prop::cycle::compute_turbofan_cycle`]
//! or one of its [`alas_prop::cycle::sweeps`] wrappers, evaluated at the
//! current [`AlasConfig`]'s engine and flight-condition fields -- matching
//! upstream's five `figure_propulsion_*` functions plus
//! `figure_engine_designer_preview` in `alas/reporting/visualization.py`.
//! None of those six Python functions reads its `report` parameter (checked
//! by inspection of each body), so this port drops it from every signature
//! rather than carrying an argument nothing consumes.

mod altitude;
mod cycle;
mod support;
mod sweeps;
mod technology;

pub use altitude::figure_propulsion_altitude_sweep;
pub use cycle::{
    figure_engine_designer_preview, figure_propulsion_cycle_summary, propulsion_cycle_summary,
};
pub use sweeps::{
    figure_propulsion_bpr_sensitivity, figure_propulsion_carpet_plot,
    figure_propulsion_efficiency_decomposition,
};

use alas_config::{ActiveEngineModel, AlasConfig, TurbofanEngineSpec};
use alas_prop::cycle::TurbofanCycleInputs;

/// The cruise design point every propulsion figure evaluates the cycle at.
///
/// Ported from `_propulsion_design_point` (`visualization.py`): the current
/// engine's BPR/OPR/FPR/TIT at the design requirements' cruise Mach and
/// altitude.
fn design_point(config: &AlasConfig) -> TurbofanCycleInputs {
    let eng = turbofan_spec(config);
    let req = &config.requirements;
    TurbofanCycleInputs {
        mach: req.cruise_mach,
        altitude_m: req.cruise_altitude_m,
        bypass_ratio: eng.bypass_ratio,
        overall_pressure_ratio: eng.overall_pressure_ratio,
        fan_pressure_ratio: eng.fan_pressure_ratio,
        turbine_inlet_temperature_k: eng.turbine_inlet_temp_k,
    }
}

/// Resolve the single authoritative turbofan payload used by every jet figure.
/// Invalid technology/payload combinations are programming/configuration
/// errors and must never fall back to the deprecated flat compatibility copy.
fn turbofan_spec(config: &AlasConfig) -> &TurbofanEngineSpec {
    match config.geometry.engine.active_model() {
        Ok(ActiveEngineModel::Turbofan(spec)) => spec,
        Ok(ActiveEngineModel::Turboprop(_)) => {
            unreachable!("technology dispatch routes turboprops to dedicated figures")
        }
        Err(error) => panic!("invalid propulsion binding: {error}"),
    }
}

fn is_turboprop(config: &AlasConfig) -> bool {
    matches!(
        config.geometry.engine.propulsion_technology,
        alas_config::PropulsionTechnology::Turboprop
    )
}

/// `numpy.linspace(start, stop, num)` with the inclusive endpoint NumPy uses.
///
/// The interior points are `start + step * i`; the last is set to `stop`
/// exactly, matching `alas-perf::performance::linspace`'s own note on why
/// (keeps the axis endpoints bit-identical rather than a rounding of
/// `start + step * (num - 1)`). Every `np.linspace` call this family ports
/// (the carpet plot's two axes, the efficiency-decomposition sweep, the BPR
/// sweep, the altitude/Mach grid) goes through this one copy.
fn linspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    if num == 0 {
        return Vec::new();
    }
    if num == 1 {
        return vec![start];
    }
    let step = (stop - start) / (num - 1) as f64;
    let mut values: Vec<f64> = (0..num).map(|i| start + step * i as f64).collect();
    values[num - 1] = stop;
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linspace_places_both_endpoints_exactly() {
        let grid = linspace(15.0, 70.0, 12);
        assert_eq!(grid.len(), 12);
        assert_eq!(grid[0], 15.0);
        assert_eq!(grid[11], 70.0);
    }

    #[test]
    fn linspace_of_one_point_is_the_start() {
        assert_eq!(linspace(3.0, 9.0, 1), vec![3.0]);
    }

    #[test]
    fn design_point_reads_the_engine_and_the_cruise_requirement() {
        let config = AlasConfig::default();
        let dp = design_point(&config);
        assert_eq!(dp.mach, config.requirements.cruise_mach);
        assert_eq!(dp.altitude_m, config.requirements.cruise_altitude_m);
        assert_eq!(dp.bypass_ratio, config.geometry.engine.bypass_ratio);
        assert_eq!(
            dp.overall_pressure_ratio,
            config.geometry.engine.overall_pressure_ratio
        );
    }
}
