// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Explicit physical-feasibility findings for a completed design run.
//!
//! A solver completing is not evidence that the aircraft can fly the stated
//! load case. The pipeline therefore carries violated conservation laws and
//! configured limits beside its numerical outputs. Keeping the measured value,
//! limit and unit makes a finding diagnosable without converting it into an
//! arbitrary scalar penalty or hiding it behind a single Boolean.

#[cfg(test)]
use alas_config::CgEnvelopeEvidence;
use alas_config::{AlasConfig, DesignVector};
use alas_mass::breakdown::{
    calculate_physical_cg, MassBreakdown, MassCoordinates, FUEL, FURNISHINGS, FUSELAGE, GEAR,
    H_STAB, PAYLOAD, PROPULSION, SYSTEMS, V_STAB, WING,
};
use alas_mission::MissionResult;
use alas_opt::{assess_model_cg_envelope, ModelCgConstraint, ModelCgEnvelopeAssessment};
use alas_perf::performance::{
    assess_oei_climb, compute_field_performance_at_masses, compute_v_speeds_at_masses,
    density_ratio, far25_oei_gradient, tw_cruise_constraint, tw_takeoff_constraint,
    ws_landing_limit, OeiClimbStatus, OeiV2Condition,
};

use crate::full_analysis::AnalysisReport;
use crate::mission_stage::SelectedLoadCase;

mod cruise_equilibrium;
mod dispatch;
mod fuel;
mod mass_balance;
mod planning;
mod report_format;
mod reported_attitude;
mod structural_mass;
mod types;

pub(crate) use cruise_equilibrium::assess as assess_cruise_equilibrium;
pub use cruise_equilibrium::CruiseEquilibriumAssessment;
pub use dispatch::{DispatchAssessment, DispatchOutcome};
pub use fuel::{
    assess_fuel_capacity, CarriedFuelBasis, FuelCapacityAssessment, FuelCapacityEvidence,
    FuelLoadingAssessment, MissionFuelAssessment, MissionFuelStatus,
};
pub(crate) use fuel::{plan_fuel_loading, report_mass_basis_kg};
pub use mass_balance::{
    takeoff_mass_properties, LedgerItemSummary, MassBalanceAssessment, MassStateSummary,
    TankSummary,
};
use planning::assess_public_cg_reference;
#[cfg(test)]
use planning::{assess_reference_limits, planning_cg_pct_mac};
pub use report_format::format_feasibility;
pub use types::*;

fn error(
    code: FindingCode,
    message: impl Into<String>,
    actual: Option<f64>,
    limit: Option<f64>,
    unit: &'static str,
) -> PhysicalFinding {
    PhysicalFinding {
        code,
        severity: FindingSeverity::Error,
        message: message.into(),
        actual,
        limit,
        unit,
    }
}

fn model_cg_assessment(
    config: &AlasConfig,
    report: &AnalysisReport,
    fuel_loading: &FuelLoadingAssessment,
) -> Result<ModelCgEnvelopeAssessment, String> {
    let mass = |name: &str| {
        report
            .component_masses
            .get(name)
            .copied()
            .ok_or_else(|| format!("model CG assessment is missing {name} mass"))
    };
    let coordinate = |name: &str| {
        report
            .mass_coordinates
            .get(name)
            .copied()
            .ok_or_else(|| format!("model CG assessment is missing {name} coordinates"))
    };
    let mut masses = MassBreakdown {
        wing: mass(WING)?,
        h_stab: mass(H_STAB)?,
        v_stab: mass(V_STAB)?,
        fuselage: mass(FUSELAGE)?,
        gear: mass(GEAR)?,
        propulsion: mass(PROPULSION)?,
        systems: mass(SYSTEMS)?,
        furnishings: mass(FURNISHINGS)?,
        payload: mass(PAYLOAD)?,
        fuel: mass(FUEL)?,
    };
    let coordinates = MassCoordinates {
        wing: coordinate(WING)?,
        h_stab: coordinate(H_STAB)?,
        v_stab: coordinate(V_STAB)?,
        fuselage: coordinate(FUSELAGE)?,
        gear: coordinate(GEAR)?,
        propulsion: coordinate(PROPULSION)?,
        systems: coordinate(SYSTEMS)?,
        furnishings: coordinate(FURNISHINGS)?,
        payload: coordinate(PAYLOAD)?,
        fuel: coordinate(FUEL)?,
    };
    masses.fuel = fuel_loading.analyzed_carried_fuel_kg;
    let analyzed_cg = calculate_physical_cg(&masses, &coordinates);
    assess_model_cg_envelope(
        &report.airplane,
        &masses,
        &coordinates,
        analyzed_cg[0],
        report.x_neutral_point,
        report.airplane.c_ref,
        config,
    )
    .map_err(|error| error.to_string())
}

