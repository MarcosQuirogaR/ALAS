// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Spanwise stations of a surface downstream of another, set by the
//! upstream trailing legs.
//!
//! A trailing leg of an upstream surface that passes within a fraction of a
//! strip of a downstream control point induces the unbounded `1 / distance`
//! velocity of a bare filament there, and the downstream circulation, the
//! pitching moment and its slope with incidence then follow how the two
//! meshes happen to interleave rather than the flow. The C295 tail, 6 cm
//! below its high wing, made the trimmed moment non-monotonic in
//! stabilizer incidence that way. The remedy of the AVL User Primer
//! (Drela and Youngren, section on vortex spacing) is a mesh rule: the
//! downstream surface takes its strip edges at the spanwise positions of
//! the upstream trailing legs, so each leg runs along a strip edge, where
//! the surface's own legs are, and each control point lies midway between
//! two upstream legs.
//!
//! Only panel edges move: every cross-section is kept and the inserted ones
//! lie on the same linear loft ([`Wing::with_inserted_stations`]).

use alas_geom::aircraft::airplane::Airplane;
use alas_geom::aircraft::wing::{SpacingFunction, Wing};

use super::{mesh_wing_panels, Panel, VlmError};

/// A target closer than this fraction of its local leg spacing to an
/// existing cross-section is dropped: the leg then lies within a quarter
/// strip of a strip edge, at least a quarter strip from any control point,
/// which is the clearance the rule secures, and no sliver strip is made.
const STATION_MERGE_FRACTION: f64 = 0.25;

/// Whether `wing` is a spanwise (not vertical) lattice surface, with its
/// streamwise extent `(leading edge x min, trailing edge x max)`.
fn planar_extent(wing: &Wing) -> Option<(f64, f64)> {
    let (root, tip) = (wing.xsecs.first()?, wing.xsecs.last()?);
    let (dy, dz) = (
        (tip.xyz_le[1] - root.xyz_le[1]).abs(),
        (tip.xyz_le[2] - root.xyz_le[2]).abs(),
    );
    if wing.is_geometry_only() || wing.xsecs.len() < 2 || dy <= dz {
        return None;
    }
    let le = wing
        .xsecs
        .iter()
        .map(|s| s.xyz_le[0])
        .fold(f64::INFINITY, f64::min);
    let te = wing
        .xsecs
        .iter()
        .map(|s| s.xyz_le[0] + s.chord)
        .fold(f64::NEG_INFINITY, f64::max);
    Some((le, te))
}

/// Mesh every wing of `airplane`, one panel list per wing in index order,
/// through `mesh(index, wing, spanwise_resolution)`: each wing as given,
/// except that a surface wholly behind another is first subdivided as usual
/// and then gains the stations of the module doc, and is meshed at
/// resolution one. A cache keyed on the `(wing, resolution)` it is handed
/// therefore sees the stations too.
pub(super) fn mesh_aligned(
    airplane: &Airplane,
    spanwise_resolution: usize,
    mut mesh: impl FnMut(usize, &Wing, usize) -> Result<Vec<Panel>, VlmError>,
) -> Result<Vec<Vec<Panel>>, VlmError> {
    let extents: Vec<Option<(f64, f64)>> = airplane.wings.iter().map(planar_extent).collect();
    let receiver = |index: usize| -> bool {
        extents[index].is_some_and(|(le, _)| {
            extents
                .iter()
                .enumerate()
                .any(|(other, e)| other != index && e.is_some_and(|(_, te)| te < le))
        })
    };
    let mut panels: Vec<Option<Vec<Panel>>> = vec![None; airplane.wings.len()];
    for (index, wing) in airplane.wings.iter().enumerate() {
        if !receiver(index) {
            panels[index] = Some(mesh(index, wing, spanwise_resolution)?);
        }
    }
    // Receivers front to back, so a surface behind a receiver sees that
    // receiver's final legs.
    let mut order: Vec<usize> = (0..airplane.wings.len()).filter(|&i| receiver(i)).collect();
    order.sort_by(|&a, &b| {
        let key = |i: usize| extents[i].map_or(0.0, |(le, _)| le);
        key(a).total_cmp(&key(b)).then(a.cmp(&b))
    });
    for index in order {
        let Some((le, _)) = extents[index] else {
            continue;
        };
        let legs: Vec<f64> = panels
            .iter()
            .enumerate()
            .filter(|(other, _)| extents[*other].is_some_and(|(_, te)| te < le))
            .filter_map(|(_, p)| p.as_ref())
            .flatten()
            .flat_map(|p| {
                [
                    p.left_vortex_vertex[1].abs(),
                    p.right_vortex_vertex[1].abs(),
                ]
            })
            .collect();
        let wing = &airplane.wings[index];
        let base = if spanwise_resolution > 1 {
            wing.subdivide_sections(spanwise_resolution, SpacingFunction::Cosspace)?
        } else {
            wing.clone()
        };
        panels[index] = Some(match aligned(&base, legs) {
            Some(fractions) => mesh(index, &base.with_inserted_stations(&fractions)?, 1)?,
            None => mesh(index, wing, spanwise_resolution)?,
        });
    }
    Ok(panels.into_iter().map(Option::unwrap_or_default).collect())
}

