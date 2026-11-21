// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The step-by-step record of a flown priced leg.
//!
//! The priced model that sizes the aeroplane used to report only totals (fuel,
//! time, footprints). The record lets the mission plausibility rules and the
//! native-versus-priced consistency check read the profile that is actually
//! priced: altitude, range, time, commanded speed, mass, lift coefficient,
//! drag and thrust of every integration step.

use alas_prop::system::FlightCondition;

use super::{EnergyLedger, FlownLeg, Integrator, SegmentKind, StepPlan, StepSolution};
use crate::mdo::mission_model::takeoff_acceleration::Acceleration;
use crate::mdo::mission_model::SegmentMissionModel;

/// The part of the flight a step belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepPhase {
    /// The ground roll allowance (time and fuel only, no distance).
    Ground,
    /// The level acceleration from the take-off safety speed at the screen
    /// height, and the take-off band.
    Takeoff,
    /// A climb rung or step climb.
    Climb,
    /// Level cruise.
    Cruise,
    /// A descent rung.
    Descent,
    /// The landing band.
    Landing,
}

impl From<SegmentKind> for StepPhase {
    fn from(kind: SegmentKind) -> Self {
        match kind {
            SegmentKind::Takeoff => Self::Takeoff,
            SegmentKind::Climb => Self::Climb,
            SegmentKind::Cruise => Self::Cruise,
            SegmentKind::Descent => Self::Descent,
            SegmentKind::Landing => Self::Landing,
        }
    }
}

/// One integration step of a flown priced leg. Times are from brake release,
/// ranges from the start of the lift-off point, SI units. The speed is the
/// commanded true airspeed of the step's segment; a change from the previous
/// step's speed is a kinetic-energy budget the integrator pays (see the module
/// documentation of `integrate`), not a discontinuity in the aircraft state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlownStep {
    /// Part of the flight.
    pub phase: StepPhase,
    /// Start and end time, s.
    pub time_s: [f64; 2],
    /// Start and end geometric altitude, m.
    pub altitude_m: [f64; 2],
    /// Start and end range, m.
    pub range_m: [f64; 2],
    /// Commanded true airspeed at the start and end of the step, m/s.
    pub tas_m_s: [f64; 2],
    /// Mach number at the step's mid altitude and commanded speed.
    pub mach: f64,
    /// Equivalent airspeed at the step's mid altitude, m/s.
    pub eas_m_s: f64,
    /// Start and end mass, kg.
    pub mass_kg: [f64; 2],
    /// Lift coefficient at the mid-step mass.
    pub lift_coefficient: f64,
    /// Lift-to-drag ratio at the mid-step mass (zero where not recorded).
    pub lift_to_drag: f64,
    /// Drag, N.
    pub drag_n: f64,
    /// Installed thrust, N (the deck's, at the throttle or bound the step flew).
    pub thrust_n: f64,
    /// Whether the step flew at a rating or idle bound rather than the solved
    /// throttle.
    pub bound_limited: bool,
}

impl FlownStep {
    /// A step of the ground roll: time and fuel, no distance or altitude.
    pub(in crate::mdo::mission_model) fn ground_roll(
        altitude_m: f64,
        time_s: f64,
        mass_kg: [f64; 2],
    ) -> Self {
        Self {
            phase: StepPhase::Ground,
            time_s: [0.0, time_s],
            altitude_m: [altitude_m; 2],
            range_m: [0.0; 2],
            tas_m_s: [0.0; 2],
            mach: 0.0,
            eas_m_s: 0.0,
            mass_kg,
            lift_coefficient: 0.0,
            lift_to_drag: 0.0,
            drag_n: 0.0,
            thrust_n: 0.0,
            bound_limited: false,
        }
    }

