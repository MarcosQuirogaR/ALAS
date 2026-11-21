// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Airport distance findings using the active propulsion field method.

use alas_config::airports::Airport;
use alas_config::{ActiveEngineModel, AlasConfig};
use alas_opt::mdo::YawAirframe;
use alas_perf::performance::{
    compute_v_speeds_at_masses, density_ratio, tw_takeoff_constraint, ws_landing_limit_for,
    FieldPerformance, FieldPolar,
};

use super::{error, static_thrust, FindingCode, PhysicalFinding};
use crate::field_performance::{
    calculate_with_airframe, landing_distance_share, report_field_polar,
};
use crate::full_analysis::AnalysisReport;

#[cfg(test)]
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
    assess_airports_on_route(
        config,
        report,
        findings,
        wing_area_m2,
        takeoff_mass_kg,
        landing_mass_kg,
        static_tw,
        sea_level_static_thrust,
        None,
    );
}

/// The same assessment for the endpoints a plan actually flies. `route` holds
/// the departure and arrival records with the stated field lengths already
/// applied (`AlasConfig::with_stated_for_route`), the pair the optimizer and
/// the Results figures use; without one the configured aerodromes are judged.
#[allow(clippy::too_many_arguments)]
pub(super) fn assess_airports_on_route(
    config: &AlasConfig,
    report: &AnalysisReport,
    findings: &mut Vec<PhysicalFinding>,
    wing_area_m2: f64,
    takeoff_mass_kg: f64,
    landing_mass_kg: f64,
    static_tw: f64,
    sea_level_static_thrust: &static_thrust::SeaLevelStaticThrust,
    route: Option<(&Airport, &Airport)>,
) {
    let gravity_m_s2 = config.requirements.gravity_m_s2;
    let wing_loading_pa = takeoff_mass_kg * gravity_m_s2 / wing_area_m2;
    let propeller = matches!(
        config.geometry.engine.active_model(),
        Ok(ActiveEngineModel::Turboprop(_))
    ) && !config.performance.legacy_field_correlations;
    // A stated field length belongs to the configured aerodrome: on a plan that
    // flies another pair, the notice and the judgement follow the endpoint it
    // actually applies to (`AlasConfig::with_stated_for_route`).
    let is_configured = |name: &str, endpoint: Option<&Airport>| {
        endpoint.is_none_or(|endpoint| {
            alas_config::airports::get(name)
                .is_ok_and(|configured| configured.icao == endpoint.icao)
        })
    };
    if let Some((stated, runway)) = config
        .takeoff_override_ignored()
        .filter(|_| is_configured(&config.departure_airport, route.map(|pair| pair.0)))
    {
        findings.push(super::warning(
            FindingCode::TakeoffOverrideIgnored,
            format!(
                "the stated design take-off field length of {stated:.0} m is longer than the departure aerodrome's {runway:.0} m and is ignored: a stated value can only shorten the requirement"
            ),
            Some(stated),
            Some(runway),
            "m",
        ));
    }
    if let Some((stated, runway)) = config
        .landing_override_ignored()
        .filter(|_| is_configured(&config.arrival_airport, route.map(|pair| pair.1)))
    {
        findings.push(super::warning(
            FindingCode::LandingOverrideIgnored,
            format!(
                "the stated design landing field length of {stated:.0} m is longer than the arrival aerodrome's {runway:.0} m and is ignored: a stated value can only shorten the requirement"
            ),
            Some(stated),
            Some(runway),
            "m",
        ));
    }
    if let Some((origin, destination)) = route {
        let (takeoff, landing) = config.stated_field_lengths();
        for (stated, name, flown, kind, label) in [
            (
                takeoff,
                &config.departure_airport,
                origin,
                "take-off",
                "departs from",
            ),
            (
                landing,
                &config.arrival_airport,
                destination,
                "landing",
                "arrives at",
            ),
        ] {
            let Some(stated) = stated else { continue };
            if is_configured(name, Some(flown)) {
                continue;
            }
            // The design sizing reads the configured aerodrome's record, where a
            // shorter stated length does apply; say so rather than "not
            // applied". Whether it applied is read off that very record, so the
            // message follows the sizing if the rule changes.
            let sizing_applied = alas_config::airports::get(name)
                .ok()
                .zip(match kind {
                    "take-off" => config.departure_airport_record().ok(),
                    _ => config.arrival_airport_record().ok(),
                })
                .is_some_and(|(raw, sized)| match kind {
                    "take-off" => sized.toda_m < raw.toda_m,
                    _ => sized.lda_m < raw.lda_m,
                });
            let sizing = if sizing_applied {
                format!("; the design sizing at {name} still uses it")
            } else {
                String::new()
            };
            findings.push(super::warning(
                FindingCode::StatedFieldLengthNotApplied,
                format!(
                    "the stated design {kind} field length of {stated:.0} m belongs to {name}, which the plan {label} {} instead; it plays no part in judging the dispatched aerodromes{sizing}",
                    flown.icao
                ),
                Some(stated),
                None,
                "m",
            ));
        }
    }
    let endpoints: [(Result<&Airport, String>, &str); 2] = match route {
        Some((origin, destination)) => [(Ok(origin), "departure"), (Ok(destination), "arrival")],
        None => [
            (
                config.departure_airport_record().map_err(|e| e.to_string()),
                "departure",
            ),
            (
                config.arrival_airport_record().map_err(|e| e.to_string()),
                "arrival",
            ),
        ],
    };
    for (resolved, role) in endpoints {
        let airport = match resolved {
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
        let field_result = field_for_airport(
            config,
            report,
            airport,
            (wing_area_m2, takeoff_mass_kg, landing_mass_kg, static_tw),
        );
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
            landing_distance_share(config)
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
            // The TOP inverse is the replay's jet constraint. The CS 25.113
            // integrated field distance judges every non-replay aircraft.
            if config.performance.legacy_field_correlations {
                let required = tw_takeoff_constraint(
                    &[wing_loading_pa],
                    airport.toda_m,
                    sigma,
                    config
                        .performance
                        .takeoff_field_cl_max(config.performance.cl_max_to),
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
            let limit =
                ws_landing_limit_for(&config.performance, propeller, airport.lda_m, share, sigma);
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
/// The field performance of the aircraft at `airport` with the given masses and
/// static thrust-to-weight ratio: `(wing area, take-off mass, landing mass, T/W)`.
pub(super) fn field_for_airport(
    config: &AlasConfig,
    report: &AnalysisReport,
    airport: &Airport,
    (wing_area_m2, takeoff_mass_kg, landing_mass_kg, static_tw): (f64, f64, f64, f64),
) -> Result<FieldPerformance, String> {
    let polar = if !config.performance.legacy_field_correlations {
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
        FieldPolar::parabola(0.0, 0.0)
    };
    calculate_with_airframe(
        config,
        Some(YawAirframe {
            plane: &report.airplane,
            cg_x_m: report.physical_cg[0],
        }),
        airport,
        wing_area_m2,
        takeoff_mass_kg,
        landing_mass_kg,
        static_tw,
        &polar,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The departure finding reads the effective take-off distance (shorter
    /// of the route's and the stated one); a longer stated value is ignored
    /// with a visible warning finding.
    #[test]
    fn the_departure_finding_reads_the_stated_take_off_distance_and_reports_an_ignored_one() {
        let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "C919"}))
            .unwrap_or_else(|reason| panic!("preset: {reason}"));
        let preset =
            alas_config::presets::get("C919").unwrap_or_else(|reason| panic!("preset: {reason}"));
        let report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&preset.design_vector, true)
            .unwrap_or_else(|reason| panic!("analysis: {reason}"));
        config.performance.oei_asymmetric_trim_cd = Some(0.001);
        let mtow = config.requirements.mtow_kg;
        let mlw = config.landing_mass_limit_kg(mtow);
        let static_thrust = static_thrust::resolve(&config, 60.0, 1.0);
        let tw = static_thrust.thrust_n / (mtow * config.requirements.gravity_m_s2);
        let run = |config: &AlasConfig| {
            let mut findings = Vec::new();
            assess_airports(
                config,
                &report,
                &mut findings,
                report.airplane.s_ref,
                mtow,
                mlw,
                tw,
                &static_thrust,
            );
            findings
        };
        let route = alas_config::airports::get(&config.departure_airport)
            .unwrap_or_else(|reason| panic!("airport: {reason}"))
            .toda_m;
        assert!(run(&config)
            .iter()
            .all(|f| f.code != FindingCode::TakeoffOverrideIgnored));
        config.toda_override_m = Some(route + 500.0);
        let longer = run(&config);
        let ignored = longer
            .iter()
            .find(|f| f.code == FindingCode::TakeoffOverrideIgnored)
            .unwrap_or_else(|| panic!("the ignored value is reported"));
        assert_eq!(ignored.severity, super::super::FindingSeverity::Warning);
        assert_eq!(
            (ignored.actual, ignored.limit),
            (Some(route + 500.0), Some(route))
        );
        config.toda_override_m = Some(700.0);
        let shorter = run(&config);
        assert!(shorter.iter().any(
            |f| f.code == FindingCode::FieldTakeoffDistanceViolation && f.limit == Some(700.0)
        ));

        // The landing end follows the same rule, at an arrival whose landing
        // distance is binding for this aircraft so the route itself has a finding.
        config.toda_override_m = None;
        config.arrival_airport = "LXGB".to_owned();
        let lda = alas_config::airports::get(&config.arrival_airport)
            .unwrap_or_else(|reason| panic!("airport: {reason}"))
            .lda_m;
        assert!(run(&config)
            .iter()
            .all(|f| f.code != FindingCode::LandingOverrideIgnored));
        config.lda_override_m = Some(lda + 500.0);
        let longer = run(&config);
        let ignored = longer
            .iter()
            .find(|f| f.code == FindingCode::LandingOverrideIgnored)
            .unwrap_or_else(|| panic!("the ignored landing value is reported"));
        assert_eq!(
            (ignored.actual, ignored.limit),
            (Some(lda + 500.0), Some(lda))
        );
        // A longer stated value changes no landing finding; a shorter one tightens them.
        let landing_limits = |findings: &[PhysicalFinding]| {
            findings
                .iter()
                .filter(|f| f.code == FindingCode::FieldLandingDistanceViolation)
                .map(|f| f.limit)
                .collect::<Vec<_>>()
        };
        let route_limits = {
            config.lda_override_m = None;
            landing_limits(&run(&config))
        };
        config.lda_override_m = Some(lda + 500.0);
        assert_eq!(landing_limits(&run(&config)), route_limits);
        assert!(
            !route_limits.is_empty(),
            "the arrival is binding at the route's LDA"
        );
        config.lda_override_m = Some(700.0);
        let shorter = run(&config);
        assert!(shorter
            .iter()
            .all(|f| f.code != FindingCode::LandingOverrideIgnored));
        // Shorter is stricter: a metre limit is the stated 700 m itself, a
        // wing-loading limit is below the route's.
        let tightened = landing_limits(&shorter);
        assert!(!tightened.is_empty());
        let lxgb =
            alas_config::airports::get("LXGB").unwrap_or_else(|reason| panic!("airport: {reason}"));
        let wing_loading_limit = ws_landing_limit_for(
            &config.performance,
            false,
            700.0,
            landing_distance_share(&config),
            density_ratio(lxgb.elevation_m, lxgb.isa_deviation_c),
        );
        assert!(
            tightened
                .iter()
                .all(|limit| *limit == Some(wing_loading_limit) || *limit == Some(700.0)),
            "{tightened:?} vs {wing_loading_limit} Pa or 700 m"
        );
    }

    /// A plan that flies another pair than the configured one is judged at the
    /// aerodromes it flies; the stated lengths stay with the configured ones.
    #[test]
    fn a_swapped_plan_is_judged_at_the_dispatched_aerodromes() {
        let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "C919"}))
            .unwrap_or_else(|reason| panic!("preset: {reason}"));
        let preset =
            alas_config::presets::get("C919").unwrap_or_else(|reason| panic!("preset: {reason}"));
        let report = crate::full_analysis::FullAnalysis::new(config.clone())
            .run(&preset.design_vector, true)
            .unwrap_or_else(|reason| panic!("analysis: {reason}"));
        config.performance.oei_asymmetric_trim_cd = Some(0.001);
        config.arrival_airport = "LXGB".to_owned();
        let mtow = config.requirements.mtow_kg;
        let mlw = config.landing_mass_limit_kg(mtow);
        let static_thrust = static_thrust::resolve(&config, 60.0, 1.0);
        let tw = static_thrust.thrust_n / (mtow * config.requirements.gravity_m_s2);
        let departure = alas_config::airports::get(&config.departure_airport)
            .unwrap_or_else(|reason| panic!("airport: {reason}"));
        let arrival = alas_config::airports::get(&config.arrival_airport)
            .unwrap_or_else(|reason| panic!("airport: {reason}"));
        // Stated lengths longer than both runways: ignored, with a notice, at
        // the configured ends only.
        config.toda_override_m = Some(departure.toda_m + 500.0);
        config.lda_override_m = Some(arrival.lda_m + 500.0);
        let run_with = |config: &AlasConfig, route: Option<(&Airport, &Airport)>| {
            let mut findings = Vec::new();
            assess_airports_on_route(
                config,
                &report,
                &mut findings,
                report.airplane.s_ref,
                mtow,
                mlw,
                tw,
                &static_thrust,
                route,
            );
            findings
        };
        let run = |route: Option<(&Airport, &Airport)>| run_with(&config, route);
        let has = |findings: &[PhysicalFinding], code: FindingCode| {
            findings.iter().any(|f| f.code == code)
        };
        let limits = |findings: &[PhysicalFinding], code: FindingCode| {
            findings
                .iter()
                .filter(|f| f.code == code)
                .map(|f| f.limit)
                .collect::<Vec<_>>()
        };
        let configured = run(None);
        assert!(has(&configured, FindingCode::TakeoffOverrideIgnored));
        assert!(has(&configured, FindingCode::LandingOverrideIgnored));
        assert!(!has(&configured, FindingCode::StatedFieldLengthNotApplied));
        // The configured pair through the stated-length rule changes nothing.
        let (origin, destination) = config.with_stated_for_route(departure, arrival);
        assert_eq!(run(Some((&origin, &destination))), configured);
        // Shorter stated lengths go through the same rule on the route path.
        let mut shorter = config.clone();
        shorter.toda_override_m = Some(700.0);
        shorter.lda_override_m = Some(700.0);
        let (origin, destination) = shorter.with_stated_for_route(departure, arrival);
        assert_eq!((origin.toda_m, destination.lda_m), (700.0, 700.0));
        let tightened = run_with(&shorter, Some((&origin, &destination)));
        assert_eq!(tightened, run_with(&shorter, None));
        assert_eq!(
            limits(&tightened, FindingCode::FieldTakeoffDistanceViolation),
            [Some(700.0)]
        );
        // The plan flies the other way: the notices belong to the configured
        // aerodromes, which are no longer its endpoints, so a finding says the
        // stated lengths are not applied, and the judgement is the dispatched
        // aerodromes' own (the configured arrival is now the departure).
        let swapped = run(Some((arrival, departure)));
        assert!(!has(&swapped, FindingCode::TakeoffOverrideIgnored));
        assert!(!has(&swapped, FindingCode::LandingOverrideIgnored));
        assert_eq!(
            swapped
                .iter()
                .filter(|f| f.code == FindingCode::StatedFieldLengthNotApplied)
                .count(),
            2,
            "one finding for each stated length"
        );
        let mut unstated = config.clone();
        unstated.toda_override_m = None;
        unstated.lda_override_m = None;
        // At the configured arrival the aircraft's landing is binding (the
        // same fixture as `arrival_gate_compares_factored_dispatch_length_with_lda`).
        assert!(!limits(
            &run_with(&unstated, None),
            FindingCode::FieldLandingDistanceViolation
        )
        .is_empty());
        // Flown the other way that arrival is the departure: no landing finding
        // at the configured departure, and the take-off is judged at its TODA.
        assert!(limits(&swapped, FindingCode::FieldLandingDistanceViolation).is_empty());
        let swapped_unstated = run_with(&unstated, Some((arrival, departure)));
        assert_eq!(
            limits(&swapped, FindingCode::FieldTakeoffDistanceViolation),
            limits(
                &swapped_unstated,
                FindingCode::FieldTakeoffDistanceViolation
            ),
            "the stated lengths play no part in the swapped judgement"
        );
        assert_eq!(
            limits(&swapped, FindingCode::FieldTakeoffDistanceViolation),
            [Some(arrival.toda_m)]
        );
        // A shorter, binding stated length is not applied on the swapped plan
        // either, and the finding says the design sizing still uses it.
        let swapped_shorter = run_with(&shorter, Some((arrival, departure)));
        let not_applied: Vec<_> = swapped_shorter
            .iter()
            .filter(|f| f.code == FindingCode::StatedFieldLengthNotApplied)
            .collect();
        assert_eq!(not_applied.len(), 2);
        assert!(
            not_applied
                .iter()
                .all(|f| f.message.contains("design sizing at")
                    && f.message.contains("still uses it"))
        );
        assert!(!has(&swapped_shorter, FindingCode::TakeoffOverrideIgnored));
        assert_eq!(
            limits(&swapped_shorter, FindingCode::FieldTakeoffDistanceViolation),
            limits(
                &swapped_unstated,
                FindingCode::FieldTakeoffDistanceViolation
            )
        );
        assert_eq!(
            limits(&swapped_shorter, FindingCode::FieldLandingDistanceViolation),
            limits(
                &swapped_unstated,
                FindingCode::FieldLandingDistanceViolation
            )
        );
        // A longer one is ignored by the sizing too, and the finding does not
        // claim otherwise.
        assert!(swapped
            .iter()
            .filter(|f| f.code == FindingCode::StatedFieldLengthNotApplied)
            .all(|f| !f.message.contains("still uses it")));
    }

    #[test]
    fn arrival_gate_compares_factored_dispatch_length_with_lda() {
        let mut config = AlasConfig::from_value(&serde_json::json!({"preset": "C919"}))
            .unwrap_or_else(|reason| panic!("preset: {reason}"));
        config.arrival_airport = "LXGB".to_owned();
        let preset =
            alas_config::presets::get("C919").unwrap_or_else(|reason| panic!("preset: {reason}"));
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
        // The arrival gate is under test; the jet take-off leg of `calculate`
        // needs an explicit asymmetric-trim assumption without a fin.
        let mut field_config = config.clone();
        field_config.performance.oei_asymmetric_trim_cd = Some(0.001);
        let field = crate::field_performance::calculate(
            &field_config,
            airport,
            area,
            mtow,
            mlw,
            tw,
            0.02,
            0.04,
        )
        .unwrap_or_else(|reason| panic!("field: {reason}"));
        assert!(field.ldr_m < airport.lda_m);
        assert!(field.ldr_m / landing_distance_share(&config) > airport.lda_m);
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
