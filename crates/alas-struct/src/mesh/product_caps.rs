// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Product spar flanges as separate rectangles at their physical centroids.
//! The existing PSHELL web carries web material; these bars carry caps only.
//! CBAR orientation is the local thickness direction (element y, DIM2), with
//! width along element z (DIM1). Coordinates/offsets are basic-frame metres.
//! See MSC Nastran Reference Guide, BAR cross section, and CBAR offset fields:
//! https://help.altair.com/hwsolvers/os/topics/solvers/os/cbar_bulk_r.htm

use super::elements::{station_value, MID_CAP, PID_CAP_BASE};
use super::{Cbar, Deck, MeshError, Pbarl};
use crate::sizing::WingboxSizing;
use alas_geom::wing_structure::WingStructureGeometry;

pub(super) fn add_spar_caps(
    deck: &mut Deck,
    eid: &mut i64,
    upper: &[Vec<i64>],
    lower: &[Vec<i64>],
    sizing: &WingboxSizing,
    geometry: &WingStructureGeometry,
) -> Result<(), MeshError> {
    let mut pid = PID_CAP_BASE;
    for (spar, (top, bottom)) in upper.iter().zip(lower).enumerate() {
        let section = sizing
            .spars
            .get(spar)
            .ok_or(MeshError::InvalidCapGeometry { spar, segment: 0 })?;
        for segment in 0..top.len().saturating_sub(1) {
            let error = MeshError::InvalidCapGeometry { spar, segment };
            let nodes = [
                top[segment],
                top[segment + 1],
                *bottom.get(segment).ok_or(error)?,
                *bottom.get(segment + 1).ok_or(error)?,
            ];
            let a = deck.grid_xyz(nodes[0]).ok_or(error)?;
            let b = deck.grid_xyz(nodes[1]).ok_or(error)?;
            let c = deck.grid_xyz(nodes[2]).ok_or(error)?;
            let d = deck.grid_xyz(nodes[3]).ok_or(error)?;
            let eta = ((a[1] + b[1]) / (2.0 * geometry.semi_span)).clamp(0.0, 1.0);
            let thickness = station_value(eta, &sizing.eta_stations, &section.t_cap);
            let width = station_value(eta, &sizing.eta_stations, &section.w_cap);
            let (normal_a, height_a) = direction(c, a).ok_or(error)?;
            let (normal_b, height_b) = direction(d, b).ok_or(error)?;
            let (tangent, _) = direction(a, b).ok_or(error)?;
            let projection = tangent
                .iter()
                .zip(normal_a)
                .map(|(a, b)| a * b)
                .sum::<f64>();
            if !thickness.is_finite()
                || !width.is_finite()
                || thickness <= 0.0
                || width <= 0.0
                || 2.0 * thickness >= height_a.min(height_b)
                || 1.0 - projection * projection <= 1.0e-12
            {
                return Err(error);
            }
            deck.bar_properties.push(Pbarl {
                pid,
                mid: MID_CAP,
                section: "BAR",
                dim: vec![width, thickness],
            });
            for (ga, gb, sign) in [(nodes[0], nodes[1], -1.0), (nodes[2], nodes[3], 1.0)] {
                deck.bars.push(Cbar {
                    eid: *eid,
                    pid,
                    ga,
                    gb,
                    x: normal_a,
                    offt: "GGG",
                    offset_a: normal_a.map(|v| sign * 0.5 * thickness * v),
                    offset_b: normal_b.map(|v| sign * 0.5 * thickness * v),
                });
                *eid += 1;
            }
            pid += 1;
        }
    }
    Ok(())
}

fn direction(from: [f64; 3], to: [f64; 3]) -> Option<([f64; 3], f64)> {
    let delta = std::array::from_fn::<_, 3, _>(|i| to[i] - from[i]);
    let length = delta.iter().map(|v| v * v).sum::<f64>().sqrt();
    (length.is_finite() && length > 0.0).then(|| (delta.map(|v| v / length), length))
}
