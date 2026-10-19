// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Clean candidate drag at the actual departure/V2 point for the OEI check.

use alas_config::AlasConfig;
use alas_opt::mdo::{
    evaluate_climb_checks, second_segment_evidence, takeoff_thrust_scatter, CertificationInputs,
    ClimbOutcome, EngineOutModel, ThrustAssessment, YawAirframe,
};
use alas_perf::performance::{
    assess_oei_climb, compute_v_speeds_at_masses, far25_oei_gradient, oei_cl_at_v2, ClimbSegment,
    OeiClimbAssessment, OeiClimbStatus,
};

use super::{error, warning, FindingCode, FindingSeverity, PhysicalFinding};

use crate::field_reference::candidate_field_polar;
use crate::full_analysis::AnalysisReport;

/// The shared engine-out evidence builder bound to this report's airframe.
///
/// `static_thrust_n` is the all-engine installed static thrust the caller's
/// thrust-to-weight axis uses (the same figure a turboprop ratio is taken
/// against); it is ignored for turbofans, whose deck is self-normalised.
///
/// `takeoff_mass_kg` bounds the bank of the asymmetric trim (CS 25.149(b)).
pub(super) fn engine_out_model(
    config: &AlasConfig,
    report: &AnalysisReport,
    wing_area_m2: f64,
    static_thrust_n: f64,
    takeoff_mass_kg: f64,
) -> Result<EngineOutModel, String> {
    let n_engines = config.geometry.engine.spanwise_positions_m.len().max(1);
    EngineOutModel::new(
        config,
        Some(YawAirframe {
            plane: &report.airplane,
            cg_x_m: report.physical_cg[0],
        }),
        wing_area_m2,
        Some(static_thrust_n / n_engines as f64),
    )
    .map(|model| model.with_weight_kg(takeoff_mass_kg))
}

pub(super) fn assess(
    config: &AlasConfig,
    report: &AnalysisReport,
    takeoff_mass_kg: f64,
    wing_area_m2: f64,
    static_thrust_n: f64,
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
    let artifacts = report.fuel.artifacts(config, &report.design)?;
    // The engine deck may be unavailable for an unusual binding; the
    // assessor then reports an evidence gap rather than a placeholder.
    let model = engine_out_model(
        config,
        report,
        wing_area_m2,
        static_thrust_n,
        takeoff_mass_kg,
    )
    .ok();
    let evidence = second_segment_evidence(
        model.as_ref(),
        perf,
        departure,
        &speeds,
        &|mach, altitude_m, isa_deviation_c| {
            artifacts
                .drag
                .cd_at_atmosphere(0.0, mach, altitude_m, isa_deviation_c)
        },
    );
    // Both diagnostic and condition-specific branches evaluate this selected
    // CL. The point projection includes trimmed clean drag; the declared
    // high-lift and the derived (or configured) asymmetric/failed-engine
    // terms remain separate.
    Ok(assess_oei_climb(
        point_cd,
        point_k,
        n_engines,
        gradient,
        cl,
        perf.oei_climb_delta_cd,
        perf.cl_max_to,
        Some(evidence.condition),
    ))
}

/// Masses, wing area and installed thrust of one climb assessment.
#[derive(Debug, Clone, Copy)]
pub(super) struct ClimbBasis {
    /// Analysed take-off mass, kg.
    pub takeoff_mass_kg: f64,
    /// Analysed (or limiting) landing mass, kg.
    pub landing_mass_kg: f64,
    /// Reference wing area, m^2.
    pub wing_area_m2: f64,
    /// All-engine installed static thrust, N.
    pub static_thrust_n: f64,
    /// Installed static thrust over take-off weight.
    pub static_tw: f64,
}

