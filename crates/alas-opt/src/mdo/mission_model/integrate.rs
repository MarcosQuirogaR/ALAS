// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Quasi-steady integration of a planned profile against the propulsion
//! deck, with an energy ledger that makes the bookkeeping checkable.
//!
//! Every step balances `(T - D) V dt = W dh + dKE`. The planned rate fixes
//! `dh` per step; the speed change at a segment boundary opens a
//! kinetic-energy budget `dKE_pending = m (V^2 - V_prev^2) / 2` that the
//! following steps pay off as fast as the deck allows. When the thrust that
//! pays the whole budget in the planned step time lies between the flight
//! idle floor and the phase rating, the step flies the planned rate at the
//! solved throttle and clears the budget. Otherwise the deck sets the thrust
//! at the bound, the planned rate keeps priority, and only the remaining
//! excess power goes to the budget; the unpaid remainder carries into the
//! next step. When the bound cannot even sustain the planned rate, the step
//! solves the combined energy equation for the time needed to cover both the
//! altitude increment and the pending kinetic budget (a rating-limited climb
//! is slower, an idle-floored descent is shallower). A step that cannot
//! balance energy at all (drag above the rating in a climb, idle thrust above
//! drag in a descent, or a rate below the service-ceiling criterion) is a
//! typed failure with the numbers, never a balancing term.
//! The ledger identity `propulsive = drag + potential + kinetic` therefore
//! holds to round-off by construction; a budget still open at the end of the
//! leg is reported as unrealized kinetic energy and bounded. A following
//! segment may continue paying the same target from the reconstructed actual
//! speed, but a material unpaid transition must close before a distinct
//! target is opened. This prevents an intermediate target from being reported
//! as flown when the aircraft never attained it.
//!
//! Mass is advanced with a midpoint predictor (the flow at the step's start
//! mass predicts the mid-step mass, at which drag and thrust are re-solved),
//! so the integration is second order in the smooth parts of the profile.
//!
//! The polar is the candidate's trimmed cruise polar. Clean phases are only
//! evaluated up to the configured clean lift limit; takeoff and landing
//! phases are evaluated up to the configured high-lift limits with the
//! configured high-lift drag increment. A step outside its limit is a
//! validity failure, not a drag extrapolation.

use alas_mass::fuel_plan::{FuelModelError, LegEstimate};
use alas_prop::system::{FlightCondition, PropulsionRating};

use super::profile::{ProfilePlan, Segment, SegmentKind};
use super::SegmentMissionModel;
use crate::mdo::propulsion::{DeckError, OperatingPoint, ThrustLimit};

/// Default midpoint steps per planned segment; the model can refine it.
pub(crate) const DEFAULT_STEPS_PER_SEGMENT: usize = 4;
/// Service-ceiling criterion, 100 ft/min, m/s: a rating-limited climb step
/// slower than this is a thrust deficit, not a slow climb.
const MINIMUM_CLIMB_RATE_M_S: f64 = 100.0 * 0.3048 / 60.0;
/// Route-closure tolerance on the cruise distance, m.
const CRUISE_DISTANCE_TOLERANCE_M: f64 = 1.0;
/// Fixed-point passes on the flown descent footprint.
const DESCENT_FOOTPRINT_PASSES: usize = 6;
/// Largest kinetic-energy budget that may remain open at the end of a leg,
/// as a fraction of the leg's propulsive work.
const UNREALIZED_KINETIC_FRACTION: f64 = 1.0e-3;
/// Absolute floor for a boundary kinetic-energy closure check, J. This only
/// absorbs round-off when the aircraft kinetic-energy scale is very small.
const BOUNDARY_KE_ABSOLUTE_TOLERANCE_J: f64 = 1.0e-3;
/// Relative boundary kinetic-energy closure tolerance against the aircraft's
/// current kinetic-energy scale. Unlike the end-of-leg diagnostic above, this
/// does not grow with total propulsive work over a long route.
const BOUNDARY_KE_RELATIVE_TOLERANCE: f64 = 1.0e-9;

/// Mechanical energy accounting over a flown leg, J.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EnergyLedger {
    /// Work done by installed thrust, `integral(T V dt)`.
    pub propulsive_work_j: f64,
    /// Work against drag, `integral(D V dt)`.
    pub drag_work_j: f64,
    /// Potential-energy change, `integral(W Vz dt)`.
    pub potential_energy_j: f64,
    /// Kinetic-energy change actually delivered by thrust.
    pub kinetic_energy_j: f64,
    /// Kinetic-energy budget still open at the end of the leg.
    pub unrealized_kinetic_j: f64,
}

