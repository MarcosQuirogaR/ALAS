// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cases B and C: the mission-sized closure on a registered design vector.

use std::time::Instant;

use alas_config::design_variables::DesignVector;
use alas_config::{AlasConfig, DesignMode, MtowSizing};
use alas_mass::dispatch::DispatchStatus;
use alas_opt::assess_product_candidate;
use serde_json::{json, Value};

use super::support::{apply_run_options, dispatch_text, masses_json, oew_kg};

pub(crate) struct MissionCase {
    pub(crate) label: &'static str,
    pub(crate) design_mode: DesignMode,
    pub(crate) mtow_sizing: MtowSizing,
    /// `None` flies the preset's operational route.
    pub(crate) range_nmi: Option<f64>,
}

/// The eight mission cases of the matrix: five fixed-aircraft (B) and
/// three coupled clean-sheet (C) closures.
pub(crate) fn mission_cases() -> [MissionCase; 8] {
    [
        MissionCase {
            label: "B_baseline_sandbox_fixed_requirement_route",
            design_mode: DesignMode::BaselineSandbox,
            mtow_sizing: MtowSizing::FixedRequirement,
            range_nmi: None,
        },
        MissionCase {
            label: "B_baseline_sandbox_sized_by_mission_route",
            design_mode: DesignMode::BaselineSandbox,
            mtow_sizing: MtowSizing::SizedByMission,
            range_nmi: None,
        },
        MissionCase {
            label: "B_baseline_sandbox_unconstrained_route",
            design_mode: DesignMode::BaselineSandbox,
            mtow_sizing: MtowSizing::Unconstrained,
            range_nmi: None,
        },
        MissionCase {
            label: "B_baseline_sandbox_sized_by_mission_design_range",
            design_mode: DesignMode::BaselineSandbox,
            mtow_sizing: MtowSizing::SizedByMission,
            range_nmi: None,
        },
        MissionCase {
            label: "B_baseline_sandbox_unconstrained_design_range",
            design_mode: DesignMode::BaselineSandbox,
            mtow_sizing: MtowSizing::Unconstrained,
            range_nmi: None,
        },
        MissionCase {
            label: "C_clean_sheet_sized_by_mission_route",
            design_mode: DesignMode::CleanSheet,
            mtow_sizing: MtowSizing::SizedByMission,
            range_nmi: None,
        },
        MissionCase {
            label: "C_clean_sheet_unconstrained_route",
            design_mode: DesignMode::CleanSheet,
            mtow_sizing: MtowSizing::Unconstrained,
            range_nmi: None,
        },
        MissionCase {
            label: "C_clean_sheet_unconstrained_design_range",
            design_mode: DesignMode::CleanSheet,
            mtow_sizing: MtowSizing::Unconstrained,
            range_nmi: None,
        },
    ]
}

