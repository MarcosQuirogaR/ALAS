// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 Marcos Quiroga Rodriguez

//! The passenger-seat bands of CS/14 CFR 25.807(g)(1)-(9).
//!
//! The per-type table of 25.807(g) (what one exit of each type is rated for,
//! [`super::EXIT_TYPES`]) is only the lead-in of the paragraph. Its numbered
//! requirements then fix what exits a cabin of a given size needs in each
//! side of the fuselage, so an arrangement is rated by the smaller of the
//! table sum and the band its exits qualify for:
//!
//! * (g)(1): 1 to 9 seats, a Type IV or larger exit in each side;
//! * (g)(2): above 9 seats every exit counted is Type III or larger;
//! * (g)(3): 10 to 19 seats, one Type III or larger exit in each side;
//! * (g)(4): 20 to 40 seats, two exits in each side, one Type II or larger;
//! * (g)(5): 41 to 110 seats, two exits in each side, one Type I or larger;
//! * (g)(6): above 110 seats, two Type I or larger exits in each side;
//! * (g)(7): all Type III exits together allow 70 seats, two Type III exits in
//!   one side separated by fewer than three seat rows 65;
//! * (g)(8): a Type A, B or C exit needs two Type C or larger exits in each
//!   side;
//! * (g)(9): a ventral or tailcone exit with the egress rate of a Type III
//!   exit allows 12, 25 or 15 more seats.
//!
//! An arrangement is a list of pairs, one exit in each side per pair, so the
//! exits of a side are the pairs of all the decks together: the bands apply
//! to the airframe, not to one deck.

use super::exit_rules::{pair_rating, EXIT_TYPES};

/// Largest cabin a Type IV exit alone allows, 25.807(g)(1) and the Type IV
/// rating.
pub const SEATS_TYPE_IV_ONLY: i64 = 9;
/// Largest cabin one Type III or larger exit in each side allows, (g)(3).
pub const SEATS_ONE_EXIT: i64 = 19;
/// Largest cabin two exits in each side, one Type II or larger, allow, (g)(4).
pub const SEATS_TWO_EXITS_TYPE_II: i64 = 40;
/// Largest cabin two exits in each side, one Type I or larger, allow, (g)(5).
pub const SEATS_TWO_EXITS_TYPE_I: i64 = 110;
/// Combined seats all Type III exits allow, (g)(7).
pub const TYPE_III_COMBINED_SEATS: i64 = 70;
/// Combined seats two Type III exits in one side separated by fewer than
/// [`TYPE_III_SEPARATION_ROWS`] seat rows allow, (g)(7).
pub const TYPE_III_ADJACENT_SEATS: i64 = 65;
/// Seat rows two Type III exits of one side must be apart for the combined
/// 70 seats instead of 65, (g)(7).
pub const TYPE_III_SEPARATION_ROWS: f64 = 3.0;

/// The ventral or tailcone exit that adds seats under 25.807(g)(9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtraExit {
    /// A ventral exit with the egress rate of a Type III exit: 12 seats.
    Ventral,
    /// A tailcone exit with a floor-level opening of at least 20 by 60 in
    /// and an approved assist means (25.810(a)): 25 seats.
    TailconeFloorLevel,
    /// A tailcone exit equivalent to a Type III exit, the top of the opening
    /// at least 56 in above the cabin floor: 15 seats.
    TailconeTypeIii,
}

impl ExtraExit {
    /// The seats 25.807(g)(9) adds for this exit.
    pub const fn seats(self) -> i64 {
        match self {
            Self::Ventral => 12,
            Self::TailconeFloorLevel => 25,
            Self::TailconeTypeIii => 15,
        }
    }
}

/// Position of a type letter in [`EXIT_TYPES`]: 0 for Type A to 6 for Type IV,
/// so a smaller index is a larger exit.
fn rank(letter: &str) -> Option<usize> {
    EXIT_TYPES.iter().position(|spec| spec.name == letter)
}

const RANK_C: usize = 2;
const RANK_I: usize = 3;
const RANK_II: usize = 4;
const RANK_III: usize = 5;

