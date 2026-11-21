// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Level accelerations and decelerations of the route model: the speed change at the 10,000 ft
//! limit and the level fallback of a climb entry (see `takeoff_acceleration` for the climb).

use alas_mass::fuel_plan::FuelModelError;
use alas_prop::system::PropulsionRating;

use super::takeoff_acceleration::{Acceleration, SPEED_STEPS};
use super::SegmentMissionModel;

impl SegmentMissionModel {
    /// Accelerate level from `plan.from_m_s` to `plan.to_m_s` at `plan.rating`
    /// (the energy method: each of [`SPEED_STEPS`] equal speed increments
    /// obeys `(T - D) V dt = m V dV`).
    ///
    /// # Errors
    ///
    /// A propulsion or polar failure, or a thrust that cannot accelerate the
    /// aeroplane (`T <= D`) at some speed of the span.
    pub(super) fn level_acceleration(
        &self,
        mass_kg: f64,
        plan: LevelAcceleration,
    ) -> Result<Acceleration, FuelModelError> {
        let LevelAcceleration {
            altitude_m: screen_m,
            from_m_s: v2_m_s,
            to_m_s: target_tas_m_s,
            rating,
            delta_cd,
            high_lift,
        } = plan;
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
            let rated_point = self
                .propulsion
                .rated_point(flight, rating)
                .map_err(super::deck_error)?;
            let net_n = |at_kg: f64| -> Result<(f64, f64), FuelModelError> {
                let (_, drag_n) = self.drag_components_n(
                    &flight,
                    at_kg,
                    delta_cd,
                    high_lift.then(|| self.phase_limits.takeoff_lift_shift()),
                )?;
                Ok((drag_n, rated_point.thrust_n - drag_n))
            };
            // Midpoint-mass predictor, as the profile integrator does.
            let (_, net0_n) = net_n(mass)?;
            if net0_n <= 0.0 {
                return Err(FuelModelError::NotConverged(format!(
                    "level acceleration thrust deficit: {:.0} N cannot accelerate the aeroplane at {mid:.1} m/s \
                     (net force {net0_n:.0} N)",
                    rated_point.thrust_n
                )));
            }
            let predicted_burn_kg = rated_point.fuel_flow_kg_s * mass * dv_m_s / net0_n;
            let mid_mass = mass - 0.5 * predicted_burn_kg;
            let (drag_n, net_n) = net_n(mid_mass)?;
            if net_n <= 0.0 {
                return Err(FuelModelError::NotConverged(format!(
                    "level acceleration thrust deficit: cannot accelerate the aeroplane at {mid:.1} m/s"
                )));
            }
            // A transport does not accelerate level faster than BADA's civil limit (`a_l,max`,
            // the limit the native flight's level speed changes respect): above it the engine
            // runs at the part-power point that gives exactly that acceleration, on the deck's
            // own normalized-force line (the point the native flight books), and the change
            // takes the longer time that implies.
            let limit_n = mid_mass * alas_config::mission::LEVEL_ACCELERATION_LIMIT_M_S2;
            let (point, net_n) = if net_n > limit_n {
                let throttled = self
                    .propulsion
                    .at_thrust_between(flight, drag_n + limit_n, rating, None, Some(rated_point))
                    .map_err(super::deck_error)?;
                // The net force of the point the deck returned (it asked for the limit).
                let net = throttled.thrust_n - drag_n;
                (throttled, net)
            } else {
                (rated_point, net_n)
            };
            let dt_s = mid_mass * dv_m_s / net_n;
            let burn_kg = point.fuel_flow_kg_s * dt_s;
            if !burn_kg.is_finite() || burn_kg < 0.0 || burn_kg >= mass {
                return Err(FuelModelError::NotConverged(format!(
                    "level acceleration burn {burn_kg} kg is unusable at {mass} kg"
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

    /// Decelerate level, clean, from `from_m_s` to `to_m_s` (below it) at `altitude_m` with the
    /// engine at flight idle, by the energy method (`(T - D) V dt = m V dV`, `T - D < 0`): the
    /// deck's own idle thrust and idle fuel flow, the point the native flight's idle-floored
    /// deceleration books. The ledger's kinetic entry is negative (energy given up).
    ///
    /// `None` when idle thrust does not exceed drag at some speed of the span: the aeroplane
    /// cannot lose the speed level at idle, and the change is left to the descent rung (the
    /// kinetic budget of the rung that follows).
    ///
    ///
    /// # Errors
    ///
    /// A propulsion or polar failure.
    pub(super) fn level_deceleration(
        &self,
        mass_kg: f64,
        altitude_m: f64,
        from_m_s: f64,
        to_m_s: f64,
    ) -> Result<Option<Acceleration>, FuelModelError> {
        let dv_m_s = (from_m_s - to_m_s) / SPEED_STEPS as f64;
        let mut out = Acceleration {
            v2_m_s: from_m_s,
            ..Acceleration::default()
        };
        let mut mass = mass_kg;
        for index in 0..SPEED_STEPS {
            let (high, low) = (
                from_m_s - dv_m_s * index as f64,
                from_m_s - dv_m_s * (index + 1) as f64,
            );
            let mid = 0.5 * (low + high);
            let flight = self
                .propulsion
                .flight_condition(
                    altitude_m,
                    mid,
                    self.gravity_at(altitude_m),
                    self.isa_deviation_c,
                )
                .map_err(super::deck_error)?;
            let idle = self
                .propulsion
                .idle_point(flight)
                .map_err(super::deck_error)?;
            let (_, drag_n) = self.drag_components_n(&flight, mass, 0.0, None)?;
            let retarding_n = drag_n - idle.thrust_n;
            if retarding_n <= 0.0 {
                return Ok(None);
            }
            // BADA bounds the longitudinal acceleration in either sense (User Manual 3.8, printed
            // p. 33, eq. 5.2-1): above the limit the engine delivers the thrust that gives exactly
            // that deceleration, and the change takes the longer time. The fuel is the idle flow
            // (a lower bound: the deck's part-power fuel below idle is a PW127M deck defect,
            // and folding it in made the turboprop sizing closure oscillate).
            let limit_n = mass * alas_config::mission::LEVEL_ACCELERATION_LIMIT_M_S2;
            let (thrust_n, retarding_n) = if retarding_n > limit_n {
                (drag_n - limit_n, limit_n)
            } else {
                (idle.thrust_n, retarding_n)
            };
            let dt_s = mass * dv_m_s / retarding_n;
            let burn_kg = idle.fuel_flow_kg_s * dt_s;
            if !burn_kg.is_finite() || burn_kg < 0.0 || burn_kg >= mass {
                return Err(FuelModelError::NotConverged(format!(
                    "level deceleration burn {burn_kg} kg is unusable at {mass} kg"
                )));
            }
            out.ledger.propulsive_work_j += thrust_n * mid * dt_s;
            out.ledger.drag_work_j += drag_n * mid * dt_s;
            out.ledger.kinetic_energy_j += 0.5 * mass * (low * low - high * high);
            out.fuel_kg += burn_kg;
            out.time_s += dt_s;
            out.distance_m += mid * dt_s;
            mass -= burn_kg;
        }
        Ok(Some(out))
    }
}

/// A level acceleration to charge: where, between which true airspeeds, at
/// which rating and in which aerodynamic configuration.
#[derive(Debug, Clone, Copy)]
pub(super) struct LevelAcceleration {
    pub altitude_m: f64,
    pub from_m_s: f64,
    pub to_m_s: f64,
    pub rating: PropulsionRating,
    pub delta_cd: f64,
    pub high_lift: bool,
}
