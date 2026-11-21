// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The integrator's state at the start of a planned climb ladder.

use super::trace::{piece_forces, FlownStep, StepPhase};
use super::{DeckMemo, FlyError, Integrator, ProfilePlan, SegmentKind, SegmentMissionModel};
use crate::mdo::mission_model::takeoff_acceleration::Acceleration;
use alas_mass::fuel_plan::FuelModelError;

impl<'a> Integrator<'a> {
    /// The integrator at the start of `plan` for an aeroplane of
    /// `start_mass_kg` at the top of the field, and the horizontal distance
    /// already flown, m.
    ///
    /// The take-off climb is charged first (see `takeoff_acceleration`): a constant-speed climb
    /// at the safety speed from the field to the acceleration height, then the acceleration to the
    /// take-off band's speed along the climb with the BADA energy share, at the take-off rating.
    /// Its fuel, time, distance and energy are in the leg, the route closure and the ledger, and
    /// the third value returned is the altitude it ended at: the ladder's rungs below it are
    /// already flown (zero when there was no such climb), and the fourth the speed it ended at.
    pub(super) fn at_lift_off(
        model: &'a SegmentMissionModel,
        plan: &ProfilePlan,
        start_mass_kg: f64,
    ) -> Result<(Self, f64, f64, f64), FlyError> {
        let (acceleration, target_tas_m_s) = match plan.climb.first() {
            Some(first) if first.kind == SegmentKind::Takeoff => {
                Self::takeoff_climb(model, plan, first, start_mass_kg)?
            }
            _ => (Acceleration::default(), 0.0),
        };
        let mut trace = Vec::new();
        if let (Some(first), true) = (plan.climb.first(), acceleration.time_s > 0.0) {
            let forces = piece_forces(
                model,
                &acceleration.ledger,
                [first.start_altitude_m, acceleration.altitude_end_m],
                acceleration.distance_m,
                [acceleration.v2_m_s, target_tas_m_s],
                [start_mass_kg, start_mass_kg - acceleration.fuel_kg],
            );
            trace.push(
                FlownStep::acceleration(
                    StepPhase::Takeoff,
                    first.start_altitude_m,
                    (0.0, 0.0),
                    start_mass_kg,
                    &acceleration,
                    target_tas_m_s,
                )
                .with_forces(forces),
            );
        }
        let integrator = Self {
            model,
            mass_kg: start_mass_kg - acceleration.fuel_kg,
            fuel_kg: acceleration.fuel_kg,
            time_s: acceleration.time_s,
            distance_m: acceleration.distance_m,
            // After the take-off climb the aeroplane is at the band's speed.
            previous_tas_m_s: (acceleration.time_s > 0.0).then_some(target_tas_m_s),
            pending_kinetic_j: 0.0,
            ledger: acceleration.ledger,
            rate_limited_steps: 0,
            minimum_climb_rate_m_s: f64::INFINITY,
            minimum_descent_rate_m_s: f64::INFINITY,
            maximum_clean_cl: 0.0,
            allow_route_transition_deficit: true,
            deck_memo: DeckMemo::default(),
            cruise_states: Vec::new(),
            trace,
        };
        Ok((
            integrator,
            acceleration.distance_m,
            acceleration.altitude_end_m,
            target_tas_m_s,
        ))
    }
}

/// Passes at fixing the speed the take-off acceleration ends at.
const RUNG_ITERATIONS: usize = 8;

/// Speed change between two passes below which the end speed is settled, m/s.
const SETTLED_M_S: f64 = 0.01;

impl<'a> Integrator<'a> {
    /// The take-off climb and the speed it ends at.
    ///
    /// The band holds a calibrated airspeed, whose true airspeed rises with the height the
    /// acceleration climbs to, so the acceleration ends at the speed of the ladder rung it ends
    /// in, as the native flight's does (its energy-share speed meets the band's speed at that
    /// height). The end height depends on the target speed, so the pair is iterated.
    fn takeoff_climb(
        model: &'a SegmentMissionModel,
        plan: &ProfilePlan,
        first: &super::Segment,
        start_mass_kg: f64,
    ) -> Result<(Acceleration, f64), FlyError> {
        let mut target_m_s = first.tas_m_s;
        for _ in 0..RUNG_ITERATIONS {
            let acceleration = model.takeoff_acceleration(
                first.start_altitude_m,
                start_mass_kg,
                target_m_s,
                first.vertical_rate_m_s,
            )?;
            let end_m = acceleration.altitude_end_m;
            // The band's own speed law at the end height: the rung of the band it ends in, or the
            // last one extended (the native flight raises the band's top to the end of the
            // acceleration, and holds its calibrated or true airspeed up to there).
            let rung_m_s = plan
                .climb
                .iter()
                .filter(|rung| rung.kind == SegmentKind::Takeoff)
                .find(|rung| rung.end_altitude_m > end_m)
                .or_else(|| {
                    plan.climb
                        .iter()
                        .rfind(|rung| rung.kind == SegmentKind::Takeoff)
                })
                .map_or(target_m_s, |rung| model.scheduled_speed_m_s(rung, end_m));
            if (rung_m_s - target_m_s).abs() <= SETTLED_M_S || acceleration.time_s == 0.0 {
                return Ok((acceleration, target_m_s));
            }
            target_m_s = rung_m_s;
        }
        Err(FuelModelError::NotConverged(format!(
            "the take-off acceleration from the field at {:.0} m did not settle on a ladder rung in {RUNG_ITERATIONS} passes",
            first.start_altitude_m
        ))
        .into())
    }
}
