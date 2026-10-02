// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Aerodynamic coefficients and operating-point provenance in exports.

use crate::full_analysis::AnalysisReport;
use alas_config::AlasConfig;

pub(super) fn database(report: &AnalysisReport, config: &AlasConfig) -> serde_json::Value {
    let req = &config.requirements;
    serde_json::json!({
        "cruise_mach": req.cruise_mach,
        "cruise_altitude_m": req.cruise_altitude_m,
        "cd0_cruise": report.polar_fit.cd0,
        "linear_lift_drag_coefficient": report.polar_fit.c1,
        "k_factor": report.polar_fit.k,
        "oswald_efficiency": report.polar_fit.oswald_e,
        "aspect_ratio": report.polar_fit.aspect_ratio,
        "polar_fit_status": report.polar_fit.status.as_str(),
        "design_point": report.design_point,
        "static_margin": report.static_margin,
        "trimmed_design_point": report.trimmed_design_point,
    })
}