pub(crate) fn mission_case(
    name: &str,
    design: &DesignVector,
    case: &MissionCase,
    declared_range_nmi: Option<f64>,
) -> Value {
    let mut config = match AlasConfig::from_value(&json!({ "preset": name })) {
        Ok(config) => config,
        Err(error) => return json!({ "status": "config_error", "reason": error.to_string() }),
    };
    apply_run_options(&mut config);
    config.optimizer.design_space.mode = case.design_mode;
    config.optimizer.objective.mtow_sizing = case.mtow_sizing;
    let range_nmi = match case.range_nmi {
        Some(range) => Some(range),
        None if case.label.contains("design_range") => declared_range_nmi,
        None => None,
    };
    if let Some(range) = range_nmi {
        config.optimizer.objective.design_range_nmi = range;
    }
    let started = Instant::now();
    let base = json!({
        "label": case.label,
        "design_mode": case.design_mode.as_str(),
        "mtow_sizing": case.mtow_sizing.as_str(),
        "range_nmi_requested": range_nmi,
        "declared_mtow_kg": config.requirements.mtow_kg,
    });
    let mut out = base.as_object().cloned().unwrap_or_default();
    match assess_product_candidate(&config, design) {
        Err(reason) => {
            out.insert("status".to_owned(), json!("error"));
            out.insert("reason".to_owned(), json!(reason));
        }
        Ok(assessment) => {
            let sized = &assessment.sized;
            let masses = &assessment.resolved.masses;
            let plan = &sized.dispatch.plan;
            let status = match &sized.dispatch.status {
                DispatchStatus::ModelFailed(_) => "model_failed",
                _ if !assessment.hard_feasible => "hard_infeasible",
                _ if !sized.sizing_closed => "not_closed",
                _ => "converged",
            };
            out.insert("status".to_owned(), json!(status));
            out.insert(
                "elapsed_s".to_owned(),
                json!(started.elapsed().as_secs_f64()),
            );
            out.insert(
                "dispatch_status".to_owned(),
                json!(dispatch_text(&sized.dispatch.status)),
            );
            out.insert("takeoff_mass_kg".to_owned(), json!(sized.takeoff_mass_kg));
            out.insert("sizing_basis".to_owned(), json!(sized.sizing_basis));
            out.insert(
                "design_gross_mass_kg".to_owned(),
                json!(sized.design_gross_mass_kg),
            );
            out.insert(
                "design_landing_mass_kg".to_owned(),
                json!(sized.design_landing_mass_kg),
            );
            out.insert(
                "structural_primary_mass_kg".to_owned(),
                json!(sized.structural_primary_mass_kg),
            );
            out.insert(
                "structural_secondary_mass_kg".to_owned(),
                json!(sized.structural_secondary_mass_kg),
            );
            out.insert(
                "structural_inventory_complete".to_owned(),
                json!(sized.structural_inventory_complete),
            );
            out.insert(
                "dispatch_takeoff_mass_kg".to_owned(),
                json!(sized.dispatch.takeoff_mass_kg),
            );
            out.insert("oew_kg".to_owned(), json!(sized.operating_empty_mass_kg));
            out.insert("oew_from_masses_kg".to_owned(), json!(oew_kg(masses)));
            out.insert(
                "zero_fuel_mass_kg".to_owned(),
                json!(sized.zero_fuel_mass_kg),
            );
            out.insert("payload_kg".to_owned(), json!(sized.payload_kg));
            out.insert(
                "carried_passengers".to_owned(),
                json!(sized.carried_passengers),
            );
            out.insert(
                "passenger_capacity".to_owned(),
                json!(sized.passenger_capacity),
            );
            out.insert("block_fuel_kg".to_owned(), json!(sized.block_fuel_kg));
            out.insert("takeoff_fuel_kg".to_owned(), json!(sized.takeoff_fuel_kg));
            out.insert("ramp_fuel_kg".to_owned(), json!(sized.ramp_fuel_kg));
            out.insert("trip_fuel_kg".to_owned(), json!(plan.trip.kg));
            out.insert("contingency_fuel_kg".to_owned(), json!(plan.contingency.kg));
            out.insert("alternate_fuel_kg".to_owned(), json!(plan.alternate.kg));
            out.insert(
                "final_reserve_fuel_kg".to_owned(),
                json!(plan.final_reserve.kg),
            );
            out.insert("taxi_fuel_kg".to_owned(), json!(plan.taxi.kg));
            out.insert(
                "destination_landing_mass_kg".to_owned(),
                json!(sized.dispatch.destination_landing_mass_kg),
            );
            // The reported limit is the sizing basis's own
            // `design_landing_mass_kg` (`SizedCandidate`'s single reported
            // value): the declared `WLDG` in every mode for a fixed
            // aircraft, unaffected by `MtowSizing`, and the configured
            // fraction of the closed takeoff mass for a coupled clean-sheet
            // design.
            out.insert(
                "landing_mass_limit_kg".to_owned(),
                json!(sized.design_landing_mass_kg),
            );
            out.insert(
                "usable_capacity_kg".to_owned(),
                json!(if sized.usable_capacity_kg.is_finite() {
                    Some(sized.usable_capacity_kg)
                } else {
                    None
                }),
            );
            out.insert("design_range_m".to_owned(), json!(sized.design_range_m));
            out.insert("lift_to_drag".to_owned(), json!(sized.lift_to_drag));
            out.insert(
                "sizing_iterations".to_owned(),
                json!(sized.sizing_iterations),
            );
            out.insert("sizing_closed".to_owned(), json!(sized.sizing_closed));
            out.insert("hard_feasible".to_owned(), json!(assessment.hard_feasible));
            out.insert(
                "violated_hard_ids".to_owned(),
                json!(assessment.violated_hard_ids()),
            );
            out.insert("masses_kg".to_owned(), masses_json(masses));
            out.insert(
                "residuals".to_owned(),
                json!(assessment
                    .residuals
                    .iter()
                    .map(|r| json!({
                        "id": r.id,
                        "actual": r.actual,
                        "limit": r.limit,
                        "unit": r.unit,
                        "raw_residual": r.raw_residual,
                        "normalized_violation": r.normalized_violation,
                        "policy": r.policy.as_str(),
                    }))
                    .collect::<Vec<_>>()),
            );
        }
    }
    Value::Object(out)
}