fn append_model_cg_findings(
    findings: &mut Vec<PhysicalFinding>,
    assessment: &ModelCgEnvelopeAssessment,
) {
    let constraints = [
        ModelCgConstraint::StaticStabilityFloor,
        ModelCgConstraint::ConfiguredForwardCgRange,
        ModelCgConstraint::NoseGearStrength,
        ModelCgConstraint::MainGearStrength,
        ModelCgConstraint::MinimumNoseGearLoad,
    ];
    for constraint in constraints {
        let worst = assessment
            .loading_states
            .iter()
            .flat_map(|state| {
                state
                    .constraints
                    .iter()
                    .filter(move |item| item.constraint == constraint && item.violated)
                    .map(move |item| (state.state, item))
            })
            .max_by(|(_, left), (_, right)| {
                left.normalized_exceedance
                    .total_cmp(&right.normalized_exceedance)
            });
        let Some((state, result)) = worst else {
            continue;
        };
        let code = match constraint {
            ModelCgConstraint::StaticStabilityFloor => FindingCode::InsufficientStaticMargin,
            ModelCgConstraint::ConfiguredForwardCgRange => {
                FindingCode::ModelCgForwardRangeViolation
            }
            ModelCgConstraint::NoseGearStrength => FindingCode::NoseGearStrengthViolation,
            ModelCgConstraint::MainGearStrength => FindingCode::MainGearStrengthViolation,
            ModelCgConstraint::MinimumNoseGearLoad => FindingCode::MinimumNoseGearLoadViolation,
        };
        findings.push(error(
            code,
            format!(
                "{} loading state violates the model {} constraint",
                state.label(),
                constraint.label()
            ),
            Some(result.actual),
            Some(result.limit),
            constraint.unit(),
        ));
    }
}

/// Evaluate conservation laws and configured limits on a completed run.
///
/// Without a selected load case the analyzed fuel is the takeoff-mass closure
/// remainder, which is what a caller that did not fly the mission has to use.
pub fn assess_physical_feasibility(
    config: &AlasConfig,
    design: &DesignVector,
    report: &AnalysisReport,
    mission: Option<&MissionResult>,
) -> FeasibilityReport {
    assess_physical_feasibility_with_load_case(config, design, report, mission, None)
}

