// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py

//! Propulsion thermodynamic cycle summary, the engine designer's nacelle
//! preview, and parametric trade-space sweeps (carpet plot, efficiency
//! decomposition, bypass-ratio sensitivity, altitude/Mach envelope).
//!
//! Every plotted quantity comes from [`alas_prop::cycle::compute_turbofan_cycle`]
//! or one of its [`alas_prop::cycle::sweeps`] wrappers, evaluated at the
//! current [`AlasConfig`]'s engine and flight-condition fields, matching
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
mod ts_preview;
mod turboprop_cycle;

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

use crate::families::common::linspace;
use crate::scene::Scene;

/// The cruise design point every propulsion figure evaluates the cycle at.
///
/// Ported from `_propulsion_design_point` (`visualization.py`): the current
/// engine's BPR/OPR/FPR/TIT at the design requirements' cruise Mach and
/// altitude.
fn design_point(config: &AlasConfig, eng: &TurbofanEngineSpec) -> TurbofanCycleInputs {
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
/// Invalid technology/payload combinations must never fall back to the
/// deprecated flat compatibility copy; they surface as the error text so the
/// figure can show a status scene instead of aborting the process.
fn turbofan_spec(config: &AlasConfig) -> Result<&TurbofanEngineSpec, String> {
    match config.geometry.engine.active_model() {
        Ok(ActiveEngineModel::Turbofan(spec)) => Ok(spec),
        Ok(ActiveEngineModel::Turboprop(_)) => {
            Err("a turboprop binding reached a turbofan-only figure".to_owned())
        }
        Err(error) => Err(format!("invalid propulsion binding: {error}")),
    }
}

/// [`turbofan_spec`], or the status scene a figure of `size` returns instead.
fn turbofan_or_status<'a>(
    config: &'a AlasConfig,
    theme: Option<&str>,
    size: (f64, f64),
) -> Result<&'a TurbofanEngineSpec, Scene> {
    turbofan_spec(config).map_err(|message| {
        crate::status_figure::sized_status_scene(
            "Propulsion model unavailable",
            &message,
            false,
            crate::theme::get_palette(theme),
            size.0,
        )
    })
}

fn is_turboprop(config: &AlasConfig) -> bool {
    matches!(
        config.geometry.engine.propulsion_technology,
        alas_config::PropulsionTechnology::Turboprop
    )
}

#[cfg(test)]
// A failed expect in this module is the assertion failing, not a library
// invariant being broken.
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn design_point_reads_the_engine_and_the_cruise_requirement() {
        let config = AlasConfig::default();
        let eng = turbofan_spec(&config).expect("default config binds a turbofan");
        let dp = design_point(&config, eng);
        assert_eq!(dp.mach, config.requirements.cruise_mach);
        assert_eq!(dp.altitude_m, config.requirements.cruise_altitude_m);
        assert_eq!(dp.bypass_ratio, config.geometry.engine.bypass_ratio);
        assert_eq!(
            dp.overall_pressure_ratio,
            config.geometry.engine.overall_pressure_ratio
        );
    }
}
