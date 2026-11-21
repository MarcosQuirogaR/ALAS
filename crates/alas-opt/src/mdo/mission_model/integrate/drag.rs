// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The installed drag the integration flies against: lift coefficient, polar and
//! dynamic pressure at one flight condition and mass.

use alas_mass::fuel_plan::FuelModelError;

use alas_prop::system::FlightCondition;

use super::SegmentMissionModel;

impl SegmentMissionModel {
    /// Lift coefficient and installed drag at `flight` for `mass_kg`:
    /// `CD = CD0 + k CL^2 + CD_wave(M) + delta_cd`.
    pub(in crate::mdo::mission_model) fn drag_components_n(
        &self,
        flight: &FlightCondition,
        mass_kg: f64,
        delta_cd: f64,
        high_lift: Option<f64>,
    ) -> Result<(f64, f64), FuelModelError> {
        let speed = flight.velocity_m_s;
        let dynamic_pressure_pa = 0.5 * flight.density_kg_m3 * speed * speed;
        if !dynamic_pressure_pa.is_finite() || dynamic_pressure_pa <= 0.0 {
            return Err(FuelModelError::InvalidModel(format!(
                "profile dynamic pressure is unusable at {} m and {speed} m/s",
                flight.altitude_m
            )));
        }
        let cl = mass_kg * self.gravity_at(flight.altitude_m)
            / (dynamic_pressure_pa * self.wing_area_m2);
        // `high_lift` is the lift the deployed devices add at constant angle
        // of attack; `None` is the clean configuration.
        let base_cd = match high_lift {
            Some(flap_lift_cl) => {
                self.high_lift_base_cd(cl, flight.mach, flight.altitude_m, flap_lift_cl)
            }
            None => self.clean_cd(cl, flight.mach, flight.altitude_m),
        };
        let cd = base_cd + delta_cd;
        if !cl.is_finite() || !cd.is_finite() || cd <= 0.0 {
            return Err(FuelModelError::NotConverged(format!(
                "invalid drag state CL={cl}, CD={cd}"
            )));
        }
        Ok((cl, dynamic_pressure_pa * self.wing_area_m2 * cd))
    }

    /// Clean installed drag at `flight` for `mass_kg`.
    pub(in crate::mdo::mission_model) fn drag_n(
        &self,
        flight: &FlightCondition,
        mass_kg: f64,
    ) -> Result<f64, FuelModelError> {
        self.drag_components_n(flight, mass_kg, 0.0, None)
            .map(|(_, drag_n)| drag_n)
    }
}