/// A flown leg with its profile diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct FlownLeg {
    /// Fuel and time.
    pub leg: LegEstimate,
    /// Mass at the end of the leg, kg.
    pub end_mass_kg: f64,
    /// Cruise altitude actually flown, m.
    pub cruise_altitude_m: f64,
    /// Whether the cruise altitude was lowered to fit the route.
    pub adapted: bool,
    /// Steps flown at the rating or the idle floor rather than at the
    /// solved throttle.
    pub rate_limited_steps: usize,
    /// Slowest climb rate flown, m/s.
    pub minimum_climb_rate_m_s: f64,
    /// Shallowest descent rate flown, m/s (positive magnitude).
    pub minimum_descent_rate_m_s: f64,
    /// Highest lift coefficient flown in a clean phase.
    pub maximum_clean_cl: f64,
    /// Horizontal distance flown in the climb ladder, m.
    pub climb_footprint_m: f64,
    /// Horizontal distance flown in the descent ladder, m.
    pub descent_footprint_m: f64,
    /// Horizontal distance integrated over every step, m; closes on the
    /// requested route to the cruise-distance tolerance.
    pub flown_distance_m: f64,
    /// Energy accounting.
    pub ledger: EnergyLedger,
}

/// Why a plan could not be flown as laid out.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum FlyError {
    /// The flown climb and descent footprints left no room for cruise.
    TooShort {
        /// Distance the ladders overrun the route by, m.
        deficit_m: f64,
    },
    /// A physical or numerical failure of the integration.
    Fuel(FuelModelError),
}

impl From<FuelModelError> for FlyError {
    fn from(error: FuelModelError) -> Self {
        Self::Fuel(error)
    }
}

impl From<DeckError> for FlyError {
    fn from(error: DeckError) -> Self {
        Self::Fuel(FuelModelError::NotConverged(error.to_string()))
    }
}

#[derive(Clone)]
struct Integrator<'a> {
    model: &'a SegmentMissionModel,
    mass_kg: f64,
    fuel_kg: f64,
    time_s: f64,
    distance_m: f64,
    previous_tas_m_s: Option<f64>,
    pending_kinetic_j: f64,
    ledger: EnergyLedger,
    rate_limited_steps: usize,
    minimum_climb_rate_m_s: f64,
    minimum_descent_rate_m_s: f64,
    maximum_clean_cl: f64,
    /// Let planned-route attempts classify an unpaid level transition as a
    /// route-fit deficit. Direct integrator tests keep the strict boundary
    /// error so they can exercise the acceptance guard itself.
    allow_route_transition_deficit: bool,
}

/// The planned geometry of one step.
#[derive(Clone, Copy)]
struct StepPlan {
    kind: SegmentKind,
    altitude_m: f64,
    tas_m_s: f64,
    /// Signed altitude change over the step, m.
    dh_m: f64,
    /// Planned duration, s.
    planned_dt_s: f64,
    /// Number of cells remaining in this segment, including this one. The
    /// open boundary kinetic budget is allocated over these cells so its
    /// energy cost converges with the altitude discretization.
    remaining_steps: usize,
    cap: PropulsionRating,
}

/// One solved step at a given mass.
struct StepSolution {
    thrust_n: f64,
    fuel_flow_kg_s: f64,
    drag_n: f64,
    dt_s: f64,
    vertical_rate_m_s: f64,
    kinetic_delivered_j: f64,
    limited: bool,
}

