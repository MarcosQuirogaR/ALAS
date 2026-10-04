// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Report inputs for the same propulsion-specific field method as feasibility.

use alas_config::airports::Airport;
use alas_config::{ActiveEngineModel, AlasConfig};
use alas_perf::performance::{compute_v_speeds_at_masses, FieldPerformance};
use alas_pipeline::field_performance::{calculate, dry_landing_distance_share, report_field_polar};
use alas_pipeline::field_reference::sea_level_static_thrust_n;
use alas_pipeline::full_analysis::AnalysisReport;

pub(super) fn field_performance(
    report: &AnalysisReport,
    config: &AlasConfig,
    airport: &Airport,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
) -> Result<FieldPerformance, String> {
    if report
        .airplane
        .wings
        .first()
        .filter(|wing| wing.xsecs.len() >= 2)
        .is_none()
    {
        return Err(
            "The analyzed report has no main wing; field performance cannot be computed."
                .to_owned(),
        );
    }
    let area_m2 = report
        .geometry_summary
        .get("wing_area_m2")
        .copied()
        .unwrap_or(report.airplane.s_ref);
    if [area_m2, takeoff_mass_kg, landing_mass_kg]
        .iter()
        .any(|value| !value.is_finite() || *value <= 0.0)
    {
        return Err("Field masses and wing area must be finite and positive.".to_owned());
    }
    let perf = &config.performance;
    let speeds_at = |field: &Airport| {
        compute_v_speeds_at_masses(
            takeoff_mass_kg,
            landing_mass_kg,
            area_m2,
            field,
            perf.cl_max_to,
            perf.cl_max_land,
            perf,
        )
    };
    let sea_level = Airport::custom("ISA sea level", 0.0, 10_000.0, 10_000.0, 0.0, 0.0, 0.0);
    // Historical propeller replay uses the deck's sea-level roll-mean thrust.
    // The corrected propeller calculation resolves thrust at each roll speed.
    let (thrust_n, _) = sea_level_static_thrust_n(config, speeds_at(&sea_level).v_r_ms)?;
    let static_tw = thrust_n / (takeoff_mass_kg * config.requirements.gravity_m_s2);
    let (cd0, k) = if matches!(
        config.geometry.engine.active_model(),
        Ok(ActiveEngineModel::Turboprop(_))
    ) && !perf.legacy_field_correlations
    {
        let atmosphere =
            alas_atmo::us1976_compute_values(airport.elevation_m, airport.isa_deviation_c);
        report_field_polar(
            config,
            report,
            speeds_at(airport).v2_ms / atmosphere.speed_of_sound_m_s,
            airport,
        )?
    } else {
        // Jets and historical replay do not consume the propeller roll polar.
        (0.0, 0.0)
    };
    let mut field = calculate(
        config,
        airport,
        area_m2,
        takeoff_mass_kg,
        landing_mass_kg,
        static_tw,
        cd0,
        k,
    )?;
    if !perf.legacy_field_correlations {
        // Actual distance under 25.125 becomes the operational dry-runway LFL.
        field.ldr_m /= dry_landing_distance_share(config);
    }
    Ok(field)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::families::performance::figure_lto_for_airport_at_masses;
    use crate::scene::{Scene, SceneElement};
    use alas_pipeline::field_reference::report_isa_sea_level_field_reference;
    use alas_pipeline::full_analysis::FullAnalysis;

    fn sample(name: &str) -> (AlasConfig, AnalysisReport) {
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name}))
            .unwrap_or_else(|reason| panic!("preset: {reason}"));
        let design = alas_config::presets::get(name)
            .unwrap_or_else(|reason| panic!("preset: {reason}"))
            .design_vector;
        let report = FullAnalysis::new(config.clone())
            .run(&design, true)
            .unwrap_or_else(|reason| panic!("analysis: {reason}"));
        (config, report)
    }

    fn labels(scene: &Scene) -> Vec<&str> {
        scene
            .elements
            .iter()
            .filter_map(|element| match element {
                SceneElement::Text { text, .. } | SceneElement::TextBlock { text, .. } => {
                    Some(text.as_str())
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn jet_and_propeller_figures_use_the_reference_method_and_dry_landing_share() {
        let airport = Airport::custom("ISA sea level", 0.0, 10_000.0, 10_000.0, 0.0, 0.0, 0.0);
        for name in ["A320-200", "ATR72-600"] {
            let (mut config, report) = sample(name);
            let takeoff = config.requirements.mtow_kg;
            let landing = config.landing_mass_limit_kg(takeoff);
            let area = report
                .geometry_summary
                .get("wing_area_m2")
                .copied()
                .unwrap_or(report.airplane.s_ref);
            let reference =
                report_isa_sea_level_field_reference(&config, &report, area, takeoff, landing)
                    .unwrap_or_else(|reason| panic!("reference: {reason}"));
            let field = field_performance(&report, &config, &airport, takeoff, landing)
                .unwrap_or_else(|reason| panic!("figure inputs: {reason}"));
            assert!((field.todr_m - reference.takeoff_field_length_m).abs() < 1e-9);
            assert!(
                (field.ldr_m * dry_landing_distance_share(&config) - reference.landing_distance_m)
                    .abs()
                    < 1e-9
            );
            assert_eq!(
                field.v_speeds.v_app_ms,
                1.23 * field.v_speeds.v_stall_land_ms
            );

            let scene = figure_lto_for_airport_at_masses(
                &report,
                &config,
                &airport,
                "Departure",
                takeoff,
                landing,
                None,
            );
            let text = labels(&scene);
            assert!(
                text.contains(&"LFL"),
                "{name} did not render its landing field length"
            );
            assert!(text.contains(&"BFL proxy"));
            assert!(!text.contains(&"LDR"));

            config.performance.legacy_field_correlations = true;
            let legacy = field_performance(&report, &config, &airport, takeoff, landing)
                .unwrap_or_else(|reason| panic!("historical figure: {reason}"));
            let speeds = compute_v_speeds_at_masses(
                takeoff,
                landing,
                area,
                &airport,
                config.performance.cl_max_to,
                config.performance.cl_max_land,
                &config.performance,
            );
            let (thrust_n, _) = sea_level_static_thrust_n(&config, speeds.v_r_ms)
                .unwrap_or_else(|reason| panic!("historical thrust: {reason}"));
            let actual = calculate(
                &config,
                &airport,
                area,
                takeoff,
                landing,
                thrust_n / (takeoff * config.requirements.gravity_m_s2),
                0.0,
                0.0,
            )
            .unwrap_or_else(|reason| panic!("historical field: {reason}"));
            assert_eq!(legacy.ldr_m, actual.ldr_m);
            let scene = figure_lto_for_airport_at_masses(
                &report,
                &config,
                &airport,
                "Departure",
                takeoff,
                landing,
                None,
            );
            assert!(labels(&scene).contains(&"LDR"));
        }
    }

    #[test]
    fn unavailable_field_inputs_render_the_reason_instead_of_distances() {
        let (config, report) = sample("ATR72-600");
        let airport = Airport::custom("ISA sea level", 0.0, 10_000.0, 10_000.0, 0.0, 0.0, 0.0);
        let takeoff = config.requirements.mtow_kg;
        let landing = config.landing_mass_limit_kg(takeoff);
        let scene = figure_lto_for_airport_at_masses(
            &report,
            &config,
            &airport,
            "Departure",
            f64::NAN,
            landing,
            None,
        );
        let text = labels(&scene).join(" ");
        assert!(text.contains("finite and positive"), "status text: {text}");
        assert!(!labels(&scene).contains(&"TODR"));

        let mut invalid = config;
        invalid.performance.propeller_dry_landing_distance_share = 0.0;
        let scene = figure_lto_for_airport_at_masses(
            &report,
            &invalid,
            &airport,
            "Departure",
            takeoff,
            landing,
            None,
        );
        let text = labels(&scene).join(" ");
        assert!(
            text.contains("dry landing distance share"),
            "status text: {text}"
        );
        assert!(!labels(&scene).contains(&"LFL"));
    }
}
