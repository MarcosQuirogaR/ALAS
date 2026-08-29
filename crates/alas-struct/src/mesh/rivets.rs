// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tying a truncated rib to the skin around it.
//!
//! A transition rib is a rib the root plane cut short, so the skin panels --
//! which start at the first full-length rib -- do not reach it. Left alone it
//! is a stiff plate floating inside a wingbox, connected to the spar and
//! nothing else. Each of its grids is therefore made the dependent grid of a
//! weighted-average rigid element whose independents are the three nearest
//! full-length skin grids, so it moves and deforms with the skin instead.
//!
//! The search band is three nominal rib spacings either side. It widens to the
//! whole skin only when fewer than three grids fall inside it, which is the
//! reference's own fallback and keeps the element well-posed rather than
//! well-conditioned.

use alas_geom::wing_structure::RibStation;
use std::collections::HashSet;

use super::cards::{Deck, Rbe3};
use super::nodes::{NodeMap, Surface};
use super::Y_ROOT_EXCL;

/// How many independent grids each rivet averages over.
const RIVET_INDEPENDENTS: usize = 3;

/// How many rib spacings either side of a transition rib its independents are
/// looked for in.
const SEARCH_BAND_RIB_SPACINGS: f64 = 3.0;

/// Write one rivet per transition-rib grid, returning how many were written --
/// `_add_transition_rbe3`.
#[allow(clippy::too_many_arguments)] // Mirrors the reference's own signature.
pub(super) fn add_transition_rbe3(
    deck: &mut Deck,
    stations: &[RibStation],
    nodes: &NodeMap,
    skin_ribs: &HashSet<usize>,
    transition_ribs: &[usize],
    semi_span: f64,
    num_ribs: i64,
    first_eid: i64,
) -> usize {
    // The independents are gathered in placement order, because that is the
    // order two equally distant candidates are offered in.
    let mut skin: Vec<(i64, [f64; 3])> = Vec::new();
    let mut seen: HashSet<i64> = HashSet::new();
    for &((rib, _, _), nid) in nodes.placements() {
        if !skin_ribs.contains(&rib) || seen.contains(&nid) {
            continue;
        }
        let Some(xyz) = deck.grid_xyz(nid) else {
            continue;
        };
        // Grids on the constrained root cannot move, so averaging a rib's
        // motion over them would tie it to ground rather than to the skin.
        if xyz[1] < Y_ROOT_EXCL {
            continue;
        }
        seen.insert(nid);
        skin.push((nid, xyz));
    }

    if skin.is_empty() || transition_ribs.is_empty() {
        return 0;
    }

    let band = SEARCH_BAND_RIB_SPACINGS * semi_span / num_ribs.max(1) as f64;

    let mut eid = first_eid;
    let mut count = 0;
    let mut riveted: HashSet<i64> = HashSet::new();
    for &rib in transition_ribs {
        let station = &stations[rib];
        let last_spar = station.j_spars.last().map_or(-1, |&point| i64::from(point));
        let end = if !station.is_full && last_spar != -1 {
            last_spar
        } else {
            station.extrados.len() as i64 - 1
        };

        let mut local: Vec<(i64, [f64; 3])> = skin
            .iter()
            .copied()
            .filter(|(_, xyz)| (xyz[1] - station.y_station).abs() <= band)
            .collect();
        if local.len() < RIVET_INDEPENDENTS {
            local = skin.clone();
        }

        for surface in [Surface::Ext, Surface::Int] {
            for j in 1..=end.max(0) as usize {
                let Some(dependent) = nodes.get((rib, j, surface)) else {
                    continue;
                };
                if !riveted.insert(dependent) {
                    continue;
                }
                let Some(origin) = deck.grid_xyz(dependent) else {
                    continue;
                };

                let mut ranked: Vec<(f64, i64)> = local
                    .iter()
                    .map(|&(nid, xyz)| (distance(origin, xyz), nid))
                    .collect();
                // A stable sort by distance alone, so that two candidates at
                // the same distance stay in placement order. NumPy's own sort
                // here is unstable, and a tie would have to be exact for the
                // two to disagree.
                ranked.sort_by(|a, b| a.0.total_cmp(&b.0));

                deck.rigid_elements.push(Rbe3 {
                    eid,
                    refgrid: dependent,
                    refc: "123456",
                    weight: 1.0,
                    comp: "123456",
                    gijs: ranked
                        .iter()
                        .take(RIVET_INDEPENDENTS)
                        .map(|&(_, nid)| nid)
                        .collect(),
                });
                eid += 1;
                count += 1;
            }
        }
    }
    count
}

/// Euclidean distance between two points.
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_is_the_ordinary_euclidean_one() {
        assert!((distance([0.0; 3], [3.0, 4.0, 0.0]) - 5.0).abs() < 1e-15);
    }

    #[test]
    fn no_transition_ribs_means_no_rivets() {
        let mut deck = Deck::new();
        let nodes = NodeMap::new();
        let written = add_transition_rbe3(&mut deck, &[], &nodes, &HashSet::new(), &[], 10.0, 8, 1);
        assert_eq!(written, 0);
        assert!(deck.rigid_elements.is_empty());
    }
}
