// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Clean candidate drag at the actual departure/V2 point for the OEI check.

use alas_config::AlasConfig;
use alas_perf::performance::{
    assess_oei_climb, compute_v_speeds_at_masses, far25_oei_gradient, oei_cl_at_v2,
    OeiClimbAssessment, OeiV2Condition,
};

use crate::field_reference::candidate_field_polar;
use crate::full_analysis::AnalysisReport;

pub(super) fn assess(
    config: &AlasConfig,
    report: &AnalysisReport,
    takeoff_mass_kg: f64,
    wing_area_m2: f64,
) -> Result<OeiClimbAssessment, String> {
    let perf = &config.performance;
    let n_engines = config.geometry.engine.spanwise_positions_m.len() as i64;
    let gradient = far25_oei_gradient(n_engines).unwrap_or(perf.oei_gradient);
    if !(2..=4).contains(&n_engines) {
        return Ok(assess_oei_climb(
            0.0,
            0.0,
            n_engines,
            gradient,
            perf.oei_climb_cl,
            perf.oei_climb_delta_cd,
            perf.cl_max_to,
            None,
        ));
    }
    let departure = alas_config::airports::get(&config.departure_airport)
        .map_err(|error| format!("departure airport: {error}"))?;
    let speeds = compute_v_speeds_at_masses(
        takeoff_mass_kg,
        takeoff_mass_kg,
        wing_area_m2,
        departure,
        perf.cl_max_to,
        perf.cl_max_land,
        perf,
    );
    let v2_over_vstall = speeds.v2_ms / speeds.v_stall_to_ms;
    let cl = oei_cl_at_v2(perf.cl_max_to, v2_over_vstall)
        .ok_or_else(|| "selected V2 lift coefficient is invalid".to_owned())?;
    let atmosphere =
        alas_atmo::us1976_compute_values(departure.elevation_m, departure.isa_deviation_c);
    let mach = speeds.v2_ms / atmosphere.speed_of_sound_m_s;
    let (point_cd, point_k) = candidate_field_polar(
        config,
        report,
        cl,
        mach,
        departure.elevation_m,
        departure.isa_deviation_c,
    )?;
    // Both diagnostic and condition-specific branches evaluate this selected
    // CL. The point projection includes trimmed clean drag; the declared
    // high-lift/asymmetric/failed-engine terms remain separate.
    Ok(assess_oei_climb(
        point_cd,
        point_k,
        n_engines,
        gradient,
        cl,
        perf.oei_climb_delta_cd,
        perf.cl_max_to,
        Some(OeiV2Condition {
            departure_elevation_m: departure.elevation_m,
            departure_isa_deviation_c: departure.isa_deviation_c,
            v2_over_vstall,
            condition_to_sls_thrust_ratio: perf.oei_condition_to_sls_thrust_ratio,
            asymmetric_trim_cd: perf.oei_asymmetric_trim_cd,
            windmilling_cd: perf.oei_windmilling_cd,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oei_departure_drag_uses_the_shared_table_and_preserves_evidence_gaps() {
        let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "A320-200"}))
            .unwrap_or_else(|error| panic!("A320 config: {error}"));
        let design = alas_config::presets::get("A320-200")
            .unwrap_or_else(|error| panic!("A320 preset: {error}"))
            .design_vector;
        let mut report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&design, true)
            .unwrap_or_else(|error| panic!("A320 report: {error}"));
        let takeoff_mass_kg = config.requirements.mtow_kg;
        let wing_area_m2 = report.airplane.s_ref;
        let artifacts = report
            .fuel
            .artifacts(&config, &report.design)
            .unwrap_or_else(|error| panic!("A320 artifacts: {error}"));
        let table = artifacts
            .drag
            .table()
            .unwrap_or_else(|| panic!("native table"));
        let departure = alas_config::airports::get(&config.departure_airport)
            .unwrap_or_else(|error| panic!("departure: {error}"));
        let speeds = compute_v_speeds_at_masses(
            takeoff_mass_kg,
            takeoff_mass_kg,
            wing_area_m2,
            departure,
            config.performance.cl_max_to,
            config.performance.cl_max_land,
            &config.performance,
        );
        let cl = oei_cl_at_v2(
            config.performance.cl_max_to,
            speeds.v2_ms / speeds.v_stall_to_ms,
        )
        .unwrap_or_else(|| panic!("V2 CL"));
        let atmosphere =
            alas_atmo::us1976_compute_values(departure.elevation_m, departure.isa_deviation_c);
        let mach = speeds.v2_ms / atmosphere.speed_of_sound_m_s;
        let reynolds_per_m =
            atmosphere.density_kg_m3 * speeds.v2_ms / atmosphere.dynamic_viscosity_pa_s;
        let clean_cd = table.cd0_at_reynolds_per_m(mach, reynolds_per_m)
            + table.induced_cd(cl)
            + table.wave_cd(cl, mach);
        report.polar_fit.cd0 = f64::NAN;
        report.polar_fit.c1 = f64::NAN;
        report.polar_fit.k = f64::NAN;
        config.performance.oei_condition_to_sls_thrust_ratio = None;
        let result = assess(&config, &report, takeoff_mass_kg, wing_area_m2)
            .unwrap_or_else(|error| panic!("OEI: {error}"));
        let expected = 2.0
            * (far25_oei_gradient(2).unwrap_or(0.0)
                + (clean_cd + config.performance.oei_climb_delta_cd) / cl);
        let actual = result
            .required_inflight_tw
            .unwrap_or_else(|| panic!("diagnostic T/W"));
        assert!((actual - expected).abs() < 1.0e-14);
        assert_eq!(
            result.status,
            alas_perf::performance::OeiClimbStatus::EvidenceGap
        );
        assert!(result.required_sls_tw.is_none());
    }

    #[test]
    fn sized_candidate_oei_evidence_gap_matches_optimizer_in_hot_and_cold_air() {
        use alas_config::airport_io::{register_custom_airport, CustomAirport};
        use alas_opt::mdo::ResidualRole;
        use alas_perf::performance::OeiClimbStatus;

        // Unique in-memory records avoid replacing the shared airport registry.
        // Both departures have the same elevation/coordinates and differ only
        // in temperature, so both consumers must retain the actual field Re.
        static DEPARTURES: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DEPARTURES.get_or_init(|| {
            for (icao, isa_delta_c) in [("ZUHC", 20.0), ("ZUCC", -20.0)] {
                register_custom_airport(CustomAirport {
                    icao: icao.to_owned(),
                    name: format!("OEI drag regression {icao}"),
                    latitude_deg: 40.0,
                    longitude_deg: -3.0,
                    isa_delta_c,
                    altitude_m: 1_200.0,
                    runway_lengths_m: vec![4_000.0],
                    declared_toda_m: Some(4_000.0),
                    declared_lda_m: Some(4_000.0),
                    provenance: Default::default(),
                })
                .unwrap_or_else(|error| panic!("regression departure {icao}: {error}"));
            }
        });

        let design = alas_config::presets::get("A320-200")
            .unwrap_or_else(|error| panic!("A320 preset: {error}"))
            .design_vector;
        let mut report_demands = Vec::new();
        for departure_icao in ["ZUHC", "ZUCC"] {
            let mut config = AlasConfig::from_value(&serde_json::json!({
                "preset": "A320-200",
                "optimizer": {"design_space": {"mode": "reference_adaptation"}}
            }))
            .unwrap_or_else(|error| panic!("A320 config: {error}"));
            config.departure_airport = departure_icao.to_owned();
            config.performance.oei_climb_cl = 0.4;
            config.performance.oei_condition_to_sls_thrust_ratio = None;
            let assessment = alas_opt::assess_candidate(
                &alas_opt::DesignObjective::new(config.clone()),
                &design.to_array(),
            )
            .unwrap_or_else(|error| panic!("{departure_icao} assessment: {error}"));
            let sized = &assessment.sized;
            let mut report = crate::full_analysis::FullAnalysis::new(config.clone())
                .run_sized_candidate(&assessment.resolved.design, sized)
                .unwrap_or_else(|error| panic!("{departure_icao} report: {error}"));
            let artifacts = report
                .fuel
                .artifacts(&config, &report.design)
                .unwrap_or_else(|error| panic!("{departure_icao} artifacts: {error}"));
            let table = artifacts
                .drag
                .table()
                .unwrap_or_else(|| panic!("native report table"));
            let sized_table = sized
                .fuel_artifacts
                .drag
                .table()
                .unwrap_or_else(|| panic!("native optimizer table"));
            assert!(std::sync::Arc::ptr_eq(table, sized_table));

            let departure = alas_config::airports::get(departure_icao)
                .unwrap_or_else(|error| panic!("regression departure: {error}"));
            let speeds = compute_v_speeds_at_masses(
                sized.takeoff_mass_kg,
                sized.takeoff_mass_kg,
                report.airplane.s_ref,
                departure,
                config.performance.cl_max_to,
                config.performance.cl_max_land,
                &config.performance,
            );
            let selected_cl = oei_cl_at_v2(
                config.performance.cl_max_to,
                speeds.v2_ms / speeds.v_stall_to_ms,
            )
            .unwrap_or_else(|| panic!("selected V2 CL"));
            assert!((selected_cl - config.performance.oei_climb_cl).abs() > 0.5);
            report.polar_fit.cd0 = f64::NAN;
            report.polar_fit.c1 = f64::NAN;
            report.polar_fit.k = f64::NAN;
            let report_assessment = assess(
                &config,
                &report,
                sized.takeoff_mass_kg,
                report.airplane.s_ref,
            )
            .unwrap_or_else(|error| panic!("{departure_icao} report OEI: {error}"));
            assert_eq!(report_assessment.status, OeiClimbStatus::EvidenceGap);
            assert!(report_assessment.required_sls_tw.is_none());
            let report_demand = report_assessment
                .required_inflight_tw
                .unwrap_or_else(|| panic!("diagnostic in-flight demand"));
            let optimizer_residual = assessment
                .residuals
                .iter()
                .find(|residual| residual.id == "oei_second_segment")
                .unwrap_or_else(|| panic!("optimizer OEI residual"));
            assert_eq!(optimizer_residual.role, ResidualRole::Preference);
            assert!(assessment.residuals.iter().any(|residual| {
                residual.id == "oei_second_segment_evidence_gap"
                    && residual.role == ResidualRole::Diagnostic
            }));
            assert!(
                (report_demand - optimizer_residual.limit).abs() < 1.0e-14,
                "{departure_icao}: report {report_demand}, optimizer {}",
                optimizer_residual.limit
            );
            report_demands.push(report_demand);
        }
        assert!(
            (report_demands[0] - report_demands[1]).abs() > 1.0e-6,
            "departure temperature must alter the clean drag demand"
        );
    }
}
