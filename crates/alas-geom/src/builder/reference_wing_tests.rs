// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The reference-wing convention and the config-driven nacelle model.

use super::*;
use alas_config::{presets, NacelleModel, ReferenceWingConfig, ReferenceWingMode};

fn build(
    name: &str,
    tweak: impl FnOnce(&mut GeometryConfig),
) -> (Airplane, BuildReport, TransportPlanform) {
    let preset = presets::get(name).expect("preset exists");
    let mut geometry = preset.geometry.clone();
    tweak(&mut geometry);
    let planform = geometry
        .wing
        .transport_planform(&preset.design_vector)
        .expect("planform");
    let (airplane, report) = AircraftBuilder::new(Some(geometry))
        .build_reported(Some(&preset.design_vector), true, WingShape::Flight)
        .expect("builds");
    (airplane, report, planform)
}

fn trapezoid(config: &mut GeometryConfig) {
    config.wing.reference_wing.mode = ReferenceWingMode::Trapezoid;
    // The B747-400 preset carries its published 525 m2 (-1.6 % from the trapezoid, warning on);
    // this test checks the construction itself, so the published check is switched off here.
    config.wing.reference_wing.published_area_m2 = None;
}

fn drawn(config: &mut GeometryConfig) {
    config.wing.reference_wing = Default::default();
}

#[test]
fn trapezoid_mode_quotes_the_outboard_panel_extended_to_the_centreline() {
    let (drawn, _, planform) = build("B747-400", drawn);
    let (reference, report, _) = build("B747-400", trapezoid);
    let t = planform.reference_trapezoid().expect("physical");

    assert!((reference.s_ref - t.area_m2).abs() < 1e-9 * t.area_m2);
    assert!((reference.c_ref - t.mac_m).abs() < 1e-12);
    assert!((reference.b_ref - drawn.b_ref).abs() < 1e-9);
    assert_eq!(reference.wings[0].reference_area(), reference.s_ref);
    // The inboard extension is dropped: the reference wing is smaller than
    // the drawn planform when the drawn inboard trailing edge bulges out.
    assert_ne!(reference.s_ref, drawn.s_ref);
    // The drawn wing itself (lofted sections, projected area) is untouched.
    assert_eq!(reference.wings[0].xsecs, drawn.wings[0].xsecs);
    assert_eq!(
        reference.wings[0].projected_area(),
        drawn.wings[0].projected_area()
    );
    assert_eq!(
        reference.wings[0].unfolded_area(),
        drawn.wings[0].unfolded_area()
    );
    // %MAC datum: the trapezoid's MAC leading edge in the aircraft frame.
    let preset = presets::get("B747-400").unwrap();
    let x_wing = preset.geometry.wing.root_datum_x_m + preset.design_vector.wing_x_shift_m;
    let frame = reference.mac_frame().expect("frame");
    assert!((frame.x_lemac_m - (x_wing + t.mac_leading_edge_x_m)).abs() < 1e-12);
    assert_eq!(frame.chord_m, reference.c_ref);
    assert!(!report.has_warning("reference_wing_published_area_mismatch"));
}

#[test]
fn drawn_mode_and_unset_config_build_identical_airplanes() {
    let (unset, _, _) = build("A320-200", |g| g.wing.reference_wing = Default::default());
    let (explicit, _, _) = build("A320-200", |g| {
        g.wing.reference_wing = ReferenceWingConfig {
            mode: ReferenceWingMode::Drawn,
            ..Default::default()
        };
    });
    assert_eq!(unset, explicit);
    assert_eq!(format!("{unset:?}"), format!("{explicit:?}"));
    assert!(unset.wings[0].reference.is_none());
}

#[test]
fn published_area_is_checked_to_one_percent() {
    let (_, _, planform) = build("A320-200", |_| {});
    let area = planform.reference_trapezoid().expect("physical").area_m2;
    let with_published = |value: f64| {
        build("A320-200", |g| {
            g.wing.reference_wing = ReferenceWingConfig {
                mode: ReferenceWingMode::Trapezoid,
                published_area_m2: Some(value),
                ..Default::default()
            };
        })
        .1
    };
    assert!(!with_published(area * 1.005).has_warning("reference_wing_published_area_mismatch"));
    assert!(with_published(area * 1.03).has_warning("reference_wing_published_area_mismatch"));
    assert!(with_published(-1.0).has_warning("reference_wing_published_area_invalid"));
}

#[test]
fn the_reference_compatibility_contract_ignores_the_reference_wing() {
    let preset = presets::get("A320-200").unwrap();
    let mut geometry = preset.geometry.clone();
    trapezoid(&mut geometry);
    let airplane = AircraftBuilder::new_reference_compatibility(Some(geometry))
        .build(Some(&preset.design_vector), false)
        .expect("builds");
    assert!(airplane.wings[0].reference.is_none());
}

#[test]
fn nacelle_model_in_the_config_selects_the_v2_revolve_and_the_builder_overrides_it() {
    let preset = presets::get("A320-200").unwrap();
    let hash = |model: NacelleModel, forced: Option<bool>| {
        let mut geometry = preset.geometry.clone();
        geometry.engine.nacelle_model = model;
        let mut builder = AircraftBuilder::new(Some(geometry));
        if let Some(on) = forced {
            builder = builder.with_nacelle_v2(on);
        }
        let airplane = builder
            .build(Some(&preset.design_vector), true)
            .expect("builds");
        format!("{airplane:?}")
    };
    let legacy = hash(NacelleModel::Legacy, None);
    let v2 = hash(NacelleModel::V2, None);
    assert_ne!(legacy, v2, "A320 engine carries V2 data");
    assert_eq!(v2, hash(NacelleModel::Legacy, Some(true)));
    assert_eq!(legacy, hash(NacelleModel::V2, Some(false)));
}

#[test]
fn published_mode_quotes_the_manufacturer_values_and_keeps_the_drawn_geometry() {
    let (drawn_plane, _, planform) = build("A340-300", drawn);
    let (published, report, _) = build("A340-300", |_| {});
    let config = presets::get("A340-300")
        .unwrap()
        .geometry
        .wing
        .reference_wing;
    assert_eq!(config.mode, ReferenceWingMode::Published);
    assert_eq!(published.s_ref, 361.6);
    assert_eq!(published.c_ref, 7.270);
    assert_eq!(published.mac_frame().expect("frame").x_lemac_m, 28.083);
    // Span and every drawn quantity are the drawn planform's.
    assert!((published.b_ref - drawn_plane.b_ref).abs() < 1e-12);
    assert_eq!(published.wings[0].xsecs, drawn_plane.wings[0].xsecs);
    assert_eq!(
        published.wings[0].projected_area(),
        drawn_plane.wings[0].projected_area()
    );
    assert!(published.wings[0].projected_area() > published.s_ref);
    assert!(!report.has_warning("reference_wing_published_area_invalid"));

    // A MAC or MAC station that is not published comes from the reference trapezoid.
    let t = planform.reference_trapezoid().expect("physical");
    let (partial, _, _) = build("A340-300", |g| {
        g.wing.reference_wing.published_mac_m = None;
        g.wing.reference_wing.published_mac_le_x_m = None;
    });
    assert_eq!(partial.s_ref, 361.6);
    assert!((partial.c_ref - t.mac_m).abs() < 1e-12);

    // Published without an area is a build warning, never a panic.
    let (_, report, _) = build("A340-300", |g| {
        g.wing.reference_wing.published_area_m2 = None
    });
    assert!(report.has_warning("reference_wing_published_area_invalid"));
}