/// Push the engine-out and landing climb findings: the CS 25.121(b) second
/// segment, then the CS 25.121(c), 25.121(d) and 25.119 checks.
///
/// An in-flight OEI estimate is useful as a diagnostic, but it is not
/// dimensionally comparable with the SLS axis used here. Only a complete
/// condition-specific SLS result may create a hard thrust-margin finding;
/// missing evidence is surfaced as a warning so a conceptual fallback is not
/// mistaken for Part 25 evidence.
pub(super) fn push_findings(
    config: &AlasConfig,
    report: &AnalysisReport,
    basis: ClimbBasis,
    findings: &mut Vec<PhysicalFinding>,
) {
    let oei_assessment = assess(
        config,
        report,
        basis.takeoff_mass_kg,
        basis.wing_area_m2,
        basis.static_thrust_n,
    )
    .unwrap_or_else(|reason| {
        findings.push(error(
            FindingCode::FieldPerformanceUnavailable,
            format!("OEI drag could not be assessed: {reason}"),
            None,
            None,
            "",
        ));
        OeiClimbAssessment {
            status: OeiClimbStatus::EvidenceGap,
            required_inflight_tw: None,
            required_sls_tw: None,
            diagnostic: "OEI shared candidate drag is unavailable; no SLS floor is scored.",
        }
    });
    if let Some(required_sls_tw) = oei_assessment.required_sls_tw {
        if required_sls_tw.is_finite() && basis.static_tw < required_sls_tw {
            // The take-off rating carries the take-off band (owner decision
            // 2026-10-14): a shortfall inside it is a flagged warning, beyond
            // it a hard failure. A turboprop has no band.
            let band = takeoff_thrust_scatter(config);
            let assessment = ThrustAssessment::takeoff(required_sls_tw, basis.static_tw, band);
            if band > 0.0 && assessment.shortfall() <= band {
                findings.push(warning(
                    FindingCode::ThrustWithinChartScatter,
                    assessment.describe("CS 25.121(b) second segment (static T/W)", "T/W"),
                    Some(basis.static_tw),
                    Some(required_sls_tw),
                    "T/W",
                ));
            } else {
                // The requirement is the in-flight need over the deck's thrust ratio
                // at V2, so the ratio and the break-even ratio follow from it.
                let ratio_note = oei_assessment
                .required_inflight_tw
                .filter(|inflight| *inflight > 0.0)
                .map(|inflight| {
                    format!(
                        " (deck thrust ratio at V2 {:.3}; the requirement is met from a ratio of {:.3}; Jenkinson, Civil Jet Aircraft Design, Ch. 10, puts the thrust at V2 about 20 % below static)",
                        inflight / required_sls_tw,
                        inflight / basis.static_tw
                    )
                })
                .unwrap_or_default();
                findings.push(error(
                FindingCode::ThrustMarginViolation,
                format!(
                    "engine-out second-segment climb requires static T/W {:.4}, but the configured rating provides {:.4}{ratio_note}",
                    required_sls_tw, basis.static_tw
                ),
                Some(basis.static_tw),
                Some(required_sls_tw),
                "T/W",
            ));
            }
        }
    } else if !matches!(
        oei_assessment.status,
        OeiClimbStatus::NotApplicable | OeiClimbStatus::ConceptualInflight
    ) {
        findings.push(PhysicalFinding {
            code: FindingCode::FieldPerformanceUnavailable,
            severity: FindingSeverity::Warning,
            message: oei_assessment.diagnostic.to_owned(),
            actual: None,
            limit: None,
            unit: "",
        });
    }
    push_certification_findings(config, report, basis, findings);
}

