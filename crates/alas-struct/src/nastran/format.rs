// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Rendering numbers the way the solution decks do.
//!
//! Every card these decks write goes through [`free_field`], and it exists for
//! a specific failure: a free-field real without a decimal point is read as an
//! integer, and a card that hands NASTRAN `500` where it wants `500.` fails the
//! run with USER FATAL 9994. The reference's own `_f` is reproduced exactly,
//! including its second format: where `%.8g` would use an exponent it re-renders
//! through `%.6f` and trims, because classic free field is happier without one.
//!
//! That second path has a quirk worth naming, since it is reproduced rather
//! than fixed: a magnitude below about 1e-7 renders as `0.0`, the value having
//! fallen off the end of six decimal places. Nothing in these decks reaches it
//! -- the smallest number any of them carries is a damping ratio -- so it is
//! recorded here rather than as a `deviation-candidate` in the ledger, which is
//! for behaviour that is reached.

/// Significant digits the reference's `%.8g` asks for.
const GENERAL_PRECISION: usize = 8;

/// Decimal places its fallback `%.6f` asks for.
const FALLBACK_DECIMALS: usize = 6;

/// One free-field real, guaranteed to carry a decimal point -- `_f`.
///
/// Public because it is part of this module's contract rather than an
/// implementation detail: every card these decks write is rendered by it, a
/// caller adding one has to render numbers the same way, and the parity test
/// holds it directly to the reference's own table of values.
pub fn free_field(value: f64) -> String {
    if value == 0.0 {
        return "0.".to_string();
    }
    let general = general(value);
    if general.contains('e') {
        let mut text = format!("{value:.FALLBACK_DECIMALS$}");
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.push('0');
        }
        return text;
    }
    if general.contains('.') {
        general
    } else {
        format!("{general}.")
    }
}

/// `value` under C's `%.8g`: fixed notation with trailing zeros trimmed, or
/// exponent notation when the exponent falls outside `[-4, 8)`.
///
/// Only the fixed result is ever used as text -- [`free_field`] re-renders the
/// other case -- so the exponent branch returns the scientific string purely as
/// the marker that it was taken.
fn general(value: f64) -> String {
    let scientific = format!("{value:.*e}", GENERAL_PRECISION - 1);
    let exponent: i32 = scientific
        .split('e')
        .nth(1)
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);
    if exponent < -4 || exponent >= GENERAL_PRECISION as i32 {
        return scientific;
    }
    let decimals = (GENERAL_PRECISION as i32 - 1 - exponent).max(0) as usize;
    let mut text = format!("{value:.decimals$}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
    }
    text
}

/// `value` as Python's `str` renders it, which is what the decks' comment lines
/// interpolate a configured frequency or spectral density with.
///
/// Rust and Python both print the shortest string that reads back exactly, and
/// they differ in two places: Python writes a trailing `.0` on a whole number,
/// and it switches to an exponent outside `[1e-4, 1e16)` where Rust spells the
/// number out. Both differences are reproduced, because the lines these values
/// land in are compared as text.
pub(super) fn python_str(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() {
            "-0.0".to_string()
        } else {
            "0.0".to_string()
        };
    }
    let magnitude = value.abs();
    if !(1e-4..1e16).contains(&magnitude) {
        let scientific = format!("{value:e}");
        let (mantissa, exponent) = scientific.split_once('e').unwrap_or((&scientific, "0"));
        let exponent: i32 = exponent.parse().unwrap_or(0);
        let sign = if exponent < 0 { '-' } else { '+' };
        return format!("{mantissa}e{sign}{:02}", exponent.abs());
    }
    let text = format!("{value}");
    if text.contains('.') {
        text
    } else {
        format!("{text}.0")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_whole_number_gains_the_decimal_point_that_stops_nastran_reading_an_integer() {
        assert_eq!(free_field(500.0), "500.");
        assert_eq!(free_field(1.0), "1.");
        assert_eq!(free_field(-1.0), "-1.");
    }

    #[test]
    fn zero_is_the_same_string_from_either_side() {
        assert_eq!(free_field(0.0), "0.");
        assert_eq!(free_field(-0.0), "0.");
    }

    #[test]
    fn an_ordinary_value_keeps_eight_significant_digits_and_no_trailing_zeros() {
        assert_eq!(free_field(9.81), "9.81");
        assert_eq!(free_field(0.02), "0.02");
        assert_eq!(free_field(0.01 * 9.81 * 9.81), "0.962361");
        assert_eq!(free_field(1_234_567.89), "1234567.9");
        assert_eq!(free_field(98_765_432.1), "98765432.");
        assert_eq!(free_field(0.000_123_456_789), "0.00012345679");
    }

    #[test]
    fn a_value_that_would_need_an_exponent_is_re_rendered_with_six_decimals() {
        assert_eq!(free_field(3.6e8), "360000000.0");
        assert_eq!(free_field(1e8), "100000000.0");
        assert_eq!(free_field(-2.5e9), "-2500000000.0");
        assert_eq!(free_field(1e-5), "0.00001");
        assert_eq!(free_field(4.8e-5), "0.000048");
    }

    #[test]
    fn a_magnitude_past_six_decimals_collapses_to_zero_as_the_reference_does() {
        assert_eq!(free_field(1.234_567_8e-7), "0.0");
    }

    #[test]
    fn python_str_writes_the_trailing_zero_rust_omits() {
        assert_eq!(python_str(2.0), "2.0");
        assert_eq!(python_str(120.0), "120.0");
        assert_eq!(python_str(0.5), "0.5");
        assert_eq!(python_str(0.01), "0.01");
        assert_eq!(python_str(0.0), "0.0");
    }

    #[test]
    fn python_str_switches_to_an_exponent_where_python_does() {
        assert_eq!(python_str(1e-5), "1e-05");
        assert_eq!(python_str(1e16), "1e+16");
        assert_eq!(python_str(0.0001), "0.0001");
    }
}
