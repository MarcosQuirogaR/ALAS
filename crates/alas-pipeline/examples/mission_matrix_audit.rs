// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Print the end-to-end mission and mass contract for every registered preset.
//!
//! This is intentionally a diagnostic example rather than a pass/fail test:
//! interactive routes are not source-backed design missions, and the output is
//! useful when comparing the native model with a manufacturer planning chart.

#![allow(clippy::print_stdout)]

use alas_aero::analysis::{AeroAnalysis, TrimPoint};
use alas_config::{airports::get as get_airport, presets, AlasConfig};
use alas_exec::RunEnvironment;
use alas_payload::layout::LayoutSummary;
use alas_pipeline::{DesignPipeline, FullAnalysis, PipelineOptions};
use alas_route::route::{Route, RouteSource};
use alas_stab::trim::stability_and_trim;

fn main() {
    let requested = std::env::args().nth(1);
    let names: Vec<String> = requested
        .as_deref()
        .map(|name| vec![name.to_owned()])
        .unwrap_or_else(|| {
            presets::available()
                .into_iter()
                .map(str::to_owned)
                .collect()
        });

    println!(
        "preset,route_nm,mtow_kg,oew_kg,payload_kg,zfw_kg,mzfw_kg,carried_fuel_kg,tow_kg,trip_fuel_kg,mission_status,exhaustion_segment,exhaustion_burned_kg,exhaustion_available_kg,mission_segments/scheduled,solution_statuses,throttle_limited,cruise_points,max_abs_force_residual_m_s2,cl_target,raw_trim_converged,raw_trim_alpha_deg,raw_trim_ih_deg,raw_cl_alpha_per_deg,raw_cm_alpha_per_deg,raw_cl_ih_per_deg,raw_cm_ih_per_deg,trim_perf_cm,trim_perf_cl,trim_perf_error,trim_cm,static_margin,physical_cg_x_m,cg_states,layout"
    );
    for name in names {
        match evaluate(&name) {
            Ok(row) => println!("{row}"),
            Err(error) => println!("{name},ERROR,{error}"),
        }
    }
}

