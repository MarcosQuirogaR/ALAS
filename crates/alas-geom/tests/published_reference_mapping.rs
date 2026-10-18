// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The `Published` reference wing is a convention mapping onto the built
//! trapezoid: exact at the registered geometry, consistent elsewhere
//! (`alas_config::ReferenceWingConfig::published_reference`).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use alas_config::{presets, DesignVector, ReferenceWingMode, WingShape};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::builder::AircraftBuilder;

fn built(name: &str, edit: impl FnOnce(&mut DesignVector)) -> Airplane {
    let preset = presets::get(name).expect("preset");
    let mut dv = preset.design_vector;
    edit(&mut dv);
    AircraftBuilder::new(Some(preset.geometry.clone()))
        .build_reported(Some(&dv), true, WingShape::Flight)
        .expect("builds")
        .0
}

fn published_presets() -> Vec<&'static alas_config::AircraftPreset> {
    presets::registry()
        .iter()
        .filter(|p| p.geometry.wing.reference_wing.mode == ReferenceWingMode::Published)
        .collect()
}

#[test]
fn the_registered_geometry_reproduces_every_published_value_exactly() {
    let all = published_presets();
    assert_eq!(all.len(), 11);
    for p in all {
        let r = p.geometry.wing.reference_wing;
        assert!(r.registered.is_some(), "{} was not registered", p.name);
        let plane = built(p.name, |_| {});
        assert_eq!(
            plane.s_ref,
            r.published_area_m2.unwrap(),
            "{} S_ref",
            p.name
        );
        let frame = plane.mac_frame().unwrap();
        if let Some(mac) = r.published_mac_m {
            assert_eq!(plane.c_ref, mac, "{} c_ref", p.name);
        }
        if let Some(x) = r.published_mac_le_x_m {
            assert_eq!(frame.x_lemac_m, x, "{} LEMAC", p.name);
        }
    }
}

#[test]
fn a_scaled_candidate_scales_the_reference_with_the_built_trapezoid() {
    for p in published_presets() {
        let r = p.geometry.wing.reference_wing;
        let basis = r.registered.unwrap();
        let mut dv = p.design_vector;
        dv.span_m *= 1.05;
        dv.root_chord_m *= 1.10;
        dv.tip_chord_m *= 0.95;
        let plane = built(p.name, |d| *d = dv);
        let t = p
            .geometry
            .wing
            .transport_planform(&dv)
            .unwrap()
            .reference_trapezoid()
            .unwrap();
        let s = r.published_area_m2.unwrap() * t.area_m2 / basis.area_m2;
        assert!((plane.s_ref - s).abs() < 1e-9 * s, "{} S_ref", p.name);
        assert!(
            (plane.s_ref - r.published_area_m2.unwrap()).abs() > 1e-3,
            "{}: the reference area must follow the candidate wing",
            p.name
        );
        if let Some(mac) = r.published_mac_m {
            let c = mac * t.mac_m / basis.mac_m;
            assert!((plane.c_ref - c).abs() < 1e-9 * c, "{} c_ref", p.name);
        }
        // Span growth alone scales S_ref by the trapezoid ratio (about +5 %).
        let wider = built(p.name, |d| d.span_m *= 1.05);
        let ratio = wider.s_ref / built(p.name, |_| {}).s_ref;
        assert!(ratio > 1.03 && ratio < 1.07, "{}: {ratio}", p.name);
    }
}

#[test]
fn the_lemac_follows_the_wing_station_shift() {
    for p in published_presets() {
        let base = built(p.name, |_| {}).mac_frame().unwrap().x_lemac_m;
        let moved = built(p.name, |d| d.wing_x_shift_m += 1.0)
            .mac_frame()
            .unwrap()
            .x_lemac_m;
        assert!(
            (moved - base - 1.0).abs() < 1e-9,
            "{}: {}",
            p.name,
            moved - base
        );
    }
}

#[test]
fn the_published_lemac_stands_on_the_drawn_wing_within_the_reading_error() {
    // Model frame: m aft of the nose tip. TCDS stations are measured from a
    // datum ahead of the nose (A320 2.540 m, A340 6.382 m, B787 1.41732 m,
    // A400M 4.820 m) and were converted before they were stored.
    // The drawn MAC leading edge differs only by the Yehudi extension that
    // the published MAC does not carry (quarter-MAC agreement below).
    for name in ["A320-200", "A400M"] {
        let p = presets::get(name).unwrap();
        let drawn = {
            let mut g = p.geometry.clone();
            g.wing.reference_wing = Default::default();
            AircraftBuilder::new(Some(g))
                .build_reported(Some(&p.design_vector), true, WingShape::Flight)
                .unwrap()
                .0
        };
        let le = built(name, |_| {}).mac_frame().unwrap().x_lemac_m;
        assert!(
            (le - drawn.mac_frame().unwrap().x_lemac_m).abs() < 0.02,
            "{name}: {le}"
        );
    }
}
