// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Diagnostic probe for the mass lane E1 (WB-01, WB-02, WB-05, engine mass).
//!
//! For each registered preset (all of them, or those named on the command
//! line) this prints, as tab-free fixed text:
//!
//! * the FLOPS wing `FCOMP` in force, the OEW, the wing and fuselage masses;
//! * the **fuselage composite OEW band** (diagnostic only, never applied):
//!   the fuselage mass multiplied by the Raymer 6th Table 15.4 advanced-
//!   composite fuselage/nacelle multipliers 0.90 and 0.95, and the resulting
//!   OEW change in kg and percent. NASA/TM-2017-219627 Eq. 56 has no
//!   fuselage technology factor, so this is an uncertainty band, not a
//!   model term;
//! * the engine, nacelle and pylon masses per engine, with the certified
//!   dry mass and the Eq. 76 correlation `THRSO / 5.5` for comparison;
//! * the operating-empty CG decomposed per component (mass, x, moment,
//!   contribution to the OEW CG in metres and percent MAC), so the A320
//!   34 % MAC residual can be read component by component. Nothing is tuned.
//!
//! Usage:
//!
//! ```text
//! cargo run -p alas-mass --release --example oew_composite_probe -- [--cg] [preset...]
//! ```
//!
//! Frames: x positive aft from the geometry origin (nose side), metres; %MAC
//! uses the airplane's own `MacFrame` (reference chord and LEMAC station).

#![allow(clippy::print_stdout)]

use alas_config::{presets, AlasConfig};
use alas_geom::builder::AircraftBuilder;
use alas_mass::breakdown::{run_product_mass_analysis_with_groups, MassCoordinateModel, OEW_KEYS};
use serde_json::json;
use std::error::Error;

/// Raymer 6th ed. Table 15.4 (p. 580), advanced composites, fuselage/nacelle.
const RAYMER_FUSELAGE_COMPOSITE_MULTIPLIERS: [f64; 2] = [0.90, 0.95];
const LB_PER_KG: f64 = 2.204_622_621_848_776;
const LBF_PER_N: f64 = 0.224_808_943_099_710_5;