/// Evaluate a run whose mission was flown at a selected load case.
///
/// The load case rewrites the analyzed fuel to what was actually flown and
/// carries the reserve plan it was sized to into the report.
pub fn assess_physical_feasibility_with_load_case(
    config: &AlasConfig,
    design: &DesignVector,
    report: &AnalysisReport,
    mission: Option<&MissionResult>,
    load_case: Option<&SelectedLoadCase>,
) -> FeasibilityReport {
    let mut findings = Vec::new();
    let envelope = alas_perf::performance::build_vn_diagram(
        report.airplane.s_ref,
        &config.requirements,
        &config.performance,
        config.requirements.cruise_altitude_m,
    );
    if let Err(message) = envelope.validate_speed_order() {
        findings.push(error(
            FindingCode::InvalidEnvelopeSpeedOrder,
            message,
            Some(envelope.v_a_kt),
            Some(envelope.v_c_kt),
            "kt EAS",
        ));
    }
    let lift_to_drag = report.design_point.l_over_d;
    if !lift_to_drag.is_finite() || lift_to_drag <= 0.0 {
        findings.push(error(
            FindingCode::InvalidCruiseAerodynamics,
            "cruise analysis did not produce a positive finite lift-to-drag ratio",
            Some(lift_to_drag),
            Some(0.0),
            "dimensionless",
        ));
    }

    let mut fuel_loading = plan_fuel_loading(config, design, report);
    dispatch::apply_load_case(&mut fuel_loading, load_case, &mut findings);
    fuel_loading.analyzed_landing_mass_kg = mission
        .and_then(MissionResult::completed_summary)
        .map(|summary| summary.landing_mass_kg);
    let cruise_equilibrium = mission.map(assess_cruise_equilibrium);
    if let Some(assessment) = &cruise_equilibrium {
        if !assessment.is_finite() {
            findings.push(error(
                FindingCode::InvalidCruiseForceBalance,
                "cruise force-balance record contains no finite solved control point",
                None,
                None,
                "",
            ));
        }
    }
    // Public planning limits apply to the load actually carried: a mass-closure
    // remainder can exceed usable tank capacity, so `report.physical_cg` would
    // compare a capped mass case against a CG still holding uncarried fuel.
    let cg_envelope =
        assess_public_cg_reference(config, report, fuel_loading.analyzed_carried_fuel_kg);
    fuel_loading.mission = fuel::assess_mission_fuel(config.mission.enabled, mission);
    findings.extend(fuel::findings(config.requirements.mtow_kg, &fuel_loading));
    structural_mass::append_structural_mass_findings(
        config,
        design,
        report,
        &fuel_loading,
        &mut findings,
    );

    let model_cg = match model_cg_assessment(config, report, &fuel_loading) {
        Ok(assessment) => {
            append_model_cg_findings(&mut findings, &assessment);
            Some(assessment)
        }
        Err(message) => {
            findings.push(error(
                FindingCode::ModelCgAssessmentUnavailable,
                message,
                None,
                None,
                "",
            ));
            None
        }
    };

    if let Some(alas_payload::layout::LayoutSummary::Passenger(summary)) =
        report.payload_layout.as_ref().map(|layout| &layout.summary)
    {
        if summary.unseated_pax > 0 {
            findings.push(error(
                FindingCode::PassengerCapacityShortfall,
                format!(
                    "passenger payload leaves {} requested passengers without seats",
                    summary.unseated_pax
                ),
                Some(summary.seated_pax as f64),
                Some(summary.total_pax as f64),
                "passengers",
            ));
        }
    }
    if let Some(alas_payload::layout::LayoutSummary::Cargo(summary)) =
        report.payload_layout.as_ref().map(|layout| &layout.summary)
    {
        let requested_net_kg = summary.requested_net_payload_t * 1_000.0;
        let loaded_net_kg = summary.loaded_net_payload_t * 1_000.0;
        if requested_net_kg.is_finite()
            && loaded_net_kg.is_finite()
            && loaded_net_kg + 1.0e-6 < requested_net_kg
        {
            findings.push(error(
                FindingCode::CargoCapacityShortfall,
                format!(
                    "cargo layout delivers {:.1} kg net against {:.1} kg requested",
                    loaded_net_kg, requested_net_kg
                ),
                Some(loaded_net_kg),
                Some(requested_net_kg),
                "kg net",
            ));
        }
    }

    if matches!(
        cg_envelope.planning_status,
        PlanningCgStatus::ForwardLimitViolation | PlanningCgStatus::AftLimitViolation
    ) {
        let limit = match cg_envelope.planning_status {
            PlanningCgStatus::ForwardLimitViolation => cg_envelope.forward_limit_pct_mac,
            PlanningCgStatus::AftLimitViolation => cg_envelope.aft_limit_pct_mac,
            _ => None,
        };
        findings.push(error(
            FindingCode::PublicPlanningCgEnvelopeViolation,
            "analyzed CG lies outside the manufacturer public planning envelope; actual aircraft WBM controls",
            cg_envelope.cg_pct_mac,
            limit,
            "% MAC",
        ));
    }

    findings.extend(reported_attitude::assess(config, report));
    let trim_is_finite = report.trimmed_design_point.as_ref().is_some_and(|trim| {
        trim.alpha_deg.is_finite()
            && trim.geometric_body_alpha_deg.is_finite()
            && trim.trim_ih_deg.is_finite()
            && trim.cl.is_finite()
            && trim.cd.is_finite()
            && trim.cm_residual.is_finite()
    });
    if !trim_is_finite {
        findings.push(error(
            FindingCode::TrimUnavailable,
            "cruise trim did not produce a finite solved operating point",
            None,
            None,
            "dimensionless",
        ));
    }

    let projected_area_m2 = report
        .airplane
        .wings
        .first()
        .map_or(f64::NAN, alas_geom::aircraft::wing::Wing::projected_area);
    let area_roundoff_m2 = config.requirements.max_wing_area_m2.abs().max(1.0) * 1.0e-12;
    if !projected_area_m2.is_finite()
        || projected_area_m2 > config.requirements.max_wing_area_m2 + area_roundoff_m2
    {
        findings.push(error(
            FindingCode::WingAreaLimit,
            "projected wing reference area exceeds the configured maximum",
            Some(projected_area_m2),
            Some(config.requirements.max_wing_area_m2),
            "m^2",
        ));
    }

    // Field performance is a feasibility check, not a report-only chart: use the
    // selected engine rating for static thrust, and arrival telemetry when it
    // exists, rather than silently evaluating landing at MTOW.
    let wing_area_m2 = report
        .geometry_summary
        .get("wing_area_m2")
        .copied()
        .or(Some(report.airplane.s_ref))
        .unwrap_or(f64::NAN);
    let n_engines = config.geometry.engine.spanwise_positions_m.len() as f64;
    let mtow_kg = config.requirements.mtow_kg;
    let takeoff_mass_kg = fuel_loading.analyzed_takeoff_mass_kg;
    let gravity_m_s2 = config.requirements.gravity_m_s2;
    let static_thrust_n = n_engines * config.geometry.engine.thrust_kn() * 1000.0;
    let static_tw = static_thrust_n / (takeoff_mass_kg * gravity_m_s2);
    let mlw_limit_kg = config.landing_mass_limit_kg(mtow_kg);
    let landing_mass_kg = fuel_loading
        .analyzed_landing_mass_kg
        .unwrap_or(mlw_limit_kg.min(takeoff_mass_kg));
    if fuel_loading
        .analyzed_landing_mass_kg
        .is_some_and(|mass_kg| mass_kg > mlw_limit_kg)
    {
        findings.push(error(
            FindingCode::LandingMassLimitViolation,
            "analyzed arrival mass exceeds the configured maximum landing mass",
            Some(landing_mass_kg),
            Some(mlw_limit_kg),
            "kg",
        ));
    }
    let wing_loading_pa = takeoff_mass_kg * gravity_m_s2 / wing_area_m2;
    let matching_inputs_are_finite = wing_loading_pa.is_finite()
        && wing_loading_pa > 0.0
        && report.polar_fit.cd0.is_finite()
        && report.polar_fit.k.is_finite()
        && report.polar_fit.cd0 >= 0.0
        && report.polar_fit.k >= 0.0
        && config.requirements.cruise_mach.is_finite()
        && config.requirements.cruise_mach > 0.0
        && config.requirements.cruise_altitude_m.is_finite()
        && config.performance.thrust_lapse.is_finite()
        && config.performance.thrust_lapse > 0.0
        && config.performance.cl_max_to.is_finite()
        && config.performance.cl_max_to > 0.0
        && config.performance.cl_max_land.is_finite()
        && config.performance.cl_max_land > 0.0
        && config.performance.k_land.is_finite()
        && config.performance.k_land > 0.0;
    if !matching_inputs_are_finite {
        findings.push(error(
            FindingCode::FieldPerformanceUnavailable,
            "matching-chart constraints require finite positive wing loading and performance inputs",
            Some(wing_loading_pa),
            Some(0.0),
            "Pa",
        ));
    }
    if matching_inputs_are_finite && static_tw.is_finite() && static_tw > 0.0 {
        let cruise_required_tw = tw_cruise_constraint(
            &[wing_loading_pa],
            report.polar_fit.cd0,
            report.polar_fit.k,
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m,
            config.performance.thrust_lapse,
        )
        .first()
        .copied()
        .unwrap_or(f64::NAN);
        let n_engines_i64 = config.geometry.engine.spanwise_positions_m.len() as i64;
        let certified_oei_gradient = far25_oei_gradient(n_engines_i64);
        let oei_condition = alas_config::airports::get(&config.departure_airport)
            .ok()
            .map(|departure| {
                let speeds = compute_v_speeds_at_masses(
                    takeoff_mass_kg,
                    takeoff_mass_kg,
                    wing_area_m2,
                    departure,
                    config.performance.cl_max_to,
                    config.performance.cl_max_land,
                    &config.performance,
                );
                OeiV2Condition {
                    departure_elevation_m: departure.elevation_m,
                    departure_isa_deviation_c: departure.isa_deviation_c,
                    v2_over_vstall: speeds.v2_ms / speeds.v_stall_to_ms,
                    condition_to_sls_thrust_ratio: config
                        .performance
                        .oei_condition_to_sls_thrust_ratio,
                    asymmetric_trim_cd: config.performance.oei_asymmetric_trim_cd,
                    windmilling_cd: config.performance.oei_windmilling_cd,
                }
            });
        let oei_assessment = assess_oei_climb(
            report.polar_fit.cd0,
            report.polar_fit.k,
            n_engines_i64,
            certified_oei_gradient.unwrap_or(config.performance.oei_gradient),
            config.performance.oei_climb_cl,
            config.performance.oei_climb_delta_cd,
            config.performance.cl_max_to,
            oei_condition,
        );
        if cruise_required_tw.is_finite() && static_tw < cruise_required_tw {
            findings.push(error(
                FindingCode::ThrustMarginViolation,
                format!(
                    "cruise requires static T/W {:.4}, but the configured rating provides {:.4}",
                    cruise_required_tw, static_tw
                ),
                Some(static_tw),
                Some(cruise_required_tw),
                "T/W",
            ));
        }
        // An in-flight OEI estimate is useful as a diagnostic, but it is not
        // dimensionally comparable with the SLS axis used here. Only the
        // shared assessor's condition-specific SLS result may create a hard
        // thrust-margin finding. Surface the missing evidence as a warning so
        // callers do not mistake a conceptual fallback for Part 25 evidence.
        if let Some(required_sls_tw) = oei_assessment.required_sls_tw {
            if required_sls_tw.is_finite() && static_tw < required_sls_tw {
                findings.push(error(
                    FindingCode::ThrustMarginViolation,
                    format!(
                        "engine-out second-segment climb requires static T/W {:.4}, but the configured rating provides {:.4}",
                        required_sls_tw, static_tw
                    ),
                    Some(static_tw),
                    Some(required_sls_tw),
                    "T/W",
                ));
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
    }
    for (airport_name, role) in [
        (&config.departure_airport, "departure"),
        (&config.arrival_airport, "arrival"),
    ] {
        let airport = match alas_config::airports::get(airport_name) {
            Ok(airport) => airport,
            Err(airport_error) => {
                findings.push(error(
                    FindingCode::FieldPerformanceUnavailable,
                    format!("{role} airport cannot be resolved: {airport_error}"),
                    None,
                    None,
                    "",
                ));
                continue;
            }
        };
        if !wing_area_m2.is_finite()
            || wing_area_m2 <= 0.0
            || !takeoff_mass_kg.is_finite()
            || takeoff_mass_kg <= 0.0
            || (role == "departure" && (!static_tw.is_finite() || static_tw <= 0.0))
        {
            findings.push(error(
                FindingCode::FieldPerformanceUnavailable,
                format!("{role} field-performance inputs are not finite and positive"),
                Some(wing_area_m2),
                Some(0.0),
                "m^2",
            ));
            continue;
        }
        let field = compute_field_performance_at_masses(
            takeoff_mass_kg,
            landing_mass_kg,
            wing_area_m2,
            airport,
            config.performance.cl_max_to,
            config.performance.cl_max_land,
            static_tw.max(1.0e-6),
            config.performance.k_land,
            config.performance.bfl_factor,
            &config.performance,
        );
        let sigma = density_ratio(airport.elevation_m, airport.isa_deviation_c);
        let takeoff_required_tw = tw_takeoff_constraint(
            &[wing_loading_pa],
            airport.toda_m,
            sigma,
            config.performance.cl_max_to,
        )
        .first()
        .copied()
        .unwrap_or(f64::NAN);
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
            if takeoff_required_tw.is_finite() && static_tw < takeoff_required_tw {
                findings.push(error(
                    FindingCode::ThrustMarginViolation,
                    format!(
                        "departure static T/W {:.4} is below field requirement {:.4}",
                        static_tw, takeoff_required_tw
                    ),
                    Some(static_tw),
                    Some(takeoff_required_tw),
                    "T/W",
                ));
            }
        } else {
            let landing_wing_loading_pa = landing_mass_kg * gravity_m_s2 / wing_area_m2;
            let landing_limit_pa = ws_landing_limit(
                airport.lda_m,
                sigma,
                config.performance.cl_max_land,
                config.performance.k_land,
            );
            if landing_wing_loading_pa.is_finite()
                && landing_limit_pa.is_finite()
                && landing_wing_loading_pa > landing_limit_pa
            {
                landing_constraint_violation = true;
                findings.push(error(
                    FindingCode::FieldLandingDistanceViolation,
                    format!(
                        "arrival wing loading {:.1} Pa exceeds landing limit {:.1} Pa",
                        landing_wing_loading_pa, landing_limit_pa
                    ),
                    Some(landing_wing_loading_pa),
                    Some(landing_limit_pa),
                    "Pa",
                ));
            }
        }
        if role == "arrival" && !landing_constraint_violation && !field.land_feasible() {
            findings.push(error(
                FindingCode::FieldLandingDistanceViolation,
                format!(
                    "arrival LDR {:.1} m exceeds LDA {:.1} m at landing mass {:.1} kg",
                    field.ldr_m,
                    field.lda_m(),
                    field.landing_mass_kg
                ),
                Some(field.ldr_m),
                Some(field.lda_m()),
                "m",
            ));
        }
    }

    if config.mission.enabled {
        match mission {
            None => findings.push(error(
                FindingCode::MissionUnavailable,
                "mission analysis was requested but produced no telemetry",
                None,
                None,
                "",
            )),
            Some(result) => {
                if let Some(exhaustion) = &result.fuel_exhaustion {
                    findings.push(error(
                        FindingCode::MissionFuelShortfall,
                        format!(
                            "usable fuel was exhausted during mission segment {}",
                            exhaustion.segment_tag
                        ),
                        Some(exhaustion.burned_fuel_kg),
                        Some(exhaustion.available_fuel_kg),
                        "kg",
                    ));
                }
                if result.solutions.is_empty()
                    || result.solutions.iter().any(|solution| !solution.converged)
                {
                    findings.push(error(
                        FindingCode::MissionNotConverged,
                        "at least one native mission segment did not converge",
                        None,
                        None,
                        "",
                    ));
                }
                if result
                    .solutions
                    .iter()
                    .any(|solution| solution.throttle_limited)
                {
                    findings.push(error(
                        FindingCode::MissionThrottleLimitViolation,
                        "mission reached the available 1.000 throttle boundary before force balance converged",
                        Some(1.0),
                        Some(1.0),
                        "fraction",
                    ));
                }
                let burned_kg = fuel_loading
                    .mission
                    .burned_fuel_kg
                    .unwrap_or_else(|| result.fuel_burned_kg());
                if !burned_kg.is_finite() || burned_kg <= 0.0 {
                    findings.push(error(
                        FindingCode::InvalidMissionFuelBurn,
                        "mission fuel burn is not a positive finite quantity",
                        Some(burned_kg),
                        Some(0.0),
                        "kg",
                    ));
                } else if result.fuel_exhaustion.is_none()
                    && fuel_loading.analyzed_carried_fuel_kg.is_finite()
                    && burned_kg > fuel_loading.analyzed_carried_fuel_kg
                {
                    findings.push(error(
                        FindingCode::MissionFuelShortfall,
                        "mission fuel burn exceeds the fuel carried in this load case",
                        Some(burned_kg),
                        Some(fuel_loading.analyzed_carried_fuel_kg),
                        "kg",
                    ));
                }
            }
        }
    }

    let mass_balance =
        mass_balance::assess_mass_balance(config, design, report, &fuel_loading, &mut findings);
    FeasibilityReport {
        findings,
        cg_envelope,
        model_cg,
        fuel_loading,
        cruise_equilibrium,
        mass_balance,
    }
}

// The registry lookups are test preconditions: a missing shipped preset is the
// failure being reported, rather than a recoverable library condition.
#[allow(clippy::expect_used)]
#[cfg(test)]
#[path = "feasibility_tests.rs"]
mod tests;
