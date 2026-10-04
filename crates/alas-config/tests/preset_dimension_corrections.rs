// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Independent geometric closure checks for published preset dimensions.

use alas_config::presets;

#[test]
fn atr_active_planform_closes_published_area_and_mac() {
    // ATR 72-600 factsheet 2020 p.22: 61 m^2 reference area; JCAB TCDS
    // No. 75 Rev 3, ATR 72-212A item (10): 2.303 m mean aerodynamic chord.
    let preset = presets::get("ATR72-600").unwrap_or_else(|error| panic!("{error}"));
    let planform = preset
        .geometry
        .wing
        .transport_planform(&preset.design_vector)
        .unwrap_or_else(|error| panic!("{error}"));
    let (mut half_area, mut chord_moment) = (0.0, 0.0);
    for panel in planform.panels() {
        let (inboard, outboard) = (panel.inboard.chord_m, panel.outboard.chord_m);
        let span = panel.outboard.y_m - panel.inboard.y_m;
        half_area += span * (inboard + outboard) / 2.0;
        chord_moment += span * (inboard * inboard + inboard * outboard + outboard * outboard) / 3.0;
    }
    let area = 2.0 * half_area;
    let mac = chord_moment / half_area;
    assert!((area - 61.0).abs() / 61.0 < 1e-5, "area {area} m^2");
    assert!((mac - 2.303).abs() / 2.303 < 1e-5, "MAC {mac} m");
}

#[test]
fn a320_planar_wing_closes_published_area_and_mac_with_an_unswept_inboard_trailing_edge() {
    // Airbus A320 AC Jun 01/24, FIGURE-2-2-0-991-004-A01: 34.10 m planar
    // span and the 1.64 m chord of the 16.29 m aileron-end station, the
    // equivalent-trapezoid tip chord; reference area 122.6 m^2 and
    // EASA.A.064 item 16 mean aerodynamic chord 4.1935 m.
    let preset = presets::get("A320-200").unwrap_or_else(|error| panic!("{error}"));
    let planform = preset
        .geometry
        .wing
        .transport_planform(&preset.design_vector)
        .unwrap_or_else(|error| panic!("{error}"));
    assert!((2.0 * planform.tip.y_m - 34.10).abs() < 1e-12);
    assert!((planform.tip.chord_m - 1.64).abs() < 1e-12);
    let (mut half_area, mut chord_moment) = (0.0, 0.0);
    for panel in planform.panels() {
        let (inboard, outboard) = (panel.inboard.chord_m, panel.outboard.chord_m);
        let span = panel.outboard.y_m - panel.inboard.y_m;
        half_area += span * (inboard + outboard) / 2.0;
        chord_moment += span * (inboard * inboard + inboard * outboard + outboard * outboard) / 3.0;
    }
    let area = 2.0 * half_area;
    let mac = chord_moment / half_area;
    assert!((area - 122.6).abs() / 122.6 < 5e-4, "area {area} m^2");
    assert!((mac - 4.1935).abs() / 4.1935 < 5e-4, "MAC {mac} m");

    // The inboard trailing edge may not run forward outboard (angle to the
    // aft fuselage axis at most 90 deg), measured from the centreline root
    // and from the exposed side-of-body station to the kink.
    let trailing_edge_x =
        |station: &alas_config::MainWingStation| station.leading_edge_x_m + station.chord_m;
    let side_of_body = planform
        .side_of_body
        .unwrap_or_else(|| panic!("the A320 declares a side-of-body station"));
    for inboard in [planform.root, side_of_body] {
        let angle_deg = (planform.kink.y_m - inboard.y_m)
            .atan2(trailing_edge_x(&planform.kink) - trailing_edge_x(&inboard))
            .to_degrees();
        assert!(
            (89.0..=90.0).contains(&angle_deg),
            "{:?} to kink trailing-edge angle {angle_deg} deg",
            inboard.kind
        );
    }
}