    /// A climb that changes speed along its path (see `takeoff_acceleration`): the take-off climb
    /// (the constant-speed climb at the safety speed and the acceleration to the first segment's
    /// speed, `phase` take-off, from the field) or a path piece at a speed change along the climb;
    /// `start` is its start time (s) and range (m).
    /// Its lift coefficient, drag and thrust are recorded by [`FlownStep::with_forces`] at the call
    /// site (zero until then).
    pub(in crate::mdo::mission_model) fn acceleration(
        phase: StepPhase,
        start_altitude_m: f64,
        start: (f64, f64),
        start_mass_kg: f64,
        acceleration: &Acceleration,
        target_tas_m_s: f64,
    ) -> Self {
        let altitude_m = start_altitude_m;
        Self {
            phase,
            time_s: [start.0, start.0 + acceleration.time_s],
            altitude_m: [altitude_m, acceleration.altitude_end_m],
            range_m: [start.1, start.1 + acceleration.distance_m],
            tas_m_s: [acceleration.v2_m_s, target_tas_m_s],
            mach: 0.0,
            eas_m_s: 0.0,
            mass_kg: [start_mass_kg, start_mass_kg - acceleration.fuel_kg],
            lift_coefficient: 0.0,
            lift_to_drag: 0.0,
            drag_n: 0.0,
            thrust_n: 0.0,
            bound_limited: false,
        }
    }
    /// A level speed change at the restriction altitude (the acceleration inside a climb or the
    /// deceleration inside a descent, by `phase`): the ground distance and time it takes, with
    /// the Mach number and the equivalent airspeed at its end on a day `isa_deviation_c` warmer than
    /// standard; lift coefficient, drag and thrust are recorded by [`FlownStep::with_forces`] at the
    /// call site (zero until then).
    pub(in crate::mdo::mission_model) fn level_acceleration(
        phase: StepPhase,
        altitude_m: f64,
        time_s: [f64; 2],
        range_m: [f64; 2],
        tas_m_s: [f64; 2],
        mass_kg: [f64; 2],
        isa_deviation_c: f64,
    ) -> Self {
        let air = alas_atmo::us1976_compute_values(altitude_m, isa_deviation_c);
        Self {
            phase,
            time_s,
            altitude_m: [altitude_m; 2],
            range_m,
            tas_m_s,
            mach: tas_m_s[1] / air.speed_of_sound_m_s,
            eas_m_s: tas_m_s[1]
                * (air.density_kg_m3 / alas_atmo::SEA_LEVEL_DENSITY_ICAO_KG_M3).sqrt(),
            mass_kg,
            lift_coefficient: 0.0,
            lift_to_drag: 0.0,
            drag_n: 0.0,
            thrust_n: 0.0,
            bound_limited: false,
        }
    }

    /// The step with the mean forces of a speed-change piece recorded (see [`piece_forces`]).
    pub(in crate::mdo::mission_model) fn with_forces(self, forces: PieceForces) -> Self {
        Self {
            lift_coefficient: forces.lift_coefficient,
            lift_to_drag: forces.lift_to_drag,
            drag_n: forces.drag_n,
            thrust_n: forces.thrust_n,
            ..self
        }
    }

    /// The step moved later by `time_s`.
    pub(in crate::mdo::mission_model) fn delayed(self, time_s: f64) -> Self {
        Self {
            time_s: [self.time_s[0] + time_s, self.time_s[1] + time_s],
            ..self
        }
    }
}

impl Integrator<'_> {
    /// Record the step just solved. `before` is the integrator's
    /// (time, distance) before the step.
    pub(super) fn record_step(
        &mut self,
        step: &StepPlan,
        flight: &FlightCondition,
        solved: &StepSolution,
        before: (f64, f64),
        masses_kg: [f64; 3],
    ) {
        let [start_kg, mid_kg, end_kg] = masses_kg;
        let model = self.model;
        let dynamic_pressure_pa = 0.5 * flight.density_kg_m3 * step.tas_m_s * step.tas_m_s;
        self.trace.push(FlownStep {
            phase: step.kind.into(),
            time_s: [before.0, self.time_s],
            altitude_m: [
                step.altitude_m - 0.5 * step.dh_m,
                step.altitude_m + 0.5 * step.dh_m,
            ],
            range_m: [before.1, self.distance_m],
            tas_m_s: [step.tas_m_s; 2],
            mach: flight.mach,
            eas_m_s: step.tas_m_s
                * (flight.density_kg_m3 / alas_atmo::SEA_LEVEL_DENSITY_ICAO_KG_M3).sqrt(),
            mass_kg: [start_kg, end_kg],
            lift_coefficient: mid_kg * model.gravity_at(step.altitude_m)
                / (dynamic_pressure_pa * model.wing_area_m2),
            lift_to_drag: mid_kg * model.gravity_at(step.altitude_m) / solved.drag_n,
            drag_n: solved.drag_n,
            thrust_n: solved.thrust_n,
            bound_limited: solved.limited,
        });
    }
}

/// Mean forces of a speed-change piece, recorded on its [`FlownStep`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(in crate::mdo::mission_model) struct PieceForces {
    /// Lift coefficient on the steady relation `m g cos(gamma) = q S CL` at the mid-piece mass,
    /// mean speed and mid altitude (the relation `record_step` uses), -.
    pub lift_coefficient: f64,
    /// Lift-to-drag ratio at the same state, -.
    pub lift_to_drag: f64,
    /// Mean drag, N.
    pub drag_n: f64,
    /// Mean installed thrust, N.
    pub thrust_n: f64,
}

