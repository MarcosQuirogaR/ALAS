// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The cruise point of the dispatched route.
//!
//! The route is another mission than the report's headline cruise point
//! (`super::cruise_point`), flown from the dispatch plan's own take-off mass.
//! Its point is level flight at the route's mid-cruise mass in the state the
//! route's dispatch flies that mass in: the plan frozen for the dispatch's
//! take-off mass and route (`alas_opt::mdo::plan_for_mission` on the model
//! the dispatch was priced on, `crate::fuel_model::report_route_mission_model`),
//! whose cruise segment starting at or above the mid-cruise mass is the one
//! that mass is flown in (`alas_opt::mdo::cruise_mass::state_flown_at_mass`),
//! at that segment's level, true airspeed and Mach. The design cruise
//! altitude and Mach stand in only when no plan can be made, and the point
//! then carries the reason (`RouteCruisePoint::level_unplanned_reason`).
//!
//! Units: masses kg, distances and altitudes m, speeds m/s; coefficients on
//! the report's reference area.

use super::cruise_condition::{cruise_cl_of_mass, trimmed_table_cd, CruiseCondition};
use super::*;

/// The cruise point of the dispatched route: level flight at the route's own
/// mid-cruise mass, in the state its dispatch flies that mass in.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteCruisePoint {
    /// Departure aerodrome of the route, as configured.
    pub departure: String,
    /// Arrival aerodrome of the route, as configured.
    pub arrival: String,
    /// Still-air distance the plan was priced over, m.
    pub distance_m: Option<f64>,
    /// Mass at brake release of the dispatch plan, kg.
    pub takeoff_mass_kg: f64,
    /// Trip fuel of the dispatch plan, kg.
    pub trip_fuel_kg: f64,
    /// `takeoff_mass_kg - trip_fuel_kg / 2`, kg.
    pub mid_cruise_mass_kg: f64,
    /// Pressure altitude the dispatch flies the mid-cruise mass at, m.
    /// `None`: no plan could be made for the dispatch and the point stands
    /// at the design cruise altitude and Mach.
    pub cruise_altitude_m: Option<f64>,
    /// True airspeed the dispatch flies the mid-cruise mass at, m/s; `None`
    /// with `cruise_altitude_m`.
    pub cruise_tas_m_s: Option<f64>,
    /// Mach of that speed on the dispatch's day; `None` with
    /// `cruise_altitude_m`.
    pub cruise_mach: Option<f64>,
    /// Why no flown state was found, when `cruise_altitude_m` is `None`.
    pub level_unplanned_reason: Option<String>,
    /// Lift coefficient.
    pub cl: f64,
    /// Total drag coefficient, with the trimmed point's trim-drag increment
    /// when the report carries a trimmed point.
    pub cd: f64,
    /// `cl / cd`.
    pub l_over_d: f64,
    /// Whether the lift coefficient lies outside the analysed polar, so the
    /// drag is the polar's end value.
    pub outside_polar: bool,
}

impl RouteCruisePoint {
    /// The exported record.
    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "departure": self.departure,
            "arrival": self.arrival,
            "distance_m": self.distance_m,
            "takeoff_mass_kg": self.takeoff_mass_kg,
            "trip_fuel_kg": self.trip_fuel_kg,
            "mid_cruise_mass_kg": self.mid_cruise_mass_kg,
            "cruise_altitude_m": self.cruise_altitude_m,
            "cruise_tas_m_s": self.cruise_tas_m_s,
            "cruise_mach": self.cruise_mach,
            "level_unplanned_reason": self.level_unplanned_reason,
            "cl": self.cl,
            "cd": self.cd,
            "l_over_d": self.l_over_d,
            "outside_polar": self.outside_polar,
        })
    }

    /// The line a text summary carries on the route's cruise point.
    pub fn summary_line(&self) -> String {
        let level = match (self.cruise_altitude_m, self.cruise_mach) {
            (Some(altitude_m), Some(mach)) => format!(" at {altitude_m:.0} m, Mach {mach:.3}"),
            _ => format!(
                " at the design cruise altitude (no flown level: {})",
                self.level_unplanned_reason.as_deref().unwrap_or("unknown")
            ),
        };
        format!(
            "  Route cruise pt  : {}-{}, {:.0} km from {:.0} kg, mid-cruise {:.0} kg{level}: \
             CL {:.3}, L/D {:.2}{}",
            self.departure,
            self.arrival,
            self.distance_m.map_or(f64::NAN, |m| m / 1_000.0),
            self.takeoff_mass_kg,
            self.mid_cruise_mass_kg,
            self.cl,
            self.l_over_d,
            if self.outside_polar {
                " (outside the analysed polar)"
            } else {
                ""
            },
        )
    }
}

