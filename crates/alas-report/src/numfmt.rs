// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The one number formatter of figure labels.
//!
//! Rule: a value is written with a fixed number of decimals per quantity
//! (masses and distances in whole units, speeds and altitudes in whole units,
//! percentages and payload tonnes to one decimal) and, from four integer
//! digits up, groups thousands with a space, exactly as the axis tick labels
//! of [`crate::chart_kit`] do, so a label and the axis beside it read alike.

/// `value` with `decimals` decimals, thousands grouped with a space.
pub fn grouped(value: f64, decimals: usize) -> String {
    if !value.is_finite() {
        return format!("{value}");
    }
    let text = format!("{value:.decimals$}");
    // "-0" / "-0.0" is a rounding artefact, not a sign.
    let text = if text
        .trim_start_matches('-')
        .chars()
        .all(|c| c == '0' || c == '.')
    {
        text.trim_start_matches('-').to_owned()
    } else {
        text
    };
    alas_units::notation::group_thousands(&text)
}

#[cfg(test)]
mod tests {
    use super::grouped;

    #[test]
    fn groups_from_four_digits_with_a_space_and_keeps_decimals() {
        assert_eq!(grouped(17313.4, 0), "17 313");
        assert_eq!(grouped(999.0, 0), "999");
        assert_eq!(grouped(1_234_567.4, 0), "1 234 567");
        assert_eq!(grouped(53.74, 1), "53.7");
        assert_eq!(grouped(-2500.4, 0), "-2 500");
        assert_eq!(grouped(-0.2, 0), "0");
    }
}
