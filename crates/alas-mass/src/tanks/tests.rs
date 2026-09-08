// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Unit and integration tests for the fuel-tank layout.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap or expect there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{
    AlasConfig, CenterTankConfig, DesignVector, FuelPolicyConfig, FuelTankLayoutConfig,
    GeometryConfig, StructuresConfig, WingTankConfig,
};
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::{Wing, WingXSec};
use alas_geom::builder::AircraftBuilder;

use super::{CapacitySource, FuelState, FuelTank, FuelTankLayout, TankKind, TankSide};

const DENSITY_KG_M3: f64 = 800.0;

/// A constant-chord, constant-airfoil symmetric wing spanning `semispan_m`,
/// whose spar-box volume has a closed form from the same thickness samples
/// [`crate::tanks::geometry::section_box`] reads.
fn rectangular_wing(chord_m: f64, semispan_m: f64) -> Wing {
    let airfoil = Airfoil::from_name("naca0012").expect("naca0012 is a valid 4-digit NACA name");
    let xsecs = vec![
        WingXSec::new([0.0, 0.0, 0.0], chord_m, 0.0, airfoil.clone()),
        WingXSec::new([0.0, semispan_m, 0.0], chord_m, 0.0, airfoil),
    ];
    Wing::new("Main Wing", xsecs, true)
}

fn rectangular_airplane(chord_m: f64, semispan_m: f64) -> Airplane {
    Airplane {
        name: "Test Aircraft".to_owned(),
        xyz_ref: [0.0, 0.0, 0.0],
        wings: vec![rectangular_wing(chord_m, semispan_m)],
        fuselages: vec![],
        s_ref: chord_m * semispan_m * 2.0,
        c_ref: chord_m,
        b_ref: semispan_m * 2.0,
    }
}

/// A hand-built tank for the loading and burning tests, which exercise
/// [`FuelTankLayout`] without needing a resolved geometry.
fn synthetic_tank(id: &str, usable_capacity_kg: f64, burn_priority: i64, x_m: f64) -> FuelTank {
    FuelTank {
        id: id.to_owned(),
        kind: TankKind::WingInner,
        side: TankSide::Centerline,
        geometric_volume_m3: usable_capacity_kg / DENSITY_KG_M3,
        usable_volume_m3: usable_capacity_kg / DENSITY_KG_M3,
        usable_capacity_kg,
        unusable_kg: 0.0,
        centroid_m: [x_m, 0.0, 0.0],
        extent_m: [1.0, 1.0, 1.0],
        burn_priority,
        capacity_source: CapacitySource::Geometric,
    }
}

/// `center` (burn first), `inner` and `outer` (burn last), the layout every
/// loading and burning test shares.
fn three_tank_layout() -> FuelTankLayout {
    FuelTankLayout {
        tanks: vec![
            synthetic_tank("center", 500.0, 1, 10.0),
            synthetic_tank("wing_inner", 1_000.0, 2, 20.0),
            synthetic_tank("wing_outer", 1_500.0, 4, 30.0),
        ],
        geometric_calibration_factor: 1.0,
        density_kg_m3: DENSITY_KG_M3,
    }
}