impl Integrator<'_> {
    /// Drag at this step after the phase lift-limit gate.
    fn phase_drag_n(
        &mut self,
        kind: SegmentKind,
        flight: &FlightCondition,
        mass_kg: f64,
    ) -> Result<f64, FlyError> {
        let limits = &self.model.phase_limits;
        let (cl_limit, delta_cd, clean) = match kind {
            SegmentKind::Takeoff => (limits.takeoff_cl_max, limits.high_lift_delta_cd, false),
            SegmentKind::Landing => (limits.landing_cl_max, limits.high_lift_delta_cd, false),
            SegmentKind::Climb | SegmentKind::Cruise | SegmentKind::Descent => {
                (limits.clean_cl_max, 0.0, true)
            }
        };
        let (cl, drag_n) = self.model.drag_components_n(flight, mass_kg, delta_cd)?;
        if cl > cl_limit {
            return Err(FuelModelError::NotConverged(format!(
                "polar validity: {kind:?} lift coefficient {cl:.3} exceeds the {cl_limit:.3} limit at {:.0} m and {:.1} m/s",
                flight.altitude_m, flight.velocity_m_s
            ))
            .into());
        }
        if clean {
            self.maximum_clean_cl = self.maximum_clean_cl.max(cl);
        }
        Ok(drag_n)
    }

    /// Solve one step at `mass_kg` against the pending kinetic budget.
    fn solve_step(
        &mut self,
        step: StepPlan,
        flight: &FlightCondition,
        mass_kg: f64,
    ) -> Result<StepSolution, FlyError> {
        let deck = &self.model.propulsion;
        let v = step.tas_m_s;
        let drag_n = self.phase_drag_n(step.kind, flight, mass_kg)?;
        let weight_n = mass_kg * self.model.gravity_m_s2;
        let dh = step.dh_m;
        let planned_dt = step.planned_dt_s;
        let planned_vz = dh / planned_dt;
        let pending = self.pending_kinetic_j;
        let remaining_steps = step.remaining_steps.max(1) as f64;
        let pending_installment = pending / remaining_steps;
        let rate_thrust_n = drag_n + weight_n * planned_vz / v;
        let required_n = rate_thrust_n + pending_installment / (v * planned_dt);
        let rated = deck.rated_point(*flight, step.cap)?;
        let idle = deck.idle_point(*flight)?;
        let planned =
            |point: OperatingPoint, kinetic_delivered_j: f64, limited: bool| StepSolution {
                thrust_n: point.thrust_n,
                fuel_flow_kg_s: point.fuel_flow_kg_s,
                drag_n,
                dt_s: planned_dt,
                vertical_rate_m_s: planned_vz,
                kinetic_delivered_j,
                limited,
            };
        let mut floor = idle;
        if idle.thrust_n <= required_n && required_n <= rated.thrust_n {
            let point = deck.at_thrust(*flight, required_n, step.cap)?;
            if point.limit != ThrustLimit::IdleFloor || point.thrust_n <= required_n {
                return Ok(planned(point, pending_installment, false));
            }
            // The deck's lowest running point is above the flight-idle
            // surrogate and above the request: treat it as the floor.
            floor = point;
        }
        let above_rating = required_n > rated.thrust_n;
        let (bound, label) = if above_rating {
            (rated, "rating")
        } else {
            (floor, "flight idle")
        };
        let bound_fits_rate = if above_rating {
            rate_thrust_n <= bound.thrust_n
        } else {
            rate_thrust_n >= bound.thrust_n
        };
        // A rate bound can sometimes sustain the planned vertical rate while
        // still being unable to pay the whole pending speed transition in the
        // planned time. For a vertical step, use the same combined balance in
        // both cases so an attainable transition is not stranded at its next
        // boundary.
        let excess_power_w = (bound.thrust_n - drag_n) * v;
        let combined_energy_j = weight_n * dh + pending_installment;
        let dt_s = combined_energy_j / excess_power_w;
        let vz = dh / dt_s;
        let vertical_combined_feasible = dh.is_finite()
            && dh != 0.0
            && pending.is_finite()
            && excess_power_w.is_finite()
            && excess_power_w != 0.0
            && combined_energy_j.is_finite()
            && dt_s.is_finite()
            && dt_s > 0.0
            && vz.is_finite()
            && vz.signum() == dh.signum()
            && (dh < 0.0 || vz >= MINIMUM_CLIMB_RATE_M_S);
        if vertical_combined_feasible {
            return Ok(StepSolution {
                thrust_n: bound.thrust_n,
                fuel_flow_kg_s: bound.fuel_flow_kg_s,
                drag_n,
                dt_s,
                vertical_rate_m_s: vz,
                kinetic_delivered_j: pending_installment,
                limited: true,
            });
        }
        if bound_fits_rate {
            // If the full transition would take a sub-service-ceiling rate,
            // keep the planned rate and carry a *signed* partial payment
            // across later steps of the same target. A bound that makes no
            // progress toward the requested transition is a real deficit and
            // must not be reported as a flown target.
            let delivered_j = (bound.thrust_n - drag_n) * v * planned_dt - weight_n * dh;
            let makes_progress = pending_installment == 0.0
                || (delivered_j.is_finite() && pending_installment * delivered_j > 0.0);
            if makes_progress {
                return Ok(planned(bound, delivered_j, true));
            }
        }
        let phase = if dh > 0.0 {
            "climb"
        } else if dh < 0.0 {
            "descent"
        } else {
            "level flight"
        };
        Err(FuelModelError::NotConverged(format!(
            "{phase} energy deficit at {:.0} m and {v:.1} m/s: {rate_thrust_n:.0} N required for {planned_vz:.2} m/s, {:.0} N at {label} against {drag_n:.0} N drag, {pending:.0} J pending (dh {dh:.3} m, combined {combined_energy_j:.0} J, excess {excess_power_w:.0} W, dt {dt_s:.3} s, Vz {vz:.3} m/s){}",
            step.altitude_m,
            bound.thrust_n,
            if dh < 0.0 && bound.thrust_n >= drag_n {
                "; a drag device would be required"
            } else {
                ""
            }
        ))
        .into())
    }

    /// Fly one step with a midpoint mass predictor; returns its horizontal
    /// distance.
    fn fly_step(&mut self, step: StepPlan) -> Result<f64, FlyError> {
        let flight = self.model.propulsion.flight_condition(
            step.altitude_m,
            step.tas_m_s,
            self.model.gravity_m_s2,
            self.model.isa_deviation_c,
        )?;
        let start_mass_kg = self.mass_kg;
        let predictor = self.solve_step(step, &flight, start_mass_kg)?;
        let mid_mass_kg = start_mass_kg - 0.5 * predictor.fuel_flow_kg_s * predictor.dt_s;
        if !mid_mass_kg.is_finite() || mid_mass_kg <= 0.0 {
            return Err(FuelModelError::NotConverged(format!(
                "profile burned {} kg from remaining mass {start_mass_kg} kg",
                predictor.fuel_flow_kg_s * predictor.dt_s
            ))
            .into());
        }
        let solved = self.solve_step(step, &flight, mid_mass_kg)?;
        let v = step.tas_m_s;
        let burn_kg = solved.fuel_flow_kg_s * solved.dt_s;
        if !burn_kg.is_finite() || burn_kg < 0.0 || burn_kg >= start_mass_kg {
            return Err(FuelModelError::NotConverged(format!(
                "profile burned {burn_kg} kg from remaining mass {start_mass_kg} kg"
            ))
            .into());
        }
        let weight_n = mid_mass_kg * self.model.gravity_m_s2;
        self.ledger.propulsive_work_j += solved.thrust_n * v * solved.dt_s;
        self.ledger.drag_work_j += solved.drag_n * v * solved.dt_s;
        self.ledger.potential_energy_j += weight_n * solved.vertical_rate_m_s * solved.dt_s;
        self.ledger.kinetic_energy_j += solved.kinetic_delivered_j;
        self.pending_kinetic_j -= solved.kinetic_delivered_j;
        if solved.limited {
            self.rate_limited_steps += 1;
        }
        if step.dh_m > 0.0 {
            self.minimum_climb_rate_m_s = self.minimum_climb_rate_m_s.min(solved.vertical_rate_m_s);
        } else if step.dh_m < 0.0 {
            self.minimum_descent_rate_m_s =
                self.minimum_descent_rate_m_s.min(-solved.vertical_rate_m_s);
        }
        self.fuel_kg += burn_kg;
        self.mass_kg = start_mass_kg - burn_kg;
        self.time_s += solved.dt_s;
        let horizontal_m =
            (v * v - solved.vertical_rate_m_s * solved.vertical_rate_m_s).sqrt() * solved.dt_s;
        self.distance_m += horizontal_m;
        Ok(horizontal_m)
    }

    /// Open the kinetic budget for a transition to `tas_m_s`.
    ///
    /// A boundary is an instantaneous profile change in this reduced model.
    /// A material unpaid budget may continue while the same target is split
    /// across segments, but it must close before a distinct target is opened.
    /// Otherwise the schedule could report an intermediate speed as flown
    /// even though the aircraft never attained it.
    fn open_transition(&mut self, tas_m_s: f64) -> Result<(), FlyError> {
        if let Some(previous) = self.previous_tas_m_s {
            let speed_scale = previous.abs().max(tas_m_s.abs());
            let kinetic_scale = 0.5 * self.mass_kg * speed_scale * speed_scale;
            let tolerance_j = BOUNDARY_KE_ABSOLUTE_TOLERANCE_J
                .max(BOUNDARY_KE_RELATIVE_TOLERANCE * kinetic_scale);
            let previous_pending_j = self.pending_kinetic_j;
            let target_is_distinct = (tas_m_s - previous).abs()
                > (2.0 * tolerance_j / self.mass_kg.max(f64::MIN_POSITIVE)).sqrt();
            if previous_pending_j.abs() > tolerance_j && target_is_distinct {
                return Err(FuelModelError::NotConverged(format!(
                    "speed schedule not attained before next distinct transition from {previous:.3} to {tas_m_s:.3} m/s: {pending:.3} J of kinetic energy remained unpaid (boundary tolerance {tolerance_j:.3} J)",
                    pending = previous_pending_j,
                ))
                .into());
            }
            let previous_kinetic_j = 0.5 * self.mass_kg * previous * previous;
            // `pending_kinetic_j` is target KE minus the KE the deck has
            // actually delivered. Reconstruct that delivered state before
            // changing targets; adding the new target delta to the old
            // budget would let two target changes cancel without ever
            // reaching the intermediate speed.
            let actual_kinetic_j = if previous_pending_j.abs() <= tolerance_j {
                previous_kinetic_j
            } else {
                previous_kinetic_j - previous_pending_j
            };
            if !actual_kinetic_j.is_finite() || actual_kinetic_j < -tolerance_j {
                return Err(FuelModelError::NotConverged(format!(
                    "nonphysical kinetic state before transition from {previous:.3} m/s: {actual_kinetic_j:.3} J"
                ))
                .into());
            }
            let actual_kinetic_j = actual_kinetic_j.max(0.0);
            let target_kinetic_j = 0.5 * self.mass_kg * tas_m_s * tas_m_s;
            let target_pending_j = target_kinetic_j - actual_kinetic_j;
            // Drop only a round-off residue. A material same-direction
            // budget must remain open so later steps can pay it; clearing it
            // here would recreate the boundary-energy cancellation bug.
            self.pending_kinetic_j = if target_pending_j.abs() <= tolerance_j {
                0.0
            } else {
                target_pending_j
            };
        }
        self.previous_tas_m_s = Some(tas_m_s);
        Ok(())
    }

    /// Fly one planned segment; returns its horizontal distance.
    fn fly_segment(&mut self, segment: &Segment) -> Result<f64, FlyError> {
        let n = self.model.steps_per_segment;
        let cap = match segment.kind {
            SegmentKind::Takeoff => PropulsionRating::TakeoffGoAround,
            SegmentKind::Cruise => PropulsionRating::Cruise,
            SegmentKind::Climb | SegmentKind::Descent | SegmentKind::Landing => {
                PropulsionRating::MaximumClimb
            }
        };
        self.open_transition(segment.tas_m_s)?;
        let dh_m = (segment.end_altitude_m - segment.start_altitude_m) / n as f64;
        let planned_dt_s = segment.duration_s() / n as f64;
        let mut distance_m = 0.0;
        for step in 0..n {
            distance_m += self.fly_step(StepPlan {
                kind: segment.kind,
                altitude_m: segment.start_altitude_m + (step as f64 + 0.5) * dh_m,
                tas_m_s: segment.tas_m_s,
                dh_m,
                planned_dt_s,
                remaining_steps: n - step,
                cap,
            })?;
        }
        if self.allow_route_transition_deficit && segment.kind == SegmentKind::Cruise {
            if let Some(deficit_m) = self.level_transition_deficit_m(segment)? {
                return Err(FlyError::TooShort { deficit_m });
            }
        }
        Ok(distance_m)
    }

    /// Additional level distance required to close a material speed transition
    /// at the current bound. A route attempt may lower cruise altitude and
    /// retry when the fixed rung fraction left too little distance; direct
    /// callers retain the strict boundary error in `open_transition`.
    fn level_transition_deficit_m(&mut self, segment: &Segment) -> Result<Option<f64>, FlyError> {
        let speed = segment.tas_m_s;
        let pending = self.pending_kinetic_j;
        let speed_scale = self
            .previous_tas_m_s
            .unwrap_or(speed)
            .abs()
            .max(speed.abs());
        let kinetic_scale = 0.5 * self.mass_kg * speed_scale * speed_scale;
        let tolerance_j =
            BOUNDARY_KE_ABSOLUTE_TOLERANCE_J.max(BOUNDARY_KE_RELATIVE_TOLERANCE * kinetic_scale);
        if !pending.is_finite() || pending.abs() <= tolerance_j {
            return Ok(None);
        }
        let flight = self.model.propulsion.flight_condition(
            segment.end_altitude_m,
            speed,
            self.model.gravity_m_s2,
            self.model.isa_deviation_c,
        )?;
        let drag_n = self.phase_drag_n(SegmentKind::Cruise, &flight, self.mass_kg)?;
        let point = if pending > 0.0 {
            self.model
                .propulsion
                .rated_point(flight, PropulsionRating::Cruise)?
        } else {
            self.model.propulsion.idle_point(flight)?
        };
        let excess_power_w = (point.thrust_n - drag_n) * speed;
        if !excess_power_w.is_finite() || pending * excess_power_w <= 0.0 {
            return Ok(None);
        }
        let deficit_m = pending.abs() * speed / excess_power_w.abs();
        if !deficit_m.is_finite() || deficit_m <= 0.0 {
            return Ok(None);
        }
        Ok(Some(deficit_m))
    }

    /// Fly the cruise rungs over `cruise_distance_m` and the descent ladder;
    /// returns the flown descent footprint.
    fn fly_cruise_and_descent(
        &mut self,
        plan: &ProfilePlan,
        cruise_distance_m: f64,
    ) -> Result<f64, FlyError> {
        let fraction_sum: f64 = plan.cruise_rungs.iter().map(|(_, f)| f).sum();
        for &(speed, fraction) in &plan.cruise_rungs {
            let distance_m = cruise_distance_m * fraction / fraction_sum;
            if distance_m > 1.0e-6 {
                self.fly_segment(&Segment::level(
                    SegmentKind::Cruise,
                    plan.cruise_altitude_m,
                    speed,
                    distance_m,
                ))?;
            }
        }
        let mut descent_footprint_m = 0.0;
        for segment in &plan.descent {
            descent_footprint_m += self.fly_segment(segment)?;
        }
        Ok(descent_footprint_m)
    }
}

