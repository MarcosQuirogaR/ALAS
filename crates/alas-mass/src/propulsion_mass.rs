// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Technology-aware preliminary installed-propulsion mass estimates.
//!
//! These estimates are conceptual-design correlations, not component weight
//! statements.  In particular, the turboprop path is calibrated to public
//! secondary PW127M/568F mass figures because no revision-controlled OEM mass
//! statement is present in the project evidence set.

use alas_config::TurbopropEngineSpec;

/// Evidence quality attached to a propulsion mass estimate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropulsionMassEvidence {
    /// Historical thrust-to-weight and installation-factor correlation.
    HistoricalCorrelation,
    /// Power-scaled correlation calibrated to secondary component figures.
    SecondaryCalibration,
}

/// Auditable installed propulsion mass result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PropulsionMassEstimate {
    /// Bare engine mass for all installed engines, kg.
    pub dry_engines_kg: f64,
    /// Propeller mass for all installed engines, kg (zero for turbofans).
    pub propellers_kg: f64,
    /// Pylons, nacelles, controls and installation accessories, kg.
    pub installation_kg: f64,
    /// Sum of dry engines, propellers and installation, kg.
    pub total_kg: f64,
    /// Evidence class governing the estimate.
    pub evidence: PropulsionMassEvidence,
    /// Approximate relative one-sigma uncertainty used for design trades.
    pub relative_uncertainty: f64,
}

/// Preserve the historical turbofan mass equation exactly.
pub(crate) fn turbofan_installed_mass(
    thrust_per_engine_n: f64,
    engine_count: usize,
    thrust_to_weight_factor: f64,
    installation_factor: f64,
    gravity_m_s2: f64,
) -> Option<PropulsionMassEstimate> {
    if thrust_per_engine_n <= 0.0 {
        return None;
    }
    // Keep the association used by the legacy buildup: n * (T / (factor*g))
    // * installation. This matters to exact-reference floating-point parity.
    let dry_engines_kg =
        engine_count as f64 * (thrust_per_engine_n / (thrust_to_weight_factor * gravity_m_s2));
    let total_kg = dry_engines_kg * installation_factor;
    Some(PropulsionMassEstimate {
        dry_engines_kg,
        propellers_kg: 0.0,
        installation_kg: total_kg - dry_engines_kg,
        total_kg,
        evidence: PropulsionMassEvidence::HistoricalCorrelation,
        relative_uncertainty: 0.20,
    })
}

/// Estimate a PW127-class engine, propeller and installation from rated power.
///
/// The calibration point is 480 kg per PW127M dry engine and 180 kg per
/// Hamilton Sundstrand 568F propeller at 1,845.8 kW (2,475 shp).  Those values
/// are secondary public evidence and therefore carry a deliberately broad
/// ±25% preliminary uncertainty. Exponents 0.8 (gas turbine) and 0.5
/// (propeller) provide smooth conceptual scaling without claiming an OEM
/// family regression. The 25% installation allowance covers nacelle, mount,
/// controls, fire protection and accessories; it excludes fuel.
pub fn turboprop_installed_mass(
    spec: &TurbopropEngineSpec,
    engine_count: usize,
) -> Option<PropulsionMassEstimate> {
    turboprop_installed_mass_from_power(spec.takeoff_shaft_power_kw, engine_count)
}

/// Power-only form used by mass bridges that do not own an engine catalogue.
pub fn turboprop_installed_mass_from_power(
    takeoff_shaft_power_kw: f64,
    engine_count: usize,
) -> Option<PropulsionMassEstimate> {
    const REFERENCE_POWER_KW: f64 = 1_845.607_183_2;
    const REFERENCE_ENGINE_MASS_KG: f64 = 480.0;
    const REFERENCE_PROPELLER_MASS_KG: f64 = 180.0;
    const INSTALLATION_FACTOR: f64 = 1.25;

    let power_kw = takeoff_shaft_power_kw;
    if engine_count == 0 || !power_kw.is_finite() || power_kw <= 0.0 {
        return None;
    }
    let power_ratio = power_kw / REFERENCE_POWER_KW;
    let dry_engines_kg = engine_count as f64 * REFERENCE_ENGINE_MASS_KG * power_ratio.powf(0.8);
    let propellers_kg = engine_count as f64 * REFERENCE_PROPELLER_MASS_KG * power_ratio.sqrt();
    let bare_kg = dry_engines_kg + propellers_kg;
    let total_kg = bare_kg * INSTALLATION_FACTOR;
    Some(PropulsionMassEstimate {
        dry_engines_kg,
        propellers_kg,
        installation_kg: total_kg - bare_kg,
        total_kg,
        evidence: PropulsionMassEvidence::SecondaryCalibration,
        relative_uncertainty: 0.25,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pw127m() -> TurbopropEngineSpec {
        TurbopropEngineSpec {
            takeoff_shaft_power_kw: 1_845.607_183_2,
            maximum_reserve_shaft_power_kw: 2_051.0,
            maximum_continuous_shaft_power_kw: 1_864.3,
            maximum_climb_shaft_power_kw: 1_790.0,
            maximum_cruise_shaft_power_kw: 1_620.0,
            maximum_cruise_fuel_flow_kg_h: 820.0,
            propeller_model: "568F-1".into(),
            propeller_diameter_m: 3.93,
            governed_propeller_speed_rpm: 1_200.0,
            reduction_ratio: 16.7,
            rating_source: String::new(),
            geometry_source: String::new(),
        }
    }

    #[test]
    fn atr_calibration_keeps_engine_propeller_and_installation_explicit() {
        let estimate = turboprop_installed_mass(&pw127m(), 2).unwrap();
        assert!((estimate.dry_engines_kg - 960.0).abs() < 1e-9);
        assert!((estimate.propellers_kg - 360.0).abs() < 1e-9);
        assert!((estimate.installation_kg - 330.0).abs() < 1e-9);
        assert!((estimate.total_kg - 1_650.0).abs() < 1e-9);
        assert_eq!(
            estimate.evidence,
            PropulsionMassEvidence::SecondaryCalibration
        );
        assert_eq!(estimate.relative_uncertainty, 0.25);
    }

    #[test]
    fn zero_power_is_not_reinterpreted_as_a_thrust_rating() {
        let mut spec = pw127m();
        spec.takeoff_shaft_power_kw = 0.0;
        assert!(turboprop_installed_mass(&spec, 2).is_none());
    }
}
