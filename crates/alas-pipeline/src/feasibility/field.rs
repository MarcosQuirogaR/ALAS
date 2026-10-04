// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airport distance findings using the active propulsion field method.

use alas_config::{ActiveEngineModel, AlasConfig};
use alas_perf::performance::{
    compute_v_speeds_at_masses, density_ratio, tw_takeoff_constraint, ws_landing_limit,
};

use super::{error, static_thrust, FindingCode, PhysicalFinding};
use crate::field_performance::{calculate, dry_landing_distance_share, report_field_polar};
use crate::full_analysis::AnalysisReport;

// Aircraft masses, installed thrust and field provenance belong to one assessment.
#[allow(clippy::too_many_arguments)]
pub(super) fn assess_airports(
    config: &AlasConfig,
    report: &AnalysisReport,
    findings: &mut Vec<PhysicalFinding>,
    wing_area_m2: f64,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
    static_tw: f64,
    sea_level_static_thrust: &static_thrust::SeaLevelStaticThrust,
) {
    let gravity_m_s2 = config.requirements.gravity_m_s2;
    let wing_loading_pa = takeoff_mass_kg * gravity_m_s2 / wing_area_m2;
    let propeller = matches!(
        config.geometry.engine.active_model(),
        Ok(ActiveEngineModel::Turboprop(_))
    ) && !config.performance.legacy_field_correlations;
    for (airport_name, role) in [
        (&config.departure_airport, "departure"),
        (&config.arrival_airport, "arrival"),
    ] {
        let airport = match alas_config::airports::get(airport_name) {
            Ok(airport) => airport,
            Err(reason) => {
                findings.push(error(
                    FindingCode::FieldPerformanceUnavailable,
                    format!("{role} airport cannot be resolved: {reason}"),
                    None,
                    None,
                    "",
                ));
                continue;
            }
        };
        let unusable = |value: f64| !value.is_finite() || value <= 0.0;
        let failure = if unusable(wing_area_m2) {
            Some(("wing reference area", wing_area_m2, "m^2"))
        } else if unusable(takeoff_mass_kg) {
            Some(("analyzed take-off mass", takeoff_mass_kg, "kg"))
        } else if role == "departure" && unusable(static_tw) {
            Some((
                "static thrust-to-weight ratio",
                static_tw,
                "fraction weight",
            ))
        } else {
            None
        };
        if let Some((quantity, value, unit)) = failure {
            let thrust_reason = match &sea_level_static_thrust.source {
                static_thrust::StaticThrustSource::Unavailable { reason }
                    if quantity == "static thrust-to-weight ratio" =>
                {
                    format!("; {reason}")
                }
                _ => String::new(),
            };
            findings.push(error(FindingCode::FieldPerformanceUnavailable,
                format!("{role} field performance could not be evaluated: {quantity} is not finite and positive{thrust_reason}"),
                Some(value), Some(0.0), unit));
            continue;
        }
        let field_result = (|| {
            let (cd0, k) = if propeller {
                let speeds = compute_v_speeds_at_masses(
                    takeoff_mass_kg,
                    landing_mass_kg,
                    wing_area_m2,
                    airport,
                    config.performance.cl_max_to,
                    config.performance.cl_max_land,
                    &config.performance,
                );
                let atmosphere =
                    alas_atmo::us1976_compute_values(airport.elevation_m, airport.isa_deviation_c);
                report_field_polar(
                    config,
                    report,
                    speeds.v2_ms / atmosphere.speed_of_sound_m_s,
                    airport,
                )?
            } else {
                (0.0, 0.0)
            };
            calculate(
                config,
                airport,
                wing_area_m2,
                takeoff_mass_kg,
                landing_mass_kg,
                static_tw,
                cd0,
                k,
            )
        })();
        let field = match field_result {
            Ok(field) => field,
            Err(reason) => {
                findings.push(error(
                    FindingCode::FieldPerformanceUnavailable,
                    format!("{role} field performance could not be evaluated: {reason}"),
                    None,
                    None,
                    "",
                ));
                continue;
            }
        };
        // 25.125 actual distance and CAT.POL.A.230/121.195 dispatch length
        // have different definitions. Replay retains the unfactored gate.
        let share = if config.performance.legacy_field_correlations {
            1.0
        } else {
            dry_landing_distance_share(config)
        };
        let landing_field_length_m = field.ldr_m / share;
        let sigma = density_ratio(airport.elevation_m, airport.isa_deviation_c);
        let mut landing_constraint_violation = false;
        if role == "departure" {
            if !field.to_feasible() {
                findings.push(error(
                    FindingCode::FieldTakeoffDistanceViolation,
                    format!(
                        "departure TODR {:.1} m exceeds TODA {:.1} m",
                        field.todr_m,
                        field.toda_m()
                    ),
                    Some(field.todr_m),
                    Some(field.toda_m()),
                    "m",
                ));
            }
            // The TOP inverse is a jet constraint. Propeller feasibility is
            // judged on the integrated field distance, not a jet T/W proxy.
            if !propeller {
                let required = tw_takeoff_constraint(
                    &[wing_loading_pa],
                    airport.toda_m,
                    sigma,
                    config.performance.cl_max_to,
                )
                .first()
                .copied()
                .unwrap_or(f64::NAN);
                if required.is_finite() && static_tw < required {
                    findings.push(error(FindingCode::ThrustMarginViolation,
                        format!("departure static T/W {static_tw:.4} is below field requirement {required:.4}"),
                        Some(static_tw), Some(required), "T/W"));
                }
            }
        } else if !propeller {
            let landing_ws = landing_mass_kg * gravity_m_s2 / wing_area_m2;
            let limit = ws_landing_limit(
                airport.lda_m * share,
                sigma,
                config.performance.cl_max_land,
                config.performance.k_land,
            );
            if landing_ws.is_finite() && limit.is_finite() && landing_ws > limit {
                landing_constraint_violation = true;
                findings.push(error(FindingCode::FieldLandingDistanceViolation,
                    format!("arrival wing loading {landing_ws:.1} Pa exceeds landing limit {limit:.1} Pa"),
                    Some(landing_ws), Some(limit), "Pa"));
            }
        }
        if role == "arrival"
            && !landing_constraint_violation
            && landing_field_length_m > field.lda_m()
        {
            findings.push(error(FindingCode::FieldLandingDistanceViolation,
                format!("arrival dry landing field length {:.1} m exceeds LDA {:.1} m at landing mass {:.1} kg",
                    landing_field_length_m, field.lda_m(), field.landing_mass_kg),
                Some(landing_field_length_m), Some(field.lda_m()), "m"));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arrival_gate_compares_factored_dispatch_length_with_lda() {
        let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"}))
            .unwrap_or_else(|reason| panic!("preset: {reason}"));
        config.arrival_airport = "LXGB".to_owned();
        let preset = alas_config::presets::get("A320-200")
            .unwrap_or_else(|reason| panic!("preset: {reason}"));
        let report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&preset.design_vector, true)
            .unwrap_or_else(|reason| panic!("analysis: {reason}"));
        let mtow = config.requirements.mtow_kg;
        let mlw = config.landing_mass_limit_kg(mtow);
        let area = report.airplane.s_ref;
        let airport =
            alas_config::airports::get("LXGB").unwrap_or_else(|reason| panic!("airport: {reason}"));
        let static_thrust = static_thrust::resolve(&config, 60.0, 1.0);
        let tw = static_thrust.thrust_n / (mtow * config.requirements.gravity_m_s2);
        let field = calculate(&config, airport, area, mtow, mlw, tw, 0.0, 0.0)
            .unwrap_or_else(|reason| panic!("field: {reason}"));
        assert!(field.ldr_m < airport.lda_m);
        assert!(field.ldr_m / dry_landing_distance_share(&config) > airport.lda_m);
        let mut findings = Vec::new();
        assess_airports(
            &config,
            &report,
            &mut findings,
            area,
            mtow,
            mlw,
            tw,
            &static_thrust,
        );
        assert!(findings
            .iter()
            .any(|finding| finding.code == FindingCode::FieldLandingDistanceViolation));
        config.performance.legacy_field_correlations = true;
        let mut replay = Vec::new();
        assess_airports(
            &config,
            &report,
            &mut replay,
            area,
            mtow,
            mlw,
            tw,
            &static_thrust,
        );
        assert!(!replay
            .iter()
            .any(|finding| finding.code == FindingCode::FieldLandingDistanceViolation));
    }
}
