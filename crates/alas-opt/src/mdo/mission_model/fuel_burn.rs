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
        // Holding is priced in standard conditions: AMC1 CAT.OP.MPA.181 sets
        // the 1,500 ft above aerodrome elevation hold "in standard
        // conditions", whatever the temperature offset of the flown day.
        let atmosphere =
            alas_atmo::us1976_try_compute_values(altitude_m, 0.0).map_err(|error| {
                FuelModelError::InvalidModel(format!("holding atmosphere: {error}"))
            })?;
        // Hold at the minimum-drag lift coefficient of the low-speed polar.
        let cl_best = self.min_drag_cl(0.0, altitude_m);
        let dynamic_pressure_pa =
            mass_kg * self.gravity_at(altitude_m) / (self.wing_area_m2 * cl_best);
        let speed_m_s = (2.0 * dynamic_pressure_pa / atmosphere.density_kg_m3).sqrt();
        self.level_fuel_flow_at_deviation_kg_s(mass_kg, altitude_m, speed_m_s, 0.0)
    }

    fn destination_elevation_m(&self) -> f64 {
        self.arrival_elevation_m
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
                self.gravity_at(self.departure_elevation_m.max(0.0)),
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

impl SegmentMissionModel {
    /// Fuel flow in steady level flight at `altitude_m` and `tas_m_s`, kg/s,
    /// on the flown day.
    pub(super) fn level_fuel_flow_kg_s(
        &self,
        mass_kg: f64,
        altitude_m: f64,
        tas_m_s: f64,
    ) -> Result<f64, FuelModelError> {
        self.level_fuel_flow_at_deviation_kg_s(mass_kg, altitude_m, tas_m_s, self.isa_deviation_c)
    }

    /// Fuel flow in steady level flight on a day `isa_deviation_c` off the
    /// standard atmosphere, kg/s.
    fn level_fuel_flow_at_deviation_kg_s(
        &self,
        mass_kg: f64,
        altitude_m: f64,
        tas_m_s: f64,
        isa_deviation_c: f64,
    ) -> Result<f64, FuelModelError> {
        if !mass_kg.is_finite() || mass_kg <= 0.0 {
            return Err(FuelModelError::MassOutOfRange { mass_kg });
        }
        let flight = self
            .propulsion
            .flight_condition(
                altitude_m,
                tas_m_s,
                self.gravity_at(altitude_m),
                isa_deviation_c,
            )
            .map_err(deck_error)?;
        let drag_n = self.drag_n(&flight, mass_kg)?;
        let point = self
            .propulsion
            .at_thrust(flight, drag_n, PropulsionRating::Cruise)
            .map_err(deck_error)?;
        Ok(point.fuel_flow_kg_s)
    }

    /// Gravity at `altitude_m`, m/s^2: `g0 (r / (r + h))^2` with the mean
    /// Earth radius of the native mission (`alas_mission::segments::frames::
    /// compute_gravity`), so the segment and native missions weigh the
    /// aircraft identically (audit MIS-09). `g0` is `gravity_m_s2`.
    pub fn gravity_at(&self, altitude_m: f64) -> f64 {
        self.gravity_m_s2 * (MEAN_EARTH_RADIUS_M / (MEAN_EARTH_RADIUS_M + altitude_m)).powi(2)
    }

    /// Cruise true airspeed at `altitude_m` for the model's cruise Mach on
    /// the flown day: `M a(T)`, `a` from the US Standard Atmosphere 1976
    /// plus the model's temperature offset (audit MIS-03, MIS-04).
    ///
    /// # Errors
    ///
    /// An atmosphere evaluation that fails at `altitude_m`.
    pub fn cruise_true_airspeed_m_s(&self, altitude_m: f64) -> Result<f64, String> {
        cruise_tas_us1976(self.cruise_mach, altitude_m, self.isa_deviation_c)
    }

    /// This model as a *range search* flies it (a payload-range corner or a
    /// fixed-fuel range): under the `OptimumStep` policy the cruise-level
    /// ceiling is the greater of the route's declared level and the design
    /// cruise altitude.
    ///
    /// The declared level is the filed flight level of the preset's declared
    /// sector (`alas_mission::route_cruise_altitude_m`: FL280 for the A320-200's
    /// 546 km sector, FL250 for the A220-300's 251 nmi sector). It is a
    /// ceiling a dispatcher works below on that sector, not a limit of the
    /// aircraft: a mission several times longer is flown to the aircraft's own
    /// design cruise altitude. A range search that kept the sector's level
    /// flew the A320-200's 5,400 km corner at FL280 and the A220-300's
    /// 6,200 km corner at FL250, 3 and 13 % short of the range of the same
    /// aircraft stepping up toward its design altitude. Policies other than
    /// `OptimumStep` name their altitude explicitly and are left alone, and a
    /// declared level above the design altitude is kept.
    #[must_use]
    pub fn for_range_search(mut self) -> Self {
        let policy = self.profile.cruise_altitude_policy;
        if policy == alas_config::mission::CruiseAltitudePolicy::OptimumStep
            && self.design_cruise_altitude_m > self.cruise_altitude_m
        {
            if let Ok(tas) = self.cruise_true_airspeed_m_s(self.design_cruise_altitude_m) {
                // The profile's cruise speeds are declared at the level the
                // model is built at and read as a Mach at its reference
                // level: moving the reference up must keep that Mach, not
                // the literal true airspeed (A320: 0.78 read as 0.80).
                if self.propulsion.kind() == crate::mdo::propulsion::DeckKind::Turbofan {
                    let sound = |h: f64| {
                        alas_atmo::us1976_try_compute_values(h, 0.0).map(|v| v.speed_of_sound_m_s)
                    };
                    if let (Ok(from), Ok(to)) = (
                        sound(self.cruise_altitude_m),
                        sound(self.design_cruise_altitude_m),
                    ) {
                        let p = &mut self.profile;
                        for speed in [
                            &mut p.cruise_1_air_speed_m_s,
                            &mut p.cruise_2_air_speed_m_s,
                            &mut p.cruise_3_air_speed_m_s,
                        ] {
                            *speed *= to / from;
                        }
                    }
                }
                self.cruise_altitude_m = self.design_cruise_altitude_m;
                self.cruise_tas_m_s = tas;
            }
        }
        self
    }
}

/// Mean Earth radius, m, as in the native mission's gravity.
const MEAN_EARTH_RADIUS_M: f64 = 6.371e6;

/// `M a(h, dT)` from the US Standard Atmosphere 1976, m/s.
pub(super) fn cruise_tas_us1976(
    mach: f64,
    altitude_m: f64,
    isa_deviation_c: f64,
) -> Result<f64, String> {
    alas_atmo::us1976_try_compute_values(altitude_m, isa_deviation_c)
        .map(|values| mach * values.speed_of_sound_m_s)
        .map_err(|error| format!("cruise atmosphere is invalid: {error}"))
}
