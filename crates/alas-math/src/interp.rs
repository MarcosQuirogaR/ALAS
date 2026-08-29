// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from numpy/core/src/multiarray/compiled_base.c (`compiled_interp`,
// `binary_search_with_guess`).
// Upstream: NumPy, BSD-3-Clause.

//! NumPy's `np.interp`: piecewise-linear interpolation between ascending
//! stations, clamped rather than extrapolated outside them.
//!
//! This is reproduced from NumPy's own source rather than from its documented
//! behaviour, because the callers use it to decide things and not only to
//! smooth them: it is the fuselage width at a cabin station (which sets how
//! many people sit in a row) and the zero-lift angle of attack read off a
//! polar (which sets the whole reported alpha axis). The end clamping, the
//! exact-hit shortcut and the NaN retry are each a branch that a
//! two-line reimplementation would get subtly differently.
//!
//! It began private to `alas-payload::numeric`, which was the first module in
//! the port to need it; `alas-aero::analysis` is the second, and that crate's
//! own doc states the rule this move follows -- a primitive with a second
//! consumer lives here rather than being copied.

/// NumPy's `np.interp(x, xp, fp)` for one query point: piecewise-linear
/// interpolation, clamped to the end values outside `xp`.
///
/// `xp` must be ascending, which is what every caller's construction
/// guarantees. Returns NaN when `xp` is empty or `fp` is a different length,
/// where NumPy raises -- library code reports the surprise in its value
/// rather than panicking (`CONTRIBUTING.md`), and no caller here can reach
/// either case.
pub fn interp(x: f64, xp: &[f64], fp: &[f64]) -> f64 {
    let n = xp.len();
    if n == 0 || fp.len() != n {
        return f64::NAN;
    }
    if x.is_nan() {
        return x;
    }
    // NumPy's one-station branch answers with that station's value on either
    // side of it, since the left and right fill values both default to it.
    if n == 1 {
        return fp[0];
    }
    // Outside the data, `interp` returns the end value rather than
    // extrapolating.
    if x > xp[n - 1] {
        return fp[n - 1];
    }
    if x < xp[0] {
        return fp[0];
    }

    // The last index whose station is at or before `x`, as
    // `binary_search_with_guess` returns it.
    let mut low = 0usize;
    let mut high = n;
    while low < high {
        let mid = low + ((high - low) >> 1);
        if x >= xp[mid] {
            low = mid + 1;
        } else {
            high = mid;
        }
    }
    let j = low.saturating_sub(1);

    if j == n - 1 {
        return fp[n - 1];
    }
    if xp[j] == x {
        return fp[j];
    }

    let slope = (fp[j + 1] - fp[j]) / (xp[j + 1] - xp[j]);
    let mut result = slope * (x - xp[j]) + fp[j];
    if result.is_nan() {
        result = slope * (x - xp[j + 1]) + fp[j + 1];
        if result.is_nan() && fp[j] == fp[j + 1] {
            result = fp[j];
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interp_returns_the_end_values_outside_the_data() {
        let xp = [0.0, 1.0, 2.0];
        let fp = [10.0, 20.0, 40.0];
        assert_eq!(interp(-5.0, &xp, &fp), 10.0);
        assert_eq!(interp(9.0, &xp, &fp), 40.0);
    }

    #[test]
    fn interp_is_exact_on_the_stations_themselves() {
        let xp = [0.0, 1.0, 2.0];
        let fp = [10.0, 20.0, 40.0];
        for (x, expected) in xp.iter().zip(fp) {
            assert_eq!(interp(*x, &xp, &fp), expected);
        }
    }

    #[test]
    fn interp_is_linear_between_stations() {
        let xp = [0.0, 1.0, 2.0];
        let fp = [10.0, 20.0, 40.0];
        assert_eq!(interp(0.5, &xp, &fp), 15.0);
        assert_eq!(interp(1.25, &xp, &fp), 25.0);
    }

    #[test]
    fn a_single_station_answers_with_itself_everywhere() {
        assert_eq!(interp(-3.0, &[4.0], &[7.0]), 7.0);
        assert_eq!(interp(99.0, &[4.0], &[7.0]), 7.0);
    }

    #[test]
    fn interp_of_an_undefined_station_is_undefined() {
        assert!(interp(f64::NAN, &[0.0, 1.0], &[2.0, 3.0]).is_nan());
    }

    #[test]
    fn a_mismatched_or_empty_station_list_is_undefined() {
        assert!(interp(0.5, &[], &[]).is_nan());
        assert!(interp(0.5, &[0.0, 1.0], &[2.0]).is_nan());
    }
}
