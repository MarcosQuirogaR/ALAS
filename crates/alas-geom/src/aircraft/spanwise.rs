// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! One spanwise discretisation for a lofted surface, defined by a panel count
//! rather than by a per-section multiplier.
//!
//! # Why this exists beside `Wing::subdivide_sections`
//!
//! [`Wing::subdivide_sections`] is a faithful port of the reference's own
//! routine and stays exactly as it is. It multiplies: each of the surface's
//! `n - 1` lofted sections becomes `ratio` of them. That makes the resulting
//! panel count a consequence of how many cross-sections the design vector
//! happened to produce, which has two costs this module removes.
//!
//! The first is inconsistency between aircraft. A wing with an explicit
//! side-of-body station carries three sections and meshes to 24 strips per
//! semispan at `n_subdivisions = 8`; the same setting on a wing without one
//! carries two and meshes to 16. Two aircraft analysed at the same declared
//! fidelity were being panelled 50 % apart.
//!
//! The second is worse, because it moves inside a single search. Whether a
//! station exists can depend on the design vector, so two adjacent candidates
//! could differ by a whole section, and therefore by eight strips and,
//! measured, by about fifteen percent in induced drag. A search cannot
//! distinguish that discretisation step from a real aerodynamic gradient; it
//! will happily climb it.
//!
//! # What is preserved, and why that is the point
//!
//! A cross-section is not a mesh node. It is where the loft *changes*: the
//! side-of-body station, the Yehudi kink, the tip. Chord slope, twist and
//! airfoil are all discontinuous in derivative there, and a panel spanning
//! such a station averages across the discontinuity and erases it: the same
//! failure mode as meshing a cambered section with one chordwise panel, where
//! the Hicks-Henne bump variables become literally invisible.
//!
//! So every existing cross-section is retained as a panel edge, at every
//! panel count. Refining the mesh subdivides the sections between the
//! stations; it never moves a station and never merges two. A kink at 37 % of
//! semispan is at 37 % of semispan in a six-panel mesh and in a ninety-six
//! panel one, and [`Wing::spanwise_stations`] is the test hook that says so.
//!
//! # How the panels are allocated, and why not in proportion to span
//!
//! The count is split evenly between sections, with any remainder going to
//! the longest sections first. That is deliberately *not* proportional to
//! span, and the reason is measured rather than aesthetic.
//!
//! Spreading panels in proportion to span gives uniform panel width, which
//! is the textbook choice and reads better. On a swept transport planform it
//! is also markedly worse at any affordable count: the A320-200 reports a
//! near-field induced-drag factor of 0.0376 on a uniform 24-panel semispan
//! against a converged 0.0411, and needs about 96 panels to recover it.
//! Cosine spacing within each section behaves the same way: 0.0380 at 24,
//! 0.0411 at 96. An even split reaches 0.0411 at 24 panels and holds it
//! through a four-fold refinement.
//!
//! The reason is that an even split concentrates panels where the sections
//! are short, and a transport's sections are short exactly where its loading
//! changes fastest: the side-of-body station and the Yehudi kink. Uniform
//! width spends its budget on the outer wing, where the loading is smooth.
//! So the even split is not a compromise for the sake of the old numbers; it
//! is the distribution this planform family is converged on, and switching
//! to uniform width would have quadrupled the cost of the same answer.
//!
//! Every section receives at least one panel, because a section is a feature
//! and a feature with no panel is a feature that is not in the mesh. Ties in
//! the remainder go to the longer section, and then to the inboard one, so
//! the result is a deterministic function of the geometry and the requested
//! count: the same wing and count give the same mesh on every run.

use super::wing::{SpacingFunction, SubdivideSectionsError, Wing, WingXSec};

impl Wing {
    /// The spanwise station of every cross-section, as a fraction of the
    /// surface's total lofted extent, root to tip.
    ///
    /// This is what "the mesh preserves the planform" is checked against: the
    /// stations of a meshed wing must contain every station of the wing it
    /// was meshed from, at the same fractions.
    ///
    /// Returns an empty vector for a surface with fewer than two
    /// cross-sections, and all-zero fractions for one of zero extent.
    pub fn spanwise_stations(&self) -> Vec<f64> {
        let extents = self.section_extents();
        let total: f64 = extents.iter().sum();
        let mut stations = Vec::with_capacity(self.xsecs.len());
        let mut running = 0.0;
        if !self.xsecs.is_empty() {
            stations.push(0.0);
        }
        for extent in extents {
            running += extent;
            stations.push(if total > 0.0 { running / total } else { 0.0 });
        }
        stations
    }