/// Per lofted section of `wing`, the fractions at which it takes an
/// upstream leg position from `legs` (|y|, m); `None` when no leg falls
/// inside its span or its cross-sections do not advance in |y|.
///
/// Within a section of width `w`, legs closer together than
/// [`STATION_MERGE_FRACTION`]` * w` share one station at their mean, and a
/// station that close to a cross-section is dropped: the legs then run
/// within that distance of a strip edge, and no strip is narrower than it.
/// A station in the root section also keeps the strip next to the plane
/// of symmetry wide enough for that clearance from the mirror image's root
/// legs ([`mirror_intrusion`]): on the B747 stabilizer, twisted about its
/// dihedral axis, a narrower root strip put its aft control points 8 cm
/// from them and the trimmed moment jumped with incidence.
fn aligned(wing: &Wing, mut legs: Vec<f64>) -> Option<Vec<Vec<f64>>> {
    let y: Vec<f64> = wing.xsecs.iter().map(|s| s.xyz_le[1].abs()).collect();
    if y.windows(2).any(|w| w[1] <= w[0]) {
        return None;
    }
    legs.sort_by(f64::total_cmp);
    let intrusion = mirror_intrusion(wing);
    let mut fractions = Vec::with_capacity(y.len() - 1);
    let mut any = false;
    for (section, pair) in y.windows(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        let merge = STATION_MERGE_FRACTION * (b - a);
        // The strip next to the plane of symmetry holds the mirror image's
        // root legs `intrusion` inside its edge; a quarter-strip clearance
        // from them needs a strip at least four intrusions wide.
        let root_floor = if section == 0 {
            a + intrusion / STATION_MERGE_FRACTION
        } else {
            a
        };
        let mut stations: Vec<f64> = Vec::new();
        let mut cluster: Vec<f64> = Vec::new();
        let close = |cluster: &mut Vec<f64>, stations: &mut Vec<f64>| {
            if !cluster.is_empty() {
                let mean = cluster.iter().sum::<f64>() / cluster.len() as f64;
                if mean - a >= merge && mean >= root_floor && b - mean >= merge {
                    stations.push(mean);
                }
                cluster.clear();
            }
        };
        for &t in legs.iter().filter(|&&t| t > a && t < b) {
            if cluster.last().is_some_and(|&last| t - last >= merge) {
                close(&mut cluster, &mut stations);
            }
            cluster.push(t);
        }
        close(&mut cluster, &mut stations);
        any |= !stations.is_empty();
        fractions.push(stations.iter().map(|t| (t - a) / (b - a)).collect());
    }
    any.then_some(fractions)
}

/// How far inside the root strip edge of a symmetric `wing` its mirror
/// image's root legs lie, m: a root chord twisted about a dihedral axis
/// crosses the plane of symmetry towards the trailing edge, taking its
/// mirror image's edge across with it. Zero when the root stays clear of
/// the plane or the wing is not mirrored.
fn mirror_intrusion(wing: &Wing) -> f64 {
    let (Some(root), Some(tip)) = (wing.xsecs.first(), wing.xsecs.last()) else {
        return 0.0;
    };
    if !wing.symmetric {
        return 0.0;
    }
    let side = (tip.xyz_le[1] - root.xyz_le[1]).signum();
    let mut half = wing.clone();
    half.symmetric = false;
    let (points, _) = half.mesh_thin_surface(1, true);
    let nearest = points
        .iter()
        .map(|p| side * p[1])
        .fold(f64::INFINITY, f64::min);
    (-2.0 * nearest).max(0.0)
}

/// [`mesh_aligned`] with the ordinary per-wing meshing, all panels in wing
/// order: the mesh of [`super::VlmSystem::assemble`].
pub(super) fn mesh_panels(
    airplane: &Airplane,
    spanwise_resolution: usize,
    chordwise_resolution: usize,
) -> Result<Vec<Panel>, VlmError> {
    let per_wing = mesh_aligned(airplane, spanwise_resolution, |index, wing, spanwise| {
        let mut panels = Vec::new();
        mesh_wing_panels(wing, index, spanwise, chordwise_resolution, &mut panels)?;
        Ok(panels)
    })?;
    Ok(per_wing.into_iter().flatten().collect())
}
