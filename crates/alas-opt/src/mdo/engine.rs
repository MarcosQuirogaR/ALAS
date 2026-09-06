// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cruise fuel-consumption and static-thrust terms read off the configured
//! engine binding.
//!
//! The analytic Breguet model needs a cruise TSFC and a takeoff fuel flow;
//! the performance residuals need an installed static thrust. Both come from
//! the same typed engine binding
//! [`alas_config::EngineConfig::active_model`] resolves, so the
//! turbofan/turboprop branch is written once here rather than twice.

use alas_config::{ActiveEngineModel, EngineConfig, TurbopropEngineSpec};
use alas_mass::breguet::equivalent_tsfc_from_psfc;

/// Kilograms of force per newton, for the catalogue's TSFC unit.
const KGF_PER_N: f64 = 1.0 / 9.806_65;

/// Propeller efficiency assumed when a turboprop's brake-specific
/// consumption is converted to an equivalent thrust-specific one: a cruise
/// constant-speed propeller at its design advance ratio (Raymer, *Aircraft
/// Design: A Conceptual Approach*, ch. 13), matching the assumption
/// `alas_pipeline::fuel_model::breguet_from_report` documents for the same
/// conversion.
const CRUISE_PROPELLER_EFFICIENCY: f64 = 0.85;

/// The catalogue's maximum-cruise turboprop fuel flow is published for a
/// two-engine installation regardless of the configured engine count
/// (`TurbopropEngineSpec::maximum_cruise_fuel_flow_kg_h`'s own doc comment).
/// Dividing by this fixed reference, rather than by the configured engine
/// count, recovers the per-engine power-specific consumption the published
/// anchor was measured against.
const TURBOPROP_FUEL_FLOW_REFERENCE_ENGINE_COUNT: f64 = 2.0;

/// Static figure of merit for the actuator-disk static-thrust estimate
/// below: the middle of the 0.7-0.85 range reported for transport
/// propellers (Raymer, ch. 5). No manufacturer static-thrust rating is
/// published for a turboprop in this catalogue, so this is a documented
/// conceptual-design assumption rather than a catalogue value.
const TURBOPROP_STATIC_FIGURE_OF_MERIT: f64 = 0.75;

/// ISA sea-level density, kg/m^3, for the static-thrust estimate below.
const SEA_LEVEL_DENSITY_KG_M3: f64 = 1.225;

/// Cruise TSFC and takeoff fuel flow for the whole installation.
pub(crate) struct EngineTerms {
    /// Cruise thrust-specific fuel consumption, kg of fuel per newton per
    /// second.
    pub tsfc_cruise_kg_per_n_s: f64,
    /// All-engines takeoff fuel flow at sea-level static, kg/s.
    pub takeoff_fuel_flow_kg_s: f64,
}

/// Resolve the cruise TSFC and takeoff fuel flow from `engine`'s active
/// physics binding.
///
/// # Errors
///
/// A description of the binding failure, when `engine` does not resolve to a
/// coherent turbofan or turboprop physics payload.
pub(crate) fn engine_terms(
    engine: &EngineConfig,
    cruise_tas_m_s: f64,
    n_engines: f64,
) -> Result<EngineTerms, String> {
    match engine
        .active_model()
        .map_err(|error| format!("engine binding failed: {error}"))?
    {
        ActiveEngineModel::Turbofan(spec) => Ok(EngineTerms {
            tsfc_cruise_kg_per_n_s: spec.cruise_tsfc_kg_kgf_hr * KGF_PER_N / 3_600.0,
            takeoff_fuel_flow_kg_s: spec.takeoff_fuel_flow_kg_s * n_engines,
        }),
        ActiveEngineModel::Turboprop(spec) => {
            let psfc_kg_per_w_s = spec.maximum_cruise_fuel_flow_kg_h
                / 3_600.0
                / (TURBOPROP_FUEL_FLOW_REFERENCE_ENGINE_COUNT
                    * spec.maximum_cruise_shaft_power_kw
                    * 1_000.0);
            Ok(EngineTerms {
                tsfc_cruise_kg_per_n_s: equivalent_tsfc_from_psfc(
                    psfc_kg_per_w_s,
                    cruise_tas_m_s,
                    CRUISE_PROPELLER_EFFICIENCY,
                ),
                takeoff_fuel_flow_kg_s: psfc_kg_per_w_s
                    * spec.takeoff_shaft_power_kw
                    * 1_000.0
                    * n_engines,
            })
        }
    }
}

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
    fn the_default_turbofan_binding_yields_finite_positive_terms() {
        let engine = EngineConfig::default();
        let terms = engine_terms(&engine, 250.0, 2.0)
            .unwrap_or_else(|error| panic!("engine terms: {error}"));
        assert!(terms.tsfc_cruise_kg_per_n_s > 0.0 && terms.tsfc_cruise_kg_per_n_s.is_finite());
        assert!(terms.takeoff_fuel_flow_kg_s > 0.0);
        let thrust_kn = static_thrust_kn_per_engine(&engine)
            .unwrap_or_else(|error| panic!("static thrust: {error}"));
        assert!(thrust_kn > 0.0 && thrust_kn.is_finite());
    }
}
