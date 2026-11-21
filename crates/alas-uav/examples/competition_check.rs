// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Headless check of the fixed-wing UAV competition search.
//!
//! The desktop UAV page is the only product entry for this workflow, so this
//! example drives the same library calls with the page's default brief for
//! every embedded competition preset and a small evaluation budget, then
//! reports the best candidate and scans the whole result for non-finite
//! numbers. It checks that the workflow starts, finishes and returns finite
//! numbers; it is not a validation of the designs.
//!
//! `cargo run --profile harness -p alas-uav --example competition_check -- [evaluations] [seed]`

// The example owns the console; library code reports through typed values.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::collections::BTreeMap;

use alas_uav::catalog::{Catalog, ComponentKind, Dimensions};
use alas_uav::optimization_catalog;
use alas_uav::optimizer::{
    optimize_competition_observed, CompetitionProblem, DesignObjectives, GeometrySearchBounds,
    OptimizationProblem, PreliminaryModel, PropulsionMap, SystemsDefinition, TailVolumeBounds,
    VariableBounds,
};
use alas_uav::propulsion_electric::{native_propulsion_map, CataloguePowertrainSelection};

fn dimensions(length_m: f64, width_m: f64, height_m: f64) -> Dimensions {
    Dimensions {
        length_m,
        width_m,
        height_m,
    }
}

fn bounds(minimum: f64, maximum: f64) -> VariableBounds {
    VariableBounds { minimum, maximum }
}

fn ids_of(catalog: &Catalog, keep: impl Fn(&ComponentKind) -> bool) -> Vec<String> {
    catalog
        .records
        .iter()
        .filter(|record| keep(&record.kind))
        .map(|record| record.id.clone())
        .collect()
}

/// One map per (motor, propeller, cell count): the first ESC that resolves.
fn maps(catalog: &Catalog, objectives: &DesignObjectives, rho: f64) -> Vec<PropulsionMap> {
    let top = 1.5
        * objectives
            .cruise_speed_m_s
            .max(objectives.maximum_stall_speed_m_s);
    let mut speeds: Vec<f64> = (0..=16).map(|i| top * f64::from(i) / 16.0).collect();
    speeds.push(objectives.maximum_stall_speed_m_s);
    speeds.push(objectives.cruise_speed_m_s);
    speeds.sort_by(f64::total_cmp);
    speeds.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    let batteries: Vec<(&str, u16)> = catalog
        .records
        .iter()
        .filter_map(|record| match &record.kind {
            ComponentKind::Battery(spec) => {
                spec.series_cells.map(|cells| (record.id.as_str(), cells))
            }
            _ => None,
        })
        .collect();
    let motors = ids_of(catalog, |k| matches!(k, ComponentKind::Motor(_)));
    let propellers = ids_of(catalog, |k| matches!(k, ComponentKind::Propeller(_)));
    let escs = ids_of(catalog, |k| matches!(k, ComponentKind::Esc(_)));
    let mut cells: Vec<u16> = batteries.iter().map(|(_, c)| *c).collect();
    cells.sort_unstable();
    cells.dedup();
    let mut out = Vec::new();
    for motor in &motors {
        for propeller in &propellers {
            for count in &cells {
                let Some((battery, _)) = batteries.iter().find(|(_, c)| c == count) else {
                    continue;
                };
                for esc in &escs {
                    let selection = CataloguePowertrainSelection {
                        battery_id: (*battery).to_owned(),
                        motor_id: motor.clone(),
                        esc_id: esc.clone(),
                        propeller_id: propeller.clone(),
                        motor_count: 1,
                    };
                    if let Ok(map) = native_propulsion_map(catalog, &selection, &speeds, rho) {
                        out.push(map);
                        break;
                    }
                }
            }
        }
    }
    out
}

fn non_finite(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    for token in ["NaN", "inf"] {
        let mut from = 0;
        while let Some(at) = text[from..].find(token) {
            let start = from + at;
            let end = start + token.len();
            let before = start.checked_sub(1).map(|i| bytes[i]);
            let after = bytes.get(end).copied();
            let word = |b: Option<u8>| b.is_some_and(|c| c.is_ascii_alphanumeric() || c == b'_');
            if !word(before) && !word(after) {
                let lo = start.saturating_sub(60);
                return Some(text[lo..end].to_owned());
            }
            from = end;
        }
    }
    None
}

