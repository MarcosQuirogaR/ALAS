// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The static ground shape of every preset whose wing and engine heights are
//! fitted to airport-planning ground clearances reproduces those clearances
//! (maximum ramp weight, aft CG), measured from the shared ground plane
//! (`alas_mass::stations::ground_plane_z_m`).
//!
//! Nacelle low point: the lowest nacelle cross-section bottom. Wing-tip low
//! point: the tip leading edge less half the tip section's thickness, the
//! lower surface under the thickest point. The flight shape, which the A380
//! raises by its declared static-to-1 g tip rise, is not what these tables
//! measure.

// A test unwrap is the assertion failing on a registered preset or geometry.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use alas_config::{presets, AlasConfig, WingShape};
use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::spacing::linspace;
use alas_geom::builder::AircraftBuilder;
use alas_mass::stations::ground_plane_z_m;

/// Resolution of the printed clearance tables, m.
const TABLE_RESOLUTION_M: f64 = 0.01;

struct GroundShape {
    plane: Airplane,
    ground_z_m: f64,
}

fn ground_shape(preset: &str) -> GroundShape {
    let config = AlasConfig::from_value(&serde_json::json!({ "preset": preset })).unwrap();
    let design = presets::get(preset).unwrap().design_vector;
    let plane = AircraftBuilder::new(Some(config.geometry.clone()))
        .build_shape(Some(&design), true, WingShape::Ground)
        .unwrap();
    let ground_z_m = ground_plane_z_m(&plane.fuselages[0], &config.geometry, &config.landing_gear);
    GroundShape { plane, ground_z_m }
}

impl GroundShape {
    /// Nacelle low points above the ground, right-hand wing, inboard first.
    fn nacelle_low_points_m(&self) -> Vec<f64> {
        let mut nacelles: Vec<(f64, f64)> = self
            .plane
            .fuselages
            .iter()
            .filter(|body| body.name == "Nacelle R")
            .map(|body| {
                let low = body
                    .xsecs
                    .iter()
                    .map(|xsec| xsec.xyz_c[2] - xsec.height / 2.0)
                    .fold(f64::INFINITY, f64::min);
                (body.xsecs[0].xyz_c[1], low - self.ground_z_m)
            })
            .collect();
        nacelles.sort_by(|a, b| a.0.total_cmp(&b.0));
        nacelles.into_iter().map(|(_, height)| height).collect()
    }

    /// Bottom of the wing-tip fence (the A380 table's W2: AC 2-3-0 draws W1/W2 to the top/bottom
    /// of the tip fence), from the tip leading edge down by the fence's lower height.
    fn wing_tip_fence_bottom_m(&self, down_height_m: f64) -> f64 {
        self.plane.wings[0].xsecs.last().unwrap().xyz_le[2] - down_height_m - self.ground_z_m
    }

    fn wing_tip_low_point_m(&self) -> f64 {
        let tip = self.plane.wings[0].xsecs.last().unwrap();
        let thickness = tip.airfoil.max_thickness(&linspace(0.0, 1.0, 101));
        tip.xyz_le[2] - 0.5 * thickness * tip.chord - self.ground_z_m
    }
}

/// Nacelle heights that the geometry freeze of 2026-10-12 (owner-approved
/// overlays) placed where the drawing puts them rather than where the
/// airport-planning table puts them. The table is a MRW clearance and the
/// drawing is "related to aircraft attitude and weight" (A380 AC 2-2-0 note),
/// so the two are different statements; the loading condition does not
/// explain the gap (A380 AC 2-3-0: light weight raises N1/N2 by 0.22/0.37 m
/// but the fuselage bottom by 0.21 m, i.e. +0.01/+0.16 m relative to the
/// airframe). The deviation is therefore PINNED here as an owner decision,
/// not hidden: `deviation_m` is model minus published, m.
fn assert_overlay_compromise(label: &str, actual: f64, published: f64, deviation_m: f64) {
    assert_close(
        &format!(
            "{label} (published {published:.3} m plus the overlay deviation {deviation_m:+.3} m)"
        ),
        actual,
        published + deviation_m,
        TABLE_RESOLUTION_M,
    );
}

