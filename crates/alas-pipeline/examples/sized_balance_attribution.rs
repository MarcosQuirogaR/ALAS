// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Where does a mission-sized aeroplane balance, group by group, against the
//! same aeroplane analysed directly at its declared ceiling?
//!
//! `crates/alas-mass/examples/gear_reaction_balance.rs` measures the second of
//! those: the preset's own nominal geometry, the lumped planning payload, and
//! the declared `mtow_kg` as the mass basis. Every registered preset comes out
//! with a positive nose reaction there. The search closes the mission instead,
//! and the A380-800 and A320-200 both come out with the centre of gravity aft
//! of the effective main-gear station, which is the `min_nose_gear_load`
//! blocker the all-preset matrix reports.
//!
//! The previous mass/gear phase recorded that gap as "15-19 %MAC, not
//! attributed", because it could not separate a legitimate loading difference
//! (the closed mass carries different fuel and a resolved cabin) from a
//! coordinate that failed to survive convergence. `sizing_coordinate_divergence`
//! has since shown the search and the finalist report agree to 0.000 m on both
//! presets, so the remaining question is only *which term* moves the centre of
//! gravity between the two mass bases. This probe prints both ledgers side by
//! side so the answer is read rather than inferred.
//!
//! SI: kg, m. `%MAC = (x - mac_le)/mac * 100`, one frame per preset, so the
//! frame cancels from every difference.
//!
//! Run:
//! `cargo run --release -p alas-pipeline --example sized_balance_attribution`

#![allow(clippy::print_stdout, clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig};
use alas_mass::breakdown::OEW_KEYS;
use alas_pipeline::FullAnalysis;

const PROBE: &[&str] = &["A380-800", "A320-200", "A340-300"];

const GROUPS: [&str; 10] = [
    "Wing",
    "H-Stab",
    "V-Stab",
    "Fuselage",
    "Gear",
    "Propulsion",
    "Systems",
    "Furnishings",
    "Payload",
    "Fuel",
];

