// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The preset's charted design point flown on one report: the design mission.
//!
//! One routine, read by the design database's `characteristics.design_point_range`
//! (the value the matrix range cell scores) and by the acceptance matrix's
//! design-mission grade, so the two cannot price the point differently. The
//! point's payload is flown at the payload-range takeoff-mass limit
//! ([`crate::quick_analysis::corners::chart_corner_masses`]) by the corner rule
//! ([`crate::quick_analysis::corners::payload_point_range`]) under the reserve rule the source
//! states (`reference.design_point.reserve`, the configured policy for every
//! term it leaves unstated), at the cruise level and Mach it states
//! (`reference.design_point.cruise`), and at the long-range cruise speed of
//! its own state when the source quotes the range at long-range cruise
//! ([`crate::quick_analysis::corners::long_range_cruise_config`]).
//! [`DesignPointFlight::fuel_balance`] prices the flown point's fuel plan
//! again on both sides of the flown range. SI units.

use alas_config::{AlasConfig, FuelPolicyConfig, PayloadRangeDesignPoint};
use alas_mass::fuel_policy::plan_fuel;
use alas_opt::mdo::PlannedTrips;

use crate::full_analysis::AnalysisReport;
use crate::quick_analysis::corners::{
    chart_corner_masses, long_range_cruise_config, payload_point_range, range_search_model,
    ChartCornerMasses, LongRangeCruisePoint, PayloadPointRange, CORNER_TOLERANCE_M,
};

/// The design point of a preset run and the model's flight of it.
#[derive(Debug, Clone)]
pub struct DesignPointFlight {
    /// The registered point.
    pub point: PayloadRangeDesignPoint,
    /// The payload-range chart masses it is flown on.
    pub chart: ChartCornerMasses,
    /// The reserve rule flown: the source's stated terms on the configured policy.
    pub policy: FuelPolicyConfig,
    /// The point's payload, or the design payload when it states none, kg.
    pub charted_payload_kg: f64,
    /// The payload flown: the charted one capped at the model's maximum payload, kg.
    pub payload_kg: f64,
    /// The stated cruise level, the ceiling of the range search's ladder, m.
    pub cruise_level_m: Option<f64>,
    /// The configuration flown (stated Mach, stated reserves, long-range speed).
    pub flown_config: AlasConfig,
    /// The long-range cruise speed search, with the source that asks for it,
    /// when the source quotes the range at long-range cruise and states no Mach.
    pub long_range: Option<(&'static str, Result<LongRangeCruisePoint, String>)>,
    /// The flown point, or why it could not be priced.
    pub range: Result<PayloadPointRange, String>,
}

/// Fly the registered design point of `config`'s preset on `report`.
///
/// With `capability_ceiling_m` the trip's initial cruise altitude capability
/// is searched up to it.
///
/// # Errors
///
/// Why no point is flown: not a preset run, an overlay that changes the
/// aircraft type or passenger count (the point is not this aircraft's), no
/// declared point, or no main wing or usable fuel capacity evidence. A point
/// that is set up but cannot be priced is `Ok` with [`DesignPointFlight::range`]
/// carrying the reason.
pub fn fly_design_point(
    report: &AnalysisReport,
    config: &AlasConfig,
    capability_ceiling_m: Option<f64>,
) -> Result<DesignPointFlight, String> {
    let Ok(preset) = alas_config::presets::get(&config.preset) else {
        return Err("not a preset run".to_owned());
    };
    if preset.requirements.aircraft_type != config.requirements.aircraft_type
        || preset.requirements.num_passengers != config.requirements.num_passengers
    {
        return Err("the configuration changes the preset's aircraft type or passenger count: the charted point is not this aircraft's".to_owned());
    }
    let Some(point) = preset.reference.design_point else {
        return Err("the preset declares no charted design point".to_owned());
    };
    let Some(chart) = chart_corner_masses(report, config) else {
        return Err("no main wing or no usable fuel capacity evidence".to_owned());
    };
    let policy = point.reserve.map_or_else(
        || config.fuel_policy.clone(),
        |reserve| reserve.applied_to(&config.fuel_policy),
    );
    let charted_payload_kg = point
        .payload_kg
        .unwrap_or_else(|| config.design_payload_kg().0);
    // A charted payload above the model's achievable maximum is flown at that
    // maximum (the chart's own plateau), as the corners are.
    let payload_kg = charted_payload_kg.min(chart.masses.max_payload_kg);
    // A stated Mach replaces the requirement's, a stated level is the range
    // search's ceiling. Both price this point and its corners only.
    let cruise_level_m = point.cruise.map(|cruise| cruise.altitude_m);
    let mut flown_config = config.with_cruise_mach(point.cruise.and_then(|cruise| cruise.mach));
    flown_config.fuel_policy = policy.clone();
    // A chart quoted at long-range cruise (and stating no speed) is flown at
    // the long-range cruise speed of its own state.
    let long_range = point
        .long_range_cruise
        .filter(|_| point.cruise.and_then(|cruise| cruise.mach).is_none())
        .map(|source| {
            let found = long_range_cruise_config(
                &flown_config,
                report,
                &chart.masses,
                payload_kg,
                &policy,
                cruise_level_m,
            )
            .map(|(flown, found)| {
                flown_config = flown;
                found
            });
            (source, found)
        });
    let range = payload_point_range(
        &flown_config,
        report,
        &chart.masses,
        payload_kg,
        &policy,
        cruise_level_m,
        capability_ceiling_m,
    );
    Ok(DesignPointFlight {
        point,
        chart,
        policy,
        charted_payload_kg,
        payload_kg,
        cruise_level_m,
        flown_config,
        long_range,
        range,
    })
}

/// The reserve-inclusive fuel balance of a flown design point: the takeoff
/// fuel the full plan needs at the flown range and one range-search
/// tolerance beyond it, against the takeoff fuel carried.
#[derive(Debug, Clone, PartialEq)]
pub struct FuelBalance {
    /// Takeoff fuel carried at brake release, kg.
    pub carried_kg: f64,
    /// Takeoff fuel the full plan needs at the flown range, kg.
    pub required_kg: f64,
    /// The range search's own termination tolerance, m
    /// ([`CORNER_TOLERANCE_M`]).
    pub tolerance_m: f64,
    /// Takeoff fuel the full plan needs `tolerance_m` beyond the flown range,
    /// kg, or why that plan cannot be priced.
    pub required_beyond_kg: Result<f64, String>,
}

impl FuelBalance {
    /// The fuel worth of one range-search tolerance at the flown range, kg:
    /// the plan's takeoff fuel `tolerance_m` beyond the flown range less the
    /// plan's at it. `None` when the plan beyond cannot be priced.
    #[must_use]
    pub fn tolerance_fuel_kg(&self) -> Option<f64> {
        self.required_beyond_kg
            .as_ref()
            .ok()
            .map(|beyond_kg| beyond_kg - self.required_kg)
    }

