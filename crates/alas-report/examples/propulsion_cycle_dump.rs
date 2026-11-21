// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The propulsion cycle summary of every registered turbofan preset: the throttled cruise point the cycle figures show, with the cycle,
//! mission-deck and catalogue TSFC side by side.
//!
//! `cargo run -p alas-report --example propulsion_cycle_dump [preset...]`
//! prints the text panel of each preset and nothing else.

#![allow(clippy::print_stdout, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_prop::cycle::{compute_turbofan_cycle, TurbofanCycleInputs};
use alas_report::families::propulsion::propulsion_cycle_summary;

fn main() {
    let wanted: Vec<String> = std::env::args().skip(1).collect();
    for name in presets::available() {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == name) {
            continue;
        }
        let config = AlasConfig::from_value(&serde_json::json!({ "preset": name }))
            .expect("registered preset");
        println!("== {name}");
        let lines = propulsion_cycle_summary(&config);
        for line in &lines {
            if !line.trim().is_empty() {
                println!("  {line}");
            }
        }
        // The station temperatures of the throttled point, from the cycle at
        // the temperature the summary reports.
        let Ok(alas_config::ActiveEngineModel::Turbofan(spec)) =
            config.geometry.engine.active_model()
        else {
            continue;
        };
        // The throttled point's own inputs, read back from the summary row.
        let row = lines
            .iter()
            .find(|line| line.trim_start().starts_with("BPR ="))
            .cloned()
            .unwrap_or_default();
        let read = |key: &str| {
            row.split(key)
                .nth(1)
                .and_then(|rest| rest.split_whitespace().next())
                .and_then(|number| number.parse::<f64>().ok())
        };
        let (tit_k, opr, fpr) = (read("TIT = "), read("OPR = "), read("FPR = "));
        // The on-design cycle at the catalogue (take-off) temperature, which
        // is what the figures showed before the cruise point was throttled.
        let full = compute_turbofan_cycle(
            &TurbofanCycleInputs {
                mach: config.requirements.cruise_mach,
                altitude_m: config.requirements.cruise_altitude_m,
                bypass_ratio: spec.bypass_ratio,
                overall_pressure_ratio: spec.overall_pressure_ratio,
                fan_pressure_ratio: spec.fan_pressure_ratio,
                turbine_inlet_temperature_k: spec.turbine_inlet_temp_k,
            },
            &config.propulsion_cycle,
        );
        println!(
            "  at take-off TIT {:.0} K: TSFC {:.2} mg/(N.s), eta_o {:.3}",
            spec.turbine_inlet_temp_k, full.tsfc_mg_ns, full.overall_efficiency
        );
        if let (Some(tit_k), Some(opr), Some(fpr)) = (tit_k, opr, fpr) {
            let out = compute_turbofan_cycle(
                &TurbofanCycleInputs {
                    mach: config.requirements.cruise_mach,
                    altitude_m: config.requirements.cruise_altitude_m,
                    bypass_ratio: spec.bypass_ratio,
                    overall_pressure_ratio: opr,
                    fan_pressure_ratio: fpr,
                    turbine_inlet_temperature_k: tit_k,
                },
                &config.propulsion_cycle,
            );
            println!(
                "  stations: T0 {:.0} Tt2 {:.0} Tt13 {:.0} Tt25 {:.0} Tt3 {:.0} Tt4 {:.0} Tt45 {:.0} Tt5 {:.0} K, fuel-air {:.4}",
                alas_atmo::Atmosphere::new(config.requirements.cruise_altitude_m).temperature(),
                out.temperature_t0_k,
                out.temperature_t13_k,
                out.temperature_t25_k,
                out.temperature_t3_k,
                out.temperature_t4_k,
                out.temperature_t45_k,
                out.temperature_t5_k,
                out.fuel_air_ratio,
            );
        }
    }
}
