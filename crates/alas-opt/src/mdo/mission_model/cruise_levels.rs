// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cruise flight levels: the initial level an aircraft can hold with a
//! residual-climb margin at its top-of-climb mass, and the step climbs it
//! takes as fuel burns off.
//!
//! The configured cruise altitude is a ceiling a dispatcher works below. A
//! heavy transport that is held to it climbs for most of the route at a
//! rating-limited crawl (the A320-200 at 78 t "climbed" for 1,948 of
//! 2,120 nmi) or cannot reach it at all (the B787-9 dispatch failed with a
//! climb energy deficit at 9,602 m). Real operations start at the highest
//! level with a 300 ft/min residual climb and step up as the aircraft
//! lightens; this module flies that rule. The level choice depends on mass
//! only, so a longer route (heavier start) never flies higher.

use alas_mass::fuel_plan::FuelModelError;
use alas_prop::system::PropulsionRating;
use alas_units::FOOT;

use super::integrate::FlyError;
use super::profile::{LegKind, ProfileGeometry, ProfilePlan, Segment, SegmentKind};
use super::{deck_error, SegmentMissionModel};

/// Minimum residual rate of climb at maximum-climb thrust that qualifies a
/// cruise level, 300 ft/min in m/s. This is the thrust-limited "maximum
/// altitude" criterion of transport flight planning: Airbus, "Getting to
/// Grips with Aircraft Performance", Flight Operations Support, 2002,
/// Sec. "Maximum altitude" (the altitude at which maximum climb thrust still
/// gives 300 ft/min); Boeing FCOM performance data use the same residual.
pub(super) const RESIDUAL_CLIMB_M_S: f64 = 300.0 * FOOT / 60.0;

/// Resolution of the initial-level search before the margin's zero is
/// interpolated, m. The level is *not* rounded to a flight level: a 1,000 ft
/// grid makes trip fuel jump with mass, and any jump can give the dispatch
/// closure more than one root (measured with mass-dependent step climbs: two
/// equivalent sizing modes of the B787-9 closed 30 kg apart), so the
/// ceiling-limited level is kept continuous.
const LEVEL_RESOLUTION_M: f64 = 1.0;

/// Spacing of the downward scan for the highest qualifying altitude, m: one
/// flight level, 1,000 ft (ICAO Annex 2, App. 3, table of cruising levels).
/// A qualifying band thinner than a flight level between two failing grid
/// altitudes is not a level that could be filed, so the scan may pass it.
pub(super) const LEVEL_SCAN_STEP_M: f64 = 1_000.0 * FOOT;

/// Same-direction level separation used for step climbs: 2,000 ft up to
/// FL410 inside RVSM airspace and 4,000 ft above it (ICAO Annex 2, App. 3,
/// table of cruising levels, RVSM and non-RVSM columns).
const RVSM_STEP_M: f64 = 2_000.0 * FOOT;
const ABOVE_RVSM_STEP_M: f64 = 4_000.0 * FOOT;
const RVSM_CEILING_M: f64 = 41_000.0 * FOOT;

/// Brake-release to top-of-climb mass ratio used to estimate the mass the
/// initial level must be held at, 0.985: Raymer, "Aircraft Design: A
/// Conceptual Approach", 6th ed., AIAA 2018, Sec. 6.3, historical mission
/// segment weight fraction for climb and accelerate. Only the *choice* of
/// level uses it; the climb itself is integrated.
pub(super) const CLIMB_MASS_FRACTION: f64 = 0.985;

/// Cruise distance between step-climb decisions, m (100 nmi). A
/// discretization of the decision, not a physical constant: a step is taken
/// at most one interval after its level qualifies, which costs at most the
/// specific-air-range gain over that interval.
pub(super) const STEP_DECISION_INTERVAL_M: f64 = 100.0 * 1_852.0;

/// Distance tolerance of the cruise bookkeeping, m.
pub(super) const CRUISE_EPSILON_M: f64 = 1.0e-6;

/// One step-climb level: its cruise rungs and the descent ladder from it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct StepLevel {
    /// Level altitude, m.
    pub altitude_m: f64,
    /// Cruise rungs `(true airspeed, distance fraction)` at this level.
    pub rungs: Vec<(f64, f64)>,
    /// Descent ladder from this level to the arrival field.
    pub descent: Vec<Segment>,
}