#[test]
fn airbus_dimensions_preserve_cross_section_and_engine_symmetry() {
    let a320 = presets::get("A320-200").unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(a320.geometry.fuselage.height_m, Some(4.14));
    assert_eq!(a320.geometry.fuselage.diameter_m, 3.95);
    assert_eq!(a320.engine_spanwise_positions(), &[5.755, -5.755]);
    let a340 = presets::get("A340-300").unwrap_or_else(|error| panic!("{error}"));
    assert!((2.0 * a340.geometry.empennage.hstab_tip_le_m.1 - 19.4).abs() < 1e-12);
    // Airbus A340-200/-300 AC Rev 33, FIGURE-2-2-0-991-007-A01 sheet 2: 2.5 m
    // streamwise tip chord, held with the 60.3 m span and the 361.6 m^2
    // reference area of section 2-1-1 by the kink station alone.
    let planform = a340
        .geometry
        .wing
        .transport_planform(&a340.design_vector)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(planform.tip.chord_m, 2.5);
    assert_eq!(2.0 * planform.tip.y_m, 60.3);
    let area: f64 = planform
        .panels()
        .iter()
        .map(|panel| {
            (panel.outboard.y_m - panel.inboard.y_m)
                * (panel.inboard.chord_m + panel.outboard.chord_m)
        })
        .sum();
    assert!((area - 361.6).abs() < 1e-10, "{area}");
}

#[test]
fn the_a380_tailplane_spans_its_published_dimension() {
    // Airbus A380 Aircraft Characteristics - Airport and Maintenance Planning,
    // Revision 20 Dec 01/25, Subject 2-2-0 General Aircraft Dimensions,
    // FIGURE-2-2-0-991-001-A01 Sheet 1 of 2, page 2-2-0 Page 2: tailplane span
    // 30.37 m (99.64 ft), read from the front elevation where the dimension's
    // extension lines terminate on the tailplane tips.
    let a380 = presets::get("A380-800").unwrap_or_else(|error| panic!("{error}"));
    let empennage = &a380.geometry.empennage;
    assert!((2.0 * empennage.hstab_tip_le_m.1 - 30.37).abs() < 1e-12);
    // The same figure carries the wing span and fuselage width this preset
    // already matches, which is what makes the tailplane reading of the third
    // symmetric dimension on that view unambiguous.
    assert!((a380.design_vector.span_m - 79.75).abs() < 1e-12);
    assert!((a380.geometry.fuselage.diameter_m - 7.14).abs() < 1e-12);
    // Sheet 2 of the same figure dimensions the 3.72 m tip chord; the root
    // chord is not dimensioned and keeps its 9.0 m estimate, so the resolved
    // trapezoidal planform is 193.15 m^2.
    let area =
        (empennage.hstab_root_chord_m + empennage.hstab_tip_chord_m) * empennage.hstab_tip_le_m.1;
    assert!((area - 193.1532).abs() < 1e-4, "{area}");
}

#[test]
fn the_787_9_nacelle_inlet_sits_at_its_published_station() {
    // Boeing 787 ACAP D6-58333 Rev Q, October 2025, section 2.2.2 (General
    // Dimensions: Model 787-9) plan view: engine centreline 32 ft 6 in
    // (9.91 m) from the airplane centreline, nacelle inlet face 68 ft 3 in
    // (20.80 m) aft of the nose tip. The builder places the inlet at the root
    // leading edge plus the planform leading edge at the engine station, less
    // the inlet offset.
    let preset = presets::get("B787-9").unwrap_or_else(|error| panic!("{error}"));
    let engine = &preset.geometry.engine;
    assert_eq!(engine.spanwise_positions_m, [9.91, -9.91]);
    let leading_edge_x_m = preset
        .geometry
        .wing
        .transport_planform(&preset.design_vector)
        .and_then(|planform| planform.leading_edge_x_at(9.91))
        .unwrap_or_else(|error| panic!("{error}"));
    let inlet_x_m = preset.geometry.wing.root_datum_x_m
        + preset.design_vector.wing_x_shift_m
        + leading_edge_x_m
        - engine.inlet_x_offset_m;
    // Half a centimetre: the drawing prints the station to the centimetre.
    assert!(
        (inlet_x_m - 20.80).abs() < 0.005,
        "inlet face at {inlet_x_m} m"
    );
}
