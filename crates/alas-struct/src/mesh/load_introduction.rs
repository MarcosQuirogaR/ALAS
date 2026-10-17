// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Which shell elements sit in a load-introduction or constraint zone.
//!
//! A shell finite element's stress next to a point load, a rigid (`RBE3`)
//! spider or a clamped edge is not a structural stress: it is the response of
//! the idealisation to a concentrated action, it grows without bound as the
//! mesh is refined, and Saint-Venant's principle says the real field is the
//! statically equivalent smooth one a short distance away. The 2026-10-07
//! A320 wingbox pull-up peak of 835 MPa was a single transition-rib element
//! whose four corners were all `RBE3` reference grids (152 MPa at its centroid
//! and 5.5 times lower than its corner value); the 10 t engine `CONM2`, the
//! rivet spiders and the clamped root rib produced the next-highest values.
//!
//! The strength assessment therefore recovers stress *outside* these zones.
//! A zone is the shells that share a grid with one of
//!
//! * an `RBE3` reference grid or any of its independent grids (rivets,
//!   engine and load-introduction spiders),
//! * a grid with an `SPC1` constraint (the root clamp),
//! * a grid carrying a wing-mounted engine `CONM2`,
//!
//! grown outward by [`LOAD_INTRODUCTION_LAYERS`] element layers in total. Three
//! layers is the usual finite-element rule of thumb that stress recovered fewer
//! than two to three element sizes from a point action is unreliable; it is a
//! convention of this assessment, not a measured decay length (for the A320
//! mesh three layers span about 0.3 m against a 0.65 m local box depth), and
//! the elements it removes are reported separately so a reviewer can see them.

use std::collections::BTreeSet;

use super::Deck;

/// Element layers excluded around each load-introduction or constraint grid:
/// the shells touching the seed grids plus this many minus one further rings.
pub const LOAD_INTRODUCTION_LAYERS: usize = 3;

/// Smallest share of a case's stress samples that must remain outside the
/// excluded zones for an assessed far-field metric to exist. Below it (or with
/// no finite assessed sample) the metric is unavailable, never zero.
pub const MIN_ASSESSED_SHARE: f64 = 0.25;

/// Peak of the assessed (non-excluded) samples of one load case, or `None`
/// when the metric is unavailable: no finite assessed sample, or fewer than
/// [`MIN_ASSESSED_SHARE`] of all samples assessed. `samples` yields
/// `(excluded, stress)` for every element sample of the case.
///
/// An empty or fully excluded case must not read as 0 Pa: the consumer
/// (`has_all_cases` in the feasibility gate) then falls back to the raw peak.
pub fn assessed_peak(samples: impl IntoIterator<Item = (bool, f64)>) -> Option<f64> {
    let (mut total, mut assessed, mut peak) = (0_usize, 0_usize, f64::NEG_INFINITY);
    for (excluded, stress) in samples {
        total += 1;
        if !excluded && stress.is_finite() {
            assessed += 1;
            peak = peak.max(stress);
        }
    }
    (assessed > 0 && (assessed as f64) >= MIN_ASSESSED_SHARE * total as f64).then_some(peak)
}

impl Deck {
    /// Grids at which a concentrated action enters or leaves the shell model.
    ///
    /// `engine_nids` are the grids the wing-mounted engine point masses hang
    /// from (`MeshNodeIndex::engine_nids`). Distributed fuel and equipment
    /// masses are deliberately not seeds: they are spread over many grids and
    /// introduce no concentrated action.
    pub fn load_introduction_grids(&self, engine_nids: &[i64]) -> BTreeSet<i64> {
        let mut seeds: BTreeSet<i64> = engine_nids.iter().copied().collect();
        for rigid in &self.rigid_elements {
            seeds.insert(rigid.refgrid);
            seeds.extend(rigid.gijs.iter().copied());
        }
        for constraint in &self.constraints {
            seeds.extend(constraint.nodes.iter().copied());
        }
        seeds
    }

