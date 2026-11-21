// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Fuel, time, distance and height of the take-off climb and of the accelerations along a climb.
//!
//! The planned ladder opens with its take-off band flown at the profile's take-off climb speed,
//! while the aeroplane clears the 35 ft screen at the take-off safety speed `V2` (CS 25.107(c):
//! `V2 >= 1.13 VSR`, with `VSR >= V_S1g`). The kinetic energy between the two,
//! `m (V_band^2 - V2^2) / 2` (about 100 MJ for a 70 t aeroplane at a 77 m/s band), is not in any
//! ladder segment: the segments open a kinetic budget only between one segment and the next.
//!
//! The take-off climb ([`SegmentMissionModel::takeoff_acceleration`]) charges it by the energy
//! method, from the field, at the take-off/go-around rating in take-off configuration (flaps, gear
//! retracted: the second-segment polar the take-off band uses): a constant-speed climb at `V2` to
//! the acceleration height ([`alas_config::mission::TAKEOFF_ACCELERATION_HEIGHT_M`]), then an
//! acceleration along the climb with BADA's energy share for an acceleration in a climb
//! ([`SegmentMissionModel::climbing_acceleration`], `f = 0.3`), which fixes the height gained by
//! the speed change alone. The native flight flies the same two parts (`takeoff_climb` and the
//! take-off band's entry piece), so the thrust work exceeds the drag work and the potential energy
//! by the kinetic energy gained, to round-off, and the burn is the deck's fuel flow at the
//! rating (or at the share of it the planned climb rate asks for) over each step's duration.
//! `V2 = 1.13 V_S1g` is evaluated at the lift-off mass, the departure field density and the
//! take-off maximum lift coefficient; no take-off distance, obstacle or engine-failure case is
//! modelled (the climb is flown with all engines, as the rest of the ladder is), and a diversion
//! go-around is charged by the same relation as the closest available description of its
//! recovery to climb speed.
//!
//! The same acceleration along a climb, at the maximum-climb rating, is the path piece the native
//! junction logic cuts at a speed change along the ladder (see `integrate::climb_entry`); the
//! level accelerations and decelerations ([`SegmentMissionModel::level_acceleration`],
//! [`SegmentMissionModel::level_deceleration`]) are the legs it falls back to or meets the 10,000 ft
//! limit with.

use alas_prop::system::PropulsionRating;

use super::integrate::EnergyLedger;
use super::SegmentMissionModel;
use alas_mass::fuel_plan::FuelModelError;

/// Screen height at which `V2` is reached, 35 ft in metres (CS 25.113).
pub(super) const SCREEN_HEIGHT_M: f64 = 35.0 * alas_units::FOOT;
/// `V2 / V_S1g` (CS 25.107(c)(2): `V2 >= 1.13 VSR`, `VSR >= V_S1g`).
pub(super) const V2_OVER_STALL_SPEED: f64 = 1.13;
/// Equal speed increments of the integration; the thrust and drag change
/// slowly over the 40-60 m/s span, and the energy identity holds exactly for
/// any count.
pub(super) const SPEED_STEPS: usize = 16;
/// Equal height increments of the constant-speed part of the take-off climb.
const CLIMB_STEPS: usize = 8;
/// Corrections of the share of a rating until its force is the required force.
const SHARE_CORRECTIONS: usize = 8;

