// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Initial cruise altitude (ICA) capability at the top-of-climb mass the
//! mission itself reaches.
//!
//! The criterion is the planner's: the highest level at which maximum-climb
//! thrust still gives a 300 ft/min residual climb
//! ([`RESIDUAL_CLIMB_M_S`], Airbus "Getting to Grips with Aircraft
//! Performance", 2002, "Maximum altitude"). The level *choice* of the planner
//! estimates the top-of-climb mass with a fixed fraction of the brake-release
//! mass; here the climb to the level is integrated instead (take-off roll and
//! climb ladder, exactly as the flown trip burns them), so the mass the
//! criterion is held at is `m_BR - climb fuel`.
//!
//! The climb is the aircraft's, not the profile's guidance target: a rung whose
//! commanded rate the maximum-climb rating cannot hold at a level is flown at
//! the largest scale of that rate it can hold there (the closure's own rung
//! revision, [`super::adapt::climb_rate_revision`]), so a profile with steep
//! commanded rates (a route-derived one) does not make the levels it cannot
//! climb to at those rates unreachable. With the commanded rates only, the
//! A380-800's capability fell to 3,077 m under a route-derived profile while
//! it flew 9,076 m.
//!
//! The level is also bounded by the 1.3 g buffet margin at that mass where the
//! model carries an onset lift ([`SegmentMissionModel::with_buffet_onset_cl`];
//! Airbus 2002, "Maximum altitude": the maximum altitude is the lowest of the
//! thrust, buffet and certified limits). The published initial levels of the
//! 747-400 and DC-10 are their 1.3 g levels.

use super::adapt::{
    climb_rate_revision, CLIMB_RATE_BRACKET_FACTOR, CLIMB_RATE_BRACKET_STEPS,
    CLIMB_RATE_RESOLUTION, MINIMUM_CLIMB_RATE_M_S,
};
use super::cruise_levels::{highest_qualifying_m, ClimbPoint, RESIDUAL_CLIMB_M_S};
use super::integrate::FlyError;
use super::profile::{LegKind, ProfileGeometry};
use super::SegmentMissionModel;

/// The 300 ft/min criterion at one initial cruise level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct InitialCruisePoint {
    /// Level the plan cruises at (the top of its climb ladder), m.
    pub(crate) altitude_m: f64,
    /// Fuel from brake release to the top of climb, kg.
    pub(crate) climb_fuel_kg: f64,
    /// Mass at the top of climb, kg.
    pub(crate) toc_mass_kg: f64,
    /// The governing top-of-climb state at maximum-climb thrust.
    pub(crate) point: ClimbPoint,
    /// Margin over the 1.3 g buffet floor at the top-of-climb mass, as a
    /// rate, m/s (infinite without a buffet bound).
    pub(crate) buffet_margin_m_s: f64,
}

impl InitialCruisePoint {
    /// Margin over the 300 ft/min criterion, m/s.
    pub(crate) fn margin_m_s(&self) -> f64 {
        self.point.rate_m_s - RESIDUAL_CLIMB_M_S
    }

    /// Margin of the level as an initial cruise level, m/s: the least of the
    /// 300 ft/min margin and the buffet margin.
    pub(crate) fn level_margin_m_s(&self) -> f64 {
        self.margin_m_s().min(self.buffet_margin_m_s)
    }
}

impl SegmentMissionModel {
    /// The criterion at the trip level `cruise_m` for a `range_m` trip from
    /// brake release at `mass_kg`, with the climb flown at the rate
    /// `scales` of the closure's plan.
    ///
    /// # Errors
    ///
    /// The plan does not fit, or the climb, deck or drag rejects it, as text.
    pub(crate) fn initial_cruise_point(
        &self,
        mass_kg: f64,
        cruise_m: f64,
        range_m: f64,
        scales: [f64; 3],
    ) -> Result<InitialCruisePoint, String> {
        self.initial_cruise_point_in(scales, mass_kg, cruise_m, range_m)
    }

    /// The highest trip level, up to `cap_m` or the route's planned cruise
    /// level if higher, that holds the criterion at the flown top-of-climb
    /// mass, m; `None` when not even the floor of the climb geometry does.
    ///
    /// # Errors
    ///
    /// The route has no plan, as text.
    pub(crate) fn achievable_initial_cruise_m(
        &self,
        mass_kg: f64,
        range_m: f64,
        scales: [f64; 3],
        cap_m: f64,
    ) -> Result<Option<f64>, String> {
        let profile = super::adapt::climb_rate_revision(&self.profile, scales);
        let geometry = ProfileGeometry {
            profile: &profile,
            ..self.geometry()
        };
        let cap_m = geometry
            .plan(LegKind::Trip, range_m)
            .map_err(|error| error.to_string())?
            .planned_cruise_m
            .max(cap_m);
        let floor_m = geometry.floor_cruise_m(LegKind::Trip).min(cap_m);
        let margin = |cruise_m: f64| {
            self.initial_cruise_point_in(scales, mass_kg, cruise_m, range_m)
                .map_or(f64::NEG_INFINITY, |point| point.level_margin_m_s())
        };
        let level_m = highest_qualifying_m(floor_m, cap_m, margin);
        Ok((level_m > floor_m || margin(floor_m) >= 0.0).then_some(level_m))
    }

