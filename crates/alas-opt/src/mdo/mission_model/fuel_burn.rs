// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Deterministic dispatch interface for the coupled mission model.
use super::*;

impl FuelBurnModel for SegmentMissionModel {
    fn deterministic_for_dispatch(&self) -> bool {
        true
    }
    fn check_cancellation(&self) -> Result<(), FuelModelError> {
        self.check_cancelled()
    }

    fn trip(&self, takeoff_mass_kg: f64, range_m: f64) -> Result<LegEstimate, FuelModelError> {
        self.fly_trip(takeoff_mass_kg, range_m)
            .map(|flown| flown.leg)
    }

    fn diversion(
        &self,
        start_mass_kg: f64,
        distance_m: f64,
    ) -> Result<LegEstimate, FuelModelError> {
        self.fly_diversion(start_mass_kg, distance_m)
            .map(|flown| flown.leg)
    }

    fn holding_fuel_flow_kg_s(&self, mass_kg: f64, altitude_m: f64) -> Result<f64, FuelModelError> {
        self.check_cancelled()?;
        self.validate().map_err(FuelModelError::InvalidModel)?;
        if !altitude_m.is_finite() {
            return Err(FuelModelError::InvalidDistance {
                distance_m: altitude_m,
            });
        }
        if !mass_kg.is_finite() || mass_kg <= 0.0 {
            return Err(FuelModelError::MassOutOfRange { mass_kg });
        }
        let atmosphere = alas_atmo::us1976_try_compute_values(altitude_m, self.isa_deviation_c)
            .map_err(|error| {
                FuelModelError::InvalidModel(format!("holding atmosphere: {error}"))
            })?;
        // Hold at the minimum-drag lift coefficient of the low-speed polar.
        let cl_best = self.min_drag_cl(0.0, altitude_m);
        let dynamic_pressure_pa = mass_kg * self.gravity_m_s2 / (self.wing_area_m2 * cl_best);
        let speed_m_s = (2.0 * dynamic_pressure_pa / atmosphere.density_kg_m3).sqrt();
        self.level_fuel_flow_kg_s(mass_kg, altitude_m, speed_m_s)
    }

    fn cruise_fuel_flow_kg_s(&self, mass_kg: f64) -> Result<f64, FuelModelError> {
        self.check_cancelled()?;
        self.validate().map_err(FuelModelError::InvalidModel)?;
        self.level_fuel_flow_kg_s(mass_kg, self.cruise_altitude_m, self.cruise_tas_m_s)
    }

    fn taxi_fuel_flow_kg_s(&self) -> Result<f64, FuelModelError> {
        self.check_cancelled()?;
        self.validate().map_err(FuelModelError::InvalidModel)?;
        let flight = self
            .propulsion
            .flight_condition(
                self.departure_elevation_m.max(0.0),
                TAXI_SPEED_M_S,
                self.gravity_m_s2,
                self.isa_deviation_c,
            )
            .map_err(deck_error)?;
        Ok(self
            .propulsion
            .idle_point(flight)
            .map_err(deck_error)?
            .fuel_flow_kg_s)
    }
}
