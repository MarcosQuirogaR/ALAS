// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

// Ported from alas/reporting/visualization.py (_assign_label_rows)

//! Greedy row assignment for x-sorted annotation labels, shared by the
//! side-view marker stack ([`super::side_view`]) and the stability-metrics
//! ruler ([`super::metrics`]).

pub(super) use crate::families::common::assign_label_rows;

#[cfg(test)]
mod tests {
    // These tests intentionally panic if their constructed fixture violates its precondition.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn widely_spaced_labels_all_share_row_zero() {
        let rows = assign_label_rows(&[0.0, 10.0, 20.0, 30.0], 1.5);
        assert_eq!(rows, vec![0, 0, 0, 0]);
    }

    #[test]
    fn a_tight_cluster_grows_as_many_rows_as_it_needs() {
        // Four labels within min_sep of every other one: none can share a
        // row with any other, so each opens a new one.
        let rows = assign_label_rows(&[0.0, 0.5, 1.0, 1.5], 2.0);
        assert_eq!(rows, vec![0, 1, 2, 3]);
    }

    #[test]
    fn a_later_label_reuses_a_row_once_it_is_far_enough_from_that_rows_last_entry() {
        // 0.0 -> row 0. 1.0 is too close to row 0 (min_sep 2.0) -> row 1.
        // 3.0 is 3.0 away from row 0's last entry (0.0) -> reuses row 0.
        let rows = assign_label_rows(&[0.0, 1.0, 3.0], 2.0);
        assert_eq!(rows, vec![0, 1, 0]);
    }

    #[test]
    fn no_two_labels_on_the_same_row_are_closer_than_min_sep() {
        let xs = [0.0, 0.3, 0.6, 5.0, 5.2, 5.4, 12.0];
        let min_sep = 1.0;
        let rows = assign_label_rows(&xs, min_sep);
        for row in 0..=*rows.iter().max().unwrap() {
            let mut last: Option<f64> = None;
            for (&x, &r) in xs.iter().zip(&rows) {
                if r != row {
                    continue;
                }
                if let Some(prev) = last {
                    assert!(x - prev >= min_sep, "row {row}: {prev} then {x}");
                }
                last = Some(x);
            }
        }
    }

    #[test]
    fn an_empty_input_produces_no_rows() {
        assert!(assign_label_rows(&[], 1.0).is_empty());
    }
}