fn assert_close(label: &str, actual: f64, published: f64, tolerance: f64) {
    assert!(
        (actual - published).abs() <= tolerance,
        "{label}: {actual:.3} m against the published {published:.3} m"
    );
}

#[test]
fn the_a380_ground_shape_stands_at_its_published_clearances() {
    // The A380 ground shape is the UNLOADED (drawn) wing; the loaded table (AC
    // 2-3-0, MRW: W2 5.21, N1 1.08, N2 1.90 m) follows by subtracting the
    // computed ground droop. W2 is the BOTTOM OF THE TIP FENCE (the W1/W2 lines
    // bound the fence in the figure), drawn 6.58-6.60 m above the ground line in
    // the 2-2-0 front view (vector re-measurement); not the thin-tip lower surface;
    // (the fence hangs 1.2 m under the
    // tip leading edge (the preset's `down_height_m`). The nacelles are not
    // fitted to the loaded table (N1 1.08 m, N2 1.90 m, MRW aft CG) but to the
    // UNLOADED nacelles of the 2-2-0 general-arrangement drawing, 1.14-1.16 m
    // (N1) and 2.31-2.32 m (N2) above the drawn ground line, with the 3.88 m
    // nacelle of 2-12-0 and one centreline height per engine pair
    // (`z_per_engine_m`). The loaded droop is a separate clearance check.
    let shape = ground_shape("A380-800");
    assert_close(
        "W2 unloaded (drawn fence bottom)",
        shape.wing_tip_fence_bottom_m(1.2),
        6.59,
        0.02,
    );
    let nacelles = shape.nacelle_low_points_m();
    assert_eq!(nacelles.len(), 2);
    assert_close("A380 N1 (drawing)", nacelles[0], 1.15, 0.015);
    assert_close("A380 N2 (drawing)", nacelles[1], 2.315, 0.015);
    assert!(
        nacelles[0] < nacelles[1],
        "the outboard nacelle stands higher"
    );
}

#[test]
fn the_a340_ground_shape_stands_at_its_published_clearances() {
    // Airbus A340-200/-300 AC Rev 33, Figure 2-3-0-991-005-A01 (aft CG):
    // wing tip W2 5.94 m, nacelles N1 1.28 m and N2 2.35 m.
    let shape = ground_shape("A340-300");
    assert_close("W2", shape.wing_tip_low_point_m(), 5.94, TABLE_RESOLUTION_M);
    let nacelles = shape.nacelle_low_points_m();
    assert_eq!(nacelles.len(), 2);
    assert_close("N1", nacelles[0], 1.28, TABLE_RESOLUTION_M);
    assert_close("N2", nacelles[1], 2.35, TABLE_RESOLUTION_M);
}

#[test]
fn the_twin_ground_shapes_stand_at_their_published_nacelle_clearances() {
    // A320-200: Airbus AC Jun 01/24, Figure 2-3-0-991-029-A01 sheet 2 (MRW,
    // aft CG), CFM56-5B nacelle 0.577 m and sharklet bottom 4.009 m.
    // A220-300: ACP Issue 013 ground-clearance table, nacelle 22.9 in
    // (0.582 m) minimum. 787-9: D6-58333 Rev Q section 2.3.2, GE nacelle
    // (F) 0.69 m minimum.
    // The freeze raised the A320, A220 and E195 nacelles to their drawings:
    // deviations A320 +0.155 m, A220 -0.118 m (drawing lip compromise),
    // 787-9 +0.024 m; see assert_overlay_compromise.
    let a320 = ground_shape("A320-200");
    assert_overlay_compromise("A320 nacelle", a320.nacelle_low_points_m()[0], 0.577, 0.155);
    assert_close(
        "A320 wing tip",
        a320.wing_tip_low_point_m(),
        4.009,
        TABLE_RESOLUTION_M,
    );
    let a220 = ground_shape("A220-300");
    assert_overlay_compromise(
        "A220 nacelle",
        a220.nacelle_low_points_m()[0],
        0.582,
        -0.118,
    );
    let b787 = ground_shape("B787-9");
    assert_overlay_compromise("787-9 nacelle", b787.nacelle_low_points_m()[0], 0.69, 0.024);
}
