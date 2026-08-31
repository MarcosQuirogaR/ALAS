#![doc = "Export corrected ALAS reports in the legacy SUAVE runner request format."]

//! This is an investigation aid, not a production bridge.  It builds each
//! registered preset through the current native full-analysis path and writes
//! the vehicle/mission request consumed by the original SUAVE 2.5.2 runner.
//! The runner lives outside this repository because its Python environment is
//! an optional user tool; keeping request generation here makes the two runs
//! use the same Rust mass, geometry, trim and route inputs.

#![allow(clippy::print_stdout)]

use std::path::PathBuf;

use alas_config::{airports::get as get_airport, presets, AlasConfig};
use alas_pipeline::full_analysis::FullAnalysis;
use alas_route::route::Route;

fn main() {
    let mut args = std::env::args().skip(1);
    let output_dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".agent/suave-requests"));
    let requested: Vec<String> = args.collect();
    let names: Vec<String> = if requested.is_empty() {
        presets::available()
            .into_iter()
            .map(str::to_owned)
            .collect()
    } else {
        requested
    };

    std::fs::create_dir_all(&output_dir).expect("request output directory creates");
    for name in names {
        match export_request(&output_dir, &name) {
            Ok(path) => println!("{name}: {}", path.display()),
            Err(error) => eprintln!("{name}: ERROR: {error}"),
        }
    }
}

fn export_request(output_dir: &std::path::Path, name: &str) -> Result<PathBuf, String> {
    let preset = presets::get(name).map_err(|error| error.to_string())?;
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
        .map_err(|error| format!("config: {error}"))?;
    let origin = get_airport(&config.departure_airport).map_err(|error| error.to_string())?;
    let destination = get_airport(&config.arrival_airport).map_err(|error| error.to_string())?;
    let route = Route::great_circle(
        origin,
        destination,
        config.mission.great_circle_points.max(1) as usize,
    );
    let route_distance_m = route.total_distance_m();

    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .map_err(|error| format!("full analysis: {error}"))?;
    let engine = &config.geometry.engine;
    let n_engines = engine.spanwise_positions_m.len();
    let l_over_d = report
        .trimmed_design_point
        .map(|point| point.l_over_d)
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(report.design_point.l_over_d);
    let cruise_thrust_kn = if l_over_d.is_finite() && l_over_d > 0.0 && n_engines > 0 {
        Some(config.requirements.mtow_kg * 9.81 / l_over_d / n_engines as f64 / 1_000.0)
    } else {
        None
    };

    let request = serde_json::json!({
        "vehicle": {
            "name": config.preset,
            "design_vector": serde_json::to_value(report.design).map_err(|error| error.to_string())?,
            "geometry_summary": report.geometry_summary,
            "geometry_config": serde_json::to_value(&config.geometry).map_err(|error| error.to_string())?,
            "mtow_kg": config.requirements.mtow_kg,
            "component_masses_kg": report.component_masses,
            "engine": {
                "n_engines": n_engines,
                "thrust_kn": engine.thrust_kn,
                "cruise_thrust_kn": cruise_thrust_kn,
                "bypass_ratio": engine.bypass_ratio,
                "nacelle_length_m": engine.nacelle_length_m(),
                "nacelle_max_radius_m": engine.radius_scale_m,
                "overall_pressure_ratio": engine.overall_pressure_ratio,
                "turbine_inlet_temp_k": engine.turbine_inlet_temp_k,
                "fan_pressure_ratio": engine.fan_pressure_ratio,
            },
            "requirements": {
                "aircraft_type": config.requirements.aircraft_type,
                "num_passengers": config.requirements.num_passengers,
                "cruise_mach": config.requirements.cruise_mach,
                "cruise_altitude_m": config.requirements.cruise_altitude_m,
                "ultimate_load_factor": config.requirements.ultimate_load_factor,
            },
        },
        "mission": {
            "mission_tag": format!("{}_to_{}", origin.icao, destination.icao),
            "cruise_altitude_m": config.requirements.cruise_altitude_m,
            "departure_elevation_m": origin.elevation_m,
            "arrival_elevation_m": destination.elevation_m,
            "departure_isa_deviation_c": origin.isa_deviation_c,
            "route_distance_m": route_distance_m,
            "profile": serde_json::to_value(&config.mission.profile).map_err(|error| error.to_string())?,
        },
    });

    let path = output_dir.join(format!("{name}.json"));
    let text = serde_json::to_string_pretty(&request).map_err(|error| error.to_string())?;
    std::fs::write(&path, text).map_err(|error| error.to_string())?;
    Ok(path)
}
