// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// A test asserts on values it constructed here directly, so a failed unwrap
// or expect is the assertion failing, not a library invariant being broken.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::{ConfigNode, DesignVector, Entry, OptionSource};

#[test]
fn the_default_wing_washes_out_from_root_to_tip() {
    // Washout is what keeps the tip from stalling before the root, which
    // is what keeps the ailerons working through the stall.
    let wing = WingConfig::default();
    assert!(wing.root_twist_deg > wing.break_twist_deg);
}

#[test]
fn the_default_wing_has_positive_dihedral() {
    let wing = WingConfig::default();
    assert!(wing.tip_z_m > wing.root_z_m);
}

#[test]
fn the_break_sits_strictly_between_the_root_and_the_tip() {
    // At 0 or 1 the crank collapses onto a defining section and the
    // outboard sweep decrement has nothing to apply to.
    let wing = WingConfig::default();
    assert!(wing.break_span_fraction > 0.0);
    assert!(wing.break_span_fraction < 1.0);
}

#[test]
fn the_product_defaults_derive_a_collinear_side_of_body_station() {
    let wing = WingConfig::default();
    let design = DesignVector::default();
    let planform = wing
        .transport_planform(&design)
        .expect("the default planform is physically valid");

    assert!(planform.side_of_body.is_some());
    assert_eq!(planform.kink.span_fraction, 0.37);
    assert_eq!(planform.stations().len(), 4);
    assert_eq!(planform.panels().len(), 3);
    assert_eq!(planform.inboard_le_sweep_deg, design.sweep_deg);
    assert_eq!(planform.outboard_le_sweep_deg, design.sweep_deg);
    let side_of_body = planform.side_of_body.expect("the station is active");
    let fraction = side_of_body.y_m / planform.kink.y_m;
    let expected_chord =
        planform.root.chord_m + fraction * (planform.kink.chord_m - planform.root.chord_m);
    assert!((side_of_body.chord_m - expected_chord).abs() < 1e-12);
}

#[test]
fn legacy_saved_wing_configuration_uses_the_pre_transport_planform() {
    let mut value = serde_json::to_value(WingConfig::default()).unwrap();
    let object = value
        .as_object_mut()
        .expect("a wing configuration serializes as an object");
    for field in [
        "side_of_body_span_fraction",
        "side_of_body_chord_ratio",
        "kink_span_fraction",
        "outboard_le_sweep_deg",
    ] {
        object.remove(field);
    }
    let legacy: WingConfig = serde_json::from_value(value)
        .expect("new optional transport fields do not invalidate saved configurations");
    let design = DesignVector::default();
    let planform = legacy
        .transport_planform(&design)
        .expect("the legacy planform remains valid");

    assert!(planform.side_of_body.is_none());
    assert_eq!(planform.kink.span_fraction, legacy.break_span_fraction);
    assert_eq!(
        planform.outboard_le_sweep_deg,
        design.sweep_deg - legacy.outboard_sweep_decrement_deg
    );
}

#[test]
fn explicit_transport_stations_cannot_create_a_second_leading_edge_sweep() {
    let wing = WingConfig {
        side_of_body_span_fraction: Some(0.10),
        side_of_body_chord_ratio: Some(0.90),
        kink_span_fraction: Some(0.40),
        outboard_le_sweep_deg: Some(28.0),
        ..WingConfig::default()
    };
    let design = DesignVector::default();

    let planform = wing
        .transport_planform(&design)
        .expect("the configured transport planform is valid");
    let panels = planform.panels();

    assert_eq!(planform.stations().len(), 4);
    assert_eq!(panels.len(), 3);
    assert_eq!(planform.kink.span_fraction, 0.40);
    assert_eq!(planform.kink.y_m, 0.40 * design.span_m / 2.0);
    assert!((panels[0].leading_edge_sweep_deg - design.sweep_deg).abs() < 1e-12);
    assert!((panels[2].leading_edge_sweep_deg - design.sweep_deg).abs() < 1e-12);
    assert!(panels[1].trailing_edge_sweep_deg < panels[1].leading_edge_sweep_deg);
    assert!(panels[2].trailing_edge_sweep_deg < panels[2].leading_edge_sweep_deg);
}

