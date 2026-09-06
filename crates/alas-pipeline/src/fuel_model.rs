// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The fuel-burn models the pipeline prices a fuel policy with.
//!
//! A fuel policy needs four physical answers -- what a trip burns, what a
//! diversion burns, what holding costs and what taxiing costs -- and the
//! pipeline has two sources for them. The native segment mission is the
//! authoritative trip, but it is a pseudospectral solve and knows nothing
//! about a hold at 1,500 ft. The analytic Breguet model built here from the
//! same report prices everything the mission does not fly, and stands in for
//! the trip too when a caller only needs a first estimate. Both the mission
//! stage that selects the load case and the feasibility stage that checks
//! the flown result build their models through this module, which is what
//! keeps the reserve plan they report identical.

use alas_atmo::Atmosphere;
use alas_config::{ActiveEngineModel, AlasConfig};
use alas_mass::breguet::{equivalent_tsfc_from_psfc, BreguetFuelModel, SegmentFractions};
use alas_mass::fuel_plan::{FuelBurnModel, FuelModelError, LegEstimate};
use alas_units::FOOT;

use crate::full_analysis::AnalysisReport;

/// Propeller efficiency assumed when a turboprop's brake-specific consumption
/// is converted to an equivalent thrust-specific one; a cruise constant-speed
/// propeller at its design advance ratio (Raymer, *Aircraft Design*, ch. 13).
const CRUISE_PROPELLER_EFFICIENCY: f64 = 0.85;

/// Representative horizontal distance a transport covers in climb and
/// descent, credited against the cruise leg of the analytic model. The
/// native mission flies its own climb and descent, so this only shapes the
/// analytic estimate.
const CLIMB_DESCENT_RANGE_CREDIT_M: f64 = 250_000.0;

/// Kilograms of force per newton, for the catalogue's TSFC unit.
const KGF_PER_N: f64 = 1.0 / 9.806_65;

/// Build the analytic burn model from a completed analysis report.
///
/// The drag polar is the report's least-squares fit, the wing area the
/// report's reference area, and the engine terms the typed binding of the
/// configured engine. A report whose fit fell back to constants still yields
/// a model: the fallback is recorded in the fit's status and the caller
/// decides whether to trust the plan.
pub fn breguet_from_report(
    config: &AlasConfig,
    report: &AnalysisReport,
) -> Result<BreguetFuelModel, String> {
    let requirements = &config.requirements;
    let cruise = Atmosphere::new(requirements.cruise_altitude_m);
    let cruise_tas_m_s = requirements.cruise_mach * cruise.speed_of_sound();
    let holding = Atmosphere::new(holding_altitude_m(config, 0.0));
    let engine = &config.geometry.engine;
    let n_engines = engine.spanwise_positions_m.len().max(1) as f64;
    let (tsfc_cruise_kg_per_n_s, takeoff_fuel_flow_kg_s) = match engine
        .active_model()
        .map_err(|error| format!("fuel model engine binding failed: {error}"))?
    {
        ActiveEngineModel::Turbofan(spec) => (
            spec.cruise_tsfc_kg_kgf_hr * KGF_PER_N / 3_600.0,
            spec.takeoff_fuel_flow_kg_s * n_engines,
        ),
        ActiveEngineModel::Turboprop(spec) => {
            // The catalogue states the cruise fuel flow for the whole
            // installation; the brake-specific consumption is per unit of
            // shaft power, so the total flow is divided by the total power.
            let total_cruise_power_w = spec.maximum_cruise_shaft_power_kw * 1_000.0 * n_engines;
            let psfc_kg_per_w_s =
                spec.maximum_cruise_fuel_flow_kg_h / 3_600.0 / total_cruise_power_w;
            let takeoff_flow_kg_s =
                psfc_kg_per_w_s * spec.takeoff_shaft_power_kw * 1_000.0 * n_engines;
            (
                equivalent_tsfc_from_psfc(
                    psfc_kg_per_w_s,
                    cruise_tas_m_s,
                    CRUISE_PROPELLER_EFFICIENCY,
                ),
                takeoff_flow_kg_s,
            )
        }
    };
    let model = BreguetFuelModel {
        cruise_tas_m_s,
        cruise_density_kg_m3: cruise.density(),
        holding_density_kg_m3: holding.density(),
        wing_area_m2: report.airplane.s_ref,
        cd0: report.polar_fit.cd0,
        induced_factor_k: report.polar_fit.k,
        tsfc_cruise_kg_per_n_s,
        holding_tsfc_factor: BreguetFuelModel::DEFAULT_HOLDING_TSFC_FACTOR,
        takeoff_fuel_flow_kg_s,
        idle_fuel_flow_fraction: BreguetFuelModel::DEFAULT_IDLE_FUEL_FLOW_FRACTION,
        gravity_m_s2: requirements.gravity_m_s2,
        segment_fractions: SegmentFractions::default(),
        climb_descent_range_credit_m: CLIMB_DESCENT_RANGE_CREDIT_M,
    };
    model
        .validate()
        .map_err(|error| format!("analytic fuel model is not usable: {error}"))?;
    Ok(model)
}

