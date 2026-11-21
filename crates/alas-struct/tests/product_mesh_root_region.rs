// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The swept-root region of the product deck: meshed through its truncated
//! ribs, with flat shells and no rivets.
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod support;

use alas_config::{materials, DesignRequirements, EngineConfig, MassModelConfig, StructuresConfig};
use alas_struct::mesh::{build_wing_mesh_bdf_product, Deck, WARPING_THRESHOLD};
use alas_struct::sizing::size_wingbox_reference_compatibility;

fn warping(deck: &Deck, nodes: &[i64]) -> f64 {
    let p: Vec<[f64; 3]> = nodes.iter().map(|&n| deck.grid_xyz(n).unwrap()).collect();
    let d = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    let norm = |a: [f64; 3]| (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    let (d1, d2) = (d(p[2], p[0]), d(p[3], p[1]));
    let n = [
        d1[1] * d2[2] - d1[2] * d2[1],
        d1[2] * d2[0] - d1[0] * d2[2],
        d1[0] * d2[1] - d1[1] * d2[0],
    ];
    let m = norm(n);
    let c: [f64; 3] = [0, 1, 2].map(|k| p.iter().map(|q| q[k]).sum::<f64>() / 4.0);
    let h = p
        .iter()
        .map(|q| {
            let r = d(*q, c);
            ((r[0] * n[0] + r[1] * n[1] + r[2] * n[2]) / m).abs()
        })
        .fold(0.0, f64::max);
    h / (2.0 * (norm(d1) + norm(d2)))
}

#[test]
fn the_swept_root_is_meshed_through_its_truncated_ribs_with_flat_shells() {
    let cfg = StructuresConfig {
        num_ribs_override: Some(12),
        mesh_chordwise_points: 18,
        ..Default::default()
    };
    let req = DesignRequirements::default();
    let material = materials::get("Al 7075-T6").expect("registered material");
    let geometry = support::build_geometry(&[0.15, 0.65], &[true, true]);
    let sizing = size_wingbox_reference_compatibility(
        &geometry, &cfg, &req, material, material, material, material,
    );
    let (deck, report, index) = build_wing_mesh_bdf_product(
        &geometry,
        &sizing,
        &cfg,
        &EngineConfig::default(),
        &MassModelConfig::default(),
        &req,
        material,
        material,
        material,
        material,
    )
    .expect("product mesh");

    // No truncated rib hangs on the skin through a rigid element, and every
    // shell keeps its orientation over its area (the mass integration checks
    // the Jacobian at each Gauss point).
    assert_eq!(report.rbe3_count, 0);
    assert!(deck.primary_structural_mass_kg().is_some());

    // The front spar runs through every row of the region, root outward.
    let front = &index.spar_upper_nids[0];
    let ys: Vec<f64> = front.iter().map(|&n| deck.node_y(n)).collect();
    assert!(
        ys.windows(2).all(|w| w[1] > w[0]),
        "front spar grids ascend: {ys:?}"
    );

    // The rear spar runs through the region too, and the skin is flat
    // everywhere: the root region was one warped strip from the root rib to
    // the first full rib.
    let rear = &index.spar_upper_nids[1];
    let rear_ys: Vec<f64> = rear.iter().map(|&n| deck.node_y(n)).collect();
    assert!(
        rear_ys.windows(2).all(|w| w[1] > w[0]),
        "rear spar grids ascend: {rear_ys:?}"
    );
    // Every bay of the region is split into at least four rows: at least
    // four front-spar grids per outboard rib spacing up to the first full rib,
    // the last grid before the spacing opens up to the rib spacing.
    let outboard = ys[ys.len() - 1] - ys[ys.len() - 2];
    let region_end = ys
        .windows(2)
        .filter(|w| w[1] - w[0] < 0.5 * outboard)
        .map(|w| w[1])
        .fold(0.0, f64::max);
    let rows = ys.iter().filter(|&&y| y > 1e-6 && y <= region_end).count();
    let bays = (region_end / outboard).ceil() as usize;
    assert!(
        rows >= 4 * bays,
        "{rows} front-spar rows for {bays} bays: {ys:?}"
    );
    let worst = deck
        .quads()
        .iter()
        .filter(|q| q.pid == 1)
        .map(|q| warping(&deck, &q.nodes))
        .fold(0.0, f64::max);
    assert!(
        worst < WARPING_THRESHOLD,
        "root-region skin warping {worst:.4}"
    );
    // The cover follows the loft: no root-region fold is sharper, per metre,
    // than the sharpest one outboard, where the strips run rib to rib on the
    // sampled sections.
    let (root, outboard_folds) = folds(&deck, region_end);
    assert!(
        root <= outboard_folds,
        "root-region cover curvature {root:.3} rad/m, outboard {outboard_folds:.3} rad/m"
    );
}

type ShellFrame = ([f64; 3], [f64; 3]);

/// The sharpest fold between two cover shells sharing an edge, as a
/// curvature: the angle between their normals over the distance between their
/// centroids, rad/m, inboard and outboard of `y_split` (by the shared edge's
/// midpoint). A step in the cover shows as a curvature far above the loft's.
fn folds(deck: &Deck, y_split: f64) -> (f64, f64) {
    let shells: Vec<&[i64]> = deck
        .quads()
        .iter()
        .chain(deck.trias())
        .filter(|s| s.pid == 1)
        .map(|s| s.nodes.as_slice())
        .collect();
    let centroid = |nodes: &[i64]| {
        let p: Vec<[f64; 3]> = nodes.iter().map(|&n| deck.grid_xyz(n).unwrap()).collect();
        [0, 1, 2].map(|k| p.iter().map(|q| q[k]).sum::<f64>() / p.len() as f64)
    };
    let normal = |nodes: &[i64]| {
        let p: Vec<[f64; 3]> = nodes.iter().map(|&n| deck.grid_xyz(n).unwrap()).collect();
        let d = |a: [f64; 3], b: [f64; 3]| [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
        let (a, b) = if p.len() == 4 {
            (d(p[2], p[0]), d(p[3], p[1]))
        } else {
            (d(p[1], p[0]), d(p[2], p[0]))
        };
        let n = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let m = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        n.map(|v| v / m)
    };
    // Each edge's shells, as (unit normal, centroid).
    let mut edges: std::collections::HashMap<(i64, i64), Vec<ShellFrame>> =
        std::collections::HashMap::new();
    for nodes in &shells {
        let (n, c) = (normal(nodes), centroid(nodes));
        for k in 0..nodes.len() {
            let (a, b) = (nodes[k], nodes[(k + 1) % nodes.len()]);
            edges.entry((a.min(b), a.max(b))).or_default().push((n, c));
        }
    }
    let (mut inboard, mut outboard) = (0.0_f64, 0.0_f64);
    for ((a, b), normals) in &edges {
        if normals.len() != 2 {
            continue;
        }
        let ((p, c), (q, d)) = (normals[0], normals[1]);
        let cos = (p[0] * q[0] + p[1] * q[1] + p[2] * q[2]).abs().min(1.0);
        let gap = ((c[0] - d[0]).powi(2) + (c[1] - d[1]).powi(2) + (c[2] - d[2]).powi(2)).sqrt();
        let angle = cos.acos() / gap.max(1e-12);
        if 0.5 * (deck.node_y(*a) + deck.node_y(*b)) < y_split {
            inboard = inboard.max(angle);
        } else {
            outboard = outboard.max(angle);
        }
    }
    (inboard, outboard)
}
