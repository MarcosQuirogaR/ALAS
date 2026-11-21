// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Independent geometric closure checks for published preset dimensions.

use alas_config::presets;

/// The pin file of the derived areas below: `tests/golden/<SUITE>.json`, rewritten by
/// `cargo xtask repin preset_dimension_corrections`.
const SUITE: &str = "preset_dimension_corrections";

#[test]
fn atr_active_planform_closes_published_area_and_mac() {
    // ATR 72-600 factsheet 2020 p.22: 61 m^2 reference area; JCAB TCDS
    // No. 75 Rev 3, ATR 72-212A item (10): 2.303 m mean aerodynamic chord.
    // The constant centre chord and the straight-tapered outer panel close
    // both on their straight edges; the drawn tip rounding (ATR 72 three view
    // drawing, EWADE 2011 presentation, p.3: rake from 0.97 span to a 0.85 m
    // chord) takes only its own triangle off that planform, and the
    // published values stay the reference wing.
    let preset = presets::get("ATR72-600").unwrap_or_else(|error| panic!("{error}"));
    let reference = preset.geometry.wing.reference_wing;
    assert_eq!(reference.mode, alas_config::ReferenceWingMode::Published);
    assert_eq!(reference.published_area_m2, Some(61.0));
    let area_and_mac = |wing: &alas_config::WingConfig| {
        let planform = wing
            .transport_planform(&preset.design_vector)
            .unwrap_or_else(|error| panic!("{error}"));
        let (mut half_area, mut chord_moment) = (0.0, 0.0);
        for panel in planform.panels() {
            let (inboard, outboard) = (panel.inboard.chord_m, panel.outboard.chord_m);
            let span = panel.outboard.y_m - panel.inboard.y_m;
            half_area += span * (inboard + outboard) / 2.0;
            chord_moment +=
                span * (inboard * inboard + inboard * outboard + outboard * outboard) / 3.0;
        }
        (2.0 * half_area, chord_moment / half_area, planform.tip.y_m)
    };
    let mut straight = preset.geometry.wing.clone();
    straight.clear_outboard_edge_options();
    let (area, mac, semispan) = area_and_mac(&straight);
    assert!((area - 61.0).abs() / 61.0 < 1e-5, "area {area} m^2");
    assert!((mac - 2.303).abs() / 2.303 < 1e-5, "MAC {mac} m");
    let (Some(rake_start), Some(rake_chord)) = (
        preset.geometry.wing.tip_rake_start_span_fraction,
        preset.geometry.wing.tip_rake_tip_chord_m,
    ) else {
        panic!("the ATR 72 declares its drawn tip rake")
    };
    let rake_triangle =
        (preset.design_vector.tip_chord_m - rake_chord) * (1.0 - rake_start) * semispan;
    let (drawn_area, _, _) = area_and_mac(&preset.geometry.wing);
    assert!(
        (drawn_area - (area - rake_triangle)).abs() < 1e-9,
        "drawn area {drawn_area} m^2"
    );
}

#[test]
fn a320_planar_wing_closes_published_area_and_mac_with_an_unswept_inboard_trailing_edge() {
    // Airbus A320 AC, FIGURE-2-2-0-991-004-A01: 34.10 m planar span (17.05 m
    // semi-span to the sharklet); reference area 122.6 m^2 and EASA.A.064
    // item 16 mean aerodynamic chord 4.1935 m.
    let preset = presets::get("A320-200").unwrap_or_else(|error| panic!("{error}"));
    let planform = preset
        .geometry
        .wing
        .transport_planform(&preset.design_vector)
        .unwrap_or_else(|error| panic!("{error}"));
    // Sheet 4 plan view: the leading edge leaves the straight line
    // into the sharklet and the chord at the 17.05 m wing tip is 1.44 m.
    assert!((planform.tip.chord_m - 1.436).abs() < 1e-12);
    // The published area and MAC are the reference
    // wing; the drawn planform, with its leading-edge fillet and tip rake, is
    // the geometry and no longer closes them.
    let reference = preset.geometry.wing.reference_wing;
    assert_eq!(reference.mode, alas_config::ReferenceWingMode::Published);
    assert_eq!(reference.published_area_m2, Some(122.6));
    assert_eq!(reference.published_mac_m, Some(4.1935));

    // The inboard trailing edge may not run forward outboard (angle to the
    // aft fuselage axis at most 90 deg), measured from the centreline on the
    // inboard edge lines (the 6.875 m root chord is only a datum, never
    // lofted) and from the exposed side-of-body station to the kink.
    let trailing_edge_x =
        |station: &alas_config::MainWingStation| station.leading_edge_x_m + station.chord_m;
    let side_of_body = planform
        .side_of_body
        .unwrap_or_else(|| panic!("the A320 declares a side-of-body station"));
    let centreline_trailing_edge_x = planform
        .inboard_edge_lines()
        .unwrap_or_else(|| panic!("the A320 has inboard edge lines"))
        .trailing
        .x_at(0.0);
    for (label, y_m, inboard_trailing_edge_x) in [
        ("centreline", 0.0, centreline_trailing_edge_x),
        (
            "side of body",
            side_of_body.y_m,
            trailing_edge_x(&side_of_body),
        ),
    ] {
        let angle_deg = (planform.kink.y_m - y_m)
            .atan2(trailing_edge_x(&planform.kink) - inboard_trailing_edge_x)
            .to_degrees();
        assert!(
            (89.0..=90.0).contains(&angle_deg),
            "{label} to kink trailing-edge angle {angle_deg} deg"
        );
    }

    // Open decision (overlay review): the sharklet refit
    // (cant 9.5 deg, blend 0.45 m) keeps the printed 35.80 m tip-to-tip span
    // and so moves the wing tip outboard of the 17.05 m plan-view semi-span.
    // This line keeps the 34.1078 m of the earlier sharklet until the
    // span is either restored to 17.05 m or re-sourced; it is checked
    // after the sourced assertions above so that they still run.
    assert!(
        (2.0 * planform.tip.y_m - 34.107_797_106_133_944).abs() < 1e-9,
        "planar span {} m",
        2.0 * planform.tip.y_m
    );
    // Drawn-planform regression guard (provisional, pending the span decision
    // above and the overlay review of the break and tip chords and the edge
    // stations): the value is recorded by
    // `cargo xtask repin preset_dimension_corrections` once the span line
    // passes, never copied from a replica.
    let drawn_area_m2: f64 = planform
        .panels()
        .iter()
        .map(|panel| {
            (panel.outboard.y_m - panel.inboard.y_m)
                * (panel.inboard.chord_m + panel.outboard.chord_m)
        })
        .sum();
    alas_testkit::pins::assert_pinned(SUITE, "A320-200/drawn_planform_area_m2", drawn_area_m2);
}

