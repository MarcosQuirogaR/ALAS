// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Mission and sizing outputs of the registered presets, for the A1G
//! before/after comparison of the drag-table compressibility basis.
//!
//! `cargo run --release -p alas-opt --example a1g_mission_effect_probe [preset ...]`
//! prints, per preset, the product candidate assessment of the preset's own
//! design vector: sized takeoff mass, block and design-mission fuel, the
//! cruise lift-to-drag the sizing used, and feasibility.
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]

use alas_config::{presets, AlasConfig};
use alas_opt::assess_product_candidate;
use serde_json::json;

fn main() {
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    for name in presets::available() {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == name) {
            continue;
        }
        let Ok(config) = AlasConfig::from_value(&json!({"preset": name})) else {
            println!("{name}: config error");
            continue;
        };
        let Ok(preset) = presets::get(name) else {
            println!("{name}: preset error");
            continue;
        };
        // `A1G_RANGE_NMI=a,b,c` flies the design mission over each range
        // instead (the smoothness of the fuel in the range).
        if let Ok(list) = std::env::var("A1G_RANGE_NMI") {
            for range in list.split(',').filter_map(|v| v.trim().parse::<f64>().ok()) {
                let mut ranged = config.clone();
                ranged.optimizer.objective.design_range_nmi = range;
                match assess_product_candidate(&ranged, &preset.design_vector) {
                    Ok(a) => println!(
                        "{name}: range {range:.0} nmi block fuel {:.1} kg ({:.4} kg/nmi) trip fuel {:.1} kg, plan altitude {:?} m, steps {:?}",
                        a.sized.block_fuel_kg,
                        a.sized.block_fuel_kg / range,
                        a.sized.design_mission_trip_fuel_kg,
                        a.sized
                            .fuel_artifacts
                            .frozen_plan
                            .as_ref()
                            .map(|plan| plan.cruise_altitude_m.round()),
                        a.sized
                            .fuel_artifacts
                            .frozen_plan
                            .as_ref()
                            .map(|plan| plan.step_positions.clone())
                    ),
                    Err(error) => println!("{name}: range {range}: ERROR {error}"),
                }
            }
            continue;
        }
        match assess_product_candidate(&config, &preset.design_vector) {
            Ok(a) => {
                if let Some(plan) = &a.sized.fuel_artifacts.frozen_plan {
                    println!(
                        "{name}: plan cruise altitude {:.0} m, climb rate scales {:?}, step positions {:?}, trip fuel {:.0} kg at {:.0} kg (Richardson {:.1} kg)",
                        plan.cruise_altitude_m,
                        plan.climb_rate_scales,
                        plan.step_positions,
                        plan.trip_fuel_kg,
                        plan.takeoff_mass_kg,
                        plan.richardson_error_kg
                    );
                }
                println!(
                "{name}: MTOW {:.0} kg block fuel {:.0} design-mission fuel {:.0} L/D {:.3} hard_feasible {} hard_violation {:.4} soft_violation {:.4}",
                a.sized.takeoff_mass_kg,
                a.sized.block_fuel_kg,
                a.sized.design_mission_fuel_kg,
                a.sized.lift_to_drag,
                a.hard_feasible,
                a.hard_violation_sum,
                a.soft_violation_sum
                );
            }
            Err(error) => println!("{name}: ERROR {error}"),
        }
    }
}
