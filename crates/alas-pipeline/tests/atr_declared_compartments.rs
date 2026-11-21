// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The ATR 72-600's declared main-deck baggage compartments are source data
//! (ATR 72-600 Factsheet p.22: forward 3.47-5.51 m, aft 20.72-22.95 m, the aft
//! one also housing the lavatory). The generated cabin has to fit around them:
//! seats and monuments start aft of the forward compartment, the declared
//! extents are never shortened, and the zero-fuel centre of gravity stays where
//! the compartments put it.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::{presets, AlasConfig, DesignMode, HoldDeck};
use alas_payload::{build_payload_layout, ItemKind};
use alas_pipeline::{assess_physical_feasibility, FullAnalysis};

const FORWARD: (f64, f64) = (3.47, 5.51);
const AFT: (f64, f64) = (20.72, 22.95);

fn config() -> AlasConfig {
    AlasConfig::from_value(&serde_json::json!({ "preset": "ATR72-600" })).unwrap()
}

#[test]
fn the_declared_compartments_are_the_sourced_extents_and_stay_so() {
    let config = config();
    let main: Vec<_> = config
        .cabin
        .cargo
        .hold_compartments
        .iter()
        .filter(|hold| hold.deck == HoldDeck::Main)
        .collect();
    assert_eq!(main.len(), 2);
    assert_eq!((main[0].x_start_m, main[0].x_end_m), FORWARD);
    assert_eq!((main[1].x_start_m, main[1].x_end_m), AFT);
}

#[test]
fn seats_and_monuments_stand_between_the_two_compartments_and_bags_stay_in_theirs() {
    let config = config();
    let preset = presets::get("ATR72-600").unwrap();
    let plane = alas_geom::builder::AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .unwrap();
    let layout = build_payload_layout(&plane, &config, 0.0, 0.0).unwrap();
    for item in &layout.items {
        let (a, b) = (item.x - 0.5 * item.length, item.x + 0.5 * item.length);
        match item.kind {
            ItemKind::SeatRow
            | ItemKind::Galley
            | ItemKind::Lav
            | ItemKind::AccessibleLav
            | ItemKind::WheelchairStowage => {
                assert!(
                    a >= FORWARD.1 - 1e-6,
                    "{:?} at {a:.2}..{b:.2} m reaches into the forward compartment",
                    item.kind
                );
                assert!(
                    b <= AFT.0 + 1e-6,
                    "{:?} at {a:.2}..{b:.2} m reaches into the aft compartment",
                    item.kind
                );
            }
            ItemKind::Bag => {
                let inside = |(lo, hi): (f64, f64)| a >= lo - 1e-6 && b <= hi + 1e-6;
                assert!(
                    inside(FORWARD) || inside(AFT),
                    "bag block {a:.2}..{b:.2} m is outside the declared compartments"
                );
            }
            _ => {}
        }
    }
}

#[test]
fn the_zero_fuel_cg_is_where_the_declared_compartments_put_it() {
    let mut config = config();
    config.optimizer.design_space.mode = DesignMode::BaselineSandbox;
    config.mission.enabled = false;
    config.mses.enabled = false;
    config.structures.enabled = false;
    let design = presets::get("ATR72-600").unwrap().design_vector;
    let report = FullAnalysis::new(config.clone())
        .run(&design, true)
        .unwrap();
    let physical = assess_physical_feasibility(&config, &design, &report, None);
    let statement = physical.mass_balance.as_ref().expect("mass statement");
    let zero_fuel = statement
        .states
        .iter()
        .find(|state| state.label == "zero fuel")
        .unwrap()
        .cg_pct_mac;
    println!("ATR72-600 zero-fuel CG {zero_fuel:.2} % MAC");
    // The zero-fuel CG (28.31 % MAC) rests on these sourced inputs. The fuselage group of a
    // wing-mounted propeller aircraft sits at 39 % of the fuselage length (Torenbeek 1982
    // Table 8-15, printed p. 294, 38-40 %), not 45 %, which moves the ATR's zero-fuel CG
    // 9.2 % MAC forward. The sourced ATR 72-600 belly clearance from the Aircraft Recovery
    // Manual (0.64 m, not the 1.0035 m 0.35 D fallback) shortens the sponson main leg, and
    // the FLOPS gear mass (eq. 63, 64) goes with the oleo length to the 0.43, so both gears
    // are about 17 % lighter (866 -> 714 kg) and the CG moves aft by 0.33 % MAC zero-fuel
    // (OEW CG 33.54 -> 34.08 % MAC). The compartment volumes are the Jane's 5.8 / 4.8 m3 of
    // the 72-seat 29 in cabin (10.6 m3; ATR's published total is 9.4 m3 for 72 passengers at
    // 30 in, EWADE 2011): the forward compartment holds most of the bags at 161 kg/m3, and
    // the rest stands aft (about 17 m on about 20 t is about 0.9 % MAC per 25 kg). The
    // 15 kg seat allowance of the regional relation (1,080 kg) sits at the seat rows
    // (12.99 m), not at the cabin station (12.08 m): +2.04 % MAC on the 20.6 t zero-fuel
    // mass. The printed outer-panel quarter-chord sweep moves the wing and its centroid.
    assert!((zero_fuel - 28.31).abs() < 0.1, "{zero_fuel}");
}
