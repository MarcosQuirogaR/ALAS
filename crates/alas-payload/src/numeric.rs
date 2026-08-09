// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The three reference-library primitives the layout engines depend on for
//! their exact answers: CPython's `//` and `round`, and NumPy's `interp`.
//!
//! None of these is arithmetic Rust spells the same way, and each of them
//! decides a whole seat, a whole container or a whole exit rather than a last
//! digit. `int((usable - aisle_w) // seat_w)` is how many people sit in a row;
//! `round(pct * l_seating / pitch)` is how many rows a class gets;
//! `np.interp` is the fuselage width at a station, which feeds both. Writing
//! `(a / b).floor()` and `x.round()` instead would agree almost everywhere and
//! disagree, by one, exactly at the tie -- which is where a cabin gains or
//! loses a seat abreast.
//!
//! So each is reproduced from the reference implementation's own source rather
//! than from its documented behaviour:
//!
//! * [`floor_div`] is CPython's `float_divmod` (`Objects/floatobject.c`),
//!   which computes the quotient from `fmod` and then snaps it, rather than
//!   flooring `a / b` directly.
//! * [`round_half_even`] is CPython's `float.__round__` with no digit count,
//!   which rounds half away from zero and then corrects the exact halves.
//! * [`round_to_digit`] is `round(x, ndigits)`, which is a correctly-rounded
//!   decimal conversion out and back rather than a scale-round-unscale.
//! * [`interp`] is NumPy's `compiled_interp` together with the
//!   `binary_search_with_guess` that locates the interval, including its
//!   clamping at both ends and its NaN retry.
//!
//! These are private to this crate because it is the first thing in the port
//! to need them. If a second module does, they belong in `alas-math` beside
//! the other numerical primitives, not copied.

/// Python's `a // b` for floats -- CPython's `float_divmod`, whose quotient
/// comes from `fmod` and is then snapped to the nearest integral value.
///
/// The difference from `(a / b).floor()` is real and is why this exists: `a /
/// b` can round *up* to an integer when the exact quotient is a hair below
/// one, and flooring that gives an answer one too large.
///
/// Division by zero returns NaN, where Python raises; no caller here divides
/// by a quantity that can be zero (every divisor is a seat width, a container
/// width or an exit spacing, each floored at a positive minimum first).
pub(crate) fn floor_div(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        return f64::NAN;
    }
    let modulus = a % b;
    let mut div = (a - modulus) / b;
    if modulus != 0.0 {
        // Give the remainder the denominator's sign, as Python's `%` does.
        if (b < 0.0) != (modulus < 0.0) {
            div -= 1.0;
        }
    }
    if div != 0.0 {
        let floored = div.floor();
        // `div` is only approximately integral, so a value that landed just
        // under the true quotient is snapped back up rather than floored away.
        if div - floored > 0.5 {
            floored + 1.0
        } else {
            floored
        }
    } else {
        0.0
    }
}

/// Python's `round(x)` with no digit count: nearest integer, halves to even --
/// `float.__round__`, reproduced as the two steps it takes.
pub(crate) fn round_half_even(x: f64) -> f64 {
    let rounded = x.round();
    if (x - rounded).abs() == 0.5 {
        2.0 * (x / 2.0).round()
    } else {
        rounded
    }
}

/// Python's `round(x, ndigits)`: the value correctly rounded to `ndigits`
/// decimal places, halves to even.
///
/// CPython converts to a decimal string of that length and reads it back,
/// rather than scaling by a power of ten -- the scaling is not exact in binary
/// and rounds the wrong way for values that are not representable. Rust's
/// fixed-precision formatting is the same correctly-rounded, ties-to-even
/// conversion, so the same out-and-back reproduces it.
pub(crate) fn round_to_digit(x: f64, ndigits: usize) -> f64 {
    if !x.is_finite() {
        return x;
    }
    format!("{x:.ndigits$}").parse().unwrap_or(x)
}

/// NumPy's `np.interp(x, xp, fp)` for one query point: piecewise-linear
/// interpolation, clamped to the end values outside `xp`.
///
/// `xp` must be ascending, which is what the caller's sort guarantees. The
/// interval search, the exact-hit shortcut and the NaN retry all follow
/// NumPy's `compiled_interp`; the retry matters because it is the only reason
/// an infinite ordinate does not poison a query that lands on a station.
pub(crate) fn interp(x: f64, xp: &[f64], fp: &[f64]) -> f64 {
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
    // extrapolating: the fuselage does not continue past its last station.
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
    fn floor_div_answers_from_the_remainder_not_from_the_quotient() {
        // 1.0 / 0.1 rounds up to exactly 10 in binary, so flooring it answers
        // 10; the true quotient is a hair under, and the remainder-based
        // computation Python does answers 9. One seat abreast, or one whole
        // container along a hold.
        assert_eq!(floor_div(1.0, 0.1), 9.0);
        assert_eq!((1.0_f64 / 0.1).floor(), 10.0);
        assert_eq!(floor_div(3.0, 0.1), 29.0);
        assert_eq!(floor_div(4.35, 0.435), 9.0);
    }

    #[test]
    fn floor_div_matches_ordinary_flooring_where_nothing_is_at_stake() {
        assert_eq!(floor_div(7.0, 2.0), 3.0);
        assert_eq!(floor_div(6.0, 2.0), 3.0);
        assert_eq!(floor_div(0.9, 2.0), 0.0);
    }

    #[test]
    fn floor_div_of_a_negative_numerator_floors_rather_than_truncates() {
        // A usable width narrower than the aisle gives a negative numerator,
        // and Python's floor division answers -1, not 0.
        assert_eq!(floor_div(-0.2, 0.46), -1.0);
        assert_eq!(floor_div(-1.0, 0.5), -2.0);
    }

    #[test]
    fn rounding_a_half_goes_to_the_even_neighbour() {
        assert_eq!(round_half_even(0.5), 0.0);
        assert_eq!(round_half_even(1.5), 2.0);
        assert_eq!(round_half_even(2.5), 2.0);
        assert_eq!(round_half_even(-0.5), -0.0);
        assert_eq!(round_half_even(-1.5), -2.0);
    }

    #[test]
    fn rounding_away_from_a_half_is_ordinary_nearest() {
        assert_eq!(round_half_even(2.4), 2.0);
        assert_eq!(round_half_even(2.6), 3.0);
        assert_eq!(round_half_even(-2.6), -3.0);
    }

    #[test]
    fn rounding_to_a_digit_reads_the_decimal_value_not_the_scaled_one() {
        // 2.675 is stored a hair below its decimal self, so a correctly
        // rounded conversion answers 2.67 where scale-round-unscale answers
        // 2.68. Python answers 2.67.
        assert_eq!(round_to_digit(2.675, 2), 2.67);
        assert_eq!(round_to_digit(100.0, 1), 100.0);
        assert_eq!(round_to_digit(66.66666666666667, 1), 66.7);
        assert_eq!(round_to_digit(0.25, 1), 0.2);
    }

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
}
