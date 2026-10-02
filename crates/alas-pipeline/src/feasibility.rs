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
#[cfg(test)]
use alas_mass::breakdown::{FUEL, PROPULSION};
use alas_mission::MissionResult;
use alas_perf::performance::{
    compute_v_speeds_at_masses, density_ratio, OeiClimbAssessment, OeiClimbStatus,
};

use crate::full_analysis::AnalysisReport;
use crate::mission_stage::SelectedLoadCase;

mod acceptance;
mod cruise_equilibrium;
mod design_mass;
mod dispatch;
mod field;
mod fuel;
mod mass_balance;
mod mission_fuel;
mod model_cg;
mod oei_drag;
mod operational_envelope;
mod payload_findings;
mod phase_limits;
mod planning;
mod report_format;
mod reported_attitude;
mod static_thrust;
mod structural_mass;
pub(crate) mod structure;
mod types;

pub use acceptance::{
    DeliveryBlocker, DeliveryClassification, DeliveryVerdict, DesignProvenance, RunCompletion,
};
pub(crate) use cruise_equilibrium::assess as assess_cruise_equilibrium;
pub use cruise_equilibrium::CruiseEquilibriumAssessment;
pub use design_mass::{
    design_mass_config, design_vn_diagram, design_vn_mass_kg, landing_mass_limit_kg,
};
pub use dispatch::{DispatchAssessment, DispatchOutcome};
pub(crate) use fuel::plan_fuel_loading;
pub use fuel::{
    assess_airplane_fuel_capacity, assess_fuel_capacity, CarriedFuelBasis, FuelCapacityAssessment,
    FuelCapacityEvidence, FuelLoadingAssessment,
};
pub use mass_balance::{
    takeoff_mass_properties, LedgerItemSummary, MassBalanceAssessment, MassStateSummary,
    TankSummary,
};
pub use mission_fuel::{MissionFuelAssessment, MissionFuelStatus, NativeMissionTelemetry};
pub use operational_envelope::{
    append_envelope_findings, assess_operational_envelope, balance_index, CheckedPoint,
    OperationalEnvelopeAssessment,
};
pub use phase_limits::CheckedPhase;
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

/// Same shape as [`error`], severity [`FindingSeverity::Warning`]: for
/// findings against a configured/assumed requirement rather than a physical
/// limit (e.g. `ModelCgConstraint::MinimumUsableCgRange`'s configured
/// `cg_range_pct_mac`), which must not reject an otherwise-feasible design
/// on the strength of an assumption alone.
fn warning(
    code: FindingCode,
    message: impl Into<String>,
    actual: Option<f64>,
    limit: Option<f64>,
    unit: &'static str,
) -> PhysicalFinding {
    PhysicalFinding {
        code,
        severity: FindingSeverity::Warning,
        message: message.into(),
        actual,
        limit,
        unit,
    }
}

