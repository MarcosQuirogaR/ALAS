// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The planner's max-cruise-thrust ceiling (owner decision 2026-10-14).
//!
//! A cruise level, initial or step, is allowed only where level flight at the
//! maximum-cruise rating is flyable under the band rule of the `cruise_thrust`
//! check ([`ThrustAssessment::flyable`], the scatter band of the engine), on
//! top of the 300 ft/min residual-climb rule of `cruise_levels`. Real
//! operations never plan a level above the thrust ceiling. The model carries
//! the band (`cruise_ceiling_band`; `None` is no ceiling: turboprops, the
//! lapse basis).

use alas_mass::fuel_plan::FuelModelError;
use alas_prop::system::PropulsionRating;

use super::profile::{LegKind, ProfileGeometry};
use super::{deck_error, SegmentMissionModel};
use crate::mdo::residuals_performance::thrust_uncertainty::ThrustAssessment;

impl SegmentMissionModel {
    /// Level flight at the maximum-cruise rating against the drag at
    /// `altitude_m`, `tas_m_s` and `mass_kg`, with the scatter band of the
    /// engine: the planner's ceiling and the `cruise_thrust` check read the
    /// same [`ThrustAssessment`] rule. `None` when no ceiling is configured.
    pub(super) fn cruise_ceiling_assessment(
        &self,
        mass_kg: f64,
        altitude_m: f64,
        tas_m_s: f64,
    ) -> Result<Option<ThrustAssessment>, FuelModelError> {
        let Some(band) = self.cruise_ceiling_band else {
            return Ok(None);
        };
        let flight = self
            .propulsion
            .flight_condition(
                altitude_m,
                tas_m_s,
                self.gravity_at(altitude_m),
                self.isa_deviation_c,
            )
            .map_err(deck_error)?;
        let drag_n = self.drag_n(&flight, mass_kg)?;
        let rated = self
            .propulsion
            .rated_point(flight, PropulsionRating::Cruise)
            .map_err(deck_error)?;
        Ok(Some(ThrustAssessment::new(drag_n, rated.thrust_n, band)))
    }

    /// The ceiling's margin as a rate, m/s (`band_margin V / W`; infinite
    /// without a ceiling), continuous in altitude and mass.
    pub(super) fn cruise_ceiling_margin_m_s(
        &self,
        mass_kg: f64,
        altitude_m: f64,
        tas_m_s: f64,
    ) -> Result<f64, FuelModelError> {
        Ok(self
            .cruise_ceiling_assessment(mass_kg, altitude_m, tas_m_s)?
            .map_or(f64::INFINITY, |a| {
                a.band_margin() * tas_m_s / (mass_kg * self.gravity_at(altitude_m))
            }))
    }

    /// [`Self::cruise_ceiling_margin_m_s`] of the plan at `cruise_m`: the
    /// least over its cruise rungs at the top-of-climb mass.
    pub(super) fn plan_cruise_ceiling_margin_m_s(
        &self,
        geometry: &ProfileGeometry<'_>,
        leg: LegKind,
        toc_mass_kg: f64,
        range_m: f64,
        cruise_m: f64,
    ) -> Option<f64> {
        let plan = geometry.plan_at(leg, range_m, cruise_m).ok()?;
        plan.cruise_rungs
            .iter()
            .try_fold(f64::INFINITY, |least, (tas, _)| {
                self.cruise_ceiling_margin_m_s(toc_mass_kg, plan.cruise_altitude_m, *tas)
                    .ok()
                    .map(|margin| least.min(margin))
            })
    }
}
