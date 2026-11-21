// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Physical quantities of every registered preset's built geometry, pinned
//! with tolerances.
//!
//! The hash pins of `preset_geometry_hash` detect any change of the built
//! geometry but cannot say which dimension moved or by how much. These pins
//! can: each is one SI quantity of the built aircraft (flight shape, engines
//! included), held through `alas_testkit::pins` in
//! `tests/golden/preset_geometry_quantities.json`, so a failure names the
//! quantity, the built and pinned values and the tolerance, and a re-pin is a
//! reviewable numeric diff.
//!
//! Per lifting surface (main wing, horizontal tail, vertical tail): area
//! m^2, span m, mean aerodynamic chord m and the x of the quarter-chord
//! aerodynamic centre m. Per aircraft: the primary fuselage's nose and tail
//! station x m, largest width and height m, and station count.

// A failed unwrap is the assertion failing on a registered preset.
#![allow(clippy::unwrap_used)]

use alas_config::presets;
use alas_geom::builder::AircraftBuilder;
use alas_testkit::pins::assert_pinned;

const SUITE: &str = "preset_geometry_quantities";

#[test]
fn every_preset_builds_its_pinned_dimensions() {
    for name in presets::available() {
        let preset = presets::get(name).unwrap();
        let plane = AircraftBuilder::new(Some(preset.geometry.clone()))
            .build(Some(&preset.design_vector), true)
            .unwrap();
        let pin = |quantity: &str, value: f64| {
            assert_pinned(SUITE, &format!("{name}/{quantity}"), value);
        };
        for (surface, wing) in ["wing", "horizontal_tail", "vertical_tail"]
            .iter()
            .zip(&plane.wings)
        {
            pin(&format!("{surface}_area_m2"), wing.area());
            pin(&format!("{surface}_span_m"), wing.span());
            pin(
                &format!("{surface}_mean_aerodynamic_chord_m"),
                wing.mean_aerodynamic_chord(),
            );
            pin(
                &format!("{surface}_quarter_chord_centre_x_m"),
                wing.aerodynamic_center(0.25)[0],
            );
        }
        let fuselage = &plane.fuselages[0];
        let first = fuselage.xsecs.first().unwrap();
        let last = fuselage.xsecs.last().unwrap();
        let largest = |values: &mut dyn Iterator<Item = f64>| values.fold(f64::NAN, f64::max);
        pin("fuselage_nose_x_m", first.xyz_c[0]);
        pin("fuselage_tail_x_m", last.xyz_c[0]);
        pin(
            "fuselage_largest_width_m",
            largest(&mut fuselage.xsecs.iter().map(|section| section.width)),
        );
        pin(
            "fuselage_largest_height_m",
            largest(&mut fuselage.xsecs.iter().map(|section| section.height)),
        );
        pin("fuselage_station_count", fuselage.xsecs.len() as f64);
    }
}