/// One bounded variation of the fixed-aircraft mission closure, used to
/// attribute a typed mission failure to mass, propulsion, scheduling or
/// input inconsistency without weakening any check.
pub(crate) struct ClimbVariant {
    pub(crate) label: &'static str,
    pub(crate) note: &'static str,
    pub(crate) apply: fn(&mut AlasConfig, &alas_config::AircraftPreset),
}

pub(crate) fn climb_variants() -> Vec<ClimbVariant> {
    vec![
        ClimbVariant {
            label: "as_loaded",
            note: "AlasConfig::from_value(preset): generic default mission profile (step climbs at 250 m/s true airspeed)",
            apply: |_, _| {},
        },
        ClimbVariant {
            label: "preset_operational_profile",
            note: "the preset's own operational speed schedule and route, as interactive clients load it (A320: 170 KCAS climbs, calibrated-airspeed reference)",
            apply: |config, preset| {
                let operational = preset.operational_mission_defaults();
                config.departure_airport = operational.departure_airport.to_owned();
                config.arrival_airport = operational.arrival_airport.to_owned();
                config.mission.profile = operational.profile;
            },
        },
        ClimbVariant {
            label: "narrowbody_cas_schedule",
            note: "bounded scheduling sensitivity: a declared narrowbody climb schedule (250 KCAS initial climb, 290 KCAS step climbs, calibrated-airspeed reference, 1500/1000/800 ft/min) and cruise at the route Mach; the generic profile's 250 m/s true-airspeed climbs are M0.78-0.85 at 5-9 km",
            apply: |config, _| {
                const KNOT: f64 = 1852.0 / 3600.0;
                const FT_MIN: f64 = 0.3048 / 60.0;
                let profile = &mut config.mission.profile;
                profile.climb_descent_speed_reference = alas_config::SpeedReference::CalibratedAirspeed;
                profile.initial_climb_air_speed_m_s = 250.0 * KNOT;
                profile.initial_climb_rate_m_s = 1_500.0 * FT_MIN;
                profile.step_climb_1_air_speed_m_s = 290.0 * KNOT;
                profile.step_climb_1_rate_m_s = 1_000.0 * FT_MIN;
                profile.step_climb_2_air_speed_m_s = 290.0 * KNOT;
                profile.step_climb_2_rate_m_s = 800.0 * FT_MIN;
                let cruise_tas = config.requirements.cruise_mach
                    * alas_atmo::Atmosphere::isa(config.requirements.cruise_altitude_m).speed_of_sound();
                profile.cruise_1_air_speed_m_s = cruise_tas;
                profile.cruise_2_air_speed_m_s = cruise_tas;
                profile.cruise_3_air_speed_m_s = cruise_tas;
                profile.descent_1_air_speed_m_s = profile.descent_1_air_speed_m_s.min(290.0 * KNOT);
            },
        },
        ClimbVariant {
            label: "narrowbody_cas_schedule_design_range",
            note: "the same declared narrowbody schedule on the preset's FLOPS design range (3,400 nmi for the A320), so the climb/descent footprint fits the mission",
            apply: |config, _| {
                for variant in climb_variants() {
                    if variant.label == "narrowbody_cas_schedule" {
                        (variant.apply)(config, &alas_config::presets::registry()[0]);
                    }
                }
                config.optimizer.objective.design_range_nmi = config
                    .mass_model
                    .flops_transport
                    .design_range_nmi
                    .unwrap_or(3_400.0);
            },
        },
        ClimbVariant {
            label: "as_loaded_thrust_x1.20",
            note: "bounded propulsion sensitivity only: rated thrust scaled by 1.20 on the generic profile",
            apply: |config, _| {
                let scaled = config.geometry.engine.thrust_kn() * 1.20;
                if config.geometry.engine.set_thrust_kn(scaled).is_err() {
                    println!("thrust scaling is not available for this engine binding");
                }
            },
        },
        ClimbVariant {
            label: "as_loaded_no_payload",
            note: "bounded mass sensitivity only: zero passenger payload on the generic profile",
            apply: |config, _| {
                config.requirements.num_passengers = 0;
                config.cabin.passenger.first.count = 0;
                config.cabin.passenger.business.count = 0;
                config.cabin.passenger.premium.count = 0;
                config.cabin.passenger.economy.count = 0;
                config.cabin.passenger.class_mix_mode = "count".to_owned();
            },
        },
    ]
}

