// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Specific-air-range curve probe (model side only).
//!
//! Builds the baseline full analysis of one preset and prints, at one mass
//! and level, the factors of the specific air range against true airspeed:
//! `SR = V / FF = 1 / (D * c)`, with `c = FF / (T V)` the fuel per unit of
//! thrust power (for a turboprop `PSFC / eta_p`). The drag columns come from
//! the mission model's own drag table, the fuel from its propulsion deck at
//! the thrust that balances that drag. No reference value is read.
//!
//! Usage: `sr_curve_probe PRESET MASS_KG ALTITUDE_M [TAS_LO TAS_HI STEP]`
#![allow(clippy::print_stdout, clippy::print_stderr, missing_docs)]

use alas_config::{presets, AlasConfig};
use alas_pipeline::fuel_model::report_mission_model;
use alas_pipeline::full_analysis::FullAnalysis;
use alas_prop::system::PropulsionRating;
use serde_json::json;

fn main() {
    if let Err(error) = run() {
        eprintln!("sr_curve_probe: {error}");
        std::process::exit(2);
    }
}

fn run() -> Result<(), String> {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let arg = |i: usize| -> Result<f64, String> {
        argv.get(i)
            .ok_or_else(|| format!("argument {i} missing"))?
            .parse::<f64>()
            .map_err(|e| e.to_string())
    };
    let name = argv.first().ok_or("preset name required")?.clone();
    let mass = arg(1)?;
    let altitude = arg(2)?;
    let (lo, hi, step) = if argv.len() >= 6 {
        (arg(3)?, arg(4)?, arg(5)?)
    } else {
        (90.0, 260.0, 5.0)
    };
    let mut config =
        AlasConfig::from_value(&json!({ "preset": name })).map_err(|e| e.to_string())?;
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    let preset = presets::get(&name).map_err(|e| e.to_string())?;
    let report = FullAnalysis::new(config.clone())
        .run(&preset.design_vector, true)
        .map_err(|e| e.to_string())?;
    let model = report_mission_model(&config, &report)?.for_range_search();
    let artifacts = report.fuel.artifacts(&config, &report.design)?;
    let table = artifacts
        .drag
        .table()
        .ok_or("no native drag table")?
        .clone();
    let atm = alas_atmo::us1976_compute_values(altitude, model.isa_deviation_c);
    let g = model.gravity_at(altitude);
    let s_ref = model.wing_area_m2;
    println!(
        "preset {name} mass {mass:.0} kg altitude {altitude:.0} m S {s_ref:.2} m2 a {:.2} m/s rho {:.4}",
        atm.speed_of_sound_m_s, atm.density_kg_m3
    );
    println!(
        "{:>6} {:>6} {:>6} {:>8} {:>8} {:>8} {:>8} {:>6} {:>9} {:>8} {:>10} {:>9} {:>7}",
        "tas",
        "mach",
        "cl",
        "cd0",
        "cd_ind",
        "cd_visL",
        "cd_wave",
        "L/D",
        "drag_kN",
        "ff_kg_h",
        "c_ug_J",
        "sr_m_kg",
        "rating"
    );
    let mut tas = lo;
    while tas <= hi + 1e-9 {
        let mach = tas / atm.speed_of_sound_m_s;
        let q = 0.5 * atm.density_kg_m3 * tas * tas;
        let cl = mass * g / (q * s_ref);
        let cd = table.cd(cl, mach, altitude);
        let cd0 = table.cd0(mach, altitude);
        let drag = cd * q * s_ref;
        let row = model
            .propulsion
            .flight_condition(altitude, tas, g, model.isa_deviation_c)
            .map_err(|e| e.to_string())
            .and_then(|flight| {
                model
                    .propulsion
                    .at_thrust(flight, drag, PropulsionRating::Cruise)
                    .map_err(|e| e.to_string())
            });
        match row {
            Ok(p) => println!(
                "{tas:6.1} {mach:6.3} {cl:6.3} {cd0:8.5} {:8.5} {:8.5} {:8.5} {:6.2} {:9.2} {:8.1} {:10.4} {:9.3} {:7.3}",
                table.induced_cd(cl),
                table.viscous_lift_cd(cl, cd0),
                table.wave_cd(cl, mach),
                cl / cd,
                drag / 1000.0,
                p.fuel_flow_kg_s * 3600.0,
                p.fuel_flow_kg_s / (p.thrust_n * tas) * 1.0e9,
                tas / p.fuel_flow_kg_s,
                p.rating_fraction
            ),
            Err(e) => println!("{tas:6.1} {mach:6.3} {cl:6.3} {cd0:8.5} cd {cd:8.5} : {e}"),
        }
        tas += step;
    }
    let reference = model.cruise_mach * atm.speed_of_sound_m_s;
    println!(
        "long_range_cruise(reference {reference:.1} m/s): {:?}",
        model.long_range_cruise(mass, altitude, reference)
    );
    println!(
        "long_range_cruise(reference {:.1} m/s, configured cruise_2): {:?}",
        model.profile.cruise_2_air_speed_m_s,
        model.long_range_cruise(mass, altitude, model.profile.cruise_2_air_speed_m_s)
    );
    Ok(())
}