#[test]
fn inboard_aerodynamic_station_uses_side_of_body_chord_and_interpolated_twist() {
    let wing = WingConfig::default();
    let design = DesignVector::default();
    let planform = wing
        .transport_planform(&design)
        .expect("the default transport planform is valid");
    let side_of_body = planform
        .side_of_body
        .expect("product default has a side-of-body station");
    let station = wing
        .inboard_aerodynamic_station(&design)
        .expect("the default inboard station is valid");
    let expected_twist_deg = wing.root_twist_deg
        + (side_of_body.y_m / planform.kink.y_m) * (wing.break_twist_deg - wing.root_twist_deg);

    assert_eq!(station.y_m, side_of_body.y_m);
    assert_eq!(station.chord_m, side_of_body.chord_m);
    assert!((station.twist_deg - expected_twist_deg).abs() < 1e-12);
}

#[test]
fn legacy_inboard_aerodynamic_station_remains_at_the_centerline_root() {
    let wing = WingConfig {
        side_of_body_span_fraction: None,
        side_of_body_chord_ratio: None,
        ..WingConfig::default()
    };
    let design = DesignVector::default();
    let station = wing
        .inboard_aerodynamic_station(&design)
        .expect("the legacy inboard station is valid");

    assert_eq!(station.y_m, 0.0);
    assert_eq!(station.chord_m, design.root_chord_m);
    assert_eq!(station.twist_deg, wing.root_twist_deg);
}

#[test]
fn a_reverse_taper_from_root_to_kink_is_rejected() {
    let wing = WingConfig::default();
    let design = DesignVector {
        break_chord_m: 17.0,
        sweep_deg: 40.0,
        ..DesignVector::default()
    };

    assert!(matches!(
        wing.transport_planform(&design),
        Err(TransportPlanformError::NonMonotoneChord { .. })
    ));
}

#[test]
fn a_side_of_body_chord_may_exceed_a_root_chord_that_is_only_a_datum() {
    let pinned = WingConfig {
        side_of_body_span_fraction: Some(0.12),
        side_of_body_chord_ratio: Some(1.2),
        ..WingConfig::default()
    };
    let design = DesignVector::default();
    assert!(pinned.root_chord_is_only_a_datum());
    let planform = pinned
        .transport_planform(&design)
        .expect("the datum is not a chord of the wing");
    let side_of_body = planform.side_of_body.expect("side-of-body station");
    assert!(side_of_body.chord_m > planform.root.chord_m);
    pinned
        .validate_custom_sections_against_planform(&planform)
        .expect("no chord of the wing grows outboard");

    // The rest of the order still holds: the kink may not stand above the side of body.
    let reversed = DesignVector {
        break_chord_m: 1.21 * design.root_chord_m,
        ..design
    };
    assert!(matches!(
        pinned.transport_planform(&reversed),
        Err(TransportPlanformError::NonMonotoneChord { .. })
    ));

    // A derived side-of-body chord leaves the root chord a chord of the wing.
    let derived = WingConfig {
        side_of_body_chord_ratio: None,
        ..pinned.clone()
    };
    assert!(!derived.root_chord_is_only_a_datum());

    // A section drawn at the side of body is refused at build, and changes nothing here.
    let drawn_inboard = WingConfig {
        custom_sections: vec![WingSection {
            span_fraction: 0.12,
            leading_edge_x_m: 2.0,
            chord_m: 1.2 * design.root_chord_m,
            z_m: 0.0,
            twist_deg: 0.0,
            airfoil: "sc20610".to_owned(),
        }],
        ..pinned
    };
    assert!(drawn_inboard.root_chord_is_only_a_datum());
    assert!(matches!(
        drawn_inboard.validate_custom_sections_outboard_of_side_of_body(),
        Err(WingSectionError::AtOrInboardOfSideOfBody { index: 0, .. })
    ));
}

#[test]
fn a_forward_swept_trailing_edge_is_a_valid_tapered_wing() {
    let wing = WingConfig {
        side_of_body_chord_ratio: Some(1.0),
        ..WingConfig::default()
    };

    assert!(wing.transport_planform(&DesignVector::default()).is_ok());
}

#[test]
fn a_planform_query_outboard_of_the_tip_is_a_typed_error() {
    let planform = WingConfig::default()
        .transport_planform(&DesignVector::default())
        .expect("the default planform is valid");

    assert!(matches!(
        planform.leading_edge_x_at(planform.tip.y_m + 0.01),
        Err(TransportPlanformError::SpanwisePositionOutsidePlanform { .. })
    ));
}

