// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Writing the element families, and the deck's numbering plan.
//!
//! Each function here is one block of the reference's builder, in the order it
//! runs. That order is not cosmetic: a single counter numbers every element, so
//! skin before webs before ribs before trailing-edge panels before caps before
//! masses is what decides which element is `1`, and a deck whose identifiers
//! moved is a different deck to anything that names one.
//!
//! The material and property identifiers live here too, with the cards that
//! point at them.

use alas_config::{DesignRequirements, EngineConfig, MassModelConfig, StructuresConfig};
use alas_geom::wing_structure::{RibStation, WingStructureGeometry};

use super::build::RibRegions;
use super::cards::{Cbar, Conm2, Deck, Pbarl, Shell};
use super::nodes::{NodeMap, Surface};
use super::te_rib_selected;
use crate::loads;
use crate::sizing::WingboxSizing;
use std::collections::HashSet;

/// Material identifiers, one per wingbox material.
pub(super) const MID_SKIN: i64 = 1;
pub(super) const MID_WEB: i64 = 2;
pub(super) const MID_CAP: i64 = 3;
pub(super) const MID_RIB: i64 = 4;

/// Property identifiers. Webs take one property per spar from
/// [`PID_WEB_BASE`], and caps one per spar *segment* from [`PID_CAP_BASE`],
/// since a cap tapers along the span and each element carries its own section.
pub(super) const PID_SKIN: i64 = 1;
pub(super) const PID_MAIN_RIB: i64 = 4;
pub(super) const PID_SEC_RIB: i64 = 5;
pub(super) const PID_TE_STRIP: i64 = 6;
pub(super) const PID_WEB_BASE: i64 = 100;
pub(super) const PID_CAP_BASE: i64 = 10_000;

/// The spar caps' orientation vector, and the offset convention their `CBAR`
/// elements are written with.
const CAP_ORIENTATION: [f64; 3] = [1.0, 0.0, 0.0];
const CAP_OFFT: &str = "GGG";

/// A quadrilateral panel, degenerating to a triangle when two of its corners
/// are the same grid and vanishing when three are: `_add_q`.
pub(super) fn add_quad(deck: &mut Deck, eid: &mut i64, corners: [i64; 4], pid: i64) {
    let mut unique = vec![corners[0]];
    for &corner in &corners[1..] {
        if unique.last() != Some(&corner) {
            unique.push(corner);
        }
    }
    if unique.len() > 1 && unique.first() == unique.last() {
        unique.pop();
    }
    if matches!(unique.len(), 3 | 4) {
        let shell = Shell {
            eid: *eid,
            pid,
            nodes: unique,
        };
        if shell.nodes.len() == 4 {
            deck.quads.push(shell);
        } else {
            deck.trias.push(shell);
        }
        *eid += 1;
    }
}

/// An explicitly triangular panel: the zipper's pivot fan, as opposed to a
/// quadrilateral that happened to collapse.
pub(super) fn add_tria(deck: &mut Deck, eid: &mut i64, corners: [i64; 3], pid: i64) {
    deck.trias.push(Shell {
        eid: *eid,
        pid,
        nodes: corners.to_vec(),
    });
    *eid += 1;
}

/// Skin panels between two consecutive skin ribs, bridging any difference in
/// their chordwise point counts with a fan of triangles: `_zipper_skin_strip`.
pub(super) fn zipper_skin_strip(
    deck: &mut Deck,
    eid: &mut i64,
    stations: &[RibStation],
    nodes: &NodeMap,
    pair: &[usize],
    surface: Surface,
) {
    let (current, next) = (pair[0], pair[1]);
    let points = |rib: usize| match surface {
        Surface::Ext => stations[rib].extrados.len(),
        Surface::Int => stations[rib].intrados.len(),
    };
    let (len_current, len_next) = (points(current), points(next));
    let shortest = len_current.min(len_next);
    // The intrados is walked the other way round so its panels keep an outward
    // normal.
    let reversed = surface == Surface::Int;

    for j in 0..shortest.saturating_sub(1) {
        let corners = [
            nodes.at(current, j, surface),
            nodes.at(next, j, surface),
            nodes.at(next, j + 1, surface),
            nodes.at(current, j + 1, surface),
        ];
        let ordered = if reversed {
            [corners[0], corners[3], corners[2], corners[1]]
        } else {
            corners
        };
        add_quad(deck, eid, ordered, PID_SKIN);
    }

    if len_current > len_next {
        let pivot = nodes.at(next, shortest - 1, surface);
        for j in shortest - 1..len_current - 1 {
            let (a, b) = (
                nodes.at(current, j, surface),
                nodes.at(current, j + 1, surface),
            );
            let corners = if reversed {
                [a, b, pivot]
            } else {
                [a, pivot, b]
            };
            add_tria(deck, eid, corners, PID_SKIN);
        }
    } else if len_next > len_current {
        let pivot = nodes.at(current, shortest - 1, surface);
        for j in shortest - 1..len_next - 1 {
            let (a, b) = (nodes.at(next, j, surface), nodes.at(next, j + 1, surface));
            let corners = if reversed {
                [b, a, pivot]
            } else {
                [pivot, a, b]
            };
            add_tria(deck, eid, corners, PID_SKIN);
        }
    }
}

