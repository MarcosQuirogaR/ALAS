// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Every forward and aft CG mechanism of each loading state of the registered
//! presets, %MAC of the model frame, next to the terms of the landing-trim and
//! rotation boundaries.
//!
//! ```text
//! cargo run -p alas-pipeline --profile harness --example stab_limits_probe -- A320-200 B787-9
//! ```
#![allow(clippy::print_stdout, clippy::unwrap_used)]

use alas_config::{presets, AlasConfig};
use alas_pipeline::feasibility::assess_physical_feasibility;
use alas_pipeline::FullAnalysis;
use serde_json::json;

const FLEET: [&str; 14] = [
    "A320-200",
    "A340-300",
    "A380-800",
    "A350-900",
    "B747-400",
    "B787-9",
    "DC-10",
    "A220-300",
    "C919",
    "E195-E2",
    "ATR72-600",
    "A400M",
    "LJ60",
    "C295",
];

fn main() {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let before = args.first().is_some_and(|a| a == "--before");
    if before {
        args.remove(0);
    }
    let names: Vec<&str> = if args.is_empty() {
        FLEET.to_vec()
    } else {
        args.iter().map(String::as_str).collect()
    };
    for preset in names {
        let mut config = AlasConfig::from_value(&json!({ "preset": preset })).unwrap();
        if before {
            // Travel data of the earlier presets.
            config.landing_gear.elevator_up_travel_deg = match preset {
                "ATR72-600" => Some(21.8),
                "C295" => Some(25.0),
                _ => None,
            };
            config.landing_gear.stabilizer_nose_up_travel_deg = match preset {
                "A320-200" => Some(11.2),
                "A340-300" => Some(13.5),
                _ => None,
            };
        }
        let design = presets::get(preset).unwrap().design_vector;
        let report = match FullAnalysis::new(config.clone()).run(&design, true) {
            Ok(report) => report,
            Err(error) => {
                println!(
                    "{}",
                    json!({ "preset": preset, "error": error.to_string() })
                );
                continue;
            }
        };
        let plane = &report.airplane;
        let frame = plane.mac_frame().unwrap();
        let wing = &plane.wings[0];
        let feasibility = assess_physical_feasibility(&config, &design, &report, None);
        let Some(model_cg) = feasibility.model_cg.as_ref() else {
            println!("{}", json!({ "preset": preset, "error": "no model cg" }));
            continue;
        };
        let states: Vec<_> = model_cg
            .loading_states
            .iter()
            .map(|state| {
                let l = &state.physical_limits;
                let row = |kind: alas_opt::ModelCgConstraint| {
                    state
                        .constraints
                        .iter()
                        .find(|row| row.constraint == kind)
                        .map(|row| json!([row.actual, row.limit]))
                };
                json!({
                    "state": format!("{:?}", state.state),
                    "h_cg": state.h_cg_m,
                    "tip_back_deg": row(alas_opt::ModelCgConstraint::TipBack),
                    "tail_scrape_deg": row(alas_opt::ModelCgConstraint::TailScrape),
                    "nose_share": state.nose_gear_load_fraction,
                    "mass_kg": state.mass_kg,
                    "cg": state.cg_pct_mac,
                    "fwd": l.fwd_limit_pct_mac,
                    "fwd_by": format!("{:?}", l.fwd_limit_governance),
                    "nose": l.max_nose_load_fwd_pct_mac,
                    "rotation": l.rotation_fwd_pct_mac,
                    "rot_force_shift": l.rotation_longitudinal_force_shift_pct_mac,
                    "land_app": l.landing_approach_fwd_pct_mac,
                    "land_flare": l.landing_flare_fwd_pct_mac,
                    "land": l.landing_trim_fwd_pct_mac,
                    "cl_h_land": l.landing_tail_lift_coefficient,
                    "trim_ltd": l.landing_tail_trim_limited,
                    "sourced": l.landing_tail_sourced,
                    "land_cond": l.landing_trim_condition,
                    "scissor": l.scissor_plot_fwd_pct_mac,
                    "iced": l.iced_tail_fwd_pct_mac,
                    "cl_h_rot": l.rotation_tail_lift_coefficient,
                    "ths_deg": l.rotation_stabilizer_nose_up_deg,
                    "aft": l.aft_limit_pct_mac,
                    "aft_by": format!("{:?}", l.aft_limit_governance),
                    "aero_aft": l.aerodynamic_aft_pct_mac,
                    "ground_aft": l.ground_aft_pct_mac,
                    "tip_aft": l.tip_back_aft_pct_mac,
                    "clean_np": l.clean_np_pct_mac,
                })
            })
            .collect();
        println!(
            "{}",
            json!({
                "preset": preset,
                "mac": frame.chord_m,
                "lemac": frame.x_lemac_m,
                "box_ac_frac": (wing.carried_box().aerodynamic_center(0.25)[0] - frame.x_lemac_m) / frame.chord_m,
                "s_ref": plane.s_ref,
                "x_np_clean": report.x_neutral_point,
                "np_conditions": report.neutral_point_conditions.as_ref().map(|n| json!({
                    "clean": n.clean_low_speed, "cruise": n.cruise, "high_lift": n.high_lift,
                    "scale": n.high_lift_slope_scale, "critical": n.critical,
                })),
                "cl_max_land": config.performance.cl_max_land,
                "cl_max_to": config.performance.cl_max_to,
                "cl_max_clean": config.performance.cl_max_clean,
                "wing": {
                    "ar": plane.b_ref * plane.b_ref / plane.s_ref,
                    "sweep_c4": wing.mean_sweep_angle(0.25),
                    "taper": wing.taper_ratio(),
                    "hinge_sweep": wing.mean_sweep_angle(1.0 - config.control_surfaces.flap_chord_fraction),
                    "z_ac": wing.carried_box().aerodynamic_center(0.25)[2],
                },
                "tail": plane.wings.get(1).map(|tail| {
                    let reference = tail.reference_planform();
                    json!({
                        "s": tail.reference_area(),
                        "ar": reference.aspect_ratio(),
                        "sweep_c4": tail.mean_sweep_angle(0.25),
                        "taper": tail.taper_ratio(),
                        "x_ac": reference.aerodynamic_center(0.25)[0],
                        "z_ac": reference.aerodynamic_center(0.25)[2],
                        "q_ratio": tail.dynamic_pressure_ratio,
                        "built_incidence_deg": tail.xsecs.iter().map(|s| s.chord * s.twist).sum::<f64>()
                            / tail.xsecs.iter().map(|s| s.chord).sum::<f64>(),
                    })
                }),
                "controls": {
                    "elev_chord": config.control_surfaces.elevator_chord_fraction,
                    "elev_span": [config.control_surfaces.elevator_span_start_frac, config.control_surfaces.elevator_span_end_frac],
                    "elev_up_deg": config.landing_gear.elevator_up_travel_deg,
                    "ths_travel_deg": config.landing_gear.stabilizer_nose_up_travel_deg,
                    "ths_takeoff_deg": config.landing_gear.takeoff_stabilizer_nose_up_deg,
                    "flap_span": [config.control_surfaces.flap_span_start_frac, config.control_surfaces.flap_span_end_frac],
                    "flap_chord": config.control_surfaces.flap_chord_fraction,
                },
                "n_stations": model_cg.loading_states.len(),
                "states": states,
            })
        );
    }
}
