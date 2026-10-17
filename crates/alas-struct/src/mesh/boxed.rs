// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The boxed product deck: the spar window every rib keeps, and the pylon
//! idealisation engines hang from.

use alas_geom::wing_structure::RibStation;

use super::build::RibRegions;
use super::cards::{Conm2, Deck, Rbe3};
use super::nodes::{NodeMap, Surface};

/// The chordwise point range `(front spar, rear spar)` of a rib, when both of
/// the outermost spars reach it. The product deck keeps structural skin and rib
/// web only inside this range: the leading-edge and trailing-edge skins ahead
/// of and behind the box are secondary structure, outside the sized box and its
/// mass inventory (`sizing/solve.rs`), so they carry no stiffness here either.
pub(super) fn spar_window(station: &RibStation) -> Option<(usize, usize)> {
    let front = *station.j_spars.first()?;
    let rear = *station.j_spars.last()?;
    (front >= 0 && rear > front).then_some((front as usize, rear as usize))
}

/// First chordwise point of a rib web: 1 (the leading-edge point is shared by
/// both surfaces), or the front spar's point in the boxed product deck.
pub(super) fn boxed_front_start(station: &RibStation, boxed: bool) -> usize {
    match (boxed, station.j_spars.first()) {
        (true, Some(&point)) if point >= 1 => point as usize,
        _ => 1,
    }
}

/// Hang an engine of `mass` kg at its centre of gravity, `offset` m from the
/// front-spar lower grid `anchor` (basic frame), through a pylon idealisation:
/// a translation-only `RBE3` from the CG grid to the front-spar upper and lower
/// grids of the nearest structural rib and its two neighbours, i.e. the pylon
/// attachment frames on the front spar. An `RBE3` transmits the inertia load
/// without adding stiffness; a pylon is not stiffer than the box it hangs on,
/// and a single-node `CONM2` on a skin grid made the engine a spurious local
/// mode of the deck (MSC SOL 103 f1 1.34 Hz on the A320 against 2.57 Hz for the
/// wing bending mode). Pylon bending stiffness is not modelled; no sourced
/// value is available here.
#[allow(clippy::too_many_arguments)] // Deck, numbering, rib set, anchor, mass and offset.
pub(super) fn attach_engine_by_pylon_spider(
    deck: &mut Deck,
    eid: &mut i64,
    stations: &[RibStation],
    nodes: &NodeMap,
    regions: &RibRegions,
    nearest: usize,
    anchor: i64,
    mass: f64,
    offset: [f64; 3],
) {
    let base = deck.grid_xyz(anchor).unwrap_or([0.0; 3]);
    let cg = [
        base[0] + offset[0],
        base[1] + offset[1],
        base[2] + offset[2],
    ];
    let cg_nid = deck.grids.iter().map(|grid| grid.nid).max().unwrap_or(0) + 1;
    deck.add_grid(cg_nid, cg);
    deck.masses.push(Conm2 {
        eid: *eid,
        nid: cg_nid,
        cid: 0,
        mass,
        offset: [0.0; 3],
    });
    *eid += 1;

    let position = regions
        .all_structural
        .iter()
        .position(|&rib| rib == nearest)
        .unwrap_or(0);
    let first = position.saturating_sub(1);
    let last = (position + 1).min(regions.all_structural.len().saturating_sub(1));
    let mut independents: Vec<i64> = Vec::new();
    for &rib in &regions.all_structural[first..=last] {
        let Some(&point) = stations[rib].j_spars.first() else {
            continue;
        };
        if point < 0 {
            continue;
        }
        for surface in [Surface::Ext, Surface::Int] {
            let nid = nodes.at(rib, point as usize, surface);
            if nid != 0 && !independents.contains(&nid) {
                independents.push(nid);
            }
        }
    }
    if independents.len() >= 3 {
        deck.rigid_elements.push(Rbe3 {
            eid: *eid,
            refgrid: cg_nid,
            refc: "123456",
            weight: 1.0,
            comp: "123",
            gijs: independents,
        });
        *eid += 1;
    } else if let Some(mass_card) = deck.masses.last_mut() {
        // Fewer than three attachment grids cannot hold a translation-only
        // spider: keep the mass on the anchor grid with its offset instead.
        mass_card.nid = anchor;
        mass_card.offset = offset;
        deck.grids.retain(|grid| grid.nid != cg_nid);
    }
}