#[test]
fn a_rectangular_wing_yields_the_analytic_wingbox_volume_and_mirrored_centroids() {
    let chord_m = 3.0;
    let semispan_m = 12.0;
    let plane = rectangular_airplane(chord_m, semispan_m);
    let geometry = GeometryConfig::default();
    let structures = StructuresConfig::default();
    let config = FuelTankLayoutConfig {
        inner_wing: WingTankConfig {
            enabled: true,
            span_start_fraction: 0.0,
            span_end_fraction: 1.0,
            usable_fraction: 1.0,
            burn_priority: 2,
            published_usable_volume_l: None,
        },
        center: CenterTankConfig {
            enabled: false,
            ..Default::default()
        },
        ..Default::default()
    };
    let policy = FuelPolicyConfig::default();

    let layout = FuelTankLayout::resolve(
        &plane,
        &geometry,
        &structures,
        &config,
        &policy,
        DENSITY_KG_M3,
        None,
    )
    .expect("a rectangular wing with a valid spar box resolves");

    // The independently computed thickness integral between the default
    // 0.25c/0.70c spars, over the same samples `section_box` reads.
    let airfoil = Airfoil::from_name("naca0012").expect("naca0012 is a valid 4-digit NACA name");
    let samples = linspace(0.25, 0.70, 41);
    let thickness = airfoil.local_thickness(&samples);
    let area_fraction: f64 = thickness
        .windows(2)
        .zip(samples.windows(2))
        .map(|(pair, x_pair)| (pair[0] + pair[1]) * (x_pair[1] - x_pair[0]) / 2.0)
        .sum();
    let expected_volume_m3 = area_fraction * chord_m * chord_m * semispan_m;

    let right = layout
        .tanks()
        .iter()
        .find(|tank| tank.id == "wing_inner_right")
        .expect("a symmetric wing produces a right-side inner tank");
    let left = layout
        .tanks()
        .iter()
        .find(|tank| tank.id == "wing_inner_left")
        .expect("a symmetric wing produces a left-side inner tank");

    let relative_error =
        (right.geometric_volume_m3 - expected_volume_m3).abs() / expected_volume_m3;
    assert!(relative_error < 1.0e-9, "relative error {relative_error}");
    assert!(
        (left.geometric_volume_m3 - right.geometric_volume_m3).abs() < 1.0e-9 * expected_volume_m3
    );
    assert!((left.centroid_m[1] + right.centroid_m[1]).abs() < 1.0e-9);
    assert!((right.centroid_m[1] - semispan_m / 2.0).abs() < 1.0e-9);
}

#[test]
fn distribute_sums_exactly_and_fills_the_last_burned_tank_first() {
    let layout = three_tank_layout();
    let state = layout
        .distribute(1_800.0)
        .expect("1800 kg fits the 3000 kg layout");
    assert!((state.total_kg() - 1_800.0).abs() < 1.0e-9);

    let mass_of = |id: &str| -> f64 {
        state
            .mass_items(&layout)
            .iter()
            .find(|item| item.id == id)
            .map_or(0.0, |item| item.mass_kg)
    };
    // Burn priority 4 (outer) is burned last, so it is filled to capacity
    // first; only the remainder reaches priority 2 (inner), and priority 1
    // (centre, burned first) gets nothing at this load.
    assert!((mass_of("wing_outer") - 1_500.0).abs() < 1.0e-9);
    assert!((mass_of("wing_inner") - 300.0).abs() < 1.0e-9);
    assert_eq!(mass_of("center"), 0.0);
}

/// A mirrored left/right pair at the same burn priority, the shape every
/// resolved wing tank comes in ([`super::resolve::wing_tank_pair`]).
fn symmetric_pair_layout() -> FuelTankLayout {
    let right = FuelTank {
        centroid_m: [20.0, 6.0, 0.0],
        ..synthetic_tank("wing_inner_right", 1_000.0, 2, 20.0)
    };
    let left = FuelTank {
        centroid_m: [20.0, -6.0, 0.0],
        ..synthetic_tank("wing_inner_left", 1_000.0, 2, 20.0)
    };
    FuelTankLayout {
        tanks: vec![right, left],
        geometric_calibration_factor: 1.0,
        density_kg_m3: DENSITY_KG_M3,
    }
}

