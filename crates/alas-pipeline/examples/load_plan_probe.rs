// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ZFW, take-off and landing CG of each preset against the model
//! envelope, with the load plan off and on, one JSON line per preset and plan
//! mode.
//!
//! ```text
//! cargo run -p alas-pipeline --profile harness --example load_plan_probe -- B747-400 DC-10
//! ```
#![allow(clippy::print_stdout, clippy::unwrap_used)]

use alas_config::{presets, AlasConfig};
use alas_pipeline::feasibility::assess_physical_feasibility;
use alas_pipeline::FullAnalysis;
use serde_json::json;

const FLEET: [&str; 6] = [
    "B747-400",
    "DC-10",
    "A320-200",
    "A350-900",
    "B747-400F",
    "DC-10-30F",
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let names: Vec<&str> = if args.is_empty() {
        FLEET.to_vec()
    } else {
        args.iter().map(String::as_str).collect()
    };
    for preset in names {
        let (base, payload_kg) = match preset {
            "B747-400F" => ("B747-400", 80_000.0),
            "DC-10-30F" => ("DC-10", 50_000.0),
            other => (other, 0.0),
        };
        let config = if payload_kg > 0.0 {
            AlasConfig::from_value(&json!({ "preset": base, "requirements": { "aircraft_type": "cargo", "cargo_payload_kg": payload_kg } })).unwrap()
        } else {
            AlasConfig::from_value(&json!({ "preset": base })).unwrap()
        };
        let design = presets::get(base).unwrap().design_vector;
        for plan in [false, true] {
            let analysis = if plan {
                let policy = if std::env::var_os("LOAD_PLAN_ALWAYS").is_some() {
                    alas_pipeline::LoadPlanPolicy::Always
                } else {
                    alas_pipeline::LoadPlanPolicy::OnViolation
                };
                FullAnalysis::new(config.clone()).with_load_plan(policy)
            } else {
                FullAnalysis::new(config.clone()).without_load_plan()
            };
            let report = match analysis.run(&design, true) {
                Ok(report) => report,
                Err(error) => {
                    println!(
                        "{}",
                        json!({ "preset": preset, "plan": plan, "error": error })
                    );
                    continue;
                }
            };
            let feasibility = assess_physical_feasibility(&config, &design, &report, None);
            let Some(model_cg) = feasibility.model_cg.as_ref() else {
                println!(
                    "{}",
                    json!({ "preset": preset, "plan": plan, "error": "no model cg" })
                );
                continue;
            };
            let states: Vec<_> = model_cg
                .loading_states
                .iter()
                .map(|state| {
                    let l = &state.physical_limits;
                    let failed: Vec<String> = state
                        .constraints
                        .iter()
                        .filter(|c| c.violated)
                        .map(|c| format!("{:?}", c.constraint))
                        .collect();
                    json!({
                        "state": state.state.label(),
                        "mass_kg": state.mass_kg.round(),
                        "cg": (state.cg_pct_mac * 100.0).round() / 100.0,
                        "fwd": (l.fwd_limit_pct_mac * 100.0).round() / 100.0,
                        "aft": (l.aft_limit_pct_mac * 100.0).round() / 100.0,
                        "nose_pct": (state.nose_gear_load_fraction * 1000.0).round() / 10.0,
                        "failed": failed,
                        "nose_min": state
                            .constraints
                            .iter()
                            .find(|c| matches!(c.constraint, alas_opt::envelope::ModelCgConstraint::MinimumNoseGearLoad))
                            .map(|c| (c.limit * 1000.0).round() / 10.0),
                    })
                })
                .collect();
            let plan_keys: std::collections::BTreeMap<_, _> = report
                .geometry_summary
                .iter()
                .filter(|(key, _)| key.starts_with("load_plan"))
                .collect();
            let fuel_items: Vec<_> =
                alas_mass::tanks::resolve_product_layout(&config, &design, &report.airplane)
                    .ok()
                    .and_then(|tanks| {
                        let fuel = report
                            .component_masses
                            .get(alas_mass::breakdown::FUEL)
                            .copied()?;
                        let state = tanks.distribute(fuel).ok()?;
                        Some(
                            state
                                .mass_items(&tanks)
                                .iter()
                                .map(|i| {
                                    json!([
                                        i.id,
                                        i.mass_kg.round(),
                                        (i.position_m[0] * 10.0).round() / 10.0
                                    ])
                                })
                                .collect(),
                        )
                    })
                    .unwrap_or_default();
            println!(
                "{}",
                json!({
                    "fuel_items": fuel_items,
                    "preset": preset,
                    "plan": plan,
                    "payload_kg": report.component_masses.get(alas_mass::breakdown::PAYLOAD),
                    "fuel_kg": report.component_masses.get(alas_mass::breakdown::FUEL),
                    "physical_cg_x": report.physical_cg[0],
                    "min_nose_pct": 100.0 * config.mass_model.pct_load_nlg_min,
                    "states": states,
                    "load_plan": plan_keys,
                })
            );
        }
    }
}
