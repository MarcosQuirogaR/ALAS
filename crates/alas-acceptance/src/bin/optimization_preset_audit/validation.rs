// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The `validation` block of one reported design: every quantity the phase-7
//! comparison (`compare.py`) scores or checks, taken from the functions the
//! report already uses.
//!
//! Nothing here is a new model. Each section calls the routine behind the
//! report figure it mirrors and records the result with its units and
//! conditions in the key names:
//!
//! - `takeoff_landing_speeds_isa_sea_level`, `oei_second_segment` and
//!   `field_lengths_isa_sea_level` read one
//!   [`alas_pipeline::field_reference::IsaSeaLevelFieldReference`], built on
//!   `report_config::config_for_report` (computed `VMC`, scaled `CLmax`,
//!   improved-climb `V2`);
//! - `payload_range` reads the report chart's own data
//!   (`alas_report::families::performance::payload_range_data`) and prices
//!   corners B and C at published chart masses through the path of the kappa
//!   calibration probe (`corners::chart_mass_corners`);
//! - `reference_missions` flies the stated manufacturer missions with the
//!   report's route pricing (`reference_mission::reference_route_fuel`);
//! - `balance` reads the feasibility CG assessments and the report static
//!   margin; `cruise_and_ceiling` the top-of-climb finding, the mission
//!   profile's `MMO` and the Quick Analysis thrust-limited cruise solve.
//!
//! A section the model cannot produce is `{"unavailable": reason}`, never a
//! substituted number. SI throughout unless a key says otherwise: masses kg,
//! speeds m/s, lengths m, ranges km and nmi (both given), gradients are rise
//! over distance (0.024 is 2.4 %), margins and CG positions % MAC.

use alas_config::AlasConfig;
use alas_opt::mdo::{top_of_climb, ThrustAssessment, ThrustVerdict, TOC_TOLERANCE_M_S};
use alas_pipeline::field_reference::IsaSeaLevelFieldReference;
use alas_pipeline::fuel_model::report_mission_model;
use alas_pipeline::quick_analysis::corners::{chart_mass_corners, ChartMasses, ChartReserveRule};
use alas_pipeline::quick_analysis::{CruiseSolve, SERVICE_CEILING_CLIMB_RATE_M_S};
use alas_pipeline::reference_mission::reference_route_fuel;
use alas_pipeline::{AnalysisReport, FeasibilityReport, PlanningCgStatus};
use alas_report::families::performance::payload_range_data;
use serde_json::{json, Value};

/// Metres per international nautical mile.
const M_PER_NMI: f64 = 1852.0;

/// Stated reference missions of the holdout (`holdout-proposed.json` Q5,
/// ATR72-600 brochure block fuel at 200 / 300 / 400 NM; 500 and 1000 NM have
/// no reference yet but are stated missions of the same quantity). Key is the
/// holdout metric id, value the still-air route distance in nautical miles.
const REFERENCE_MISSIONS: &[(&str, &[(&str, f64)])] = &[(
    "ATR72-600",
    &[
        ("Q5.block200", 200.0),
        ("Q5.block300", 300.0),
        ("Q5.block400", 400.0),
        ("Q5.block500", 500.0),
        ("Q5.block1000", 1000.0),
    ],
)];

/// Chart masses of the published payload-range charts, `data/chart_corner_masses.json`.
const CHART_MASSES_JSON: &str = include_str!("../../../data/chart_corner_masses.json");

fn unavailable(reason: impl Into<String>) -> Value {
    json!({ "unavailable": reason.into() })
}

/// The masses and the ISA sea-level field reference the block is built on.
pub(super) struct FieldBasis<'a> {
    /// Take-off mass of the field reference, kg.
    pub takeoff_mass_kg: f64,
    /// The reference, or why it could not be built.
    pub reference: &'a Result<IsaSeaLevelFieldReference, String>,
}

/// The `validation` block of the design `report` describes under `config`.
pub(super) fn validation_json(
    config: &AlasConfig,
    report: &AnalysisReport,
    feasibility: &FeasibilityReport,
    field: &FieldBasis<'_>,
) -> Value {
    let (speeds, oei, lengths) = match field.reference {
        Ok(reference) => (
            speeds_json(field, reference),
            oei_json(reference),
            lengths_json(field, reference),
        ),
        Err(reason) => (
            unavailable(reason.clone()),
            unavailable(reason.clone()),
            unavailable(reason.clone()),
        ),
    };
    json!({
        "note": "Phase-7 validation quantities of this design, computed by the report's own routines (see the module documentation of optimization_preset_audit/validation.rs). SI units; gradients are rise over distance; CG and margins in % MAC.",
        "takeoff_landing_speeds_isa_sea_level": speeds,
        "oei_second_segment": oei,
        "field_lengths_isa_sea_level": lengths,
        "payload_range": payload_range_json(config, report),
        "reference_missions": reference_missions_json(config, report),
        "balance": balance_json(report, feasibility),
        "cruise_and_ceiling": cruise_json(config, report),
        "thrust_uncertainty": thrust_uncertainty_json(config, report, feasibility),
    })
}