#[test]
fn both_section_fields_offer_the_airfoil_library_and_still_accept_a_naca_code() {
    // The geometry layer resolves any NACA 4-digit code without the
    // library carrying it, so a strict list would reject valid input.
    for name in ["root_airfoil", "tip_airfoil"] {
        let schema = WingConfig::default().schema();
        let Entry::Leaf(leaf) = &schema.field(name).unwrap().entry else {
            panic!("{name} is not a group");
        };
        assert_eq!(leaf.options, Some(OptionSource::Airfoil));
    }
    assert!(OptionSource::Airfoil.editable());
}

#[test]
fn a_wing_without_a_tip_rise_has_one_shape_on_the_ground_and_in_flight() {
    let wing = WingConfig::default();
    let planform = wing.transport_planform(&DesignVector::default()).unwrap();
    assert_eq!(
        wing.heights(WingShape::Flight, &planform).unwrap(),
        wing.heights(WingShape::Ground, &planform).unwrap()
    );
}

#[test]
fn the_flight_shape_adds_one_dihedral_increment_from_the_centreline() {
    // The declared tip rise over the semispan, the same at every station:
    // the root does not move, the tip rises by the declared fraction of the
    // semispan, and every station in between by its own span fraction of it.
    let wing = WingConfig {
        flight_tip_rise_semispan_fraction: Some(0.1),
        ..WingConfig::default()
    };
    let planform = wing.transport_planform(&DesignVector::default()).unwrap();
    let ground = wing.heights(WingShape::Ground, &planform).unwrap();
    let flight = wing.heights(WingShape::Flight, &planform).unwrap();
    let semi_span = planform.tip.y_m;

    assert_eq!(flight.root_z_m, ground.root_z_m);
    assert!((flight.tip_z_m - ground.tip_z_m - 0.1 * semi_span).abs() < 1e-12);
    for k in 0..=40 {
        let y = semi_span * f64::from(k) / 40.0;
        let rise = flight.at(&planform, y) - ground.at(&planform, y);
        assert!((rise - 0.1 * y).abs() < 1e-9, "rise {rise} at y = {y}");
    }
}

#[test]
fn every_preset_wing_rises_from_the_centreline_and_only_grows_outboard() {
    // The rise is one wing bending, not the difference of two fits: none at the
    // centreline, where the box sits in the body, and never falling outboard.
    for preset in crate::presets::registry() {
        let wing = &preset.geometry.wing;
        let Ok(planform) = wing.transport_planform(&preset.design_vector) else {
            continue;
        };
        let ground = wing.heights(WingShape::Ground, &planform).unwrap();
        let flight = wing.heights(WingShape::Flight, &planform).unwrap();
        let mut previous = 0.0;
        for k in 0..=80 {
            let y = planform.tip.y_m * f64::from(k) / 80.0;
            let rise = flight.at(&planform, y) - ground.at(&planform, y);
            assert!(
                rise >= previous - 1e-3,
                "{}: rise {rise} m at y = {y} m after {previous} m",
                preset.name
            );
            previous = rise;
        }
        assert!(
            (flight.root_z_m - ground.root_z_m).abs() < 1e-12,
            "{}",
            preset.name
        );
    }
}

#[test]
fn a_sagging_or_out_of_range_flight_tip_rise_is_rejected() {
    let planform = WingConfig::default()
        .transport_planform(&DesignVector::default())
        .unwrap();
    for fraction in [
        -0.01,
        MAX_FLIGHT_TIP_RISE_SEMISPAN_FRACTION + 0.01,
        f64::NAN,
    ] {
        let wing = WingConfig {
            flight_tip_rise_semispan_fraction: Some(fraction),
            ..WingConfig::default()
        };
        assert!(
            wing.heights(WingShape::Flight, &planform).is_err(),
            "{fraction} accepted"
        );
    }
}