#[test]
fn a_partial_load_splits_evenly_between_tanks_tied_on_burn_priority() {
    let layout = symmetric_pair_layout();
    let state = layout
        .distribute(600.0)
        .expect("600 kg fits the 2000 kg symmetric layout");
    let mass_of = |id: &str| -> f64 {
        state
            .mass_items(&layout)
            .iter()
            .find(|item| item.id == id)
            .map_or(0.0, |item| item.mass_kg)
    };
    // Sequential fill-by-index would put all 600 kg in whichever tank is
    // listed first and none in the other; a symmetric aircraft with no
    // declared asymmetric load must instead keep the lateral CG at y=0.
    assert!((mass_of("wing_inner_right") - 300.0).abs() < 1.0e-9);
    assert!((mass_of("wing_inner_left") - 300.0).abs() < 1.0e-9);
    let cg_y = state.properties(&layout).cg_m[1];
    assert!(cg_y.abs() < 1.0e-9, "lateral CG {cg_y} should be zero");
}

#[test]
fn draining_a_tied_pair_keeps_it_balanced_at_every_step() {
    let layout = symmetric_pair_layout();
    let full = layout
        .distribute(layout.usable_capacity_kg())
        .expect("a full load fits its own capacity");
    let after = full
        .burned(&layout, 700.0)
        .expect("700 kg is less than the 2000 kg on board");
    let mass_of = |state: &FuelState, id: &str| -> f64 {
        state
            .mass_items(&layout)
            .iter()
            .find(|item| item.id == id)
            .map_or(0.0, |item| item.mass_kg)
    };
    assert!((mass_of(&after, "wing_inner_right") - 650.0).abs() < 1.0e-9);
    assert!((mass_of(&after, "wing_inner_left") - 650.0).abs() < 1.0e-9);
    let cg_y = after.properties(&layout).cg_m[1];
    assert!(cg_y.abs() < 1.0e-9, "lateral CG {cg_y} should be zero");
}

#[test]
fn burning_removes_from_the_centre_tank_before_the_wing_tanks() {
    let layout = three_tank_layout();
    let full = layout
        .distribute(layout.usable_capacity_kg())
        .expect("a full load fits its own capacity");
    let after = full
        .burned(&layout, 700.0)
        .expect("700 kg is less than the 3000 kg on board");

    let mass_of = |state: &super::FuelState, id: &str| {
        state
            .mass_items(&layout)
            .iter()
            .find(|item| item.id == id)
            .map_or(0.0, |item| item.mass_kg)
    };
    assert_eq!(mass_of(&after, "center"), 0.0);
    assert!((mass_of(&after, "wing_inner") - 800.0).abs() < 1.0e-9);
    assert!((mass_of(&after, "wing_outer") - 1_500.0).abs() < 1.0e-9);
    assert!((after.total_kg() - 2_300.0).abs() < 1.0e-9);
}

#[test]
fn overflowing_the_layout_reports_the_exact_excess() {
    let layout = three_tank_layout();
    let capacity_kg = layout.usable_capacity_kg();
    let error = layout
        .distribute(capacity_kg + 100.0)
        .expect_err("100 kg over capacity must be rejected");
    match error {
        super::TankLayoutError::Overflow { excess_kg } => {
            assert!((excess_kg - 100.0).abs() < 1.0e-9);
        }
        other => panic!("expected Overflow, got {other:?}"),
    }

    let negative = layout.distribute(-1.0);
    assert!(matches!(
        negative,
        Err(super::TankLayoutError::InvalidFuelMass { .. })
    ));
    let not_a_number = layout.distribute(f64::NAN);
    assert!(matches!(
        not_a_number,
        Err(super::TankLayoutError::InvalidFuelMass { .. })
    ));
}