impl SegmentMissionModel {
    /// Lift coefficient and installed drag at `flight` for `mass_kg`:
    /// `CD = CD0 + k CL^2 + CD_wave(M) + delta_cd`.
    pub(super) fn drag_components_n(
        &self,
        flight: &FlightCondition,
        mass_kg: f64,
        delta_cd: f64,
    ) -> Result<(f64, f64), FuelModelError> {
        let speed = flight.velocity_m_s;
        let dynamic_pressure_pa = 0.5 * flight.density_kg_m3 * speed * speed;
        if !dynamic_pressure_pa.is_finite() || dynamic_pressure_pa <= 0.0 {
            return Err(FuelModelError::InvalidModel(format!(
                "profile dynamic pressure is unusable at {} m and {speed} m/s",
                flight.altitude_m
            )));
        }
        let cl = mass_kg * self.gravity_m_s2 / (dynamic_pressure_pa * self.wing_area_m2);
        let cd =
            self.cd0 + self.induced_factor_k * cl * cl + self.wave_drag_at(flight.mach) + delta_cd;
        if !cl.is_finite() || !cd.is_finite() || cd <= 0.0 {
            return Err(FuelModelError::NotConverged(format!(
                "invalid drag state CL={cl}, CD={cd}"
            )));
        }
        Ok((cl, dynamic_pressure_pa * self.wing_area_m2 * cd))
    }