    /// The lofted spanwise extent of each section, inboard to outboard.
    fn section_extents(&self) -> Vec<f64> {
        self.xsecs
            .windows(2)
            .map(|pair| {
                let (a, b) = (pair[0].xyz_le, pair[1].xyz_le);
                (b[1] - a[1]).hypot(b[2] - a[2])
            })
            .collect()
    }

    /// A copy of this surface meshed into `panels` spanwise panels per side,
    /// with every existing cross-section retained as a panel edge.
    ///
    /// `panels` is an absolute count for the whole surface, not a multiplier:
    /// the same number gives the same mesh density whether the planform has
    /// two lofted sections or four. A count below the number of sections
    /// cannot honour both the count and the stations, and the stations win:
    /// the result then has one panel per section. A surface with fewer than
    /// two cross-sections is returned unchanged.
    ///
    /// # Errors
    ///
    /// [`SubdivideSectionsError::Blend`] when a station between two
    /// structurally distinct airfoils needs them blended and that blend's
    /// `repanel` step fails.
    pub fn mesh_spanwise(
        &self,
        panels: usize,
        spacing_function: SpacingFunction,
    ) -> Result<Self, SubdivideSectionsError> {
        let sections = self.xsecs.len().saturating_sub(1);
        if sections == 0 {
            return Ok(self.clone());
        }
        let allocation = allocate(&self.section_extents(), panels.max(sections));

        let mut new_xsecs = Vec::with_capacity(allocation.iter().sum::<usize>() + 1);
        for (pair, &count) in self.xsecs.windows(2).zip(&allocation) {
            let (xsec_a, xsec_b) = (&pair[0], &pair[1]);
            // `count` fractions covering [0, 1): the outboard station is the
            // next section's inboard one, and the tip is appended once after
            // the loop, so including 1.0 here would duplicate every station.
            let full = spacing_function.spaced(0.0, 1.0, count + 1);
            for &s in &full[..full.len() - 1] {
                new_xsecs.push(interpolate(xsec_a, xsec_b, s)?);
            }
        }
        if let Some(last) = self.xsecs.last() {
            new_xsecs.push(last.clone());
        }

        Ok(Self {
            name: self.name.clone(),
            xsecs: new_xsecs,
            symmetric: self.symmetric,
        })
    }
}

/// One cross-section a fraction `s` of the way from `a` to `b`.
///
/// Identical in result to what [`Wing::subdivide_sections`] produces at the
/// same fraction: the linear loft of leading edge, chord and twist, and the
/// reference's own airfoil rule: carry the inboard section unless the two are
/// structurally distinct, and blend only then.
fn interpolate(a: &WingXSec, b: &WingXSec, s: f64) -> Result<WingXSec, SubdivideSectionsError> {
    let (a_weight, b_weight) = (1.0 - s, s);
    let airfoil = if a.airfoil == b.airfoil || a_weight == 1.0 {
        a.airfoil.clone()
    } else if b_weight == 1.0 {
        b.airfoil.clone()
    } else {
        a.airfoil.blend_with_another_airfoil(
            &b.airfoil,
            b_weight,
            super::wing::SUBDIVIDE_BLEND_N_POINTS_PER_SIDE,
        )?
    };
    Ok(WingXSec {
        xyz_le: super::vector3::blend3(a.xyz_le, b.xyz_le, a_weight, b_weight),
        chord: a.chord * a_weight + b.chord * b_weight,
        twist: a.twist * a_weight + b.twist * b_weight,
        airfoil,
    })
}

/// Split `panels` between sections of the given extents, every section
/// receiving at least one.
///
/// Evenly, with the remainder going to the longest sections first; see the
/// module doc for why this is not proportional to span. The total is exactly
/// `panels` whenever `panels >= extents.len()`, which the caller guarantees.
fn allocate(extents: &[f64], panels: usize) -> Vec<usize> {
    let sections = extents.len();
    let mut counts = vec![panels / sections; sections];
    let remainder = panels % sections;
    if remainder > 0 {
        // Longest section first, then inboard, so the extra panels land where
        // they buy the most resolution and the result stays reproducible.
        let mut order: Vec<usize> = (0..sections).collect();
        order.sort_by(|&left, &right| {
            extents[right]
                .partial_cmp(&extents[left])
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(left.cmp(&right))
        });
        for &index in order.iter().take(remainder) {
            counts[index] += 1;
        }
    }
    counts
}

#[cfg(test)]
#[path = "spanwise_tests.rs"]
mod tests;