#[test]
fn calibration_reproduces_the_published_total_and_leaves_published_tanks_alone() {
    let chord_m = 3.0;
    let semispan_m = 12.0;
    let plane = rectangular_airplane(chord_m, semispan_m);
    let geometry = GeometryConfig::default();
    let structures = StructuresConfig::default();
    let config = FuelTankLayoutConfig {
        inner_wing: WingTankConfig {
            enabled: true,
            span_start_fraction: 0.0,
            span_end_fraction: 0.5,
            usable_fraction: 1.0,
            burn_priority: 2,
            published_usable_volume_l: Some(2_000.0),
        },
        outer_wing: WingTankConfig {
            enabled: true,
            span_start_fraction: 0.5,
            span_end_fraction: 0.9,
            usable_fraction: 1.0,
            burn_priority: 4,
            published_usable_volume_l: None,
        },
        center: CenterTankConfig {
            enabled: false,
            ..Default::default()
        },
        calibrate_to_published_capacity: true,
        ..Default::default()
    };
    let policy = FuelPolicyConfig::default();
    let published_total_l = 6_000.0;

    let layout = FuelTankLayout::resolve(
        &plane,
        &geometry,
        &structures,
        &config,
        &policy,
        DENSITY_KG_M3,
        Some(published_total_l),
    )
    .expect("a published total with a positive geometric remainder calibrates");

    let total_after_m3: f64 = layout
        .tanks()
        .iter()
        .map(|tank| tank.usable_volume_m3)
        .sum();
    assert!((total_after_m3 - published_total_l * 1.0e-3).abs() < 1.0e-9);

    for tank in layout.tanks() {
        if tank.id.starts_with("wing_inner") {
            assert_eq!(tank.capacity_source, CapacitySource::Published);
            assert!((tank.usable_volume_m3 - 1.0).abs() < 1.0e-12);
        } else {
            assert_eq!(tank.capacity_source, CapacitySource::GeometricCalibrated);
        }
    }
    assert!(layout.geometric_calibration_factor.is_finite());
    assert!(layout.geometric_calibration_factor > 0.0);
}

#[test]
fn a_layout_of_published_cells_calibrates_to_the_identity() {
    // Every registered preset declares a published volume on every cell, so
    // there is no geometric volume for the published total to rescale; the
    // cells stand as declared and the factor is one, not an error.
    let plane = rectangular_airplane(3.0, 12.0);
    let geometry = GeometryConfig::default();
    let structures = StructuresConfig::default();
    let config = FuelTankLayoutConfig {
        inner_wing: WingTankConfig {
            enabled: true,
            span_start_fraction: 0.0,
            span_end_fraction: 0.5,
            usable_fraction: 1.0,
            burn_priority: 2,
            published_usable_volume_l: Some(2_000.0),
        },
        outer_wing: WingTankConfig {
            enabled: true,
            span_start_fraction: 0.5,
            span_end_fraction: 0.9,
            usable_fraction: 1.0,
            burn_priority: 4,
            published_usable_volume_l: Some(500.0),
        },
        center: CenterTankConfig {
            enabled: false,
            ..Default::default()
        },
        calibrate_to_published_capacity: true,
        ..Default::default()
    };
    let layout = FuelTankLayout::resolve(
        &plane,
        &geometry,
        &structures,
        &config,
        &FuelPolicyConfig::default(),
        DENSITY_KG_M3,
        Some(2_500.0),
    )
    .expect("published cells need no calibration");
    assert_eq!(layout.geometric_calibration_factor, 1.0);
    let total_m3: f64 = layout
        .tanks()
        .iter()
        .map(|tank| tank.usable_volume_m3)
        .sum();
    assert!((total_m3 - 2.5).abs() < 1.0e-9);
    assert!(layout
        .tanks()
        .iter()
        .all(|tank| tank.capacity_source == CapacitySource::Published));
}