use model_cg::{append_model_cg_findings, model_cg_assessment};

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
    structure::append_native(config, design, report, &mut findings);
    let envelope = design_vn_diagram(config, report);
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
    // The arrival mass is the route's on the unified model's frozen plan:
    // takeoff mass less trip, which for a settled dispatch is the zero-fuel
    // mass plus reserves, extra and the taxi-in budget. The native flight is
    // telemetry and never sets it.
    fuel_loading.analyzed_landing_mass_kg = fuel_loading.dispatch.and_then(|dispatch| {
        dispatch
            .plan
            .map(|plan| dispatch.takeoff_mass_kg - plan.trip.kg)
    });
    let cruise_equilibrium = mission.map(assess_cruise_equilibrium);
    if let Some(assessment) = &cruise_equilibrium {
        if !assessment.is_finite() {
            findings.push(warning(
                FindingCode::InvalidCruiseForceBalance,
                "native mission cruise force-balance record contains no finite solved control point (telemetry only)",
                None,
                None,
                "",
            ));
        }
    }
    fuel_loading.mission = mission_fuel::assess_mission_fuel(
        config.mission.enabled,
        fuel_loading.dispatch.as_ref(),
        mission,
    );
    findings.extend(fuel::findings(config.requirements.mtow_kg, &fuel_loading));
    structural_mass::append_structural_mass_findings(
        config,
        design,
        report,
        &fuel_loading,
        &mut findings,
    );

    // Built before the model CG assessment: when the ledger
    // exists, `model_cg_assessment` reads its OEW/ZFW/TOW points instead of
    // the lumped ten-group model's, so the hard gate evaluates the same
    // tank-fill-order, detailed-payload centre of gravity this statement
    // shows a reviewer. `assess_mass_balance` depends only on `config`,
    // `design`, `report` and the already-finalized `fuel_loading` above, so
    // moving it ahead of `model_cg_assessment` changes no result of its own.
    let mass_balance =
        mass_balance::assess_mass_balance(config, design, report, &fuel_loading, &mut findings);
    let model_cg = match model_cg_assessment(config, report, &fuel_loading, mass_balance.as_ref()) {
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
    let operational_envelope = model_cg.as_ref().and_then(|assessment| {
        operational_envelope::assess_operational_envelope(config, report, assessment)
    });
    operational_envelope::append_envelope_findings(operational_envelope.as_ref(), &mut findings);
    // Public planning limits apply to the load actually carried: a mass-closure
    // remainder can exceed usable tank capacity, so `report.physical_cg` would
    // compare a capped mass case against a CG still holding uncarried fuel.
    //
    // Computed after `model_cg`/`operational_envelope` : the
    // planning-curve sweep needs every named loading state and every
    // potato/fuel-vector extreme those two already built, not just the one
    // analyzed point the frozen single-point comparison used.
    let cg_envelope = assess_public_cg_reference(
        config,
        report,
        fuel_loading.analyzed_carried_fuel_kg,
        model_cg.as_ref(),
        operational_envelope.as_ref(),
    );
    payload_findings::append_payload_findings(report, &mut findings);

    if matches!(
        cg_envelope.planning_status,
        PlanningCgStatus::ForwardLimitViolation | PlanningCgStatus::AftLimitViolation
    ) {
        let limit = match cg_envelope.planning_status {
            PlanningCgStatus::ForwardLimitViolation => cg_envelope.forward_limit_pct_mac,
            PlanningCgStatus::AftLimitViolation => cg_envelope.aft_limit_pct_mac,
            _ => None,
        };
        // The violation stands as reported. What is added is the frame
        // evidence a reader needs to act on it: the moment sum is built on
        // the model's own component stations while the percentage is referred
        // to the manufacturer's published leading edge and chord, so a datum
        // or chord offset between the two shifts every reported percentage
        // systematically. Stating the offset does not resolve which reference
        // is wrong for this preset (that is a source reconciliation) and it
        // does not move a published vertex or a verdict.
        let frame_note = if cg_envelope.mac_references_disagree() {
            let datum_shift = cg_envelope
                .mac_datum_shift_pct_mac()
                .map(|shift| format!("{shift:+.2} % MAC"))
                .unwrap_or_else(|| "unknown".to_owned());
            format!(
                "; the built model's MAC leading edge sits {} from the published planning one \
                 ({} of the published chord) and its chord differs by {} m, so this percentage is \
                 referred to a different chord from the limit it is compared against and the \
                 comparison needs source reconciliation before the exceedance is attributed to \
                 the loading state",
                cg_envelope
                    .model_mac_leading_edge_offset_m
                    .map(|offset| format!("{offset:+.3} m"))
                    .unwrap_or_else(|| "an unknown distance".to_owned()),
                datum_shift,
                cg_envelope
                    .model_mac_length_difference_m
                    .map(|difference| format!("{difference:+.3}"))
                    .unwrap_or_else(|| "an unknown amount".to_owned()),
            )
        } else {
            String::new()
        };
        findings.push(error(
            FindingCode::PublicPlanningCgEnvelopeViolation,
            format!(
                "analyzed CG lies outside the manufacturer public planning envelope; actual \
                 aircraft WBM controls{frame_note}"
            ),
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
    let takeoff_mass_kg = fuel_loading.analyzed_takeoff_mass_kg;
    let gravity_m_s2 = config.requirements.gravity_m_s2;
    // The installed sea-level reference thrust, taken from whichever physical
    // model the aircraft actually has: the certificated jet rating for a
    // turbofan, the propeller model's ground-roll mean thrust for a
    // turboprop. The jet rating is exactly zero for a shaft-power engine, so
    // a propeller aircraft needs the propeller model's value, and it is the
    // roll mean rather than the static value because a propeller's thrust
    // falls through the roll. See `static_thrust`.
    //
    // The propeller branch needs the lift-off speed the roll mean is taken
    // against, so resolve the departure field's own V speeds first; a field
    // that cannot be resolved leaves the speed unusable and the module
    // reports a typed absence rather than guessing one.
    let departure_airport = alas_config::airports::get(&config.departure_airport).ok();
    let departure_density_ratio = departure_airport
        .map(|airport| density_ratio(airport.elevation_m, airport.isa_deviation_c))
        .unwrap_or(f64::NAN);
    let lift_off_true_airspeed_m_s = departure_airport
        .map(|airport| {
            compute_v_speeds_at_masses(
                takeoff_mass_kg,
                takeoff_mass_kg,
                wing_area_m2,
                airport,
                config.performance.cl_max_to,
                config.performance.cl_max_land,
                &config.performance,
            )
            .v_r_ms
        })
        .unwrap_or(f64::NAN);
    let sea_level_static_thrust =
        static_thrust::resolve(config, lift_off_true_airspeed_m_s, departure_density_ratio);
    let static_thrust_n = sea_level_static_thrust.thrust_n;
    let static_tw = static_thrust_n / (takeoff_mass_kg * gravity_m_s2);
    if let Some(note) = sea_level_static_thrust.provenance_note() {
        findings.push(PhysicalFinding {
            code: FindingCode::FieldPerformanceUnavailable,
            severity: FindingSeverity::Warning,
            message: note,
            actual: Some(static_tw),
            limit: None,
            unit: "fraction weight",
        });
    }
    let mlw_limit_kg = landing_mass_limit_kg(config, report);
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
        let oei_assessment = oei_drag::assess(config, report, takeoff_mass_kg, wing_area_m2)
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
        findings.extend(static_thrust::cruise_thrust_margin(
            config,
            report,
            static_tw,
            wing_loading_pa,
            takeoff_mass_kg * gravity_m_s2,
        ));
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
    field::assess_airports(
        config,
        report,
        &mut findings,
        wing_area_m2,
        takeoff_mass_kg,
        landing_mass_kg,
        static_tw,
        &sea_level_static_thrust,
    );

    // The route is judged on the unified segment mission model, on the plan
    // frozen for it: the dispatch already reports an unpriced policy
    // (`FuelPolicyUnavailable`), an unsettled closure (`DispatchNotConverged`)
    // and trip plus reserves above the loadable fuel (`ReserveFuelShortfall`).
    // What is added here is a requested mission with no route result at all,
    // a trip that does not fit in the loadable fuel, and a non-physical trip.
    // The native pseudospectral flight is telemetry
    // (`fuel_loading.mission.native`) and never gates.
    if config.mission.enabled {
        let route = &fuel_loading.mission;
        if fuel_loading.dispatch.is_none() {
            findings.push(error(
                FindingCode::MissionUnavailable,
                "mission analysis was requested but the route was not flown",
                None,
                None,
                "",
            ));
        } else if route.status == MissionFuelStatus::Exhausted {
            let required_kg = fuel_loading
                .dispatch
                .and_then(|dispatch| dispatch.plan)
                .map(|plan| plan.trip.kg);
            findings.push(error(
                FindingCode::MissionFuelShortfall,
                "the route's trip fuel on the segment mission model exceeds the loadable fuel",
                required_kg,
                route.burned_fuel_kg,
                "kg",
            ));
        } else if let Some(trip_kg) = route.required_trip_fuel_kg {
            if !trip_kg.is_finite() || trip_kg <= 0.0 {
                findings.push(error(
                    FindingCode::InvalidMissionFuelBurn,
                    "route trip fuel on the segment mission model is not a positive finite quantity",
                    Some(trip_kg),
                    Some(0.0),
                    "kg",
                ));
            }
        }
    }

    FeasibilityReport {
        findings,
        cg_envelope,
        model_cg,
        fuel_loading,
        cruise_equilibrium,
        mass_balance,
        propulsion_station_fallback: structural_mass::propulsion_fallback_station(config, report),
        operational_envelope,
        native_mission_error: load_case.and_then(|case| case.native_error.clone()),
    }
}

// The registry lookups are test preconditions: a missing shipped preset is the
// failure being reported, rather than a recoverable library condition.
#[allow(clippy::expect_used)]
#[cfg(test)]
#[path = "feasibility_tests.rs"]
mod tests;
