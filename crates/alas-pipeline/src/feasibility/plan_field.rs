// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The field check of the take-off stages of a mission plan that name a field.
//!
//! A [`alas_config::StageKind::Takeoff`] with a `field` is judged at that field
//! and at the mass the aircraft has when the stage starts, with the take-off
//! distance of the existing airport model against the field's distance
//! available. The distance is the method the aircraft has: for a jet the CS 25.113
//! integration (the greater of the continued one-engine-out take-off and 1.15
//! times the all-engines distance, with the balanced field), for a propeller
//! aeroplane Torenbeek's estimate (the greater of 1.15 times the all-engines
//! distance and the balanced-field length of Eq. 5-89, which is not a continued
//! one-engine-out distance). The result is reported for every such stage
//! ([`StageFieldCheck`], with the method named), and a stage that does not fit
//! is also a finding.

use alas_config::airports::Airport;
use alas_config::{AlasConfig, StageKind};
use alas_mission::MissionResult;
use alas_perf::performance::{compute_v_speeds_at_masses, density_ratio};

use super::field::field_for_airport;
use super::{error, static_thrust, FindingCode, PhysicalFinding};
use crate::full_analysis::AnalysisReport;
use crate::mission_stage::plan::{stage_of_tag, PlanLoadCase};

const METRES_PER_FOOT: f64 = 0.3048;

/// The critical-field result of one take-off stage that names a field.
#[derive(Debug, Clone, PartialEq)]
pub struct StageFieldCheck {
    /// The field's name.
    pub field: String,
    /// Mass at the start of the stage, kg.
    pub mass_kg: f64,
    /// Take-off distance required, m, by [`StageFieldCheck::method`].
    pub required_m: f64,
    /// The method behind `required_m`.
    pub method: &'static str,
    /// Take-off safety speed V2 the distance was flown with, m/s.
    pub v2_m_s: f64,
    /// Take-off distance available, m.
    pub available_m: f64,
    /// Whether the required distance fits.
    pub feasible: bool,
}

/// The field check of every take-off stage of `case` that names a field, with
/// the stage's index; an error message when the performance cannot be evaluated.
pub(in crate::feasibility) fn checks(
    config: &AlasConfig,
    report: &AnalysisReport,
    mission: &MissionResult,
    case: &PlanLoadCase,
) -> Vec<(usize, Result<StageFieldCheck, String>)> {
    let landing_mass_kg = mission.final_mass_kg();
    let mut out = Vec::new();
    for (index, stage) in case.plan.stages.iter().enumerate() {
        let StageKind::Takeoff { field: Some(field) } = &stage.kind else {
            continue;
        };
        let Some(mass_kg) = mission
            .segments
            .iter()
            .find(|segment| stage_of_tag(&segment.spec.tag) == Some(index))
            .and_then(|segment| segment.conditions.total_mass_kg.first().copied())
        else {
            continue;
        };
        let airport = Airport {
            name: field.name.clone(),
            icao: String::new(),
            elevation_m: field.elevation_ft * METRES_PER_FOOT,
            toda_m: field.toda_m,
            lda_m: field.toda_m,
            isa_deviation_c: field.isa_deviation_k,
            notes: String::new(),
            latitude_deg: 0.0,
            longitude_deg: 0.0,
        };
        let wing_area_m2 = report.airplane.s_ref;
        // The speed basis of the aircraft as built, as the feasibility gate and the
        // ISA sea-level reference read it: the candidate's CLmax, the minimum
        // control speed its fin and rudder give (the declared ratio is only the
        // fallback and the cap) and, for a jet, the improved-climb V2, all at this
        // field and this mass. Judging the take-off with the declared ratio alone
        // raises V2 to 1.10 of a minimum control speed the aircraft does not have
        // and lengthens the distance of the stage by the same share (FIELD-OEI).
        let resolved = crate::report_config::config_for_report_at(
            config,
            report,
            &airport,
            mass_kg,
            landing_mass_kg.min(mass_kg),
            wing_area_m2,
        );
        let config = &*resolved;
        let speeds = compute_v_speeds_at_masses(
            mass_kg,
            mass_kg,
            wing_area_m2,
            &airport,
            config.performance.cl_max_to,
            config.performance.cl_max_land,
            &config.performance,
        );
        let thrust = static_thrust::resolve(
            config,
            speeds.v_r_ms,
            density_ratio(airport.elevation_m, airport.isa_deviation_c),
        );
        let static_tw = thrust.thrust_n / (mass_kg * config.requirements.gravity_m_s2);
        let result = field_for_airport(
            config,
            report,
            &airport,
            (
                wing_area_m2,
                mass_kg,
                landing_mass_kg.min(mass_kg),
                static_tw,
            ),
        )
        .map(|performance| StageFieldCheck {
            field: field.name.clone(),
            mass_kg,
            required_m: performance.todr_m,
            method: if performance.takeoff_field.is_some() {
                "CS 25.113 integration"
            } else {
                "Torenbeek propeller estimate: 1.15 x all-engines or balanced field"
            },
            v2_m_s: performance.v_speeds.v2_ms,
            available_m: performance.toda_m(),
            feasible: performance.to_feasible(),
        });
        out.push((index, result));
    }
    out
}

/// Append a finding for every named-field take-off whose distance exceeds the
/// field's, or whose field performance cannot be evaluated.
pub(super) fn assess(
    config: &AlasConfig,
    report: &AnalysisReport,
    mission: &MissionResult,
    case: &PlanLoadCase,
    findings: &mut Vec<PhysicalFinding>,
) {
    for (index, result) in checks(config, report, mission, case) {
        let id = &case.plan.stages[index].id;
        match result {
            Ok(check) if !check.feasible => findings.push(error(
                FindingCode::FieldTakeoffDistanceViolation,
                format!(
                    "stage '{id}': take-off at {:.0} kg needs {:.0} m at {} ({}), {:.0} m available",
                    check.mass_kg, check.required_m, check.field, check.method, check.available_m
                ),
                Some(check.required_m),
                Some(check.available_m),
                "m",
            )),
            Ok(_) => {}
            Err(reason) => findings.push(error(
                FindingCode::FieldPerformanceUnavailable,
                format!("stage '{id}': field performance could not be evaluated: {reason}"),
                None,
                None,
                "",
            )),
        }
    }
}