#[test]
fn a_scaled_resolution_grows_a_published_cell_with_the_candidate_spar_box() {
    // The reference wing carries a published inner cell; a candidate with
    // twice the chord has four times the spar-box volume (section area goes
    // with chord squared), so the published cell must scale by four.
    let reference = rectangular_airplane(3.0, 12.0);
    let candidate = rectangular_airplane(6.0, 12.0);
    let geometry = GeometryConfig::default();
    let structures = StructuresConfig::default();
    let config = FuelTankLayoutConfig {
        inner_wing: WingTankConfig {
            enabled: true,
            span_start_fraction: 0.0,
            span_end_fraction: 0.5,
            usable_fraction: 1.0,
            burn_priority: 2,
            published_usable_volume_l: Some(2_000.0),
        },
        outer_wing: WingTankConfig {
            enabled: false,
            ..Default::default()
        },
        center: CenterTankConfig {
            enabled: false,
            ..Default::default()
        },
        calibrate_to_published_capacity: true,
        ..Default::default()
    };
    let policy = FuelPolicyConfig::default();
    let scaled = FuelTankLayout::resolve_scaled(
        &candidate,
        &reference,
        &geometry,
        &structures,
        &config,
        &policy,
        DENSITY_KG_M3,
        Some(2_000.0),
    )
    .expect("the published cell scales with the candidate spar box");
    let wing_m3: f64 = scaled
        .tanks()
        .iter()
        .filter(|tank| tank.id.starts_with("wing_inner"))
        .map(|tank| tank.usable_volume_m3)
        .sum();
    assert!(
        (wing_m3 - 8.0).abs() < 1.0e-9,
        "scaled wing volume {wing_m3} m3"
    );
    assert!(scaled
        .tanks()
        .iter()
        .filter(|tank| tank.id.starts_with("wing_inner"))
        .all(|tank| tank.capacity_source == CapacitySource::GeometricCalibrated));

    // On the reference itself the scaled resolution reproduces the published
    // cell exactly.
    let same = FuelTankLayout::resolve_scaled(
        &reference,
        &reference,
        &geometry,
        &structures,
        &config,
        &policy,
        DENSITY_KG_M3,
        Some(2_000.0),
    )
    .expect("the reference reproduces itself");
    let same_wing_m3: f64 = same
        .tanks()
        .iter()
        .filter(|tank| tank.id.starts_with("wing_inner"))
        .map(|tank| tank.usable_volume_m3)
        .sum();
    assert!((same_wing_m3 - 2.0).abs() < 1.0e-9);
}

#[test]
fn the_fuel_loading_curve_is_monotone_in_fuel_and_ends_at_the_full_centroid() {
    let layout = three_tank_layout();
    let curve = layout.fuel_cg_curve(6);
    assert_eq!(curve.len(), 6);
    for pair in curve.windows(2) {
        assert!(pair[1].fuel_kg > pair[0].fuel_kg);
    }

    let full_state = layout
        .distribute(layout.usable_capacity_kg())
        .expect("full load fits its own capacity");
    let expected_cg = full_state.properties(&layout).cg_m;
    let last = curve.last().expect("six steps produce a last point");
    assert!((last.fuel_kg - layout.usable_capacity_kg()).abs() < 1.0e-9);
    for (axis, expected) in expected_cg.iter().enumerate() {
        assert!((last.cg_m[axis] - expected).abs() < 1.0e-9);
    }
}

#[test]
fn the_default_aircraft_resolves_a_finite_layout_with_positive_capacity() {
    let config = AlasConfig::default();
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&DesignVector::default()), true)
        .expect("the default geometry builds");

    let layout = FuelTankLayout::resolve(
        &plane,
        &config.geometry,
        &config.structures,
        &config.fuel_tanks,
        &config.fuel_policy,
        config.mass_model.fuel_density_kg_m3,
        None,
    )
    .expect("the default aircraft resolves a fuel-tank layout");

    assert!(layout.usable_capacity_kg().is_finite());
    assert!(layout.usable_capacity_kg() > 0.0);
    assert!(layout.unusable_fuel_kg().is_finite());
    assert!(layout.unusable_fuel_kg() >= 0.0);
    for tank in layout.tanks() {
        assert!(tank.centroid_m.iter().all(|value| value.is_finite()));
        assert!(tank
            .extent_m
            .iter()
            .all(|value| value.is_finite() && *value > 0.0));
    }
}