/// The panels closing each rib between its two surfaces.
pub(super) fn add_rib_panels(
    deck: &mut Deck,
    eid: &mut i64,
    stations: &[RibStation],
    nodes: &NodeMap,
    regions: &RibRegions,
) {
    let skin_ribs_main: HashSet<usize> = regions.skin_ribs_main.iter().copied().collect();
    for &rib in &regions.all_structural {
        let station = &stations[rib];
        let last_spar = last_spar_point(station);
        let n_chord = station.extrados.len();
        let pid = if station.is_full {
            PID_MAIN_RIB
        } else {
            PID_SEC_RIB
        };

        // The reference writes this as a nested conditional whose two
        // spar-reaching branches do the same thing: a main skin rib stops its
        // panels at the rear spar, and so does a truncated one. Only a rib that
        // is neither (the root rib, or one whose cut reaches no spar at all)
        // is closed all the way to its last chordwise point.
        let stops_at_spar = last_spar != -1 && (skin_ribs_main.contains(&rib) || !station.is_full);
        let end = if stops_at_spar {
            last_spar
        } else {
            n_chord as i64 - 1
        };

        // The leading-edge point is skipped: both surfaces share one grid
        // there, so the first panel would be degenerate.
        for j in 1..end.max(0) as usize {
            add_quad(
                deck,
                eid,
                [
                    nodes.at(rib, j, Surface::Ext),
                    nodes.at(rib, j + 1, Surface::Ext),
                    nodes.at(rib, j + 1, Surface::Int),
                    nodes.at(rib, j, Surface::Int),
                ],
                pid,
            );
        }
    }
}

/// The optional aft-of-the-rear-spar rib panels, and the strip that closes the
/// trailing edge between consecutive full ribs.
pub(super) fn add_trailing_edge(
    deck: &mut Deck,
    eid: &mut i64,
    stations: &[RibStation],
    nodes: &NodeMap,
    regions: &RibRegions,
    cfg: &StructuresConfig,
    wsg: &WingStructureGeometry,
) {
    let y_break = wsg.y_break;
    let n_inboard = regions
        .skin_ribs_main
        .iter()
        .filter(|&&rib| stations[rib].y_station <= y_break)
        .count();

    for (position, &rib) in regions.skin_ribs_main.iter().enumerate() {
        let station = &stations[rib];
        if !te_rib_selected(
            &cfg.te_rib_mode,
            position,
            station.y_station,
            y_break,
            n_inboard,
        ) {
            continue;
        }
        let last_spar = last_spar_point(station);
        if last_spar < 0 {
            continue;
        }
        let n_chord = station.extrados.len();
        if last_spar >= n_chord as i64 - 1 {
            continue;
        }
        for j in last_spar as usize..n_chord - 1 {
            add_quad(
                deck,
                eid,
                [
                    nodes.at(rib, j, Surface::Ext),
                    nodes.at(rib, j + 1, Surface::Ext),
                    nodes.at(rib, j + 1, Surface::Int),
                    nodes.at(rib, j, Surface::Int),
                ],
                PID_SEC_RIB,
            );
        }
    }

    for pair in regions.skin_ribs.windows(2) {
        let (current, next) = (pair[0], pair[1]);
        if !(stations[current].is_full && stations[next].is_full) {
            continue;
        }
        let (last_current, last_next) = (
            stations[current].extrados.len() - 1,
            stations[next].extrados.len() - 1,
        );
        add_quad(
            deck,
            eid,
            [
                nodes.at(current, last_current, Surface::Ext),
                nodes.at(next, last_next, Surface::Ext),
                nodes.at(next, last_next, Surface::Int),
                nodes.at(current, last_current, Surface::Int),
            ],
            PID_TE_STRIP,
        );
    }
}