/// The cruise-thrust and top-of-climb checks with the scatter of the engine's
/// altitude thrust (owner decision 2026-10-13): required, available, scatter
/// and verdict (`pass`, `warning` inside the band, `hard` beyond it), for
/// every jet, passing or not. A warning is never dropped.
fn thrust_uncertainty_json(
    config: &AlasConfig,
    report: &AnalysisReport,
    feasibility: &FeasibilityReport,
) -> Value {
    if matches!(
        config.geometry.engine.active_model(),
        Ok(alas_config::ActiveEngineModel::Turboprop(_))
    ) {
        return unavailable("turboprop: the jet chart scatter does not apply");
    }
    let checks = alas_pipeline::thrust_checks(config, report, feasibility);
    let one = |check: &Result<ThrustAssessment, String>, unit: &str| match check {
        Ok(a) => json!({
            "required": a.required,
            "available": a.available,
            "unit": unit,
            "shortfall_fraction": a.shortfall(),
            "scatter_fraction": a.scatter,
            "band_available_min": a.required * (1.0 - a.scatter),
            "verdict": match a.verdict() {
                ThrustVerdict::Pass => "pass",
                ThrustVerdict::Warning => "warning",
                ThrustVerdict::Hard => "hard",
            },
        }),
        Err(reason) => unavailable(reason.clone()),
    };
    json!({
        "basis": if checks.scatter > 0.0 {
            "altitude thrust from the Jenkinson chart correlation alone: hard only beyond the scatter, a smaller shortfall is a flagged warning"
        } else {
            "engine has a cruise or climb thrust datum: any shortfall is hard"
        },
        "scatter_fraction": checks.scatter,
        // The key keeps its historical name; since 2026-10-14 the check is the
        // worst of the cruise levels the mission plan flies (`plan` below), and
        // the design level at the mid-cruise mass only when no plan is carried.
        "level_cruise_mid_mass_static_tw": one(&checks.level, "T/W"),
        "level_cruise_basis": if checks.plan.is_some() {
            "worst of the cruise levels the mission plan flies, each at its mass and Mach"
        } else {
            "design level at the mid-cruise mass (no mission plan carried)"
        },
        "planned_cruise_levels": checks.plan.as_ref().map(|plan| json!({
            "levels": plan.levels.iter().map(|level| json!({
                "altitude_m": level.altitude_m,
                "flight_level": level.altitude_m / 0.3048 / 100.0,
                "mach": level.mach,
                "mass_kg": level.mass_kg,
                "required_static_tw": level.required_tw,
            })).collect::<Vec<_>>(),
            "governing_altitude_m": plan.governing.altitude_m,
            "highest_level_at_initial_mass_m": plan.highest_level_m,
        })),
        "top_of_climb_300ft_min_thrust": one(&checks.top_of_climb, "N"),
        "findings": feasibility.findings.iter()
            .filter(|f| f.code == alas_pipeline::FindingCode::ThrustWithinChartScatter)
            .map(|f| json!({"severity": format!("{:?}", f.severity), "message": f.message, "actual": f.actual, "limit": f.limit, "unit": f.unit}))
            .collect::<Vec<_>>(),
    })
}

fn speeds_json(field: &FieldBasis<'_>, reference: &IsaSeaLevelFieldReference) -> Value {
    json!({
        "condition": "ISA sea level, still air; V2 and VSR (take-off configuration) at the take-off mass, VREF = 1.23 VS1g (14 CFR 25.125) at the landing mass; speed model of the report (computed VMC, geometry-scaled CLmax, improved-climb V2)",
        "takeoff_mass_kg": field.takeoff_mass_kg,
        "landing_mass_kg": reference.landing_mass_kg,
        "v2_over_vsr": reference.oei.v2_over_vsr,
        "v2_m_s": reference.v2_m_s,
        "vsr_takeoff_m_s": reference.v_stall_takeoff_m_s,
        "improved_climb_v2": reference.oei.improved_climb,
        "vref_m_s": reference.vref_m_s,
        "vs1g_landing_m_s": reference.vs1g_landing_m_s,
    })
}