/// Whether the Type III exits at `stations_m` (metres, one per Type III pair)
/// include two separated by fewer than three seat rows of `row_pitch_m`.
/// Stations that are not all declared cannot tell, and are not adjacent.
pub fn type_iii_adjacent(stations_m: &[Option<f64>], row_pitch_m: f64) -> bool {
    if !(row_pitch_m.is_finite() && row_pitch_m > 0.0) {
        return false;
    }
    let mut known: Vec<f64> = stations_m
        .iter()
        .filter_map(|station| station.filter(|x| x.is_finite()))
        .collect();
    known.sort_by(f64::total_cmp);
    known
        .windows(2)
        .any(|pair| pair[1] - pair[0] < TYPE_III_SEPARATION_ROWS * row_pitch_m)
}

/// The seats the exits of one side qualify for, from the numbered
/// requirements (g)(1)-(g)(6) and (g)(8) alone: `letters` names one exit per
/// pair. Unbounded (`i64::MAX`) when two Type I or larger exits remove the
/// bands, zero without any exit.
///
/// A Type A, B or C exit without a second Type C or larger one breaks (g)(8):
/// that arrangement is not approvable as it stands, and it is rated with its
/// large exits counted as Type I, the next class down, instead of as nothing.
pub fn band_limit(letters: &[&str]) -> i64 {
    let mut ranks: Vec<usize> = letters.iter().filter_map(|letter| rank(letter)).collect();
    if ranks.is_empty() {
        return 0;
    }
    ranks.retain(|&r| r <= RANK_III);
    if ranks.is_empty() {
        return SEATS_TYPE_IV_ONLY;
    }
    let large = ranks.iter().filter(|&&r| r <= RANK_C).count();
    if large == 1 {
        for r in ranks.iter_mut().filter(|r| **r <= RANK_C) {
            *r = RANK_I;
        }
    }
    let at_least = |limit: usize| ranks.iter().filter(|&&r| r <= limit).count();
    if ranks.len() >= 2 && at_least(RANK_I) >= 2 {
        i64::MAX
    } else if ranks.len() >= 2 && at_least(RANK_I) >= 1 {
        SEATS_TWO_EXITS_TYPE_I
    } else if ranks.len() >= 2 && at_least(RANK_II) >= 1 {
        SEATS_TWO_EXITS_TYPE_II
    } else {
        SEATS_ONE_EXIT
    }
}

/// The seats the table of 25.807(g) rates `letters` for, with the Type III
/// exits together held to 70 (65 when two are adjacent) by (g)(7) and
/// (g)(2) leaving the Type IV exits unrated above nine seats.
pub fn table_limit(letters: &[&str], adjacent_type_iii: bool) -> i64 {
    let mut type_iii = 0;
    let mut other = 0;
    let mut type_iv = 0;
    for letter in letters {
        match rank(letter) {
            Some(RANK_III) => type_iii += pair_rating(letter),
            Some(r) if r < RANK_III => other += pair_rating(letter),
            Some(_) => type_iv += pair_rating(letter),
            None => {}
        }
    }
    let type_iii_cap = if adjacent_type_iii {
        TYPE_III_ADJACENT_SEATS
    } else {
        TYPE_III_COMBINED_SEATS
    };
    let counted = other + type_iii.min(type_iii_cap);
    if counted == 0 {
        type_iv.min(SEATS_TYPE_IV_ONLY)
    } else {
        counted
    }
}

/// The most passengers 25.807(g) lets exits of `letters` seat: the table sum
/// or the band the exits qualify for, whichever is smaller, plus the seats a
/// ventral or tailcone exit adds under (g)(9).
pub fn seat_limit(letters: &[&str], adjacent_type_iii: bool, extra: &[ExtraExit]) -> i64 {
    let extra_seats: i64 = extra.iter().map(|exit| exit.seats()).sum();
    table_limit(letters, adjacent_type_iii)
        .min(band_limit(letters))
        .saturating_add(extra_seats)
}

