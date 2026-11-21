// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Cruise cycle of every catalogue turbofan against its declared cruise TSFC:
//! the cycle at the catalogue (take-off) turbine
//! inlet temperature, and the throttled operating point that delivers the
//! catalogue cruise thrust.
//!
//! `cargo run -p alas-prop --example turbofan_cycle_probe` prints two lines per
//! engine and nothing else.

#![allow(clippy::print_stdout)]

use alas_config::engines::database;
use alas_config::{PropulsionCycleConfig, PropulsionTechnology};
use alas_prop::cycle::operating_line::solve_cruise_operating_point;
use alas_prop::cycle::{compute_turbofan_cycle, TurbofanCycleInputs, TurbofanCycleResult};

fn line(
    label: &str,
    tit: f64,
    out: &TurbofanCycleResult,
    catalogue: f64,
    thrust_kn: f64,
) -> String {
    format!(
        "  {label:<8} TIT {tit:>5.0} K  F {thrust_kn:>6.1} kN  TSFC {:>6.2} vs {catalogue:>5.2} mg/Ns (x{:.3})  eta th {:.3} p {:.3} o {:.3}  v19 {:>4.0} v9 {:>4.0}",
        out.tsfc_mg_ns,
        out.tsfc_mg_ns / catalogue,
        out.thermal_efficiency,
        out.propulsive_efficiency,
        out.overall_efficiency,
        out.exit_velocity_fan_ms,
        out.exit_velocity_core_ms,
    )
}

fn main() {
    let cfg = PropulsionCycleConfig::default();
    for engine in database() {
        if engine.technology != PropulsionTechnology::Turbofan {
            continue;
        }
        let (Some(off), Some(spec)) = (engine.off_design.as_ref(), engine.turbofan_spec()) else {
            continue;
        };
        let inputs = TurbofanCycleInputs {
            mach: off.cruise_reference_mach,
            altitude_m: off.cruise_reference_altitude_m,
            bypass_ratio: engine.bypass_ratio,
            overall_pressure_ratio: engine.overall_pressure_ratio,
            fan_pressure_ratio: engine.fan_pressure_ratio,
            turbine_inlet_temperature_k: engine.turbine_inlet_temp_k,
        };
        let catalogue = engine.cruise_tsfc_kg_kgf_hr / (9.806_65 * 3_600.0) * 1.0e6;
        println!(
            "{} (BPR {:.1}, OPR {:.1}, FPR {:.2}, rated {:.0} kN, cruise ref {:.1} kN at M{:.2} {:.0} m)",
            engine.name,
            engine.bypass_ratio,
            engine.overall_pressure_ratio,
            engine.fan_pressure_ratio,
            spec.rated_thrust_kn,
            off.cruise_reference_thrust_n / 1000.0,
            off.cruise_reference_mach,
            off.cruise_reference_altitude_m,
        );
        let full = compute_turbofan_cycle(&inputs, &cfg);
        println!(
            "{}",
            line(
                "take-off",
                engine.turbine_inlet_temp_k,
                &full,
                catalogue,
                f64::NAN
            )
        );
        match solve_cruise_operating_point(
            &inputs,
            spec.rated_thrust_kn * 1_000.0,
            off.cruise_reference_thrust_n,
            &cfg,
        ) {
            Ok(point) => println!(
                "{}",
                line(
                    "throttled",
                    point.inputs.turbine_inlet_temperature_k,
                    &point.cycle,
                    catalogue,
                    point.thrust_n / 1000.0
                )
            ),
            Err(reason) => println!("  throttled: {reason}"),
        }
        // Fan pressure ratio scan at the throttled thrust (TSFC only).
        let mut best = (f64::INFINITY, 0.0);
        let mut row = String::new();
        for step in 0..=20 {
            let fpr = 1.2 + 0.06 * f64::from(step);
            let scan = TurbofanCycleInputs {
                fan_pressure_ratio: fpr,
                ..inputs
            };
            if let Ok(point) = solve_cruise_operating_point(
                &scan,
                spec.rated_thrust_kn * 1_000.0,
                off.cruise_reference_thrust_n,
                &cfg,
            ) {
                row.push_str(&format!(" {fpr:.2}:{:.2}", point.cycle.tsfc_mg_ns));
                if point.cycle.tsfc_mg_ns < best.0 {
                    best = (point.cycle.tsfc_mg_ns, fpr);
                }
            }
        }
        println!("  FPR scan (FPR:TSFC):{row}");
        println!(
            "  best TSFC {:.2} at FPR {:.2} (x{:.3} of catalogue)",
            best.0,
            best.1,
            best.0 / catalogue
        );
    }
}