fn oei_json(reference: &IsaSeaLevelFieldReference) -> Value {
    let oei = &reference.oei;
    let margin = oei
        .required_gradient
        .zip(oei.available_gradient)
        .map(|(required, available)| available - required);
    json!({
        "condition": "one engine inoperative, take-off configuration, gear up, at V2, ISA sea level, take-off mass; CS 25.121(b) / 14 CFR 25.121(b) gross gradient",
        "unit_gradient": "rise over distance, dimensionless (0.024 = 2.4 %)",
        "required_gradient_cs25_121b": oei.required_gradient,
        "available_gradient": oei.available_gradient,
        "margin_gradient": margin,
        "meets_requirement": margin.map(|value| value >= 0.0),
        "status": format!("{:?}", oei.status),
        "v2_over_vsr": oei.v2_over_vsr,
        "improved_climb": oei.improved_climb,
        "required_installed_sls_tw": oei.required_sls_tw,
        "installed_static_tw": reference.static_tw,
        "installed_thrust_basis": reference.static_thrust_basis,
        "basis": oei.basis,
    })
}

fn lengths_json(field: &FieldBasis<'_>, reference: &IsaSeaLevelFieldReference) -> Value {
    json!({
        "condition_takeoff": "take-off field length at the take-off mass, ISA sea level, dry runway, zero wind, level",
        "condition_landing": "factored dry landing field length at the landing mass, ISA sea level, zero wind, level",
        "takeoff_mass_kg": field.takeoff_mass_kg,
        "landing_mass_kg": reference.landing_mass_kg,
        "tofl_m": reference.takeoff_field_length_m,
        "tofl_basis": reference.takeoff_field_length_basis,
        "balanced_field_length_m": reference.model_balanced_field_length_m,
        "lfl_m": reference.landing_field_length_m,
        "lfl_basis": reference.landing_field_length_basis,
        "landing_distance_unfactored_m": reference.landing_distance_m,
    })
}

/// Chart masses registered for `preset`.
fn chart_masses(preset: &str) -> Result<ChartMasses, String> {
    let document: Value = serde_json::from_str(CHART_MASSES_JSON)
        .map_err(|error| format!("chart_corner_masses.json: {error}"))?;
    let entry = document["presets"]
        .as_array()
        .and_then(|list| list.iter().find(|entry| entry["preset"] == preset))
        .ok_or_else(|| format!("no published chart masses registered for {preset}"))?;
    let number = |key: &str| {
        entry[key]
            .as_f64()
            .ok_or_else(|| format!("chart_corner_masses.json: {preset} lacks {key}"))
    };
    Ok(ChartMasses {
        tow_kg: number("tow_kg")?,
        tank_kg: number("fuel_tank_kg")?,
        delta_payload_kg: number("delta_payload_kg")?,
        reserve_rule: ChartReserveRule::from_json(entry.get("reserve_rule"))?,
        cruise_mach: entry["cruise_mach"].as_f64(),
    })
}

fn payload_range_json(config: &AlasConfig, report: &AnalysisReport) -> Value {
    let model_masses = match payload_range_data(report, config) {
        Some(data) => {
            let corner = |index: usize| {
                let point = &data.points[index];
                json!({
                    "range_nmi": point.range_nm,
                    "range_km": point.range_nm * M_PER_NMI / 1000.0,
                    "payload_kg": point.payload_kg,
                    "reserve_fuel_kg": data.reserve_fuel_kg[index],
                })
            };
            json!({
                "condition": "report payload-range chart: still air, reserve-inclusive fuel plan of the configured policy, takeoff mass = MTOW limit of the design, modelled OEW; A = max payload at zero range, B = max payload at MTOW, C = max fuel at MTOW, D = ferry (zero payload)",
                "mtow_kg": data.mtow_kg,
                "mass_is_sized": data.mass_is_sized,
                "oew_kg": data.oew_kg,
                "payload_basis": data.payload_basis,
                "fuel_capacity_kg": data.fuel_capacity_kg,
                "fuel_capacity_limit": data.fuel_capacity_limit,
                "range_basis": data.range_basis.label(),
                "method_note": data.method_note,
                "corners": {"A": corner(0), "B": corner(1), "C": corner(2), "D": corner(3)},
            })
        }
        None => unavailable("no main wing or no typed usable-fuel capacity evidence"),
    };
    let chart = chart_masses(&config.preset).and_then(|masses| {
        let corners = chart_mass_corners(config, report, &masses)?;
        Ok(json!({
            "condition": "corners B and C flown at the published chart masses (takeoff-mass line, usable fuel less taxi, chart payload difference): the range does not depend on the modelled OEW; still air, reserve rule printed on the chart where it prints one (reserve_scheme), else the configured policy",
            "tow_kg": masses.tow_kg,
            "tank_kg": masses.tank_kg,
            "delta_payload_b_minus_c_kg": masses.delta_payload_kg,
            "reserve_scheme": corners.scheme.as_str(),
            "reserve_alternate_nmi": corners.alternate_distance_nmi,
            "reserve_final_hold_min": corners.final_reserve_hold_min,
            "taxi_fuel_kg": corners.taxi_kg,
            "fuel_b_kg": corners.fuel_b_kg,
            "fuel_c_kg": corners.fuel_c_kg,
            "range_b_km": corners.range_b_m / 1000.0,
            "range_c_km": corners.range_c_m / 1000.0,
            "range_b_nmi": corners.range_b_m / M_PER_NMI,
            "range_c_nmi": corners.range_c_m / M_PER_NMI,
            "reserve_b_kg": corners.reserve_b_kg,
            "reserve_c_kg": corners.reserve_c_kg,
        }))
    });
    json!({
        "model_masses": model_masses,
        "chart_mass_corners": chart.unwrap_or_else(unavailable),
    })
}

