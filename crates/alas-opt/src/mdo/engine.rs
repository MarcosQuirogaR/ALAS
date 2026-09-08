// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Installed static thrust read off the configured engine binding, for the
//! airworthiness thrust-to-weight residuals.
//!
//! Mission fuel and off-design thrust no longer come from scalar terms: the
//! sizing loop flies the selected engine's deck through
//! [`super::propulsion::PropulsionDeck`]. What remains here is the
//! sea-level-static rating the takeoff and one-engine-inoperative climb
//! residuals compare against, written once for both technology branches.

use alas_config::{ActiveEngineModel, EngineConfig, TurbopropEngineSpec};

/// Static figure of merit for the actuator-disk static-thrust estimate
/// below: the middle of the 0.7-0.85 range reported for transport
/// propellers (Raymer, ch. 5). No manufacturer static-thrust rating is
/// published for a turboprop in this catalogue, so this is a documented
/// conceptual-design assumption rather than a catalogue value.
const TURBOPROP_STATIC_FIGURE_OF_MERIT: f64 = 0.75;

/// ISA sea-level density, kg/m^3, for the static-thrust estimate below.
const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;

/// Installed static thrust per engine, kN, for the performance residuals'
/// available thrust-to-weight ratio.
///
/// # Errors
///
/// A description of the binding failure, when `engine` does not resolve to a
/// coherent turbofan or turboprop physics payload.
pub(crate) fn static_thrust_kn_per_engine(engine: &EngineConfig) -> Result<f64, String> {
    match engine
        .active_model()
        .map_err(|error| format!("engine binding failed: {error}"))?
    {
        ActiveEngineModel::Turbofan(spec) => Ok(spec.rated_thrust_kn),
        ActiveEngineModel::Turboprop(spec) => Ok(turboprop_static_thrust_n(spec) / 1_000.0),
    }
}

/// Ideal actuator-disk static thrust from momentum theory (e.g. McCormick,
/// *Aerodynamics, Aeronautics, and Flight Mechanics*, ch. 6):
/// `T = (2 rho A P^2)^(1/3)`, discounted by
/// [`TURBOPROP_STATIC_FIGURE_OF_MERIT`].
fn turboprop_static_thrust_n(spec: &TurbopropEngineSpec) -> f64 {
    let disk_area_m2 = std::f64::consts::PI * (spec.propeller_diameter_m / 2.0).powi(2);
    let shaft_power_w = spec.takeoff_shaft_power_kw * 1_000.0;
    let ideal_static_thrust_n =
        (2.0 * SEA_LEVEL_DENSITY_KG_M3 * disk_area_m2 * shaft_power_w * shaft_power_w).cbrt();
    TURBOPROP_STATIC_FIGURE_OF_MERIT.powf(2.0 / 3.0) * ideal_static_thrust_n
}

#[cfg(test)]
mod tests {
    use super::*;
    use alas_config::EngineConfig;

    #[test]
    fn the_default_turbofan_binding_yields_a_finite_positive_static_thrust() {
        let engine = EngineConfig::default();
        let thrust_kn = static_thrust_kn_per_engine(&engine)
            .unwrap_or_else(|error| panic!("static thrust: {error}"));
        assert!(thrust_kn > 0.0 && thrust_kn.is_finite());
    }
}