#[test]
fn a_declared_inboard_sweep_cranks_the_leading_edge_at_the_kink_only() {
    let design = DesignVector::default();
    let plain = WingConfig::default().transport_planform(&design).unwrap();
    let wing = WingConfig {
        inboard_le_sweep_deg: Some(design.sweep_deg + 3.0),
        ..WingConfig::default()
    };
    let cranked = wing.transport_planform(&design).unwrap();
    assert_eq!(cranked.inboard_le_sweep_deg, design.sweep_deg + 3.0);
    assert_eq!(cranked.outboard_le_sweep_deg, design.sweep_deg);
    let tan = |deg: f64| deg.to_radians().tan();
    assert!(
        (cranked.kink.leading_edge_x_m - cranked.kink.y_m * tan(design.sweep_deg + 3.0)).abs()
            < 1e-12
    );
    let side = cranked.side_of_body.unwrap();
    assert!((side.leading_edge_x_m - side.y_m * tan(design.sweep_deg + 3.0)).abs() < 1e-12);
    // outboard of the kink the leading edge keeps the design sweep
    let panels = cranked.panels();
    assert!((panels.last().unwrap().leading_edge_sweep_deg - design.sweep_deg).abs() < 1e-9);
    // unset: bit-identical to the single-sweep planform
    let unset = WingConfig {
        inboard_le_sweep_deg: None,
        ..WingConfig::default()
    };
    assert_eq!(unset.transport_planform(&design).unwrap(), plain);
}

#[test]
fn an_outboard_trailing_edge_break_moves_only_the_trailing_edge() {
    let design = DesignVector::default();
    let plain = WingConfig::default().transport_planform(&design).unwrap();
    let wing = WingConfig {
        te_break_span_fraction: Some(0.6),
        te_break_chord_offset_m: Some(0.2),
        ..WingConfig::default()
    };
    let broken = wing.transport_planform(&design).unwrap();
    let station = broken.te_breaks[0];
    assert_eq!(broken.te_breaks.len(), 1);
    assert_eq!(station.kind, MainWingStationKind::TeBreak);
    assert!((station.span_fraction - 0.6).abs() < 1e-12);
    // the leading edge is the straight outboard line, the chord the straight taper plus the offset
    assert!(
        (station.leading_edge_x_m - plain.leading_edge_x_at(station.y_m).unwrap()).abs() < 1e-9
    );
    let straight_chord = plain.kink.chord_m
        + (station.y_m - plain.kink.y_m) / (plain.tip.y_m - plain.kink.y_m)
            * (plain.tip.chord_m - plain.kink.chord_m);
    assert!((station.chord_m - straight_chord - 0.2).abs() < 1e-9);
    assert!(broken.reference_area_m2() > plain.reference_area_m2());
    assert_eq!(broken.outboard_stations(), vec![station]);
    // a break span without a break fraction is no break: bit-identical to the plain planform
    let unset = WingConfig {
        te_break_span_fraction: None,
        te_break_chord_offset_m: Some(0.2),
        ..WingConfig::default()
    };
    assert_eq!(unset.transport_planform(&design).unwrap(), plain);
}

