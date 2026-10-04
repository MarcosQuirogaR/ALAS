// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Unit and integration tests for the fuel-tank layout.

// A test asserts on values it constructed or loaded from a fixture it
// controls, so a failed unwrap or expect there is the assertion failing.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{
    AlasConfig, CenterTankConfig, DesignVector, FeedTankConfig, FuelPolicyConfig,
    FuelTankLayoutConfig, GeometryConfig, StructuresConfig, WingTankConfig,
};
use alas_geom::aircraft::airfoil::Airfoil;
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::aircraft::wing::{Wing, WingXSec};
use alas_geom::builder::AircraftBuilder;

use super::{fuel_vector, CapacitySource, FuelState, FuelTank, FuelTankLayout, TankKind, TankSide};

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
        low_point_m: [x_m, 0.0, 0.0],
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
            feed: FeedTankConfig::default(),
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
            feed: FeedTankConfig::default(),
        },
        outer_wing: WingTankConfig {
            enabled: true,
            span_start_fraction: 0.5,
            span_end_fraction: 0.9,
            usable_fraction: 1.0,
            burn_priority: 4,
            published_usable_volume_l: None,
            feed: FeedTankConfig::default(),
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
            feed: FeedTankConfig::default(),
        },
        outer_wing: WingTankConfig {
            enabled: true,
            span_start_fraction: 0.5,
            span_end_fraction: 0.9,
            usable_fraction: 1.0,
            burn_priority: 4,
            published_usable_volume_l: Some(500.0),
            feed: FeedTankConfig::default(),
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
            feed: FeedTankConfig::default(),
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
        None,
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
        None,
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

/// A single tank whose `low_point_m` is forward of and below its full-tank
/// `centroid_m`, the partial-fill geometry (a swept, dihedral wing
/// cell's inboard boundary sits forward and below the cell's own centroid).
fn tank_with_low_point() -> FuelTankLayout {
    let mut tank = synthetic_tank("wing", 1_000.0, 1, 20.0);
    tank.low_point_m = [15.0, 0.0, -1.0];
    FuelTankLayout {
        tanks: vec![tank],
        geometric_calibration_factor: 1.0,
        density_kg_m3: DENSITY_KG_M3,
    }
}

#[test]
fn a_full_tank_sits_at_its_full_centroid_not_its_low_point() {
    let layout = tank_with_low_point();
    let full = layout
        .distribute(layout.usable_capacity_kg())
        .expect("full load fits its own capacity");
    let position = full.mass_items(&layout)[0].position_m;
    assert!((position[0] - 20.0).abs() < 1.0e-9);
    assert!((position[2] - 0.0).abs() < 1.0e-9);
}

#[test]
fn a_nearly_empty_tank_sits_at_its_low_point() {
    let layout = tank_with_low_point();
    // `mass_items` filters zero-fill tanks out entirely (there is no fuel
    // to place), so the fill-fraction-0 limit is approached, not reached
    // exactly, with a vanishing load.
    let nearly_empty = layout
        .distribute(1.0e-6)
        .expect("a trace load fits the tank");
    let position = nearly_empty.mass_items(&layout)[0].position_m;
    assert!((position[0] - 15.0).abs() < 1.0e-6);
    assert!((position[2] - (-1.0)).abs() < 1.0e-6);
}

#[test]
fn the_partial_fill_centroid_moves_monotonically_from_the_low_point_to_the_full_centroid() {
    let layout = tank_with_low_point();
    let levels_kg = [0.0, 100.0, 250.0, 500.0, 750.0, 1_000.0];
    let mut previous_x = f64::NEG_INFINITY;
    for &fuel_kg in &levels_kg {
        let state = layout.distribute(fuel_kg).expect("every level fits");
        let x = if fuel_kg > 0.0 {
            state.mass_items(&layout)[0].position_m[0]
        } else {
            // The empty state carries no item; its limit is the low point.
            15.0
        };
        assert!(
            x >= previous_x - 1.0e-9,
            "x did not increase monotonically with fill: {x} after {previous_x}"
        );
        assert!((15.0..=20.0).contains(&x));
        previous_x = x;
    }
    assert!((previous_x - 20.0).abs() < 1.0e-9);
}

#[test]
fn the_fuel_vector_ends_at_the_takeoff_centroid_and_at_empty_with_monotonic_mass() {
    let layout = three_tank_layout();
    let takeoff = layout
        .distribute(1_800.0)
        .expect("1800 kg fits the 3000 kg layout");
    let vector = fuel_vector(&layout, &takeoff, 7);
    assert_eq!(vector.len(), 7);

    let first = vector.first().expect("seven points produce a first one");
    let takeoff_cg = takeoff.properties(&layout).cg_m;
    assert!((first.fuel_kg - 1_800.0).abs() < 1.0e-9);
    assert!((first.x_m - takeoff_cg[0]).abs() < 1.0e-9);
    assert!((first.z_m - takeoff_cg[2]).abs() < 1.0e-9);

    let last = vector.last().expect("seven points produce a last one");
    assert!((last.fuel_kg - 0.0).abs() < 1.0e-9);

    for pair in vector.windows(2) {
        assert!(pair[1].fuel_kg <= pair[0].fuel_kg + 1.0e-9);
    }
}

#[test]
fn the_fuel_vector_is_empty_for_zero_points_and_a_single_point_is_the_empty_state() {
    let layout = three_tank_layout();
    let takeoff = layout
        .distribute(1_800.0)
        .expect("1800 kg fits the 3000 kg layout");
    assert!(fuel_vector(&layout, &takeoff, 0).is_empty());
    let single = fuel_vector(&layout, &takeoff, 1);
    assert_eq!(single.len(), 1);
    assert!((single[0].fuel_kg - 0.0).abs() < 1.0e-9);
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

fn fill_fraction(layout: &FuelTankLayout, state: &FuelState, tank: &FuelTank) -> f64 {
    fill_of(layout, state, &tank.id) / tank.usable_capacity_kg
}

/// The registered A380 ground fill against the only published A380 fuel
/// distribution in the corpus: the example load on the refuel/defuel panel,
/// Airbus A380 Aircraft Characteristics, Dec 01/25, FIGURE-5-4-6-991-001-A01.
/// At 180,800 kg on board the panel shows the outer tanks full (8,320 kg
/// each), the mid tanks at 28,040 kg (95.6 %), the inner tanks at 8,840 kg
/// (23.8 %, the least-filled wing tanks), and 11,140 kg of CG-targeting trim
/// fuel this model does not compute. Burning back down from the takeoff load
/// must then leave the landing fuel in the feed tanks the engines draw from.
#[test]
fn the_a380_fill_reproduces_the_published_refuel_panel_load_and_lands_on_the_feed_tanks() {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": "A380-800" }))
        .expect("the registered A380 preset loads");
    let preset = alas_config::presets::get("A380-800").expect("the A380 preset resolves");
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build(Some(&preset.design_vector), true)
        .expect("the A380 geometry builds");
    let (density_kg_m3, published_total_l) =
        crate::product_stations::tank_reference(&config, &preset.design_vector);
    let layout = FuelTankLayout::resolve(
        &plane,
        &config.geometry,
        &config.structures,
        &config.fuel_tanks,
        &config.fuel_policy,
        density_kg_m3,
        published_total_l,
    )
    .expect("the A380 tank arrangement resolves on its own geometry");
    // EASA TCDS EASA.A.110 Issue 17, section 3.3: 323,546 L of tanks.
    let certified_tank_kg = 323_546.0 * 1.0e-3 * density_kg_m3;
    assert!((layout.usable_capacity_kg() - certified_tank_kg).abs() < 1.0e-6 * certified_tank_kg);
    assert_eq!(
        layout
            .tanks()
            .iter()
            .filter(|tank| tank.kind == TankKind::WingFeed)
            .count(),
        4
    );

    let panel = layout.distribute(180_800.0).expect("the panel load fits");
    let fractions = |kind: TankKind| -> Vec<f64> {
        layout
            .tanks()
            .iter()
            .filter(|tank| tank.kind == kind)
            .map(|tank| fill_fraction(&layout, &panel, tank))
            .collect()
    };
    assert!(fractions(TankKind::WingOuter).iter().all(|f| *f > 0.999));
    assert!(fractions(TankKind::WingMid)
        .iter()
        .all(|f| *f > 0.956 - 0.02));
    for inner in fractions(TankKind::WingInner) {
        assert!((inner - 0.238).abs() < 0.10, "inner tank {inner:.3} full");
        for kind in [TankKind::WingMid, TankKind::WingOuter, TankKind::WingFeed] {
            assert!(fractions(kind).iter().all(|f| *f > inner));
        }
    }
    assert!(panel.properties(&layout).cg_m[1].abs() < 1.0e-8);

    let takeoff_fuel_kg = 0.95 * layout.usable_capacity_kg();
    let landing_fuel_kg = 25_000.0;
    let takeoff = layout.distribute(takeoff_fuel_kg).expect("load fits");
    let landing = takeoff
        .burned(&layout, takeoff_fuel_kg - landing_fuel_kg)
        .expect("burn fits");
    let feed_kg: f64 = layout
        .tanks()
        .iter()
        .filter(|tank| tank.kind == TankKind::WingFeed)
        .map(|tank| fill_of(&layout, &landing, &tank.id))
        .sum();
    assert!((feed_kg - landing_fuel_kg).abs() < 1.0e-6);
}

/// Wing cells burned 1, 2, 4 around a trim tank declared third and a wing
/// cell that shares the trim tank's priority, the A380 burn order.
fn trim_layout() -> FuelTankLayout {
    let trim = FuelTank {
        kind: TankKind::Trim,
        ..synthetic_tank("trim", 400.0, 3, 60.0)
    };
    FuelTankLayout {
        tanks: vec![
            synthetic_tank("wing_inner", 1_000.0, 1, 20.0),
            synthetic_tank("wing_mid", 800.0, 2, 25.0),
            trim,
            synthetic_tank("wing_aux", 200.0, 3, 27.0),
            synthetic_tank("wing_outer", 300.0, 4, 30.0),
        ],
        geometric_calibration_factor: 1.0,
        density_kg_m3: DENSITY_KG_M3,
    }
}

fn fill_of(layout: &FuelTankLayout, state: &FuelState, id: &str) -> f64 {
    state
        .mass_items(layout)
        .iter()
        .find(|item| item.id == id)
        .map_or(0.0, |item| item.mass_kg)
}

/// A trim tank is filled after every other tank, whatever its declared
/// burn priority, and never shares a fill group with a wing cell that has
/// the same priority.
#[test]
fn a_trim_tank_fills_only_once_every_other_tank_is_full() {
    let layout = trim_layout();
    let wings_kg = layout.usable_capacity_kg() - 400.0;
    for (load_kg, trim_kg) in [(1_000.0, 0.0), (wings_kg, 0.0), (wings_kg + 150.0, 150.0)] {
        let state = layout.distribute(load_kg).expect("load fits");
        assert!((fill_of(&layout, &state, "trim") - trim_kg).abs() < 1.0e-9);
        assert!((state.total_kg() - load_kg).abs() < 1.0e-9);
    }
    let partial = layout.distribute(1_000.0).expect("load fits");
    assert!((fill_of(&layout, &partial, "wing_outer") - 300.0).abs() < 1.0e-9);
    assert!((fill_of(&layout, &partial, "wing_aux") - 200.0).abs() < 1.0e-9);
}

/// A trim tank is emptied before every other tank, so a landing state
/// burned down from a full load carries no trim fuel, and the fuel CG only
/// moves forward while the trim tank drains.
#[test]
fn a_trim_tank_is_emptied_before_every_other_tank() {
    let layout = trim_layout();
    let full = layout
        .distribute(layout.usable_capacity_kg())
        .expect("full fits");
    let drained = full.burned(&layout, 400.0).expect("burn fits");
    assert_eq!(fill_of(&layout, &drained, "trim"), 0.0);
    assert!((fill_of(&layout, &drained, "wing_inner") - 1_000.0).abs() < 1.0e-9);
    let landing = full.burned(&layout, 2_000.0).expect("burn fits");
    assert_eq!(fill_of(&layout, &landing, "trim"), 0.0);
    assert_eq!(fill_of(&layout, &landing, "wing_inner"), 0.0);
    assert!((landing.total_kg() - 700.0).abs() < 1.0e-9);

    let vector = fuel_vector(&layout, &full, 41);
    let trim_draining: Vec<_> = vector
        .iter()
        .filter(|point| point.fuel_kg >= layout.usable_capacity_kg() - 400.0)
        .collect();
    assert!(trim_draining.len() > 2);
    assert!(trim_draining
        .windows(2)
        .all(|pair| pair[1].x_m <= pair[0].x_m + 1.0e-12));
}