/// What the cruise and descent of one pass produced.
pub(super) struct CruiseOutcome {
    /// Flown descent footprint, m.
    pub descent_footprint_m: f64,
    /// Level the descent started from, m.
    pub final_altitude_m: f64,
    /// Step-climb starts as fractions of the cruise distance.
    pub step_positions: Vec<f64>,
    /// Horizontal distance flown between the climb and the descent, m.
    pub cruise_flown_m: f64,
}

/// The step separation above `altitude_m`.
fn step_increment_m(altitude_m: f64) -> f64 {
    if altitude_m + RVSM_STEP_M <= RVSM_CEILING_M + CRUISE_EPSILON_M {
        RVSM_STEP_M
    } else {
        ABOVE_RVSM_STEP_M
    }
}

impl ProfilePlan {
    /// Attribute a climb failure on `segment` to its rung when a gentler
    /// commanded rate on that rung alone can pay for it.
    pub(super) fn tag_climb_failure(&self, segment: &Segment, error: FlyError) -> FlyError {
        let FlyError::Fuel(fuel) = error else {
            return error;
        };
        if !super::adapt::is_climb_rate_recoverable(&fuel) {
            return FlyError::Fuel(fuel);
        }
        let (takeoff_top, initial_top, step_one_top, _) = self.climb_bands;
        let top = segment.end_altitude_m;
        let rung = if segment.kind == SegmentKind::Takeoff || top <= takeoff_top {
            0
        } else if top <= initial_top {
            1
        } else if top <= step_one_top {
            2
        } else {
            3
        };
        FlyError::Climb { rung, error: fuel }
    }

    /// Cruise rungs and altitude of level `index` (0 is the initial level).
    pub(super) fn level(&self, index: usize) -> (f64, &[(f64, f64)], &[Segment]) {
        match index.checked_sub(1).and_then(|i| self.step_levels.get(i)) {
            Some(level) => (level.altitude_m, &level.rungs, &level.descent),
            None => (self.cruise_altitude_m, &self.cruise_rungs, &self.descent),
        }
    }
}

impl ProfileGeometry<'_> {
    /// The step-climb levels above `plan`'s initial level up to `cap_m`,
    /// each with the plan's envelope factor.
    pub(super) fn step_levels(
        &self,
        plan: &ProfilePlan,
        cap_m: f64,
    ) -> Result<Vec<StepLevel>, FuelModelError> {
        let p = self.profile;
        let scale = plan.envelope_scale;
        let mut levels = Vec::new();
        let mut altitude_m = plan.cruise_altitude_m;
        loop {
            let next_m = altitude_m + step_increment_m(altitude_m);
            if next_m > cap_m + CRUISE_EPSILON_M {
                return Ok(levels);
            }
            let mut rungs = Vec::with_capacity(3);
            for (speed, fraction) in [
                (p.cruise_1_air_speed_m_s, p.cruise_1_distance_fraction),
                (p.cruise_2_air_speed_m_s, p.cruise_2_distance_fraction),
                (p.cruise_3_air_speed_m_s, p.cruise_3_distance_fraction),
            ] {
                rungs.push((self.cruise_tas_at(next_m, speed)? * scale, fraction));
            }
            let descent = self.descent_ladder_to(next_m, self.arrival_elevation_m)?;
            let descent = if scale < 1.0 {
                Self::scaled_segments(&descent, scale)?
            } else {
                descent
            };
            levels.push(StepLevel {
                altitude_m: next_m,
                rungs,
                descent,
            });
            altitude_m = next_m;
        }
    }
}

impl SegmentMissionModel {
    /// Residual rate of climb at maximum-climb thrust in level flight at
    /// `altitude_m` and `tas_m_s` for `mass_kg`, m/s:
    /// `(T_MCL - D) V / (m g)`, at constant true airspeed.
    pub(super) fn residual_climb_m_s(
        &self,
        mass_kg: f64,
        altitude_m: f64,
        tas_m_s: f64,
    ) -> Result<f64, FuelModelError> {
        let flight = self
            .propulsion
            .flight_condition(altitude_m, tas_m_s, self.gravity_m_s2, self.isa_deviation_c)
            .map_err(deck_error)?;
        let drag_n = self.drag_n(&flight, mass_kg)?;
        let rated = self
            .propulsion
            .rated_point(flight, PropulsionRating::MaximumClimb)
            .map_err(deck_error)?;
        Ok((rated.thrust_n - drag_n) * tas_m_s / (mass_kg * self.gravity_m_s2))
    }

