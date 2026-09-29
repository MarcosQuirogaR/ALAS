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
