// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rows naming the mission of the reported cruise point, and the cruise
//! point of the dispatched route beside it.
//!
//! The headline cruise lift coefficient and L/D are the mid-cruise state of
//! one mission (`alas_pipeline::CruisePointBasis`); the route is flown from
//! the dispatch plan's own take-off mass and is shown as a separate point.
//! Masses in kg, distances in km, coefficients dimensionless.

use alas_pipeline::{AnalysisReport, PipelineResult};

use crate::views::tr;

/// The rows of the selected report's cruise point and of its route's.
pub(super) fn rows(
    result: &PipelineResult,
    report: &AnalysisReport,
) -> Vec<(&'static str, String)> {
    let mut rows = Vec::new();
    let kg = |mass_kg: f64| format!("{mass_kg:.0} kg");
    if let Some(basis) = report.cruise_point_basis() {
        let cl = report
            .trimmed_design_point
            .as_ref()
            .map_or(report.design_point.cl, |point| point.cl);
        rows.push(("Cruise CL", format!("{cl:.3}")));
        rows.push(("Cruise point mission", tr(basis.mission.label())));
        rows.push(("Cruise point take-off mass", kg(basis.takeoff_mass_kg)));
        rows.push(("Cruise point mid-cruise mass", kg(basis.mid_cruise_mass_kg)));
    }
    if let Some(route) =
        alas_pipeline::route_cruise_point(&result.config, report, &result.feasibility.fuel_loading)
    {
        let distance = route
            .distance_m
            .map_or_else(String::new, |m| format!(", {:.0} km", m / 1_000.0));
        rows.push((
            "Route cruise point",
            format!("{}-{}{distance}", route.departure, route.arrival),
        ));
        rows.push(("Route take-off mass", kg(route.takeoff_mass_kg)));
        rows.push(("Route mid-cruise mass", kg(route.mid_cruise_mass_kg)));
        rows.push(("Route cruise CL", format!("{:.3}", route.cl)));
        rows.push(("Route cruise L/D", format!("{:.1}", route.l_over_d)));
    }
    rows
}
