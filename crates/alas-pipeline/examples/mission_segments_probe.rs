// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Scratch probe: every mission segment for one preset.
#![allow(clippy::print_stdout, missing_docs)]
// Standalone fixture diagnostics fail immediately when their curated inputs are invalid.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_pipeline::{DesignPipeline, PipelineOptions, RunEnvironment};

fn main() {
    let name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "A320-200".to_owned());
    let preset = presets::get(&name).unwrap();
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset.name })).unwrap();
    println!(
        "route {} -> {}; requirements cruise {:.0} ft @ M{:.2}",
        config.departure_airport,
        config.arrival_airport,
        config.requirements.cruise_altitude_m / 0.3048,
        config.requirements.cruise_mach
    );
    let p = &config.mission.profile;
    println!(
        "profile cruise TAS {:.1}/{:.1}/{:.1} m/s; climb fracs {:.2}/{:.2}/{:.2}; cruise dist fracs {:.2}/{:.2}/{:.2}",
        p.cruise_1_air_speed_m_s, p.cruise_2_air_speed_m_s, p.cruise_3_air_speed_m_s,
        p.initial_climb_altitude_fraction, p.step_climb_1_altitude_fraction, 0.0,
        p.cruise_1_distance_fraction, p.cruise_2_distance_fraction, p.cruise_3_distance_fraction,
    );
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
    let result = DesignPipeline::new(config)
        .run(&options, &RunEnvironment::default())
        .unwrap();
    let Some(m) = result.mission_result.as_ref() else {
        println!("no mission result");
        return;
    };
    if let Some(r) = result.route.as_ref() {
        println!(
            "planned route: {:.1} km over {} waypoints; status {:?}",
            r.total_distance_m() / 1000.0,
            r.waypoints.len(),
            result.route_status
        );
    }
    println!(
        "segments {} of {} scheduled; exhaustion {:?}",
        m.segments.len(),
        m.scheduled_segment_count,
        m.fuel_exhaustion.is_some()
    );
    println!(
        "{:<28} {:>7} {:>7} {:>7} {:>7} {:>8} {:>8} {:>7} {:>7}",
        "segment", "FL0", "FL1", "Mach", "range_km", "mass_t", "ff_kg_h", "thr", "L/D"
    );
    for seg in &m.segments {
        let c = &seg.conditions;
        let n = c.altitude_m.len();
        if n == 0 {
            continue;
        }
        let mid = n / 2;
        println!(
            "{:<28} {:>7.0} {:>7.0} {:>7.3} {:>8.0} {:>8.1} {:>8.0} {:>7.3} {:>7.2}",
            format!("{:?}", seg.spec.kind),
            c.altitude_m[0] / 30.48,
            c.altitude_m[n - 1] / 30.48,
            c.mach[mid],
            c.aircraft_range_m[n - 1] / 1000.0,
            c.total_mass_kg[mid] / 1000.0,
            c.vehicle_mass_rate_kg_s[mid] * 3600.0,
            c.throttle[mid],
            c.lift_coefficient[mid] / c.drag_coefficient[mid],
        );
    }
}