#[test]
fn two_trailing_edge_breaks_draw_a_polyline_and_a_break_may_stand_above_the_kink_chord() {
    let design = DesignVector::default();
    let plain = WingConfig::default().transport_planform(&design).unwrap();
    let wing = WingConfig {
        te_break_span_fraction: Some(0.45),
        // a bulge larger than the chord drop between the kink and the break: the break chord stands
        // above the kink chord (a drawn trailing edge can bulge aft of the kink)
        te_break_chord_offset_m: Some(0.9 * plain.kink.chord_m),
        te_break_2_span_fraction: Some(0.7),
        te_break_2_chord_offset_m: Some(-0.1),
        ..WingConfig::default()
    };
    let broken = wing.transport_planform(&design).unwrap();
    assert_eq!(broken.te_breaks.len(), 2);
    assert!(broken.te_breaks[0].y_m < broken.te_breaks[1].y_m);
    assert!(broken.te_breaks[0].chord_m > broken.kink.chord_m);
    let ys: Vec<f64> = broken.outboard_stations().iter().map(|s| s.y_m).collect();
    assert!(ys.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn trailing_edge_breaks_must_lie_outboard_of_the_kink_in_order_with_a_positive_chord() {
    let design = DesignVector::default();
    let at = |first: f64, second: Option<f64>, offset_m: f64| {
        WingConfig {
            te_break_span_fraction: Some(first),
            te_break_chord_offset_m: Some(offset_m),
            te_break_2_span_fraction: second,
            te_break_2_chord_offset_m: second.map(|_| 0.0),
            ..WingConfig::default()
        }
        .transport_planform(&design)
    };
    assert!(
        matches!(
            at(0.3, None, 0.0),
            Err(TransportPlanformError::InvalidTrailingEdgeBreak { .. })
        ),
        "inboard of the kink"
    );
    assert!(at(1.0, None, 0.0).is_err(), "at the tip");
    assert!(
        at(0.6, Some(0.5), 0.0).is_err(),
        "second break inboard of the first"
    );
    assert!(at(0.5, Some(0.6), 0.0).is_ok());
    assert!(at(0.5, None, -50.0).is_err(), "non-positive chord");
}

#[test]
fn section_stations_are_validated_and_lofted_like_the_profile_stations() {
    let with = |stations: &[(f64, &str)]| WingConfig {
        section_stations: stations
            .iter()
            .map(|&(span_fraction, airfoil)| WingSectionStation {
                span_fraction,
                airfoil: airfoil.to_owned(),
            })
            .collect(),
        ..WingConfig::default()
    };
    let kink = 0.35;
    let named = with(&[(0.2, "naca0012"), (0.6, "naca4412")]);
    assert!(named.validate_profiles(kink).is_ok());
    assert_eq!(named.profile_span_fractions(), vec![0.2, 0.6]);
    assert!(
        with(&[(kink, "naca0012")]).validate_profiles(kink).is_ok(),
        "a station may sit on the kink"
    );
    assert!(matches!(
        with(&[(0.6, "naca0012"), (0.2, "naca4412")]).validate_profiles(kink),
        Err(WingProfileError::InvalidOrder {
            list: "section",
            ..
        })
    ));
    assert!(matches!(
        with(&[(0.34, "naca0012")]).validate_profiles(kink),
        Err(WingProfileError::TooCloseToStation {
            list: "section",
            anchor: "kink",
            ..
        })
    ));
    for outside in [0.0, 1.0, f64::NAN] {
        assert!(
            with(&[(outside, "naca0012")])
                .validate_profiles(kink)
                .is_err(),
            "{outside}"
        );
    }
}

#[test]
fn a_station_just_off_the_side_of_body_is_rejected_whether_or_not_its_chord_is_pinned() {
    let kink = 0.35;
    for chord_ratio in [None, Some(0.9)] {
        let with = |span_fraction: f64| WingConfig {
            side_of_body_span_fraction: Some(0.12),
            side_of_body_chord_ratio: chord_ratio,
            thickness_stations: vec![WingThicknessStation {
                span_fraction,
                thickness_scale: 1.0,
            }],
            ..WingConfig::default()
        };
        assert!(
            matches!(
                with(0.13).validate_profiles(kink),
                Err(WingProfileError::TooCloseToStation {
                    list: "thickness",
                    anchor: "side-of-body",
                    ..
                })
            ),
            "{chord_ratio:?}"
        );
        assert!(
            with(0.12).validate_profiles(kink).is_ok(),
            "on the side of body"
        );
        assert!(with(0.20).validate_profiles(kink).is_ok(), "clear of it");
    }
}

#[test]
fn a_smaller_candidate_tip_chord_keeps_the_swept_back_tip_and_chords_falling_outboard() {
    // The rake is a feature of the tip: the registry anchors each drawn rake chord to
    // the preset's own design tip chord, and the rake keeps that ratio on any other
    // candidate. A fixed vector or an optimiser point with a smaller tip chord (the
    // recorded A320-200 Phase 4 winner has 1.2794 m, below the drawn 1.336 m rake
    // chord) keeps a leading edge swept aft of its straight line and chords falling
    // outboard, and every preset's own vector builds the drawn rake chord bit for bit.
    let mut raked = Vec::new();
    for preset in crate::presets::registry() {
        let wing = &preset.geometry.wing;
        let design = preset.design_vector;
        let Some(rake_chord_m) = wing.tip_rake_tip_chord_m else {
            continue;
        };
        raked.push(preset.name);
        let reference_m = wing
            .tip_rake_reference_tip_chord_m
            .unwrap_or_else(|| panic!("{}: a drawn rake without its reference", preset.name));
        assert_eq!(
            reference_m.to_bits(),
            design.tip_chord_m.to_bits(),
            "{}",
            preset.name
        );
        // The edge stations are absolute offsets from the lines: leave them out to
        // read the rake itself, down to a tip chord equal to the drawn rake chord.
        let mut lines = wing.clone();
        lines.edge_stations.clear();
        let own = lines
            .transport_planform(&design)
            .unwrap_or_else(|error| panic!("{}: {error}", preset.name));
        assert_eq!(
            own.tip.chord_m.to_bits(),
            rake_chord_m.to_bits(),
            "{}",
            preset.name
        );
        for factor in [0.95, 0.9, 0.8, rake_chord_m / design.tip_chord_m] {
            let candidate = DesignVector {
                tip_chord_m: factor * design.tip_chord_m,
                ..design
            };
            let label = format!("{} at a {} m tip chord", preset.name, candidate.tip_chord_m);
            let planform = lines
                .transport_planform(&candidate)
                .unwrap_or_else(|error| panic!("{label}: {error}"));
            let (kink, tip) = (planform.kink, planform.tip);
            let start = planform
                .tip_rake
                .unwrap_or_else(|| panic!("{label}: no rake"));
            let expected_m = rake_chord_m * candidate.tip_chord_m / reference_m;
            assert!(
                (tip.chord_m - expected_m).abs() <= 1e-12 * expected_m,
                "{label}"
            );
            assert!(tip.chord_m < candidate.tip_chord_m, "{label}");
            // The extreme-tip leading edge lies aft of the straight leading edge through
            // the kink and the rake start: a forward rake.
            let straight_le_x_m = kink.leading_edge_x_m
                + (tip.y_m - kink.y_m) * (start.leading_edge_x_m - kink.leading_edge_x_m)
                    / (start.y_m - kink.y_m);
            assert!(tip.leading_edge_x_m > straight_le_x_m, "{label}");
            let outboard: Vec<f64> = planform
                .stations_with_rake()
                .iter()
                .filter(|station| {
                    station.y_m >= kink.y_m && station.kind != MainWingStationKind::TeBreak
                })
                .map(|station| station.chord_m)
                .collect();
            assert!(
                outboard.windows(2).all(|pair| pair[1] < pair[0] + 1.0e-9),
                "{label}: {outboard:?}"
            );
            // Inside the reference-adaptation box (tip chord within 10 %) the drawn
            // wing, edge stations and all, builds too.
            if factor >= 0.9 {
                wing.transport_planform(&candidate)
                    .unwrap_or_else(|error| panic!("{label} with its edge stations: {error}"));
            }
        }
    }
    assert!(raked.contains(&"A320-200"), "raked presets: {raked:?}");
}

#[test]
fn an_absolute_rake_chord_at_or_above_the_candidate_tip_chord_is_refused_by_name() {
    // Without a reference tip chord the rake chord is absolute: a candidate tip chord at
    // or below it has no swept-back tip, and a reference at or below the rake chord
    // describes none either. Both are refused by name, never built as a leading edge
    // running forward of its straight line.
    let preset = crate::presets::get("A320-200").expect("A320-200 is registered");
    let design = preset.design_vector;
    let mut absolute = preset.geometry.wing.clone();
    absolute.edge_stations.clear();
    absolute.tip_rake_reference_tip_chord_m = None;
    let rake_chord_m = absolute
        .tip_rake_tip_chord_m
        .expect("A320-200 draws a tip rake");
    for tip_chord_m in [rake_chord_m, 0.9 * rake_chord_m] {
        assert!(
            matches!(
                absolute.transport_planform(&DesignVector {
                    tip_chord_m,
                    ..design
                }),
                Err(TransportPlanformError::TipRakeChordNotBelowTip { .. })
            ),
            "{tip_chord_m} m"
        );
    }
    let between = DesignVector {
        tip_chord_m: 0.5 * (rake_chord_m + design.tip_chord_m),
        ..design
    };
    let planform = absolute
        .transport_planform(&between)
        .expect("a tip chord above the rake chord builds");
    assert_eq!(planform.tip.chord_m.to_bits(), rake_chord_m.to_bits());
    absolute.tip_rake_reference_tip_chord_m = Some(0.9 * rake_chord_m);
    assert!(matches!(
        absolute.transport_planform(&design),
        Err(TransportPlanformError::TipRakeChordNotBelowTip { .. })
    ));
}