/// The CS 25.121(c), 25.121(d) and 25.119 checks of the shared evaluator.
fn push_certification_findings(
    config: &AlasConfig,
    report: &AnalysisReport,
    basis: ClimbBasis,
    findings: &mut Vec<PhysicalFinding>,
) {
    let usable = |value: f64| value.is_finite() && value > 0.0;
    if !(usable(basis.takeoff_mass_kg)
        && usable(basis.landing_mass_kg)
        && usable(basis.wing_area_m2))
    {
        return;
    }
    let (model, artifacts) = match (
        engine_out_model(
            config,
            report,
            basis.wing_area_m2,
            basis.static_thrust_n,
            basis.takeoff_mass_kg,
        ),
        report.fuel.artifacts(config, &report.design),
    ) {
        (Ok(model), Ok(artifacts)) => (model, artifacts),
        (model, artifacts) => {
            let reason = model.err().or(artifacts.err()).unwrap_or_default();
            findings.push(PhysicalFinding {
                code: FindingCode::FieldPerformanceUnavailable,
                severity: FindingSeverity::Warning,
                message: format!("certification climb checks not scored: {reason}"),
                actual: None,
                limit: None,
                unit: "",
            });
            return;
        }
    };
    let departure = alas_config::airports::get(&config.departure_airport).ok();
    let arrival = alas_config::airports::get(&config.arrival_airport).ok();
    let checks = evaluate_climb_checks(&CertificationInputs {
        config,
        drag: &artifacts.drag,
        engine_out: &model,
        s_ref_m2: basis.wing_area_m2,
        takeoff_mass_kg: basis.takeoff_mass_kg,
        landing_mass_kg: basis.landing_mass_kg,
        departure,
        arrival,
    });
    for (segment, outcome) in checks {
        match outcome {
            ClimbOutcome::Evaluated(evaluation) => {
                if evaluation.required_sls_tw.is_finite()
                    && basis.static_tw < evaluation.required_sls_tw
                {
                    // The approach and landing climbs fly the take-off/go-around
                    // rating and carry the take-off band; the final segment
                    // (maximum continuous) does not.
                    let band = match segment {
                        ClimbSegment::ApproachClimb | ClimbSegment::LandingClimb => {
                            takeoff_thrust_scatter(config)
                        }
                        _ => 0.0,
                    };
                    let assessment = ThrustAssessment::takeoff(
                        evaluation.required_sls_tw,
                        basis.static_tw,
                        band,
                    );
                    if band > 0.0 && assessment.shortfall() <= band {
                        findings.push(warning(
                            FindingCode::ThrustWithinChartScatter,
                            assessment
                                .describe(&format!("{} (static T/W)", clause(segment)), "T/W"),
                            Some(basis.static_tw),
                            Some(evaluation.required_sls_tw),
                            "T/W",
                        ));
                        continue;
                    }
                    findings.push(error(
                        FindingCode::ThrustMarginViolation,
                        format!(
                            "{} requires static T/W {:.4} (gradient {:.3}, CL {:.3}, CD {:.4}), but the configured rating provides {:.4}",
                            clause(segment),
                            evaluation.required_sls_tw,
                            evaluation.gradient,
                            evaluation.cl,
                            evaluation.cd,
                            basis.static_tw
                        ),
                        Some(basis.static_tw),
                        Some(evaluation.required_sls_tw),
                        "T/W",
                    ));
                }
            }
            ClimbOutcome::EvidenceGap(reason) => findings.push(PhysicalFinding {
                code: FindingCode::FieldPerformanceUnavailable,
                severity: FindingSeverity::Warning,
                message: format!("{} not scored: {reason}", clause(segment)),
                actual: None,
                limit: None,
                unit: "",
            }),
        }
    }
}