fn probe(name: &str, show_cg: bool) -> Result<(), Box<dyn Error>> {
    let preset = presets::get(name).map_err(std::io::Error::other)?;
    // ALAS_PROBE_CABIN=flops_transport_v1 | lth_civil_transport_v1 overrides
    // the cabin-equipment method for a what-if; unset keeps the preset's own.
    let mut overlay = json!({"preset": name});
    if let Ok(method) = std::env::var("ALAS_PROBE_CABIN") {
        overlay["mass_model"] = json!({"flops_transport": {"cabin_equipment_method": method}});
    }
    // ALAS_PROBE_OVERLAY='{"mass_model":{...}}' is merged on top (what-if only).
    if let Ok(extra) = std::env::var("ALAS_PROBE_OVERLAY") {
        let extra: serde_json::Value = serde_json::from_str(&extra)?;
        if let (Some(base), Some(add)) = (overlay.as_object_mut(), extra.as_object()) {
            for (key, value) in add {
                base.insert(key.clone(), value.clone());
            }
        }
    }
    let config = AlasConfig::from_value(&overlay)?;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)?;
    let (masses, coords, _, build) = run_product_mass_analysis_with_groups(
        &plane,
        &config.requirements,
        &config.geometry,
        &config.cabin,
        &config.control_surfaces,
        Some(&config.mass_model),
        None,
        MassCoordinateModel::StructuralWingbox(&config.structures),
        &config.landing_gear,
    )?;
    if std::env::var_os("ALAS_PROBE_XSEC").is_some() {
        if let Some(f) = plane.fuselages.iter().find(|f| f.name == "Fuselage") {
            for x in &f.xsecs {
                println!(
                    "  xsec x {:.2} w {:.2} h {:.2}",
                    x.xyz_c[0], x.width, x.height
                );
            }
        }
    }
    let oew: f64 = OEW_KEYS.iter().filter_map(|key| masses.get(key)).sum();
    let build = build.ok_or("no FLOPS buildup (not the pure FLOPS product)")?;
    let fcomp = build.airframe.structure_inputs.wing.composite_utilization;
    println!(
        "{name}: FCOMP {fcomp:.2}  OEW {oew:.0} kg  wing {:.0}  fuselage {:.0}",
        masses.wing, masses.fuselage
    );
    let band: Vec<String> = RAYMER_FUSELAGE_COMPOSITE_MULTIPLIERS
        .iter()
        .map(|m| {
            let delta = masses.fuselage * (m - 1.0);
            format!(
                "x{m:.2}: {delta:+.0} kg ({:+.2} % OEW)",
                100.0 * delta / oew
            )
        })
        .collect();
    println!(
        "  fuselage composite band (Raymer T15.4, NOT applied): {}",
        band.join("; ")
    );
    if let (Some(prop), Some(structure)) = (&build.airframe.propulsion, &build.airframe.structure) {
        let n = build.airframe.propulsion_inputs.engine_count.max(1) as f64;
        let thrust_n = build.airframe.propulsion_inputs.rated_thrust_per_engine_n;
        let eq76_kg = thrust_n * LBF_PER_N / 5.5 / LB_PER_KG;
        let declared = config
            .mass_model
            .flops_structure
            .baseline_engine_mass_kg
            .map_or("none".to_owned(), |m| format!("{m:.1}"));
        println!(
            "  per engine: model {:.0} kg  Eq.76 THRSO/5.5 {:.0} kg  declared certified dry {declared} kg",
            prop.engines_kg / n,
            eq76_kg
        );
        println!(
            "  per engine: nacelle {:.0} kg (Eq.69, cowl only)  pylon {:.0} kg (LTH, outside FLOPS)  pylon/OEW {:.2} %",
            structure.nacelle_kg / n,
            prop.pylons_kg / n,
            100.0 * prop.pylons_kg / oew
        );
    }
    if std::env::var_os("ALAS_PROBE_GROUPS").is_some() {
        println!(
            "  systems/operating items: {:#?}",
            build.systems_and_operating_items
        );
        println!("  structure: {:#?}", build.airframe.structure);
        println!("  structure_inputs: {:#?}", build.airframe.structure_inputs);
        println!("  propulsion: {:#?}", build.airframe.propulsion);
        println!("  turboprop: {:#?}", build.airframe.turboprop_propulsion);
        println!("  masses: {masses:#?}");
    }
    if show_cg {
        let frame = plane.mac_frame().ok_or("no MAC frame")?;
        let pairs_m = masses.as_pairs();
        let pairs_x = coords.as_pairs();
        let mut sum_m = 0.0;
        let mut sum_mx = 0.0;
        for ((key, mass), (_, xyz)) in pairs_m.iter().zip(pairs_x.iter()) {
            if OEW_KEYS.contains(key) {
                sum_m += mass;
                sum_mx += mass * xyz[0];
            }
        }
        let cg = sum_mx / sum_m;
        let pct = |x: f64| 100.0 * (x - frame.x_lemac_m) / frame.chord_m;
        println!(
            "  OEW CG x {cg:.3} m = {:.1} % MAC (LEMAC {:.3} m, MAC {:.3} m)",
            pct(cg),
            frame.x_lemac_m,
            frame.chord_m
        );
        println!("  component        mass_kg      x_m   %MAC   dCG_if_removed_%MAC");
        for ((key, mass), (_, xyz)) in pairs_m.iter().zip(pairs_x.iter()) {
            if !OEW_KEYS.contains(key) {
                continue;
            }
            // Shift of the OEW CG if this component were absent: a component
            // aft of the CG moves it forward when removed.
            let without = (sum_mx - mass * xyz[0]) / (sum_m - mass);
            println!(
                "  {key:<14} {mass:>9.0} {:>8.3} {:>6.1} {:>+10.1}",
                xyz[0],
                pct(xyz[0]),
                pct(without) - pct(cg)
            );
        }
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut show_cg = false;
    let mut names: Vec<String> = Vec::new();
    for arg in std::env::args().skip(1) {
        if arg == "--cg" {
            show_cg = true;
        } else {
            names.push(arg);
        }
    }
    if names.is_empty() {
        names = presets::available()
            .into_iter()
            .map(str::to_owned)
            .collect();
    }
    for name in &names {
        if let Err(error) = probe(name, show_cg) {
            println!("{name}: unavailable ({error})");
        }
    }
    Ok(())
}
