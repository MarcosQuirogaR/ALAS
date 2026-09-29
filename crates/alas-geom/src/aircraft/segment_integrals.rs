// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Per-segment planform integrals shared by every mean-aerodynamic-chord
//! and MAC-station computation on a linearly-tapered wing panel.
//!
//! Each quantity is computed directly from `int c ds` and `int c^2 ds`
//! rather than through a taper ratio (`c1 / c0`), so a segment whose
//! inboard chord is zero does not divide by zero.

/// The planform integrals of one linearly-tapered wing segment: area, its
/// own mean aerodynamic chord length, and the chord-weighted fraction (from
/// the inboard end) at which any linearly-interpolated quantity's local MAC
/// centroid sits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SegmentIntegrals {
    /// `span * (c0 + c1) / 2`: `int c ds`.
    pub area: f64,
    /// `(2/3) * int c^2 ds / int c ds`, evaluated as
    /// `span * (c0^2 + c0*c1 + c1^2) / 3 / area`. Algebraically identical to
    /// the taper-ratio form `(2/3) * c0 * (1 + t + t^2) / (1 + t)` for
    /// `c0 > 0`, but never divides by `c0`.
    pub mac_length: f64,
    /// `(c0 + 2*c1) / (3*(c0 + c1))`: the fraction of the segment, from the
    /// inboard end, at which any quantity that varies linearly across the
    /// segment (leading-edge x, y, z) sits at its own chord-weighted
    /// centroid.
    pub le_fraction: f64,
}

impl SegmentIntegrals {
    /// The integrals of a segment of (non-negative) spanwise length `span`
    /// between inboard chord `c0` and outboard chord `c1`.
    ///
    /// Degenerates to `area: 0.0, mac_length: 0.0, le_fraction: 0.5` when
    /// both chords are zero: there is no chord to weight by, so a neutral
    /// midpoint fraction avoids propagating a `0/0`. Builder-produced wings
    /// never reach this branch (chords are validated positive, see
    /// `alas-config`'s `NonPositiveChord`); it defends CPACS imports and
    /// tail geometry scaled to zero.
    pub(crate) fn of_segment(span: f64, c0: f64, c1: f64) -> Self {
        let chord_sum = c0 + c1;
        let area = span * chord_sum / 2.0;
        let mac_length = if area > 0.0 {
            span * (c0 * c0 + c0 * c1 + c1 * c1) / 3.0 / area
        } else {
            0.0
        };
        Self {
            area,
            mac_length,
            le_fraction: Self::le_fraction(c0, c1),
        }
    }

    /// `(c0 + 2*c1) / (3*(c0 + c1))` alone, for a caller (e.g.
    /// [`Wing::aerodynamic_center`](super::wing::Wing::aerodynamic_center))
    /// that only needs the fraction, not the area or MAC length a `span`
    /// would otherwise require. `0.5` when both chords are zero, for the
    /// same reason [`Self::of_segment`] does.
    pub(crate) fn le_fraction(c0: f64, c1: f64) -> f64 {
        let chord_sum = c0 + c1;
        if chord_sum > 0.0 {
            (c0 + 2.0 * c1) / (3.0 * chord_sum)
        } else {
            0.5
        }
    }

    /// `numerator / denominator`, or `0.0` when `denominator` is not
    /// positive: [`Wing::mean_aerodynamic_chord`](super::wing::
    /// Wing::mean_aerodynamic_chord)'s area-weighted division, factored out
    /// so that call site stays `NaN`-free without repeating the guard.
    pub(crate) fn divide_or_zero(numerator: f64, denominator: f64) -> f64 {
        if denominator > 0.0 {
            numerator / denominator
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_taper_ratio_closed_form_for_a_non_degenerate_segment() {
        let integrals = SegmentIntegrals::of_segment(20.0, 4.0, 1.0);
        let taper = 1.0 / 4.0_f64;
        let expected_mac = (2.0 / 3.0) * 4.0 * (1.0 + taper + taper * taper) / (1.0 + taper);
        let expected_fraction = (1.0 + 2.0 * taper) / (3.0 + 3.0 * taper);
        assert!((integrals.mac_length - expected_mac).abs() / expected_mac < 1e-12);
        assert!((integrals.le_fraction - expected_fraction).abs() / expected_fraction < 1e-12);
        assert!((integrals.area - 20.0 * (4.0 + 1.0) / 2.0).abs() < 1e-12);
    }

    #[test]
    fn a_zero_inboard_chord_no_longer_produces_nan() {
        let integrals = SegmentIntegrals::of_segment(10.0, 0.0, 2.0);
        assert!(integrals.mac_length.is_finite());
        assert!(integrals.le_fraction.is_finite());
        // Tip-only chord: the exact rectangle/triangle-tip closed form is
        // (2/3) * c1.
        assert!((integrals.mac_length - (2.0 / 3.0) * 2.0).abs() < 1e-12);
    }

    #[test]
    fn both_chords_zero_is_nan_free_with_a_neutral_fraction() {
        let integrals = SegmentIntegrals::of_segment(10.0, 0.0, 0.0);
        assert_eq!(integrals.area, 0.0);
        assert_eq!(integrals.mac_length, 0.0);
        assert_eq!(integrals.le_fraction, 0.5);
    }
}