fn clause(segment: ClimbSegment) -> &'static str {
    match segment {
        ClimbSegment::SecondSegment => "CS 25.121(b) second segment",
        ClimbSegment::FinalSegment => "CS 25.121(c) final segment",
        ClimbSegment::ApproachClimb => "CS 25.121(d) approach climb",
        ClimbSegment::LandingClimb => "CS 25.119 landing climb",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oei_departure_drag_uses_the_shared_table_and_the_shared_evidence() {
        use alas_opt::mdo::{EngineOutCondition, EngineOutRating};

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
        // The point projection must not depend on the report's polar fit.
        report.polar_fit.cd0 = f64::NAN;
        report.polar_fit.c1 = f64::NAN;
        report.polar_fit.k = f64::NAN;

        // The evidence the shared builder derives for this very point.
        let static_thrust_n = 2.0 * config.geometry.engine.thrust_kn() * 1_000.0;
        let model = engine_out_model(
            &config,
            &report,
            wing_area_m2,
            static_thrust_n,
            takeoff_mass_kg,
        )
        .unwrap_or_else(|error| panic!("engine-out model: {error}"));
        let cd0 = artifacts.drag.cd_at_atmosphere(
            0.0,
            mach,
            departure.elevation_m,
            departure.isa_deviation_c,
        );
        let point = model
            .point(
                EngineOutCondition {
                    altitude_m: departure.elevation_m,
                    isa_deviation_c: departure.isa_deviation_c,
                    tas_m_s: speeds.v2_ms,
                },
                EngineOutRating::TakeoffGoAround,
                cd0,
            )
            .unwrap_or_else(|error| panic!("engine-out point: {error}"));
        let trim = point
            .asymmetric_trim_cd
            .unwrap_or_else(|| panic!("handbook trim drag"));

        config.performance.oei_condition_to_sls_thrust_ratio = None;
        let result = assess(
            &config,
            &report,
            takeoff_mass_kg,
            wing_area_m2,
            static_thrust_n,
        )
        .unwrap_or_else(|error| panic!("OEI: {error}"));
        let expected_inflight = 2.0
            * (far25_oei_gradient(2).unwrap_or(0.0)
                + (clean_cd + config.performance.oei_climb_delta_cd + point.windmilling_cd + trim)
                    / cl);
        let actual = result
            .required_inflight_tw
            .unwrap_or_else(|| panic!("in-flight T/W"));
        assert!((actual - expected_inflight).abs() < 1.0e-12);
        // Nothing is configured, yet the evidence is complete: the deck ratio
        // converts the requirement to the sea-level-static axis.
        assert_eq!(
            result.status,
            alas_perf::performance::OeiClimbStatus::SlsEquivalent
        );
        let required_sls = result
            .required_sls_tw
            .unwrap_or_else(|| panic!("SLS requirement"));
        assert!((required_sls - expected_inflight / point.thrust_ratio).abs() < 1.0e-12);
        assert!(required_sls > expected_inflight);

        // A configured ratio still overrides the deck.
        config.performance.oei_condition_to_sls_thrust_ratio = Some(0.9);
        let manual = assess(
            &config,
            &report,
            takeoff_mass_kg,
            wing_area_m2,
            static_thrust_n,
        )
        .unwrap_or_else(|error| panic!("OEI: {error}"));
        let manual_sls = manual
            .required_sls_tw
            .unwrap_or_else(|| panic!("manual SLS requirement"));
        assert!((manual_sls - expected_inflight / 0.9).abs() < 1.0e-12);
    }

    #[test]
    fn sized_candidate_oei_requirement_matches_optimizer_in_hot_and_cold_air() {
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
            // The feasibility entry point resolves the computed VMC and then
            // the improved-climb V2 into the configuration before any speed
            // schedule (the registered A320 flies the CS 25.107 schedule, which
            // the hot-high departure's second segment lifts), as the optimizer
            // does; do the same here.
            let resolution = crate::report_config::resolve_report(
                &config,
                &report,
                sized.takeoff_mass_kg,
                sized.design_landing_mass_kg,
                report.airplane.s_ref,
            );
            let resolved = resolution.config;
            let report_assessment = assess(
                &resolved,
                &report,
                sized.takeoff_mass_kg,
                report.airplane.s_ref,
                2.0 * config.geometry.engine.thrust_kn() * 1_000.0,
            )
            .unwrap_or_else(|error| panic!("{departure_icao} report OEI: {error}"));
            assert_eq!(report_assessment.status, OeiClimbStatus::SlsEquivalent);
            let report_demand = report_assessment
                .required_sls_tw
                .unwrap_or_else(|| panic!("SLS demand"));
            let optimizer_residual = assessment
                .residuals
                .iter()
                .find(|residual| residual.id == "oei_second_segment")
                .unwrap_or_else(|| panic!("optimizer OEI residual"));
            assert_eq!(optimizer_residual.role, ResidualRole::Constraint);
            assert!(!assessment
                .residuals
                .iter()
                .any(|residual| residual.id == "oei_second_segment_evidence_gap"));
            // One shared evidence builder: the report and the optimizer agree
            // on the thrust ratio, the failed-engine drag and the trim drag,
            // up to the small difference between the report's mass-weighted
            // centre of gravity and the optimizer's.
            assert!(
                (report_demand / optimizer_residual.limit - 1.0).abs() < 2.0e-3,
                "{departure_icao}: report {report_demand}, optimizer {}",
                optimizer_residual.limit
            );
            // The improved-climb V2 pins the demand to the installed thrust, so
            // the departure-temperature check reads the demand at the schedule
            // speed (computed VMC only).
            let vmc = crate::report_config::evaluate_report_vmc(
                &config,
                &report,
                sized.takeoff_mass_kg,
                sized.design_landing_mass_kg,
                report.airplane.s_ref,
            );
            let scheduled = alas_opt::mdo::performance_with_vmc(&config, &vmc);
            let schedule_demand = assess(
                &scheduled,
                &report,
                sized.takeoff_mass_kg,
                report.airplane.s_ref,
                2.0 * config.geometry.engine.thrust_kn() * 1_000.0,
            )
            .unwrap_or_else(|error| panic!("{departure_icao} schedule OEI: {error}"))
            .required_sls_tw
            .unwrap_or_else(|| panic!("schedule SLS demand"));
            report_demands.push(schedule_demand);
        }
        assert!(
            (report_demands[0] - report_demands[1]).abs() > 1.0e-6,
            "departure temperature must alter the clean drag demand"
        );
    }
}
