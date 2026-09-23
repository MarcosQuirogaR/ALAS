// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Baseline-versus-optimized comparison for `optimization_preset_audit`,
//! including the design gross mass each side's fuel closure refers to.

use alas_pipeline::PipelineResult;
use serde_json::{json, Value};

/// The nominal design and the delivered design, side by side.
///
/// `compare_baseline` is on for every run this harness starts, so an
/// optimization run analyses the registered preset's own design vector at
/// reporting fidelity as well as the search's. Without both columns a row
/// says what the optimizer produced but not what it produced it *against*,
/// and an improvement claim cannot be checked.
///
/// Units and frame are the run's own, restated in `metric_conventions`:
/// lengths in metres in body axes with the origin at the fuselage nose and
/// `+x` aft, masses in kilograms, `l_over_d` dimensionless at the reported
/// design point. Deltas are optimized minus baseline, so a negative fuel
/// delta is less fuel.
pub(super) fn baseline_comparison(result: &PipelineResult) -> Value {
    let metrics = |report: Option<&alas_pipeline::AnalysisReport>| {
        report.map_or_else(
            || json!(null),
            |report| {
                json!({
                    "cruise_l_over_d": report.design_point.l_over_d,
                    "trimmed_l_over_d": report.trimmed_design_point.as_ref().map(|point| point.l_over_d),
                    "x_neutral_point_m": report.x_neutral_point,
                    "static_margin": report.static_margin,
                    "cd0": report.polar_fit.cd0,
                    "fuel_kg": report.component_masses.get("Fuel").copied(),
                    "design_gross_mass_kg": design_gross_mass_kg(report),
                    "payload_kg": report.component_masses.get("Payload").copied(),
                    "wing_area_m2": report.airplane.s_ref,
                })
            },
        )
    };
    let delta = |extract: fn(&alas_pipeline::AnalysisReport) -> f64| match (
        result.baseline_analysis.as_ref(),
        result.optimized_report.as_ref(),
    ) {
        (Some(baseline), Some(optimized)) => json!(extract(optimized) - extract(baseline)),
        _ => json!(null),
    };
    json!({
        "baseline": metrics(result.baseline_analysis.as_ref()),
        "optimized": metrics(result.optimized_report.as_ref()),
        "delta": {
            "cruise_l_over_d": delta(|report| report.design_point.l_over_d),
            "x_neutral_point_m": delta(|report| report.x_neutral_point),
            "static_margin": delta(|report| report.static_margin),
            "fuel_kg": if same_mass_basis(result) { delta(|report| report.component_masses.get("Fuel").copied().unwrap_or(f64::NAN)) } else { json!(null) },
            "design_gross_mass_kg": delta(design_gross_mass_kg),
            "wing_area_m2": delta(|report| report.airplane.s_ref),
        },
        "delta_convention": "optimized minus baseline, in the units of the same field above; fuel_kg is the MTOW - MZFW closure at each side's design_gross_mass_kg, so its delta is null (NotComparable) when those masses differ by more than 1 kg",
        "comparable": result.baseline_analysis.is_some() && result.optimized_report.is_some(),
    })
}

/// Design gross mass a report's breakdown closes to, kg: `Fuel` is the signed
/// `MTOW - MZFW` closure, so the component masses sum to the sizing mass.
pub(super) fn design_gross_mass_kg(report: &alas_pipeline::AnalysisReport) -> f64 {
    report.component_masses.values().sum()
}

/// Whether both sides' fuel closures refer to the same design gross mass.
fn same_mass_basis(result: &PipelineResult) -> bool {
    match (
        result.baseline_analysis.as_ref(),
        result.optimized_report.as_ref(),
    ) {
        (Some(b), Some(o)) => (design_gross_mass_kg(b) - design_gross_mass_kg(o)).abs() <= 1.0,
        _ => false,
    }
}
