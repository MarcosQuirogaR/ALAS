// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Resolved airfoil outlines placed in aircraft axes.
//!
//! A wing cross-section stores its airfoil in the normalized unit-chord
//! frame. Renderers that must show the actual section shape (camber,
//! thickness, taper and twist together) need those coordinates carried
//! through the same local frame the mesher uses, so that what is drawn is
//! what the solvers loft. This module exposes that transform without opening
//! the crate-private frame computation itself.
//!
//! Conventions: aircraft axes are `x` aft along the fuselage, `y` toward the
//! right wing tip, `z` up. A cross-section airfoil `(x/c, z/c)` point maps
//! to `xyz_le + xg * (x/c * chord) + zg * (z/c * chord)`, where `xg` and `zg`
//! are the section chordwise and normal directions after its twist
//! rotation about the local spanwise axis. A symmetric wing is mirrored by
//! negating `y`.

use super::wing::Wing;

impl Wing {
    /// The resolved outline of cross-section `index` in aircraft axes.
    ///
    /// Points follow the airfoil ordering (upper-surface trailing edge,
    /// forward over the upper surface, around the leading edge, aft along
    /// the lower surface). `max_points` bounds the returned count for
    /// interactive rendering: the leading edge and both trailing-edge points
    /// are always kept, and the remaining points are sampled uniformly by
    /// index so upper and lower surfaces keep the same density. `None` keeps
    /// every stored coordinate.
    ///
    /// Returns an empty vector when the wing has fewer than two
    /// cross-sections, which is not a loftable wing.
    pub fn section_outline(&self, index: usize, max_points: Option<usize>) -> Vec<[f64; 3]> {
        if self.xsecs.len() < 2 || index >= self.xsecs.len() {
            return Vec::new();
        }
        let airfoil = &self.xsecs[index].airfoil;
        let coordinates = &airfoil.coordinates;
        if coordinates.is_empty() {
            return Vec::new();
        }
        let le = airfoil.le_index();
        let keep = decimation_indices(coordinates.len(), le, max_points);
        keep.into_iter()
            .map(|i| {
                let (x_over_c, z_over_c) = coordinates[i];
                self.xyz_of_xsec(index, x_over_c, z_over_c)
            })
            .collect()
    }

    /// Every cross-section outline, root to tip, with the same point count
    /// per section so consecutive outlines can be joined into a surface.
    ///
    /// The per-section count is the smallest decimated count over all
    /// sections, which keeps neighbouring sections index-aligned even when
    /// their stored airfoils have different resolutions.
    pub fn section_outlines(&self, max_points: Option<usize>) -> Vec<Vec<[f64; 3]>> {
        if self.xsecs.len() < 2 {
            return Vec::new();
        }
        let per_section: Vec<Vec<[f64; 3]>> = (0..self.xsecs.len())
            .map(|index| self.section_outline(index, max_points))
            .collect();
        let count = per_section.iter().map(Vec::len).min().unwrap_or(0);
        if count < 3 {
            return per_section;
        }
        per_section
            .into_iter()
            .map(|outline| resample_by_index(&outline, count))
            .collect()
    }
}

/// Mirror a point about the aircraft symmetry plane (`y = 0`).
pub fn mirror_y(point: [f64; 3]) -> [f64; 3] {
    [point[0], -point[1], point[2]]
}

/// Indices to keep so the outline has at most `max_points` entries while the
/// first point, the leading-edge point and the last point survive.
fn decimation_indices(len: usize, le: usize, max_points: Option<usize>) -> Vec<usize> {
    let Some(max_points) = max_points.filter(|&m| m < len) else {
        return (0..len).collect();
    };
    let max_points = max_points.max(3);
    let mut indices = Vec::with_capacity(max_points);
    // Sample each surface independently so a thin trailing edge keeps both
    // its upper and lower points and the leading edge is shared exactly once.
    let upper_budget = (max_points / 2).max(2);
    let lower_budget = (max_points - upper_budget + 1).max(2);
    indices.extend(sample_range(0, le, upper_budget));
    indices.extend(sample_range(le, len - 1, lower_budget).into_iter().skip(1));
    indices.dedup();
    indices
}

/// `count` indices from `start` to `end` inclusive, uniformly spaced, always
/// including both ends.
fn sample_range(start: usize, end: usize, count: usize) -> Vec<usize> {
    if end <= start {
        return vec![start];
    }
    let span = end - start;
    let count = count.max(2).min(span + 1);
    (0..count)
        .map(|k| start + (k * span + (count - 1) / 2) / (count - 1))
        .collect()
}