    /// Specific air range in level flight, m/kg.
    pub(super) fn specific_air_range_m_kg(
        &self,
        mass_kg: f64,
        altitude_m: f64,
        tas_m_s: f64,
    ) -> Result<f64, FuelModelError> {
        Ok(tas_m_s / self.level_fuel_flow_kg_s(mass_kg, altitude_m, tas_m_s)?)
    }

    /// Whether level flight at `altitude_m`, `tas_m_s` keeps the residual
    /// climb margin at `mass_kg`. A deck or polar rejection is a no.
    pub(super) fn level_qualifies(&self, mass_kg: f64, altitude_m: f64, tas_m_s: f64) -> bool {
        self.residual_climb_m_s(mass_kg, altitude_m, tas_m_s)
            .is_ok_and(|rate| rate >= RESIDUAL_CLIMB_M_S)
    }

    /// The initial cruise level for a leg from `mass_kg`: `cap_m` itself if
    /// it qualifies at the estimated top-of-climb mass, otherwise the highest
    /// altitude below it that does; the floor of the geometry when none does
    /// (the integration then reports the typed shortfall).
    ///
    /// The residual climb is not monotone in altitude: below the Mach
    /// crossover the cruise speed rises with altitude at the operating
    /// calibrated-airspeed limit and the drag rise outruns the thrust lapse
    /// (the default widebody at 95 % MTOW keeps 0.98 m/s at 8,000 m and
    /// 1.53 m/s at 10,000 m). A bisection over the whole band then lands on
    /// whichever root its first midpoint selects, so a heavier start could
    /// fly 2,800 m lower than a 2 % lighter one. The band is scanned down
    /// from the cap on the flight-level grid instead and only the highest
    /// qualifying grid interval is refined. The margin falls with mass at
    /// every altitude, so the qualifying set shrinks with mass and the level
    /// found never rises with it.
    pub(super) fn initial_cruise_level_m(
        &self,
        geometry: &ProfileGeometry<'_>,
        leg: LegKind,
        mass_kg: f64,
        range_m: f64,
        cap_m: f64,
    ) -> f64 {
        let toc_mass_kg = mass_kg * CLIMB_MASS_FRACTION;
        let floor_m = geometry.floor_cruise_m(leg).min(cap_m);
        // Residual-climb margin over the criterion, m/s, at the cruise speed
        // and at the speed the top climb rung arrives at (a calibrated
        // schedule without a Mach crossover flies faster than cruise); a
        // rejected plan or deck point counts as no margin at all.
        let margin = |cruise_m: f64| -> f64 {
            geometry
                .plan_at(leg, range_m, cruise_m)
                .ok()
                .and_then(|plan| {
                    let top_m = plan.cruise_altitude_m;
                    let cruise = plan.cruise_rungs.first()?.0;
                    let climb = plan.climb.last().map_or(cruise, |segment| segment.tas_m_s);
                    let at_cruise = self.residual_climb_m_s(toc_mass_kg, top_m, cruise).ok()?;
                    let at_climb = self.residual_climb_m_s(toc_mass_kg, top_m, climb).ok()?;
                    Some(at_cruise.min(at_climb) - RESIDUAL_CLIMB_M_S)
                })
                .unwrap_or(f64::NEG_INFINITY)
        };
        let mut high = (cap_m, margin(cap_m));
        if high.1 >= 0.0 {
            return cap_m;
        }
        // Highest qualifying grid altitude, scanning down from the cap.
        let mut low = loop {
            let below_m = (high.0 - LEVEL_SCAN_STEP_M).max(floor_m);
            let below = (below_m, margin(below_m));
            if below.1 >= 0.0 {
                break below;
            }
            if below_m <= floor_m {
                return floor_m;
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
        // Interpolate the zero of the margin inside the final bracket, so the
        // level is a continuous function of mass within a grid interval.
        if high.1.is_finite() {
            low.0 + (high.0 - low.0) * low.1 / (low.1 - high.1)
        } else {
            low.0
        }
    }
}
