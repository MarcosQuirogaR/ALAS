// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The level deceleration at the 10,000 ft speed restriction in a descent.
//!
//! A descent holds its own, faster, schedule above 10,000 ft and 250 kt indicated below it
//! (14 CFR 91.117(a)). The aeroplane meets the limit level, at the restriction altitude, at
//! flight idle, as the native flight does (a `speed_change_to_*` level leg); the change is
//! charged by the energy method with the deck's own idle point, clean. It starts from the speed
//! the descent schedule gives at the restriction altitude, not the mid-rung speed the sub-rung
//! above it is flown at. The speed change of a climb at the same altitude is the path piece of
//! [`super::climb_entry`]. The 250 kt limit is applied to every aircraft (including the military
//! A400M) and without the 14 CFR 91.117(d) minimum-safe-airspeed exception.

use alas_config::mission::{SpeedReference, SPEED_RESTRICTION_ALTITUDE_M};
use alas_mass::fuel_plan::FuelModelError;

use super::trace::{piece_forces, FlownStep, StepPhase};
use super::{FlyError, Integrator, Segment, SegmentKind, SegmentMissionModel};

/// Two altitudes closer than this are the same altitude, m.
const ALTITUDE_IDENTITY_M: f64 = 1.0e-6;
/// Two speeds closer than this are the same speed, m/s.
const SPEED_IDENTITY_M_S: f64 = 0.05;

impl SegmentMissionModel {
    /// The true airspeed the descent rung `segment` has at `altitude_m` on its own schedule: the
    /// rung holds one calibrated airspeed (its midpoint value of true airspeed, `segment.tas_m_s`,
    /// gives it) or, for a literal true-airspeed profile, one true airspeed.
    pub(in crate::mdo::mission_model) fn scheduled_speed_m_s(
        &self,
        segment: &Segment,
        altitude_m: f64,
    ) -> f64 {
        if self.profile.climb_descent_speed_reference != SpeedReference::CalibratedAirspeed {
            return segment.tas_m_s;
        }
        let isa = self.isa_deviation_c;
        let mid = alas_atmo::us1976_compute_values(
            0.5 * (segment.start_altitude_m + segment.end_altitude_m),
            isa,
        );
        let at = alas_atmo::us1976_compute_values(altitude_m, isa);
        alas_atmo::airspeed::calibrated_from_true(
            segment.tas_m_s,
            mid.pressure_pa,
            mid.temperature_k,
        )
        .and_then(|cas_m_s| {
            alas_atmo::airspeed::true_from_calibrated(cas_m_s, at.pressure_pa, at.temperature_k)
        })
        .unwrap_or(segment.tas_m_s)
    }
}

impl Integrator<'_> {
    /// If `previous` ends at the restriction altitude where `next` starts and `next` is slower,
    /// fly the level deceleration between them (at flight idle) and return its horizontal
    /// distance, m; zero otherwise (also where idle cannot lose the speed level).
    pub(in crate::mdo::mission_model) fn decelerate_through_restriction(
        &mut self,
        previous: Option<&Segment>,
        next: &Segment,
    ) -> Result<f64, FlyError> {
        let Some(previous) = previous else {
            return Ok(0.0);
        };
        let at_restriction = |altitude_m: f64| {
            (altitude_m - SPEED_RESTRICTION_ALTITUDE_M).abs() < ALTITUDE_IDENTITY_M
        };
        if previous.kind != SegmentKind::Descent
            || next.kind != SegmentKind::Descent
            || !at_restriction(previous.end_altitude_m)
            || !at_restriction(next.start_altitude_m)
            || next.tas_m_s >= previous.tas_m_s - SPEED_IDENTITY_M_S
        {
            return Ok(0.0);
        }
        let from_m_s = self
            .model
            .scheduled_speed_m_s(previous, SPEED_RESTRICTION_ALTITUDE_M);
        if from_m_s <= next.tas_m_s {
            return Ok(0.0);
        }
        let deceleration = self.model.level_deceleration(
            self.mass_kg,
            SPEED_RESTRICTION_ALTITUDE_M,
            from_m_s,
            next.tas_m_s,
        )?;
        let Some(deceleration) = deceleration else {
            return Ok(0.0);
        };
        let start_mass_kg = self.mass_kg;
        let (start_time_s, start_range_m) = (self.time_s, self.distance_m);
        self.mass_kg -= deceleration.fuel_kg;
        self.fuel_kg += deceleration.fuel_kg;
        self.time_s += deceleration.time_s;
        self.distance_m += deceleration.distance_m;
        self.ledger.propulsive_work_j += deceleration.ledger.propulsive_work_j;
        self.ledger.drag_work_j += deceleration.ledger.drag_work_j;
        self.ledger.kinetic_energy_j += deceleration.ledger.kinetic_energy_j;
        self.pending_kinetic_j = 0.0;
        self.previous_tas_m_s = Some(next.tas_m_s);
        let forces = piece_forces(
            self.model,
            &deceleration.ledger,
            [SPEED_RESTRICTION_ALTITUDE_M; 2],
            deceleration.distance_m,
            [from_m_s, next.tas_m_s],
            [start_mass_kg, self.mass_kg],
        );
        self.trace.push(
            FlownStep::level_acceleration(
                StepPhase::Descent,
                SPEED_RESTRICTION_ALTITUDE_M,
                [start_time_s, self.time_s],
                [start_range_m, self.distance_m],
                [from_m_s, next.tas_m_s],
                [start_mass_kg, self.mass_kg],
                self.model.isa_deviation_c,
            )
            .with_forces(forces),
        );
        if !self.mass_kg.is_finite() || self.mass_kg <= 0.0 {
            return Err(FuelModelError::NotConverged(
                "level deceleration at the speed restriction left no mass".to_owned(),
            )
            .into());
        }
        Ok(deceleration.distance_m)
    }
}
