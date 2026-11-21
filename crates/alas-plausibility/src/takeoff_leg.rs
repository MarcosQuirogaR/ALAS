// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The take-off climb and the 10,000 ft speed changes, native against priced, as report lines
//! (the legs and the rules that judge them are in [`super::leg_rules`]).

use alas_mission::MissionResult;
use alas_pipeline::{AnalysisReport, PipelineResult};

use super::leg_rules::{
    native_restriction_legs, native_takeoff_leg, priced_restriction_legs, priced_takeoff_leg,
    LegFacts,
};
use super::mission::MissionFacts;
use super::mission_priced::priced_flight;

impl LegFacts {
    /// The report line of the leg (printer; the rules only judge the numbers).
    pub fn line(&self, label: &str) -> String {
        format!(
            "{label} v {:.1}->{:.1} m/s h {:.0}->{:.0} m t {:.1} s r {:.0} m fuel {:.1} kg",
            self.v0_m_s,
            self.v1_m_s,
            self.h0_m,
            self.h1_m,
            self.time_s,
            self.distance_m,
            self.fuel_kg
        )
    }
}

/// The report lines of the take-off climb and the speed changes at the limit for `preset`:
/// `TOACC` for the take-off climb, `RESTR` for the climb side and the descent side, each with the
/// native leg and the priced leg (empty when the priced model cannot be built or flown).
pub fn leg_lines(
    preset: &str,
    mission: &MissionResult,
    result: &PipelineResult,
    report: Option<&AnalysisReport>,
) -> Vec<String> {
    let native = MissionFacts::from_result(mission, result.config.requirements.cruise_mach);
    let mut lines = Vec::new();
    let steps = report
        .and_then(|report| {
            alas_pipeline::fuel_model::report_route_mission_model(&result.config, report).ok()
        })
        .and_then(|model| priced_flight(&model, &native).ok());
    let priced_takeoff = steps.as_deref().and_then(priced_takeoff_leg);
    let (priced_climb, priced_descent) = steps
        .as_deref()
        .map_or((None, None), priced_restriction_legs);
    let (native_climb, native_descent) = native_restriction_legs(&native);
    for (tag, model, leg) in [
        ("TOACC", "native", native_takeoff_leg(&native)),
        ("TOACC", "priced", priced_takeoff),
        ("RESTR climb", "native", native_climb),
        ("RESTR climb", "priced", priced_climb),
        ("RESTR descent", "native", native_descent),
        ("RESTR descent", "priced", priced_descent),
    ] {
        if let Some(leg) = leg {
            lines.push(leg.line(&format!("{tag} {preset} {model}")));
        }
    }
    lines
}

/// The state the lift coefficient of the native take-off band is computed at, and the reference
/// area it implies (`S = m g cos(gamma) / (q CL)`), against the priced model's wing area: the two
/// models must use one reference area.
pub fn reference_area_line(
    preset: &str,
    mission: &MissionResult,
    result: &PipelineResult,
    report: Option<&AnalysisReport>,
) -> Option<String> {
    let c = &mission
        .segments
        .iter()
        .find(|s| s.spec.tag == "takeoff")?
        .conditions;
    let (v, rho, m, cl) = (
        c.velocity_m_s[0],
        c.density_kg_m3[0],
        c.total_mass_kg[0],
        c.lift_coefficient[0],
    );
    let gamma = (c.velocity_vector_m_s[0][2] / v).asin();
    let implied = m * 9.80665 * gamma.cos() / (0.5 * rho * v * v * cl);
    let priced = report
        .and_then(|r| alas_pipeline::fuel_model::report_route_mission_model(&result.config, r).ok())
        .map_or(f64::NAN, |model| model.wing_area_m2);
    Some(format!(
        "SREF {preset} native band: V {v:.2} m/s rho {rho:.4} kg/m3 m {m:.0} kg gamma {:.2} deg CL {cl:.3} implied S {implied:.2} m2; priced S {priced:.2} m2",
        gamma.to_degrees()
    ))
}