    /// Clean installed drag at `flight` for `mass_kg`.
    pub(super) fn drag_n(
        &self,
        flight: &FlightCondition,
        mass_kg: f64,
    ) -> Result<f64, FuelModelError> {
        self.drag_components_n(flight, mass_kg, 0.0)
            .map(|(_, drag_n)| drag_n)
    }

    /// Fly `plan` from `start_mass_kg`.
    ///
    /// The climb ladder is flown first; the cruise distance is then closed
    /// against the flown climb footprint and a fixed-point estimate of the
    /// flown descent footprint, which depends only weakly on the landing
    /// mass and on any deck-limited descent steps.
    pub(super) fn fly(&self, start_mass_kg: f64, plan: &ProfilePlan) -> Result<FlownLeg, FlyError> {
        if !start_mass_kg.is_finite() || start_mass_kg <= 0.0 {
            return Err(FuelModelError::MassOutOfRange {
                mass_kg: start_mass_kg,
            }
            .into());
        }
        let mut climbed = Integrator {
            model: self,
            mass_kg: start_mass_kg,
            fuel_kg: 0.0,
            time_s: 0.0,
            distance_m: 0.0,
            previous_tas_m_s: None,
            pending_kinetic_j: 0.0,
            ledger: EnergyLedger::default(),
            rate_limited_steps: 0,
            minimum_climb_rate_m_s: f64::INFINITY,
            minimum_descent_rate_m_s: f64::INFINITY,
            maximum_clean_cl: 0.0,
            allow_route_transition_deficit: true,
        };
        let mut climb_footprint_m = 0.0;
        for segment in &plan.climb {
            climb_footprint_m += climbed.fly_segment(segment)?;
        }
        let mut descent_estimate_m = plan.descent_footprint_m();
        let mut flown: Option<(Integrator<'_>, f64)> = None;
        for _ in 0..DESCENT_FOOTPRINT_PASSES {
            let cruise_distance_m = plan.range_m - climb_footprint_m - descent_estimate_m;
            if cruise_distance_m < -CRUISE_DISTANCE_TOLERANCE_M {
                return Err(FlyError::TooShort {
                    deficit_m: -cruise_distance_m,
                });
            }
            let mut integrator = climbed.clone();
            let descent_footprint_m =
                integrator.fly_cruise_and_descent(plan, cruise_distance_m.max(0.0))?;
            let mismatch_m = descent_footprint_m - descent_estimate_m;
            flown = Some((integrator, descent_footprint_m));
            if mismatch_m.abs() <= CRUISE_DISTANCE_TOLERANCE_M {
                break;
            }
            descent_estimate_m = descent_footprint_m;
        }
        let Some((integrator, descent_footprint_m)) = flown else {
            return Err(
                FuelModelError::NotConverged("descent footprint did not close".to_owned()).into(),
            );
        };
        if (integrator.distance_m - plan.range_m).abs() > 10.0 * CRUISE_DISTANCE_TOLERANCE_M {
            return Err(FuelModelError::NotConverged(format!(
                "route closure: flew {:.1} m of a {:.1} m route after {DESCENT_FOOTPRINT_PASSES} descent passes",
                integrator.distance_m, plan.range_m
            ))
            .into());
        }
        if !integrator.fuel_kg.is_finite() || !integrator.time_s.is_finite() {
            return Err(FuelModelError::NotConverged(
                "profile integration is non-finite".to_owned(),
            )
            .into());
        }
        let unrealized_j = integrator.pending_kinetic_j;
        if unrealized_j.abs()
            > UNREALIZED_KINETIC_FRACTION * integrator.ledger.propulsive_work_j.abs()
        {
            return Err(FuelModelError::NotConverged(format!(
                "speed schedule not attained: {unrealized_j:.0} J of kinetic energy remained unrealized against {:.0} J of propulsive work",
                integrator.ledger.propulsive_work_j
            ))
            .into());
        }
        Ok(FlownLeg {
            leg: LegEstimate {
                fuel_kg: integrator.fuel_kg,
                time_s: integrator.time_s,
            },
            end_mass_kg: integrator.mass_kg,
            cruise_altitude_m: plan.cruise_altitude_m,
            adapted: plan.adapted,
            rate_limited_steps: integrator.rate_limited_steps,
            minimum_climb_rate_m_s: integrator.minimum_climb_rate_m_s,
            minimum_descent_rate_m_s: integrator.minimum_descent_rate_m_s,
            maximum_clean_cl: integrator.maximum_clean_cl,
            climb_footprint_m,
            descent_footprint_m,
            flown_distance_m: integrator.distance_m,
            ledger: EnergyLedger {
                unrealized_kinetic_j: unrealized_j,
                ..integrator.ledger
            },
        })
    }
}

#[cfg(test)]
// These tests use unwrap/expect for controlled fixtures; a failure is the
// assertion with its context rather than a production error path.
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::mdo::build::build_geometry;
    use crate::mdo::propulsion::max_climb_rate_ft_min;
    use alas_config::design_variables::DesignVector;
    use alas_config::AlasConfig;