fn reference_missions_json(config: &AlasConfig, report: &AnalysisReport) -> Value {
    let Some((_, missions)) = REFERENCE_MISSIONS
        .iter()
        .find(|(preset, _)| *preset == config.preset)
    else {
        return json!({"note": "no stated reference mission for this preset in the holdout"});
    };
    let mut out = serde_json::Map::new();
    for (id, range_nmi) in *missions {
        let value = match reference_route_fuel(config, report, range_nmi * M_PER_NMI) {
            Ok(fuel) => json!({
                "condition": "still-air route distance, configured fuel policy, report route pricing (segment mission model, planned trip, nominal payload of the report); block fuel = taxi out + trip + taxi in",
                "range_nmi": range_nmi,
                "block_fuel_kg": fuel.block_fuel_kg,
                "trip_fuel_kg": fuel.trip_fuel_kg,
                "taxi_out_fuel_kg": fuel.taxi_out_fuel_kg,
                "reserve_fuel_kg": fuel.reserve_fuel_kg,
                "takeoff_fuel_kg": fuel.takeoff_fuel_kg,
                "takeoff_mass_kg": fuel.takeoff_mass_kg,
                "zero_fuel_mass_kg": fuel.zero_fuel_mass_kg,
                "trip_time_s": fuel.trip_time_s,
                "dispatch_status": fuel.status,
                "converged": fuel.converged,
                "shortfall_kg": fuel.shortfall_kg,
            }),
            Err(reason) => unavailable(reason),
        };
        out.insert((*id).to_owned(), value);
    }
    Value::Object(out)
}

fn balance_json(report: &AnalysisReport, feasibility: &FeasibilityReport) -> Value {
    let cg = &feasibility.cg_envelope;
    let loading = feasibility.operational_envelope.as_ref().map(|envelope| {
        let potato = &envelope.potato_pct_mac;
        let fold = |select: fn(&(f64, f64, f64)) -> f64, init: f64, pick: fn(f64, f64) -> f64| {
            potato
                .iter()
                .map(select)
                .filter(|value| value.is_finite())
                .fold(init, pick)
        };
        let (min_pct, max_pct) = (
            fold(|p| p.1, f64::INFINITY, f64::min),
            fold(|p| p.2, f64::NEG_INFINITY, f64::max),
        );
        let (mass_lo, mass_hi) = (
            fold(|p| p.0, f64::INFINITY, f64::min),
            fold(|p| p.0, f64::NEG_INFINITY, f64::max),
        );
        let have = min_pct.is_finite() && max_pct.is_finite();
        json!({
            "condition": "min / max CG over every sampled boarding, cargo-loading and fuel-burn sequence between operating empty and zero-fuel mass (the model's loading envelope), % MAC of the planning frame",
            "forward_pct_mac": have.then_some(min_pct),
            "aft_pct_mac": have.then_some(max_pct),
            "range_pct_mac": have.then_some(max_pct - min_pct),
            "mass_range_kg": have.then_some([mass_lo, mass_hi]),
            "zfw_operational_limits_pct_mac": envelope.zfw_operational_limits_pct_mac.map(|(lo, hi)| [lo, hi]),
        })
    });
    let np_cruise = report
        .neutral_point_conditions
        .as_ref()
        .map(|conditions| conditions.cruise);
    let c_ref = report.airplane.c_ref;
    let x_cg = report.x_neutral_point - report.static_margin * c_ref;
    json!({
        "analysed_load_case": {
            "condition": "CG of the analysed load case against the published planning curve (preliminary design evidence, not the aircraft WBM)",
            "mass_kg": cg.mass_kg,
            "cg_pct_mac": cg.cg_pct_mac,
            "planning_forward_limit_pct_mac": cg.forward_limit_pct_mac,
            "planning_aft_limit_pct_mac": cg.aft_limit_pct_mac,
            "planning_status": format!("{:?}", cg.planning_status),
            "inside_planning_envelope": match cg.planning_status {
                PlanningCgStatus::NotEvaluated | PlanningCgStatus::AftLimitNotPublished => Value::Null,
                PlanningCgStatus::WithinPublishedLimits => json!(true),
                PlanningCgStatus::ForwardLimitViolation | PlanningCgStatus::AftLimitViolation => json!(false),
            },
            "published_mac_chord_m": cg.published_mac_chord_m,
            "model_mac_length_difference_m": cg.model_mac_length_difference_m,
        },
        "cg_range_over_loading": loading.unwrap_or_else(|| unavailable("no operational (loading) envelope evaluated for this design")),
        "static_margin": {
            "condition": "aerodynamic static margin (x_np - x_cg) / c_ref of the report's neutral-point model at the analysed CG, positive stable",
            "static_margin_pct_mac": report.static_margin * 100.0,
            "stable": report.static_margin > 0.0,
            "neutral_point_x_m": report.x_neutral_point,
            "cg_x_m_implied": x_cg,
            "mac_ref_m": c_ref,
            "static_margin_cruise_neutral_point_pct_mac": np_cruise.map(|np| (np - x_cg) / c_ref * 100.0),
            "neutral_point_cruise_x_m": np_cruise,
        },
    })
}

