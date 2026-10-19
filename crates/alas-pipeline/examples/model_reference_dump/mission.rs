// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The pipeline route and mission-telemetry section of the model dump.

use alas_config::AlasConfig;
use alas_pipeline::full_analysis::AnalysisReport;
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};
use serde_json::{json, Value};

use crate::{performance, validation_exports};

/// The `route` section and the CG envelope limits of the feasibility result.
pub(crate) fn section(config: &AlasConfig, report: &AnalysisReport) -> (Value, Value) {
    let options = PipelineOptions {
        optimize: false,
        compare_baseline: false,
        parallel: true,
        output_dir: None,
        save_plots: false,
        seed: Some(42),
        quiet: true,
        ..Default::default()
    };
    let mut cg_envelope_limits = Value::Null;
    let mission = match DesignPipeline::new(config.clone())
        .run(&options, &RunEnvironment::default())
    {
        Err(error) => json!({"status": "pipeline_failed", "error": error.to_string()}),
        Ok(result) => {
            cg_envelope_limits = validation_exports::cg_limits(&result.feasibility);
            let mut summary = json!({"status": "route_unavailable"});
            if let Some(route) = result.route.as_ref() {
                summary = json!({
                    "status": "route_planned",
                    "departure": route.origin_airport.as_ref().map(|a| &a.icao)
                        .or_else(|| alas_config::airports::get(&config.departure_airport).ok().map(|a| &a.icao)),
                    "arrival": route.dest_airport.as_ref().map(|a| &a.icao)
                        .or_else(|| alas_config::airports::get(&config.arrival_airport).ok().map(|a| &a.icao)),
                    "route_source": route.source.as_str(),
                    "planned_route_km": route.total_distance_m() / 1000.0,
                    "route_waypoints": route.waypoints.len(),
                    "mission_status": "unavailable",
                });
            }
            if let Some(m) = result.mission_result.as_ref() {
                let mut cruise = None;
                for seg in &m.segments {
                    if format!("{:?}", seg.spec.kind)
                        .to_lowercase()
                        .contains("cruise")
                    {
                        let c = &seg.conditions;
                        let mid = c.altitude_m.len() / 2;
                        if !c.altitude_m.is_empty() {
                            cruise = Some(json!({
                                "altitude_m": c.altitude_m[mid],
                                "flight_level": c.altitude_m[mid] / 30.48,
                                "mach": c.mach[mid],
                                "tas_m_s": c.velocity_m_s[mid],
                                "mass_kg": c.total_mass_kg[mid],
                                "cl": c.lift_coefficient[mid],
                                "cd": c.drag_coefficient[mid],
                                "l_over_d": c.lift_coefficient[mid] / c.drag_coefficient[mid],
                                "thrust_n": c.thrust[mid].thrust_n,
                                "fuel_flow_kg_h": c.vehicle_mass_rate_kg_s[mid] * 3600.0,
                                "throttle": c.throttle[mid],
                            }));
                        }
                    }
                }
                let last = m.segments.last();
                let initial_cruise_altitude_m = m
                    .segments
                    .iter()
                    .find(|seg| {
                        format!("{:?}", seg.spec.kind)
                            .to_lowercase()
                            .contains("cruise")
                    })
                    .and_then(|seg| seg.conditions.altitude_m.first().copied());
                let telemetry = json!({
                    "initial_cruise_altitude_m": initial_cruise_altitude_m,
                    "mission_status": if m.segments.len() == m.scheduled_segment_count {"completed"} else {"partial"},
                    "segments_flown": m.segments.len(),
                    "segments_scheduled": m.scheduled_segment_count,
                    "distance_flown_km": last.and_then(|s| s.conditions.aircraft_range_m.last()).map(|v| v / 1000.0),
                    "block_time_min": last.and_then(|s| s.conditions.time_s.last()).map(|v| v / 60.0),
                    "trip_fuel_kg": m.segments.first().and_then(|s| s.conditions.total_mass_kg.first().copied())
                        .zip(last.and_then(|s| s.conditions.total_mass_kg.last().copied()))
                        .map(|(a, b)| a - b),
                    "final_cruise": cruise,
                    "cruise_drag_breakdown": performance::cruise_drag_breakdown(m, &report.airplane),
                });
                if let (Some(target), Some(fields)) =
                    (summary.as_object_mut(), telemetry.as_object())
                {
                    target.extend(fields.clone());
                }
            }
            summary
        }
    };
    (mission, cg_envelope_limits)
}
