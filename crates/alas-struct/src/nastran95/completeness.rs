// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! Is a NASTRAN-95 inverse-power eigenvalue set the *lowest* set?
//!
//! `EIGR,,INV` returns whichever roots its shifts converge on inside the
//! requested band. Its Sturm-sequence bookkeeping is printed as lines
//! `<n> ROOTS BELOW <lambda>`: the number of eigenvalues below the shift
//! `lambda` (rad/s)^2 from the signs of the `LDL^T` pivots of `K - lambda M`.
//! That count is independent of which roots the iteration happened to find, so
//! it can audit the reported set: at every probe `lambda` the Sturm count must
//! equal the number of reported eigenvalues below it.
//!
//! The 2026-10-07 A320 wingbox run printed `19 ROOTS BELOW 9.437151E+03` while
//! its lowest reported eigenvalue was `9.437199E+03` (15.46 Hz): nineteen
//! lower roots, including the 1.84 Hz first wing bending, were never extracted.
//! The earlier adapter reported that set as the structure's modes.
//!
//! A probe within [`RELATIVE_TOLERANCE`] of a root is ambiguous (the pivot
//! sign at an eigenvalue is arbitrary), so counts there may differ by the roots
//! inside that window.

use super::Mode;
use crate::nastran::text::splitlines;

/// Relative window around a root inside which the Sturm count is ambiguous.
pub(crate) const RELATIVE_TOLERANCE: f64 = 2.0e-3;

/// Every `<n> ROOTS BELOW <lambda>` line of a print, as `(n, lambda)`.
pub(crate) fn read_sturm_counts(print: &str) -> Vec<(usize, f64)> {
    let mut counts = Vec::new();
    for line in splitlines(print) {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if let [count, "ROOTS", "BELOW", lambda] = tokens.as_slice() {
            if let (Ok(count), Ok(lambda)) = (count.parse::<usize>(), lambda.parse::<f64>()) {
                if lambda.is_finite() && lambda >= 0.0 {
                    counts.push((count, lambda));
                }
            }
        }
    }
    counts
}

/// The number of leading (lowest-first) reported roots that the Sturm counts
/// confirm to be gap-free.
///
/// Root `k` (1-based, ascending) is confirmed when at least one probe lies at
/// or below it (otherwise nothing audits the interval below it) and every
/// probe `p <= lambda_k (1 + tol)` satisfies
/// `#(lambda_j < p (1 - tol)) <= count(p) <= #(lambda_j < p (1 + tol))`.
/// The confirmed prefix stops at the first root that violates this, so a run
/// that skipped the lowest roots confirms zero.
pub(crate) fn confirmed_prefix(modes: &[Mode], counts: &[(usize, f64)]) -> usize {
    let mut lambdas: Vec<f64> = modes
        .iter()
        .map(|mode| mode.eigenvalue)
        .filter(|value| value.is_finite() && *value > 0.0)
        .collect();
    lambdas.sort_by(f64::total_cmp);
    let below = |limit: f64| lambdas.iter().filter(|&&l| l < limit).count();
    let mut confirmed = 0;
    for (index, &lambda) in lambdas.iter().enumerate() {
        let horizon = lambda * (1.0 + RELATIVE_TOLERANCE);
        let probes: Vec<&(usize, f64)> = counts
            .iter()
            .filter(|(_, probe)| *probe <= horizon)
            .collect();
        let consistent = !probes.is_empty()
            && probes.iter().all(|&&(count, probe)| {
                below(probe * (1.0 - RELATIVE_TOLERANCE)) <= count
                    && count <= below(probe * (1.0 + RELATIVE_TOLERANCE))
            });
        if !consistent {
            break;
        }
        confirmed = index + 1;
    }
    confirmed
}

/// The largest number of roots the Sturm counts say lie below the lowest
/// reported eigenvalue: how many roots the run skipped at its low end.
pub(crate) fn roots_missed_below_lowest(modes: &[Mode], counts: &[(usize, f64)]) -> usize {
    let lowest = modes
        .iter()
        .map(|mode| mode.eigenvalue)
        .filter(|value| value.is_finite() && *value > 0.0)
        .fold(f64::INFINITY, f64::min);
    counts
        .iter()
        .filter(|(_, probe)| *probe <= lowest * (1.0 + RELATIVE_TOLERANCE))
        .map(|&(count, _)| count)
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modes(hz: &[f64]) -> Vec<Mode> {
        hz.iter()
            .map(|&f| Mode {
                eigenvalue: (2.0 * std::f64::consts::PI * f).powi(2),
                cyclic_hz: f,
            })
            .collect()
    }

    fn lambda(hz: f64) -> f64 {
        (2.0 * std::f64::consts::PI * hz).powi(2)
    }

    #[test]
    fn sturm_lines_are_parsed_and_page_furniture_ignored() {
        let print =
            "   19 ROOTS BELOW   9.437151E+03\n  1  ROOTS  ABOVE 3.0\n   0 ROOTS BELOW   1.0E+02\n";
        assert_eq!(read_sturm_counts(print), vec![(19, 9.437151e3), (0, 1.0e2)]);
    }

    #[test]
    fn the_a320_first_run_is_a_gap_not_a_modal_set() {
        // 15.46 Hz is the lowest reported root and nineteen roots lie below it.
        let found = modes(&[15.46115, 15.74382, 16.31409]);
        let counts = [(19, 9.437151e3), (20, 9.785e3), (21, 1.0507e4)];
        assert_eq!(confirmed_prefix(&found, &counts), 0);
        assert_eq!(roots_missed_below_lowest(&found, &counts), 19);
    }

    #[test]
    fn a_gap_free_low_band_is_confirmed() {
        let found = modes(&[1.842, 2.221, 4.821, 4.958]);
        let counts = [
            (0, lambda(1.84) * 0.99),
            (1, lambda(2.2) * 0.99),
            (2, lambda(3.5)),
            (2, lambda(4.8)),
            (4, lambda(5.0) * 1.01),
        ];
        assert_eq!(confirmed_prefix(&found, &counts), 4);
        assert_eq!(roots_missed_below_lowest(&found, &counts), 0);
    }

    #[test]
    fn a_hole_between_reported_roots_truncates_the_confirmed_prefix() {
        // A root near 3 Hz exists (count 3 below 4 Hz) but was not reported.
        let found = modes(&[1.842, 2.221, 4.821]);
        let counts = [
            (0, lambda(1.84) * 0.99),
            (1, lambda(2.2) * 0.99),
            (3, lambda(4.0)),
            (3, lambda(4.8) * 0.999),
        ];
        assert_eq!(confirmed_prefix(&found, &counts), 2);
    }

    #[test]
    fn no_probe_below_the_lowest_root_confirms_nothing() {
        let found = modes(&[5.0, 6.0]);
        let counts = [(2, lambda(7.0))];
        assert_eq!(confirmed_prefix(&found, &counts), 0);
        assert_eq!(confirmed_prefix(&found, &[]), 0);
    }
}
