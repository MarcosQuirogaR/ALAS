// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fuel, time and distance of the acceleration from the take-off safety
//! speed to the first climb speed (audit follow-up of lane C1).
//!
//! The planned ladder opens with its take-off band flown at the profile's
//! take-off climb speed (128.6 m/s TAS, 250 kt, by default), while the
//! aeroplane clears the 35 ft screen at the take-off safety speed `V2`
//! (CS 25.107(c): `V2 >= 1.13 VSR`, with `VSR >= V_S1g`). The kinetic
//! energy between the two, `m (V_climb^2 - V2^2) / 2` (about 380 MJ for a
//! 70 t aeroplane, 1.9 GJ for a 400 t one), is not in any ladder segment:
//! the segments open a kinetic budget only between one segment and the next.
//!
//! It is charged here by the energy method, level at the screen height, at
//! the take-off/go-around rating, in take-off configuration (flaps, gear
//! retracted: the second-segment polar the take-off band uses). Each of
//! [`SPEED_STEPS`] equal speed increments obeys
//! `(T - D) V dt = m V dV`, so the thrust work exceeds the drag work by the
//! kinetic energy gained, to round-off, and the burn is the deck's fuel flow
//! at that rating over `dt`. `V2 = 1.13 V_S1g` is evaluated at the lift-off
//! mass, the departure field density and the take-off maximum lift
//! coefficient; no take-off distance, obstacle or engine-failure case is
//! modelled (the acceleration is flown with all engines, as the rest of the
//! ladder is), and a diversion go-around is charged by the same relation as
//! the closest available description of its recovery to climb speed.

use alas_prop::system::PropulsionRating;

use super::integrate::EnergyLedger;
use super::SegmentMissionModel;
use alas_mass::fuel_plan::FuelModelError;

/// Screen height at which `V2` is reached, 35 ft in metres (CS 25.113).
const SCREEN_HEIGHT_M: f64 = 35.0 * alas_units::FOOT;
/// `V2 / V_S1g` (CS 25.107(c)(2): `V2 >= 1.13 VSR`, `VSR >= V_S1g`).
pub(super) const V2_OVER_STALL_SPEED: f64 = 1.13;
/// Equal speed increments of the integration; the thrust and drag change
/// slowly over the 40-60 m/s span, and the energy identity holds exactly for
/// any count.
const SPEED_STEPS: usize = 16;

/// What the acceleration cost and where it left the aeroplane.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(super) struct Acceleration {
    /// Fuel burned, kg.
    pub fuel_kg: f64,
    /// Time, s.
    pub time_s: f64,
    /// Horizontal distance flown, m.
    pub distance_m: f64,
    /// Thrust work, drag work and kinetic energy gained (the other ledger
    /// entries are zero: the acceleration is level).
    pub ledger: EnergyLedger,
    /// Take-off safety speed it started from, m/s (zero when no
    /// acceleration was needed).
    pub v2_m_s: f64,
}

impl SegmentMissionModel {
    /// The take-off safety speed, m/s, at lift-off mass `mass_kg` over the
    /// field at `altitude_m`: `1.13 sqrt(2 m g / (rho S CL_max,takeoff))`.
    pub(super) fn takeoff_safety_speed_m_s(
        &self,
        altitude_m: f64,
        mass_kg: f64,
    ) -> Result<f64, FuelModelError> {
        let screen_m = altitude_m + SCREEN_HEIGHT_M;
        let flight = self
            .propulsion
            .flight_condition(
                screen_m,
                1.0,
                self.gravity_at(screen_m),
                self.isa_deviation_c,
            )
            .map_err(super::deck_error)?;
        let weight_n = mass_kg * self.gravity_at(screen_m);
        let stall_m_s = (2.0 * weight_n
            / (flight.density_kg_m3 * self.wing_area_m2 * self.phase_limits.takeoff_cl_max))
            .sqrt();
        if !stall_m_s.is_finite() || stall_m_s <= 0.0 {
            return Err(FuelModelError::InvalidModel(format!(
                "take-off stall speed {stall_m_s} m/s is unusable at {mass_kg} kg"
            )));
        }
        Ok(V2_OVER_STALL_SPEED * stall_m_s)
    }

