// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The first level of a route-initialised cruise ladder, held to the planner's
//! flyability rule (owner decision 2026-10-14, extended to the ladder).
//!
//! `alas_mission::configure_cruise_legs` moves the end of the initial climb to
//! the first cruise level of the route (about FL370) and keeps the climb speeds
//! of the aircraft's canonical schedule, whose second rung accelerates to the
//! climb Mach. A heavy aircraft then has to make that acceleration at the
//! first level, where it has no excess thrust to spare (A380 at 560 t: 6.8 GJ
//! pending at 8.5 km, a climb energy deficit even at the 0.5 m/s floor rate),
//! so the dispatch clips at a climb edge far below the design mass. Real heavy
//! transports start such a route lower and step-climb when able. The first
//! level is therefore the highest one that satisfies the shared rule of
//! `optimum_step`: the 300 ft/min residual climb at maximum-climb thrust
//! ([`RESIDUAL_CLIMB_M_S`]) at the speed the next rung arrives at, and the
//! max-cruise thrust ceiling band ([`SegmentMissionModel::cruise_ceiling_margin_m_s`]),
//! both at the top-of-climb mass. No new margin: the 300 ft/min is kept while
//! the first segment of the second rung also pays its speed-up, as energy
//! height spread over the height that segment climbs (the plain residual alone passed at 8.5 km for the
//! A380 at 560 t, 0.75 m/s, yet the rung's acceleration was not payable). A
//! level that already qualifies is left exactly as configured.

use super::climb_bands::bands;
use super::cruise_levels::{
    CLIMB_MASS_FRACTION, LEVEL_RESOLUTION_M, LEVEL_SCAN_STEP_M, RESIDUAL_CLIMB_M_S,
};
use super::profile::{LegKind, ProfileGeometry};
use super::SegmentMissionModel;

impl SegmentMissionModel {
    /// The `initial_climb_altitude_fraction` that holds the first level of the
    /// ladder to the flyability rule at `mass_kg`, or `None` when the
    /// configured level qualifies (or the profile has no step climb after it).
    pub(super) fn limited_initial_fraction(
        &self,
        geometry: &ProfileGeometry<'_>,
        leg: LegKind,
        mass_kg: f64,
        range_m: f64,
        cruise_m: f64,
    ) -> Option<f64> {
        let profile = geometry.profile;
        if profile.cruise_2_distance_fraction <= 0.0 {
            return None;
        }
        let (takeoff_top, initial_top, step_one_top, _) =
            bands(profile, self.departure_elevation_m, cruise_m);
        if step_one_top <= initial_top || initial_top <= takeoff_top {
            return None;
        }
        let plan = geometry.plan_at(leg, range_m, cruise_m).ok()?;
        // The speed change opens at the first segment of the second rung: the
        // integrator pays it over that segment's steps (`fly_segment`), so the
        // height that matters is that segment's, not the rung's.
        let first = plan
            .climb
            .iter()
            .position(|segment| segment.start_altitude_m >= initial_top - 1.0)?;
        let (before, after) = (
            plan.climb.get(first.checked_sub(1)?)?,
            plan.climb.get(first)?,
        );
        let (initial_tas, arrival_tas) = (before.tas_m_s, after.tas_m_s);
        let first_height_m = after.end_altitude_m - after.start_altitude_m;
        let rung_height_m = step_one_top - initial_top;
        // Specific kinetic energy (energy height, m) that segment must add.
        let speed_up_m = ((arrival_tas * arrival_tas - initial_tas * initial_tas)
            / (2.0 * self.gravity_at(initial_top)))
        .max(0.0);
        let cruise_tas = plan.cruise_rungs.first()?.0;
        let toc_mass_kg = mass_kg * CLIMB_MASS_FRACTION;
        let margin = |altitude_m: f64| -> f64 {
            // The 300 ft/min must remain while that segment also pays its
            // speed-up over the height it climbs (energy height per metre).
            let segment_m = (first_height_m * (step_one_top - altitude_m) / rung_height_m).max(1.0);
            let required = RESIDUAL_CLIMB_M_S * (1.0 + speed_up_m / segment_m);
            let climb = self
                .residual_climb_point(toc_mass_kg, altitude_m + 0.5 * segment_m, arrival_tas)
                .map(|point| point.rate_m_s - required);
            let ceiling = self.cruise_ceiling_margin_m_s(toc_mass_kg, altitude_m, cruise_tas);
            match (climb, ceiling) {
                (Ok(climb), Ok(ceiling)) => climb.min(ceiling),
                _ => f64::NEG_INFINITY,
            }
        };
        let mut high = (initial_top, margin(initial_top));
        if high.1 >= 0.0 {
            return None;
        }
        let mut low = loop {
            let below_m = (high.0 - LEVEL_SCAN_STEP_M).max(takeoff_top);
            let below = (below_m, margin(below_m));
            if below.1 >= 0.0 {
                break below;
            }
            if below_m <= takeoff_top {
                return Some(takeoff_top / cruise_m);
            }
            high = below;
        };
        while high.0 - low.0 > LEVEL_RESOLUTION_M {
            let middle_m = 0.5 * (low.0 + high.0);
            let middle = (middle_m, margin(middle_m));
            if middle.1 >= 0.0 {
                low = middle;
            } else {
                high = middle;
            }
        }
        let level_m = if high.1.is_finite() {
            low.0 + (high.0 - low.0) * low.1 / (low.1 - high.1)
        } else {
            low.0
        };
        Some(level_m / cruise_m)
    }
}