/// What a climb or an acceleration cost and where it left the aeroplane.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(super) struct Acceleration {
    /// Fuel burned, kg.
    pub fuel_kg: f64,
    /// Time, s.
    pub time_s: f64,
    /// Horizontal distance flown, m.
    pub distance_m: f64,
    /// Thrust work, drag work, potential energy and kinetic energy gained (the potential entry is
    /// zero for a level acceleration).
    pub ledger: EnergyLedger,
    /// Speed it started from, m/s (the take-off safety speed for the take-off climb; zero when no
    /// acceleration was needed).
    pub v2_m_s: f64,
    /// Altitude it ended at, m (zero for a level acceleration, which keeps its altitude).
    pub altitude_end_m: f64,
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

    /// One step of a climb at `altitude`, `speed` and mass `at_kg`, with `f` of the excess power
    /// going into height (1 at constant speed, the energy share while accelerating): the climb
    /// rate is the planned one, or the engine's boundary at `rating` where that is lower, and the
    /// thrust, drag and fuel flow are those of the point that rate asks for. A rate below the
    /// boundary asks a share of the rating, booked on the deck's own rated-fraction line, the
    /// point the native flight books for the same share.
    ///
    /// `(thrust N, drag N, fuel flow kg/s, climb rate m/s)`, or `None` where the rating cannot
    /// climb the aeroplane (`T <= D`).
    fn climb_point(
        &self,
        plan: &ClimbingAcceleration,
        altitude: f64,
        speed: f64,
        at_kg: f64,
        f: f64,
    ) -> Result<Option<(f64, f64, f64, f64)>, FuelModelError> {
        let flight = self
            .propulsion
            .flight_condition(
                altitude,
                speed,
                self.gravity_at(altitude),
                self.isa_deviation_c,
            )
            .map_err(super::deck_error)?;
        let full = self
            .propulsion
            .rated_point(flight, plan.rating)
            .map_err(super::deck_error)?;
        let high_lift = if plan.high_lift {
            self.phase_limits.high_lift_delta_cd
        } else {
            0.0
        };
        let (_, drag_n) = self.drag_components_n(
            &flight,
            at_kg,
            high_lift,
            plan.high_lift
                .then(|| self.phase_limits.takeoff_lift_shift()),
        )?;
        if full.thrust_n <= drag_n {
            return Ok(None);
        }
        let weight = at_kg * self.gravity_at(altitude);
        let boundary = f * (full.thrust_n - drag_n) * speed / weight;
        if plan.planned_rate_m_s >= boundary {
            return Ok(Some((full.thrust_n, drag_n, full.fuel_flow_kg_s, boundary)));
        }
        let required_n = drag_n + weight * plan.planned_rate_m_s / (f * speed);
        // The share of the rating whose force is the required one. The deck's rated-fraction
        // line is not exactly linear (the turboprop's), so the share is corrected until the
        // force it gives is the required force: the native flight's root find does the same, and
        // the leg then climbs at exactly the planned rate.
        let mut share = (required_n / full.thrust_n).clamp(0.0, 1.0);
        let mut point = self
            .propulsion
            .rated_fraction_point(flight, plan.rating, share)
            .map_err(super::deck_error)?;
        for _ in 0..SHARE_CORRECTIONS {
            if (point.thrust_n - required_n).abs() <= 1.0e-6 * required_n {
                break;
            }
            share = (share * required_n / point.thrust_n).clamp(0.0, 1.0);
            point = self
                .propulsion
                .rated_fraction_point(flight, plan.rating, share)
                .map_err(super::deck_error)?;
        }
        let rate = f * (point.thrust_n - drag_n) * speed / weight;
        Ok(Some((point.thrust_n, drag_n, point.fuel_flow_kg_s, rate)))
    }

    /// Accelerate in the climb from `plan.from_m_s` to `plan.to_m_s` with BADA's energy share
    /// (`dh/dt = f (T - D) V / (m g)`, `V dV/dt = (1 - f) (T - D) V / m`, `f = 0.3` for an
    /// acceleration in a climb), so the height gained follows the speed alone
    /// ([`alas_mission::segments::acceleration_climb_altitude_gain_m`]). The climb rate is the
    /// planned one, or the boundary of the rating where that is lower.
    ///
    /// This is the path piece the native flight cuts at a speed change along a climb (a
    /// `*_path_entry` leg) and the second part of the take-off climb.
    ///
    /// `None` when the rating cannot climb the aeroplane at some speed of the span, or when the
    /// rate falls below `plan.minimum_rate_m_s` (the native flight flies such a change level).
    ///
    /// # Errors
    ///
    /// A propulsion or polar failure, or a cancelled evaluation.
    pub(super) fn climbing_acceleration(
        &self,
        mass_kg: f64,
        plan: &ClimbingAcceleration,
    ) -> Result<Option<Acceleration>, FuelModelError> {
        // The native leg climbs at one rate for its whole length (the largest rate every point of
        // it can fly), so the rate is the planned one or the smallest boundary along the span,
        // taken at the nodes of the span (its ends included, where the boundary is smallest).
        let probe = ClimbingAcceleration {
            planned_rate_m_s: f64::INFINITY,
            minimum_rate_m_s: 0.0,
            ..*plan
        };
        let share = alas_mission::segments::energy_share_factor(true, true);
        let steps = SPEED_STEPS / 4 * self.steps_per_segment;
        let nodes: Vec<(f64, f64)> = (0..=steps)
            .map(|index| {
                let speed =
                    plan.from_m_s + (plan.to_m_s - plan.from_m_s) * index as f64 / steps as f64;
                let gain = alas_mission::segments::acceleration_climb_altitude_gain_m(
                    plan.from_m_s,
                    speed,
                );
                let height = plan.altitude_m
                    + gain * alas_atmo::STANDARD_GRAVITY / self.gravity_at(plan.altitude_m);
                (height, speed)
            })
            .collect();
        let Some(boundary_m_s) = self.slowest_climb_rate(&probe, mass_kg, &nodes, share)? else {
            return Ok(None);
        };
        let constant = ClimbingAcceleration {
            planned_rate_m_s: plan.planned_rate_m_s.min(boundary_m_s),
            ..*plan
        };
        Ok(self
            .climbing_pass(mass_kg, &constant)?
            .map(|(acceleration, _)| acceleration))
    }

    /// The smallest climb rate the rating of `probe` gives at the `(altitude, speed)` nodes with
    /// `f` of the excess power going into height, the mass falling by what the climb between
    /// nodes burns; `None` where the rating cannot climb the aeroplane at some node.
    fn slowest_climb_rate(
        &self,
        probe: &ClimbingAcceleration,
        mass_kg: f64,
        nodes: &[(f64, f64)],
        f: f64,
    ) -> Result<Option<f64>, FuelModelError> {
        let mut mass = mass_kg;
        let mut slowest = f64::INFINITY;
        for (index, &(altitude, speed)) in nodes.iter().enumerate() {
            self.check_cancelled()?;
            let Some((_, _, flow, rate)) = self.climb_point(probe, altitude, speed, mass, f)?
            else {
                return Ok(None);
            };
            slowest = slowest.min(rate);
            if let Some(&(next, _)) = nodes.get(index + 1) {
                mass -= flow * (next - altitude).max(0.0) / rate;
            }
        }
        Ok(Some(slowest))
    }

    /// One pass over the span of a climbing acceleration at the rate `plan` asks for: what it
    /// costs and the smallest climb rate any of its steps flew.
    fn climbing_pass(
        &self,
        mass_kg: f64,
        plan: &ClimbingAcceleration,
    ) -> Result<Option<(Acceleration, f64)>, FuelModelError> {
        let mut slowest_m_s = f64::INFINITY;
        let share = alas_mission::segments::energy_share_factor(true, true);
        let speed_steps = SPEED_STEPS / 4 * self.steps_per_segment;
        let dv_m_s = (plan.to_m_s - plan.from_m_s) / speed_steps as f64;
        let mut out = Acceleration {
            v2_m_s: plan.from_m_s,
            ..Acceleration::default()
        };
        let mut mass = mass_kg;
        let mut height_m = plan.altitude_m;
        for index in 0..speed_steps {
            self.check_cancelled()?;
            let (low, high) = (
                plan.from_m_s + dv_m_s * index as f64,
                plan.from_m_s + dv_m_s * (index + 1) as f64,
            );
            let mid = 0.5 * (low + high);
            // BADA's relation is written with standard gravity; with the local one, taken at the
            // middle of the step, the height follows by the ratio, which keeps the energy
            // identity exact.
            let gain = alas_mission::segments::acceleration_climb_altitude_gain_m(low, high);
            let first_guess = gain * alas_atmo::STANDARD_GRAVITY / self.gravity_at(height_m);
            let altitude = height_m + 0.5 * first_guess;
            let dh = gain * alas_atmo::STANDARD_GRAVITY / self.gravity_at(altitude);
            // Midpoint-mass predictor, as the profile integrator does.
            let Some((_, _, flow0, rate0)) = self.climb_point(plan, altitude, mid, mass, share)?
            else {
                return Ok(None);
            };
            let mid_mass = mass - 0.5 * flow0 * dh / rate0;
            let Some((thrust, drag, flow, rate)) =
                self.climb_point(plan, altitude, mid, mid_mass, share)?
            else {
                return Ok(None);
            };
            if rate < plan.minimum_rate_m_s {
                return Ok(None);
            }
            slowest_m_s = slowest_m_s.min(rate);
            let weight = mid_mass * self.gravity_at(altitude);
            let dt_s = dh / rate;
            let burn_kg = flow * dt_s;
            out.ledger.propulsive_work_j += thrust * mid * dt_s;
            out.ledger.drag_work_j += drag * mid * dt_s;
            out.ledger.potential_energy_j += weight * dh;
            out.ledger.kinetic_energy_j += 0.5 * mid_mass * (high * high - low * low);
            out.fuel_kg += burn_kg;
            out.time_s += dt_s;
            out.distance_m += (mid * mid - rate * rate).max(0.0).sqrt() * dt_s;
            height_m += dh;
            mass -= burn_kg;
        }
        out.altitude_end_m = height_m;
        Ok(Some((out, slowest_m_s)))
    }

    /// The take-off climb from the field to the climb-out speed `target_tas_m_s`: a constant-speed
    /// climb at the take-off safety speed `V2` at the take-off rating from the field to the
    /// acceleration height ([`alas_config::mission::TAKEOFF_ACCELERATION_HEIGHT_M`]), then the
    /// acceleration along the climb to the target ([`Self::climbing_acceleration`], take-off
    /// rating, take-off configuration). Zero when `V2` is not below the target.
    ///
    /// The native flight's `takeoff_climb` leg and its take-off band (whose entry speed is `V2`)
    /// are the same two parts.
    ///
    /// # Errors
    ///
    /// A propulsion or polar failure, or a thrust that cannot climb the aeroplane (`T <= D`).
    pub(super) fn takeoff_acceleration(
        &self,
        altitude_m: f64,
        mass_kg: f64,
        target_tas_m_s: f64,
        planned_rate_m_s: f64,
    ) -> Result<Acceleration, FuelModelError> {
        if !mass_kg.is_finite() || mass_kg <= 0.0 {
            return Err(FuelModelError::MassOutOfRange { mass_kg });
        }
        let v2_m_s = self.takeoff_safety_speed_m_s(altitude_m, mass_kg)?;
        // A change not above the minimum speed change is left to the band's own kinetic budget,
        // as the native schedule leaves it (it cuts neither the climb at V2 nor an acceleration
        // piece there, `schedule::split_takeoff_climb`'s caller): the two models fly one leg.
        if !target_tas_m_s.is_finite()
            || target_tas_m_s <= v2_m_s + alas_config::mission::MINIMUM_SPEED_CHANGE_M_S
        {
            return Ok(Acceleration::default());
        }
        let acceleration_altitude_m =
            altitude_m + alas_config::mission::TAKEOFF_ACCELERATION_HEIGHT_M;
        let deficit = |speed: f64, altitude: f64| {
            FuelModelError::NotConverged(format!(
                "take-off climb thrust deficit: the take-off rating cannot climb the aeroplane at {speed:.1} m/s and {altitude:.0} m"
            ))
        };
        let plan = ClimbingAcceleration {
            altitude_m: acceleration_altitude_m,
            from_m_s: v2_m_s,
            to_m_s: target_tas_m_s,
            rating: PropulsionRating::TakeoffGoAround,
            high_lift: true,
            planned_rate_m_s,
            minimum_rate_m_s: 0.0,
        };
        let mut out = Acceleration {
            v2_m_s,
            ..Acceleration::default()
        };
        let mut mass = mass_kg;
        // A: constant-speed climb at V2 from the field to the acceleration height, all the power
        // going into height: `Vz = (T - D) V / (m g)`. The steps refine with the integrator's own
        // (the midpoint rule converges with them).
        let climb_steps = CLIMB_STEPS / 4 * self.steps_per_segment;
        let dh_a = (acceleration_altitude_m - altitude_m) / climb_steps as f64;
        // The native leg climbs at one rate for its whole length: the planned one, or the largest
        // rate every point of the span can fly.
        let slowest_m_s = {
            let probe = ClimbingAcceleration {
                planned_rate_m_s: f64::INFINITY,
                ..plan
            };
            let nodes: Vec<(f64, f64)> = (0..=climb_steps)
                .map(|index| (altitude_m + dh_a * index as f64, v2_m_s))
                .collect();
            self.slowest_climb_rate(&probe, mass, &nodes, 1.0)?
                .ok_or_else(|| deficit(v2_m_s, altitude_m))?
        };
        let plan_a = ClimbingAcceleration {
            planned_rate_m_s: plan.planned_rate_m_s.min(slowest_m_s),
            ..plan
        };
        for index in 0..climb_steps {
            self.check_cancelled()?;
            let altitude = altitude_m + dh_a * (index as f64 + 0.5);
            // Midpoint-mass predictor, as the profile integrator does.
            let (_, _, flow0, rate0) = self
                .climb_point(&plan_a, altitude, v2_m_s, mass, 1.0)?
                .ok_or_else(|| deficit(v2_m_s, altitude))?;
            let mid_mass = mass - 0.5 * flow0 * dh_a / rate0;
            let (thrust, drag, flow, rate) = self
                .climb_point(&plan_a, altitude, v2_m_s, mid_mass, 1.0)?
                .ok_or_else(|| deficit(v2_m_s, altitude))?;
            let weight = mid_mass * self.gravity_at(altitude);
            let dt_s = dh_a / rate;
            let burn_kg = flow * dt_s;
            out.ledger.propulsive_work_j += thrust * v2_m_s * dt_s;
            out.ledger.drag_work_j += drag * v2_m_s * dt_s;
            out.ledger.potential_energy_j += weight * dh_a;
            out.fuel_kg += burn_kg;
            out.time_s += dt_s;
            out.distance_m += (v2_m_s * v2_m_s - rate * rate).max(0.0).sqrt() * dt_s;
            mass -= burn_kg;
        }
        // B: acceleration in the climb to the climb-out speed.
        let accelerated = self
            .climbing_acceleration(mass, &plan)?
            .ok_or_else(|| deficit(0.5 * (v2_m_s + target_tas_m_s), acceleration_altitude_m))?;
        out.ledger.propulsive_work_j += accelerated.ledger.propulsive_work_j;
        out.ledger.drag_work_j += accelerated.ledger.drag_work_j;
        out.ledger.potential_energy_j += accelerated.ledger.potential_energy_j;
        out.ledger.kinetic_energy_j += accelerated.ledger.kinetic_energy_j;
        out.fuel_kg += accelerated.fuel_kg;
        out.time_s += accelerated.time_s;
        out.distance_m += accelerated.distance_m;
        out.altitude_end_m = accelerated.altitude_end_m;
        Ok(out)
    }
}