fn main() {
    println!(
        "Mission-sized ledger against the direct declared-ceiling ledger.\n\
         SI: kg, m. Mode: the registered preset's own configuration and design vector.\n"
    );
    for name in PROBE {
        println!("================ {name} ================");
        let Ok(preset) = presets::get(name) else {
            println!("preset lookup failed");
            continue;
        };
        let Ok(config) = AlasConfig::from_value(&serde_json::json!({ "preset": name })) else {
            println!("configuration did not load");
            continue;
        };
        let design = preset.design_vector;

        // Direct: the declared ceiling is the mass basis, exactly what
        // `FullAnalysis::run` does without a mission closure.
        let Ok(direct) = FullAnalysis::new(config.clone()).run(&design, true) else {
            println!("direct analysis refused");
            continue;
        };
        // Sized: the mission-closed takeoff mass the search's own gate used.
        let assessment = match alas_opt::assess_product_candidate(&config, &design) {
            Ok(assessment) => assessment,
            Err(reason) => {
                println!("search refused the nominal candidate: {reason}");
                continue;
            }
        };
        let Ok(sized) = FullAnalysis::new(config.clone()).run_at_sized_takeoff_mass(
            &assessment.resolved.design,
            true,
            assessment.sized.takeoff_mass_kg,
        ) else {
            println!("sized report refused");
            continue;
        };

        let mac = direct.airplane.c_ref;
        let mac_le = direct.airplane.wings.first().map_or(f64::NAN, |wing| {
            wing.aerodynamic_center(0.25)[0] - 0.25 * mac
        });
        let pct = |x: f64| 100.0 * (x - mac_le) / mac;
        println!("mac {mac:.4} m | mac_le {mac_le:.4} m");

        println!(
            "{:<12} {:>12} {:>12} {:>10} {:>10} {:>12}",
            "group", "direct kg", "sized kg", "direct x", "sized x", "moment d kgm"
        );
        let mut moment_shift_total = 0.0;
        for group in GROUPS {
            let direct_kg = direct.component_masses.get(group).copied().unwrap_or(0.0);
            let sized_kg = sized.component_masses.get(group).copied().unwrap_or(0.0);
            let direct_x = direct
                .mass_coordinates
                .get(group)
                .and_then(|value| value.first().copied())
                .unwrap_or(f64::NAN);
            let sized_x = sized
                .mass_coordinates
                .get(group)
                .and_then(|value| value.first().copied())
                .unwrap_or(f64::NAN);
            let moment_shift = sized_kg * sized_x - direct_kg * direct_x;
            moment_shift_total += moment_shift;
            println!(
                "{group:<12} {direct_kg:>12.1} {sized_kg:>12.1} {direct_x:>10.3} {sized_x:>10.3} \
                 {moment_shift:>12.0}"
            );
        }

        let oew = |report: &alas_pipeline::AnalysisReport| -> (f64, f64) {
            let mut mass = 0.0;
            let mut moment = 0.0;
            for key in OEW_KEYS {
                let group_mass = report.component_masses.get(key).copied().unwrap_or(0.0);
                let group_x = report
                    .mass_coordinates
                    .get(key)
                    .and_then(|value| value.first().copied())
                    .unwrap_or(0.0);
                mass += group_mass;
                moment += group_mass * group_x;
            }
            (mass, if mass > 0.0 { moment / mass } else { f64::NAN })
        };
        let (direct_oew, direct_x_oew) = oew(&direct);
        let (sized_oew, sized_x_oew) = oew(&sized);
        println!(
            "\noperating empty : direct {direct_oew:.1} kg at {direct_x_oew:.3} m \
             ({:.2} %MAC) | sized {sized_oew:.1} kg at {sized_x_oew:.3} m ({:.2} %MAC)",
            pct(direct_x_oew),
            pct(sized_x_oew)
        );
        println!(
            "physical cg     : direct {:.3} m ({:.2} %MAC) | sized {:.3} m ({:.2} %MAC) | \
             shift {:+.3} m ({:+.2} points)",
            direct.physical_cg[0],
            pct(direct.physical_cg[0]),
            sized.physical_cg[0],
            pct(sized.physical_cg[0]),
            sized.physical_cg[0] - direct.physical_cg[0],
            pct(sized.physical_cg[0]) - pct(direct.physical_cg[0]),
        );
        println!("total moment difference {moment_shift_total:.0} kg m");

        // The five loading states the gear residuals are actually taken from,
        // with the admissibility verdict the envelope now carries.
        let envelope = alas_opt::assess_model_cg_envelope(
            &sized.airplane,
            &assessment.resolved.masses,
            &assessment.resolved.coords,
            assessment.resolved.cg_x_m,
            assessment.resolved.x_neutral_point_m,
            assessment.resolved.mac_m,
            &config,
        );
        match envelope {
            Ok(envelope) => {
                println!(
                    "\n{:<22} {:>12} {:>10} {:>10} {:>12} {:>14}",
                    "state", "mass kg", "cg m", "cg %MAC", "nose frac", "reactions"
                );
                for state in &envelope.loading_states {
                    println!(
                        "{:<22} {:>12.1} {:>10.3} {:>10.2} {:>12.5} {:>14}",
                        state.state.label(),
                        state.mass_kg,
                        state.cg_x_m,
                        state.cg_pct_mac,
                        state.nose_gear_load_fraction,
                        if state.ground_reactions_admissible {
                            "on both legs"
                        } else {
                            "TAIL-SITTING"
                        },
                    );
                }
            }
            Err(error) => println!("\nenvelope refused: {error}"),
        }

        // The only group whose mass and station both move between the two
        // bases is the fuel, so print where the closed load actually sits.
        // `FuelTankLayout::distribute` fills the tanks burned *last* first
        // (`crates/alas-mass/src/tanks/distribute.rs` module doc), so a
        // partial load is the outer wing and, where one exists, the trim
        // tank -- the two furthest aft volumes on a swept wing.
        let (density_kg_m3, published_total_l) =
            alas_mass::product_stations::tank_reference(&config, &design);
        match alas_mass::tanks::FuelTankLayout::resolve(
            &sized.airplane,
            &config.geometry,
            &config.structures,
            &config.fuel_tanks,
            &config.fuel_policy,
            density_kg_m3,
            published_total_l,
        ) {
            Ok(layout) => {
                let sized_fuel_kg = sized.component_masses.get("Fuel").copied().unwrap_or(0.0);
                let direct_fuel_kg = direct.component_masses.get("Fuel").copied().unwrap_or(0.0);
                println!(
                    "\nusable capacity {:.1} kg | direct load {direct_fuel_kg:.1} kg | \
                     sized load {sized_fuel_kg:.1} kg",
                    layout.usable_capacity_kg()
                );
                println!(
                    "{:<18} {:>8} {:>10} {:>10} {:>12} {:>12}",
                    "tank", "burn", "x m", "x %MAC", "capacity kg", "sized fill"
                );
                let sized_state = layout.distribute(sized_fuel_kg.min(layout.usable_capacity_kg()));
                let fills: Vec<(String, f64)> = sized_state
                    .map(|state| {
                        state
                            .mass_items(&layout)
                            .into_iter()
                            .map(|item| (item.id, item.mass_kg))
                            .collect()
                    })
                    .unwrap_or_default();
                for tank in layout.tanks() {
                    let fill = fills
                        .iter()
                        .find(|(id, _)| *id == tank.id)
                        .map_or(0.0, |(_, mass)| *mass);
                    println!(
                        "{:<18} {:>8} {:>10.3} {:>10.2} {:>12.1} {:>12.1}",
                        tank.id,
                        tank.burn_priority,
                        tank.centroid_m[0],
                        pct(tank.centroid_m[0]),
                        tank.usable_capacity_kg,
                        fill,
                    );
                }
            }
            Err(error) => println!("\nfuel tank layout refused: {error}"),
        }
        println!();
    }
}
