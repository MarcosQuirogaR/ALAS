// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Probe of the `cruise_thrust` check at the levels the mission plan flies
//! (owner decision 2026-10-14): for each registered jet, the design-level
//! requirement at the mid-cruise mass (the earlier definition), then every
//! level of the frozen plan with its mass, Mach, required and available
//! static T/W, the governing level, and the highest level reachable at the
//! initial cruise mass. Run with
//! `cargo test -p alas-opt --release --test cruise_levels_probe -- --ignored --nocapture`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::AlasConfig;
use alas_opt::assess_product_candidate;
use alas_opt::mdo::{
    deck_cruise_requirements, highest_cruise_level_m, planned_cruise_requirements, thrust_scatter,
    EngineOutModel, ThrustAssessment, ThrustVerdict,
};

const FT_M: f64 = 0.3048;

fn verdict(a: &ThrustAssessment) -> &'static str {
    match a.verdict() {
        ThrustVerdict::Pass => "pass",
        ThrustVerdict::Warning => "warning",
        ThrustVerdict::Hard => "HARD",
    }
}

#[test]
#[ignore = "probe: prints the per-jet cruise levels of all registered presets"]
fn cruise_levels_of_every_registered_jet() {
    let only = std::env::var("ALAS_PROBE_PRESETS").ok();
    for name in alas_config::presets::available() {
        if only
            .as_ref()
            .is_some_and(|list| !list.split(',').any(|n| n == name))
        {
            continue;
        }
        let config = AlasConfig::from_value(&serde_json::json!({"preset": name})).unwrap();
        let design = alas_config::presets::get(name).unwrap().design_vector;
        let a = assess_product_candidate(&config, &design).unwrap();
        println!(
            "FUEL {name}: block fuel {:.1} kg, takeoff {:.0} kg, initial plan level FL{:.1}, hard {:?}",
            a.sized.block_fuel_kg,
            a.sized.takeoff_mass_kg,
            a.sized
                .fuel_artifacts
                .frozen_plan
                .as_ref()
                .map_or(f64::NAN, |p| p.cruise_altitude_m / FT_M / 100.0),
            a.violated_hard_ids()
        );
        let Some(row) = a.residuals.iter().find(|r| r.id == "cruise_thrust") else {
            println!("PROBE {name}: no cruise_thrust row");
            continue;
        };
        let scatter = thrust_scatter(&config);
        let available = row.actual;
        let artifacts = &a.sized.fuel_artifacts;
        let s_ref = artifacts.reference_area_m2;
        let tow = a.sized.takeoff_mass_kg;
        let req = &config.requirements;
        let Ok(model) = EngineOutModel::new(&config, None, s_ref, None) else {
            println!("PROBE {name}: no engine-out model (turboprop?)");
            continue;
        };
        let design_level = deck_cruise_requirements(
            &model,
            &artifacts.drag,
            req,
            tow,
            a.sized.mid_cruise_mass_kg(),
            s_ref,
        )
        .unwrap();
        let before = ThrustAssessment::new(design_level.level_tw, available, scatter);
        println!(
            "PROBE {name}: BEFORE design FL{:.0} M{:.3} mid-mass {:.0} kg: req {:.4} avail {available:.4} short {:+.1} % band {:.1} % -> {}",
            req.cruise_altitude_m / FT_M / 100.0,
            req.cruise_mach,
            a.sized.mid_cruise_mass_kg(),
            design_level.level_tw,
            100.0 * before.shortfall(),
            100.0 * scatter,
            verdict(&before)
        );
        if let Ok(mission) = alas_opt::mdo::candidate_mission_model(&config, artifacts) {
            if let Some(plan) = &artifacts.frozen_plan {
                match mission.fly_trip(tow, plan.range_m) {
                    Ok(flown) => println!(
                        "PROBE {name}: mission flight: rate-limited steps {} of the trip, cruise segments {}, final level {:.0} m",
                        flown.rate_limited_steps,
                        flown.cruise_states.len(),
                        flown.final_cruise_altitude_m
                    ),
                    Err(error) => println!("PROBE {name}: mission flight failed: {error}"),
                }
            }
        }
        match planned_cruise_requirements(&model, &config, artifacts, tow, s_ref) {
            Ok(plan) => {
                let after = ThrustAssessment::new(plan.worst.level_tw, available, scatter);
                for level in &plan.levels {
                    println!(
                        "PROBE {name}:   level FL{:.0} ({:.0} m) M{:.3} mass {:.0} kg req {:.4}{}",
                        level.altitude_m / FT_M / 100.0,
                        level.altitude_m,
                        level.mach,
                        level.mass_kg,
                        level.required_tw,
                        if level.required_tw > available {
                            " ABOVE-AVAILABLE"
                        } else {
                            ""
                        }
                    );
                }
                let highest = highest_cruise_level_m(
                    &model,
                    &artifacts.drag,
                    req.gravity_m_s2,
                    &plan.initial(),
                    (tow, s_ref),
                    available,
                );
                println!(
                    "PROBE {name}: AFTER worst FL{:.0} req {:.4} avail {available:.4} short {:+.1} % -> {} | row violation {:.4} | highest level at the initial mass FL{} | row {:?} | hard {:?}",
                    plan.worst_level.altitude_m / FT_M / 100.0,
                    plan.worst.level_tw,
                    100.0 * after.shortfall(),
                    verdict(&after),
                    row.normalized_violation,
                    highest.map_or("none".to_owned(), |h| format!("{:.0}", h / FT_M / 100.0)),
                    row.limit,
                    a.violated_hard_ids(),
                );
            }
            Err(reason) => println!("PROBE {name}: AFTER no plan ({reason})"),
        }
    }
}
