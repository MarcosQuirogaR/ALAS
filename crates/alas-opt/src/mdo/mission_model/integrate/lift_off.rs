// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The integrator's state at the start of a planned climb ladder.

use super::{DeckMemo, FlyError, Integrator, ProfilePlan, SegmentKind, SegmentMissionModel};
use crate::mdo::mission_model::takeoff_acceleration::Acceleration;

impl<'a> Integrator<'a> {
    /// The integrator at the start of `plan` for an aeroplane of
    /// `start_mass_kg` at the top of the field, and the horizontal distance
    /// already flown, m.
    ///
    /// The ladder's take-off band is flown at the take-off climb speed; the
    /// level acceleration to it from the safety speed at the screen height is
    /// charged first (see `takeoff_acceleration`), so its fuel, time,
    /// distance and energy are in the leg, the route closure and the ledger.
    pub(super) fn at_lift_off(
        model: &'a SegmentMissionModel,
        plan: &ProfilePlan,
        start_mass_kg: f64,
    ) -> Result<(Self, f64), FlyError> {
        let acceleration = match plan.climb.first() {
            Some(first) if first.kind == SegmentKind::Takeoff => {
                model.takeoff_acceleration(first.start_altitude_m, start_mass_kg, first.tas_m_s)?
            }
            _ => Acceleration::default(),
        };
        let integrator = Self {
            model,
            mass_kg: start_mass_kg - acceleration.fuel_kg,
            fuel_kg: acceleration.fuel_kg,
            time_s: acceleration.time_s,
            distance_m: acceleration.distance_m,
            previous_tas_m_s: None,
            pending_kinetic_j: 0.0,
            ledger: acceleration.ledger,
            rate_limited_steps: 0,
            minimum_climb_rate_m_s: f64::INFINITY,
            minimum_descent_rate_m_s: f64::INFINITY,
            maximum_clean_cl: 0.0,
            allow_route_transition_deficit: true,
            deck_memo: DeckMemo::default(),
            cruise_states: Vec::new(),
        };
        Ok((integrator, acceleration.distance_m))
    }
}