    /// Accelerate from `V2` to `target_tas_m_s` level at the field's screen
    /// height with the take-off rating; zero when `V2` is not below the
    /// target.
    ///
    /// # Errors
    ///
    /// A propulsion or polar failure, or a thrust that cannot accelerate the
    /// aeroplane (`T <= D`) at some speed of the span.
    pub(super) fn takeoff_acceleration(
        &self,
        altitude_m: f64,
        mass_kg: f64,
        target_tas_m_s: f64,
    ) -> Result<Acceleration, FuelModelError> {
        if !mass_kg.is_finite() || mass_kg <= 0.0 {
            return Err(FuelModelError::MassOutOfRange { mass_kg });
        }
        let v2_m_s = self.takeoff_safety_speed_m_s(altitude_m, mass_kg)?;
        if !target_tas_m_s.is_finite() || target_tas_m_s <= v2_m_s {
            return Ok(Acceleration::default());
        }
        let screen_m = altitude_m + SCREEN_HEIGHT_M;
        let dv_m_s = (target_tas_m_s - v2_m_s) / SPEED_STEPS as f64;
        let mut out = Acceleration {
            v2_m_s,
            ..Acceleration::default()
        };
        let mut mass = mass_kg;
        for index in 0..SPEED_STEPS {
            let (low, high) = (
                v2_m_s + dv_m_s * index as f64,
                v2_m_s + dv_m_s * (index + 1) as f64,
            );
            let mid = 0.5 * (low + high);
            let flight = self
                .propulsion
                .flight_condition(
                    screen_m,
                    mid,
                    self.gravity_at(screen_m),
                    self.isa_deviation_c,
                )
                .map_err(super::deck_error)?;
            let point = self
                .propulsion
                .rated_point(flight, PropulsionRating::TakeoffGoAround)
                .map_err(super::deck_error)?;
            let delta_cd = self.phase_limits.high_lift_delta_cd;
            let net_n = |at_kg: f64| -> Result<(f64, f64), FuelModelError> {
                let (_, drag_n) = self.drag_components_n(&flight, at_kg, delta_cd, true)?;
                Ok((drag_n, point.thrust_n - drag_n))
            };
            // Midpoint-mass predictor, as the profile integrator does.
            let (_, net0_n) = net_n(mass)?;
            if net0_n <= 0.0 {
                return Err(FuelModelError::NotConverged(format!(
                    "take-off thrust deficit: {:.0} N cannot accelerate the aeroplane at {mid:.1} m/s \
                     (net force {net0_n:.0} N)",
                    point.thrust_n
                )));
            }
            let predicted_burn_kg = point.fuel_flow_kg_s * mass * dv_m_s / net0_n;
            let mid_mass = mass - 0.5 * predicted_burn_kg;
            let (drag_n, net_n) = net_n(mid_mass)?;
            if net_n <= 0.0 {
                return Err(FuelModelError::NotConverged(format!(
                    "take-off thrust deficit: cannot accelerate the aeroplane at {mid:.1} m/s"
                )));
            }
            let dt_s = mid_mass * dv_m_s / net_n;
            let burn_kg = point.fuel_flow_kg_s * dt_s;
            if !burn_kg.is_finite() || burn_kg < 0.0 || burn_kg >= mass {
                return Err(FuelModelError::NotConverged(format!(
                    "take-off acceleration burn {burn_kg} kg is unusable at {mass} kg"
                )));
            }
            out.ledger.propulsive_work_j += point.thrust_n * mid * dt_s;
            out.ledger.drag_work_j += drag_n * mid * dt_s;
            out.ledger.kinetic_energy_j += 0.5 * mid_mass * (high * high - low * low);
            out.fuel_kg += burn_kg;
            out.time_s += dt_s;
            out.distance_m += mid * dt_s;
            mass -= burn_kg;
        }
        Ok(out)
    }
}