/// Run the fixed-aircraft mission closure on the operational route under one
/// variation and report the typed outcome.
pub(crate) fn climb_variant_case(
    name: &str,
    design: &DesignVector,
    variant: &ClimbVariant,
) -> Value {
    let Ok(preset) = alas_config::presets::get(name) else {
        return json!({ "label": variant.label, "status": "preset_error" });
    };
    let mut config = match AlasConfig::from_value(&json!({ "preset": name })) {
        Ok(config) => config,
        Err(error) => {
            return json!({ "label": variant.label, "status": "config_error", "reason": error.to_string() })
        }
    };
    apply_run_options(&mut config);
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    config.optimizer.objective.mtow_sizing = MtowSizing::SizedByMission;
    (variant.apply)(&mut config, preset);
    let started = Instant::now();
    match assess_product_candidate(&config, design) {
        Err(reason) => {
            json!({ "label": variant.label, "note": variant.note, "status": "error", "reason": reason })
        }
        Ok(assessment) => {
            let sized = &assessment.sized;
            let status = match &sized.dispatch.status {
                DispatchStatus::ModelFailed(_) => "model_failed",
                _ if !assessment.hard_feasible => "hard_infeasible",
                _ if !sized.sizing_closed => "not_closed",
                _ => "converged",
            };
            json!({
                "label": variant.label,
                "note": variant.note,
                "status": status,
                "elapsed_s": started.elapsed().as_secs_f64(),
                "dispatch_status": dispatch_text(&sized.dispatch.status),
                "takeoff_mass_kg": sized.takeoff_mass_kg,
                "zero_fuel_mass_kg": sized.zero_fuel_mass_kg,
                "oew_kg": sized.operating_empty_mass_kg,
                "block_fuel_kg": sized.block_fuel_kg,
                "design_range_m": sized.design_range_m,
                "lift_to_drag": sized.lift_to_drag,
                "violated_hard_ids": assessment.violated_hard_ids(),
                "profile": {
                    "speed_reference": format!("{:?}", config.mission.profile.climb_descent_speed_reference),
                    "step_climb_1_air_speed_m_s": config.mission.profile.step_climb_1_air_speed_m_s,
                    "step_climb_2_air_speed_m_s": config.mission.profile.step_climb_2_air_speed_m_s,
                    "step_climb_1_rate_m_s": config.mission.profile.step_climb_1_rate_m_s,
                    "cruise_1_air_speed_m_s": config.mission.profile.cruise_1_air_speed_m_s,
                },
                "thrust_kn_per_engine": config.geometry.engine.thrust_kn(),
            })
        }
    }
}