fn evaluate(name: &str) -> Result<String, String> {
    let preset = presets::get(name).map_err(|error| error.to_string())?;
    let config = AlasConfig::from_value(&serde_json::json!({"preset": name}))
        .map_err(|error| format!("config: {error}"))?;
    let origin = get_airport(&config.departure_airport).map_err(|error| error.to_string())?;
    let destination = get_airport(&config.arrival_airport).map_err(|error| error.to_string())?;
    let mut route = Route::great_circle(
        origin,
        destination,
        config.mission.great_circle_points.max(1) as usize,
    );
    route.source = RouteSource::SimbriefApi;
    let route_nm = route.total_distance_m() / 1_852.0;

    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: false,
        aerodynamic_solver: Default::default(),
        optimization_solver: Default::default(),
        output_dir: None,
        save_plots: false,
        seed: None,
        quiet: true,
    };
    let result = DesignPipeline::new(config.clone())
        .run_with_environment_and_route(&options, &RunEnvironment::default(), Some(route))
        .map_err(|error| format!("pipeline: {error}"))?;
    let report = result
        .optimized_report
        .as_ref()
        .ok_or_else(|| "optimized report missing".to_owned())?;
    let fuel = &result.feasibility.fuel_loading;
    let mission = result
        .mission_result
        .as_ref()
        .ok_or_else(|| "mission result missing".to_owned())?;
    let summary = mission.completed_summary();
    let cruise = result.feasibility.cruise_equilibrium.as_ref();
    let trim_cm = report
        .trimmed_design_point
        .as_ref()
        .map(|trim| trim.cm_residual)
        .unwrap_or(f64::NAN);
    let cl_target = FullAnalysis::new(config.clone()).cruise_cl(&report.airplane);
    let mut fine_analysis = config.analysis.clone();
    fine_analysis.spanwise_resolution = fine_analysis.fine_spanwise_resolution;
    fine_analysis.chordwise_resolution = fine_analysis.fine_chordwise_resolution;
    let raw_trim = stability_and_trim(
        &report.airplane,
        &fine_analysis,
        cl_target,
        config.requirements.cruise_mach,
        config.requirements.cruise_altitude_m,
    )
    .ok();
    let raw_trim_converged =
        raw_trim.map_or(
            "error",
            |trim| {
                if trim.converged {
                    "true"
                } else {
                    "false"
                }
            },
        );
    let raw_trim_alpha = raw_trim.map_or(f64::NAN, |trim| trim.trim_alpha_deg);
    let raw_trim_ih = raw_trim.map_or(f64::NAN, |trim| trim.trim_ih_deg);
    let raw_cl_alpha = raw_trim.map_or(f64::NAN, |trim| trim.cl_alpha);
    let raw_cm_alpha = raw_trim.map_or(f64::NAN, |trim| trim.cm_alpha);
    let raw_cl_ih = raw_trim.map_or(f64::NAN, |trim| trim.cl_ih);
    let raw_cm_ih = raw_trim.map_or(f64::NAN, |trim| trim.cm_ih);
    let trim_perf_result = raw_trim.map(|trim| {
        let aero = AeroAnalysis::new(
            &report.airplane,
            report.design.sweep_deg,
            Some(config.geometry.clone()),
            Some(config.drag_model.clone()),
            Some(fine_analysis),
        );
        aero.trimmed_performance(
            &TrimPoint {
                trim_alpha_deg: trim.trim_alpha_deg,
                trim_ih_deg: trim.trim_ih_deg,
                cl_alpha: trim.cl_alpha,
            },
            config.requirements.cruise_mach,
            config.requirements.cruise_altitude_m,
        )
    });
    let trim_perf_cm = trim_perf_result
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .map_or(f64::NAN, |performance| performance.cm_residual);
    let trim_perf_cl = trim_perf_result
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .map_or(f64::NAN, |performance| performance.cl);
    let trim_perf_error = trim_perf_result
        .as_ref()
        .and_then(|result| result.as_ref().err())
        .map(|error| format!("{:?}", error).replace([',', '\n', '\r'], ";"))
        .unwrap_or_default();
    let layout = report
        .payload_layout
        .as_ref()
        .map(|layout| match &layout.summary {
            LayoutSummary::Passenger(summary) => format!(
                "passenger(total={}|seated={}|unseated={}|payload_t={:.3}|belly_t={:.3}|hold_t={:.3})",
                summary.total_pax,
                summary.seated_pax,
                summary.unseated_pax,
                summary.payload_t,
                summary.belly_cargo_t,
                summary.hold_used_t
            ),
            LayoutSummary::Cargo(summary) => format!(
                "cargo(requested_t={:.3}|loaded_t={:.3}|payload_t={:.3}|slots={})",
                summary.requested_net_payload_t,
                summary.loaded_net_payload_t,
                summary.payload_t,
                summary.n_slots
            ),
        })
        .unwrap_or_else(|| "none".to_owned());
    let solution_statuses = mission
        .solutions
        .iter()
        .map(|solution| format!("{:?}", solution.status))
        .collect::<Vec<_>>()
        .join("|");
    let throttle_limited = mission
        .solutions
        .iter()
        .filter(|solution| solution.throttle_limited)
        .count();
    let segment_count = format!(
        "{}/{}",
        mission.segments.len(),
        mission.scheduled_segment_count
    );
    let oew = report
        .component_masses
        .get("Operating Empty")
        .copied()
        .unwrap_or_else(|| {
            report
                .component_masses
                .iter()
                .filter(|(name, _)| name.as_str() != "Payload" && name.as_str() != "Fuel")
                .map(|(_, mass)| *mass)
                .sum()
        });
    let payload = report
        .component_masses
        .get("Payload")
        .copied()
        .unwrap_or(f64::NAN);
    let mission_status = format!("{:?}", fuel.mission.status);
    let trip_fuel = summary.map(|value| value.trip_fuel_kg).unwrap_or(f64::NAN);
    let exhaustion_segment = mission
        .fuel_exhaustion
        .as_ref()
        .map(|value| value.segment_tag.as_str())
        .unwrap_or("");
    let exhaustion_burned = mission
        .fuel_exhaustion
        .as_ref()
        .map(|value| value.burned_fuel_kg)
        .unwrap_or(f64::NAN);
    let exhaustion_available = mission
        .fuel_exhaustion
        .as_ref()
        .map(|value| value.available_fuel_kg)
        .unwrap_or(f64::NAN);
    let max_force = cruise
        .and_then(|value| value.max_abs_residual_acceleration_m_s2)
        .unwrap_or(f64::NAN);
    let zfw = fuel.zero_fuel_mass_kg;
    let mzfw = preset.reference.mzfw_kg.unwrap_or(f64::NAN);
    let finding_codes = result
        .feasibility
        .findings
        .iter()
        .map(|finding| format!("{:?}", finding.code))
        .collect::<Vec<_>>()
        .join("+");
    let cg_states = result
        .feasibility
        .model_cg
        .as_ref()
        .map(|assessment| {
            assessment
                .loading_states
                .iter()
                .map(|state| {
                    format!(
                        "{}:{:.5}/{:.5}/{:.5}",
                        state.state.label(),
                        state.static_margin,
                        state.nose_gear_load_fraction,
                        state.cg_pct_mac
                    )
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .unwrap_or_default();
    Ok(format!(
        "{name},{route_nm:.1},{:.1},{oew:.1},{payload:.1},{zfw:.1},{mzfw:.1},{:.1},{:.1},{trip_fuel:.1},{mission_status},{exhaustion_segment},{exhaustion_burned:.1},{exhaustion_available:.1},{segment_count},{solution_statuses},{throttle_limited},{},{max_force:.4e},{cl_target:.6e},{raw_trim_converged},{raw_trim_alpha:.5e},{raw_trim_ih:.5e},{raw_cl_alpha:.5e},{raw_cm_alpha:.5e},{raw_cl_ih:.5e},{raw_cm_ih:.5e},{trim_perf_cm:.4e},{trim_perf_cl:.6e},{trim_perf_error},{trim_cm:.4e},{:.5e},{:.5e},{cg_states}, {},findings={finding_codes}",
        config.requirements.mtow_kg,
        fuel.analyzed_carried_fuel_kg,
        fuel.analyzed_takeoff_mass_kg,
        cruise.map_or(0, |value| value.control_points),
        report.static_margin,
        report.physical_cg[0],
        layout,
    ))
}