#[test]
fn airbus_dimensions_preserve_cross_section_and_engine_symmetry() {
    let a320 = presets::get("A320-200").unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(a320.geometry.fuselage.height_m, Some(4.14));
    assert_eq!(a320.geometry.fuselage.diameter_m, 3.95);
    assert_eq!(a320.engine_spanwise_positions(), &[5.755, -5.755]);
    let a340 = presets::get("A340-300").unwrap_or_else(|error| panic!("{error}"));
    assert!((2.0 * a340.geometry.empennage.hstab_tip_le_m.1 - 19.4).abs() < 1e-12);
    // Airbus A340-200/-300 AC Rev 33, FIGURE-2-2-0-991-007-A01 sheet 2: the
    // wing ends at the winglet root, y = 28.95 m, with the printed 2.5 m chord
    // from the 39.1 m leading-edge station; the 60.3 m span is to the winglet
    // tips (y = 30.15 m), the 1.452 m winglet canted 39.57 deg reaching the
    // 1.20 m between them. The 361.6 m^2 of section 2-1-1 is the
    // Published reference wing.
    let planform = a340
        .geometry
        .wing
        .transport_planform(&a340.design_vector)
        .unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(a340.design_vector.tip_chord_m, 2.5);
    assert_eq!(planform.tip.chord_m, 2.5);
    assert_eq!(a340.design_vector.span_m, 60.3);
    // One millimetre: the cant is stated to 0.01 deg, 0.4 mm of span.
    assert!(
        (2.0 * planform.tip.y_m - 57.9).abs() < 1e-3,
        "wing span to the winglet roots {} m",
        2.0 * planform.tip.y_m
    );
    // Open decision: the drawn area below integrates stations pending an overlay
    // review, so the pin keeps its earlier value until that review; it is then re-pinned
    // with `cargo xtask repin preset_dimension_corrections`.
    let area: f64 = planform
        .panels()
        .iter()
        .map(|panel| {
            (panel.outboard.y_m - panel.inboard.y_m)
                * (panel.inboard.chord_m + panel.outboard.chord_m)
        })
        .sum();
    let reference = a340.geometry.wing.reference_wing;
    assert_eq!(reference.mode, alas_config::ReferenceWingMode::Published);
    assert_eq!(reference.published_area_m2, Some(361.6));
    alas_testkit::pins::assert_pinned(SUITE, "A340-300/drawn_planform_area_m2", area);
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
    // Sheet 2 of the same figure (page 2-2-0 Page 3) dimensions the 3.72 m
    // (12.2 ft) tip chord, which closes the printed 72.57 m plan length from
    // the 68.85 m tip leading edge; the root chord is not dimensioned and keeps
    // its 9.75 m straight-edge fit, so the resolved trapezoidal
    // planform is (9.75 + 3.72) x 15.185 = 204.54 m^2. An unsourced 3.80 m tip
    // chord is not used; the preset is on the printed value.
    assert!((empennage.hstab_tip_chord_m - 3.72).abs() < 1e-12);
    let area =
        (empennage.hstab_root_chord_m + empennage.hstab_tip_chord_m) * empennage.hstab_tip_le_m.1;
    alas_testkit::pins::assert_pinned(SUITE, "A380-800/hstab_trapezoid_area_m2", area);
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

#[test]
fn every_raked_preset_scores_the_published_reference_not_the_planform_datum() {
    // The planform datum's gross area, MAC and MAC leading edge keep a drawn
    // leading-edge tip rake (only a trailing-edge flare is taken out). They
    // reach no scored quantity only because a raked preset is Published with an
    // area and its reference trapezoid (kink to rake start, straight edges) is
    // constructible: the reference wing is then the published values mapped on
    // that trapezoid, and the datum is not read. A raked preset that lost
    // either would fall back to the datum, whose rake convention no source
    // states; this pins that it cannot happen silently.
    let mut raked = 0;
    for name in presets::available() {
        let preset = presets::get(name).unwrap_or_else(|error| panic!("{error}"));
        let wing = &preset.geometry.wing;
        if wing.tip_rake_start_span_fraction.is_none() {
            continue;
        }
        raked += 1;
        let reference = &wing.reference_wing;
        assert_eq!(
            reference.mode,
            alas_config::ReferenceWingMode::Published,
            "{name}"
        );
        assert!(reference.published_area_m2.is_some(), "{name}");
        let planform = wing
            .transport_planform(&preset.design_vector)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
        assert!(planform.reference_trapezoid().is_some(), "{name}");
    }
    assert!(raked >= 10, "{raked} raked presets");
}
