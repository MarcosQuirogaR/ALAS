// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Every registered aircraft's fin stands on its body: the built root
//! touches the top of the fuselage (or of a centreline nacelle) at the
//! lowest point under its root chord and floats nowhere above it, the root
//! chord ends over the body, and the tip stays at the configured height.
//! The attached fin keeps the drawn planform's straight edges, so the
//! published fin areas follow from them.

// A test unwrap is the assertion failing on a registered preset.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, GeometryConfig};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::fuselage::Fuselage;
use alas_geom::aircraft::wing::{Wing, WingXSec};
use alas_geom::builder::AircraftBuilder;

fn built(preset: &str) -> (GeometryConfig, Airplane) {
    let preset = presets::get(preset).unwrap();
    let plane = AircraftBuilder::new(Some(preset.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .unwrap();
    (preset.geometry.clone(), plane)
}

fn fin_of(plane: &Airplane) -> &Wing {
    plane
        .wings
        .iter()
        .find(|wing| wing.name == "Vertical Stabilizer")
        .unwrap()
}

/// The bodies on the plane of symmetry: the fuselage and centreline nacelles.
fn seat(plane: &Airplane) -> Vec<&Fuselage> {
    plane
        .fuselages
        .iter()
        .enumerate()
        .filter(|(index, body)| *index == 0 || body.xsecs[0].xyz_c[1] == 0.0)
        .map(|(_, body)| body)
        .collect()
}

fn top_at(body: &Fuselage, x: f64) -> Option<f64> {
    body.xsecs.windows(2).find_map(|pair| {
        let (a, b) = (&pair[0], &pair[1]);
        (x >= a.xyz_c[0] && x <= b.xyz_c[0] && b.xyz_c[0] > a.xyz_c[0]).then(|| {
            let t = (x - a.xyz_c[0]) / (b.xyz_c[0] - a.xyz_c[0]);
            let (za, zb) = (a.xyz_c[2] + a.height / 2.0, b.xyz_c[2] + b.height / 2.0);
            za + t * (zb - za)
        })
    })
}

fn highest_top(bodies: &[&Fuselage], x: f64) -> Option<f64> {
    bodies
        .iter()
        .filter_map(|body| top_at(body, x))
        .reduce(f64::max)
}

/// Area of the fin's straight-edged trapezoid between height `z_m` and its
/// tip, m^2.
fn area_above(root: &WingXSec, tip: &WingXSec, z_m: f64) -> f64 {
    let s = (z_m - root.xyz_le[2]) / (tip.xyz_le[2] - root.xyz_le[2]);
    let chord = root.chord + s * (tip.chord - root.chord);
    0.5 * (chord + tip.chord) * (tip.xyz_le[2] - z_m)
}

#[test]
fn every_preset_fin_root_stands_on_its_body() {
    for preset in presets::available() {
        let (geometry, plane) = built(preset);
        let fin = fin_of(&plane);
        let (root, tip) = (&fin.xsecs[0], fin.xsecs.last().unwrap());
        let bodies = seat(&plane);
        let tail = &geometry.empennage;
        let design = presets::get(preset).unwrap().design_vector;
        let fin_scale = design.tail_scale * tail.vstab_scale_ratio;

        assert!(
            (tip.xyz_le[2] - (tail.vstab_z_m + tail.vstab_tip_le_m.2 * fin_scale)).abs() < 1e-9,
            "{preset}: the fin tip moved"
        );
        let body_end = bodies
            .iter()
            .filter_map(|body| body.xsecs.last().map(|xsec| xsec.xyz_c[0]))
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(
            root.xyz_le[0] + root.chord <= body_end + 1e-9,
            "{preset}: the root chord overhangs the end of the body by {:.2} m",
            root.xyz_le[0] + root.chord - body_end
        );
        let tops: Vec<f64> = (0..=4000)
            .map(|k| root.xyz_le[0] + root.chord * f64::from(k) / 4000.0)
            .map(|x| highest_top(&bodies, x).unwrap())
            .collect();
        let lowest = tops.iter().copied().fold(f64::INFINITY, f64::min);
        assert!(
            root.xyz_le[2] <= lowest + 1e-6,
            "{preset}: the root floats {:.3} m above the body",
            root.xyz_le[2] - lowest
        );
        assert!(
            lowest - root.xyz_le[2] < 5e-3,
            "{preset}: the root sits {:.3} m inside the body at its lowest point",
            lowest - root.xyz_le[2]
        );
    }
}

#[test]
fn the_a320_and_a220_fins_close_their_published_areas() {
    // A320-200: 21.5 m^2 over the 5.87 m fin height Airbus measures from the
    // fuselage top line (AC Jun 01/24, Figure 2-2-0-991-004-A01 sheet 1).
    let (geometry, plane) = built("A320-200");
    let fin = fin_of(&plane);
    let body = &geometry.fuselage;
    let crown = body.cabin_z_m + body.height_m.unwrap_or(body.diameter_m) / 2.0;
    let area = area_above(&fin.xsecs[0], fin.xsecs.last().unwrap(), crown);
    assert!((area - 21.5).abs() < 0.05, "A320 fin area {area:.2} m^2");

    // A220-300: ACP Issue 013 Table 6, 28.2 m^2 (304 ft^2) to the fuselage
    // axis, the edges extended down through the body.
    let (geometry, plane) = built("A220-300");
    let fin = fin_of(&plane);
    let axis = geometry.fuselage.cabin_z_m;
    let area = area_above(&fin.xsecs[0], fin.xsecs.last().unwrap(), axis);
    assert!((area - 28.2).abs() < 0.05, "A220 fin area {area:.2} m^2");
}
