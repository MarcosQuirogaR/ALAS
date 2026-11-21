// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The planner's max-cruise-thrust ceiling.
//!
//! A cruise level, initial or step, is allowed only where level flight at the
//! maximum-cruise rating is flyable under the band rule of the `cruise_thrust`
//! check ([`ThrustAssessment::flyable`], the scatter band of the engine), on
//! top of the 300 ft/min residual-climb rule of `cruise_levels`. Real
//! operations never plan a level above the thrust ceiling. The model carries
//! the band (`cruise_ceiling_band`; `None` is no ceiling: turboprops, the
//! lapse basis).
//!
//! The same levels are held under the buffet ceiling: at the cruise speed and
//! the current mass, a load factor of 1.3 g must remain before buffet onset,
//! `n = CL_b q S / W >= 1.3` (the 0.3 g operating margin that, with the
//! 300 ft/min residual climb and the certified maximum altitude, sets the
//! maximum recommended altitude: Airbus, *Getting to Grips with Aircraft
//! Performance*, Airbus Customer Services, 2002, "Maximum altitude"; Obert,
//! *Aerodynamic Design of Transport Aircraft*, IOS Press, 2009, ch. 28
//! p. 371). An airline does not plan a level above it, initial or step.
//! `CL_b` is the wing's onset lift coefficient at the cruise Mach number,
//! carried by the model (`buffet_onset_cl`, set by the sizing closure from
//! the same onset function the `buffet_margin` check reads; `None` is no
//! buffet bound: a Mach below the wave-drag onset or no onset table).

use alas_mass::fuel_plan::FuelModelError;
use alas_prop::system::PropulsionRating;

use super::profile::{LegKind, ProfileGeometry};
use super::{deck_error, SegmentMissionModel};
use crate::mdo::residuals_buffet::BUFFET_LOAD_FACTOR;
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

    /// The ceilings' margin as a rate, m/s, continuous in altitude and mass:
    /// the lesser of the thrust ceiling's (`band_margin V / W`) and the buffet
    /// ceiling's ([`Self::buffet_margin_m_s`]); infinite without either.
    pub(super) fn cruise_ceiling_margin_m_s(
        &self,
        mass_kg: f64,
        altitude_m: f64,
        tas_m_s: f64,
    ) -> Result<f64, FuelModelError> {
        let thrust = self
            .cruise_ceiling_assessment(mass_kg, altitude_m, tas_m_s)?
            .map_or(f64::INFINITY, |a| {
                a.band_margin() * tas_m_s / (mass_kg * self.gravity_at(altitude_m))
            });
        Ok(thrust.min(self.buffet_margin_m_s(mass_kg, altitude_m, tas_m_s)))
    }

    /// Bound the planned levels (initial and step) by the buffet ceiling of
    /// a wing whose onset lift coefficient at the cruise Mach is `cl_b`
    /// (`None`: no buffet bound).
    pub fn with_buffet_onset_cl(mut self, cl_b: Option<f64>) -> Self {
        self.buffet_onset_cl = cl_b.filter(|cl| cl.is_finite() && *cl > 0.0);
        self
    }

    /// Load factor to buffet onset in level flight at `altitude_m`,
    /// `tas_m_s` and `mass_kg`, `CL_b q S / W` (q from the ambient density
    /// at the model's ISA deviation); `None` without a buffet bound.
    pub fn buffet_load_factor(&self, mass_kg: f64, altitude_m: f64, tas_m_s: f64) -> Option<f64> {
        let cl_b = self.buffet_onset_cl?;
        let density =
            alas_atmo::us1976_compute_values(altitude_m, self.isa_deviation_c).density_kg_m3;
        let q_pa = 0.5 * density * tas_m_s * tas_m_s;
        Some(cl_b * q_pa * self.wing_area_m2 / (mass_kg * self.gravity_at(altitude_m)))
    }

    /// The buffet ceiling's margin as a rate, m/s: the lift reserve over the
    /// 1.3 g floor, `(n / 1.3 - 1) V`, i.e. `(L_b / 1.3 - W) V / W` with
    /// `L_b` the onset lift, on the scale of the other ceiling margins;
    /// infinite without a buffet bound.
    pub(super) fn buffet_margin_m_s(&self, mass_kg: f64, altitude_m: f64, tas_m_s: f64) -> f64 {
        self.buffet_load_factor(mass_kg, altitude_m, tas_m_s)
            .map_or(f64::INFINITY, |n| (n / BUFFET_LOAD_FACTOR - 1.0) * tas_m_s)
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
