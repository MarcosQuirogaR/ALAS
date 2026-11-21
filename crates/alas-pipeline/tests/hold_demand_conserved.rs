// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The default (GE9X widebody) config's belly-hold demand (525 seated x 16 kg bags)
//! against what is loaded, per compartment (container and bulk), with tare separated:
//! Masses kg, x m.
//!
//! ```text
//! cargo test -p alas-pipeline --test hold_demand_conserved -- --nocapture
//! ```
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::print_stdout)]

use alas_config::design_variables::DesignVector;
use alas_config::AlasConfig;
use alas_payload::{ItemKind, ItemMeta, LayoutSummary};
use alas_pipeline::FullAnalysis;

#[test]
fn the_belly_hold_demand_is_met_in_full_and_the_payload_differs_only_by_tare() {
    let mut config = AlasConfig::default();
    config.structures.run_nastran = false;
    config.structures.run_patran_export = false;
    let report = FullAnalysis::new(config.clone())
        .run(&DesignVector::default(), true)
        .expect("analysis");
    let layout = report.payload_layout.as_ref().expect("payload layout");
    let LayoutSummary::Passenger(s) = &layout.summary else {
        panic!("not a passenger layout")
    };
    let demand = s.bag_mass_t * 1000.0 + s.belly_cargo_t * 1000.0;
    println!(
        "seated {} bags {:.1} kg belly {:.1} kg: hold demand (net) {demand:.1} kg; hold used (gross, incl. tare) {:.1} kg; capacity {:.1} kg",
        s.seated_pax,
        s.bag_mass_t * 1000.0,
        s.belly_cargo_t * 1000.0,
        s.hold_used_t * 1000.0,
        s.hold_capacity_t * 1000.0
    );
    println!(
        "compartment masses (gross): {:?}",
        s.hold_compartment_masses_kg
    );
    let (mut gross, mut bulk, mut tare) = (0.0, 0.0, 0.0);
    for item in &layout.items {
        if item.kind != ItemKind::Bag {
            continue;
        }
        let what = match &item.meta {
            ItemMeta::BulkBag => {
                bulk += item.mass;
                "bulk"
            }
            ItemMeta::Container(c) => {
                tare += alas_payload::cargo::uld_by_code(c.uld)
                    .expect("catalogued container")
                    .tare_weight;
                "container"
            }
            _ => continue,
        };
        gross += item.mass;
        println!(
            "  {what:9} x {:7.2} y {:+.2} len {:.2} w {:.2} mass {:.1}",
            item.x, item.y, item.length, item.width, item.mass
        );
    }
    println!(
        "loaded {gross:.1} kg = net {:.1} + tare {tare:.1} (bulk {bulk:.1})",
        gross - tare
    );
    // The hold demand is the checked bags (and requested belly freight): it is met in
    // full, in containers and bulk; the payload differs between hold geometries only by
    // the container tare of the positions the demand needs.
    assert!(
        (gross - tare - demand).abs() < 1.0e-6,
        "hold load net {:.3} kg against a demand of {demand:.3} kg",
        gross - tare
    );
    assert!(
        (demand - 525.0 * 16.0).abs() < 1.0e-6,
        "525 seated x 16 kg bags"
    );
}