    /// Identifiers of the shells inside a load-introduction or constraint zone:
    /// those within `layers` element layers of a
    /// [`load_introduction_grids`](Self::load_introduction_grids) grid (layer
    /// one touches a seed grid, layer two touches a grid of layer one, ...).
    pub fn load_introduction_elements(&self, engine_nids: &[i64], layers: usize) -> BTreeSet<i64> {
        let mut reached = self.load_introduction_grids(engine_nids);
        let mut excluded = BTreeSet::new();
        for _ in 0..layers {
            let ring: Vec<_> = self
                .quads
                .iter()
                .chain(&self.trias)
                .filter(|shell| {
                    !excluded.contains(&shell.eid)
                        && shell.nodes.iter().any(|nid| reached.contains(nid))
                })
                .collect();
            if ring.is_empty() {
                break;
            }
            for shell in ring {
                excluded.insert(shell.eid);
                reached.extend(shell.nodes.iter().copied());
            }
        }
        excluded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cards::{Rbe3, Shell, Spc1};

    /// A strip of `n` quads along x: nodes 1..=2(n+1), quad k joins columns k, k+1.
    fn strip(n: i64) -> Deck {
        let mut deck = Deck::new();
        for column in 0..=n {
            deck.add_grid(2 * column + 1, [column as f64, 0.0, 0.0]);
            deck.add_grid(2 * column + 2, [column as f64, 1.0, 0.0]);
        }
        for k in 0..n {
            deck.quads.push(Shell {
                eid: 100 + k,
                pid: 1,
                nodes: vec![2 * k + 1, 2 * k + 3, 2 * k + 4, 2 * k + 2],
            });
        }
        deck
    }

    #[test]
    fn layers_grow_from_a_rigid_spider_and_stop_at_the_requested_count() {
        let mut deck = strip(8);
        deck.rigid_elements.push(Rbe3 {
            eid: 1,
            refgrid: 9,
            refc: "123456",
            weight: 1.0,
            comp: "123456",
            gijs: vec![10],
        });
        // Grids 9 and 10 are column 4: quads 103 and 104 touch them.
        let one = deck.load_introduction_elements(&[], 1);
        assert_eq!(one.into_iter().collect::<Vec<_>>(), vec![103, 104]);
        let three = deck.load_introduction_elements(&[], 3);
        assert_eq!(
            three.into_iter().collect::<Vec<_>>(),
            vec![101, 102, 103, 104, 105, 106]
        );
        assert!(deck.load_introduction_elements(&[], 0).is_empty());
    }

    #[test]
    fn assessed_peak_is_unavailable_not_zero_for_empty_or_excluded_cases() {
        assert_eq!(assessed_peak([]), None);
        assert_eq!(assessed_peak([(true, 5.0), (true, 6.0)]), None);
        assert_eq!(assessed_peak([(false, f64::NAN)]), None);
        // One assessed sample out of five is below the minimum share.
        let sparse = [
            (false, 1.0),
            (true, 9.0),
            (true, 9.0),
            (true, 9.0),
            (true, 9.0),
        ];
        assert_eq!(assessed_peak(sparse), None);
        assert_eq!(
            assessed_peak([(false, 1.0), (false, 3.0), (true, 9.0)]),
            Some(3.0)
        );
    }

    #[test]
    fn engine_grids_and_constrained_grids_seed_zones_but_distributed_masses_do_not() {
        let mut deck = strip(6);
        deck.constraints.push(Spc1 {
            sid: 1,
            components: "123456",
            nodes: vec![1, 2],
        });
        deck.masses.push(crate::mesh::cards::Conm2 {
            eid: 900,
            nid: 13,
            cid: 0,
            mass: 5.0,
            offset: [0.0; 3],
        });
        // Root clamp seeds quad 100; the fuel-like CONM2 at grid 13 seeds nothing.
        let clamp_only = deck.load_introduction_elements(&[], 1);
        assert_eq!(clamp_only.into_iter().collect::<Vec<_>>(), vec![100]);
        // An engine hung from grid 13 (column 6, the last) seeds quad 105.
        let with_engine = deck.load_introduction_elements(&[13], 1);
        assert_eq!(with_engine.into_iter().collect::<Vec<_>>(), vec![100, 105]);
    }
}
