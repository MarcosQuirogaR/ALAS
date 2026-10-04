// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The structural model is built from the evaluated design vector, not from
//! the preset that happens to share the configuration: its planform (semi-span,
//! chords, kink station and leading-edge offsets) is the one the aerodynamic
//! wing is lofted from, and the finite-element deck spans the evaluated
//! semi-span. SI throughout: metres, kilograms, radians internally.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{materials, presets, AlasConfig, DesignVector};
use alas_geom::builder::AircraftBuilder;
use alas_geom::wing_structure::WingStructureGeometry;

/// A design whose span, chords and sweep all differ from the A320 preset, kept
/// within the planform's monotone-chord rule.
fn resized_a320() -> (AlasConfig, DesignVector) {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A320-200" })).unwrap();
    let preset = presets::get("A320-200").unwrap().design_vector;
    let design = DesignVector {
        span_m: 41.6,
        root_chord_m: 7.9,
        break_chord_m: 3.9,
        tip_chord_m: 1.6,
        sweep_deg: 25.0,
        ..preset
    };
    assert!((design.span_m - preset.span_m).abs() > 5.0);
    (config, design)
}

/// Kink, tip and root sections of the built aerodynamic main wing, as
/// `(x_le - x_root_le, y, chord)`.
fn aerodynamic_stations(config: &AlasConfig, design: &DesignVector) -> [(f64, f64, f64); 3] {
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(design), true)
        .unwrap();
    let wing = alas_mass::wing_reconciliation::main_wing(&plane).unwrap();
    let root = wing.xsecs.first().unwrap();
    let tip = wing.xsecs.last().unwrap();
    let kink = wing
        .xsecs
        .iter()
        .find(|xsec| (xsec.chord - design.break_chord_m).abs() < 1e-9 && xsec.xyz_le[1] > 0.0)
        .expect("the built wing carries the kink section at the design break chord");
    let station = |xsec: &alas_geom::aircraft::wing::WingXSec| {
        (xsec.xyz_le[0] - root.xyz_le[0], xsec.xyz_le[1], xsec.chord)
    };
    [station(root), station(kink), station(tip)]
}

fn structural_geometry(config: &AlasConfig, design: &DesignVector) -> WingStructureGeometry {
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(design), true)
        .unwrap();
    let wing = alas_mass::wing_reconciliation::main_wing(&plane).unwrap();
    let (fractions, full_span) = config.structures.resolved_spars();
    WingStructureGeometry::new(
        design,
        &config.geometry.wing,
        &wing.xsecs.first().unwrap().airfoil,
        &wing.xsecs.last().unwrap().airfoil,
        &fractions,
        Some(&full_span),
    )
    .unwrap()
}

#[test]
fn wing_structure_planform_is_the_evaluated_designs_aerodynamic_planform() {
    let (config, design) = resized_a320();
    let [root, kink, tip] = aerodynamic_stations(&config, &design);
    let geometry = structural_geometry(&config, &design);

    let tolerance = 1e-9;
    assert!((geometry.semi_span - design.span_m / 2.0).abs() < tolerance);
    assert!((geometry.semi_span - tip.1).abs() < tolerance);
    assert!((geometry.c_root - root.2).abs() < tolerance);
    assert!((geometry.c_break - kink.2).abs() < tolerance);
    assert!((geometry.c_tip - tip.2).abs() < tolerance);
    // Kink station and the leading-edge offsets that carry the sweep: an
    // explicit kink fraction and the transport outboard-sweep rule apply to
    // the A320 preset, which a break-fraction/decrement pair does not
    // reproduce.
    assert!((geometry.y_break - kink.1).abs() < tolerance, "kink y");
    assert!((geometry.dx_break - kink.0).abs() < tolerance, "kink x_le");
    assert!((geometry.dx_tip - tip.0).abs() < tolerance, "tip x_le");
    // The preset's own semi-span is not what the model was built with.
    let preset = presets::get("A320-200").unwrap().design_vector;
    assert!((geometry.semi_span - preset.span_m / 2.0).abs() > 2.0);
}

#[test]
fn nastran_deck_spans_the_evaluated_semi_span() {
    let (config, design) = resized_a320();
    let geometry = structural_geometry(&config, &design);
    let structures = &config.structures;
    let skin = materials::get(&structures.skin_material).unwrap();
    let web = materials::get(&structures.spar_web_material).unwrap();
    let cap = materials::get(&structures.spar_cap_material).unwrap();
    let rib = materials::get(&structures.rib_material).unwrap();
    let requirements = config.requirements.clone();
    let sizing = alas_struct::sizing::size_wingbox(
        &geometry,
        structures,
        &requirements,
        skin,
        web,
        cap,
        rib,
    );
    let (deck, _, _) = alas_struct::mesh::build_wing_mesh_bdf_product(
        &geometry,
        &sizing,
        structures,
        &config.geometry.engine,
        &config.mass_model,
        &requirements,
        skin,
        web,
        cap,
        rib,
    )
    .expect("the evaluated design meshes");

    let (mut y_max, mut nodes) = (0.0_f64, 0);
    for id in 1..=200_000 {
        if let Some(xyz) = deck.grid_xyz(id) {
            y_max = y_max.max(xyz[1]);
            nodes += 1;
        }
    }
    assert!(nodes > 100, "the deck has {nodes} grids");
    assert!(
        (y_max - design.span_m / 2.0).abs() < 1e-6,
        "deck semi-span {y_max} m against evaluated {} m",
        design.span_m / 2.0
    );
}

/// The rear spar of a transport planform keeps its nominal streamwise chord
/// fraction across the trailing-edge kink. The frozen perpendicular-rib-cut
/// construction drifted it to about 0.87 of the rib outboard of the kink on
/// this planform, collapsing the web to a third of its nominal depth, which is
/// what made the product flange bars unplaceable (`InvalidCapGeometry`).
#[test]
fn rear_spar_holds_its_chord_fraction_across_the_kink() {
    let (config, design) = resized_a320();
    let geometry = structural_geometry(&config, &design);
    let nominal = *geometry.spar_fracs.last().unwrap();
    let rear = geometry.spar_fracs.len() - 1;
    let mut worst = 0.0_f64;
    for step in 2..=40 {
        let eta = step as f64 / 40.0;
        let y = eta * geometry.semi_span;
        let x_le = geometry.x_le(eta);
        let (aft_x, aft_y) = geometry.rib_vector(eta);
        let (l_nominal, l_actual) = geometry.get_rib_lengths(y, x_le, aft_x, aft_y);
        let s = geometry.compute_spar_intersections(y, x_le, aft_x, aft_y, l_nominal)[rear]
            .expect("full-span spar exists at every station");
        let fraction = s / l_nominal;
        if fraction <= l_actual / l_nominal {
            worst = worst.max((fraction - nominal).abs());
        }
    }
    assert!(
        worst < 0.08,
        "rear spar strays {worst:.3} of the rib from its nominal {nominal} chord fraction"
    );
}