    /// The initial cruise altitude capability of a trip of `range_m` from
    /// brake release at `mass_kg`, searched up to `cap_m`, with the climb
    /// flown at the configured rates ([`Self::achievable_initial_cruise_m`]
    /// without a closure's rate revision), m; `None` when not even the floor
    /// of the climb geometry holds the criterion.
    ///
    /// # Errors
    ///
    /// The route has no plan, as text.
    pub fn initial_cruise_capability_m(
        &self,
        mass_kg: f64,
        range_m: f64,
        cap_m: f64,
    ) -> Result<Option<f64>, String> {
        self.achievable_initial_cruise_m(mass_kg, range_m, [1.0; 3], cap_m)
    }

    /// The climb fuel to `cruise_m` with the climb `scales`, as a
    /// [`FlyError`] when that climb fails.
    fn climb_fuel_at_scales(
        &self,
        scales: [f64; 3],
        mass_kg: f64,
        cruise_m: f64,
        range_m: f64,
    ) -> Result<f64, FlyError> {
        let profile = climb_rate_revision(&self.profile, scales);
        let geometry = ProfileGeometry {
            profile: &profile,
            ..self.geometry()
        };
        let plan = geometry
            .plan_at(LegKind::Trip, range_m, cruise_m)
            .map_err(FlyError::Fuel)?;
        self.climb_fuel_kg(mass_kg, &plan)
    }

    /// The climb `scales` with which the climb to `cruise_m` is flown: the
    /// given ones, each rung the rating refuses lowered to the largest scale
    /// it clears (geometric bracket, then bisection to
    /// [`CLIMB_RATE_RESOLUTION`], as the closure revises a rung).
    fn flyable_climb_scales(
        &self,
        mut scales: [f64; 3],
        mass_kg: f64,
        cruise_m: f64,
        range_m: f64,
    ) -> Result<[f64; 3], String> {
        let rates = [
            self.profile.initial_climb_rate_m_s,
            self.profile.step_climb_1_rate_m_s,
            self.profile.step_climb_2_rate_m_s,
        ];
        let mut revised = [false; 3];
        loop {
            let rung = match self.climb_fuel_at_scales(scales, mass_kg, cruise_m, range_m) {
                Ok(_) => return Ok(scales),
                Err(FlyError::Climb { rung, .. })
                    if (1..=3).contains(&rung) && !revised[rung - 1] =>
                {
                    rung
                }
                Err(error) => return Err(format!("the climb to {cruise_m:.0} m fails: {error:?}")),
            };
            let index = rung - 1;
            let floor = (MINIMUM_CLIMB_RATE_M_S / rates[index]).clamp(0.0, 1.0);
            let clears = |scale: f64| {
                let mut trial = scales;
                trial[index] = scale;
                match self.climb_fuel_at_scales(trial, mass_kg, cruise_m, range_m) {
                    Ok(_) => true,
                    Err(FlyError::Climb { rung: other, .. }) => other != rung,
                    Err(_) => false,
                }
            };
            let mut bracket = None;
            let (mut failed, mut scale) = (scales[index], scales[index]);
            for _ in 0..CLIMB_RATE_BRACKET_STEPS {
                scale = (scale * CLIMB_RATE_BRACKET_FACTOR).max(floor);
                if clears(scale) {
                    bracket = Some((scale, failed));
                    break;
                }
                failed = scale;
                if scale <= floor {
                    break;
                }
            }
            let Some((mut low, mut high)) = bracket else {
                return Err(format!(
                    "the climb to {cruise_m:.0} m fails on climb rung {rung} at the gentlest \
                     commanded rate"
                ));
            };
            while high - low > CLIMB_RATE_RESOLUTION {
                let middle = 0.5 * (low + high);
                if clears(middle) {
                    low = middle;
                } else {
                    high = middle;
                }
            }
            scales[index] = low;
            revised[index] = true;
        }
    }

    fn initial_cruise_point_in(
        &self,
        scales: [f64; 3],
        mass_kg: f64,
        cruise_m: f64,
        range_m: f64,
    ) -> Result<InitialCruisePoint, String> {
        let scales = self.flyable_climb_scales(scales, mass_kg, cruise_m, range_m)?;
        let profile = climb_rate_revision(&self.profile, scales);
        let geometry = ProfileGeometry {
            profile: &profile,
            ..self.geometry()
        };
        let plan = geometry
            .plan_at(LegKind::Trip, range_m, cruise_m)
            .map_err(|error| error.to_string())?;
        let climb_fuel_kg = self
            .climb_fuel_kg(mass_kg, &plan)
            .map_err(|error| format!("the climb to {cruise_m:.0} m fails: {error:?}"))?;
        let toc_mass_kg = mass_kg - climb_fuel_kg;
        let point = self
            .plan_residual_climb_point(&geometry, LegKind::Trip, toc_mass_kg, range_m, cruise_m)
            .ok_or_else(|| format!("no top-of-climb state at {cruise_m:.0} m"))?;
        // The buffet margin is the cruise's: the onset lift is read at the cruise Mach, so it is
        // held at the speeds the level is cruised at, not at the climb speed that may govern the
        // residual climb (at the climb CAS below the crossover no level of a DC-10 held 1.3 g and
        // the capability was reported unavailable).
        let buffet_margin_m_s = plan
            .cruise_rungs
            .iter()
            .map(|(tas_m_s, _)| {
                self.buffet_margin_m_s(toc_mass_kg, plan.cruise_altitude_m, *tas_m_s)
            })
            .reduce(f64::min)
            .unwrap_or_else(|| {
                self.buffet_margin_m_s(toc_mass_kg, plan.cruise_altitude_m, point.tas_m_s)
            });
        Ok(InitialCruisePoint {
            altitude_m: plan.cruise_altitude_m,
            climb_fuel_kg,
            toc_mass_kg,
            point,
            buffet_margin_m_s,
        })
    }
}