/// An acceleration in a climb to charge: where it starts, between which true airspeeds, at which
/// rating and in which configuration, and the climb rate asked for.
#[derive(Debug, Clone, Copy)]
pub(super) struct ClimbingAcceleration {
    pub altitude_m: f64,
    pub from_m_s: f64,
    pub to_m_s: f64,
    pub rating: PropulsionRating,
    pub high_lift: bool,
    pub planned_rate_m_s: f64,
    /// A climb rate below this cannot fly the change (the change is flown level instead).
    pub minimum_rate_m_s: f64,
}

// These tests use unwrap/expect for controlled fixtures; a failure is the
// assertion with its context rather than a production error path.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::mdo::build::build_geometry;
    use crate::mdo::mission_model::level_acceleration::LevelAcceleration;
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
        let acceleration = model.takeoff_acceleration(0.0, mass, target, 6.0).unwrap();
        assert!(
            acceleration.v2_m_s > 50.0 && acceleration.v2_m_s < target,
            "V2 {} m/s, target {target} m/s",
            acceleration.v2_m_s
        );
        let ledger = acceleration.ledger;
        // Thrust work = drag work + potential + kinetic energy, to the mass-update order of the
        // step (the height gain carries the local gravity, so the identity is exact per step).
        let residual = ledger.propulsive_work_j
            - ledger.drag_work_j
            - ledger.potential_energy_j
            - ledger.kinetic_energy_j;
        assert!(
            residual.abs() <= 1.0e-6 * ledger.propulsive_work_j,
            "residual {residual} J of {} J",
            ledger.propulsive_work_j
        );
        // It ends at the acceleration height plus the height the energy share allows.
        let gain =
            alas_mission::segments::acceleration_climb_altitude_gain_m(acceleration.v2_m_s, target);
        assert!(
            (acceleration.altitude_end_m
                - (alas_config::mission::TAKEOFF_ACCELERATION_HEIGHT_M + gain))
                .abs()
                < 0.5
        );
        // The potential energy is the weight times the height climbed from the field.
        let climbed = acceleration.altitude_end_m;
        let weight_lo = (mass - acceleration.fuel_kg) * 9.81 * climbed;
        let weight_hi = mass * 9.81 * climbed;
        assert!(
            ledger.potential_energy_j >= 0.99 * weight_lo
                && ledger.potential_energy_j <= 1.01 * weight_hi
        );
        // The kinetic energy is m (V_t^2 - V_2^2) / 2 at a mass between the
        // lift-off and the end-of-acceleration masses.
        let delta_j = 0.5 * (target * target - acceleration.v2_m_s * acceleration.v2_m_s);
        let low = (mass - acceleration.fuel_kg) * delta_j;
        let high = mass * delta_j;
        assert!(ledger.kinetic_energy_j >= low && ledger.kinetic_energy_j <= high);
        // Order of magnitude of the charge: 70 s or so of take-off thrust.
        let kg_per_tonne = acceleration.fuel_kg / (mass / 1000.0);
        assert!(
            kg_per_tonne > 0.3 && kg_per_tonne < 6.0,
            "{kg_per_tonne} kg/t"
        );
        assert!(acceleration.time_s > 20.0 && acceleration.time_s < 200.0);
        assert!(acceleration.distance_m > 2_000.0 && acceleration.distance_m < 15_000.0);
        // Second law: the thrust work cannot exceed the fuel's heat
        // (Jet-A lower heating value 43 MJ/kg), and the overall efficiency
        // of a take-off-rating acceleration is of order 0.1-0.35.
        let heat_j = acceleration.fuel_kg * 43.0e6;
        let efficiency = ledger.propulsive_work_j / heat_j;
        assert!(efficiency > 0.05 && efficiency < 0.5, "{efficiency}");
    }

    /// The level deceleration at idle gives up kinetic energy to the drag, to round-off; it burns the
    /// deck's idle fuel flow (so fuel over time lies between the idle flows at its two end
    /// speeds).
    #[test]
    fn the_level_deceleration_at_idle_closes_its_energy_and_burns_idle_fuel() {
        let model = model();
        let altitude_m = alas_config::mission::SPEED_RESTRICTION_ALTITUDE_M;
        let mass = 450.0 * model.wing_area_m2;
        let (from, to) = (150.0, 128.0);
        let out = model
            .level_deceleration(mass, altitude_m, from, to)
            .unwrap()
            .expect("idle decelerates a 70 t aeroplane at 10,000 ft");
        let ledger = out.ledger;
        assert!(ledger.kinetic_energy_j < 0.0);
        let residual = ledger.propulsive_work_j - ledger.drag_work_j - ledger.kinetic_energy_j;
        assert!(
            residual.abs() <= 1.0e-9 * ledger.drag_work_j.abs(),
            "residual {residual} J of {} J",
            ledger.drag_work_j
        );
        let idle_flow = |speed: f64| {
            let flight = model
                .propulsion
                .flight_condition(altitude_m, speed, model.gravity_at(altitude_m), 0.0)
                .unwrap();
            model.propulsion.idle_point(flight).unwrap().fuel_flow_kg_s
        };
        let mean_flow = out.fuel_kg / out.time_s;
        let (lo, hi) = (
            idle_flow(from).min(idle_flow(to)),
            idle_flow(from).max(idle_flow(to)),
        );
        assert!(
            mean_flow >= lo * (1.0 - 1.0e-6) && mean_flow <= hi * (1.0 + 1.0e-6),
            "{mean_flow} kg/s outside the idle flows {lo}..{hi}"
        );
    }

    /// Above BADA's longitudinal limit the deceleration takes the longer time the limit asks for
    /// (never less than dV / a_max), and its energy still closes.
    #[test]
    fn a_level_deceleration_above_the_civil_limit_is_slowed_to_it_and_closes_its_energy() {
        let model = model();
        let altitude_m = alas_config::mission::SPEED_RESTRICTION_ALTITUDE_M;
        let mass = 40.0 * model.wing_area_m2;
        let (from, to) = (150.0, 128.0);
        let out = model
            .level_deceleration(mass, altitude_m, from, to)
            .unwrap()
            .expect("idle decelerates a light aeroplane at 10,000 ft");
        let limit = alas_config::mission::LEVEL_ACCELERATION_LIMIT_M_S2;
        assert!(
            out.time_s >= (from - to) / limit * (1.0 - 1.0e-9),
            "{} s against {} s at the limit",
            out.time_s,
            (from - to) / limit
        );
        let ledger = out.ledger;
        let residual = ledger.propulsive_work_j - ledger.drag_work_j - ledger.kinetic_energy_j;
        assert!(residual.abs() <= 1.0e-9 * ledger.drag_work_j.abs());
    }

    /// Where idle thrust exceeds drag (a slow, light aeroplane) the aeroplane
    /// cannot lose the speed level: the deceleration is `None` and is left to the descent rung
    /// (the `Ok(None)` path of `decelerate_through_restriction`, tested in `integrate`), while a heavy
    /// aeroplane at transport speeds decelerates.
    #[test]
    fn a_level_deceleration_idle_cannot_fly_is_left_to_the_descent_rung() {
        let model = model();
        let altitude_m = alas_config::mission::SPEED_RESTRICTION_ALTITUDE_M;
        let heavy = 450.0 * model.wing_area_m2;
        // Idle thrust falls with speed and drag rises with its square, so a slow, light aeroplane
        // is the case idle cannot retard.
        let none_for = |load: f64, from: f64, to: f64| {
            model
                .level_deceleration(load * model.wing_area_m2, altitude_m, from, to)
                .unwrap()
                .is_none()
        };
        assert!(
            none_for(1.0, 60.0, 45.0),
            "a light aeroplane at low speed cannot retard at idle"
        );
        assert!(model
            .level_deceleration(heavy, altitude_m, 150.0, 128.0)
            .unwrap()
            .is_some());
    }

    /// An acceleration along a climb (the path piece at a speed change, the second part of the
    /// take-off climb) gains the height the BADA energy share gives, climbs at one rate that no
    /// point of it exceeds the boundary of, and closes its energy.
    #[test]
    fn a_climbing_acceleration_gains_the_energy_share_height_at_one_rate_and_closes_its_energy() {
        let model = model();
        let altitude_m = alas_config::mission::SPEED_RESTRICTION_ALTITUDE_M;
        let mass = 450.0 * model.wing_area_m2;
        let plan = ClimbingAcceleration {
            altitude_m,
            from_m_s: 148.0,
            to_m_s: 180.0,
            rating: PropulsionRating::MaximumClimb,
            high_lift: false,
            planned_rate_m_s: 50.0,
            minimum_rate_m_s: 0.0,
        };
        let out = model
            .climbing_acceleration(mass, &plan)
            .unwrap()
            .expect("the climb accelerates");
        let gain = alas_mission::segments::acceleration_climb_altitude_gain_m(148.0, 180.0);
        assert!((out.altitude_end_m - altitude_m - gain).abs() < 0.5);
        let ledger = out.ledger;
        let residual = ledger.propulsive_work_j
            - ledger.drag_work_j
            - ledger.potential_energy_j
            - ledger.kinetic_energy_j;
        assert!(
            residual.abs() <= 1.0e-6 * ledger.propulsive_work_j,
            "residual {residual} J of {} J",
            ledger.propulsive_work_j
        );
        // One rate for the whole span: the height over the time is below the boundary of the
        // rating at both ends (the planned 50 m/s is far above it).
        let rate = (out.altitude_end_m - altitude_m) / out.time_s;
        let boundary_at = |altitude: f64, speed: f64, at_kg: f64| {
            let probe = ClimbingAcceleration {
                planned_rate_m_s: f64::INFINITY,
                ..plan
            };
            model
                .climb_point(&probe, altitude, speed, at_kg, 0.3)
                .unwrap()
                .unwrap()
                .3
        };
        assert!(rate <= boundary_at(altitude_m, 148.0, mass) * 1.001);
        assert!(rate <= boundary_at(out.altitude_end_m, 180.0, mass - out.fuel_kg) * 1.001);
        // A change the engine can only fly slower than the minimum rate is not a climbing one.
        let slow = ClimbingAcceleration {
            minimum_rate_m_s: 1.0e3,
            ..plan
        };
        assert!(model.climbing_acceleration(mass, &slow).unwrap().is_none());
    }

    /// The acceleration at the 10,000 ft restriction respects the civil level-acceleration limit
    /// the native flight's level speed changes respect (it takes at least dV / a_max), and its
    /// energy closes.
    #[test]
    fn the_restriction_acceleration_respects_the_civil_limit_like_the_native_flight() {
        let model = model();
        let altitude_m = alas_config::mission::SPEED_RESTRICTION_ALTITUDE_M;
        let mass = 450.0 * model.wing_area_m2;
        let (from, to) = (128.0, 150.0);
        let out = model
            .level_acceleration(
                mass,
                LevelAcceleration {
                    altitude_m,
                    from_m_s: from,
                    to_m_s: to,
                    rating: PropulsionRating::MaximumClimb,
                    delta_cd: 0.0,
                    high_lift: false,
                },
            )
            .unwrap();
        let shortest = (to - from) / alas_config::mission::LEVEL_ACCELERATION_LIMIT_M_S2;
        assert!(
            out.time_s >= shortest * (1.0 - 1.0e-9),
            "{} s against the {shortest} s the limit allows",
            out.time_s
        );
        let ledger = out.ledger;
        let residual = ledger.propulsive_work_j - ledger.drag_work_j - ledger.kinetic_energy_j;
        assert!(residual.abs() <= 1.0e-6 * ledger.propulsive_work_j);
    }

    #[test]
    fn nothing_is_charged_when_the_safety_speed_is_not_below_the_climb_speed() {
        let model = model();
        let v2 = model
            .takeoff_safety_speed_m_s(0.0, 450.0 * model.wing_area_m2)
            .unwrap();
        let none = model
            .takeoff_acceleration(0.0, 450.0 * model.wing_area_m2, v2, 6.0)
            .unwrap();
        assert_eq!(none.fuel_kg, 0.0);
        assert_eq!(none.ledger, EnergyLedger::default());
    }

    #[test]
    fn a_heavier_aeroplane_pays_more_for_the_same_climb_speed() {
        let model = model();
        let target = model.profile.takeoff_air_speed_m_s;
        let light = model
            .takeoff_acceleration(0.0, 350.0 * model.wing_area_m2, target, 6.0)
            .unwrap();
        let heavy = model
            .takeoff_acceleration(0.0, 550.0 * model.wing_area_m2, target, 6.0)
            .unwrap();
        assert!(heavy.v2_m_s > light.v2_m_s);
        assert!(heavy.fuel_kg > 0.0 && light.fuel_kg > 0.0);
        assert!(heavy.ledger.kinetic_energy_j > light.ledger.kinetic_energy_j);
    }
}
