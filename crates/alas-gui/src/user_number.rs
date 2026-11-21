// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Typed numbers: one parser for every text field that takes a number.

/// Parse a number typed by the user. The decimal separator is `.` or `,`
/// (`0,85` reads as 0.85). Text holding both separators, or more than one
/// comma, is ambiguous (thousands grouping or decimal) and is rejected.
pub fn parse_user_number(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    let commas = trimmed.matches(',').count();
    if commas == 0 {
        return trimmed.parse::<f64>().ok();
    }
    if commas > 1 || trimmed.contains('.') {
        return None;
    }
    trimmed.replace(',', ".").parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::parse_user_number;

    #[test]
    fn the_decimal_comma_reads_like_the_decimal_point() {
        assert_eq!(parse_user_number("0,85"), Some(0.85));
        assert_eq!(parse_user_number(" 12.5 "), Some(12.5));
        assert_eq!(parse_user_number("-3,5"), Some(-3.5));
    }

    #[test]
    fn mixed_or_repeated_separators_are_rejected() {
        assert_eq!(parse_user_number("1,234.5"), None);
        assert_eq!(parse_user_number("1.234,5"), None);
        assert_eq!(parse_user_number("1,2,3"), None);
        assert_eq!(parse_user_number("abc"), None);
    }
}
