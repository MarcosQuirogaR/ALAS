// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Parity test for [`alas_report::route_geometry`]: spherical projection and mission profile sync.
//!
//! Validates coordinate calculations and interpolation at Tier::Closed.

// A test asserts on values it constructed or loaded from a fixture it controls, so a failed unwrap there is the assertion failing.
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

use alas_report::route_geometry::{route_to_xyz, sync_mass_to_route_series};
use alas_route::route::{Route, RouteSource, Waypoint};
use alas_testkit::{Comparison, Tier};
use serde_json::Value;

#[test]
fn route_geometry_and_sync_match_reference() {
    let fixture: Value = alas_testkit::load("report", "route_geometry");
    let mut check = Comparison::new("report/route_geometry", Tier::Closed);

    let wps = vec![
        Waypoint::named(40.4168, -3.7038, "LEMD"),
        Waypoint::named(48.8566, 2.3522, "LFPG"),
        Waypoint::named(51.5074, -0.1278, "EGLL"),
    ];
    let route = Route::new(wps, RouteSource::NavdataGraph);
    let alts = vec![600.0, 11000.0, 30.0];

    let xyz = route_to_xyz(&route, &alts);
    let ref_xyz = &fixture["route_to_xyz"]["xyz"];

    for (i, pt) in xyz.iter().enumerate() {
        check.scalar(
            &format!("xyz[{i}].x"),
            pt[0],
            ref_xyz[i][0].as_f64().unwrap(),
        );
        check.scalar(
            &format!("xyz[{i}].y"),
            pt[1],
            ref_xyz[i][1].as_f64().unwrap(),
        );
        check.scalar(
            &format!("xyz[{i}].z"),
            pt[2],
            ref_xyz[i][2].as_f64().unwrap(),
        );
    }

    let mission_time = vec![0.0, 1800.0, 5400.0, 7200.0];
    let mission_tas = vec![120.0, 230.0, 240.0, 140.0];
    let mission_mass = vec![75000.0, 72000.0, 68000.0, 66000.0];
    let mission_alt = vec![600.0, 11000.0, 11000.0, 30.0];

    let (mass_synced, alt_synced) = sync_mass_to_route_series(
        &route,
        &mission_time,
        &mission_tas,
        &mission_mass,
        &mission_alt,
    );

    let ref_mass = &fixture["sync_mass_to_route"]["mass"];
    let ref_alt = &fixture["sync_mass_to_route"]["altitude"];

    for i in 0..mass_synced.len() {
        check.scalar(
            &format!("mass_synced[{i}]"),
            mass_synced[i],
            ref_mass[i].as_f64().unwrap(),
        );
        check.scalar(
            &format!("alt_synced[{i}]"),
            alt_synced[i],
            ref_alt[i].as_f64().unwrap(),
        );
    }

    check.finish();
}