    fn test_model() -> SegmentMissionModel {
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
            0.018,
            0.045,
            0.002,
            requirements.gravity_m_s2,
            457.2,
            phase_limits,
            deck,
        )
        .expect("default mission model")
    }

    fn integrator<'a>(model: &'a SegmentMissionModel) -> Integrator<'a> {
        Integrator {
            model,
            mass_kg: 10_000.0,
            fuel_kg: 0.0,
            time_s: 0.0,
            distance_m: 0.0,
            previous_tas_m_s: None,
            pending_kinetic_j: 0.0,
            ledger: EnergyLedger::default(),
            rate_limited_steps: 0,
            minimum_climb_rate_m_s: f64::INFINITY,
            minimum_descent_rate_m_s: f64::INFINITY,
            maximum_clean_cl: 0.0,
            allow_route_transition_deficit: false,
        }
    }

    fn level(tas_m_s: f64, distance_m: f64) -> Segment {
        Segment::level(SegmentKind::Cruise, 0.0, tas_m_s, distance_m)
    }

    #[test]
    fn a_rating_limited_100_to_110_to_100_profile_rejects_unpaid_boundary_energy() {
        let model = test_model();
        let mut integrator = integrator(&model);
        integrator
            .fly_segment(&level(100.0, 10_000.0))
            .expect("the first level segment is reachable");
        integrator
            .fly_segment(&level(110.0, 1.0))
            .expect("the short speed-up segment is rating-limited but reachable");
        assert!(integrator.pending_kinetic_j > 1.0e6);

        let error = integrator
            .fly_segment(&level(100.0, 10_000.0))
            .expect_err("an unpaid speed-up must not be cancelled by the next speed-down");
        assert!(matches!(
            error,
            FlyError::Fuel(FuelModelError::NotConverged(reason))
                if reason.contains("speed schedule not attained before next distinct transition")
        ));
    }

    #[test]
    fn a_rating_limited_100_to_110_to_105_profile_rejects_partial_reversal() {
        let model = test_model();
        let mut integrator = integrator(&model);
        integrator
            .fly_segment(&level(100.0, 10_000.0))
            .expect("the first level segment is reachable");
        integrator
            .fly_segment(&level(110.0, 1.0))
            .expect("the short speed-up segment is rating-limited but reachable");
        assert!(integrator.pending_kinetic_j > 1.0e6);

        let error = integrator
            .fly_segment(&level(105.0, 10_000.0))
            .expect_err("a partial reversal must not abandon an unpaid 110 m/s target");
        assert!(matches!(
            error,
            FlyError::Fuel(FuelModelError::NotConverged(reason))
                if reason.contains("speed schedule not attained before next distinct transition")
        ));
    }

    #[test]
    fn a_rating_limited_unpaid_monotonic_transition_rejects_next_target() {
        let model = test_model();
        let mut integrator = integrator(&model);
        integrator
            .fly_segment(&level(100.0, 10_000.0))
            .expect("the first level segment is reachable");
        integrator
            .fly_segment(&level(110.0, 1.0))
            .expect("the short speed-up segment is rating-limited but reachable");
        assert!(integrator.pending_kinetic_j > 1.0e6);

        let error = integrator
            .fly_segment(&level(115.0, 10_000.0))
            .expect_err("a monotonic next target must not bypass an unpaid 110 m/s target");
        assert!(matches!(
            error,
            FlyError::Fuel(FuelModelError::NotConverged(reason))
                if reason.contains("speed schedule not attained before next distinct transition")
        ));
    }

    #[test]
    fn a_reachable_small_speed_transition_closes_before_the_next_boundary() {
        let model = test_model();
        let mut integrator = integrator(&model);
        integrator
            .fly_segment(&level(100.0, 10_000.0))
            .expect("the first level segment is reachable");
        integrator
            .fly_segment(&level(100.1, 100_000.0))
            .expect("a small speed-up with sufficient distance is reachable");
        assert!(integrator.pending_kinetic_j.abs() <= 1.0);
        integrator
            .fly_segment(&level(100.0, 100_000.0))
            .expect("the paid speed-down must not be rejected");
    }

    fn rating_limited_vertical_fixture() -> (
        SegmentMissionModel,
        StepPlan,
        FlightCondition,
        f64,
        f64,
        f64,
    ) {
        let model = test_model();
        let mass_kg = 50_000.0;
        let altitude_m = 10_000.0;
        let tas_m_s = 200.0;
        let flight = model
            .propulsion
            .flight_condition(
                altitude_m,
                tas_m_s,
                model.gravity_m_s2,
                model.isa_deviation_c,
            )
            .expect("fixture flight condition");
        let mut probe = integrator(&model);
        let drag_n = probe
            .phase_drag_n(SegmentKind::Climb, &flight, mass_kg)
            .expect("fixture clean drag");
        let rated = model
            .propulsion
            .rated_point(flight, PropulsionRating::MaximumClimb)
            .expect("fixture rating");
        let weight_n = mass_kg * model.gravity_m_s2;
        let excess_power_w = (rated.thrust_n - drag_n) * tas_m_s;
        let rating_rate_m_s = excess_power_w / weight_n;
        assert!(rating_rate_m_s > MINIMUM_CLIMB_RATE_M_S);
        assert!(rating_rate_m_s < tas_m_s);
        let planned_rate_m_s = rating_rate_m_s + 5.0;
        let planned_dt_s = 100.0;
        let dh_m = planned_rate_m_s * planned_dt_s;
        let pending_kinetic_j = 0.1 * weight_n * dh_m;
        (
            model,
            StepPlan {
                kind: SegmentKind::Climb,
                altitude_m,
                tas_m_s,
                dh_m,
                planned_dt_s,
                remaining_steps: 1,
                cap: PropulsionRating::MaximumClimb,
            },
            flight,
            mass_kg,
            pending_kinetic_j,
            excess_power_w,
        )
    }

    #[test]
    fn a_rating_limited_climb_slows_to_pay_pending_kinetic_energy() {
        let (model, step, flight, mass_kg, pending_kinetic_j, excess_power_w) =
            rating_limited_vertical_fixture();
        let mut integrator = integrator(&model);
        integrator.pending_kinetic_j = pending_kinetic_j;
        let solved = integrator
            .solve_step(step, &flight, mass_kg)
            .expect("the bound can pay the transition during a slower climb");
        assert!(solved.limited);
        assert!(solved.dt_s > step.planned_dt_s);
        assert!(solved.vertical_rate_m_s < step.dh_m / step.planned_dt_s);
        assert!(solved.vertical_rate_m_s >= MINIMUM_CLIMB_RATE_M_S);
        assert_eq!(solved.kinetic_delivered_j, pending_kinetic_j);
        let propulsive_j = solved.thrust_n * step.tas_m_s * solved.dt_s;
        let drag_j = solved.drag_n * step.tas_m_s * solved.dt_s;
        let potential_j = mass_kg * model.gravity_m_s2 * step.dh_m;
        let residual_j = propulsive_j - drag_j - potential_j - pending_kinetic_j;
        assert!(
            residual_j.abs() <= 1.0e-10 * propulsive_j.abs().max(1.0),
            "combined vertical balance residual {residual_j} J with {excess_power_w} W excess power"
        );
    }

    #[test]
    fn an_unreachable_bound_limited_climb_still_rejects_below_service_ceiling_rate() {
        let (model, step, flight, mass_kg, _, excess_power_w) = rating_limited_vertical_fixture();
        let weight_n = mass_kg * model.gravity_m_s2;
        let impossible_rate_m_s = 0.5 * MINIMUM_CLIMB_RATE_M_S;
        let pending_kinetic_j =
            excess_power_w * step.dh_m / impossible_rate_m_s - weight_n * step.dh_m;
        assert!(pending_kinetic_j.is_finite() && pending_kinetic_j > 0.0);
        let mut integrator = integrator(&model);
        integrator.pending_kinetic_j = pending_kinetic_j;
        let error = match integrator.solve_step(step, &flight, mass_kg) {
            Ok(_) => panic!("a transition requiring a sub-service-ceiling climb must reject"),
            Err(error) => error,
        };
        assert!(matches!(
            error,
            FlyError::Fuel(FuelModelError::NotConverged(reason))
                if reason.contains("climb energy deficit")
        ));
    }
}
