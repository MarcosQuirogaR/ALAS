// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The fuel-burn models the pipeline prices a fuel policy with.
//!
//! A fuel policy needs four physical answers: what a trip burns, what a
//! diversion burns, what holding costs and what taxiing costs, and the
//! pipeline has two sources for them. The native segment mission is the
//! authoritative trip, but it is a pseudospectral solve and knows nothing
//! about a hold at 1,500 ft. The analytic Breguet model built here from the
//! same report prices everything the mission does not fly, and stands in for
//! the trip too when a caller only needs a first estimate. Both the mission
//! stage that selects the load case and the feasibility stage that checks
//! the flown result build their models through this module, which is what
//! keeps the reserve plan they report identical.

use alas_config::{airport_dataset, AlasConfig};
use alas_mass::fuel_plan::{FuelBurnModel, FuelModelError, LegEstimate};
use alas_units::FOOT;

use crate::full_analysis::AnalysisReport;
use alas_opt::mdo::mission_model::PhaseAeroLimits;
use alas_opt::mdo::propulsion::{max_climb_rate_ft_min, PropulsionDeck};
use alas_opt::SegmentMissionModel;

/// Build the segment burn model from a completed analysis report.
///
/// The drag components are read at the report's design-point lift, the wing
/// area is the report's reference area, and thrust and fuel flow come from
/// the same off-design propulsion deck the optimizer's sizing loop and the
/// native mission stage use. A report whose fit fell back to constants still
/// yields a model: the fallback is recorded in the fit's status and the
/// caller decides whether to trust the plan.
pub fn breguet_from_report(
    config: &AlasConfig,
    report: &AnalysisReport,
) -> Result<SegmentMissionModel, String> {
    let requirements = &config.requirements;
    let propulsion = PropulsionDeck::from_engine(
        &config.geometry.engine,
        requirements.cruise_mach,
        requirements.cruise_altitude_m,
        max_climb_rate_ft_min(config.mission.profile.initial_climb_rate_m_s),
    )
    .map_err(|error| format!("fuel model engine binding failed: {error}"))?;
    let (cd0, induced_factor_k, wave_drag_cd) = report_drag_components(report);
    let departure_elevation_m = airport_dataset::resolve(&config.departure_airport)
        .ok()
        .and_then(|airport| airport.elevation_m.value)
        .unwrap_or(0.0);
    let arrival_elevation_m = airport_dataset::resolve(&config.arrival_airport)
        .ok()
        .and_then(|airport| airport.elevation_m.value)
        .unwrap_or(0.0);
    // The native mission applies the departure airport's ISA deviation (from
    // the same registry `build_mission_request` reads) to every segment; the
    // segment burn model flies the same deviation so the fuel policy and the
    // flown mission share one ambient convention.
    let departure_isa_deviation_c = alas_config::airports::get(&config.departure_airport)
        .map(|airport| airport.isa_deviation_c)
        .unwrap_or(0.0);
    let holding_altitude = holding_altitude_m(config, arrival_elevation_m);
    // The altitude this route is actually flown at, by the same rule the
    // published mission and the optimizer's sizing mission use
    // (`alas_mission::route_cruise_altitude_m`). `requirements.cruise_altitude_m`
    // is the *design* cruise altitude, and pricing the fuel policy there while
    // the mission is flown somewhere else is the third instance of one
    // inconsistency: the policy model is then built for a flight the aircraft
    // does not make, and when it cannot be built or cannot solve the run
    // reports `fuel_policy_unavailable` and refuses the design. That was the
    // dominant acceptance rejection in the measured matrix - the AVE and the
    // B787-9 finalists both.
    //
    // A route whose airports do not resolve keeps the design altitude, because
    // nothing else has been declared for it.
    let flown_cruise_altitude_m = match (
        alas_config::airports::get(&config.departure_airport),
        alas_config::airports::get(&config.arrival_airport),
    ) {
        (Ok(origin), Ok(destination)) => {
            alas_mission::route_cruise_altitude_m(config, origin, destination)
        }
        _ => requirements.cruise_altitude_m,
    };
    SegmentMissionModel::new(
        config.mission.profile.clone(),
        requirements.cruise_mach,
        flown_cruise_altitude_m,
        departure_elevation_m,
        arrival_elevation_m,
        report.airplane.s_ref,
        cd0,
        induced_factor_k,
        wave_drag_cd,
        requirements.gravity_m_s2,
        holding_altitude,
        PhaseAeroLimits::from_config(config),
        propulsion,
    )
    .map(|model| model.with_isa_deviation_c(departure_isa_deviation_c))
    .map_err(|error| format!("segment fuel model is not usable: {error}"))
}

/// Read the component arrays at the report's design-point lift. The fitted
/// `PolarFit` is retained for reporting, but fitting total CD can fold the
/// transonic wave term into `k`; the shared mission model must receive the
/// VLM induced term and the wave term separately.
fn report_drag_components(report: &AnalysisReport) -> (f64, f64, f64) {
    let index = report
        .polar
        .cl
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            (**left - report.design_point.cl)
                .abs()
                .total_cmp(&(**right - report.design_point.cl).abs())
        })
        .map(|(index, _)| index);
    let Some(index) = index else {
        return (
            report.polar_fit.cd0.max(1.0e-5),
            report.polar_fit.k.max(1.0e-5),
            0.0,
        );
    };
    let cl = report.polar.cl.get(index).copied().unwrap_or(f64::NAN);
    let parasite = report
        .polar
        .cd_parasite
        .get(index)
        .copied()
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(report.polar_fit.cd0);
    let induced = report
        .polar
        .cd_induced
        .get(index)
        .copied()
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(f64::NAN);
    let k = if cl.is_finite() && cl.abs() > 1.0e-8 && induced.is_finite() {
        induced / (cl * cl)
    } else {
        report.polar_fit.k
    };
    let wave = report
        .polar
        .cd_wave
        .get(index)
        .copied()
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.0);
    (parasite.max(1.0e-5), k.max(1.0e-5), wave.max(0.0))
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
pub struct FlownTripModel<'a, M: FuelBurnModel + ?Sized> {
    /// The flown trip.
    pub leg: LegEstimate,
    /// The analytic model for holding, diversion and taxi.
    pub analytic: &'a M,
}

impl<M: FuelBurnModel + ?Sized> FuelBurnModel for FlownTripModel<'_, M> {
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
    use alas_mass::breguet::{BreguetFuelModel, SegmentFractions};

    #[test]
    fn the_default_aircraft_yields_a_usable_analytic_model() {
        let config = AlasConfig::default();
        let report = FullAnalysis::new(config.clone())
            .run(&DesignVector::default(), true)
            .unwrap_or_else(|error| panic!("default report: {error}"));
        let model = breguet_from_report(&config, &report)
            .unwrap_or_else(|error| panic!("analytic model: {error}"));
        assert!(model.cruise_tas_m_s > 200.0);
        let cruise_flow = model
            .cruise_fuel_flow_kg_s(config.requirements.mtow_kg)
            .unwrap_or_else(|error| panic!("cruise flow: {error}"));
        let taxi_flow = model
            .taxi_fuel_flow_kg_s()
            .unwrap_or_else(|error| panic!("taxi flow: {error}"));
        assert!(
            cruise_flow > 0.1 && cruise_flow < 10.0,
            "cruise flow {cruise_flow} kg/s"
        );
        assert!(taxi_flow > 0.0 && taxi_flow < cruise_flow);
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