    /// Whether the flown range spends the carried fuel: the plan at the
    /// flown range fits the carried fuel, and the fuel left unspent is no
    /// more than one range-search tolerance worth of fuel. A search that
    /// converged meets both, because it stops with the fitting range and the
    /// first range that does not fit within one tolerance of each other; one
    /// stopped by its iteration cap, or bounded by a leg the burn model could
    /// not price rather than by the fuel, does not.
    #[must_use]
    pub fn closes(&self) -> bool {
        let unspent_kg = self.carried_kg - self.required_kg;
        self.tolerance_fuel_kg().is_some_and(|worth_kg| {
            unspent_kg.is_finite() && unspent_kg >= 0.0 && unspent_kg <= worth_kg
        })
    }
}

impl DesignPointFlight {
    /// The fuel balance of `found` (this flight's priced point, flown on
    /// `report`): the full plan priced again on the flight's configuration,
    /// reserve rule and cruise level at the flown range and one range-search
    /// tolerance ([`CORNER_TOLERANCE_M`]) beyond it.
    ///
    /// # Errors
    ///
    /// When the fuel model cannot be rebuilt or the plan at the flown range
    /// cannot be priced.
    pub fn fuel_balance(
        &self,
        report: &AnalysisReport,
        found: &PayloadPointRange,
    ) -> Result<FuelBalance, String> {
        let model = range_search_model(&self.flown_config, report, self.cruise_level_m)?;
        let trips = PlannedTrips(&model);
        let required_kg = |range_m: f64| {
            plan_fuel(&self.policy, &trips, found.tow_kg, range_m)
                .map_err(|error| error.to_string())
                .and_then(|plan| {
                    if plan.is_finite_and_nonnegative() {
                        Ok(plan.takeoff_fuel_kg())
                    } else {
                        Err(format!(
                            "the plan at {range_m:.0} m is not finite and nonnegative"
                        ))
                    }
                })
        };
        Ok(FuelBalance {
            carried_kg: found.fuel_kg,
            required_kg: required_kg(found.range_m)?,
            tolerance_m: CORNER_TOLERANCE_M,
            required_beyond_kg: required_kg(found.range_m + CORNER_TOLERANCE_M),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn balance(carried_kg: f64, beyond_kg: Result<f64, String>) -> FuelBalance {
        FuelBalance {
            carried_kg,
            required_kg: 10_000.0,
            tolerance_m: CORNER_TOLERANCE_M,
            required_beyond_kg: beyond_kg,
        }
    }

    #[test]
    fn the_balance_closes_only_within_one_search_tolerance_of_fuel() {
        // 6 kg per tolerance: a converged search leaves 0..=6 kg unspent.
        assert!(balance(10_000.0, Ok(10_006.0)).closes());
        assert!(balance(10_006.0, Ok(10_006.0)).closes());
        // A search stopped short leaves more fuel than one tolerance is worth.
        assert!(!balance(10_050.0, Ok(10_006.0)).closes());
        // A plan that needs more than is carried does not fit.
        assert!(!balance(9_990.0, Ok(10_006.0)).closes());
        // A range bounded by an unpriceable leg, not by the fuel, does not close.
        assert!(!balance(10_000.0, Err("not converged".to_owned())).closes());
        assert!(!balance(f64::NAN, Ok(10_006.0)).closes());
    }
}