/// The state the route's dispatch flies `mid_cruise_mass_kg` in: the cruise
/// segment of the plan frozen for `takeoff_mass_kg` over `range_m` that
/// starts at or above that mass.
///
/// # Errors
///
/// No route distance, a model or plan that cannot be made, a plan with no
/// cruise segment, or a level that is not a usable altitude, as a
/// description.
fn route_mid_cruise_point(
    config: &AlasConfig,
    report: &AnalysisReport,
    takeoff_mass_kg: f64,
    range_m: Option<f64>,
    mid_cruise_mass_kg: f64,
) -> Result<alas_opt::mdo::cruise_mass::FlownCruisePoint, String> {
    let range_m = range_m.ok_or_else(|| "the dispatch carries no route distance".to_owned())?;
    let model = crate::fuel_model::report_route_mission_model(config, report)?;
    let planned = alas_opt::mdo::plan_for_mission(&model, takeoff_mass_kg, range_m)
        .map_err(|error| error.to_string())?;
    let states = planned.planned_cruise_states(takeoff_mass_kg)?;
    let state = alas_opt::mdo::cruise_mass::state_flown_at_mass(&states, mid_cruise_mass_kg)
        .ok_or_else(|| "the route's plan flies no cruise segment".to_owned())?;
    let point = alas_opt::mdo::cruise_mass::FlownCruisePoint::of_state(&planned, state)?;
    if point.altitude_m > 0.0 {
        Ok(point)
    } else {
        Err(format!("the mid-cruise level {} m is not usable", point.altitude_m))
    }
}

/// The [`RouteCruisePoint`] of the dispatch plan in `loading`, on the polar
/// of `report`; `None` when no plan was priced or the polar cannot supply
/// the point.
///
/// The point is separate from the report's headline cruise point
/// ([`AnalysisReport::cruise_point_basis`]): the route is flown from the
/// plan's own take-off mass, not from the analysed loading.
pub fn route_cruise_point(
    config: &AlasConfig,
    report: &AnalysisReport,
    loading: &crate::feasibility::FuelLoadingAssessment,
) -> Option<RouteCruisePoint> {
    let dispatch = loading.dispatch.as_ref()?;
    let trip_fuel_kg = dispatch.plan?.trip.kg;
    let takeoff_mass_kg = dispatch.takeoff_mass_kg;
    let mid_cruise_mass_kg = takeoff_mass_kg - 0.5 * trip_fuel_kg;
    let flown = route_mid_cruise_point(
        config,
        report,
        takeoff_mass_kg,
        dispatch.route_distance_m,
        mid_cruise_mass_kg,
    );
    if let Err(reason) = &flown {
        tracing::warn!(%reason, "the route cruise point stays at the design altitude");
    }
    let condition = flown.as_ref().ok().map(|point| CruiseCondition {
        altitude_m: point.altitude_m,
        mach: point.mach,
    });
    let at = condition.unwrap_or_else(|| CruiseCondition::design(config));
    let cl = cruise_cl_of_mass(config, report, mid_cruise_mass_kg, at);
    if !(cl.is_finite() && cl > 0.0) {
        return None;
    }
    let (anchor_cl, anchor_cd) = report
        .trimmed_design_point
        .map_or((report.design_point.cl, report.design_point.cd), |point| {
            (point.cl, point.cd)
        });
    let mut point =
        crate::cruise_mass::cruise_point_at_cl(&report.polar, cl, anchor_cl, anchor_cd)?;
    if let Some(cd) = trimmed_table_cd(config, report, point.cl, condition) {
        point.cd = cd;
        point.l_over_d = point.cl / cd;
    }
    Some(RouteCruisePoint {
        departure: config.departure_airport.clone(),
        arrival: config.arrival_airport.clone(),
        distance_m: dispatch.route_distance_m,
        takeoff_mass_kg,
        trip_fuel_kg,
        mid_cruise_mass_kg,
        cruise_altitude_m: condition.map(|at| at.altitude_m),
        cruise_tas_m_s: flown.as_ref().ok().map(|point| point.tas_m_s),
        cruise_mach: condition.map(|at| at.mach),
        level_unplanned_reason: flown.err(),
        cl: point.cl,
        cd: point.cd,
        l_over_d: point.l_over_d,
        outside_polar: point.clamped,
    })
}