/// The mean lift, drag and thrust of a speed-change piece from its energy ledger.
///
/// The ledger integrates `T V dt` and `D V dt` along the flight, and the piece's ground distance
/// is `integral(V cos(gamma) dt)`, so the distance-weighted means are `work / distance`; the
/// error factor is `cos(gamma)`, 0.5 % at a 6 deg climb. `gamma` is the path angle of the piece
/// (`altitude_m` at its ends, `distance_m` on the ground). A piece with no distance or no
/// altitude-path length records nothing (zero forces).
pub(in crate::mdo::mission_model) fn piece_forces(
    model: &SegmentMissionModel,
    ledger: &EnergyLedger,
    altitude_m: [f64; 2],
    distance_m: f64,
    tas_m_s: [f64; 2],
    mass_kg: [f64; 2],
) -> PieceForces {
    let gain_m = altitude_m[1] - altitude_m[0];
    let path_m = distance_m.hypot(gain_m);
    if distance_m <= 0.0 || path_m <= 0.0 {
        return PieceForces::default();
    }
    let cos_gamma = distance_m / path_m;
    let mid_altitude_m = 0.5 * (altitude_m[0] + altitude_m[1]);
    let mean_tas_m_s = 0.5 * (tas_m_s[0] + tas_m_s[1]);
    let mid_mass_kg = 0.5 * (mass_kg[0] + mass_kg[1]);
    let air = alas_atmo::us1976_compute_values(mid_altitude_m, model.isa_deviation_c);
    let dynamic_pressure_pa = 0.5 * air.density_kg_m3 * mean_tas_m_s * mean_tas_m_s;
    let weight_n = mid_mass_kg * model.gravity_at(mid_altitude_m) * cos_gamma;
    let drag_n = ledger.drag_work_j / distance_m;
    PieceForces {
        lift_coefficient: weight_n / (dynamic_pressure_pa * model.wing_area_m2),
        lift_to_drag: if drag_n > 0.0 { weight_n / drag_n } else { 0.0 },
        drag_n,
        thrust_n: ledger.propulsive_work_j / distance_m,
    }
}

/// The climb-band tops of `plan` as an array (take-off, initial, step one, ladder), m.
pub(super) fn bands_of(plan: &super::ProfilePlan) -> [f64; 4] {
    let (takeoff, initial, step_one, ladder) = plan.climb_bands;
    [takeoff, initial, step_one, ladder]
}

/// The levels and speeds of a flown priced leg, which the native flight follows.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CruiseLadder {
    /// Tops of the climb bands (take-off, initial, step one, ladder), m.
    pub climb_bands: [f64; 4],
    /// The level cruise runs, in flight order.
    pub runs: Vec<CruiseRun>,
}

/// One level cruise run of a flown priced leg: consecutive cruise steps at one
/// level and one commanded speed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CruiseRun {
    /// Geometric altitude of the run, m.
    pub altitude_m: f64,
    /// Commanded true airspeed, m/s.
    pub tas_m_s: f64,
    /// Ground distance flown, m.
    pub distance_m: f64,
}

impl FlownLeg {
    /// The cruise ladder of the leg: the climb-band tops and the level cruise
    /// runs in flight order, which another model of the same route can follow.
    /// Steps that differ by less than a millimetre in altitude and a millimetre
    /// per second in speed are one run.
    pub fn cruise_ladder(&self) -> CruiseLadder {
        CruiseLadder {
            climb_bands: self.climb_bands,
            runs: self.cruise_runs(),
        }
    }

    fn cruise_runs(&self) -> Vec<CruiseRun> {
        let mut runs: Vec<CruiseRun> = Vec::new();
        for step in self.steps.iter().filter(|s| s.phase == StepPhase::Cruise) {
            let altitude_m = 0.5 * (step.altitude_m[0] + step.altitude_m[1]);
            let distance_m = step.range_m[1] - step.range_m[0];
            match runs.last_mut() {
                Some(run)
                    if (run.altitude_m - altitude_m).abs() < 1.0e-3
                        && (run.tas_m_s - step.tas_m_s[0]).abs() < 1.0e-3 =>
                {
                    run.distance_m += distance_m;
                }
                _ => runs.push(CruiseRun {
                    altitude_m,
                    tas_m_s: step.tas_m_s[0],
                    distance_m,
                }),
            }
        }
        runs
    }
}
