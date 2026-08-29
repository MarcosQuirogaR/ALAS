// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from reference geometry/numpy/spacing.py
// Upstream: reference geometry 4.2.8, MIT.
// Reference: alas @ rust-port-baseline.

//! Point spacing along an interval: evenly spaced and cosine-spaced.
//!
//! Every non-CasADi call in `spacing.py` bottoms out in NumPy's own
//! `linspace`, so [`linspace`] reproduces that directly rather than
//! reimplementing a general-purpose numeric routine from scratch. [`cosspace`]
//! is `Airfoil.repanel`'s and `get_NACA_coordinates`'s spacing function --
//! cosine spacing bunches points near both ends of the interval, which is
//! what concentrates panels near an airfoil's leading and trailing edges.
//!
//! Both carry the same floating-point endpoint fixup the Python source does:
//! after generating the array from the closed-form trigonometric expression,
//! the first and last entries are overwritten with the exact `start`/`stop`
//! values, because the trigonometric round trip (`cos(linspace(pi, 0,
//! ...))`) does not always land on them bit-for-bit. For `num == 1` this
//! means the *second* write wins over the first (`stop` ends up written to
//! the only element, even though the first line asked for `start`) -- an
//! artifact of the two assignments sharing one index, kept here exactly as
//! upstream has it. `num == 0` is not reproduced as a crash; see
//! [`cosspace`].

/// Evenly spaced points from `start` to `stop`, inclusive, matching NumPy's
/// `linspace(start, stop, num, endpoint=True)`: `num - 1` equal steps, with
/// the last point forced to exactly `stop` rather than accumulated there.
pub fn linspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    if num == 0 {
        return Vec::new();
    }
    if num == 1 {
        return vec![start];
    }
    let step = (stop - start) / (num - 1) as f64;
    let mut values: Vec<f64> = (0..num).map(|i| start + i as f64 * step).collect();
    let last = values.len() - 1;
    values[last] = stop;
    values
}

/// Cosine-spaced points from `start` to `stop`: Chebyshev nodes remapped onto
/// `[start, stop]`, bunching points near both ends.
///
/// `num == 0` returns an empty vector rather than reproducing the upstream
/// `IndexError` the endpoint fixup would raise on an empty array -- nothing
/// in this crate's scope calls it that way, and a panic is not an option here
/// (see `CONTRIBUTING.md`).
pub fn cosspace(start: f64, stop: f64, num: usize) -> Vec<f64> {
    if num == 0 {
        return Vec::new();
    }
    let mean = (stop + start) / 2.0;
    let amp = (stop - start) / 2.0;
    let mut spaced: Vec<f64> = linspace(std::f64::consts::PI, 0.0, num)
        .into_iter()
        .map(|t| mean + amp * t.cos())
        .collect();

    // Order matters when `num == 1`: both indices are 0, so the second write
    // is the one that survives -- reproduced faithfully, see the module doc.
    spaced[0] = start;
    let last = spaced.len() - 1;
    spaced[last] = stop;
    spaced
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linspace_endpoints_are_exact() {
        let values = linspace(0.3, 7.9, 37);
        assert_eq!(values[0], 0.3);
        assert_eq!(values[36], 7.9);
    }

    #[test]
    fn linspace_of_one_point_returns_the_start() {
        assert_eq!(linspace(2.0, 5.0, 1), vec![2.0]);
    }

    #[test]
    fn linspace_of_zero_points_is_empty() {
        assert!(linspace(0.0, 1.0, 0).is_empty());
    }

    #[test]
    fn cosspace_endpoints_are_exact_and_interior_is_denser_near_the_ends() {
        let values = cosspace(0.0, 1.0, 11);
        assert_eq!(values[0], 0.0);
        assert_eq!(values[10], 1.0);
        // Chebyshev nodes: the first interior gap is smaller than the
        // midpoint gap, which is what "bunched near the ends" means.
        let first_gap = values[1] - values[0];
        let middle_gap = values[6] - values[5];
        assert!(first_gap < middle_gap);
    }

    #[test]
    fn cosspace_is_symmetric_about_the_midpoint() {
        let values = cosspace(-2.0, 3.0, 9);
        let mid = (-2.0 + 3.0) / 2.0;
        for i in 0..values.len() {
            let mirror = values.len() - 1 - i;
            assert!(
                ((values[i] - mid) + (values[mirror] - mid)).abs() < 1e-12,
                "i={i}"
            );
        }
    }

    #[test]
    fn cosspace_of_one_point_keeps_the_upstream_endpoint_fixup_order() {
        // Both endpoint writes land on index 0; the second (`stop`) wins.
        assert_eq!(cosspace(2.0, 5.0, 1), vec![5.0]);
    }
}