// These tests use unwrap/expect for controlled fixtures; a failure is the
// assertion with its context rather than a production error path.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdo::build::build_geometry;
    use crate::mdo::propulsion::max_climb_rate_ft_min;
    use alas_config::design_variables::DesignVector;
    use alas_config::AlasConfig;

    fn model() -> SegmentMissionModel {
        let (config, _, plane) =
            build_geometry(&AlasConfig::default(), &DesignVector::default().to_array())
                .expect("default geometry");
        let requirements = &config.requirements;
        let deck = crate::mdo::propulsion::PropulsionDeck::from_engine(
            &config.geometry.engine,
            requirements.cruise_mach,
            requirements.cruise_altitude_m,
            max_climb_rate_ft_min(config.mission.profile.initial_climb_rate_m_s),
        )
        .expect("default propulsion deck");
        let phase_limits = super::super::PhaseAeroLimits::from_config(&config);
        SegmentMissionModel::new(
            config.mission.profile,
            requirements.cruise_mach,
            requirements.cruise_altitude_m,
            0.0,
            0.0,
            plane.s_ref,
            std::sync::Arc::new(super::super::ParabolicPolar::new(
                0.018,
                0.045,
                0.002,
                requirements.cruise_mach,
            )),
            requirements.gravity_m_s2,
            457.2,
            phase_limits,
            deck,
        )
        .expect("default mission model")
    }

    #[test]
    fn the_acceleration_energy_ledger_closes_and_matches_the_kinetic_energy() {
        let model = model();
        let target = model.profile.takeoff_air_speed_m_s;
        let mass = 450.0 * model.wing_area_m2;
        let acceleration = model.takeoff_acceleration(0.0, mass, target).unwrap();
        assert!(
            acceleration.v2_m_s > 50.0 && acceleration.v2_m_s < target,
            "V2 {} m/s, target {target} m/s",
            acceleration.v2_m_s
        );
        let ledger = acceleration.ledger;
        // Thrust work = drag work + kinetic energy, to round-off, per step.
        let residual = ledger.propulsive_work_j - ledger.drag_work_j - ledger.kinetic_energy_j;
        assert!(
            residual.abs() <= 1.0e-9 * ledger.propulsive_work_j,
            "residual {residual} J of {} J",
            ledger.propulsive_work_j
        );
        // The kinetic energy is m (V_t^2 - V_2^2) / 2 at a mass between the
        // lift-off and the end-of-acceleration masses.
        let delta_j = 0.5 * (target * target - acceleration.v2_m_s * acceleration.v2_m_s);
        let low = (mass - acceleration.fuel_kg) * delta_j;
        let high = mass * delta_j;
        assert!(ledger.kinetic_energy_j >= low && ledger.kinetic_energy_j <= high);
        // Order of magnitude of the charge: lane C1 estimated 0.4 kg/t from
        // the kinetic energy alone (30 kg at 70 t, 150 kg at 400 t); the
        // drag work of the take-off polar roughly doubles it here.
        let kg_per_tonne = acceleration.fuel_kg / (mass / 1000.0);
        assert!(
            kg_per_tonne > 0.2 && kg_per_tonne < 1.5,
            "{kg_per_tonne} kg/t"
        );
        assert!(acceleration.time_s > 10.0 && acceleration.time_s < 90.0);
        assert!(acceleration.distance_m > 1_000.0 && acceleration.distance_m < 8_000.0);
        // Second law: the thrust work cannot exceed the fuel's heat
        // (Jet-A lower heating value 43 MJ/kg), and the overall efficiency
        // of a take-off-rating acceleration is of order 0.1-0.35.
        let heat_j = acceleration.fuel_kg * 43.0e6;
        let efficiency = ledger.propulsive_work_j / heat_j;
        assert!(efficiency > 0.05 && efficiency < 0.5, "{efficiency}");
    }

    #[test]
    fn nothing_is_charged_when_the_safety_speed_is_not_below_the_climb_speed() {
        let model = model();
        let v2 = model
            .takeoff_safety_speed_m_s(0.0, 450.0 * model.wing_area_m2)
            .unwrap();
        let none = model
            .takeoff_acceleration(0.0, 450.0 * model.wing_area_m2, v2)
            .unwrap();
        assert_eq!(none.fuel_kg, 0.0);
        assert_eq!(none.ledger, EnergyLedger::default());
    }

    #[test]
    fn a_heavier_aeroplane_pays_more_for_the_same_climb_speed() {
        let model = model();
        let target = model.profile.takeoff_air_speed_m_s;
        let light = model
            .takeoff_acceleration(0.0, 350.0 * model.wing_area_m2, target)
            .unwrap();
        let heavy = model
            .takeoff_acceleration(0.0, 550.0 * model.wing_area_m2, target)
            .unwrap();
        assert!(heavy.v2_m_s > light.v2_m_s);
        assert!(heavy.fuel_kg > 0.0 && light.fuel_kg > 0.0);
        assert!(heavy.ledger.kinetic_energy_j > light.ledger.kinetic_energy_j);
    }
}