/// The altitude the policy evaluates holding fuel at: the configured height
/// above the aerodrome, over `aerodrome_elevation_m`.
pub fn holding_altitude_m(config: &AlasConfig, aerodrome_elevation_m: f64) -> f64 {
    aerodrome_elevation_m + config.fuel_policy.holding_altitude_ft * FOOT
}

/// A burn model whose trip is a leg that was actually flown.
///
/// The dispatch closure flies the native mission at a takeoff mass and asks
/// the policy what that flight requires. The trip in that question is the
/// flown one, not a re-estimate, so this model answers `trip` with the leg it
/// was given and prices every other quantity through the analytic model.
/// The mass argument of `trip` is deliberately ignored: the leg belongs to
/// the takeoff mass the caller flew, and asking for another mass is a caller
/// error the closure loop never makes because it re-flies instead.
#[derive(Debug, Clone, Copy)]
pub struct FlownTripModel<'a> {
    /// The flown trip.
    pub leg: LegEstimate,
    /// The analytic model for holding, diversion and taxi.
    pub analytic: &'a BreguetFuelModel,
}

impl FuelBurnModel for FlownTripModel<'_> {
    fn trip(&self, _takeoff_mass_kg: f64, range_m: f64) -> Result<LegEstimate, FuelModelError> {
        if !range_m.is_finite() || range_m < 0.0 {
            return Err(FuelModelError::InvalidDistance {
                distance_m: range_m,
            });
        }
        Ok(self.leg)
    }

    fn diversion(
        &self,
        start_mass_kg: f64,
        distance_m: f64,
    ) -> Result<LegEstimate, FuelModelError> {
        self.analytic.diversion(start_mass_kg, distance_m)
    }

    fn holding_fuel_flow_kg_s(&self, mass_kg: f64, altitude_m: f64) -> Result<f64, FuelModelError> {
        self.analytic.holding_fuel_flow_kg_s(mass_kg, altitude_m)
    }

    fn cruise_fuel_flow_kg_s(&self, mass_kg: f64) -> Result<f64, FuelModelError> {
        self.analytic.cruise_fuel_flow_kg_s(mass_kg)
    }

    fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
        self.analytic.taxi_fuel_flow_kg_s()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::full_analysis::FullAnalysis;
    use alas_config::design_variables::DesignVector;

    #[test]
    fn the_default_aircraft_yields_a_usable_analytic_model() {
        let config = AlasConfig::default();
        let report = FullAnalysis::new(config.clone())
            .run(&DesignVector::default(), true)
            .unwrap_or_else(|error| panic!("default report: {error}"));
        let model = breguet_from_report(&config, &report)
            .unwrap_or_else(|error| panic!("analytic model: {error}"));
        assert!(model.cruise_tas_m_s > 200.0);
        assert!(model.tsfc_cruise_kg_per_n_s > 1.0e-5 && model.tsfc_cruise_kg_per_n_s < 3.0e-5);
        assert!(model.takeoff_fuel_flow_kg_s > 1.0);
        let trip = model
            .trip(config.requirements.mtow_kg, 5_000_000.0)
            .unwrap_or_else(|error| panic!("trip: {error}"));
        assert!(trip.fuel_kg > 0.0 && trip.fuel_kg < config.requirements.mtow_kg);
    }

    #[test]
    fn a_flown_trip_model_answers_the_trip_with_the_flown_leg() {
        let analytic = BreguetFuelModel {
            cruise_tas_m_s: 230.0,
            cruise_density_kg_m3: 0.38,
            holding_density_kg_m3: 1.17,
            wing_area_m2: 122.6,
            cd0: 0.02,
            induced_factor_k: 0.045,
            tsfc_cruise_kg_per_n_s: 1.7e-5,
            holding_tsfc_factor: 1.0,
            takeoff_fuel_flow_kg_s: 2.3,
            idle_fuel_flow_fraction: 0.07,
            gravity_m_s2: 9.81,
            segment_fractions: SegmentFractions::default(),
            climb_descent_range_credit_m: 250_000.0,
        };
        let leg = LegEstimate {
            fuel_kg: 4_321.0,
            time_s: 5_400.0,
        };
        let model = FlownTripModel {
            leg,
            analytic: &analytic,
        };
        assert_eq!(model.trip(70_000.0, 1.0e6).ok(), Some(leg));
        assert!(model.trip(70_000.0, -1.0).is_err());
        assert!(model.taxi_fuel_flow_kg_s().is_ok());
        assert_eq!(
            holding_altitude_m(&AlasConfig::default(), 100.0),
            100.0 + 1_500.0 * FOOT
        );
    }
}
