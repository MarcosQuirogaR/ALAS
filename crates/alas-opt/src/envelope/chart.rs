// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Reading of the digitised handbook charts of the envelope: one
//! piecewise-linear lookup that holds the end value outside the chart, and
//! the query that says when a reading was held rather than read.

/// Which end of a chart axis a query lies beyond.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChartEnd {
    /// Below the first tabulated abscissa.
    Below,
    /// Above the last tabulated abscissa.
    Above,
    /// The query is `NaN`.
    NotANumber,
}

/// Whether `x` lies outside the tabulated abscissae `xs` (ascending), and on
/// which side. [`interpolate`] returns the end value there: a held value, not
/// a chart reading.
#[must_use]
pub(super) fn outside(xs: &[f64], x: f64) -> Option<ChartEnd> {
    if x.is_nan() {
        Some(ChartEnd::NotANumber)
    } else if x < xs[0] {
        Some(ChartEnd::Below)
    } else if x > xs[xs.len() - 1] {
        Some(ChartEnd::Above)
    } else {
        None
    }
}

/// Piecewise-linear interpolation, clamped to the ends of `xs`.
pub(super) fn interpolate(xs: &[f64], ys: &[f64], x: f64) -> f64 {
    let x = x.clamp(xs[0], xs[xs.len() - 1]);
    for i in 1..xs.len() {
        if x <= xs[i] {
            return ys[i - 1] + (ys[i] - ys[i - 1]) * (x - xs[i - 1]) / (xs[i] - xs[i - 1]);
        }
    }
    ys[ys.len() - 1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_beyond_the_chart_is_held_at_the_end_value_and_reported() {
        let (xs, ys) = ([0.15, 0.20, 0.36], [1.69, 1.67, 1.25]);
        assert_eq!(outside(&xs, 0.20), None);
        assert_eq!(outside(&xs, 0.15), None);
        assert_eq!(outside(&xs, 0.36), None);
        assert_eq!(outside(&xs, 0.10), Some(ChartEnd::Below));
        assert_eq!(outside(&xs, 0.50), Some(ChartEnd::Above));
        assert_eq!(outside(&xs, f64::NAN), Some(ChartEnd::NotANumber));
        assert_eq!(interpolate(&xs, &ys, 0.10), 1.69);
        assert_eq!(interpolate(&xs, &ys, 0.50), 1.25);
        assert!((interpolate(&xs, &ys, 0.175) - 1.68).abs() < 1.0e-12);
    }
}