/// The tapered spar caps: one `CBAR` per spar segment, each with its own
/// `PBARL` section, sized from the root cap and the taper law.
pub(super) fn add_spar_caps(
    deck: &mut Deck,
    eid: &mut i64,
    spar_upper: &[Vec<i64>],
    spar_lower: &[Vec<i64>],
    sizing: &WingboxSizing,
    wsg: &WingStructureGeometry,
) {
    for (spar, (upper, lower)) in spar_upper.iter().zip(spar_lower).enumerate() {
        let Some(root_sizing) = sizing.spars.get(spar) else {
            continue;
        };
        let family = PID_CAP_BASE + spar as i64 * 1000;

        for index in 0..upper.len().saturating_sub(1) {
            let start = deck.grid_xyz(upper[index]).unwrap_or([0.0; 3]);
            let end = deck.grid_xyz(upper[index + 1]).unwrap_or([0.0; 3]);
            let below = deck.grid_xyz(lower[index]).unwrap_or([0.0; 3]);

            let eta_mid = ((start[1] + end[1]) / 2.0 / wsg.semi_span).clamp(0.0, 1.0);
            let height = (0..3)
                .map(|k| (start[k] - below[k]).powi(2))
                .sum::<f64>()
                .sqrt()
                .max(0.05);

            // The flange the sizing carried at this station (the tapered
            // root section, sized up wherever the local moment demanded more)
            // so the mesh and the sizing describe the same cap.
            let sized_thickness = station_value(eta_mid, &sizing.eta_stations, &root_sizing.t_cap);
            let sized_width = station_value(eta_mid, &sizing.eta_stations, &root_sizing.w_cap);
            let flange_thickness = sized_thickness.min(height / 3.0);
            let flange_width = sized_width.max(flange_thickness);

            let pid = family + index as i64;
            deck.bar_properties.push(Pbarl {
                pid,
                mid: MID_CAP,
                section: "I",
                dim: vec![
                    height,
                    flange_width,
                    flange_width,
                    0.001,
                    flange_thickness,
                    flange_thickness,
                ],
            });
            deck.bars.push(Cbar {
                eid: *eid,
                pid,
                ga: upper[index],
                gb: upper[index + 1],
                x: CAP_ORIENTATION,
                offt: CAP_OFFT,
            });
            *eid += 1;
        }
    }
}

/// Every grid on the root rib, upper surface first, each listed once.
pub(super) fn root_constraint_nodes(stations: &[RibStation], nodes: &NodeMap) -> Vec<i64> {
    let Some(root) = stations.first() else {
        return Vec::new();
    };
    let mut constrained: Vec<i64> = (0..root.extrados.len())
        .map(|j| nodes.at(0, j, Surface::Ext))
        .collect();
    for j in 0..root.intrados.len() {
        let nid = nodes.at(0, j, Surface::Int);
        if !constrained.contains(&nid) {
            constrained.push(nid);
        }
    }
    constrained
}

/// One `CONM2` per wing-mounted engine, hung off the front-spar grid of the
/// nearest structural rib.
#[allow(clippy::too_many_arguments)] // Every argument is one input of the reference's own block.
pub(super) fn add_engine_masses(
    deck: &mut Deck,
    eid: &mut i64,
    stations: &[RibStation],
    nodes: &NodeMap,
    regions: &RibRegions,
    wsg: &WingStructureGeometry,
    engine_cfg: &EngineConfig,
    mass_cfg: &MassModelConfig,
    req: &DesignRequirements,
) -> Vec<i64> {
    let mut attached = Vec::new();
    for (y_engine, mass) in loads::engine_point_loads_n(engine_cfg, mass_cfg, req) {
        let Some(nearest) = regions.all_structural.iter().copied().min_by(|&a, &b| {
            (stations[a].y_station - y_engine.abs())
                .abs()
                .total_cmp(&(stations[b].y_station - y_engine.abs()).abs())
        }) else {
            continue;
        };
        let station = &stations[nearest];
        let front = match station.j_spars.first() {
            Some(&point) if point >= 0 => point as usize,
            _ => 0,
        };
        let nid = nodes.at(nearest, front, Surface::Int);
        let chord = wsg.local_chord(station.eta);
        deck.masses.push(Conm2 {
            eid: *eid,
            nid,
            cid: 0,
            mass,
            // Generic pylon geometry: the mass hangs ahead of and below the
            // front spar.
            offset: [-0.15 * chord, 0.0, -1.0],
        });
        attached.push(nid);
        *eid += 1;
    }
    attached
}

/// The last chordwise point index this rib's spars reach, or `-1` when the
/// rib's truncated cut reaches none of them.
pub(super) fn last_spar_point(station: &RibStation) -> i64 {
    station.j_spars.last().map_or(-1, |&point| i64::from(point))
}

/// A sizing quantity sampled at `eta` by linear interpolation over the
/// sizing's own station grid, held at the end values outside it.
fn station_value(eta: f64, stations: &[f64], values: &[f64]) -> f64 {
    let n = stations.len().min(values.len());
    if n == 0 {
        return 0.0;
    }
    if eta <= stations[0] {
        return values[0];
    }
    if eta >= stations[n - 1] {
        return values[n - 1];
    }
    let upper = stations[..n]
        .partition_point(|&station| station < eta)
        .min(n - 1);
    let lower = upper.saturating_sub(1);
    let span = stations[upper] - stations[lower];
    if span <= 0.0 {
        return values[upper];
    }
    let t = (eta - stations[lower]) / span;
    values[lower] + t * (values[upper] - values[lower])
}