fn main() {
    let mut args = std::env::args().skip(1);
    let evaluations: usize = args.next().and_then(|v| v.parse().ok()).unwrap_or(40);
    let seed: u64 = args
        .next()
        .and_then(|v| v.parse().ok())
        .unwrap_or(20_260_922);

    // The desktop page's default brief.
    let objectives = DesignObjectives {
        endurance_s: 900.0,
        range_m: 10_000.0,
        cruise_speed_m_s: 20.0,
        maximum_stall_speed_m_s: 10.0,
        payload_mass_kg: 0.5,
        payload_dimensions: dimensions(0.25, 0.12, 0.10),
        minimum_propulsive_efficiency: 0.15,
        efficiency_priority: 0.5,
    };
    let geometry_bounds = GeometrySearchBounds {
        wing_area_m2: bounds(0.9, 1.2),
        wing_aspect_ratio: bounds(8.0, 10.0),
        fuselage_length_m: bounds(1.8, 2.0),
        wing_leading_edge_fraction: bounds(0.25, 0.45),
    };
    let model = PreliminaryModel {
        air_density_kg_m3: 1.225,
        maximum_lift_coefficient: 1.6,
        zero_lift_drag_coefficient: 0.035,
        oswald_efficiency: 0.8,
        limit_load_factor: 3.0,
        structural_safety_factor: 1.5,
        horizontal_tail_volume_coefficient: 0.5,
        vertical_tail_volume_coefficient: 0.04,
        horizontal_tail_aspect_ratio: 4.0,
        vertical_tail_aspect_ratio: 1.6,
        forward_cg_chord_fraction: 0.15,
        aft_cg_chord_fraction: 0.35,
        equipment_clearance_m: 0.015,
        equipment_gap_m: 0.015,
        nose_length_fraction: 0.10,
        tailcone_length_fraction: 0.30,
        spar_cap_width_fraction: 0.06,
        spar_cap_separation_fraction: 0.14,
        aileron_area_fraction: 0.08,
        aileron_chord_fraction: 0.25,
        elevator_area_fraction: 0.30,
        elevator_chord_fraction: 0.30,
        hinge_moment_coefficient: 0.01,
        landing_gear_track_fraction: 0.18,
        landing_gear_wheelbase_fraction: 0.35,
        propeller_ground_clearance_m: 0.05,
        fixed_systems_mass_kg: 0.15,
    };
    let systems = SystemsDefinition {
        avionics_mass_kg: 0.10,
        avionics_dimensions: dimensions(0.08, 0.06, 0.03),
        avionics_power_w: 25.0,
        control_bus_current_a: 0.8,
        control_bus_voltage_v: 6.0,
        minimum_receiver_channels: 6,
        servo_continuous_current_fraction: 0.2,
        maximum_depth_of_discharge: 0.8,
        reserve_fraction: 0.2,
    };

    let catalog = match optimization_catalog() {
        Ok(catalog) => catalog,
        Err(error) => {
            println!("FAIL catalogue unavailable: {error}");
            std::process::exit(1);
        }
    };
    let maps = maps(catalog, &objectives, model.air_density_kg_m3);
    println!(
        "catalogue {} records, {} propulsion maps",
        catalog.records.len(),
        maps.len()
    );
    let mut failures = 0;
    for preset_id in alas_uav::score::presets::ids() {
        let base = OptimizationProblem {
            catalog,
            propulsion_maps: &maps,
            mission_profile: None,
            objectives,
            geometry_bounds,
            model,
            systems,
            required_electronics_role: "gps_sensor",
            seed,
            evaluations,
        };
        let problem = CompetitionProblem {
            base,
            preset_id,
            preset_variant: None,
            announced_takeoff_m: 60.0,
            extra_inputs: BTreeMap::from([
                ("payload_count".to_owned(), 2.0),
                ("declared_payload_count".to_owned(), 2.0),
                ("payload_volume".to_owned(), 0.25 * 0.12 * 0.10),
            ]),
            tail_volume_bounds: TailVolumeBounds::default(),
            population: None,
            include_landing: true,
        };
        let result = optimize_competition_observed(&problem, &mut |_| true);
        match result {
            Ok(optimum) => {
                let text = format!("{optimum:?}");
                let bad = non_finite(&text);
                let (feasible, score) = optimum.best.as_ref().map_or((false, f64::NAN), |best| {
                    (
                        best.evaluation.feasible,
                        best.evaluation.score.unwrap_or(f64::NAN),
                    )
                });
                println!(
                    "{preset_id}: ok evaluations {} feasible {} bundles {} (+{} rejected) best_feasible {feasible} score {score:.3} {:.1}s{}",
                    optimum.evaluations,
                    optimum.feasible_evaluations,
                    optimum.compatible_bundles,
                    optimum.rejected_bundles,
                    optimum.elapsed_s,
                    bad.as_ref()
                        .map_or(String::new(), |b| format!(" NONFINITE ..{b}"))
                );
                if bad.is_some() {
                    failures += 1;
                }
            }
            Err(error) => {
                println!("{preset_id}: FAIL {error}");
                failures += 1;
            }
        }
    }
    std::process::exit(i32::from(failures > 0));
}