fn cruise_json(config: &AlasConfig, report: &AnalysisReport) -> Value {
    let requirements = &config.requirements;
    let model = report_mission_model(config, report);
    let mmo = model
        .as_ref()
        .map(|model| model.profile.max_operating_mach)
        .map_err(String::clone);
    let max_operating_mach = match &mmo {
        Ok(value) if value.is_finite() && *value > 0.0 => json!({
            "max_operating_mach": value,
            "cruise_mach_margin_to_mmo": value - requirements.cruise_mach,
            "cruise_mach_within_mmo": requirements.cruise_mach <= *value * (1.0 + 1.0e-9),
        }),
        Ok(_) => unavailable("the mission profile declares no MMO"),
        Err(reason) => unavailable(reason.clone()),
    };
    let top_of_climb = report
        .fuel
        .artifacts(config, &report.design)
        .and_then(|artifacts| top_of_climb(config, &artifacts))
        .map(|toc| {
            json!({
                "condition": "design-mission plan: residual climb at maximum-climb thrust at the initial cruise level less 30 m, top-of-climb mass (the report's 300 ft/min finding)",
                "initial_cruise_level_m": toc.altitude_m,
                "initial_cruise_level_ft": toc.altitude_m / 0.3048,
                "residual_climb_rate_m_s": toc.rate_m_s,
                "residual_climb_criterion_m_s": toc.rate_m_s - toc.margin_m_s(),
                "residual_climb_margin_m_s": toc.margin_m_s(),
                "residual_climb_criterion_met": toc.margin_m_s() >= -TOC_TOLERANCE_M_S,
            })
        })
        .unwrap_or_else(unavailable);
    let mass_kg = report.analysis_takeoff_mass_kg(requirements.mtow_kg);
    let solve = CruiseSolve::new(config, report, mass_kg);
    let (achievable, ceiling) = match &solve {
        Ok(solve) => (
            solve
                .achievable_mach_at_requested_altitude()
                .map_or_else(unavailable, |mach| json!(mach)),
            solve
                .service_ceiling_m()
                .map_or_else(unavailable, |height| json!(height)),
        ),
        Err(reason) => (unavailable(reason.clone()), unavailable(reason.clone())),
    };
    json!({
        "requirement_cruise_mach": requirements.cruise_mach,
        "requirement_cruise_altitude_m": requirements.cruise_altitude_m,
        "mmo_check": max_operating_mach,
        "top_of_climb_initial_cruise_level": top_of_climb,
        "thrust_limited": {
            "condition": "standard day, report drag table and propulsion deck at maximum-climb rating, at the analysed take-off mass (conservative for the end of cruise)",
            "mass_kg": mass_kg,
            "mach_at_requirement_altitude": achievable,
            "service_ceiling_m": ceiling,
            "service_ceiling_criterion_climb_rate_m_s": SERVICE_CEILING_CLIMB_RATE_M_S,
        },
    })
}

#[cfg(test)]
#[path = "validation_tests.rs"]
mod tests;
