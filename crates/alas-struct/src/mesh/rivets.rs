// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Tying a truncated rib to the skin around it.
//!
//! A transition rib is a rib the root plane cut short, so the skin panels,
//! which start at the first full-length rib, do not reach it. Left alone it
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

/// Smallest `area / longest_edge^2` the three independents of a rivet may
/// span. A translation-only `RBE3` recovers the dependent grid's rotation from
/// the triangle of its independents, so a sliver (three nearly collinear
/// grids) cannot constrain the six rigid-body modes. MSC Nastran 2026.1
/// reported a singular element (USER FATAL MESSAGE 2038, RBE3S) at
/// 8.8e-14 and an equilibrium-matrix warning (7343) at 3.7e-6 on a
/// clean-sheet 300-seat wing, and accepted every element at or above 8.9e-4.
/// The value sits between the warned and the accepted triangles; the three
/// nearest grids are kept untouched whenever they already satisfy it.
const MIN_INDEPENDENT_TRIANGLE_QUALITY: f64 = 1.0e-4;

/// How many rib spacings either side of a transition rib its independents are
/// looked for in.
const SEARCH_BAND_RIB_SPACINGS: f64 = 3.0;

/// Write one rivet per transition-rib grid, returning how many were written:
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
    translations_only: bool,
) -> usize {
    // The independents are gathered in placement order, because that is the
    // order two equally distant candidates are offered in.
    let mut skin: Vec<(i64, [f64; 3])> = Vec::new();
    let mut seen: HashSet<i64> = HashSet::new();
    for &((rib, point, _), nid) in nodes.placements() {
        if !skin_ribs.contains(&rib) || seen.contains(&nid) {
            continue;
        }
        // Boxed deck: only skin inside the spar window exists to tie to.
        if translations_only {
            if let Some((front, rear)) = super::boxed::spar_window(&stations[rib]) {
                if point < front || point > rear {
                    continue;
                }
            }
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
            let first = super::boxed::boxed_front_start(station, translations_only);
            for j in first..=end.max(0) as usize {
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
                    // Independent rotations are weighted like translations, so a
                    // 123456 spider is not unit-invariant (NASTRAN-95 units.rs).
                    // The frozen reference deck keeps 123456.
                    comp: if translations_only { "123" } else { "123456" },
                    // Only the translation-only form needs the triangle: with
                    // independent rotations (the frozen reference deck) a
                    // sliver still constrains the six rigid-body modes.
                    gijs: if translations_only {
                        well_posed_independents(&ranked, &local)
                    } else {
                        ranked
                            .iter()
                            .take(RIVET_INDEPENDENTS)
                            .map(|&(_, nid)| nid)
                            .collect()
                    },
                });
                eid += 1;
                count += 1;
            }
        }
    }
    count
}

/// The three independents of a rivet: the nearest three grids, except that the
/// third is the nearest grid that makes a non-degenerate triangle when the
/// nearest three are (almost) collinear.
fn well_posed_independents(ranked: &[(f64, i64)], local: &[(i64, [f64; 3])]) -> Vec<i64> {
    let nearest: Vec<i64> = ranked
        .iter()
        .take(RIVET_INDEPENDENTS)
        .map(|&(_, nid)| nid)
        .collect();
    if nearest.len() < RIVET_INDEPENDENTS {
        return nearest;
    }
    let position = |nid: i64| local.iter().find(|(id, _)| *id == nid).map(|(_, xyz)| *xyz);
    let (Some(a), Some(b)) = (position(nearest[0]), position(nearest[1])) else {
        return nearest;
    };
    let quality_with = |nid: i64| position(nid).map(|c| triangle_quality(a, b, c));
    if quality_with(nearest[2]).is_some_and(|q| q >= MIN_INDEPENDENT_TRIANGLE_QUALITY) {
        return nearest;
    }
    // Ranked order is nearest first, so the first grid that opens the
    // triangle is the nearest one that does.
    let replacement = ranked
        .iter()
        .skip(RIVET_INDEPENDENTS)
        .map(|&(_, nid)| nid)
        .find(|&nid| quality_with(nid).is_some_and(|q| q >= MIN_INDEPENDENT_TRIANGLE_QUALITY));
    match replacement {
        Some(nid) => vec![nearest[0], nearest[1], nid],
        None => nearest,
    }
}

/// Triangle area over the square of its longest edge: dimensionless, zero for
/// collinear points, 0.433 for an equilateral triangle.
fn triangle_quality(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    let area = 0.5 * (cross[0].powi(2) + cross[1].powi(2) + cross[2].powi(2)).sqrt();
    let longest = distance(a, b).max(distance(a, c)).max(distance(b, c));
    if longest > 0.0 {
        area / (longest * longest)
    } else {
        0.0
    }
}

/// Euclidean distance between two points.
fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collinear_nearest_grids_get_a_third_that_opens_the_triangle() {
        // 1, 2 and 3 lie on one line; 4 is farther but off it.
        let local = vec![
            (1, [0.0, 0.0, 0.0]),
            (2, [0.0, 0.0, 0.05]),
            (3, [0.0, 0.0, 0.10]),
            (4, [1.0, 0.0, 0.0]),
        ];
        let ranked = vec![(0.0, 1), (0.05, 2), (0.10, 3), (1.0, 4)];
        assert_eq!(well_posed_independents(&ranked, &local), vec![1, 2, 4]);
        // A healthy nearest-three set is returned unchanged.
        let healthy = vec![
            (1, [0.0, 0.0, 0.0]),
            (2, [1.0, 0.0, 0.0]),
            (3, [0.0, 1.0, 0.0]),
            (4, [5.0, 5.0, 0.0]),
        ];
        let ranked = vec![(0.0, 1), (1.0, 2), (1.0, 3), (7.0, 4)];
        assert_eq!(well_posed_independents(&ranked, &healthy), vec![1, 2, 3]);
        assert!(
            (triangle_quality([0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]) - 0.25).abs() < 1e-12
        );
    }

    #[test]
    fn no_transition_ribs_means_no_rivets() {
        let mut deck = Deck::new();
        let nodes = NodeMap::new();
        let written = add_transition_rbe3(
            &mut deck,
            &[],
            &nodes,
            &HashSet::new(),
            &[],
            10.0,
            8,
            1,
            false,
        );
        assert_eq!(written, 0);
        assert!(deck.rigid_elements.is_empty());
    }
}