/// Reduce an outline to exactly `count` points by uniform index sampling.
fn resample_by_index(outline: &[[f64; 3]], count: usize) -> Vec<[f64; 3]> {
    if outline.len() <= count {
        return outline.to_vec();
    }
    sample_range(0, outline.len() - 1, count)
        .into_iter()
        .map(|i| outline[i])
        .collect()
}

// Tests assert on values they construct here, so a failed expect is the
// assertion failing, not a library invariant being broken.
#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;
    use crate::aircraft::airfoil::Airfoil;
    use crate::aircraft::wing::WingXSec;

    fn straight_wing(twist_deg: f64) -> Wing {
        let airfoil = Airfoil::from_name("naca0012").expect("closed-form NACA section");
        Wing::new(
            "Test",
            vec![
                WingXSec::new([0.0, 0.0, 0.0], 2.0, 0.0, airfoil.clone()),
                WingXSec::new([1.0, 10.0, 0.0], 1.0, twist_deg, airfoil),
            ],
            true,
        )
    }

    #[test]
    fn root_outline_is_the_airfoil_scaled_by_chord_at_the_leading_edge() {
        let wing = straight_wing(0.0);
        let outline = wing.section_outline(0, None);
        let airfoil = &wing.xsecs[0].airfoil;
        assert_eq!(outline.len(), airfoil.coordinates.len());
        for (point, &(x, z)) in outline.iter().zip(&airfoil.coordinates) {
            assert!((point[0] - 2.0 * x).abs() < 1e-12);
            assert!(point[1].abs() < 1e-12);
            assert!((point[2] - 2.0 * z).abs() < 1e-12);
        }
    }

    #[test]
    fn tip_outline_is_placed_at_the_tip_leading_edge_and_scaled_by_tip_chord() {
        let wing = straight_wing(0.0);
        let outline = wing.section_outline(1, None);
        let le = wing.xsecs[1].airfoil.le_index();
        let le_point = outline[le];
        assert!((le_point[0] - 1.0).abs() < 1e-9);
        assert!((le_point[1] - 10.0).abs() < 1e-9);
        assert!(le_point[2].abs() < 1e-9);
        let te = outline[0];
        assert!(
            (te[0] - 2.0).abs() < 1e-6,
            "trailing edge sits one tip chord aft"
        );
    }

    #[test]
    fn twist_rotates_the_section_about_its_spanwise_axis() {
        let untwisted = straight_wing(0.0).section_outline(1, None);
        let twisted = straight_wing(-5.0).section_outline(1, None);
        let le = straight_wing(0.0).xsecs[1].airfoil.le_index();
        // The leading edge is the rotation origin; the trailing edge moves.
        assert!((untwisted[le][2] - twisted[le][2]).abs() < 1e-9);
        assert!((untwisted[0][2] - twisted[0][2]).abs() > 0.05);
        let chord_vector = [
            twisted[0][0] - twisted[le][0],
            twisted[0][2] - twisted[le][2],
        ];
        let angle = chord_vector[1].atan2(chord_vector[0]).to_degrees();
        assert!(
            (angle.abs() - 5.0).abs() < 0.6,
            "section chord rotated by the twist: {angle}"
        );
    }

    #[test]
    fn decimation_keeps_leading_and_trailing_edges_and_bounds_the_count() {
        let wing = straight_wing(0.0);
        let full = wing.section_outline(0, None);
        let reduced = wing.section_outline(0, Some(24));
        assert!(reduced.len() <= 24 && reduced.len() >= 3);
        assert_eq!(reduced[0], full[0]);
        assert_eq!(reduced[reduced.len() - 1], full[full.len() - 1]);
        let le = wing.xsecs[0].airfoil.le_index();
        assert!(reduced
            .iter()
            .any(|p| (p[0] - full[le][0]).abs() < 1e-12 && (p[2] - full[le][2]).abs() < 1e-12));
    }

    #[test]
    fn aligned_outlines_share_one_point_count() {
        let wing = straight_wing(0.0);
        let outlines = wing.section_outlines(Some(30));
        assert_eq!(outlines.len(), 2);
        assert_eq!(outlines[0].len(), outlines[1].len());
    }

    #[test]
    fn mirroring_negates_only_the_spanwise_coordinate() {
        assert_eq!(mirror_y([1.0, 2.0, 3.0]), [1.0, -2.0, 3.0]);
    }
}