#[allow(clippy::unwrap_used, clippy::expect_used)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cabin_of_at_most_nine_seats_needs_a_type_iv_exit_or_larger() {
        assert_eq!(seat_limit(&["IV"], false, &[]), 9);
        assert_eq!(seat_limit(&["IV", "IV"], false, &[]), 9);
        assert_eq!(seat_limit(&[], false, &[]), 0);
        // Above nine seats a Type IV exit counts for nothing.
        assert_eq!(seat_limit(&["III", "IV"], false, &[]), 19);
    }

    #[test]
    fn one_exit_in_each_side_seats_nineteen_whatever_its_size() {
        // The table alone would give 35 for a Type III pair, 45 for Type I,
        // 110 for Type A: (g)(3) stops them at 19.
        assert_eq!(seat_limit(&["III"], false, &[]), 19);
        assert_eq!(seat_limit(&["I"], false, &[]), 19);
        assert_eq!(seat_limit(&["A"], false, &[]), 19);
    }

    #[test]
    fn two_exits_with_a_type_ii_seat_forty() {
        assert_eq!(seat_limit(&["III", "III"], false, &[]), 19);
        assert_eq!(seat_limit(&["II", "III"], false, &[]), 40);
        // The table alone is 35 + 40 = 75.
        assert_eq!(table_limit(&["II", "III"], false), 75);
    }

    #[test]
    fn two_exits_with_a_type_i_seat_110_and_two_type_i_remove_the_bands() {
        assert_eq!(seat_limit(&["I", "III"], false, &[]), 80);
        assert_eq!(band_limit(&["I", "III"]), 110);
        assert_eq!(seat_limit(&["I", "I"], false, &[]), 90);
        assert_eq!(band_limit(&["I", "I"]), i64::MAX);
        assert_eq!(seat_limit(&["C", "III", "III", "C"], false, &[]), 180);
        assert_eq!(seat_limit(&["I", "III", "III", "I"], false, &[]), 160);
    }

    #[test]
    fn the_type_iii_exits_together_seat_70_and_65_when_adjacent() {
        assert_eq!(table_limit(&["III", "III", "III"], false), 70);
        assert_eq!(table_limit(&["I", "III", "III", "I"], false), 160);
        assert_eq!(table_limit(&["I", "III", "III", "I"], true), 155);
        // Two Type III exits less than three rows of 0.8 m (2.4 m) apart.
        assert!(type_iii_adjacent(&[Some(10.0), Some(12.0)], 0.8));
        assert!(!type_iii_adjacent(&[Some(10.0), Some(13.0)], 0.8));
        assert!(!type_iii_adjacent(&[None, Some(10.0)], 0.8));
        assert!(!type_iii_adjacent(&[Some(10.0), Some(10.5)], 0.0));
    }

    #[test]
    fn a_large_exit_needs_a_second_type_c_or_larger_exit() {
        // One Type A alone breaks (g)(8) and is rated as a Type I pair.
        assert_eq!(table_limit(&["A"], false), 110);
        assert_eq!(band_limit(&["A"]), SEATS_ONE_EXIT);
        assert_eq!(band_limit(&["A", "III"]), SEATS_TWO_EXITS_TYPE_I);
        assert_eq!(band_limit(&["A", "A"]), i64::MAX);
        assert_eq!(band_limit(&["C", "C"]), i64::MAX);
        assert_eq!(band_limit(&["B", "C"]), i64::MAX);
        assert_eq!(seat_limit(&["A", "A", "A", "A"], false, &[]), 440);
    }

    #[test]
    fn a_ventral_or_tailcone_exit_adds_its_seats() {
        assert_eq!(seat_limit(&["III"], false, &[ExtraExit::Ventral]), 31);
        assert_eq!(
            seat_limit(&["I", "I"], false, &[ExtraExit::TailconeFloorLevel]),
            115
        );
        assert_eq!(ExtraExit::TailconeTypeIii.seats(), 15);
    }

    #[test]
    fn a_letter_the_regulation_does_not_define_seats_nobody() {
        assert_eq!(seat_limit(&["Z"], false, &[]), 0);
        assert_eq!(seat_limit(&["Z", "III"], false, &[]), 19);
    }
}
